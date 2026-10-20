//! The plugin side of the bridge.
//!
//! This module owns everything that happens on the socket the Figma plugin
//! dials:
//!
//! - the auth window: the first frame must be `hello_auth` and it must
//!   arrive inside [`figtether_protocol::AUTH_WINDOW`];
//! - the pairing secret: single use, compared in constant time, and bound to
//!   the local installation that minted it;
//! - the connection epoch: every reconnect raises it and frames that carry an
//!   older epoch are dropped without side effects;
//! - the heartbeat: three missed heartbeats mark the session offline;
//! - the request channel: the scheduler sends requests through the hub and the
//!   plugin answers with a result or an error frame.
//!
//! Nothing that belongs to a design ever leaves this module through a log
//! line. Only codes, counts, and sizes are logged.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::ws::Message;
use figtether_model::capabilities::CapabilityState;
use figtether_model::errors::{ErrorCode, FigError};
use figtether_model::ids::SessionId;
use figtether_protocol::{
    AuthAccepted, AuthRejected, AuthRequest, BridgeFrame, BridgeOp, CapabilitiesPayload,
    ChunkPayload, ErrorPayload, HeartbeatPayload, MessageType, ProtocolError, ResultPayload,
    AUTH_WINDOW, HEARTBEAT_INTERVAL, MISSED_HEARTBEATS_BEFORE_OFFLINE, PROTOCOL_VERSION,
};
use futures_util::{SinkExt, Stream, StreamExt};
use tokio::sync::{mpsc, oneshot};

use crate::secrets::{PairingError, Secret};
use crate::DaemonState;

/// Number of frames the daemon may queue for one plugin connection.
const OUTBOUND_QUEUE: usize = 64;

/// Why the first frame of a connection was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthFailure {
    /// No frame arrived inside the auth window.
    WindowExpired,
    /// The peer closed the socket before authenticating.
    Closed,
    /// The transport failed.
    Transport(String),
    /// The frame could not be decoded.
    Malformed,
    /// The first frame was not a `hello_auth` frame.
    NotHelloAuth,
    /// The peer speaks an incompatible protocol version.
    ProtocolMismatch,
    /// The pairing secret is unknown or already used.
    BadSecret,
    /// The pairing secret expired.
    ExpiredSecret,
}

impl AuthFailure {
    /// Stable error code of the failure.
    pub const fn code(&self) -> &'static str {
        match self {
            Self::WindowExpired | Self::Closed | Self::NotHelloAuth | Self::BadSecret => {
                "AUTH_REQUIRED"
            }
            Self::ExpiredSecret => "AUTH_EXPIRED",
            Self::ProtocolMismatch => "PROTOCOL_MISMATCH",
            Self::Malformed => "PROTOCOL_MISMATCH",
            Self::Transport(_) => "INTERNAL_ERROR",
        }
    }
}

/// The connection epochs of the live sessions.
///
/// A reconnect raises the epoch of the session. Answers that carry an older
/// epoch are dropped, so work of a dead connection never lands in a new one.
#[derive(Debug, Default)]
pub struct ConnectionEpochs {
    /// Current epoch per session.
    inner: Mutex<HashMap<SessionId, u64>>,
}

impl ConnectionEpochs {
    /// Create an empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Raise the epoch of a session and return the new value.
    pub fn bump(&self, session_id: &SessionId) -> u64 {
        let mut guard = self.lock();
        let entry = guard.entry(*session_id).or_insert(0);
        *entry = entry.saturating_add(1);
        *entry
    }

    /// The current epoch of a session.
    pub fn current(&self, session_id: &SessionId) -> Option<u64> {
        self.lock().get(session_id).copied()
    }

    /// Whether a frame may be accepted for a session.
    ///
    /// A frame without an epoch is accepted: the epoch is optional in the
    /// frame envelope. A frame with an epoch must carry the current one.
    pub fn accepts(&self, session_id: &SessionId, epoch: Option<u64>) -> bool {
        match (epoch, self.current(session_id)) {
            (None, _) => true,
            (Some(frame), Some(current)) => frame == current,
            (Some(_), None) => false,
        }
    }

    /// Forget a session.
    pub fn forget(&self, session_id: &SessionId) {
        self.lock().remove(session_id);
    }

    /// Lock the table, recovering from a poisoned lock.
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<SessionId, u64>> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// The heartbeat state of one session.
#[derive(Debug, Clone)]
pub struct HeartbeatTracker {
    /// Instant of the last heartbeat.
    last_seen: Instant,
    /// Cadence of the heartbeats.
    interval: Duration,
    /// Number of missed heartbeats that mark the session offline.
    limit: u32,
}

impl HeartbeatTracker {
    /// Create a tracker that has just seen a heartbeat.
    pub fn new(now: Instant) -> Self {
        Self {
            last_seen: now,
            interval: HEARTBEAT_INTERVAL,
            limit: MISSED_HEARTBEATS_BEFORE_OFFLINE,
        }
    }

