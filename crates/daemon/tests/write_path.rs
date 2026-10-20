//! Integration tests of the write path of the daemon.
//!
//! The tests drive the real daemon over real loopback sockets: a plugin
//! pairs, the user grants a scope with the write mode, a local client
//! previews a plan, the user approves the preview in the plugin UI, and the
//! client applies the plan with the approval token.
//!
//! They prove the rules of the write path:
//!
//! - a session without a write grant refuses every write;
//! - the preview reaches the plugin and carries an approval id;
//! - the decision of the user yields a token, and only the client that asked
//!   for the preview learns it;
//! - the apply reaches the plugin with the token and returns the result.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use figtether_daemon::{run, DaemonConfig, DaemonHandle};
use figtether_protocol::client::{ClientAuth, ClientRequest};
use figtether_protocol::write::ApprovalDecisionPayload;
use figtether_protocol::{
    AuthAccepted, AuthRequest, BridgeFrame, MessageType, RequestPayload, ResultPayload,
    PROTOCOL_VERSION,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

/// The websocket stream of the daemon.
type Stream = WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>;

/// The writing half of a connection to the daemon.
type Sink = futures_util::stream::SplitSink<Stream, WsMessage>;

/// The client id the plugin UI presents.
const PLUGIN_LABEL: &str = "figma-plugin-ui";

/// The client id the local client presents.
const CLIENT_LABEL: &str = "figtether-cli";

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
    let temp = tempfile::tempdir().expect("temp dir");
    let mut last_error = None;
    for _ in 0..5 {
        let config = DaemonConfig::new()
            .with_port(free_port())
            .with_limits(fast_limits())
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
async fn open_client(port: u16, presented: &str, label: &str) -> Option<Socket> {
    let mut socket = Socket::open(port, "/client").await;
    let auth = ClientAuth {
        credential: presented.to_string(),
        client_id: label.to_string(),
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

/// A fake plugin that serves the daemon and answers the write requests.
struct FakePlugin {
    /// The session the daemon opened.
    session_id: figtether_model::ids::SessionId,
    /// The frames the plugin writes to the bridge.
    outbox: tokio::sync::mpsc::Sender<String>,
    /// The operations the plugin received.
    received: Arc<Mutex<Vec<String>>>,
    /// The parameters the plugin received.
    params: Arc<Mutex<Vec<serde_json::Value>>>,
}

impl FakePlugin {
    /// Pair a plugin with the daemon.
    async fn connect(port: u16, secret: &str) -> Self {
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
        let params = Arc::new(Mutex::new(Vec::new()));
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
        let task_params = params.clone();
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
                task_received
                    .lock()
                    .expect("lock")
                    .push(payload.op.as_str().to_string());
                task_params
                    .lock()
                    .expect("lock")
                    .push(payload.params.clone());
                // The plugin resolves the plan without mutating the
                // document, and applies it when the plan is approved.
                let data = match payload.op.as_str() {
                    "preview_write" => json!({
                        "preview": {
                            "plan_id": "plan_1",
                            "operations_count": 1,
                            "changed": ["1:2"],
                            "blocked": false,
                        }
                    }),
                    _ => json!({
                        "result": {
                            "plan_id": "plan_1",
                            "changed": ["1:2"],
                            "undo_committed": true,
                        }
                    }),
                };
                let reply = BridgeFrame::new(
                    MessageType::Result,
                    frame.request_id.clone(),
                    serde_json::to_value(ResultPayload { data }).expect("encodes"),
                );
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
            params,
        }
    }

    /// Grant a scope to the session.
    async fn grant(&self, roots: &[&str], mode: &str) {
        self.grant_with_session_mode(roots, mode, "live").await;
    }

    /// Grant a scope to the session with an explicit session mode.
    ///
    /// The session mode is the pause extension of the grant update: a
    /// session in the mode `paused` serves nothing.
    async fn grant_with_session_mode(&self, roots: &[&str], mode: &str, session_mode: &str) {
        let payload = json!({
            "root_ids": roots.iter().map(|root| root.to_string()).collect::<Vec<_>>(),
            "grant_epoch": 0,
            "mode": mode,
            "session_mode": session_mode,
        });
        let frame = BridgeFrame::new(MessageType::GrantUpdate, "grant", payload);
        let text = serde_json::to_string(&frame).expect("encodes");
        self.outbox.send(text).await.expect("the frame is written");
    }

    /// Report the decision of the user about a previewed plan.
    async fn decide(&self, approval_id: &str, plan_hash: &str, approved: bool) {
        let payload = ApprovalDecisionPayload {
            approval_id: approval_id.to_string(),
            plan_hash: plan_hash.to_string(),
            approved,
            reason: None,
        };
        let frame = BridgeFrame::new(
            MessageType::Approval,
            "approval",
            serde_json::to_value(&payload).expect("encodes"),
        );
        let text = serde_json::to_string(&frame).expect("encodes");
        self.outbox.send(text).await.expect("the frame is written");
    }

    /// The operations the plugin received.
    fn operations(&self) -> Vec<String> {
        self.received.lock().expect("lock").clone()
    }

    /// The parameters of the requests the plugin received.
    fn parameters(&self) -> Vec<Value> {
        self.params.lock().expect("lock").clone()
    }
}

/// The parameters of a write request.
fn write_params(session_id: figtether_model::ids::SessionId) -> Value {
    json!({
        "plan": {
            "plan_id": "plan_1",
            "session_id": session_id.to_string(),
            "operations": [
                { "kind": "set_text", "node_id": "1:2", "text": "hello" }
            ],
        }
    })
}

/// Wait until the daemon reports a state for the session.
async fn wait_for_state(
    client: &mut Socket,
    session_id: &figtether_model::ids::SessionId,
    state: &str,
) -> Value {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let answer = request(
            client,
            ClientRequest::ListSessions {
                request_id: "req_list".to_string(),
                cursor: None,
            },
        )
        .await;
        let sessions = answer["envelope"]["data"]["sessions"]
            .as_array()
            .cloned()
            .unwrap_or_default();
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

/// Wait until the daemon knows the decision of the user.
///
/// The decision travels over the socket of the plugin, so the daemon may
/// learn it a moment after the plugin sent it.
async fn wait_for_approval(client: &mut Socket, approval_id: &str) -> Value {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let status = request(
            client,
            ClientRequest::GetWriteApproval {
                request_id: "req_status".to_string(),
                params: json!({ "approval_id": approval_id }),
            },
        )
        .await;
        let state = status["envelope"]["data"]["status"].as_str();
        if state.is_some_and(|state| state != "pending") {
            return status;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the decision of the user never arrived"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_session_without_a_write_grant_refuses_every_write() {
    let (handle, temp) = start_daemon().await;
    let port = handle.port();

    let secret = mint_secret(port);
    let plugin = FakePlugin::connect(port, &secret).await;
    let mut client = open_client(port, &credential(&temp), CLIENT_LABEL)
        .await
        .expect("the daemon accepts the client");

    // The user grants a read scope only.
    plugin.grant(&["1:2"], "read").await;
    wait_for_state(&mut client, &plugin.session_id, "ready").await;

    let answer = request(
        &mut client,
        ClientRequest::PreviewWrite {
            request_id: "req_preview".to_string(),
            params: write_params(plugin.session_id),
        },
    )
    .await;
    assert_eq!(answer["type"], "error", "{answer}");
    assert_eq!(answer["error"]["code"], "WRITE_NOT_GRANTED", "{answer}");
    assert!(
        plugin.operations().is_empty(),
        "the plugin was never contacted"
    );

    handle.shutdown();
    handle.wait_stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_approved_plan_is_applied_with_the_token_of_the_approval() {
    let (handle, temp) = start_daemon().await;
    let port = handle.port();

    let secret = mint_secret(port);
    let plugin = FakePlugin::connect(port, &secret).await;
    let mut client = open_client(port, &credential(&temp), CLIENT_LABEL)
        .await
        .expect("the daemon accepts the client");

    // The user grants a scope with the write mode.
    plugin.grant(&["1:2"], "write").await;
    wait_for_state(&mut client, &plugin.session_id, "ready").await;

    // The client previews the plan.
    let answer = request(
        &mut client,
        ClientRequest::PreviewWrite {
            request_id: "req_preview".to_string(),
            params: write_params(plugin.session_id),
        },
    )
    .await;
    assert_eq!(answer["type"], "ok", "{answer}");
    let data = &answer["envelope"]["data"];
    let approval_id = data["approval_id"].as_str().expect("an approval id");
    let plan_hash = data["plan_hash"].as_str().expect("a plan hash");
    assert_eq!(data["preview"]["operations_count"], 1, "{answer}");
    assert_eq!(
        plugin.operations(),
        vec!["preview_write".to_string()],
        "the preview reached the plugin"
    );

    // The user approves the preview in the plugin UI.
    plugin.decide(approval_id, plan_hash, true).await;

    // The client reads the status of the approval.
    let status = wait_for_approval(&mut client, approval_id).await;
    assert_eq!(status["type"], "ok", "{status}");
    assert_eq!(status["envelope"]["data"]["status"], "approved", "{status}");
    let token = status["envelope"]["data"]["approval_token"]
        .as_str()
        .expect("a token")
        .to_string();
    assert!(token.starts_with("w:"), "{token}");

    // Another client learns nothing about the approval.
    let mut other = open_client(port, &credential(&temp), "another-client")
        .await
        .expect("the daemon accepts the client");
    let refused = request(
        &mut other,
        ClientRequest::GetWriteApproval {
            request_id: "req_status".to_string(),
            params: json!({ "approval_id": approval_id }),
        },
    )
    .await;
    assert_eq!(refused["type"], "error", "{refused}");
    assert_eq!(refused["error"]["code"], "PERMISSION_DENIED", "{refused}");

    // The client applies the plan with the token.
    let mut params = write_params(plugin.session_id);
    params["approval_token"] = Value::String(token);
    let answer = request(
        &mut client,
        ClientRequest::ApplyWrite {
            request_id: "req_apply".to_string(),
            params,
        },
    )
    .await;
    assert_eq!(answer["type"], "ok", "{answer}");
    assert_eq!(
        answer["envelope"]["data"]["result"]["undo_committed"], true,
        "{answer}"
    );
    assert_eq!(
        plugin.operations(),
        vec!["preview_write".to_string(), "apply_write".to_string()],
        "the apply reached the plugin"
    );

    handle.shutdown();
    handle.wait_stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_write_without_an_approval_token_never_reaches_the_plugin() {
    let (handle, temp) = start_daemon().await;
    let port = handle.port();

    let secret = mint_secret(port);
    let plugin = FakePlugin::connect(port, &secret).await;
    let mut client = open_client(port, &credential(&temp), CLIENT_LABEL)
        .await
        .expect("the daemon accepts the client");

    plugin.grant(&["1:2"], "write").await;
    wait_for_state(&mut client, &plugin.session_id, "ready").await;

    let answer = request(
        &mut client,
        ClientRequest::ApplyWrite {
            request_id: "req_apply".to_string(),
            params: write_params(plugin.session_id),
        },
    )
    .await;
    assert_eq!(answer["type"], "error", "{answer}");
    assert_eq!(answer["error"]["code"], "APPROVAL_REQUIRED", "{answer}");
    assert!(
        plugin.operations().is_empty(),
        "the plugin was never contacted"
    );

    handle.shutdown();
    handle.wait_stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_plan_outside_the_granted_scope_is_refused() {
    let (handle, temp) = start_daemon().await;
    let port = handle.port();

    let secret = mint_secret(port);
    let plugin = FakePlugin::connect(port, &secret).await;
    let mut client = open_client(port, &credential(&temp), CLIENT_LABEL)
        .await
        .expect("the daemon accepts the client");

    plugin.grant(&["1:2"], "write").await;
    wait_for_state(&mut client, &plugin.session_id, "ready").await;

    let params = json!({
        "plan": {
            "plan_id": "plan_1",
            "session_id": plugin.session_id.to_string(),
            "operations": [
                { "kind": "set_text", "node_id": "9:9", "text": "hello" }
            ],
        }
    });
    let answer = request(
        &mut client,
        ClientRequest::PreviewWrite {
            request_id: "req_preview".to_string(),
            params,
        },
    )
    .await;
    assert_eq!(answer["type"], "error", "{answer}");
    assert_eq!(answer["error"]["code"], "OUT_OF_SCOPE", "{answer}");

    handle.shutdown();
    handle.wait_stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_preview_carries_the_approval_id_and_the_plan_hash() {
    let (handle, temp) = start_daemon().await;
    let port = handle.port();

    let secret = mint_secret(port);
    let plugin = FakePlugin::connect(port, &secret).await;
    let mut client = open_client(port, &credential(&temp), CLIENT_LABEL)
        .await
        .expect("the daemon accepts the client");

    plugin.grant(&["1:2"], "write").await;
    wait_for_state(&mut client, &plugin.session_id, "ready").await;

    let answer = request(
        &mut client,
        ClientRequest::PreviewWrite {
            request_id: "req_preview".to_string(),
            params: write_params(plugin.session_id),
        },
    )
    .await;
    assert_eq!(answer["type"], "ok", "{answer}");
    let approval_id = answer["envelope"]["data"]["approval_id"]
        .as_str()
        .expect("an approval id")
        .to_string();
    let plan_hash = answer["envelope"]["data"]["plan_hash"]
        .as_str()
        .expect("a plan hash")
        .to_string();

    // The plan travels with the identity of the approval and the hash the
    // daemon computed, so the plugin can bind the decision of the user to
    // exactly this plan.
    let parameters = plugin.parameters();
    assert_eq!(parameters.len(), 1, "{parameters:?}");
    assert_eq!(parameters[0]["approval_id"], approval_id, "{parameters:?}");
    assert_eq!(parameters[0]["plan_hash"], plan_hash, "{parameters:?}");
    assert_eq!(parameters[0]["plan"]["plan_id"], "plan_1", "{parameters:?}");

    handle.shutdown();
    handle.wait_stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_session_mode_pauses_and_resumes_the_session() {
    let (handle, temp) = start_daemon().await;
    let port = handle.port();

    let secret = mint_secret(port);
    let plugin = FakePlugin::connect(port, &secret).await;
    let mut client = open_client(port, &credential(&temp), CLIENT_LABEL)
        .await
        .expect("the daemon accepts the client");

    // The user pauses the session in the plugin UI.
    plugin
        .grant_with_session_mode(&["1:2"], "write", "paused")
        .await;
    wait_for_state(&mut client, &plugin.session_id, "paused").await;

    let answer = request(
        &mut client,
        ClientRequest::PreviewWrite {
            request_id: "req_preview".to_string(),
            params: write_params(plugin.session_id),
        },
    )
    .await;
    assert_eq!(answer["type"], "error", "{answer}");
    assert_eq!(answer["error"]["code"], "PAUSED", "{answer}");

    // The user resumes the session.
    plugin
        .grant_with_session_mode(&["1:2"], "write", "live")
        .await;
    wait_for_state(&mut client, &plugin.session_id, "ready").await;

    let answer = request(
        &mut client,
        ClientRequest::PreviewWrite {
            request_id: "req_preview".to_string(),
            params: write_params(plugin.session_id),
        },
    )
    .await;
    assert_eq!(answer["type"], "ok", "{answer}");

    handle.shutdown();
    handle.wait_stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_client_reads_the_audit_records_of_its_own_installation() {
    let (handle, temp) = start_daemon().await;
    let port = handle.port();

    let secret = mint_secret(port);
    let plugin = FakePlugin::connect(port, &secret).await;
    let mut client = open_client(port, &credential(&temp), CLIENT_LABEL)
        .await
        .expect("the daemon accepts the client");

    plugin.grant(&["1:2"], "write").await;
    wait_for_state(&mut client, &plugin.session_id, "ready").await;

    // The client previews the plan and the user approves it.
    let answer = request(
        &mut client,
        ClientRequest::PreviewWrite {
            request_id: "req_preview".to_string(),
            params: write_params(plugin.session_id),
        },
    )
    .await;
    assert_eq!(answer["type"], "ok", "{answer}");
    let approval_id = answer["envelope"]["data"]["approval_id"]
        .as_str()
        .expect("an approval id")
        .to_string();
    let plan_hash = answer["envelope"]["data"]["plan_hash"]
        .as_str()
        .expect("a plan hash")
        .to_string();
    plugin.decide(&approval_id, &plan_hash, true).await;
    let status = wait_for_approval(&mut client, &approval_id).await;
    let token = status["envelope"]["data"]["approval_token"]
        .as_str()
        .expect("a token")
        .to_string();

    let mut params = write_params(plugin.session_id);
    params["approval_token"] = Value::String(token);
    let answer = request(
        &mut client,
        ClientRequest::ApplyWrite {
            request_id: "req_apply".to_string(),
            params,
        },
    )
    .await;
    assert_eq!(answer["type"], "ok", "{answer}");

    // The audit trail of the daemon is observable by the client. The record
    // holds identifiers, counts, and hashes, never design content.
    let answer = request(
        &mut client,
        ClientRequest::Status {
            request_id: "req_status".to_string(),
        },
    )
    .await;
    assert_eq!(answer["type"], "ok", "{answer}");
    let audit = answer["envelope"]["data"]["write_audit"]
        .as_array()
        .expect("the status carries the audit trail")
        .clone();
    assert_eq!(audit.len(), 1, "{audit:?}");
    assert_eq!(audit[0]["plan_id"], "plan_1", "{audit:?}");
    assert_eq!(audit[0]["approval_id"], approval_id, "{audit:?}");
    assert_eq!(audit[0]["plan_hash"], plan_hash, "{audit:?}");
    assert_eq!(audit[0]["operations_count"], 1, "{audit:?}");
    assert_eq!(audit[0]["changed"], 1, "{audit:?}");
    assert_eq!(audit[0]["created"], 0, "{audit:?}");
    let rendered = audit[0].to_string();
    assert!(!rendered.contains("hello"), "{rendered}");
    assert!(!rendered.contains("set_text"), "{rendered}");

    handle.shutdown();
    handle.wait_stopped().await;
}
