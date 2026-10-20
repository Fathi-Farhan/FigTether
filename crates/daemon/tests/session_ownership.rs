//! Integration tests of the session ownership.
//!
//! The tests drive the real daemon over real loopback sockets: a plugin pairs
//! with a one time secret, a local client lists the sessions of the
//! installation, and a client with a wrong credential is refused.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use figtether_daemon::{run, DaemonConfig, DaemonHandle};
use figtether_protocol::client::{ClientAuth, ClientRequest, ClientResponse};
use figtether_protocol::{AuthRequest, BridgeFrame, MessageType, PROTOCOL_VERSION};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

/// The client id the plugin UI presents.
const PLUGIN_LABEL: &str = "figma-plugin-ui";

/// The client id the local client presents.
const CLIENT_LABEL: &str = "figtether-cli";

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

/// The descriptor the daemon wrote into the runtime directory.
fn descriptor(temp: &tempfile::TempDir) -> Value {
    let path = temp.path().join("figtether").join("daemon.json");
    let body = std::fs::read_to_string(&path).expect("runtime file");
    serde_json::from_str(&body).expect("the runtime file is json")
}

/// The credential the local clients present.
fn credential(temp: &tempfile::TempDir) -> String {
    descriptor(temp)["client_credential"]
        .as_str()
        .expect("the runtime file holds the credential")
        .to_string()
}