    /// Record a heartbeat.
    pub fn record(&mut self, now: Instant) {
        self.last_seen = now;
    }

    /// Number of heartbeats that are overdue.
    pub fn missed(&self, now: Instant) -> u32 {
        let elapsed = now.saturating_duration_since(self.last_seen);
        let interval_ms = self.interval.as_millis().max(1);
        u32::try_from(elapsed.as_millis() / interval_ms).unwrap_or(u32::MAX)
    }

    /// Whether the session must be considered offline.
    pub fn is_offline(&self, now: Instant) -> bool {
        self.missed(now) >= self.limit
    }

    /// The configured cadence.
    pub fn interval(&self) -> Duration {
        self.interval
    }
}

/// The answer of a plugin request.
#[derive(Debug, Clone)]
pub enum BridgeOutcome {
    /// The plugin answered with a result payload.
    Result(serde_json::Value),
    /// The plugin answered with an error payload.
    Error(FigError),
}

/// One live plugin connection.
#[derive(Debug, Clone)]
pub struct SessionLink {
    /// The current connection epoch.
    pub epoch: u64,
    /// The label the plugin presented.
    ///
    /// The label names the plugin UI. It is never the owner of the session:
    /// the owner is the local installation the pairing secret was minted for.
    pub client_id: String,
    /// The temporary session credential of this connection.
    pub credential: Secret,
    /// Outgoing frames of this connection.
    pub tx: mpsc::Sender<BridgeFrame>,
    /// Capabilities reported by the plugin runtime.
    pub capabilities: BTreeMap<String, CapabilityState>,
}

/// One request that waits for a plugin answer.
#[derive(Debug)]
struct PendingRequest {
    /// Session the request belongs to.
    session_id: Option<SessionId>,
    /// Channel of the waiting caller.
    tx: oneshot::Sender<BridgeOutcome>,
}

/// The hub between the scheduler and the plugin connections.
#[derive(Debug, Default)]
pub struct BridgeHub {
    /// Live connections by session.
    sessions: Mutex<HashMap<SessionId, SessionLink>>,
    /// Session of every plugin label.
    ///
    /// The label is the client id the plugin presented. It names the plugin
    /// UI, so a reconnect of the same UI finds its session again. It never
    /// decides which sessions a local client may see.
    plugins: Mutex<HashMap<String, SessionId>>,
    /// Requests that wait for an answer.
    pending: Mutex<HashMap<String, PendingRequest>>,
    /// Connection epochs.
    epochs: ConnectionEpochs,
    /// Sessions whose grant was revoked.
    revoked: Mutex<HashSet<SessionId>>,
}

impl BridgeHub {
    /// Create an empty hub.
    pub fn new() -> Self {
        Self::default()
    }

    /// The connection epochs of the hub.
    pub fn epochs(&self) -> &ConnectionEpochs {
        &self.epochs
    }

    /// Attach a live connection to a session.
    pub fn attach(&self, session_id: SessionId, link: SessionLink) {
        self.lock_sessions().insert(session_id, link);
    }

    /// Detach the connection of a session.
    pub fn detach(&self, session_id: &SessionId) {
        self.lock_sessions().remove(session_id);
    }

    /// The live connection of a session.
    pub fn link(&self, session_id: &SessionId) -> Option<SessionLink> {
        self.lock_sessions().get(session_id).cloned()
    }

    /// Number of live connections.
    pub fn connection_count(&self) -> usize {
        self.lock_sessions().len()
    }

    /// The capabilities the connected plugins reported.
    ///
    /// The reports are sorted by session id, so the sanitized status of the
    /// daemon does not change its order between two calls.
    pub fn capability_reports(&self) -> Vec<(SessionId, BTreeMap<String, CapabilityState>)> {
        let sessions = self.lock_sessions();
        let mut reports: Vec<(SessionId, BTreeMap<String, CapabilityState>)> = sessions
            .iter()
            .map(|(session_id, link)| (*session_id, link.capabilities.clone()))
            .collect();
        reports.sort_by_key(|(session_id, _)| *session_id);
        reports
    }

    /// Remember which session belongs to a plugin label.
    pub fn remember_plugin(&self, label: &str, session_id: SessionId) {
        self.lock_plugins().insert(label.to_string(), session_id);
    }

    /// The session of a plugin label.
    pub fn session_for_plugin(&self, label: &str) -> Option<SessionId> {
        self.lock_plugins().get(label).copied()
    }

