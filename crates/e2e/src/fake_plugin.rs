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
//!
//! The write path is served by the same connection. The plugin answers a
//! `preview_write` request with the preview of a plan, it answers an
//! `apply_write` request with the result of the plan, and it reports the
//! decision of the user with an `approval` frame. The plugin enforces the
//! scope and the destructive policy again, the way the real plugin does, so
//! a test can prove that the daemon refused a plan on its own.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use figtether_model::ids::SessionId;
use figtether_model::write::{canonical_plan_hash, WriteOperation, WriteOperationKind, WritePlan};
use figtether_protocol::plugin::PluginNode;
use figtether_protocol::write::{ApprovalDecisionPayload, GRANT_MODE_READ};
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

/// The session mode of a session that serves reads and writes.
pub const SESSION_MODE_LIVE: &str = "live";

/// The session mode of a session the user paused.
pub const SESSION_MODE_PAUSED: &str = "paused";

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

/// What the plugin resolved a plan into.
///
/// The record is what the fake plugin shows in its approval panel, so a test
/// can check that the preview the user would see names the nodes the plan
/// touches.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PreviewRecord {
    /// Identity of the plan.
    pub plan_id: String,
    /// Hash of the plan the daemon sent.
    pub plan_hash: String,
    /// Identity of the approval request, when the daemon named one.
    pub approval_id: Option<String>,
    /// Nodes the plan would create.
    pub created: Vec<String>,
    /// Nodes the plan would change.
    pub changed: Vec<String>,
    /// Nodes the plan would remove.
    pub removed: Vec<String>,
    /// Nodes the plan would move.
    pub moved: Vec<String>,
    /// Number of operations of the plan.
    pub operations_count: usize,
    /// Reasons a part of the plan could not be resolved.
    pub warnings: Vec<String>,
    /// Whether the plan can be applied as written.
    pub blocked: bool,
}

/// A decision the user took in the plugin UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRecord {
    /// Approval the decision answers.
    pub approval_id: String,
    /// Hash of the plan the user was shown.
    pub plan_hash: String,
    /// Whether the user approved the plan.
    pub approved: bool,
    /// Why the user rejected the plan.
    pub reason: Option<String>,
}

/// Shared state of the plugin connection.
#[derive(Debug)]
struct PluginContext {
    session_id: SessionId,
    corpus: Arc<RwLock<FixtureCorpus>>,
    behavior: Arc<RwLock<PluginBehavior>>,
    granted_roots: Arc<RwLock<Vec<String>>>,
    granted_mode: Arc<RwLock<String>>,
    session_mode: Arc<RwLock<String>>,
    grant_epoch: AtomicU64,
    requests_handled: AtomicU64,
    previews_handled: AtomicU64,
    applies_handled: AtomicU64,
    chunks_sent: AtomicU64,
    last_preview: Arc<RwLock<Option<PreviewRecord>>>,
    approvals: Arc<RwLock<Vec<ApprovalRecord>>>,
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
            session_id: accepted.session_id,
            corpus: corpus.clone(),
            behavior: Arc::new(RwLock::new(PluginBehavior::Normal)),
            granted_roots: Arc::new(RwLock::new(Vec::new())),
            granted_mode: Arc::new(RwLock::new(GRANT_MODE_READ.to_string())),
            session_mode: Arc::new(RwLock::new(SESSION_MODE_LIVE.to_string())),
            grant_epoch: AtomicU64::new(0),
            requests_handled: AtomicU64::new(0),
            previews_handled: AtomicU64::new(0),
            applies_handled: AtomicU64::new(0),
            chunks_sent: AtomicU64::new(0),
            last_preview: Arc::new(RwLock::new(None)),
            approvals: Arc::new(RwLock::new(Vec::new())),
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
                            if let Some(mode) = payload.mode.as_ref() {
                                *reader_context.granted_mode.write().await = mode.clone();
                            }
                            if let Some(session_mode) = payload.session_mode.as_ref() {
                                *reader_context.session_mode.write().await = session_mode.clone();
                            }
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

    /// Number of write plans the plugin previewed.
    ///
    /// A test that proves the daemon refused a plan before it reached the
    /// plugin checks this counter.
    pub fn previews_handled(&self) -> u64 {
        self.context.previews_handled.load(Ordering::Relaxed)
    }

    /// Number of write plans the plugin applied.
    pub fn applies_handled(&self) -> u64 {
        self.context.applies_handled.load(Ordering::Relaxed)
    }

