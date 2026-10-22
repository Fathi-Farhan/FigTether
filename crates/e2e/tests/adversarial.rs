//! Adversarial test suite of the shipped FigTether daemon.
//!
//! Every test in this file acts as a hostile local process or as a malicious
//! web page. It opens real sockets against the shipped daemon, forges the
//! headers a browser controls, presents credentials that do not belong to it,
//! and sends the payloads a hostile peer would send. The daemon is the local
//! trust boundary of the product, so a request that should be refused must be
//! refused.
//!
//! The suite covers the threat classes of `docs/05-security/01-threat-model.md`
//! and the acceptance scenarios `AT06`, `AT13`, and `AT17`:
//!
//! - a forged `Host` header on every route, including a DNS rebinding name;
//! - a hostile `Origin` header on every route;
//! - the control endpoints under a `null` origin and without an origin;
//! - a wrong client credential, a replayed secret, and an expired secret;
//! - a credential of one channel presented on the other channel;
//! - an out of scope read and two sessions that share every node id;
//! - hostile frames: oversized, deeply nested, wrong version, unknown type,
//!   and a chunk that lies about its total size;
//! - resource pressure: a saturated queue answers `BUSY`;
//! - the runtime directory and the loopback binding of the daemon.
//!
//! A failing test names the defect of the daemon it found. No assertion is
//! weakened to make a failure disappear.

use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use figtether_e2e::fake_client::FakeLocalClient;
use figtether_e2e::fake_plugin::{FakePlugin, PluginError};
use figtether_e2e::fixtures::FixtureCorpus;
use figtether_e2e::params;
use figtether_e2e::shipped::{ShippedDaemon, ShortLivedPairingDaemon};
use figtether_model::envelope::Envelope;
use figtether_model::errors::ErrorCode;
use figtether_model::ids::SessionId;
use figtether_protocol::client::{ClientAuth, ClientRequest, ClientResponse};
use figtether_protocol::{
    AuthRequest, BridgeFrame, MessageType, MAX_FRAME_BYTES, MAX_TRANSFER_BYTES, PROTOCOL_VERSION,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

/// The client id the hostile local clients present.
const CLIENT_ID: &str = "adversarial-client";

/// The label the hostile plugin presents.
const PLUGIN_ID: &str = "adversarial-plugin";

/// The label of the second hostile plugin.
const OTHER_PLUGIN_ID: &str = "adversarial-plugin-two";

/// A node id that is in no document of the corpus.
const OUTSIDE_NODE: &str = "9:999999";

/// The label of the document the hostile plugin serves.
const DOCUMENT_LABEL: &str = "FX-ALPHA-DOC";

/// The label of the document of the second hostile plugin.
const OTHER_DOCUMENT_LABEL: &str = "FX-BETA-DOC";

/// The routes of the daemon, with the method that reaches them.
const ROUTES: [(&str, &str); 5] = [
    ("GET", "/health"),
    ("GET", "/client"),
    ("GET", "/bridge"),
    ("POST", "/control/pair"),
    ("POST", "/control/shutdown"),
];

/// How long a test waits for the daemon to answer a raw request.
const RAW_TIMEOUT: Duration = Duration::from_secs(20);

/// How long a test waits for the daemon to answer a frame.
const FRAME_TIMEOUT: Duration = Duration::from_secs(5);

/// How long a test waits for a grant to be recorded by the daemon.
const GRANT_TIMEOUT: Duration = Duration::from_secs(20);

/// A raw WebSocket connection to the daemon.
type RawSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// What the daemon answered with.
#[derive(Debug)]
enum Answer {
    /// The daemon closed the connection.
    Closed(Vec<String>),
    /// The daemon kept the connection open.
    Open(Vec<String>),
}

impl Answer {
    /// The frames the daemon sent.
    fn frames(&self) -> &[String] {
        match self {
            Self::Closed(frames) | Self::Open(frames) => frames,
        }
    }
}

/// What a hostile local client received from the daemon.
#[derive(Debug)]
struct ClientOutcome {
    /// The frames the daemon sent.
    frames: Vec<String>,
    /// Whether the daemon refused the connection, that is, whether the
    /// connection ended instead of staying open.
    refused: bool,
    /// How the connection ended.
    end: String,
}

// ---------------------------------------------------------------------------
// Helpers of the raw lane
// ---------------------------------------------------------------------------

/// Build a raw request for one route of the daemon.
///
/// The WebSocket routes carry the upgrade headers, so a request with a
/// correct host is upgraded and a request with a forged host is refused by
/// the header guard and not by the route.
fn raw_request(method: &str, path: &str, host: &str, origin: Option<&str>) -> String {
    let mut request = format!("{method} {path} HTTP/1.1\r\nHost: {host}\r\n");
    if path == "/client" || path == "/bridge" {
        request.push_str(
            "Connection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\n\
             Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n",
        );
    } else {
        request.push_str("Connection: close\r\n");
    }
    if let Some(origin) = origin {
        request.push_str(&format!("Origin: {origin}\r\n"));
    }
    if method == "POST" {
        request.push_str("Content-Length: 0\r\n");
    }
    request.push_str("\r\n");
    request
}

/// Send a raw request to the daemon and read the answer.
///
/// The reader stops when the head of the answer is complete and the socket
/// carries nothing more for a short moment. A refused request answers with a
/// status line and a body, an accepted upgrade answers with a status line and
/// keeps the socket open.
async fn raw_exchange(addr: SocketAddr, request: &str) -> String {
    let mut stream = TcpStream::connect(addr)
        .await
        .expect("the daemon accepts the connection");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("the request is written");
    let mut answer = Vec::new();
    let mut chunk = [0u8; 4096];
    let deadline = tokio::time::Instant::now() + RAW_TIMEOUT;
    loop {
        let read = match tokio::time::timeout_at(deadline, stream.read(&mut chunk)).await {
            Err(_elapsed) => break,
            Ok(Err(_err)) => break,
            Ok(Ok(0)) => break,
            Ok(Ok(read)) => read,
        };
        answer.extend_from_slice(&chunk[..read]);
        if !answer.windows(4).any(|window| window == b"\r\n\r\n") {
            continue;
        }
        let more = tokio::time::timeout(Duration::from_millis(300), stream.read(&mut chunk)).await;
        match more {
            Ok(Ok(0)) => break,
            Ok(Ok(read)) => answer.extend_from_slice(&chunk[..read]),
            Ok(Err(_err)) => break,
            Err(_elapsed) => break,
        }
        break;
    }
    String::from_utf8_lossy(&answer).to_string()
}

/// The status code of a raw answer.
fn status_of(answer: &str) -> u16 {
    answer
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or(0)
}

/// The body of a raw answer.
fn body_of(answer: &str) -> String {
    answer
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_string())
        .unwrap_or_default()
}