    /// Send a frame to the plugin of a session.
    pub async fn send(&self, session_id: &SessionId, frame: BridgeFrame) -> Result<(), FigError> {
        let link = self.link(session_id).ok_or_else(|| {
            FigError::new(ErrorCode::PluginOffline, "the plugin is not connected")
        })?;
        link.tx
            .send(frame)
            .await
            .map_err(|_| FigError::new(ErrorCode::PluginOffline, "the plugin connection is closed"))
    }

    /// Send a frame to the plugin of a session without waiting.
    pub fn try_send(&self, session_id: &SessionId, frame: BridgeFrame) -> Result<(), FigError> {
        let link = self.link(session_id).ok_or_else(|| {
            FigError::new(ErrorCode::PluginOffline, "the plugin is not connected")
        })?;
        link.tx
            .try_send(frame)
            .map_err(|_| FigError::new(ErrorCode::PluginOffline, "the plugin connection is closed"))
    }

    /// Register a request that waits for an answer.
    pub fn expect(
        &self,
        request_id: &str,
        session_id: Option<SessionId>,
    ) -> oneshot::Receiver<BridgeOutcome> {
        let (tx, rx) = oneshot::channel();
        self.lock_pending()
            .insert(request_id.to_string(), PendingRequest { session_id, tx });
        rx
    }

    /// Drop a pending request without answering it.
    pub fn cancel_expect(&self, request_id: &str) {
        self.lock_pending().remove(request_id);
    }

    /// Answer a pending request.
    pub fn complete(&self, request_id: &str, outcome: BridgeOutcome) {
        if let Some(pending) = self.lock_pending().remove(request_id) {
            let _ = pending.tx.send(outcome);
        }
    }

    /// Mark a session as revoked and drop its pending work.
    pub fn revoke(&self, session_id: &SessionId) -> usize {
        self.lock_revoked().insert(*session_id);
        self.lock_sessions().remove(session_id);
        let mut pending = self.lock_pending();
        let ids: Vec<String> = pending
            .iter()
            .filter(|(_, entry)| entry.session_id == Some(*session_id))
            .map(|(id, _)| id.clone())
            .collect();
        let mut cancelled = 0;
        for id in ids {
            if let Some(entry) = pending.remove(&id) {
                let _ = entry.tx.send(BridgeOutcome::Error(FigError::new(
                    ErrorCode::Revoked,
                    "the grant was revoked before the answer arrived",
                )));
                cancelled += 1;
            }
        }
        cancelled
    }

    /// Whether the grant of a session was revoked.
    pub fn is_revoked(&self, session_id: &SessionId) -> bool {
        self.lock_revoked().contains(session_id)
    }

    /// Fail the pending work of a session.
    ///
    /// The daemon uses this when the plugin stops answering, so a client
    /// never waits for the full deadline of a dead connection.
    pub fn fail_session(&self, session_id: &SessionId, reason: &str) -> usize {
        let mut pending = self.lock_pending();
        let ids: Vec<String> = pending
            .iter()
            .filter(|(_, entry)| entry.session_id == Some(*session_id))
            .map(|(id, _)| id.clone())
            .collect();
        let mut failed = 0;
        for id in ids {
            if let Some(entry) = pending.remove(&id) {
                let _ = entry.tx.send(BridgeOutcome::Error(FigError::new(
                    ErrorCode::PluginOffline,
                    reason.to_string(),
                )));
                failed += 1;
            }
        }
        failed
    }

