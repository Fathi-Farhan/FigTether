//! The fake plugin client.
//!
//! The fake plugin is an in-process client that behaves like the TypeScript
//! plugin inside Figma Desktop: it dials the daemon bridge endpoint, spends a
//! pairing secret in the first frame, reports its capabilities, applies the
//! granted scope and answers read requests from a fixture corpus.
//!
//! The behaviours below cover the disturbances the acceptance catalog needs:
//!
//! - [`PluginBehavior::Slow`] answers late, which exercises the deadline and
//!   the cancellation paths;
//! - [`PluginBehavior::Disconnect`] closes the connection, which exercises
//!   offline detection and reconnect;
//! - [`PluginBehavior::StaleEpoch`] answers with the epoch of an older
//!   connection, which exercises the stale frame rule;
//! - [`PluginBehavior::DocumentChanged`] answers with `DOCUMENT_CHANGED`,
//!   which exercises the edit during a read rule;
//! - [`PluginBehavior::Oversized`] answers with a payload larger than the
//!   frame budget;
//! - [`PluginBehavior::Chunked`] answers an export as a chunked transfer,
//!   which exercises a revoke during a transfer.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use figtether_model::ids::SessionId;
use figtether_protocol::plugin::PluginNode;
use figtether_protocol::{
    AuthAccepted, AuthRejected, AuthRequest, BridgeFrame, BridgeOp, CapabilitiesPayload,
    ChunkPayload, ErrorPayload, GrantUpdatePayload, MessageType, RequestPayload, ResultPayload,
    MAX_FRAME_BYTES, PROTOCOL_VERSION,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, RwLock};
use tokio_tungstenite::client_async;
use tokio_tungstenite::tungstenite::http::Request as HttpRequest;
use tokio_tungstenite::tungstenite::http::Uri as HttpUri;
use tokio_tungstenite::tungstenite::Message as ClientMessage;

use crate::fixtures::FixtureCorpus;

/// Failure of the fake plugin.
#[derive(Debug)]
pub enum PluginError {
    /// The connection could not be established.
    Connection(String),
    /// The daemon refused the pairing secret.
    Rejected(AuthRejected),
    /// The daemon spoke something the plugin does not understand.
    Protocol(String),
    /// The daemon did not answer in time.
    Timeout(String),
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Connection(message) => write!(f, "connection failure: {message}"),
            Self::Rejected(rejected) => {
                write!(
                    f,
                    "the daemon refused the pairing: {} {}",
                    rejected.code, rejected.message
                )
            }
            Self::Protocol(message) => write!(f, "protocol failure: {message}"),
            Self::Timeout(message) => write!(f, "timeout: {message}"),
        }
    }
}

impl std::error::Error for PluginError {}

/// How the fake plugin answers the next request.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PluginBehavior {
    /// Answer normally.
    #[default]
    Normal,
    /// Answer after the given delay.
    Slow {
        /// How long the plugin waits before it answers.
        delay: Duration,
    },
    /// Close the connection instead of answering.
    Disconnect,
    /// Answer with the connection epoch of an older connection.
    StaleEpoch,
    /// Answer with a `DOCUMENT_CHANGED` error.
    DocumentChanged,
    /// Answer with a payload larger than the frame budget.
    Oversized,
    /// Answer an export as a chunked transfer.
    Chunked {
        /// Number of chunks of the transfer.
        chunks: usize,
        /// Number of bytes per chunk.
        chunk_bytes: usize,
        /// Delay between two chunks.
        delay: Duration,
    },
}

/// One message the plugin writes to the bridge.
#[derive(Debug)]
enum Outgoing {
    /// A protocol frame.
    Frame(BridgeFrame),
    /// A raw text message, used to break the frame budget on purpose.
    Raw(String),
    /// Close the connection.
    Close,
}