/// The error code of a refused answer.
fn refusal_code(answer: &str) -> Option<String> {
    serde_json::from_str::<Value>(&body_of(answer))
        .ok()?
        .get("code")?
        .as_str()
        .map(str::to_string)
}

/// Ask the daemon for its status and return the status code of the answer.
async fn health_status(daemon: &ShippedDaemon) -> u16 {
    let port = daemon.port();
    let request = raw_request("GET", "/health", &format!("127.0.0.1:{port}"), None);
    status_of(&raw_exchange(daemon.addr(), &request).await)
}

/// Open a raw WebSocket connection to the daemon.
async fn raw_socket(url: &str) -> RawSocket {
    let (socket, _response) = connect_async(url).await.expect("the socket connects");
    socket
}

/// Collect the frames the daemon sends until it closes the connection or the
/// deadline passes.
async fn collect(socket: &mut RawSocket, deadline: Duration) -> Answer {
    let mut frames = Vec::new();
    let end = tokio::time::Instant::now() + deadline;
    loop {
        match tokio::time::timeout_at(end, socket.next()).await {
            Err(_elapsed) => return Answer::Open(frames),
            Ok(None) => return Answer::Closed(frames),
            Ok(Some(Err(_err))) => return Answer::Closed(frames),
            Ok(Some(Ok(message))) => match message {
                Message::Text(text) => frames.push(text.to_string()),
                Message::Binary(bytes) => frames.push(String::from_utf8_lossy(&bytes).to_string()),
                Message::Close(_) => return Answer::Closed(frames),
                Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => continue,
            },
        }
    }
}

/// Whether the daemon answered with a session credential.
///
/// A session credential only ever leaves the daemon after a successful
/// authentication, so its presence proves that the connection was admitted.
fn authenticated(frames: &[String]) -> bool {
    frames.iter().any(|frame| {
        serde_json::from_str::<Value>(frame)
            .ok()
            .and_then(|value| value.get("payload").cloned())
            .and_then(|payload| payload.get("session_credential").cloned())
            .is_some()
    })
}

/// Open a client connection, present a credential, and collect every frame
/// the daemon sends before the connection ends.
async fn client_frames(
    url: &str,
    credential: &str,
    client_id: &str,
    follow_up: Option<ClientRequest>,
) -> ClientOutcome {
    let mut socket = raw_socket(url).await;
    let auth = ClientAuth {
        credential: credential.to_string(),
        client_id: client_id.to_string(),
        protocol_version: PROTOCOL_VERSION.to_string(),
    };
    let encoded = serde_json::to_string(&auth).expect("the authentication serializes");
    socket
        .send(Message::Text(encoded.into()))
        .await
        .expect("the authentication is written");
    if let Some(request) = follow_up {
        let encoded = serde_json::to_string(&request).expect("the request serializes");
        // The daemon may close the connection right after a refused
        // authentication, so a broken pipe here is not a failure of the test.
        let _ = socket.send(Message::Text(encoded.into())).await;
    }
    let mut frames = Vec::new();
    let deadline = tokio::time::Instant::now() + FRAME_TIMEOUT;
    loop {
        match tokio::time::timeout_at(deadline, socket.next()).await {
            Err(_elapsed) => {
                return ClientOutcome {
                    frames,
                    refused: false,
                    end: "the daemon kept the connection open".to_string(),
                }
            }
            Ok(None) => {
                return ClientOutcome {
                    frames,
                    refused: true,
                    end: "the daemon closed the socket without a close frame".to_string(),
                }
            }
            Ok(Some(Err(err))) => {
                return ClientOutcome {
                    frames,
                    refused: true,
                    end: format!("the daemon closed the socket with the error {err}"),
                }
            }
            Ok(Some(Ok(message))) => match message {
                Message::Text(text) => frames.push(text.to_string()),
                Message::Binary(bytes) => frames.push(String::from_utf8_lossy(&bytes).to_string()),
                Message::Close(_) => {
                    return ClientOutcome {
                        frames,
                        refused: true,
                        end: "the daemon sent a close frame".to_string(),
                    }
                }
                Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => continue,
            },
        }
    }
}

/// Open an authenticated client connection.
///
/// The helper waits for the acknowledgement of the daemon, so the connection
/// is ready for a request.
async fn raw_client(url: &str, credential: &str, client_id: &str) -> RawSocket {
    let mut socket = raw_socket(url).await;
    let auth = ClientAuth {
        credential: credential.to_string(),
        client_id: client_id.to_string(),
        protocol_version: PROTOCOL_VERSION.to_string(),
    };
    let encoded = serde_json::to_string(&auth).expect("the authentication serializes");
    socket
        .send(Message::Text(encoded.into()))
        .await
        .expect("the authentication is written");
    let answer = tokio::time::timeout(FRAME_TIMEOUT, socket.next())
        .await
        .expect("the daemon answers the authentication in time")
        .expect("the connection stays open")
        .expect("the answer is a frame");
    let text = match answer {
        Message::Text(text) => text.to_string(),
        other => panic!("the daemon answered the authentication with {other:?}"),
    };
    let response: ClientResponse =
        serde_json::from_str(&text).expect("the answer is a client response");
    assert!(
        matches!(response, ClientResponse::Ok { .. }),
        "the daemon refused the client credential: {text}"
    );
    socket
}

/// Open a bridge connection and authenticate it with a pairing secret.
async fn raw_bridge_session(url: &str, secret: &str, client_id: &str) -> RawSocket {
    let mut socket = raw_socket(url).await;
    let request = AuthRequest {
        secret: secret.to_string(),
        client_id: client_id.to_string(),
        protocol_version: PROTOCOL_VERSION.to_string(),
    };
    let frame = BridgeFrame::new(
        MessageType::HelloAuth,
        "hello-adversarial",
        serde_json::to_value(request).expect("the authentication serializes"),
    );
    let encoded = serde_json::to_string(&frame).expect("the frame serializes");
    socket
        .send(Message::Text(encoded.into()))
        .await
        .expect("the frame is written");
    let answer = tokio::time::timeout(FRAME_TIMEOUT, socket.next())
        .await
        .expect("the daemon answers the authentication in time")
        .expect("the connection stays open")
        .expect("the answer is a frame");
    let text = match answer {
        Message::Text(text) => text.to_string(),
        other => panic!("the daemon answered the authentication with {other:?}"),
    };
    let accepted: Value = serde_json::from_str(&text).expect("the answer is JSON");
    assert!(
        accepted["payload"]["session_credential"].is_string(),
        "the daemon refused the pairing secret: {text}"
    );
    socket
}