    /// Lock the session table, recovering from a poisoned lock.
    fn lock_sessions(&self) -> std::sync::MutexGuard<'_, HashMap<SessionId, SessionLink>> {
        self.sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Lock the plugin table, recovering from a poisoned lock.
    fn lock_plugins(&self) -> std::sync::MutexGuard<'_, HashMap<String, SessionId>> {
        self.plugins
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Lock the pending table, recovering from a poisoned lock.
    fn lock_pending(&self) -> std::sync::MutexGuard<'_, HashMap<String, PendingRequest>> {
        self.pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Lock the revoked table, recovering from a poisoned lock.
    fn lock_revoked(&self) -> std::sync::MutexGuard<'_, HashSet<SessionId>> {
        self.revoked
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Whether a protocol version is compatible.
///
/// The major version has to match. The minor version may differ, because the
/// protocol only adds optional fields.
pub fn protocol_version_matches(version: &str) -> bool {
    let mut parts = version.split('.');
    let major = parts.next().unwrap_or_default();
    let expected = PROTOCOL_VERSION.split('.').next().unwrap_or_default();
    !major.is_empty() && major == expected
}

/// Encode a frame for the wire.
pub fn encode_frame(frame: &BridgeFrame) -> Result<Message, ProtocolError> {
    let bytes = frame.to_bytes()?;
    let text = String::from_utf8(bytes).map_err(|err| ProtocolError::Encode(err.to_string()))?;
    Ok(Message::Text(text.into()))
}

/// Read the first frame of a connection inside the auth window.
///
/// The first frame must be a `hello_auth` frame. Anything else closes the
/// connection before a single byte of design data could flow.
pub async fn read_first_frame<S>(stream: &mut S) -> Result<BridgeFrame, AuthFailure>
where
    S: Stream<Item = Result<Message, axum::Error>> + Unpin,
{
    read_first_frame_within(stream, AUTH_WINDOW).await
}

/// Read the first frame of a connection inside a given window.
///
/// The window is a parameter so the deadline logic can be exercised without
/// waiting for the production window.
pub async fn read_first_frame_within<S>(
    stream: &mut S,
    window: Duration,
) -> Result<BridgeFrame, AuthFailure>
where
    S: Stream<Item = Result<Message, axum::Error>> + Unpin,
{
    let message = match tokio::time::timeout(window, stream.next()).await {
        Err(_elapsed) => return Err(AuthFailure::WindowExpired),
        Ok(None) => return Err(AuthFailure::Closed),
        Ok(Some(Err(err))) => return Err(AuthFailure::Transport(err.to_string())),
        Ok(Some(Ok(message))) => message,
    };

    let bytes = match message {
        Message::Text(text) => text.as_str().as_bytes().to_vec(),
        Message::Binary(binary) => binary.to_vec(),
        Message::Close(_) => return Err(AuthFailure::Closed),
        Message::Ping(_) | Message::Pong(_) => return Err(AuthFailure::NotHelloAuth),
    };

    let frame = BridgeFrame::from_bytes(&bytes).map_err(|_| AuthFailure::Malformed)?;
    if frame.message_type != MessageType::HelloAuth {
        return Err(AuthFailure::NotHelloAuth);
    }
    Ok(frame)
}

/// Serve one plugin connection.
pub async fn serve_plugin(socket: axum::extract::ws::WebSocket, state: Arc<DaemonState>) {
    let (mut sender, mut receiver) = socket.split();

    let frame = match read_first_frame(&mut receiver).await {
        Ok(frame) => frame,
        Err(failure) => {
            tracing::warn!(code = failure.code(), "plugin authentication refused");
            let _ = sender.send(Message::Close(None)).await;
            return;
        }
    };

    let request: AuthRequest = match frame.payload_of() {
        Ok(request) => request,
        Err(_) => {
            tracing::warn!(code = "PROTOCOL_MISMATCH", "plugin authentication refused");
            let _ = sender.send(Message::Close(None)).await;
            return;
        }
    };

    if !protocol_version_matches(&request.protocol_version) {
        reject_auth(
            &mut sender,
            "PROTOCOL_MISMATCH",
            "the protocol version is not supported",
        )
        .await;
        return;
    }

    // The secret carries the owner of the session it opens. The owner is the
    // local installation that minted the secret, not the plugin UI that
    // spends it.
    let owner = match state.pairing.consume(&request.secret) {
        Ok(paired) => paired.owner,
        Err(PairingError::Expired) => {
            reject_auth(&mut sender, "AUTH_EXPIRED", "the pairing secret expired").await;
            return;
        }
        Err(PairingError::Unknown) => {
            reject_auth(
                &mut sender,
                "AUTH_REQUIRED",
                "the pairing secret is not valid",
            )
            .await;
            return;
        }
    };

    // A revoked grant never comes back. A plugin that pairs again after a
    // revocation opens a new session and waits for a new grant, so the old
    // session keeps refusing reads.
    let known_session = state.hub.session_for_plugin(&request.client_id);
    let revoked = known_session.is_some_and(|session_id| state.hub.is_revoked(&session_id));
    let session_id = match known_session {
        Some(session_id) if !revoked => session_id,
        _ => SessionId::new(),
    };
    let epoch = state.hub.epochs().bump(&session_id);

    let (out_tx, mut out_rx) = mpsc::channel::<BridgeFrame>(OUTBOUND_QUEUE);
    let credential = Secret::generate();
    let capabilities = daemon_capabilities();

    let probes = figtether_model::capabilities::Capabilities {
        probes: capabilities.clone(),
    };
    if known_session.is_none() || revoked {
        let registered = crate::domain_call(ErrorCode::InternalError, || {
            state.registry.register(session_id, owner.clone(), probes)
        });
        if let Err(err) = registered {
            reject_auth(
                &mut sender,
                err.code.as_str(),
                "the session could not be registered",
            )
            .await;
            return;
        }
    } else {
        let _ = crate::domain_call(ErrorCode::InternalError, || {
            state.registry.bump_connection_epoch(&session_id)
        });
    }

    // The first frame authenticated the plugin, so the session leaves the
    // unpaired state and waits for the grant of the user.
    authenticate_session(&state, session_id);

    state.hub.remember_plugin(&request.client_id, session_id);
    state.hub.attach(
        session_id,
        SessionLink {
            epoch,
            client_id: request.client_id.clone(),
            credential: credential.clone(),
            tx: out_tx,
            capabilities: capabilities.clone(),
        },
    );

    let accepted = AuthAccepted {
        session_credential: credential.as_str().to_string(),
        session_id,
        connection_epoch: epoch,
        grant_epoch: crate::guarded(None, || {
            state
                .registry
                .get(&session_id)
                .and_then(|session| session.grant_epoch())
        }),
        capabilities,
    };
    let answer = BridgeFrame::new(
        MessageType::Result,
        "auth",
        serde_json::to_value(&accepted).unwrap_or_default(),
    )
    .with_session(session_id)
    .with_connection_epoch(epoch);
    if let Ok(message) = encode_frame(&answer) {
        if sender.send(message).await.is_err() {
            return;
        }
    }

    tracing::info!(code = "AUTH_ACCEPTED", "plugin session authenticated");

    let mut heartbeat = HeartbeatTracker::new(Instant::now());
    let mut offline = false;
    let mut last_heartbeat_sent = Instant::now();
    let mut ticker = tokio::time::interval(HEARTBEAT_INTERVAL / 2);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        let bytes = text.as_str().as_bytes().to_vec();
                        handle_incoming(&state, session_id, epoch, &bytes, &mut heartbeat).await;
                    }
                    Some(Ok(Message::Binary(binary))) => {
                        handle_incoming(&state, session_id, epoch, &binary, &mut heartbeat).await;
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        let _ = sender.send(Message::Pong(payload)).await;
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Err(_)) => break,
                }
            }
            outbound = out_rx.recv() => {
                match outbound {
                    Some(frame) => {
                        match encode_frame(&frame) {
                            Ok(message) => {
                                if sender.send(message).await.is_err() {
                                    break;
                                }
                            }
                            Err(_err) => {
                                tracing::warn!(code = "PROTOCOL_MISMATCH", "outbound frame rejected");
                            }
                        }
                    }
                    None => break,
                }
            }
            _ = ticker.tick() => {
                let now = Instant::now();
                if now.saturating_duration_since(last_heartbeat_sent) >= HEARTBEAT_INTERVAL {
                    last_heartbeat_sent = now;
                    let frame = BridgeFrame::new(
                        MessageType::Heartbeat,
                        "heartbeat",
                        serde_json::json!({ "sequence": now.elapsed().as_secs() }),
                    )
                    .with_session(session_id)
                    .with_connection_epoch(epoch);
                    if let Ok(message) = encode_frame(&frame) {
                        if sender.send(message).await.is_err() {
                            break;
                        }
                    }
                }
                if !offline && heartbeat.is_offline(now) {
                    offline = true;
                    tracing::warn!(code = "PLUGIN_OFFLINE", "plugin stopped sending heartbeats");
                    let _ = crate::domain_call(ErrorCode::InternalError, || {
                        state
                            .registry
                            .set_state(&session_id, figtether_core::SessionState::Offline)
                    });
                    state.hub.fail_session(&session_id, "the plugin stopped answering");
                }
            }
        }
    }

    state.hub.detach(&session_id);
    tracing::info!(code = "PLUGIN_CLOSED", "plugin connection closed");
}

