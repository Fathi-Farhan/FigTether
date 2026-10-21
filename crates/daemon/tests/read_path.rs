//! Integration tests of the read path of the daemon.
//!
//! The tests drive the real daemon over real loopback sockets: a plugin
//! pairs, the user grants a scope, and a local client reads. They prove the
//! rules of the read path:
//!
//! - the session reaches the states that serve reads: `paired` after the
//!   authentication and `ready` after the grant;
//! - the operation of the request reaches the plugin;
//! - a truncated traversal carries a cursor, and the cursor dies with the
//!   grant epoch;
//! - a read fills the cache and an export fills the asset store;
//! - a revoke empties both;
//! - a job that never ends leaves the lane of its session free.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use figtether_daemon::{run, DaemonConfig, DaemonHandle};
use figtether_protocol::client::{ClientAuth, ClientRequest};
use figtether_protocol::{
    AuthAccepted, AuthRequest, BridgeFrame, BridgeOp, MessageType, RequestPayload, ResultPayload,
    PROTOCOL_VERSION,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

/// The client id the plugin UI presents.
const PLUGIN_LABEL: &str = "figma-plugin-ui";

/// The client id the local client presents.
const CLIENT_LABEL: &str = "figtether-cli";

/// The bytes of a tiny PNG, base64 encoded.
const PNG_BYTES: &str = "iVBORw0KGgo=";

/// The websocket stream of the daemon.
type Stream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// The writing half of a connection to the daemon.
type Sink = futures_util::stream::SplitSink<Stream, WsMessage>;

/// A free loopback port.
fn free_port() -> u16 {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bound");
    let port = listener.local_addr().expect("address").port();
    drop(listener);
    port
}

/// The budgets of a daemon that answers fast.
fn fast_limits() -> figtether_core::ResourceLimits {
    figtether_core::ResourceLimits {
        read_timeout: Duration::from_millis(400),
        export_timeout: Duration::from_millis(400),
        ..figtether_core::ResourceLimits::default()
    }
}

/// Start a daemon on a free port with a temporary runtime directory.
async fn start_daemon() -> (DaemonHandle, tempfile::TempDir) {
    start_daemon_with(fast_limits()).await
}

/// Start a daemon with explicit budgets.
async fn start_daemon_with(
    limits: figtether_core::ResourceLimits,
) -> (DaemonHandle, tempfile::TempDir) {
    let temp = tempfile::tempdir().expect("temp dir");
    let mut last_error = None;
    for _ in 0..5 {
        let config = DaemonConfig::new()
            .with_port(free_port())
            .with_limits(limits.clone())
            .with_runtime_dir(temp.path().join("figtether"));
        match run(config).await {
            Ok(handle) => return (handle, temp),
            Err(err) => last_error = Some(err),
        }
    }
    panic!("the daemon did not start: {last_error:?}");
}

/// The credential the local clients present.
fn credential(temp: &tempfile::TempDir) -> String {
    let path = temp.path().join("figtether").join("daemon.json");
    let body = std::fs::read_to_string(&path).expect("runtime file");
    serde_json::from_str::<Value>(&body).expect("the runtime file is json")["client_credential"]
        .as_str()
        .expect("the runtime file holds the credential")
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
    sink: Sink,
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

/// Open an authenticated client connection.
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

/// The answers the fake plugin gives.
#[derive(Debug, Clone, Default)]
struct Answers {
    /// The nodes the plugin returns for a listing.
    nodes: Vec<Value>,
    /// Whether the plugin reports a partial traversal.
    truncated: bool,
    /// Whether the plugin stays silent.
    silent: bool,
}

/// One request the plugin received.
#[derive(Debug, Clone)]
struct Received {
    /// The operation the daemon asked for.
    op: String,
    /// The parameters the daemon sent.
    params: Value,
}

/// A fake plugin that serves the daemon.
struct FakePlugin {
    /// The session the daemon opened.
    session_id: figtether_model::ids::SessionId,
    /// The frames the plugin writes to the bridge.
    outbox: tokio::sync::mpsc::Sender<String>,
    /// The requests the plugin received.
    received: Arc<Mutex<Vec<Received>>>,
    /// The answers the plugin gives.
    answers: Arc<Mutex<Answers>>,
}

impl FakePlugin {
    /// Pair a plugin with the daemon.
    async fn connect(port: u16, secret: &str) -> Self {
        Self::connect_with(port, secret, Answers::default()).await
    }

    /// Pair a plugin with explicit answers.
    async fn connect_with(port: u16, secret: &str, answers: Answers) -> Self {
        let url = format!("ws://127.0.0.1:{port}/bridge");
        let (stream, _answer) = connect_async(url).await.expect("the plugin connects");
        let (mut sink, mut source) = stream.split();
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
        sink.send(WsMessage::Text(
            serde_json::to_string(&frame).expect("encodes").into(),
        ))
        .await
        .expect("written");
        let answer = source
            .next()
            .await
            .expect("the daemon answers")
            .expect("a frame");
        let WsMessage::Text(text) = answer else {
            panic!("the daemon answers with a text frame");
        };
        let frame = BridgeFrame::from_bytes(text.as_bytes()).expect("a frame");
        let accepted: AuthAccepted = frame.payload_of().expect("the session");

        let received = Arc::new(Mutex::new(Vec::new()));
        let answers = Arc::new(Mutex::new(answers));
        let (outbox, mut outbox_rx) = tokio::sync::mpsc::channel::<String>(32);
        tokio::spawn(async move {
            while let Some(text) = outbox_rx.recv().await {
                if sink.send(WsMessage::Text(text.into())).await.is_err() {
                    break;
                }
            }
        });
        let task_outbox = outbox.clone();
        let task_received = received.clone();
        let task_answers = answers.clone();
        tokio::spawn(async move {
            while let Some(Ok(message)) = source.next().await {
                let text = match message {
                    WsMessage::Text(text) => text.to_string(),
                    WsMessage::Binary(bytes) => String::from_utf8_lossy(&bytes).to_string(),
                    _ => continue,
                };
                let Ok(frame) = BridgeFrame::from_bytes(text.as_bytes()) else {
                    continue;
                };
                if frame.message_type != MessageType::Request {
                    continue;
                }
                let Ok(payload) = frame.payload_of::<RequestPayload>() else {
                    continue;
                };
                task_received.lock().expect("lock").push(Received {
                    op: payload.op.as_str().to_string(),
                    params: payload.params.clone(),
                });
                let plan = task_answers.lock().expect("lock").clone();
                if plan.silent {
                    continue;
                }
                let data = match payload.op {
                    BridgeOp::ListNodes => json!({
                        "nodes": plan.nodes,
                        "remaining": plan.truncated,
                        "truncated": plan.truncated,
                    }),
                    BridgeOp::ExportAssets => json!({
                        "node_id": "1:2",
                        "mime": "image/png",
                        "width": 4,
                        "height": 4,
                        "bytes": PNG_BYTES,
                        "hash": "sha256:0000",
                    }),
                    _ => json!({ "nodes": plan.nodes }),
                };
                let mut reply = BridgeFrame::new(
                    MessageType::Result,
                    frame.request_id.clone(),
                    serde_json::to_value(ResultPayload { data }).expect("encodes"),
                );
                reply.connection_epoch = frame.connection_epoch;
                let text = serde_json::to_string(&reply).expect("encodes");
                if task_outbox.send(text).await.is_err() {
                    break;
                }
            }
        });

        Self {
            session_id: accepted.session_id,
            outbox,
            received,
            answers,
        }
    }

    /// Grant a scope to the session.
    ///
    /// The payload is built by hand, so the test does not depend on the
    /// shape of the protocol struct: the daemon reads the roots and the
    /// mode out of the payload.
    async fn grant(&self, roots: &[&str]) {
        self.grant_with_mode(roots, "read").await;
    }

    /// Grant a scope to the session with a mode.
    async fn grant_with_mode(&self, roots: &[&str], mode: &str) {
        let payload = json!({
            "root_ids": roots.iter().map(|root| root.to_string()).collect::<Vec<_>>(),
            "grant_epoch": 0,
            "mode": mode,
        });
        let frame = BridgeFrame::new(MessageType::GrantUpdate, "grant", payload);
        let text = serde_json::to_string(&frame).expect("encodes");
        self.outbox.send(text).await.expect("the frame is written");
    }

    /// The operations the plugin received.
    fn operations(&self) -> Vec<String> {
        self.received
            .lock()
            .expect("lock")
            .iter()
            .map(|entry| entry.op.clone())
            .collect()
    }

    /// The parameters of the last request of an operation.
    fn last_params(&self, op: &str) -> Option<Value> {
        self.received
            .lock()
            .expect("lock")
            .iter()
            .rev()
            .find(|entry| entry.op == op)
            .map(|entry| entry.params.clone())
    }

    /// Change the answers of the plugin.
    fn set_answers(&self, answers: Answers) {
        *self.answers.lock().expect("lock") = answers;
    }
}

/// Wait until the daemon reports a state for the session.
async fn wait_for_state(
    client: &mut Socket,
    session_id: &figtether_model::ids::SessionId,
    state: &str,
) -> Value {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let sessions = list_sessions(client).await;
        if let Some(session) = sessions
            .iter()
            .find(|session| session["session_id"].as_str() == Some(&session_id.to_string()))
        {
            if session["state"].as_str() == Some(state) {
                return session.clone();
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the session did not reach the state {state}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// The parameters of a listing.
fn list_params(session_id: figtether_model::ids::SessionId, root_id: &str) -> Value {
    json!({
        "session_id": session_id.to_string(),
        "root_id": root_id,
    })
}

/// A node of the fake document.
fn node(id: &str) -> Value {
    json!({
        "id": id,
        "node_type": "FRAME",
        "name": id,
    })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_session_is_paired_after_the_authentication_and_ready_after_the_grant() {
    let (handle, temp) = start_daemon().await;
    let port = handle.port();

    let secret = mint_secret(port);
    let plugin = FakePlugin::connect(port, &secret).await;
    let mut client = open_client(port, &credential(&temp))
        .await
        .expect("the daemon accepts the client");

    // The authentication pairs the session.
    let paired = wait_for_state(&mut client, &plugin.session_id, "paired").await;
    assert_eq!(paired["state"], "paired", "{paired}");

    // The grant of the user makes the session serve reads.
    plugin.grant(&["1:2"]).await;
    let ready = wait_for_state(&mut client, &plugin.session_id, "ready").await;
    assert_eq!(ready["state"], "ready", "{ready}");
    assert_eq!(ready["grant_epoch"], 1, "{ready}");
    assert_eq!(ready["root_ids"], 1, "{ready}");

    handle.shutdown();
    handle.wait_stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_list_nodes_request_reaches_the_plugin_as_the_list_nodes_operation() {
    let (handle, temp) = start_daemon().await;
    let port = handle.port();

    let secret = mint_secret(port);
    let plugin = FakePlugin::connect_with(
        port,
        &secret,
        Answers {
            nodes: vec![node("1:3")],
            ..Answers::default()
        },
    )
    .await;
    plugin.grant(&["1:2"]).await;

    let mut client = open_client(port, &credential(&temp))
        .await
        .expect("the daemon accepts the client");
    wait_for_state(&mut client, &plugin.session_id, "ready").await;

    let answer = request(
        &mut client,
        ClientRequest::ListNodes {
            request_id: "req_list".to_string(),
            params: list_params(plugin.session_id, "1:2"),
        },
    )
    .await;

    assert_eq!(answer["type"], "ok", "{answer}");
    assert_eq!(
        plugin.operations(),
        vec!["list_nodes".to_string()],
        "the plugin receives the operation of the request"
    );
    let params = plugin.last_params("list_nodes").expect("the request");
    assert_eq!(params["root_id"], "1:2", "{params}");
    assert!(params.get("session_id").is_none(), "{params}");

    handle.shutdown();
    handle.wait_stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_truncated_traversal_carries_a_cursor_that_dies_with_the_grant() {
    let (handle, temp) = start_daemon().await;
    let port = handle.port();

    let secret = mint_secret(port);
    let plugin = FakePlugin::connect_with(
        port,
        &secret,
        Answers {
            nodes: vec![node("1:3"), node("1:4")],
            truncated: true,
            ..Answers::default()
        },
    )
    .await;
    plugin.grant(&["1:2"]).await;

    let mut client = open_client(port, &credential(&temp))
        .await
        .expect("the daemon accepts the client");
    wait_for_state(&mut client, &plugin.session_id, "ready").await;

    let answer = request(
        &mut client,
        ClientRequest::ListNodes {
            request_id: "req_page".to_string(),
            params: list_params(plugin.session_id, "1:2"),
        },
    )
    .await;
    assert_eq!(answer["type"], "ok", "{answer}");
    let coverage = &answer["envelope"]["coverage"];
    assert_eq!(coverage["truncated"], true, "{answer}");
    let cursor = coverage["next_cursor"]
        .as_str()
        .expect("a truncated traversal carries a cursor")
        .to_string();

    // The continuation carries the position of the traversal to the plugin.
    let mut continuation = list_params(plugin.session_id, "1:2");
    continuation["cursor"] = Value::String(cursor.clone());
    let answer = request(
        &mut client,
        ClientRequest::ListNodes {
            request_id: "req_next".to_string(),
            params: continuation,
        },
    )
    .await;
    assert_eq!(answer["type"], "ok", "{answer}");
    let params = plugin.last_params("list_nodes").expect("the request");
    assert_eq!(params["after"], "1:4", "{params}");

    // A new grant moves the epoch, so the cursor of the old epoch dies.
    plugin.grant(&["1:2"]).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let mut continuation = list_params(plugin.session_id, "1:2");
    continuation["cursor"] = Value::String(cursor);
    let answer = request(
        &mut client,
        ClientRequest::ListNodes {
            request_id: "req_stale".to_string(),
            params: continuation,
        },
    )
    .await;
    assert_eq!(answer["type"], "error", "{answer}");
    assert_eq!(answer["error"]["code"], "CURSOR_EXPIRED", "{answer}");

    handle.shutdown();
    handle.wait_stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_read_fills_the_cache_and_an_export_fills_the_asset_store() {
    let (handle, temp) = start_daemon().await;
    let port = handle.port();

    let secret = mint_secret(port);
    let plugin = FakePlugin::connect_with(
        port,
        &secret,
        Answers {
            nodes: vec![node("1:3")],
            ..Answers::default()
        },
    )
    .await;
    plugin.grant(&["1:2"]).await;

    let mut client = open_client(port, &credential(&temp))
        .await
        .expect("the daemon accepts the client");
    wait_for_state(&mut client, &plugin.session_id, "ready").await;

    let answer = request(
        &mut client,
        ClientRequest::ListNodes {
            request_id: "req_read".to_string(),
            params: list_params(plugin.session_id, "1:2"),
        },
    )
    .await;
    assert_eq!(answer["type"], "ok", "{answer}");
    let status = handle.status();
    assert!(
        status.cache_entries > 0,
        "the read fills the cache of the daemon: {status:?}"
    );

    let export = request(
        &mut client,
        ClientRequest::ExportAssets {
            request_id: "req_export".to_string(),
            params: json!({
                "session_id": plugin.session_id.to_string(),
                "node_ids": ["1:2"],
                "format": "png",
            }),
        },
    )
    .await;
    assert_eq!(export["type"], "ok", "{export}");
    assert!(
        export["envelope"]["data"]["asset_id"].is_string(),
        "the export mints an asset id: {export}"
    );
    let status = handle.status();
    assert!(
        status.asset_count > 0,
        "the export fills the asset store of the daemon: {status:?}"
    );

    handle.shutdown();
    handle.wait_stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_revoke_clears_the_cache_and_the_asset_store() {
    let (handle, temp) = start_daemon().await;
    let port = handle.port();

    let secret = mint_secret(port);
    let plugin = FakePlugin::connect_with(
        port,
        &secret,
        Answers {
            nodes: vec![node("1:3")],
            ..Answers::default()
        },
    )
    .await;
    plugin.grant(&["1:2"]).await;

    let mut client = open_client(port, &credential(&temp))
        .await
        .expect("the daemon accepts the client");
    wait_for_state(&mut client, &plugin.session_id, "ready").await;

    request(
        &mut client,
        ClientRequest::ListNodes {
            request_id: "req_read".to_string(),
            params: list_params(plugin.session_id, "1:2"),
        },
    )
    .await;
    request(
        &mut client,
        ClientRequest::ExportAssets {
            request_id: "req_export".to_string(),
            params: json!({
                "session_id": plugin.session_id.to_string(),
                "node_ids": ["1:2"],
                "format": "png",
            }),
        },
    )
    .await;
    let status = handle.status();
    assert!(status.cache_entries > 0, "{status:?}");
    assert!(status.asset_count > 0, "{status:?}");

    // A scope change issues a new grant. The assets of the older grant die
    // with the session as well.
    plugin.grant(&["1:2"]).await;
    tokio::time::sleep(Duration::from_millis(50)).await;

    let answer = request(
        &mut client,
        ClientRequest::Revoke {
            request_id: "req_revoke".to_string(),
            session_id: plugin.session_id,
        },
    )
    .await;
    assert_eq!(answer["type"], "ok", "{answer}");

    let status = handle.status();
    assert_eq!(status.cache_entries, 0, "the revoke clears the cache");
    assert_eq!(status.asset_count, 0, "the revoke clears the asset store");

    // The revoked session refuses every later read.
    let answer = request(
        &mut client,
        ClientRequest::ListNodes {
            request_id: "req_after".to_string(),
            params: list_params(plugin.session_id, "1:2"),
        },
    )
    .await;
    assert_eq!(answer["type"], "error", "{answer}");
    assert_eq!(answer["error"]["code"], "REVOKED", "{answer}");

    handle.shutdown();
    handle.wait_stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn missed_plugin_heartbeats_close_the_socket_and_allow_authenticated_recovery() {
    let (handle, temp) = start_daemon().await;
    let port = handle.port();
    let mut plugin = Socket::open(port, "/bridge").await;
    let hello = |secret: String| {
        serde_json::to_value(BridgeFrame::new(
            MessageType::HelloAuth,
            "hello",
            json!({
                "secret": secret,
                "client_id": PLUGIN_LABEL,
                "protocol_version": PROTOCOL_VERSION,
            }),
        ))
        .expect("hello serializes")
    };
    plugin.send(&hello(mint_secret(port))).await;
    let accepted: BridgeFrame =
        serde_json::from_value(plugin.next().await.expect("auth")).expect("auth frame");
    let auth: AuthAccepted = accepted.payload_of().expect("accepted");
    let grant = serde_json::to_value(BridgeFrame::new(
        MessageType::GrantUpdate,
        "grant",
        json!({ "root_ids": ["1:2"], "grant_epoch": 0, "mode": "read" }),
    ))
    .expect("grant serializes");
    plugin.send(&grant).await;
    let mut client = open_client(port, &credential(&temp)).await.expect("client");
    wait_for_state(&mut client, &auth.session_id, "ready").await;

    // Receive daemon heartbeats but send none: the plugin-side watchdog
    // would otherwise see traffic forever while every read is refused.
    tokio::time::timeout(Duration::from_secs(22), async {
        while plugin.next().await.is_some() {}
    })
    .await
    .expect("daemon closes the timed-out connection");
    wait_for_state(&mut client, &auth.session_id, "offline").await;
    assert_eq!(handle.status().plugin_connections, 0);

    let mut reconnected = Socket::open(port, "/bridge").await;
    reconnected.send(&hello(auth.session_credential)).await;
    let accepted: BridgeFrame =
        serde_json::from_value(reconnected.next().await.expect("auth")).expect("auth frame");
    let recovered: AuthAccepted = accepted.payload_of().expect("re-authenticated");
    assert_eq!(recovered.session_id, auth.session_id);
    assert!(recovered.connection_epoch > auth.connection_epoch);
    // Authentication alone does not make an offline grant ready.
    wait_for_state(&mut client, &auth.session_id, "paired").await;
    reconnected.send(&grant).await;
    wait_for_state(&mut client, &auth.session_id, "ready").await;
    handle.shutdown();
    handle.wait_stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fresh_pairing_with_the_same_plugin_label_does_not_inherit_a_grant() {
    let (handle, temp) = start_daemon().await;
    let port = handle.port();
    let plugin = FakePlugin::connect(port, &mint_secret(port)).await;
    plugin.grant(&["1:2"]).await;
    let mut client = open_client(port, &credential(&temp)).await.expect("client");
    wait_for_state(&mut client, &plugin.session_id, "ready").await;
    let replacement = FakePlugin::connect(port, &mint_secret(port)).await;
    assert_ne!(replacement.session_id, plugin.session_id);
    let paired = wait_for_state(&mut client, &replacement.session_id, "paired").await;
    assert_eq!(paired["granted_root_ids"], json!([]));
    assert!(paired["grant_mode"].is_null());
    handle.shutdown();
    handle.wait_stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_reconnect_never_revives_a_revoked_grant() {
    let (handle, temp) = start_daemon().await;
    let port = handle.port();

    let secret = mint_secret(port);
    let plugin = FakePlugin::connect(port, &secret).await;
    plugin.grant(&["1:2"]).await;

    let mut client = open_client(port, &credential(&temp))
        .await
        .expect("the daemon accepts the client");
    wait_for_state(&mut client, &plugin.session_id, "ready").await;

    let answer = request(
        &mut client,
        ClientRequest::Revoke {
            request_id: "req_revoke".to_string(),
            session_id: plugin.session_id,
        },
    )
    .await;
    assert_eq!(answer["type"], "ok", "{answer}");

    // The plugin pairs again. The new connection opens a new session, and the
    // old session stays revoked.
    let second_secret = mint_secret(port);
    let second = FakePlugin::connect(port, &second_secret).await;
    assert_ne!(
        second.session_id, plugin.session_id,
        "a reconnect after a revocation opens a new session"
    );
    let paired = wait_for_state(&mut client, &second.session_id, "paired").await;
    assert_eq!(paired["state"], "paired", "{paired}");

    let answer = request(
        &mut client,
        ClientRequest::ListNodes {
            request_id: "req_old".to_string(),
            params: list_params(plugin.session_id, "1:2"),
        },
    )
    .await;
    assert_eq!(answer["type"], "error", "{answer}");
    assert_eq!(answer["error"]["code"], "REVOKED", "{answer}");

    handle.shutdown();
    handle.wait_stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_read_that_never_ends_leaves_the_lane_of_the_session_free() {
    let (handle, temp) = start_daemon().await;
    let port = handle.port();

    let secret = mint_secret(port);
    let plugin = FakePlugin::connect_with(
        port,
        &secret,
        Answers {
            silent: true,
            ..Answers::default()
        },
    )
    .await;
    plugin.grant(&["1:2"]).await;

    let mut client = open_client(port, &credential(&temp))
        .await
        .expect("the daemon accepts the client");
    wait_for_state(&mut client, &plugin.session_id, "ready").await;

    // The plugin never answers, so the read misses its deadline.
    let answer = request(
        &mut client,
        ClientRequest::ListNodes {
            request_id: "req_slow".to_string(),
            params: list_params(plugin.session_id, "1:2"),
        },
    )
    .await;
    assert_eq!(answer["type"], "error", "{answer}");
    assert_eq!(answer["error"]["code"], "TIMEOUT", "{answer}");

    // The lane of the session is free again: the next read runs.
    plugin.set_answers(Answers {
        nodes: vec![node("1:3")],
        ..Answers::default()
    });
    let answer = request(
        &mut client,
        ClientRequest::ListNodes {
            request_id: "req_after".to_string(),
            params: list_params(plugin.session_id, "1:2"),
        },
    )
    .await;
    assert_eq!(answer["type"], "ok", "{answer}");
    assert_eq!(
        answer["envelope"]["coverage"]["returned_nodes"], 1,
        "{answer}"
    );

    handle.shutdown();
    handle.wait_stopped().await;
}