/// A plugin that answers every read with content the grant does not cover.
///
/// The helper stands in for a hostile or a buggy plugin. It authenticates
/// with a pairing secret, grants a scope, and then answers every request of
/// the daemon with the content of a node outside that scope.
async fn hostile_plugin(
    url: String,
    secret: String,
    client_id: String,
    root: String,
    answer: Value,
) {
    let mut socket = raw_socket(&url).await;
    let request = AuthRequest {
        secret,
        client_id,
        protocol_version: PROTOCOL_VERSION.to_string(),
    };
    let frame = BridgeFrame::new(
        MessageType::HelloAuth,
        "hello-hostile",
        serde_json::to_value(request).expect("the authentication serializes"),
    );
    let encoded = serde_json::to_string(&frame).expect("the frame serializes");
    if socket.send(Message::Text(encoded.into())).await.is_err() {
        return;
    }
    let accepted = tokio::time::timeout(FRAME_TIMEOUT, socket.next()).await;
    if !matches!(accepted, Ok(Some(Ok(_)))) {
        return;
    }

    // The plugin grants a scope to the session it opened.
    let grant = BridgeFrame::new(
        MessageType::GrantUpdate,
        "grant-hostile",
        json!({ "root_ids": [root], "grant_epoch": 0 }),
    );
    let encoded = serde_json::to_string(&grant).expect("the frame serializes");
    if socket.send(Message::Text(encoded.into())).await.is_err() {
        return;
    }

    // Every request of the daemon is answered with the hostile content.
    while let Some(Ok(message)) = socket.next().await {
        let Message::Text(text) = message else {
            continue;
        };
        let Ok(frame) = BridgeFrame::from_bytes(text.as_bytes()) else {
            continue;
        };
        if frame.message_type != MessageType::Request {
            continue;
        }
        let mut reply = BridgeFrame::new(
            MessageType::Result,
            frame.request_id.clone(),
            json!({ "data": answer }),
        );
        reply.connection_epoch = frame.connection_epoch;
        let encoded = serde_json::to_string(&reply).expect("the frame serializes");
        if socket.send(Message::Text(encoded.into())).await.is_err() {
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers of the session lane
// ---------------------------------------------------------------------------

/// Wait until the daemon reports the grant of a session.
async fn wait_for_grant(client: &mut FakeLocalClient, session_id: SessionId, root_count: usize) {
    let deadline = Instant::now() + GRANT_TIMEOUT;
    loop {
        let sessions = client
            .list_sessions()
            .await
            .expect("the daemon lists the sessions");
        if let Some(session) = sessions
            .iter()
            .find(|session| session["session_id"].as_str() == Some(&session_id.to_string()))
        {
            let granted = session["root_ids"].as_u64().unwrap_or(0) as usize;
            let epoch = session["grant_epoch"].as_u64().unwrap_or(0);
            if granted == root_count && epoch > 0 {
                return;
            }
        }
        assert!(
            Instant::now() < deadline,
            "the daemon did not record the grant of the session {session_id}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Grant a scope to the session of the shipped daemon.
async fn grant_to_shipped(plugin: &FakePlugin, client: &mut FakeLocalClient, root: &str) {
    let roots = vec![root.to_string()];
    plugin
        .grant_quiet(&roots)
        .await
        .expect("the grant frame is sent");
    wait_for_grant(client, plugin.session_id(), roots.len()).await;
}

/// Wait until the daemon reports a session that holds a grant.
async fn wait_for_any_grant(client: &mut FakeLocalClient) -> SessionId {
    let deadline = Instant::now() + GRANT_TIMEOUT;
    loop {
        let sessions = client
            .list_sessions()
            .await
            .expect("the daemon lists the sessions");
        if let Some(session) = sessions.iter().find(|session| {
            session["root_ids"].as_u64().unwrap_or(0) > 0
                && session["grant_epoch"].as_u64().unwrap_or(0) > 0
        }) {
            if let Some(session_id) = session["session_id"].as_str() {
                return SessionId::from_str(session_id).expect("the session id parses");
            }
        }
        assert!(
            Instant::now() < deadline,
            "the daemon did not record a grant of a session"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// The text of one node inside an envelope payload.
///
/// The shipped daemon carries the text of a node in the typography of the
/// normalized model, the harness daemon carries it in a plain text field.
fn node_text(envelope: &Envelope, node_id: &str) -> Option<String> {
    let node = envelope
        .data
        .get("nodes")
        .and_then(Value::as_array)?
        .iter()
        .find(|node| node.get("id").and_then(Value::as_str) == Some(node_id))?;
    node.get("typography")
        .and_then(|typography| typography.get("text"))
        .and_then(Value::as_str)
        .or_else(|| {
            node.get("text")
                .and_then(|text| text.get("text"))
                .and_then(Value::as_str)
        })
        .map(str::to_string)
}

// ---------------------------------------------------------------------------
// TH01 - a forged Host header
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_foreign_host_header_is_refused_on_every_route() {
    let daemon = ShippedDaemon::start()
        .await
        .expect("the shipped daemon starts");
    let port = daemon.port();
    let addr = daemon.addr();

    for (method, path) in ROUTES {
        let request = raw_request(method, path, &format!("attacker.example:{port}"), None);
        let answer = raw_exchange(addr, &request).await;
        assert_eq!(
            status_of(&answer),
            403,
            "the route {path} answered a request with the foreign Host header attacker.example:{port}: {answer}"
        );
        assert_eq!(
            refusal_code(&answer).as_deref(),
            Some("PERMISSION_DENIED"),
            "the route {path} refused the request without the code PERMISSION_DENIED: {answer}"
        );
    }

    // The refused shutdown did not stop the daemon.
    assert_eq!(
        health_status(&daemon).await,
        200,
        "the daemon stopped after the refused requests"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_dns_rebinding_host_header_is_refused_on_every_route() {
    let daemon = ShippedDaemon::start()
        .await
        .expect("the shipped daemon starts");
    let port = daemon.port();
    let addr = daemon.addr();

    // The name below belongs to an attacker who points it at the loopback
    // address of this machine. The daemon never resolves the name of the Host
    // header, so the resolution cannot help the attacker.
    let rebinding_host = format!("rebind.attacker.example:{port}");
    for (method, path) in ROUTES {
        let answer = raw_exchange(addr, &raw_request(method, path, &rebinding_host, None)).await;
        assert_eq!(
            status_of(&answer),
            403,
            "the route {path} answered a request with the rebinding Host header {rebinding_host}: {answer}"
        );
        assert_eq!(
            refusal_code(&answer).as_deref(),
            Some("PERMISSION_DENIED"),
            "the route {path} refused the request without the code PERMISSION_DENIED: {answer}"
        );
    }

    // The control: the same route with the loopback literal is admitted, so
    // the refusal above is caused by the name and not by the route.
    assert_eq!(
        health_status(&daemon).await,
        200,
        "the daemon refuses the loopback host as well"
    );
}

// ---------------------------------------------------------------------------
// TH01 - a hostile Origin header
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_hostile_origin_header_is_refused_on_every_route() {
    let daemon = ShippedDaemon::start()
        .await
        .expect("the shipped daemon starts");
    let port = daemon.port();
    let addr = daemon.addr();

    for (method, path) in ROUTES {
        let request = raw_request(
            method,
            path,
            &format!("127.0.0.1:{port}"),
            Some("https://attacker.example"),
        );
        let answer = raw_exchange(addr, &request).await;
        assert_eq!(
            status_of(&answer),
            403,
            "the route {path} answered a request from the origin https://attacker.example: {answer}"
        );
        assert_eq!(
            refusal_code(&answer).as_deref(),
            Some("PERMISSION_DENIED"),
            "the route {path} refused the request without the code PERMISSION_DENIED: {answer}"
        );
    }

    assert_eq!(
        health_status(&daemon).await,
        200,
        "the daemon stopped after the refused requests"
    );
}

// ---------------------------------------------------------------------------
// TH01 - the control endpoints under a browser origin
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_control_request_with_a_null_origin_can_neither_pair_nor_stop_the_daemon() {
    let daemon = ShippedDaemon::start()
        .await
        .expect("the shipped daemon starts");
    let port = daemon.port();
    let addr = daemon.addr();

    // The pairing endpoint must refuse a browser origin of "null".
    let answer = raw_exchange(
        addr,
        &raw_request(
            "POST",
            "/control/pair",
            &format!("127.0.0.1:{port}"),
            Some("null"),
        ),
    )
    .await;
    let status = status_of(&answer);
    assert_eq!(
        status,
        403,
        "DEFECT: the pairing endpoint answered a request whose Origin is \"null\" with the status {status} and the \
         body {}; the guard in crates/daemon/src/server.rs::origin_is_allowed admits the literal \"null\", and the \
         pair handler in crates/daemon/src/server.rs::pair_handler only checks the TCP peer, so any page that can \
         open a socket to the loopback port mints a pairing secret and pairs a plugin of its own",
        body_of(&answer)
    );
    assert!(
        !body_of(&answer).contains("secret"),
        "the pairing endpoint handed out a secret to a request with the origin null: {}",
        body_of(&answer)
    );

    // The shutdown endpoint must refuse the request as well.
    let answer = raw_exchange(
        addr,
        &raw_request(
            "POST",
            "/control/shutdown",
            &format!("127.0.0.1:{port}"),
            Some("null"),
        ),
    )
    .await;
    let status = status_of(&answer);
    assert_eq!(
        status,
        403,
        "DEFECT: the shutdown endpoint answered a request whose Origin is \"null\" with the status {status}; the \
         guard in crates/daemon/src/server.rs::origin_is_allowed admits the literal \"null\", and the shutdown handler \
         in crates/daemon/src/server.rs::shutdown_handler only checks the TCP peer, so any page that can open a \
         socket to the loopback port stops the daemon"
    );

    assert_eq!(
        health_status(&daemon).await,
        200,
        "the daemon stopped after a request with the origin null"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_control_request_without_an_origin_can_neither_pair_nor_stop_the_daemon() {
    let daemon = ShippedDaemon::start()
        .await
        .expect("the shipped daemon starts");
    let port = daemon.port();
    let addr = daemon.addr();

    // The pairing endpoint must refuse a request that names no origin.
    let answer = raw_exchange(
        addr,
        &raw_request("POST", "/control/pair", &format!("127.0.0.1:{port}"), None),
    )
    .await;
    let status = status_of(&answer);
    assert_eq!(
        status,
        403,
        "DEFECT: the pairing endpoint answered a request without an Origin header with the status {status} and the \
         body {}; the guard in crates/daemon/src/server.rs::origin_is_allowed treats a missing origin as a non \
         browser client, and the pair handler in crates/daemon/src/server.rs::pair_handler only checks the TCP peer, \
         so any process that can open a socket to the loopback port mints a pairing secret",
        body_of(&answer)
    );
    assert!(
        !body_of(&answer).contains("secret"),
        "the pairing endpoint handed out a secret to a request without an origin: {}",
        body_of(&answer)
    );

    // The shutdown endpoint must refuse the request as well.
    let answer = raw_exchange(
        addr,
        &raw_request(
            "POST",
            "/control/shutdown",
            &format!("127.0.0.1:{port}"),
            None,
        ),
    )
    .await;
    let status = status_of(&answer);
    assert_eq!(
        status,
        403,
        "DEFECT: the shutdown endpoint answered a request without an Origin header with the status {status}; the \
         shutdown handler in crates/daemon/src/server.rs::shutdown_handler only checks the TCP peer, so any process \
         that can open a socket to the loopback port stops the daemon"
    );

    assert_eq!(
        health_status(&daemon).await,
        200,
        "the daemon stopped after a request without an origin"
    );
}

// ---------------------------------------------------------------------------
// TH03 - the client credential
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_wrong_client_credential_is_refused_and_leaks_nothing() {
    let daemon = ShippedDaemon::start()
        .await
        .expect("the shipped daemon starts");
    let credential = daemon
        .client_credential()
        .expect("the daemon file holds the credential");
    let corpus = Arc::new(RwLock::new(FixtureCorpus::labelled(DOCUMENT_LABEL, 40)));
    let root = corpus.read().await.root_id().to_string();
    let secret = daemon
        .mint_pairing_secret()
        .await
        .expect("the daemon mints a pairing secret");
    let plugin = FakePlugin::connect(&daemon.bridge_url(), &secret, PLUGIN_ID, corpus)
        .await
        .expect("the plugin pairs");
    let mut client = FakeLocalClient::connect(&daemon.client_url(), &credential, CLIENT_ID)
        .await
        .expect("the honest client connects");
    grant_to_shipped(&plugin, &mut client, &root).await;
    let session_id = plugin.session_id();

    // The hostile client presents a credential that does not belong to this
    // installation and asks for the session list right after it.
    let follow_up = ClientRequest::ListSessions {
        request_id: "hostile-list-sessions".to_string(),
        cursor: None,
    };
    let outcome = client_frames(
        &daemon.client_url(),
        "ftsecret-this-is-not-the-credential",
        CLIENT_ID,
        Some(follow_up),
    )
    .await;

    assert!(
        outcome.refused,
        "the daemon kept a client connection open after a refused credential: {}",
        outcome.end
    );
    for frame in &outcome.frames {
        assert!(
            !frame.contains(&session_id.to_string()),
            "the refusal leaks the session id: {frame}"
        );
        assert!(
            !frame.contains(DOCUMENT_LABEL),
            "the refusal leaks the document label: {frame}"
        );
        assert!(
            !frame.contains(&root),
            "the refusal leaks the root node id: {frame}"
        );
        assert!(
            !frame.contains(&credential),
            "the refusal leaks the client credential: {frame}"
        );
        if let Ok(response) = serde_json::from_str::<ClientResponse>(frame) {
            assert!(
                !matches!(response, ClientResponse::Ok { .. }),
                "the daemon answered an unauthenticated request: {frame}"
            );
        }
    }

    assert_eq!(
        health_status(&daemon).await,
        200,
        "the daemon stopped after the refused credential"
    );
}

// ---------------------------------------------------------------------------
// AS2 - the pairing secret
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_replayed_pairing_secret_is_refused() {
    let daemon = ShippedDaemon::start()
        .await
        .expect("the shipped daemon starts");
    let secret = daemon
        .mint_pairing_secret()
        .await
        .expect("the daemon mints a pairing secret");
    let corpus = Arc::new(RwLock::new(FixtureCorpus::labelled(DOCUMENT_LABEL, 40)));

    let first = FakePlugin::connect(&daemon.bridge_url(), &secret, PLUGIN_ID, corpus.clone())
        .await
        .expect("the first connection pairs");
    let error = FakePlugin::connect(&daemon.bridge_url(), &secret, OTHER_PLUGIN_ID, corpus)
        .await
        .expect_err("the replay is refused");
    match error {
        PluginError::Rejected(rejected) => {
            assert_eq!(
                rejected.code, "AUTH_REQUIRED",
                "the replay is refused with the wrong code: {rejected:?}"
            );
            assert!(
                !rejected.message.contains(&secret),
                "the refusal repeats the secret: {rejected:?}"
            );
        }
        other => panic!("the replay must be refused, got {other}"),
    }

    let status = daemon.handle().status();
    assert_eq!(status.sessions, 1, "the replay opened a second session");
    assert_eq!(
        status.capabilities.len(),
        1,
        "the replay opened a second plugin connection"
    );
    assert_eq!(
        status.capabilities[0].session_id,
        first.session_id().to_string(),
        "the replay took over the session of the first connection"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_expired_pairing_secret_is_refused() {
    // The shipped lifetime of a pairing secret is five minutes. The daemon
    // code that serves the bridge is the shipped one, only the lifetime of
    // the secret is shortened, so the expiry is observed without a five
    // minute sleep inside the test run.
    let ttl = Duration::from_secs(1);
    let daemon = ShortLivedPairingDaemon::start(ttl)
        .await
        .expect("the shipped daemon starts");
    let secret = daemon
        .mint_pairing_secret()
        .await
        .expect("the daemon mints a pairing secret");
    let minted_at = Instant::now();

    // The secret is presented after its lifetime elapsed.
    while minted_at.elapsed() <= ttl + Duration::from_millis(200) {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let corpus = Arc::new(RwLock::new(FixtureCorpus::labelled(DOCUMENT_LABEL, 40)));
    let error = FakePlugin::connect(&daemon.bridge_url(), &secret, PLUGIN_ID, corpus)
        .await
        .expect_err("the expired secret is refused");
    match error {
        PluginError::Rejected(rejected) => {
            assert_eq!(
                rejected.code, "AUTH_EXPIRED",
                "the expired secret is refused with the wrong code: {rejected:?}"
            );
            assert!(
                !rejected.message.contains(&secret),
                "the refusal repeats the secret: {rejected:?}"
            );
        }
        other => panic!("the expired secret must be refused, got {other}"),
    }

    assert_eq!(
        daemon.session_count(),
        0,
        "the expired secret opened a session"
    );
}

// ---------------------------------------------------------------------------
// The two channels must not accept each other's credential
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_pairing_secret_is_refused_on_the_client_endpoint() {
    let daemon = ShippedDaemon::start()
        .await
        .expect("the shipped daemon starts");
    let secret = daemon
        .mint_pairing_secret()
        .await
        .expect("the daemon mints a pairing secret");

    let outcome = client_frames(&daemon.client_url(), &secret, CLIENT_ID, None).await;
    assert!(
        outcome.refused,
        "the client endpoint accepted a pairing secret as the client credential: {}",
        outcome.end
    );
    for frame in &outcome.frames {
        assert!(
            !frame.contains(&secret),
            "the refusal repeats the pairing secret: {frame}"
        );
        if let Ok(response) = serde_json::from_str::<ClientResponse>(frame) {
            assert!(
                !matches!(response, ClientResponse::Ok { .. }),
                "the daemon answered a client that presented a pairing secret: {frame}"
            );
        }
    }

    assert_eq!(
        health_status(&daemon).await,
        200,
        "the daemon stopped after the refused pairing secret"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_client_credential_is_refused_on_the_bridge_endpoint() {
    let daemon = ShippedDaemon::start()
        .await
        .expect("the shipped daemon starts");
    let credential = daemon
        .client_credential()
        .expect("the daemon file holds the credential");
    let corpus = Arc::new(RwLock::new(FixtureCorpus::labelled(DOCUMENT_LABEL, 40)));

    let error = FakePlugin::connect(&daemon.bridge_url(), &credential, PLUGIN_ID, corpus)
        .await
        .expect_err("the client credential is refused on the bridge");
    match error {
        PluginError::Rejected(rejected) => {
            assert_eq!(
                rejected.code, "AUTH_REQUIRED",
                "the client credential is refused with the wrong code: {rejected:?}"
            );
            assert!(
                !rejected.message.contains(&credential),
                "the refusal repeats the client credential: {rejected:?}"
            );
        }
        other => panic!("the client credential must be refused on the bridge, got {other}"),
    }

    assert_eq!(
        daemon.handle().status().sessions,
        0,
        "the client credential opened a session on the bridge"
    );
}

// ---------------------------------------------------------------------------
// TH05 and TH10 - the scope and the session isolation
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_node_outside_the_grant_is_refused_with_out_of_scope() {
    let daemon = ShippedDaemon::start()
        .await
        .expect("the shipped daemon starts");
    let credential = daemon
        .client_credential()
        .expect("the daemon file holds the credential");
    let corpus = Arc::new(RwLock::new(FixtureCorpus::labelled(DOCUMENT_LABEL, 40)));
    let root = corpus.read().await.root_id().to_string();
    let secret = daemon
        .mint_pairing_secret()
        .await
        .expect("the daemon mints a pairing secret");
    let plugin = FakePlugin::connect(&daemon.bridge_url(), &secret, PLUGIN_ID, corpus)
        .await
        .expect("the plugin pairs");
    let mut client = FakeLocalClient::connect(&daemon.client_url(), &credential, CLIENT_ID)
        .await
        .expect("the client connects");
    grant_to_shipped(&plugin, &mut client, &root).await;

    let error = client
        .error(ClientRequest::GetNodes {
            request_id: client.next_request_id(),
            params: serde_json::to_value(params::get_nodes(plugin.session_id(), &[OUTSIDE_NODE]))
                .expect("the parameters serialize"),
        })
        .await
        .expect("the daemon refuses the node");
    assert_eq!(
        error.code,
        ErrorCode::OutOfScope,
        "the node outside the grant is refused with the wrong code: {error:?}"
    );
    assert!(
        !error.message.contains(DOCUMENT_LABEL),
        "the refusal carries design data: {error:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_out_of_scope_read_is_refused_even_when_the_plugin_answers_it() {
    let daemon = ShippedDaemon::start()
        .await
        .expect("the shipped daemon starts");
    let credential = daemon
        .client_credential()
        .expect("the daemon file holds the credential");
    let secret = daemon
        .mint_pairing_secret()
        .await
        .expect("the daemon mints a pairing secret");
    let root = "1:2".to_string();

    // The hostile plugin answers every read with a node the grant does not
    // cover. The daemon has to refuse the read on its own, because either
    // side of the bridge can be buggy or fed manipulated frames.
    let hostile = json!({
        "document_label": "FX-HOSTILE-DOC",
        "page_name": "Page 1",
        "nodes": [{
            "id": OUTSIDE_NODE,
            "node_type": "TEXT",
            "name": "hostile node",
            "text": { "text": "the content of a node outside the grant" },
        }],
    });
    let plugin = tokio::spawn(hostile_plugin(
        daemon.bridge_url(),
        secret.clone(),
        PLUGIN_ID.to_string(),
        root.clone(),
        hostile,
    ));

    let mut client = FakeLocalClient::connect(&daemon.client_url(), &credential, CLIENT_ID)
        .await
        .expect("the client connects");
    let session_id = wait_for_any_grant(&mut client).await;

    let response = client
        .request(ClientRequest::GetNodes {
            request_id: client.next_request_id(),
            params: serde_json::to_value(params::get_nodes(session_id, &[OUTSIDE_NODE]))
                .expect("the parameters serialize"),
        })
        .await
        .expect("the daemon answers the request");
    plugin.abort();

    match response {
        ClientResponse::Error { error, .. } => {
            assert_eq!(
                error.code,
                ErrorCode::OutOfScope,
                "the read outside the grant is refused with the wrong code: {error:?}"
            );
        }
        ClientResponse::Ok { envelope, .. } => panic!(
            "DEFECT: the daemon served the node {OUTSIDE_NODE} of a read that is outside the grant and answered \
             with the content {envelope:?}; crates/daemon/src/client_api.rs::run_job never calls \
             SessionRegistry::require_in_scope, so the daemon forwards every node id to the plugin and serves \
             whatever the plugin answers, while docs/05-security/02-access-control.md requires the membership \
             check on the daemon side as well (E2 and E3)"
        ),
        ClientResponse::Accepted { .. } => {
            panic!("the daemon accepted the read outside the grant in the background")
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_sessions_with_the_same_node_ids_never_exchange_data() {
    let daemon = ShippedDaemon::start()
        .await
        .expect("the shipped daemon starts");
    let credential = daemon
        .client_credential()
        .expect("the daemon file holds the credential");
    // Two documents that share every node id of the first one but carry
    // different content, and one node that only the second one carries.
    let corpus_a = Arc::new(RwLock::new(FixtureCorpus::labelled(DOCUMENT_LABEL, 40)));
    let corpus_b = Arc::new(RwLock::new(FixtureCorpus::labelled(
        OTHER_DOCUMENT_LABEL,
        60,
    )));
    let root = corpus_a.read().await.root_id().to_string();

    let secret_a = daemon
        .mint_pairing_secret()
        .await
        .expect("the daemon mints a pairing secret");
    let secret_b = daemon
        .mint_pairing_secret()
        .await
        .expect("the daemon mints a pairing secret");
    let plugin_a = FakePlugin::connect(&daemon.bridge_url(), &secret_a, PLUGIN_ID, corpus_a)
        .await
        .expect("the first plugin pairs");
    let plugin_b = FakePlugin::connect(&daemon.bridge_url(), &secret_b, OTHER_PLUGIN_ID, corpus_b)
        .await
        .expect("the second plugin pairs");
    assert_ne!(
        plugin_a.session_id(),
        plugin_b.session_id(),
        "the two plugins share one session"
    );

    let mut client = FakeLocalClient::connect(&daemon.client_url(), &credential, CLIENT_ID)
        .await
        .expect("the client connects");
    grant_to_shipped(&plugin_a, &mut client, &root).await;
    grant_to_shipped(&plugin_b, &mut client, &root).await;

    let read_a = client
        .list_nodes(params::list_nodes(plugin_a.session_id(), &root))
        .await
        .expect("the first session reads");
    let read_b = client
        .list_nodes(params::list_nodes(plugin_b.session_id(), &root))
        .await
        .expect("the second session reads");

    assert_eq!(
        read_a.source.session_id,
        Some(plugin_a.session_id()),
        "the answer of the first session names another session"
    );
    assert_eq!(
        read_b.source.session_id,
        Some(plugin_b.session_id()),
        "the answer of the second session names another session"
    );

    let text_a = node_text(&read_a, "1:5").expect("the first document carries the node");
    let text_b = node_text(&read_b, "1:5").expect("the second document carries the node");
    assert!(
        text_a.contains(DOCUMENT_LABEL),
        "the first session served the content of the second document: {text_a}"
    );
    assert!(
        text_b.contains(OTHER_DOCUMENT_LABEL),
        "the second session served the content of the first document: {text_b}"
    );
    assert_ne!(
        text_a, text_b,
        "the two sessions served the same content for the same node id"
    );
    assert!(
        !text_a.contains(OTHER_DOCUMENT_LABEL),
        "the first session leaked the content of the second document: {text_a}"
    );
    assert!(
        !text_b.contains(DOCUMENT_LABEL),
        "the second session leaked the content of the first document: {text_b}"
    );

    // A node id that only the document of the other session carries is
    // refused for this session.
    let error = client
        .error(ClientRequest::GetNodes {
            request_id: client.next_request_id(),
            params: serde_json::to_value(params::get_nodes(plugin_a.session_id(), &["1:50"]))
                .expect("the parameters serialize"),
        })
        .await
        .expect("the daemon refuses the node of the other document");
    assert_eq!(
        error.code,
        ErrorCode::OutOfScope,
        "the node of the other document is refused with the wrong code: {error:?}"
    );
    assert!(
        !error.message.contains(OTHER_DOCUMENT_LABEL),
        "the refusal carries the content of the other document: {error:?}"
    );
}

// ---------------------------------------------------------------------------
// TH06 - hostile payloads
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_oversized_bridge_frame_is_refused_without_a_crash() {
    let daemon = ShippedDaemon::start()
        .await
        .expect("the shipped daemon starts");
    let mut socket = raw_socket(&daemon.bridge_url()).await;

    let blob = "x".repeat(MAX_FRAME_BYTES + 4096);
    let frame = json!({
        "protocol_version": PROTOCOL_VERSION,
        "message_type": "hello_auth",
        "request_id": "hostile-oversized",
        "payload": { "blob": blob },
    });
    socket
        .send(Message::Text(frame.to_string().into()))
        .await
        .expect("the frame is written");

    let outcome = collect(&mut socket, FRAME_TIMEOUT).await;
    assert!(
        matches!(outcome, Answer::Closed(_)),
        "the daemon kept an oversized frame open: {outcome:?}"
    );
    assert!(
        !authenticated(outcome.frames()),
        "the daemon authenticated an oversized frame: {:?}",
        outcome.frames()
    );
    assert_eq!(
        health_status(&daemon).await,
        200,
        "the daemon died on the oversized frame"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_deeply_nested_json_payload_is_refused_without_a_crash() {
    let daemon = ShippedDaemon::start()
        .await
        .expect("the shipped daemon starts");
    let mut socket = raw_socket(&daemon.bridge_url()).await;

    let depth = 256;
    let nested = format!("{}{}{}", "[".repeat(depth), "1", "]".repeat(depth));
    let frame = format!(
        "{{\"protocol_version\":\"{PROTOCOL_VERSION}\",\"message_type\":\"hello_auth\",\
         \"request_id\":\"hostile-nested\",\"payload\":{nested}}}"
    );
    socket
        .send(Message::Text(frame.into()))
        .await
        .expect("the frame is written");

    let outcome = collect(&mut socket, FRAME_TIMEOUT).await;
    assert!(
        matches!(outcome, Answer::Closed(_)),
        "the daemon kept a deeply nested payload open: {outcome:?}"
    );
    assert!(
        !authenticated(outcome.frames()),
        "the daemon authenticated a deeply nested payload: {:?}",
        outcome.frames()
    );
    assert_eq!(
        health_status(&daemon).await,
        200,
        "the daemon died on the deeply nested payload"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_frame_with_a_wrong_protocol_version_is_refused_without_a_crash() {
    let daemon = ShippedDaemon::start()
        .await
        .expect("the shipped daemon starts");

    // The envelope carries a version the daemon does not speak.
    let mut socket = raw_socket(&daemon.bridge_url()).await;
    let frame = json!({
        "protocol_version": "0.9",
        "message_type": "hello_auth",
        "request_id": "hostile-version",
        "payload": { "secret": "irrelevant" },
    });
    socket
        .send(Message::Text(frame.to_string().into()))
        .await
        .expect("the frame is written");
    let outcome = collect(&mut socket, FRAME_TIMEOUT).await;
    assert!(
        matches!(outcome, Answer::Closed(_)),
        "the daemon kept a frame with a wrong protocol version open: {outcome:?}"
    );
    assert!(
        !authenticated(outcome.frames()),
        "the daemon authenticated a frame with a wrong protocol version: {:?}",
        outcome.frames()
    );

    // The authentication carries a version the daemon does not speak. The
    // daemon answers with a structured refusal instead of closing silently.
    let mut socket = raw_socket(&daemon.bridge_url()).await;
    let request = AuthRequest {
        secret: "irrelevant".to_string(),
        client_id: PLUGIN_ID.to_string(),
        protocol_version: "0.9".to_string(),
    };
    let frame = BridgeFrame::new(
        MessageType::HelloAuth,
        "hello-version",
        serde_json::to_value(request).expect("the authentication serializes"),
    );
    socket
        .send(Message::Text(
            serde_json::to_string(&frame)
                .expect("the frame serializes")
                .into(),
        ))
        .await
        .expect("the frame is written");
    let outcome = collect(&mut socket, FRAME_TIMEOUT).await;
    assert!(
        outcome
            .frames()
            .iter()
            .any(|frame| frame.contains("PROTOCOL_MISMATCH")),
        "the daemon did not refuse the wrong protocol version with PROTOCOL_MISMATCH: {outcome:?}"
    );
    assert!(
        !authenticated(outcome.frames()),
        "the daemon authenticated a wrong protocol version: {:?}",
        outcome.frames()
    );

    assert_eq!(
        health_status(&daemon).await,
        200,
        "the daemon died on the wrong protocol version"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_frame_with_an_unknown_message_type_is_refused_without_a_crash() {
    let daemon = ShippedDaemon::start()
        .await
        .expect("the shipped daemon starts");
    let mut socket = raw_socket(&daemon.bridge_url()).await;

    let frame = json!({
        "protocol_version": PROTOCOL_VERSION,
        "message_type": "hostile_unknown_type",
        "request_id": "hostile-unknown",
        "payload": {},
    });
    socket
        .send(Message::Text(frame.to_string().into()))
        .await
        .expect("the frame is written");

    let outcome = collect(&mut socket, FRAME_TIMEOUT).await;
    assert!(
        matches!(outcome, Answer::Closed(_)),
        "the daemon kept a frame with an unknown message type open: {outcome:?}"
    );
    assert!(
        !authenticated(outcome.frames()),
        "the daemon authenticated a frame with an unknown message type: {:?}",
        outcome.frames()
    );
    assert_eq!(
        health_status(&daemon).await,
        200,
        "the daemon died on the unknown message type"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_chunk_that_lies_about_its_total_size_is_refused_without_a_crash() {
    let daemon = ShippedDaemon::start()
        .await
        .expect("the shipped daemon starts");
    let secret = daemon
        .mint_pairing_secret()
        .await
        .expect("the daemon mints a pairing secret");

    // The chunk arrives on an authenticated connection, so the daemon really
    // reads it instead of refusing the connection for the missing handshake.
    let mut socket = raw_bridge_session(&daemon.bridge_url(), &secret, PLUGIN_ID).await;
    let frame = BridgeFrame::new(
        MessageType::Chunk,
        "chunk-adversarial",
        json!({
            "transfer_id": "transfer-adversarial",
            "sequence": 0,
            "bytes": "cGF5bG9hZA==",
            "total_bytes": u64::MAX,
        }),
    );
    socket
        .send(Message::Text(
            serde_json::to_string(&frame)
                .expect("the frame serializes")
                .into(),
        ))
        .await
        .expect("the frame is written");

    let outcome = collect(&mut socket, FRAME_TIMEOUT).await;
    let refused = match &outcome {
        Answer::Closed(_) => true,
        Answer::Open(frames) => frames
            .iter()
            .any(|frame| frame.contains("PAYLOAD_TOO_LARGE")),
    };
    assert!(
        refused,
        "DEFECT: the daemon accepted a chunk frame that claims {} bytes, which is above the transfer budget of {} \
         bytes; crates/daemon/src/bridge.rs::handle_incoming logs the chunk and drops payload.total_bytes without \
         comparing it against MAX_TRANSFER_BYTES, so a hostile peer can announce an unbounded transfer",
        u64::MAX,
        MAX_TRANSFER_BYTES
    );
    assert_eq!(
        health_status(&daemon).await,
        200,
        "the daemon died on the hostile chunk"
    );
}

// ---------------------------------------------------------------------------
// TH06 - resource pressure
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_saturated_queue_answers_busy_with_a_retry_delay() {
    let daemon = ShippedDaemon::start()
        .await
        .expect("the shipped daemon starts");
    let credential = daemon
        .client_credential()
        .expect("the daemon file holds the credential");
    let corpus = Arc::new(RwLock::new(FixtureCorpus::labelled(DOCUMENT_LABEL, 40)));
    let root = corpus.read().await.root_id().to_string();
    let secret = daemon
        .mint_pairing_secret()
        .await
        .expect("the daemon mints a pairing secret");
    let plugin = FakePlugin::connect(&daemon.bridge_url(), &secret, PLUGIN_ID, corpus)
        .await
        .expect("the plugin pairs");
    let mut client = FakeLocalClient::connect(&daemon.client_url(), &credential, CLIENT_ID)
        .await
        .expect("the client connects");
    grant_to_shipped(&plugin, &mut client, &root).await;

    // The plugin stops answering, so every request of the flood waits in the
    // queue of the scheduler and the queue reaches its cap.
    plugin
        .set_behavior(figtether_e2e::fake_plugin::PluginBehavior::Slow {
            delay: Duration::from_secs(30),
        })
        .await;

    let flood = 24usize;
    let mut sockets = Vec::new();
    for _ in 0..flood {
        sockets.push(raw_client(&daemon.client_url(), &credential, CLIENT_ID).await);
    }

    let (sender, mut receiver) = tokio::sync::mpsc::channel::<String>(flood * 2);
    let session_id = plugin.session_id();
    for mut socket in sockets {
        let sender = sender.clone();
        let root = root.clone();
        tokio::spawn(async move {
            let request = ClientRequest::ListNodes {
                request_id: "flood-request".to_string(),
                params: serde_json::to_value(params::list_nodes(session_id, &root))
                    .expect("the parameters serialize"),
            };
            let encoded = serde_json::to_string(&request).expect("the request serializes");
            if socket.send(Message::Text(encoded.into())).await.is_err() {
                return;
            }
            while let Some(Ok(message)) = socket.next().await {
                if let Message::Text(text) = message {
                    if sender.send(text.to_string()).await.is_err() {
                        break;
                    }
                }
            }
        });
    }
    drop(sender);

    let mut busy = 0usize;
    loop {
        let text = match tokio::time::timeout(Duration::from_secs(10), receiver.recv()).await {
            Ok(Some(text)) => text,
            _ => break,
        };
        let response: ClientResponse =
            serde_json::from_str(&text).expect("the daemon answers with a client response");
        if let ClientResponse::Error { error, .. } = response {
            if error.code == ErrorCode::Busy {
                assert_eq!(
                    error.retry_after_ms,
                    Some(250),
                    "the busy answer carries no retry delay: {error:?}"
                );
                busy += 1;
            }
        }
    }

    assert!(
        busy > 0,
        "DEFECT: the daemon answered none of the {flood} concurrent requests with BUSY, so the queue of the \
         scheduler has no cap on the number of queued requests"
    );
    assert!(
        daemon.handle().status().busy_rejections > 0,
        "the daemon counted no busy rejection"
    );
    assert_eq!(
        health_status(&daemon).await,
        200,
        "the daemon stopped answering under the flood"
    );
}

// ---------------------------------------------------------------------------
// The runtime directory and the binding of the daemon
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_daemon_binds_loopback_only_and_writes_only_inside_its_runtime_directory() {
    let daemon = ShippedDaemon::start()
        .await
        .expect("the shipped daemon starts");
    let credential = daemon
        .client_credential()
        .expect("the daemon file holds the credential");
    let secret = daemon
        .mint_pairing_secret()
        .await
        .expect("the daemon mints a pairing secret");

    // The daemon listens on the loopback interface only.
    let status = daemon.handle().status();
    assert!(
        status.bind_address.starts_with("127.0.0.1"),
        "the daemon reports the bind address {}",
        status.bind_address
    );
    if let Some(ip) = figtether_e2e::harness::non_loopback_ipv4() {
        let addr = SocketAddr::from((ip, daemon.port()));
        let outcome = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(addr)).await;
        match outcome {
            Ok(Ok(_stream)) => {
                panic!("the daemon answered on the routable address {addr}")
            }
            Ok(Err(err)) => assert_eq!(
                err.kind(),
                std::io::ErrorKind::ConnectionRefused,
                "the connection to {addr} failed with an unexpected error"
            ),
            Err(_elapsed) => panic!("the connection to {addr} never completed"),
        }
    }

    // The daemon holds its descriptor, empty OS-lock file and diagnostic log.
    let runtime_dir = daemon.runtime_dir();
    let mut entries: Vec<String> = std::fs::read_dir(runtime_dir)
        .expect("the runtime directory is readable")
        .filter_map(|entry| entry.ok())
        .map(|entry| {
            entry
                .file_name()
                .to_str()
                .map(str::to_string)
                .unwrap_or_default()
        })
        .collect();
    entries.sort();
    assert_eq!(
        entries,
        vec![
            "daemon.json".to_string(),
            "daemon.lock".to_string(),
            "figtether.log".to_string()
        ],
        "the runtime directory holds an unexpected file"
    );
    assert_eq!(
        std::fs::metadata(runtime_dir.join("daemon.lock"))
            .expect("lock metadata")
            .len(),
        0,
        "the lock file must not contain credentials or design data"
    );

    // The daemon wrote nothing next to its runtime directory.
    let parent = runtime_dir
        .parent()
        .expect("the runtime directory has a parent");
    let mut siblings: Vec<String> = std::fs::read_dir(parent)
        .expect("the parent of the runtime directory is readable")
        .filter_map(|entry| entry.ok())
        .map(|entry| {
            entry
                .file_name()
                .to_str()
                .map(str::to_string)
                .unwrap_or_default()
        })
        .collect();
    siblings.sort();
    assert_eq!(
        siblings,
        vec!["figtether".to_string()],
        "the daemon wrote a file next to its runtime directory"
    );

    // The credential lives in the descriptor and in no other file.
    let descriptor = daemon
        .daemon_file_value()
        .expect("the daemon file is readable");
    assert_eq!(
        descriptor["client_credential"].as_str(),
        Some(credential.as_str()),
        "the descriptor does not hold the credential of the daemon"
    );
    for entry in std::fs::read_dir(runtime_dir)
        .expect("the runtime directory is readable")
        .filter_map(|entry| entry.ok())
    {
        let name = entry
            .file_name()
            .to_str()
            .map(str::to_string)
            .unwrap_or_default();
        let body = std::fs::read_to_string(entry.path()).unwrap_or_default();
        if name == "daemon.json" {
            continue;
        }
        assert!(
            !body.contains(&credential),
            "the file {name} of the runtime directory holds the client credential"
        );
        assert!(
            !body.contains(&secret),
            "the file {name} of the runtime directory holds the pairing secret"
        );
    }
}