/// Answer a failed authentication and close the socket.
async fn reject_auth(
    sender: &mut futures_util::stream::SplitSink<axum::extract::ws::WebSocket, Message>,
    code: &str,
    message: &str,
) {
    let rejected = AuthRejected {
        code: code.to_string(),
        message: message.to_string(),
    };
    let frame = BridgeFrame::new(
        MessageType::Error,
        "auth",
        serde_json::to_value(&rejected).unwrap_or_default(),
    );
    if let Ok(message) = encode_frame(&frame) {
        let _ = sender.send(message).await;
    }
    let _ = sender.close().await;
}

/// Handle one frame that arrived after the authentication.
async fn handle_incoming(
    state: &Arc<DaemonState>,
    session_id: SessionId,
    epoch: u64,
    bytes: &[u8],
    heartbeat: &mut HeartbeatTracker,
) {
    let frame = match BridgeFrame::from_bytes(bytes) {
        Ok(frame) => frame,
        Err(_) => {
            tracing::warn!(code = "PROTOCOL_MISMATCH", "plugin frame rejected");
            return;
        }
    };

    if frame.connection_epoch.is_some_and(|value| value != epoch)
        || !state
            .hub
            .epochs()
            .accepts(&session_id, frame.connection_epoch)
    {
        tracing::debug!(code = "STALE_EPOCH", "plugin frame dropped");
        return;
    }

    match frame.message_type {
        MessageType::Heartbeat => {
            heartbeat.record(Instant::now());
            if let Ok(payload) = frame.payload_of::<HeartbeatPayload>() {
                let _ = payload.sequence;
            }
        }
        MessageType::Capabilities => {
            if let Ok(payload) = frame.payload_of::<CapabilitiesPayload>() {
                if let Some(link) = state.hub.link(&session_id) {
                    let mut updated = link;
                    updated.capabilities = payload.capabilities;
                    state.hub.attach(session_id, updated);
                }
            }
        }
        MessageType::GrantUpdate => {
            if let Ok(payload) = frame.payload_of::<figtether_protocol::GrantUpdatePayload>() {
                let granted = crate::domain_call(ErrorCode::PermissionDenied, || {
                    state.registry.grant(&session_id, payload.root_ids.clone())
                });
                match granted {
                    Ok(grant) => {
                        // The grant is bound, so the session serves reads from
                        // now on. A session that is paused or revoked keeps
                        // refusing reads: the registry refuses the grant.
                        let ready = crate::domain_call(ErrorCode::InternalError, || {
                            state
                                .registry
                                .set_state(&session_id, figtether_core::SessionState::Ready)
                        });
                        if let Err(err) = ready {
                            tracing::warn!(
                                code = err.code.as_str(),
                                "the session did not become ready"
                            );
                        }
                        // The scope moved, so a queued read of the old scope
                        // never runs.
                        crate::client_api::supersede_queued_jobs(state, &session_id);
                        // The daemon remembers the grant, so a revocation
                        // drops the assets of every grant of the session.
                        state.grants.remember(&session_id, grant.grant_id);
                        // The plugin learns the epoch of the grant it holds,
                        // so both sides agree on the authorization.
                        let reply = BridgeFrame::new(
                            MessageType::GrantUpdate,
                            frame.request_id.clone(),
                            serde_json::to_value(figtether_protocol::GrantUpdatePayload {
                                root_ids: payload.root_ids.clone(),
                                grant_epoch: grant.grant_epoch,
                            })
                            .unwrap_or_default(),
                        )
                        .with_session(session_id)
                        .with_connection_epoch(epoch);
                        let _ = state.hub.try_send(&session_id, reply);
                        tracing::info!(code = "GRANT_UPDATED", "grant scope changed");
                    }
                    Err(err) => {
                        tracing::warn!(code = err.code.as_str(), "grant update refused");
                    }
                }
            }
        }
        MessageType::Result => {
            if let Ok(payload) = frame.payload_of::<ResultPayload>() {
                state
                    .hub
                    .complete(&frame.request_id, BridgeOutcome::Result(payload.data));
            }
        }
        MessageType::Error => {
            if let Ok(payload) = frame.payload_of::<ErrorPayload>() {
                let error =
                    FigError::new(plugin_error_code(&payload.code), payload.message.clone())
                        .with_request_id(frame.request_id.clone())
                        .with_retryable(payload.retryable)
                        .with_action(payload.action.clone());
                state
                    .hub
                    .complete(&frame.request_id, BridgeOutcome::Error(error));
            }
        }
        MessageType::Chunk => {
            if let Ok(payload) = frame.payload_of::<ChunkPayload>() {
                tracing::debug!(code = "CHUNK", "chunk received");
                let _ = payload.total_bytes;
            }
        }
        MessageType::Progress => {
            tracing::debug!(code = "PROGRESS", "progress reported");
        }
        MessageType::Revoke => {
            tracing::info!(code = "REVOKED", "plugin reported a revoked grant");
            let _ = crate::control::revoke_session(state, &session_id);
        }
        MessageType::HelloAuth => {
            tracing::warn!(code = "PROTOCOL_MISMATCH", "duplicate authentication frame");
        }
        MessageType::Request | MessageType::Cancel => {
            tracing::warn!(code = "PROTOCOL_MISMATCH", "unexpected frame type");
        }
    }
}