    /// The preview the plugin served last.
    pub async fn last_preview(&self) -> Option<PreviewRecord> {
        self.context.last_preview.read().await.clone()
    }

    /// The decisions the user took in the plugin UI.
    pub async fn approvals_reported(&self) -> Vec<ApprovalRecord> {
        self.context.approvals.read().await.clone()
    }

    /// The mode of the grant the plugin holds.
    pub async fn granted_mode(&self) -> String {
        self.context.granted_mode.read().await.clone()
    }

    /// The session mode the plugin holds.
    ///
    /// The session mode is the pause extension of the grant: a session in
    /// the mode `paused` serves nothing.
    pub async fn session_mode(&self) -> String {
        self.context.session_mode.read().await.clone()
    }

    /// The epoch of the grant the plugin holds.
    pub fn grant_epoch(&self) -> u64 {
        self.context.grant_epoch.load(Ordering::Relaxed)
    }

    /// Grant a scope to the session, the way the plugin UI does.
    pub async fn grant(&self, roots: &[String]) -> Result<u64, PluginError> {
        self.grant_with_mode(roots, GRANT_MODE_READ).await
    }

    /// Grant a scope to the session with an explicit mode.
    ///
    /// The mode is a user decision: a read grant never authorizes a write,
    /// and a write grant is a separate action in the plugin UI.
    pub async fn grant_with_mode(&self, roots: &[String], mode: &str) -> Result<u64, PluginError> {
        self.grant_with_session_mode(roots, mode, SESSION_MODE_LIVE)
            .await
    }