/// Shared state of the plugin connection.
#[derive(Debug)]
struct PluginContext {
    corpus: Arc<RwLock<FixtureCorpus>>,
    behavior: Arc<RwLock<PluginBehavior>>,
    granted_roots: Arc<RwLock<Vec<String>>>,
    grant_epoch: AtomicU64,
    requests_handled: AtomicU64,
    chunks_sent: AtomicU64,
    outbound: mpsc::Sender<Outgoing>,
}

/// The fake plugin client.
pub struct FakePlugin {
    session_id: SessionId,
    connection_epoch: u64,
    peer_addr: SocketAddr,
    corpus: Arc<RwLock<FixtureCorpus>>,
    behavior: Arc<RwLock<PluginBehavior>>,
    context: Arc<PluginContext>,
    outbound: mpsc::Sender<Outgoing>,
    tasks: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl FakePlugin {
    /// Dial the bridge endpoint and authenticate with the pairing secret.
    pub async fn connect(
        url: &str,
        secret: &str,
        client_id: &str,
        corpus: Arc<RwLock<FixtureCorpus>>,
    ) -> Result<Self, PluginError> {
        Self::connect_with(
            url,
            Vec::new(),
            secret,
            client_id,
            corpus,
            Duration::from_millis(50),
        )
        .await
    }

    /// Dial the bridge endpoint with extra handshake headers.
    ///
    /// The harness uses this to open a connection with a wrong `Host` or a
    /// wrong `Origin`, which the daemon has to refuse before the upgrade.
    pub async fn connect_with(
        url: &str,
        headers: Vec<(String, String)>,
        secret: &str,
        client_id: &str,
        corpus: Arc<RwLock<FixtureCorpus>>,
        heartbeat_interval: Duration,
    ) -> Result<Self, PluginError> {
        let request = build_request(url, headers)?;
        let uri: HttpUri = url
            .parse()
            .map_err(|err| PluginError::Connection(format!("the bridge url is invalid: {err}")))?;
        let host = uri
            .host()
            .ok_or_else(|| PluginError::Connection("the bridge url has no host".to_string()))?;
        let port = uri.port_u16().unwrap_or(80);
        let tcp = TcpStream::connect((host, port))
            .await
            .map_err(|err| PluginError::Connection(err.to_string()))?;
        let peer_addr = tcp
            .peer_addr()
            .map_err(|err| PluginError::Connection(err.to_string()))?;
        let (stream, _response) = client_async(request, tcp)
            .await
            .map_err(|err| PluginError::Connection(err.to_string()))?;
        let (mut sink, mut source) = stream.split();

        let auth = BridgeFrame::new(
            MessageType::HelloAuth,
            format!("hello-{}", uuid::Uuid::new_v4()),
            serde_json::to_value(AuthRequest {
                secret: secret.to_string(),
                client_id: client_id.to_string(),
                protocol_version: PROTOCOL_VERSION.to_string(),
            })
            .unwrap_or(Value::Null),
        );
        let encoded =
            serde_json::to_string(&auth).map_err(|err| PluginError::Protocol(err.to_string()))?;
        sink.send(ClientMessage::Text(encoded.into()))
            .await
            .map_err(|err| PluginError::Connection(err.to_string()))?;

        let accepted = match source.next().await {
            Some(Ok(ClientMessage::Text(text))) => parse_auth_response(text.as_str())?,
            Some(Ok(ClientMessage::Close(Some(frame)))) => {
                return Err(PluginError::Rejected(parse_close_reason(
                    frame.reason.as_str(),
                )))
            }
            Some(Ok(ClientMessage::Close(None))) => {
                return Err(PluginError::Rejected(AuthRejected {
                    code: "AUTH_REQUIRED".to_string(),
                    message: "the daemon closed the connection".to_string(),
                }))
            }
            Some(Ok(_)) => {
                return Err(PluginError::Protocol(
                    "the daemon answered with an unexpected message".to_string(),
                ))
            }
            Some(Err(err)) => return Err(PluginError::Connection(err.to_string())),
            None => {
                return Err(PluginError::Rejected(AuthRejected {
                    code: "AUTH_REQUIRED".to_string(),
                    message: "the daemon closed the connection".to_string(),
                }))
            }
        };

        let (outbound, mut outbound_rx) = mpsc::channel::<Outgoing>(64);
        let context = Arc::new(PluginContext {
            corpus: corpus.clone(),
            behavior: Arc::new(RwLock::new(PluginBehavior::Normal)),
            granted_roots: Arc::new(RwLock::new(Vec::new())),
            grant_epoch: AtomicU64::new(0),
            requests_handled: AtomicU64::new(0),
            chunks_sent: AtomicU64::new(0),
            outbound: outbound.clone(),
        });

        // The plugin reports its capabilities right after the handshake, the
        // way the plugin runtime probes them.
        {
            let mut capabilities = std::collections::BTreeMap::new();
            capabilities.insert(
                "nodes".to_string(),
                figtether_model::capabilities::CapabilityState::Available,
            );
            capabilities.insert(
                "variables".to_string(),
                figtether_model::capabilities::CapabilityState::Available,
            );
            capabilities.insert(
                "export_png".to_string(),
                figtether_model::capabilities::CapabilityState::Available,
            );
            capabilities.insert(
                "watch".to_string(),
                figtether_model::capabilities::CapabilityState::Unsupported {
                    reason: "the phase P1 watch is not enabled in this build".to_string(),
                },
            );
            let frame = BridgeFrame::new(
                MessageType::Capabilities,
                format!("capabilities-{}", uuid::Uuid::new_v4()),
                serde_json::to_value(CapabilitiesPayload { capabilities }).unwrap_or(Value::Null),
            );
            let _ = outbound.send(Outgoing::Frame(frame)).await;
        }

        let writer = tokio::spawn(async move {
            while let Some(item) = outbound_rx.recv().await {
                match item {
                    Outgoing::Frame(frame) => {
                        let encoded = match serde_json::to_string(&frame) {
                            Ok(encoded) => encoded,
                            Err(_) => continue,
                        };
                        if sink
                            .send(ClientMessage::Text(encoded.into()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Outgoing::Raw(text) => {
                        if sink.send(ClientMessage::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    Outgoing::Close => break,
                }
            }
            let _ = sink.close().await;
        });

        let reader_context = context.clone();
        let reader = tokio::spawn(async move {
            while let Some(Ok(message)) = source.next().await {
                let text = match message {
                    ClientMessage::Text(text) => text.to_string(),
                    ClientMessage::Binary(bytes) => String::from_utf8_lossy(&bytes).to_string(),
                    _ => continue,
                };
                let Ok(frame) = BridgeFrame::from_bytes(text.as_bytes()) else {
                    continue;
                };
                match frame.message_type {
                    MessageType::Request => {
                        handle_request(reader_context.clone(), frame).await;
                    }
                    MessageType::GrantUpdate => {
                        if let Ok(payload) = frame.payload_of::<GrantUpdatePayload>() {
                            *reader_context.granted_roots.write().await = payload.root_ids;
                            reader_context
                                .grant_epoch
                                .store(payload.grant_epoch, Ordering::Relaxed);
                        }
                    }
                    MessageType::Revoke => {
                        reader_context.granted_roots.write().await.clear();
                    }
                    MessageType::Capabilities => {
                        let payload = CapabilitiesPayload {
                            capabilities: std::collections::BTreeMap::new(),
                        };
                        let reply = BridgeFrame::new(
                            MessageType::Capabilities,
                            frame.request_id.clone(),
                            serde_json::to_value(payload).unwrap_or(Value::Null),
                        );
                        let _ = reader_context.outbound.send(Outgoing::Frame(reply)).await;
                    }
                    _ => {}
                }
            }
        });

        let heartbeat = {
            let context = context.clone();
            tokio::spawn(async move {
                let mut ticker = tokio::time::interval(heartbeat_interval);
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                loop {
                    ticker.tick().await;
                    let payload = json!({
                        "sequence": 0,
                        "timestamp": chrono::Utc::now(),
                    });
                    let frame = BridgeFrame::new(
                        MessageType::Heartbeat,
                        format!("hb-{}", uuid::Uuid::new_v4()),
                        payload,
                    );
                    if context.outbound.send(Outgoing::Frame(frame)).await.is_err() {
                        break;
                    }
                }
            })
        };

        Ok(Self {
            session_id: accepted.session_id,
            connection_epoch: accepted.connection_epoch,
            peer_addr,
            corpus,
            behavior: context.behavior.clone(),
            context,
            outbound,
            tasks: Mutex::new(vec![writer, reader, heartbeat]),
        })
    }

    /// The session the daemon created for this connection.
    pub fn session_id(&self) -> SessionId {
        self.session_id
    }

    /// The connection epoch the daemon assigned.
    pub fn connection_epoch(&self) -> u64 {
        self.connection_epoch
    }

    /// The address of the daemon end of the connection.
    pub fn peer_addr(&self) -> SocketAddr {
        self.peer_addr
    }

    /// The corpus the plugin serves.
    pub fn corpus(&self) -> Arc<RwLock<FixtureCorpus>> {
        self.corpus.clone()
    }

    /// Change the behaviour of the plugin.
    pub async fn set_behavior(&self, behavior: PluginBehavior) {
        *self.behavior.write().await = behavior;
    }

    /// The current behaviour of the plugin.
    pub async fn behavior(&self) -> PluginBehavior {
        self.behavior.read().await.clone()
    }

    /// Number of requests the plugin answered.
    pub fn requests_handled(&self) -> u64 {
        self.context.requests_handled.load(Ordering::Relaxed)
    }

    /// Number of chunks the plugin sent.
    pub fn chunks_sent(&self) -> u64 {
        self.context.chunks_sent.load(Ordering::Relaxed)
    }

    /// Grant a scope to the session, the way the plugin UI does.
    pub async fn grant(&self, roots: &[String]) -> Result<u64, PluginError> {
        let payload = GrantUpdatePayload {
            root_ids: roots.to_vec(),
            grant_epoch: 0,
        };
        let frame = BridgeFrame::new(
            MessageType::GrantUpdate,
            format!("grant-{}", uuid::Uuid::new_v4()),
            serde_json::to_value(payload).unwrap_or(Value::Null),
        );
        self.outbound
            .send(Outgoing::Frame(frame))
            .await
            .map_err(|err| PluginError::Connection(err.to_string()))?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            let granted = self.context.granted_roots.read().await.clone();
            let epoch = self.context.grant_epoch.load(Ordering::Relaxed);
            if granted == roots && epoch > 0 {
                return Ok(epoch);
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(PluginError::Timeout(
                    "the daemon did not acknowledge the grant".to_string(),
                ));
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Send the grant frame without waiting for an acknowledgement.
    ///
    /// The harness daemon answers a grant with a `grant_update` frame that
    /// carries the new epoch. The shipped daemon records the grant and sends
    /// no answer, so a caller that talks to the shipped daemon has to learn
    /// about the grant from the daemon instead of from the bridge.
    pub async fn grant_quiet(&self, roots: &[String]) -> Result<(), PluginError> {
        let payload = GrantUpdatePayload {
            root_ids: roots.to_vec(),
            grant_epoch: 0,
        };
        let frame = BridgeFrame::new(
            MessageType::GrantUpdate,
            format!("grant-{}", uuid::Uuid::new_v4()),
            serde_json::to_value(payload).unwrap_or(Value::Null),
        );
        self.outbound
            .send(Outgoing::Frame(frame))
            .await
            .map_err(|err| PluginError::Connection(err.to_string()))?;
        Ok(())
    }

    /// Close the connection, the way a closed plugin tab does.
    pub async fn close(&self) -> Result<(), PluginError> {
        self.outbound
            .send(Outgoing::Close)
            .await
            .map_err(|err| PluginError::Connection(err.to_string()))
    }
}

impl std::fmt::Debug for FakePlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FakePlugin")
            .field("session_id", &self.session_id)
            .field("connection_epoch", &self.connection_epoch)
            .field("peer_addr", &self.peer_addr)
            .finish()
    }
}

impl Drop for FakePlugin {
    fn drop(&mut self) {
        if let Ok(mut tasks) = self.tasks.lock() {
            for task in tasks.drain(..) {
                task.abort();
            }
        }
    }
}

/// Build the handshake request, optionally with extra headers.
fn build_request(
    url: &str,
    headers: Vec<(String, String)>,
) -> Result<HttpRequest<()>, PluginError> {
    let mut builder = HttpRequest::builder()
        .method("GET")
        .uri(url)
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==");
    // A test may replace the host header on purpose, so the default one is
    // only added when the caller did not bring its own.
    if !headers
        .iter()
        .any(|(name, _)| name.eq_ignore_ascii_case("host"))
    {
        builder = builder.header("Host", host_of(url));
    }
    for (name, value) in headers {
        builder = builder.header(name, value);
    }
    builder
        .body(())
        .map_err(|err| PluginError::Connection(err.to_string()))
}

/// The host part of a bridge url.
fn host_of(url: &str) -> String {
    url.trim_start_matches("ws://")
        .split('/')
        .next()
        .unwrap_or("127.0.0.1")
        .to_string()
}

/// Parse the answer of the daemon to the authentication frame.
fn parse_auth_response(text: &str) -> Result<AuthAccepted, PluginError> {
    let frame = BridgeFrame::from_bytes(text.as_bytes())
        .map_err(|err| PluginError::Protocol(err.to_string()))?;
    if let Ok(rejected) = frame.payload_of::<AuthRejected>() {
        return Err(PluginError::Rejected(rejected));
    }
    frame
        .payload_of::<AuthAccepted>()
        .map_err(|err| PluginError::Protocol(err.to_string()))
}

/// Parse the reason of a close frame into an authentication rejection.
fn parse_close_reason(reason: &str) -> AuthRejected {
    serde_json::from_str::<AuthRejected>(reason).unwrap_or_else(|_| AuthRejected {
        code: "AUTH_REQUIRED".to_string(),
        message: reason.to_string(),
    })
}

/// Answer one request of the daemon.
async fn handle_request(context: Arc<PluginContext>, frame: BridgeFrame) {
    context.requests_handled.fetch_add(1, Ordering::Relaxed);
    let behavior = context.behavior.read().await.clone();

    match &behavior {
        PluginBehavior::Slow { delay } => {
            tokio::time::sleep(*delay).await;
        }
        PluginBehavior::Disconnect => {
            let _ = context.outbound.send(Outgoing::Close).await;
            return;
        }
        _ => {}
    }

    let Ok(payload) = frame.payload_of::<RequestPayload>() else {
        return;
    };
    let op = payload.op;
    let params = payload.params;

    match behavior {
        PluginBehavior::DocumentChanged => {
            let error = error_frame(
                &frame,
                "DOCUMENT_CHANGED",
                "the document changed during the observation window",
                "retry once",
            );
            let _ = context.outbound.send(Outgoing::Frame(error)).await;
            return;
        }
        PluginBehavior::Oversized => {
            let raw = oversized_frame(&frame);
            let _ = context.outbound.send(Outgoing::Raw(raw)).await;
            return;
        }
        PluginBehavior::Chunked {
            chunks,
            chunk_bytes,
            delay,
        } if op == BridgeOp::ExportAssets => {
            send_chunks(&context, &frame, chunks, chunk_bytes, delay).await;
            return;
        }
        _ => {}
    }

    let epoch = match behavior {
        PluginBehavior::StaleEpoch => Some(0),
        _ => frame.connection_epoch,
    };
    match answer(&context, op, &params).await {
        Ok(data) => {
            let result = ResultPayload { data };
            let mut reply = BridgeFrame::new(
                MessageType::Result,
                frame.request_id.clone(),
                serde_json::to_value(result).unwrap_or(Value::Null),
            );
            reply.connection_epoch = epoch;
            let _ = context.outbound.send(Outgoing::Frame(reply)).await;
        }
        Err((code, message)) => {
            let error = error_frame(&frame, &code, &message, "retry once");
            let _ = context.outbound.send(Outgoing::Frame(error)).await;
        }
    }
}

/// Build the payload of an answer.
async fn answer(
    context: &Arc<PluginContext>,
    op: BridgeOp,
    params: &Value,
) -> Result<Value, (String, String)> {
    let corpus = context.corpus.read().await;
    let roots = context.granted_roots.read().await.clone();
    let index = corpus.node_index();

    match op {
        BridgeOp::ListNodes => {
            let root_id = params
                .get("root_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let depth = params
                .get("depth")
                .and_then(Value::as_u64)
                .unwrap_or(32)
                .min(32) as u32;
            let limit = params
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(200)
                .max(1) as usize;
            let after = params
                .get("after")
                .and_then(Value::as_str)
                .map(str::to_string);
            if !roots.contains(&root_id) {
                return Err((
                    "OUT_OF_SCOPE".to_string(),
                    format!("node {root_id} is outside the granted scope"),
                ));
            }
            let descendants = corpus.descendants(&root_id, depth);
            let scoped: Vec<PluginNode> = descendants
                .into_iter()
                .filter(|node| {
                    roots
                        .iter()
                        .any(|root| *root == node.id || index.contains(root, &node.id))
                })
                .collect();
            let start = match after.as_ref() {
                Some(after) => scoped
                    .iter()
                    .position(|node| &node.id == after)
                    .map(|position| position + 1)
                    .unwrap_or(scoped.len()),
                None => 0,
            };
            let rest = &scoped[start..];
            let take = limit.min(rest.len());
            let page = &rest[..take];
            let remaining = rest.len() > take;
            // The plugin reports the page and the fact that the traversal is
            // not finished. The traversal cursor is minted by the daemon: it
            // binds the session, the grant epoch, the scope and the
            // projection, and the plugin never sees it (ADR-006).
            Ok(json!({
                "nodes": page,
                "remaining": remaining,
                "truncated": remaining,
            }))
        }
        BridgeOp::ReadNodes => {
            let node_ids: Vec<String> = params
                .get("node_ids")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let mut selected: Vec<PluginNode> = Vec::new();
            for node_id in &node_ids {
                let in_scope = roots
                    .iter()
                    .any(|root| *root == *node_id || index.contains(root, node_id));
                if !in_scope {
                    return Err((
                        "OUT_OF_SCOPE".to_string(),
                        format!("node {node_id} is outside the granted scope"),
                    ));
                }
                if let Some(node) = corpus.node(node_id) {
                    selected.push(node.clone());
                }
            }
            Ok(json!({ "nodes": selected }))
        }
        BridgeOp::InspectDesign => {
            let root_id = params
                .get("root_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let node = corpus.node(root_id).ok_or_else(|| {
                (
                    "NODE_NOT_FOUND".to_string(),
                    format!("node {root_id} does not exist"),
                )
            })?;
            Ok(json!({ "node": node }))
        }
        BridgeOp::GetDesignTokens => Ok(json!({ "variables": [], "styles": [] })),
        BridgeOp::ExportAssets => {
            let node_ids: Vec<String> = params
                .get("node_ids")
                .and_then(Value::as_array)
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let node_id = node_ids.first().cloned().unwrap_or_default();
            if !roots
                .iter()
                .any(|root| *root == node_id || index.contains(root, &node_id))
            {
                return Err((
                    "OUT_OF_SCOPE".to_string(),
                    format!("node {node_id} is outside the granted scope"),
                ));
            }
            let mime = match params.get("format").and_then(Value::as_str) {
                Some("svg") => "image/svg+xml",
                _ => "image/png",
            };
            let bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
            let hash = figtether_protocol::sha256_hex(&bytes);
            Ok(json!({
                "node_id": node_id,
                "mime": mime,
                "width": 16,
                "height": 16,
                "bytes": base64_encode(&bytes),
                "hash": hash,
            }))
        }
        BridgeOp::GetSelection => {
            let selection: Vec<String> = corpus
                .nodes()
                .iter()
                .filter(|node| node.node_type == "TEXT")
                .take(1)
                .map(|node| node.id.clone())
                .collect();
            Ok(json!({ "node_ids": selection }))
        }
        BridgeOp::CaptureSnapshot => {
            let scoped: Vec<PluginNode> = corpus
                .nodes()
                .iter()
                .filter(|node| {
                    roots
                        .iter()
                        .any(|root| *root == node.id || index.contains(root, &node.id))
                })
                .cloned()
                .collect();
            Ok(json!({
                "document_label": corpus.document_label(),
                "page_name": corpus.page_name(),
                "nodes": scoped,
            }))
        }
        BridgeOp::ListSessions => Ok(json!({ "sessions": [] })),
        _ => Err((
            "CAPABILITY_UNAVAILABLE".to_string(),
            format!("the operation {} is not probed", op.as_str()),
        )),
    }
}

/// Send a chunked transfer.
async fn send_chunks(
    context: &Arc<PluginContext>,
    frame: &BridgeFrame,
    chunks: usize,
    chunk_bytes: usize,
    delay: Duration,
) {
    let transfer_id = format!("transfer-{}", uuid::Uuid::new_v4());
    let payload = vec![b'x'; chunk_bytes.max(1)];
    let total = (chunks.max(1) * chunk_bytes) as u64;
    for sequence in 0..chunks.max(1) {
        let is_last = sequence + 1 == chunks.max(1);
        let chunk = ChunkPayload {
            transfer_id: transfer_id.clone(),
            sequence: sequence as u32,
            bytes: payload.clone(),
            total_bytes: total,
            checksum: if is_last {
                Some(figtether_protocol::sha256_hex(&payload))
            } else {
                None
            },
        };
        let mut chunk_frame = BridgeFrame::new(
            MessageType::Chunk,
            frame.request_id.clone(),
            serde_json::to_value(chunk).unwrap_or(Value::Null),
        );
        chunk_frame.connection_epoch = frame.connection_epoch;
        if context
            .outbound
            .send(Outgoing::Frame(chunk_frame))
            .await
            .is_err()
        {
            return;
        }
        context.chunks_sent.fetch_add(1, Ordering::Relaxed);
        tokio::time::sleep(delay).await;
    }
}

/// Build an error frame for a failed operation.
fn error_frame(frame: &BridgeFrame, code: &str, message: &str, action: &str) -> BridgeFrame {
    let payload = ErrorPayload {
        code: code.to_string(),
        message: message.to_string(),
        retryable: false,
        action: action.to_string(),
        request_id: frame.request_id.clone(),
        retry_after_ms: None,
    };
    let mut reply = BridgeFrame::new(
        MessageType::Error,
        frame.request_id.clone(),
        serde_json::to_value(payload).unwrap_or(Value::Null),
    );
    reply.connection_epoch = frame.connection_epoch;
    reply
}

/// Build a frame that is larger than the frame budget.
fn oversized_frame(frame: &BridgeFrame) -> String {
    let blob = "x".repeat(MAX_FRAME_BYTES + 4096);
    json!({
        "protocol_version": PROTOCOL_VERSION,
        "message_type": "result",
        "request_id": frame.request_id,
        "payload": { "blob": blob },
    })
    .to_string()
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;
    STANDARD.encode(bytes)
}