/// The error code a plugin reported.
///
/// The plugin names a failure with the vocabulary of the error catalog. An
/// unknown code becomes an internal error, so a failure never passes as a
/// successful read.
pub fn plugin_error_code(code: &str) -> ErrorCode {
    match code {
        "AUTH_REQUIRED" => ErrorCode::AuthRequired,
        "AUTH_EXPIRED" => ErrorCode::AuthExpired,
        "PERMISSION_DENIED" => ErrorCode::PermissionDenied,
        "OUT_OF_SCOPE" => ErrorCode::OutOfScope,
        "PLUGIN_OFFLINE" => ErrorCode::PluginOffline,
        "SESSION_NOT_FOUND" => ErrorCode::SessionNotFound,
        "PAUSED" => ErrorCode::Paused,
        "REVOKED" => ErrorCode::Revoked,
        "NODE_NOT_FOUND" => ErrorCode::NodeNotFound,
        "CAPABILITY_UNAVAILABLE" => ErrorCode::CapabilityUnavailable,
        "PAYLOAD_TOO_LARGE" => ErrorCode::PayloadTooLarge,
        "SCOPE_TOO_LARGE" => ErrorCode::ScopeTooLarge,
        "BUSY" => ErrorCode::Busy,
        "TIMEOUT" => ErrorCode::Timeout,
        "CANCELLED" => ErrorCode::Cancelled,
        "DOCUMENT_CHANGED" => ErrorCode::DocumentChanged,
        "CURSOR_EXPIRED" => ErrorCode::CursorExpired,
        "RESYNC_REQUIRED" => ErrorCode::ResyncRequired,
        "UNSUPPORTED_SCHEMA" => ErrorCode::UnsupportedSchema,
        "PROTOCOL_MISMATCH" => ErrorCode::ProtocolMismatch,
        "EXPORT_FAILED" => ErrorCode::ExportFailed,
        "FONT_UNAVAILABLE" => ErrorCode::FontUnavailable,
        "SESSION_LIMIT" => ErrorCode::SessionLimit,
        "INVALID_LIMIT" => ErrorCode::InvalidLimit,
        _ => ErrorCode::InternalError,
    }
}