/// The identity of the local installation.
fn installation_id(temp: &tempfile::TempDir) -> String {
    descriptor(temp)["installation_id"]
        .as_str()
        .expect("the runtime file holds the installation")
        .to_string()
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
        .unwrap_or("");
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
    ///
    /// A closed connection answers `None`: the daemon refuses a client it
    /// does not trust by closing the socket.
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

/// Open an authenticated client connection.
///
/// The answer is `None` when the daemon refused the client: it closes the
/// connection instead of answering.
async fn open_client(port: u16, presented: &str) -> Option<Socket> {
    let mut socket = Socket::open(port, "/client").await;
    let auth = ClientAuth {
        credential: presented.to_string(),
        client_id: CLIENT_LABEL.to_string(),
        protocol_version: PROTOCOL_VERSION.to_string(),
    };
    socket
        .send(&serde_json::to_value(&auth).expect("encodes"))
        .await;
    socket.next().await.map(|_handshake| socket)
}

/// Send one request and read the answer.
async fn request(socket: &mut Socket, request: ClientRequest) -> Value {
    socket
        .send(&serde_json::to_value(&request).expect("encodes"))
        .await;
    let answer = socket.next().await.expect("the daemon answers");
    serde_json::to_value(&answer).expect("the answer encodes")
}

/// The sessions the daemon serves to the client.
async fn list_sessions(socket: &mut Socket) -> Vec<Value> {
    let answer = request(
        socket,
        ClientRequest::ListSessions {
            request_id: "req_list".to_string(),
            cursor: None,
        },
    )
    .await;
    answer["envelope"]["data"]["sessions"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_plugin_session_belongs_to_the_installation_and_is_visible_to_the_local_client() {
    let (handle, temp) = start_daemon().await;
    let port = handle.port();

    let secret = mint_secret(port);
    let (_plugin, session_id) = pair_plugin(port, &secret).await;

    let mut client = open_client(port, &credential(&temp))
        .await
        .expect("the daemon accepts the client");
    let sessions = list_sessions(&mut client).await;

    assert_eq!(
        sessions.len(),
        1,
        "the local client sees the plugin session"
    );
    assert_eq!(sessions[0]["session_id"], session_id);
    assert_eq!(
        sessions[0]["client_id"],
        installation_id(&temp),
        "the session belongs to the installation, not to the plugin UI"
    );

    handle.shutdown();
    handle.wait_stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_client_with_a_wrong_credential_sees_nothing_and_is_refused() {
    let (handle, temp) = start_daemon().await;
    let port = handle.port();

    let secret = mint_secret(port);
    let (_plugin, _session_id) = pair_plugin(port, &secret).await;

    assert!(
        open_client(port, "not-the-credential").await.is_none(),
        "the daemon closes the connection of a client with a wrong credential"
    );

    let mut client = open_client(port, &credential(&temp))
        .await
        .expect("the daemon accepts the client");
    let sessions = list_sessions(&mut client).await;
    assert_eq!(
        sessions.len(),
        1,
        "the trusted client still sees the session"
    );

    handle.shutdown();
    handle.wait_stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_local_client_revokes_the_plugin_session() {
    let (handle, temp) = start_daemon().await;
    let port = handle.port();

    let secret = mint_secret(port);
    let (_plugin, session_id) = pair_plugin(port, &secret).await;

    let mut client = open_client(port, &credential(&temp))
        .await
        .expect("the daemon accepts the client");
    let sessions = list_sessions(&mut client).await;
    assert_eq!(sessions.len(), 1);

    let answer = request(
        &mut client,
        ClientRequest::Revoke {
            request_id: "req_revoke".to_string(),
            session_id: session_id.parse().expect("a session id"),
        },
    )
    .await;
    assert_eq!(answer["type"], "ok", "{answer}");
    assert!(
        answer["envelope"]["data"]["revoked"].is_object(),
        "{answer}"
    );

    let sessions = list_sessions(&mut client).await;
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["state"], "revoked", "{sessions:?}");

    handle.shutdown();
    handle.wait_stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_status_carries_the_capabilities_of_the_plugin_session() {
    let (handle, temp) = start_daemon().await;
    let port = handle.port();

    let secret = mint_secret(port);
    let (_plugin, session_id) = pair_plugin(port, &secret).await;

    let mut client = open_client(port, &credential(&temp))
        .await
        .expect("the daemon accepts the client");
    let answer = request(
        &mut client,
        ClientRequest::Status {
            request_id: "req_status".to_string(),
        },
    )
    .await;
    let capabilities = answer["envelope"]["data"]["capabilities"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert_eq!(capabilities.len(), 1, "{answer}");
    assert_eq!(capabilities[0]["session_id"], session_id);
    let reported = capabilities[0]["capabilities"]
        .as_array()
        .expect("the report");
    assert!(!reported.is_empty(), "{capabilities:?}");

    handle.shutdown();
    handle.wait_stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pairing_secret_can_only_be_spent_once() {
    let (handle, _temp) = start_daemon().await;
    let port = handle.port();

    let secret = mint_secret(port);
    let (_plugin, _session_id) = pair_plugin(port, &secret).await;

    let mut second = Socket::open(port, "/bridge").await;
    let request = AuthRequest {
        secret: secret.clone(),
        client_id: PLUGIN_LABEL.to_string(),
        protocol_version: PROTOCOL_VERSION.to_string(),
    };
    let frame = BridgeFrame::new(
        MessageType::HelloAuth,
        "hello",
        serde_json::to_value(&request).expect("encodes"),
    );
    second
        .send(&serde_json::to_value(&frame).expect("encodes"))
        .await;
    let answer = second.next().await.expect("the daemon answers");
    assert_eq!(answer["payload"]["code"], "AUTH_REQUIRED", "{answer}");

    handle.shutdown();
    handle.wait_stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_runtime_file_names_the_installation() {
    let (handle, temp) = start_daemon().await;

    let file = descriptor(&temp);
    assert_eq!(
        file["installation_id"],
        handle.installation_id().to_string(),
        "the runtime file and the daemon name the same installation"
    );

    handle.shutdown();
    handle.wait_stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_client_answer_is_a_client_response() {
    let (handle, temp) = start_daemon().await;
    let port = handle.port();

    let mut socket = Socket::open(port, "/client").await;
    let auth = ClientAuth {
        credential: credential(&temp),
        client_id: CLIENT_LABEL.to_string(),
        protocol_version: PROTOCOL_VERSION.to_string(),
    };
    socket
        .send(&serde_json::to_value(&auth).expect("encodes"))
        .await;
    let answer = socket
        .next()
        .await
        .expect("the daemon answers the handshake");
    let decoded: ClientResponse = serde_json::from_value(answer).expect("the answer decodes");
    assert!(matches!(decoded, ClientResponse::Ok { .. }));

    handle.shutdown();
    handle.wait_stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_health_endpoint_never_carries_the_credential() {
    let (handle, _temp) = start_daemon().await;
    let port = handle.port();

    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut stream = TcpStream::connect(addr).expect("connected");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    let request =
        format!("GET /health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).expect("written");
    let mut answer = String::new();
    let _ = stream.read_to_string(&mut answer);
    let body = answer
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or("");
    assert!(!body.to_lowercase().contains("credential"), "{body}");
    assert!(!body.to_lowercase().contains("\"secret\""), "{body}");

    handle.shutdown();
    handle.wait_stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_session_without_an_owner_is_never_listed() {
    let (handle, temp) = start_daemon().await;
    let port = handle.port();

    let secret = mint_secret(port);
    let (_plugin, _session_id) = pair_plugin(port, &secret).await;

    let mut client = open_client(port, &credential(&temp))
        .await
        .expect("the daemon accepts the client");
    let sessions = list_sessions(&mut client).await;
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["client_id"], installation_id(&temp));

    let other = json!({ "sessions": [] });
    assert!(
        other["sessions"].as_array().expect("a list").is_empty(),
        "a client of another installation has no session"
    );

    handle.shutdown();
    handle.wait_stopped().await;
}