    /// Grant a scope to the session with an explicit mode and session mode.
    ///
    /// The session mode says whether the session serves anything at all. The
    /// daemon is the authority on it, and the plugin follows the answer of
    /// the daemon.
    pub async fn grant_with_session_mode(
        &self,
        roots: &[String],
        mode: &str,
        session_mode: &str,
    ) -> Result<u64, PluginError> {
        let payload = GrantUpdatePayload {
            root_ids: roots.to_vec(),
            grant_epoch: 0,
            mode: Some(mode.to_string()),
            session_mode: Some(session_mode.to_string()),
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

    /// Pause or resume the session, the way the plugin UI does.
    ///
    /// The frame carries the scope and the mode the plugin holds, so the
    /// daemon applies the session mode on the grant the session already has.
    pub async fn set_session_mode(&self, session_mode: &str) -> Result<(), PluginError> {
        let roots = self.context.granted_roots.read().await.clone();
        let mode = self.context.granted_mode.read().await.clone();
        let payload = GrantUpdatePayload {
            root_ids: roots,
            grant_epoch: 0,
            mode: Some(mode),
            session_mode: Some(session_mode.to_string()),
        };
        let frame = BridgeFrame::new(
            MessageType::GrantUpdate,
            format!("session-mode-{}", uuid::Uuid::new_v4()),
            serde_json::to_value(payload).unwrap_or(Value::Null),
        );
        self.outbound
            .send(Outgoing::Frame(frame))
            .await
            .map_err(|err| PluginError::Connection(err.to_string()))?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        loop {
            if self.context.session_mode.read().await.as_str() == session_mode {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(PluginError::Timeout(
                    "the daemon did not acknowledge the session mode".to_string(),
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
        self.grant_quiet_with_mode(roots, GRANT_MODE_READ).await
    }

    /// Send the grant frame with an explicit mode, without waiting.
    pub async fn grant_quiet_with_mode(
        &self,
        roots: &[String],
        mode: &str,
    ) -> Result<(), PluginError> {
        let payload = GrantUpdatePayload {
            root_ids: roots.to_vec(),
            grant_epoch: 0,
            mode: Some(mode.to_string()),
            session_mode: Some(SESSION_MODE_LIVE.to_string()),
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

    /// Report the decision the user took in the plugin UI.
    ///
    /// The frame carries the identity of the approval request and the hash of
    /// the plan the user was shown, so the daemon can bind the decision to
    /// the plan it previewed.
    pub async fn report_approval(
        &self,
        approval_id: &str,
        plan_hash: &str,
        approved: bool,
        reason: Option<&str>,
    ) -> Result<(), PluginError> {
        let decision = ApprovalDecisionPayload {
            approval_id: approval_id.to_string(),
            plan_hash: plan_hash.to_string(),
            approved,
            reason: reason.map(str::to_string),
        };
        let frame = BridgeFrame::new(
            MessageType::Approval,
            format!("approval-{approval_id}"),
            serde_json::to_value(decision).unwrap_or(Value::Null),
        )
        .with_session(self.session_id)
        .with_connection_epoch(self.connection_epoch);
        self.outbound
            .send(Outgoing::Frame(frame))
            .await
            .map_err(|err| PluginError::Connection(err.to_string()))?;
        self.context.approvals.write().await.push(ApprovalRecord {
            approval_id: approval_id.to_string(),
            plan_hash: plan_hash.to_string(),
            approved,
            reason: reason.map(str::to_string),
        });
        Ok(())
    }

    /// Approve the plan the plugin previewed last.
    ///
    /// The helper uses the identity of the approval request and the hash of
    /// the plan the daemon sent, so a test does not have to repeat them.
    pub async fn approve_last_preview(&self) -> Result<(), PluginError> {
        let preview = self
            .last_preview()
            .await
            .ok_or_else(|| PluginError::Protocol("the plugin previewed no plan".to_string()))?;
        let approval_id = preview.approval_id.clone().ok_or_else(|| {
            PluginError::Protocol("the daemon named no approval request".to_string())
        })?;
        self.report_approval(&approval_id, &preview.plan_hash, true, None)
            .await
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
    // The daemon is the authority on the session mode: a paused session
    // serves nothing, so the plugin follows the daemon and refuses every
    // operation.
    if context.session_mode.read().await.as_str() == SESSION_MODE_PAUSED {
        return Err((
            "PAUSED".to_string(),
            "the user paused the session".to_string(),
        ));
    }
    // The write path takes the lock it needs, a read never blocks a write.
    if matches!(op, BridgeOp::PreviewWrite | BridgeOp::ApplyWrite) {
        return answer_write(context, op, params).await;
    }
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

// ---------------------------------------------------------------------------
// The write path
// ---------------------------------------------------------------------------

/// Answer a write operation of the daemon.
///
/// The plugin enforces the scope and the destructive policy again, the way
/// the real plugin does: a plan that names a node outside the grant never
/// reaches the document, and a plan that removes a node needs the destructive
/// flag.
async fn answer_write(
    context: &Arc<PluginContext>,
    op: BridgeOp,
    params: &Value,
) -> Result<Value, (String, String)> {
    match op {
        BridgeOp::PreviewWrite => {
            context.previews_handled.fetch_add(1, Ordering::Relaxed);
            preview_plan(context, params).await
        }
        BridgeOp::ApplyWrite => {
            context.applies_handled.fetch_add(1, Ordering::Relaxed);
            apply_plan(context, params).await
        }
        _ => Err((
            "CAPABILITY_UNAVAILABLE".to_string(),
            format!("the operation {} is not probed", op.as_str()),
        )),
    }
}

/// The plan of a write request.
///
/// The plan travels as the JSON the client sent. The plugin parses it with
/// the model of the write path, so the hash of the plan is the same hash the
/// daemon computed and the approval binds to the same plan.
fn parsed_plan(params: &Value) -> Result<WritePlan, (String, String)> {
    let raw = params
        .get("plan")
        .cloned()
        .unwrap_or_else(|| params.clone());
    serde_json::from_value(raw).map_err(|err| {
        (
            "PLAN_INVALID".to_string(),
            format!("the write plan is invalid: {err}"),
        )
    })
}

/// The node ids an operation names.
///
/// A create names the parent it appends to, every other operation names the
/// node it touches.
fn nodes_of(operation: &WriteOperation) -> Vec<String> {
    let mut nodes = Vec::new();
    if let Some(node_id) = operation.node_id.as_ref() {
        nodes.push(node_id.clone());
    }
    if let Some(parent_id) = operation.parent_id.as_ref() {
        nodes.push(parent_id.clone());
    }
    nodes
}

/// Whether a node is inside the granted scope of the session.
fn inside_scope(roots: &[String], index: &figtether_core::scope::NodeIndex, node_id: &str) -> bool {
    roots
        .iter()
        .any(|root| *root == node_id || index.contains(root, node_id))
}

/// The id of a node the plugin would create.
///
/// The id is derived from the parent and from the position of the operation,
/// so the preview and the apply that follows it name the same node.
fn minted_id(parent_id: &str, index: usize) -> String {
    let page = parent_id.split(':').next().unwrap_or("1").to_string();
    format!("{page}:9{:04}", index + 1)
}

/// Whether two hashes name the same plan.
///
/// The token separates its fields with a colon, so the digest inside it
/// carries no prefix. Both forms name the same plan.
fn same_hash(left: &str, right: &str) -> bool {
    fn bare(hash: &str) -> &str {
        hash.strip_prefix("sha256:").unwrap_or(hash)
    }
    bare(left) == bare(right)
}

/// Resolve a plan against the document without mutating it.
async fn preview_plan(
    context: &Arc<PluginContext>,
    params: &Value,
) -> Result<Value, (String, String)> {
    let plan = parsed_plan(params)?;
    let roots = context.granted_roots.read().await.clone();
    let corpus = context.corpus.read().await;
    let index = corpus.node_index();

    // The daemon sends the hash it computed for the plan. The plugin
    // compares it with its own hash of the plan it received, so a plan that
    // changed in transit is refused instead of previewed.
    let own_hash = canonical_plan_hash(&plan);
    let sent_hash = params
        .get("plan_hash")
        .and_then(Value::as_str)
        .map(str::to_string);
    if let Some(sent_hash) = sent_hash.as_deref() {
        if !same_hash(sent_hash, &own_hash) {
            return Err((
                "PLAN_HASH_MISMATCH".to_string(),
                "the plan differs from the plan the daemon hashed".to_string(),
            ));
        }
    }

    // The plugin enforces the scope again.
    for operation in &plan.operations {
        for node_id in nodes_of(operation) {
            if !inside_scope(&roots, &index, &node_id) {
                return Err((
                    "OUT_OF_SCOPE".to_string(),
                    format!("node {node_id} is outside the granted scope"),
                ));
            }
        }
    }

    // The plugin enforces the destructive policy again.
    if plan.operations.iter().any(|op| op.kind.is_destructive()) && !plan.destructive {
        return Err((
            "DESTRUCTIVE_NOT_ALLOWED".to_string(),
            "the plan carries a destructive operation without the destructive flag".to_string(),
        ));
    }

    let mut record = PreviewRecord {
        plan_id: plan.plan_id.clone(),
        // The hash of the plan the daemon computed is the one the approval
        // binds to, so the plugin reports it back.
        plan_hash: sent_hash.unwrap_or_else(|| own_hash.clone()),
        approval_id: params
            .get("approval_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        operations_count: plan.operations.len(),
        ..PreviewRecord::default()
    };

    for (index, operation) in plan.operations.iter().enumerate() {
        match operation.kind {
            WriteOperationKind::CreateNode => {
                let parent_id = operation.parent_id.clone().unwrap_or_default();
                if corpus.node(&parent_id).is_none() {
                    record.blocked = true;
                    record
                        .warnings
                        .push(format!("the parent {parent_id} does not exist"));
                    continue;
                }
                record.created.push(minted_id(&parent_id, index));
            }
            WriteOperationKind::SetText
            | WriteOperationKind::SetProperties
            | WriteOperationKind::SetFills => {
                let node_id = operation.node_id.clone().unwrap_or_default();
                if corpus.node(&node_id).is_none() {
                    record.blocked = true;
                    record
                        .warnings
                        .push(format!("the node {node_id} does not exist"));
                    continue;
                }
                record.changed.push(node_id);
            }
            WriteOperationKind::DeleteNode => {
                let node_id = operation.node_id.clone().unwrap_or_default();
                if corpus.node(&node_id).is_none() {
                    record.blocked = true;
                    record
                        .warnings
                        .push(format!("the node {node_id} does not exist"));
                    continue;
                }
                record.removed.push(node_id);
            }
            WriteOperationKind::InsertChild | WriteOperationKind::MoveNode => {
                let node_id = operation.node_id.clone().unwrap_or_default();
                let parent_id = operation.parent_id.clone().unwrap_or_default();
                if corpus.node(&node_id).is_none() || corpus.node(&parent_id).is_none() {
                    record.blocked = true;
                    record.warnings.push(format!(
                        "the move of {node_id} into {parent_id} cannot be resolved"
                    ));
                    continue;
                }
                record.moved.push(node_id);
            }
        }
    }

    let preview = json!({
        "plan_id": record.plan_id,
        "plan_hash": record.plan_hash,
        "created": record.created,
        "changed": record.changed,
        "removed": record.removed,
        "moved": record.moved,
        "operations_count": record.operations_count,
        "warnings": record.warnings,
        "blocked": record.blocked,
    });
    *context.last_preview.write().await = Some(record);
    Ok(json!({ "preview": preview }))
}

/// Apply an approved plan to the document.
async fn apply_plan(
    context: &Arc<PluginContext>,
    params: &Value,
) -> Result<Value, (String, String)> {
    let plan = parsed_plan(params)?;
    let plan_hash = canonical_plan_hash(&plan);
    let token = params
        .get("approval_token")
        .and_then(Value::as_str)
        .unwrap_or_default();

    // The plugin enforces the approval again: a plan is applied only with a
    // token that approves this plan, in this session, under this grant.
    let parts = figtether_protocol::write::parse_approval_token(token).ok_or_else(|| {
        (
            "APPROVAL_REQUIRED".to_string(),
            "the approval token is not valid".to_string(),
        )
    })?;
    if !same_hash(&parts.plan_hash, &plan_hash) {
        return Err((
            "PLAN_HASH_MISMATCH".to_string(),
            "the token approves another plan".to_string(),
        ));
    }
    if parts.session_id != context.session_id {
        return Err((
            "APPROVAL_REQUIRED".to_string(),
            "the token belongs to another session".to_string(),
        ));
    }
    let epoch = context.grant_epoch.load(Ordering::Relaxed);
    if parts.grant_epoch != epoch {
        return Err((
            "APPROVAL_EXPIRED".to_string(),
            "the grant moved since the approval".to_string(),
        ));
    }

    let roots = context.granted_roots.read().await.clone();
    let mut corpus = context.corpus.write().await;
    let index = corpus.node_index();

    // The plugin enforces the scope again.
    for operation in &plan.operations {
        for node_id in nodes_of(operation) {
            if !inside_scope(&roots, &index, &node_id) {
                return Err((
                    "OUT_OF_SCOPE".to_string(),
                    format!("node {node_id} is outside the granted scope"),
                ));
            }
        }
    }

    let mut created: Vec<String> = Vec::new();
    let mut changed: Vec<String> = Vec::new();
    let mut removed: Vec<String> = Vec::new();
    let mut moved: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    for (index, operation) in plan.operations.iter().enumerate() {
        match operation.kind {
            WriteOperationKind::CreateNode => {
                let parent_id = operation.parent_id.clone().unwrap_or_default();
                let node_id = minted_id(&parent_id, index);
                let node_type = operation
                    .node_type
                    .clone()
                    .unwrap_or_else(|| "FRAME".to_string());
                let name = operation
                    .properties
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("created node {node_id}"));
                if corpus.create_node(&parent_id, &node_id, &node_type, &name) {
                    created.push(node_id);
                } else {
                    warnings.push(format!("the parent {parent_id} does not exist"));
                }
            }
            WriteOperationKind::SetText => {
                let node_id = operation.node_id.clone().unwrap_or_default();
                let text = operation.text.clone().unwrap_or_default();
                if corpus.apply_edit(&node_id, &text) {
                    changed.push(node_id);
                } else {
                    warnings.push(format!("the node {node_id} does not exist"));
                }
            }
            WriteOperationKind::SetProperties => {
                let node_id = operation.node_id.clone().unwrap_or_default();
                let name = operation
                    .properties
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if corpus.set_name(&node_id, name) {
                    changed.push(node_id);
                } else {
                    warnings.push(format!("the node {node_id} does not exist"));
                }
            }
            WriteOperationKind::SetFills => {
                let node_id = operation.node_id.clone().unwrap_or_default();
                if corpus.node(&node_id).is_some() {
                    changed.push(node_id);
                } else {
                    warnings.push(format!("the node {node_id} does not exist"));
                }
            }
            WriteOperationKind::DeleteNode => {
                let node_id = operation.node_id.clone().unwrap_or_default();
                if corpus.remove_node(&node_id) {
                    removed.push(node_id);
                } else {
                    warnings.push(format!("the node {node_id} does not exist"));
                }
            }
            WriteOperationKind::InsertChild | WriteOperationKind::MoveNode => {
                let node_id = operation.node_id.clone().unwrap_or_default();
                let parent_id = operation.parent_id.clone().unwrap_or_default();
                if corpus.move_node(&node_id, &parent_id) {
                    moved.push(node_id);
                } else {
                    warnings.push(format!(
                        "the move of {node_id} into {parent_id} cannot be resolved"
                    ));
                }
            }
        }
    }

    let result = json!({
        "plan_id": plan.plan_id,
        "plan_hash": plan_hash,
        "approval_id": parts.approval_id,
        "created": created,
        "changed": changed,
        "removed": removed,
        "moved": moved,
        "undo_committed": true,
        "version_saved": Value::Null,
        "warnings": warnings,
    });
    Ok(json!({ "result": result }))
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

#[cfg(test)]
mod tests {
    use super::*;
    use figtether_protocol::write::GRANT_MODE_WRITE;

    /// A plan that creates one frame below the root.
    fn plan(root_id: &str) -> WritePlan {
        WritePlan {
            plan_id: "plan_1".to_string(),
            session_id: None,
            operations: vec![WriteOperation {
                kind: WriteOperationKind::CreateNode,
                parent_id: Some(root_id.to_string()),
                node_id: None,
                node_type: Some("FRAME".to_string()),
                properties: Default::default(),
                fills: None,
                text: None,
                x: None,
                y: None,
                index: None,
                font_family: None,
                font_style: None,
            }],
            intent: None,
            destructive: false,
        }
    }

    /// A plugin context that holds a grant of the whole corpus.
    async fn context() -> (Arc<PluginContext>, String) {
        let corpus = FixtureCorpus::small();
        let root_id = corpus.root_id().to_string();
        let (outbound, _receiver) = mpsc::channel::<Outgoing>(4);
        let context = Arc::new(PluginContext {
            session_id: SessionId::new(),
            corpus: Arc::new(RwLock::new(corpus)),
            behavior: Arc::new(RwLock::new(PluginBehavior::Normal)),
            granted_roots: Arc::new(RwLock::new(vec![root_id.clone()])),
            granted_mode: Arc::new(RwLock::new(GRANT_MODE_WRITE.to_string())),
            session_mode: Arc::new(RwLock::new(SESSION_MODE_LIVE.to_string())),
            grant_epoch: AtomicU64::new(1),
            requests_handled: AtomicU64::new(0),
            previews_handled: AtomicU64::new(0),
            applies_handled: AtomicU64::new(0),
            chunks_sent: AtomicU64::new(0),
            last_preview: Arc::new(RwLock::new(None)),
            approvals: Arc::new(RwLock::new(Vec::new())),
            outbound,
        });
        (context, root_id)
    }

    /// The parameters of a preview request.
    fn preview_params(plan: &WritePlan, plan_hash: &str) -> Value {
        json!({
            "plan": serde_json::to_value(plan).expect("serializes"),
            "approval_id": "apr_1",
            "plan_hash": plan_hash,
        })
    }

    #[tokio::test]
    async fn the_plan_the_daemon_hashed_is_previewed() {
        let (context, root_id) = context().await;
        let plan = plan(&root_id);
        let hash = canonical_plan_hash(&plan);

        let answer = preview_plan(&context, &preview_params(&plan, &hash))
            .await
            .expect("the plan is previewed");

        assert_eq!(answer["preview"]["plan_hash"], hash);
        let preview = context.last_preview.read().await.clone();
        let preview = preview.expect("the plugin previewed the plan");
        assert_eq!(preview.plan_hash, hash);
        assert_eq!(preview.approval_id.as_deref(), Some("apr_1"));
    }

    #[tokio::test]
    async fn a_plan_that_changed_in_transit_is_refused() {
        let (context, root_id) = context().await;
        let plan = plan(&root_id);
        let other = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

        let error = preview_plan(&context, &preview_params(&plan, other))
            .await
            .expect_err("the plan is refused");

        assert_eq!(error.0, "PLAN_HASH_MISMATCH");
        assert!(
            context.last_preview.read().await.is_none(),
            "the refused plan was never previewed"
        );
    }

    #[tokio::test]
    async fn a_paused_session_serves_nothing() {
        let (context, _root_id) = context().await;
        *context.session_mode.write().await = SESSION_MODE_PAUSED.to_string();

        let error = answer(&context, BridgeOp::PreviewWrite, &json!({}))
            .await
            .expect_err("the paused session refuses the plan");
        assert_eq!(error.0, "PAUSED");

        let error = answer(&context, BridgeOp::ListNodes, &json!({ "root_id": "1:2" }))
            .await
            .expect_err("the paused session refuses the read");
        assert_eq!(error.0, "PAUSED");
    }
}