/// Move a session into the state that follows a successful authentication.
///
/// The first frame of a connection authenticates the plugin, so a new session
/// leaves the unpaired state and waits for the grant of the user. A session
/// that already serves reads keeps its state, a session whose grant is bound
/// keeps serving reads, and a session that the user paused keeps refusing
/// reads.
fn authenticate_session(state: &Arc<DaemonState>, session_id: SessionId) {
    let Some(session) = crate::guarded(None::<figtether_core::Session>, || {
        state.registry.get(&session_id)
    }) else {
        return;
    };
    let target = match session.state {
        figtether_core::SessionState::Unpaired | figtether_core::SessionState::Offline => {
            figtether_core::SessionState::Paired
        }
        // The grant is bound, so the session serves reads again.
        figtether_core::SessionState::AwaitingGrant => figtether_core::SessionState::Ready,
        _ => return,
    };
    if let Err(err) = crate::domain_call(ErrorCode::InternalError, || {
        state.registry.set_state(&session_id, target)
    }) {
        tracing::warn!(code = err.code.as_str(), "the session kept its state");
    }
}

/// The capabilities the daemon itself supports.
pub fn daemon_capabilities() -> BTreeMap<String, CapabilityState> {
    let mut capabilities = BTreeMap::new();
    for op in [
        BridgeOp::ListNodes,
        BridgeOp::ReadNodes,
        BridgeOp::InspectDesign,
        BridgeOp::GetDesignTokens,
        BridgeOp::ExportAssets,
        BridgeOp::GetSelection,
        BridgeOp::CaptureSnapshot,
        BridgeOp::ListSessions,
        BridgeOp::WatchScope,
        BridgeOp::GetChanges,
    ] {
        capabilities.insert(op.as_str().to_string(), CapabilityState::Available);
    }
    capabilities
}

#[cfg(test)]
mod tests {
    use super::*;
    use figtether_protocol::AuthRequest;

    fn hello_frame(secret: &str) -> Vec<u8> {
        let request = AuthRequest {
            secret: secret.to_string(),
            client_id: "client-1".to_string(),
            protocol_version: PROTOCOL_VERSION.to_string(),
        };
        BridgeFrame::new(
            MessageType::HelloAuth,
            "auth",
            serde_json::to_value(&request).expect("serializes"),
        )
        .to_bytes()
        .expect("encodes")
    }

    fn message_stream(items: Vec<Message>) -> impl Stream<Item = Result<Message, axum::Error>> {
        futures_util::stream::iter(items.into_iter().map(Ok))
    }

    #[tokio::test]
    async fn auth_window_expires_without_a_first_frame() {
        // The stream never produces a frame, so the window has to expire.
        let stream = futures_util::stream::pending::<Result<Message, axum::Error>>();
        futures_util::pin_mut!(stream);
        let failure = read_first_frame_within(&mut stream, Duration::from_millis(50))
            .await
            .expect_err("window expired");
        assert_eq!(failure, AuthFailure::WindowExpired);
        assert_eq!(failure.code(), "AUTH_REQUIRED");
    }

    #[test]
    fn the_auth_window_is_three_seconds() {
        assert_eq!(AUTH_WINDOW, Duration::from_secs(3));
    }

    #[tokio::test]
    async fn a_frame_that_is_not_hello_auth_is_refused() {
        let frame = BridgeFrame::new(MessageType::Heartbeat, "req", serde_json::json!({}));
        let bytes = frame.to_bytes().expect("encodes");
        let mut stream = message_stream(vec![Message::Text(
            String::from_utf8(bytes).expect("utf8").into(),
        )]);
        let failure = read_first_frame(&mut stream).await.expect_err("refused");
        assert_eq!(failure, AuthFailure::NotHelloAuth);
    }

