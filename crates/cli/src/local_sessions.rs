//! Tests of the local session surface of the CLI.
//!
//! The tests drive the real daemon over real loopback sockets: a plugin pairs
//! with a one time secret, the CLI lists the sessions of the installation,
//! and `figtether revoke --all` reaches them. A client that presents a wrong
//! credential is refused.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use figtether_daemon::{run, DaemonConfig, DaemonHandle};
use figtether_protocol::{AuthRequest, BridgeFrame, MessageType, PROTOCOL_VERSION};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use crate::commands::{revoke, status};
use crate::control::{ControlClient, ControlError};

/// The client id the plugin UI presents.
const PLUGIN_LABEL: &str = "figma-plugin-ui";

/// The websocket stream of the daemon.
type Stream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// A free loopback port.
fn free_port() -> u16 {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bound");
    let port = listener.local_addr().expect("address").port();
    drop(listener);
    port
}

/// Start a daemon on a free port with a temporary runtime directory.
async fn start_daemon() -> (DaemonHandle, tempfile::TempDir) {
    let temp = tempfile::tempdir().expect("temp dir");
    let mut last_error = None;
    for _ in 0..5 {
        let config = DaemonConfig::new()
            .with_port(free_port())
            .with_runtime_dir(temp.path().join("figtether"));
        match run(config).await {
            Ok(handle) => return (handle, temp),
            Err(err) => last_error = Some(err),
        }
    }
    panic!("the daemon did not start: {last_error:?}");
}

/// The runtime directory of the daemon.
fn runtime_dir(temp: &tempfile::TempDir) -> PathBuf {
    temp.path().join("figtether")
}

/// Mint a pairing secret over the control endpoint.
fn mint_secret(port: u16) -> String {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut stream = TcpStream::connect(addr).expect("connected");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let request = figtether_daemon::control::pair_request(port);
    stream.write_all(request.as_bytes()).expect("written");
    let mut answer = String::new();
    let _ = stream.read_to_string(&mut answer);
    let body = answer
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or_default();
    serde_json::from_str::<Value>(body).expect("the answer is json")["secret"]
        .as_str()
        .expect("the answer holds the secret")
        .to_string()
}

/// One websocket connection to the daemon.
struct Socket {
    /// The writing half of the connection.
    sink: futures_util::stream::SplitSink<Stream, WsMessage>,
    /// The reading half of the connection.
    source: futures_util::stream::SplitStream<Stream>,
}

impl Socket {
    /// Open a connection to one endpoint of the daemon.
    async fn open(port: u16, path: &str) -> Self {
        let url = format!("ws://127.0.0.1:{port}{path}");
        let (stream, _answer) = connect_async(url).await.expect("connected");
        let (sink, source) = stream.split();
        Self { sink, source }
    }

    /// Send one frame.
    async fn send(&mut self, value: &Value) {
        let text = serde_json::to_string(value).expect("encodes");
        self.sink
            .send(WsMessage::Text(text.into()))
            .await
            .expect("written");
    }

    /// Read the next text frame.
    async fn next(&mut self) -> Option<Value> {
        loop {
            match self.source.next().await? {
                Ok(WsMessage::Text(text)) => {
                    return Some(serde_json::from_str(text.as_str()).expect("the frame is json"))
                }
                Ok(WsMessage::Close(_)) => return None,
                Ok(_) => continue,
                Err(_) => return None,
            }
        }
    }
}

/// Pair a plugin and return the socket and the id of the session.
async fn pair_plugin(port: u16, secret: &str) -> (Socket, String) {
    let mut socket = Socket::open(port, "/bridge").await;
    let request = AuthRequest {
        secret: secret.to_string(),
        client_id: PLUGIN_LABEL.to_string(),
        protocol_version: PROTOCOL_VERSION.to_string(),
    };
    let frame = BridgeFrame::new(
        MessageType::HelloAuth,
        "hello",
        serde_json::to_value(&request).expect("encodes"),
    );
    socket
        .send(&serde_json::to_value(&frame).expect("encodes"))
        .await;
    let answer = socket.next().await.expect("the daemon answers");
    let session_id = answer["payload"]["session_id"]
        .as_str()
        .expect("the answer holds the session")
        .to_string();
    (socket, session_id)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_cli_sees_the_plugin_session_and_revokes_it() {
    let (handle, temp) = start_daemon().await;
    let port = handle.port();

    let secret = mint_secret(port);
    let (_plugin, session_id) = pair_plugin(port, &secret).await;

    let client = ControlClient::connect(Some(port), Some(&runtime_dir(&temp)))
        .await
        .expect("the CLI connects");

    let sessions = status::visible_sessions(&client)
        .await
        .expect("the sessions are listed");
    assert_eq!(sessions.len(), 1, "{sessions:?}");
    assert_eq!(sessions[0]["session_id"], session_id);

    let revoked = revoke::revoke_all(&client)
        .await
        .expect("the sessions are revoked");
    assert_eq!(revoked.len(), 1);
    assert_eq!(revoked[0].0.to_string(), session_id);

    let sessions = status::visible_sessions(&client)
        .await
        .expect("the sessions are listed");
    assert_eq!(sessions.len(), 1, "{sessions:?}");
    assert_eq!(sessions[0]["state"], "revoked", "{sessions:?}");

    handle.shutdown();
    handle.wait_stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_cli_is_refused_when_the_credential_is_wrong() {
    let (handle, temp) = start_daemon().await;
    let port = handle.port();

    let secret = mint_secret(port);
    let (_plugin, _session_id) = pair_plugin(port, &secret).await;

    // The runtime file carries a credential that the daemon does not know.
    let path = runtime_dir(&temp).join("daemon.json");
    let body = std::fs::read_to_string(&path).expect("runtime file");
    let mut file: Value = serde_json::from_str(&body).expect("the runtime file is json");
    file["client_credential"] = json!("not-the-credential");
    std::fs::write(&path, file.to_string()).expect("written");

    let error = match ControlClient::connect(Some(port), Some(&runtime_dir(&temp))).await {
        Ok(_client) => panic!("the CLI must be refused"),
        Err(error) => error,
    };
    assert!(
        matches!(error, ControlError::Channel(_)),
        "the CLI fails closed: {error}"
    );

    handle.shutdown();
    handle.wait_stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_status_reports_the_capabilities_of_the_plugin_session() {
    let (handle, temp) = start_daemon().await;
    let port = handle.port();

    let secret = mint_secret(port);
    let (_plugin, session_id) = pair_plugin(port, &secret).await;

    let client = ControlClient::connect(Some(port), Some(&runtime_dir(&temp)))
        .await
        .expect("the CLI connects");
    let status = client
        .request(figtether_protocol::client::ClientRequest::Status {
            request_id: crate::control::new_request_id(),
        })
        .await
        .expect("the daemon answers");
    let reports = status["capabilities"].as_array().expect("the report");
    assert_eq!(reports.len(), 1, "{status}");
    assert_eq!(reports[0]["session_id"], session_id);

    handle.shutdown();
    handle.wait_stopped().await;
}