    #[tokio::test]
    async fn the_first_frame_may_be_the_hello_auth_frame() {
        let bytes = hello_frame("secret");
        let mut stream = message_stream(vec![Message::Text(
            String::from_utf8(bytes).expect("utf8").into(),
        )]);
        let frame = read_first_frame(&mut stream).await.expect("accepted");
        assert_eq!(frame.message_type, MessageType::HelloAuth);
        let request: AuthRequest = frame.payload_of().expect("decodes");
        assert_eq!(request.client_id, "client-1");
    }

    #[test]
    fn connection_epochs_reject_stale_frames() {
        let epochs = ConnectionEpochs::new();
        let session = SessionId::new();
        assert_eq!(epochs.bump(&session), 1);
        assert!(epochs.accepts(&session, Some(1)));
        assert_eq!(epochs.bump(&session), 2);
        assert!(!epochs.accepts(&session, Some(1)));
        assert!(epochs.accepts(&session, Some(2)));
        assert!(epochs.accepts(&session, None));
    }

    #[test]
    fn a_frame_of_an_unknown_session_is_refused() {
        let epochs = ConnectionEpochs::new();
        let session = SessionId::new();
        assert!(!epochs.accepts(&session, Some(1)));
    }

    #[test]
    fn three_missed_heartbeats_mark_the_session_offline() {
        let start = Instant::now();
        let mut tracker = HeartbeatTracker::new(start);
        assert!(!tracker.is_offline(start + Duration::from_secs(10)));
        assert_eq!(tracker.missed(start + Duration::from_secs(10)), 2);
        assert!(tracker.is_offline(start + Duration::from_secs(15)));
        assert_eq!(tracker.missed(start + Duration::from_secs(15)), 3);
        tracker.record(start + Duration::from_secs(15));
        assert!(!tracker.is_offline(start + Duration::from_secs(15)));
        assert_eq!(tracker.missed(start + Duration::from_secs(15)), 0);
    }

    #[test]
    fn the_protocol_version_must_match_the_major_version() {
        assert!(protocol_version_matches(PROTOCOL_VERSION));
        assert!(protocol_version_matches("1.9"));
        assert!(!protocol_version_matches("2.0"));
        assert!(!protocol_version_matches(""));
    }

    #[test]
    fn daemon_capabilities_cover_every_operation() {
        let capabilities = daemon_capabilities();
        assert_eq!(capabilities.len(), 10);
        assert!(capabilities
            .get(BridgeOp::ExportAssets.as_str())
            .is_some_and(CapabilityState::is_available));
    }

    fn hub_with_pending(
        session_id: SessionId,
        request_id: &str,
    ) -> (BridgeHub, oneshot::Receiver<BridgeOutcome>) {
        let hub = BridgeHub::new();
        let answer = hub.expect(request_id, Some(session_id));
        (hub, answer)
    }

    #[tokio::test]
    async fn a_revoked_session_drops_its_pending_work() {
        let session_id = SessionId::new();
        let (hub, answer) = hub_with_pending(session_id, "req_1");

        let cancelled = hub.revoke(&session_id);

        assert_eq!(cancelled, 1);
        assert!(hub.is_revoked(&session_id));
        let outcome = answer.await.expect("answered");
        match outcome {
            BridgeOutcome::Error(err) => assert_eq!(err.code, ErrorCode::Revoked),
            BridgeOutcome::Result(_) => panic!("the pending work must not succeed"),
        }
    }

    #[tokio::test]
    async fn a_dead_plugin_fails_the_pending_work() {
        let session_id = SessionId::new();
        let (hub, answer) = hub_with_pending(session_id, "req_2");

        let failed = hub.fail_session(&session_id, "the plugin stopped answering");

        assert_eq!(failed, 1);
        let outcome = answer.await.expect("answered");
        match outcome {
            BridgeOutcome::Error(err) => assert_eq!(err.code, ErrorCode::PluginOffline),
            BridgeOutcome::Result(_) => panic!("the pending work must not succeed"),
        }
    }

    #[tokio::test]
    async fn a_request_without_a_session_is_not_failed_by_a_session() {
        let session_id = SessionId::new();
        let hub = BridgeHub::new();
        let answer = hub.expect("req_3", None);

        assert_eq!(hub.fail_session(&session_id, "offline"), 0);
        assert!(answer.is_empty(), "the request still waits for its answer");
    }

    #[tokio::test]
    async fn a_frame_of_a_revoked_session_is_not_answered() {
        let session_id = SessionId::new();
        let (hub, _answer) = hub_with_pending(session_id, "req_4");
        hub.revoke(&session_id);
        assert!(hub.link(&session_id).is_none());
    }
}
