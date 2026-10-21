//! The harness daemon.
//!
//! This module is a **stand-in** for `figtether-daemon`. It is not the
//! shipped daemon and it never claims to be: the shipped daemon is exercised
//! by the acceptance tests of `crates/e2e/tests/acceptance.rs` through the
//! [`crate::shipped::ShippedDaemon`] helper.
//!
//! The stand-in exists because the shipped daemon does not implement the
//! following behaviours yet, and the acceptance flows of the catalog need
//! them. Only this module can produce them:
//!
//! 1. **The snapshot surface.** The shipped daemon answers
//!    `get_snapshot_info` with `CAPABILITY_UNAVAILABLE` and keeps no
//!    snapshot store, so it cannot capture a snapshot, cannot answer a read
//!    over a snapshot and cannot show that a snapshot stays stable while the
//!    document changes.
//! 2. **The grant bound asset endpoint.** The shipped daemon serves no
//!    `/assets/{asset_id}` route, so the rule that an asset of one grant is
//!    never served to another session has no other place to be proven.
//! 3. **The cached read path.** The shipped daemon never fills its design
//!    cache, so it cannot answer a cached read with `freshness: cache_hit`
//!    and a cache age.
//! 4. **The opaque traversal cursor.** The shipped daemon neither mints a
//!    cursor nor validates one, so the rule that a cursor of another
//!    session is refused with `CURSOR_EXPIRED` has no other place to be
//!    proven.
//! 5. **The scope filter and the request limits.** The shipped daemon
//!    forwards the parameters of the client to the plugin without checking
//!    the depth, the batch size or the membership of the root, so the
//!    `INVALID_LIMIT` and `OUT_OF_SCOPE` answers have no other place to be
//!    proven.
//! 6. **The chunked transfer.** The shipped daemon logs an incoming chunk
//!    and never assembles the chunks of a transfer.
//!
//! Everything else the acceptance flows need, the loopback binding, the
//! pairing secret, the connection epochs, the heartbeat, the grant update
//! and the request routing, is covered by the shipped daemon itself.
//!
//! The daemon binds `127.0.0.1` only and never opens an outbound connection,
//! which is what `AT01` and `AT02` assert.

use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use chrono::{DateTime, Utc};
use figtether_core::limits::ResourceLimits;
use figtether_core::scope::NodeIndex;
use figtether_core::state::{transition, SessionEvent, SessionState};
use figtether_model::capabilities::CapabilityState;
use figtether_model::envelope::{
    Consistency, Coverage, Envelope, Freshness, Observation, SourceKind, SourceRef,
};
use figtether_model::errors::{ErrorCode, FigError};
use figtether_model::ids::{AssetId, ObservationId, SessionId, SnapshotId};
use figtether_model::tools::{
    AssetFormat, ConsistencyRequest, ExportAssetsParams, GetDesignTokensParams, GetNodesParams,
    GetSelectionParams, GetSnapshotInfoParams, InspectDesignParams, ListNodesParams,
};
use figtether_protocol::client::{ClientAuth, ClientRequest, ClientResponse};
use figtether_protocol::plugin::PluginNode;
use figtether_protocol::{
    AuthAccepted, AuthRequest, BridgeFrame, BridgeOp, CapabilitiesPayload, ChunkAssembler,
    ChunkPayload, ErrorPayload, GrantUpdatePayload, MessageType, ProtocolError, RequestPayload,
    ResultPayload, RevokePayload, AUTH_WINDOW, MAX_FRAME_BYTES, MAX_TRANSFER_BYTES,
    PROTOCOL_VERSION,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};

use crate::fake_plugin::{SESSION_MODE_LIVE, SESSION_MODE_PAUSED};
use crate::harness::HarnessError;

/// Path of the bridge endpoint.
pub const BRIDGE_PATH: &str = "/bridge";
/// Path of the local client endpoint.
pub const CLIENT_PATH: &str = "/client";
/// Path of the health endpoint.
pub const HEALTH_PATH: &str = "/health";
/// Path prefix of the grant bound asset endpoint.
pub const ASSET_PATH: &str = "/assets";

/// Time to live of a pairing secret.
pub const SECRET_TTL: Duration = Duration::from_secs(5 * 60);

/// Close code used when a connection is refused.
const CLOSE_POLICY_VIOLATION: u16 = 1008;

/// Tuning of the harness daemon.
///
/// The defaults match the production profile of
/// `docs/02-architecture/02-bridge-design.md`. [`DaemonProfile::fast`]
/// compresses the heartbeat cadence so the connection disturbance scenarios
/// stay fast without changing any rule.
#[derive(Debug, Clone)]
pub struct DaemonProfile {
    /// Resource budgets of the daemon.
    pub limits: ResourceLimits,
    /// Interval between heartbeats.
    pub heartbeat_interval: Duration,
    /// Number of missed heartbeats before a session is offline.
    pub missed_heartbeats: u32,
    /// Time the daemon waits for the first authentication frame.
    pub auth_window: Duration,
    /// Time to live of a pairing secret.
    pub secret_ttl: Duration,
    /// Number of nodes the plugin returns per traversal page.
    pub page_size: u32,
}

impl Default for DaemonProfile {
    fn default() -> Self {
        Self {
            limits: ResourceLimits::default(),
            heartbeat_interval: Duration::from_secs(5),
            missed_heartbeats: 3,
            auth_window: AUTH_WINDOW,
            secret_ttl: SECRET_TTL,
            page_size: 200,
        }
    }
}

impl DaemonProfile {
    /// A profile with the same rules but a compressed heartbeat cadence.
    ///
    /// The read and export deadlines are shortened as well, so a request that
    /// never gets an answer fails fast instead of blocking a test for the
    /// full production budget.
    pub fn fast() -> Self {
        let limits = ResourceLimits {
            read_timeout: Duration::from_secs(2),
            export_timeout: Duration::from_secs(5),
            ..ResourceLimits::default()
        };
        Self {
            limits,
            heartbeat_interval: Duration::from_millis(200),
            missed_heartbeats: 3,
            auth_window: AUTH_WINDOW,
            secret_ttl: SECRET_TTL,
            page_size: 200,
        }
    }
}

/// A pairing secret.
#[derive(Debug, Clone)]
struct SecretRecord {
    secret: String,
    expires_at: Instant,
    consumed: bool,
}

/// A local client that authenticated on the client endpoint.
#[derive(Debug, Clone)]
struct ClientRecord {
    client_id: String,
}

/// One plugin session.
#[derive(Debug)]
struct SessionRecord {
    session_id: SessionId,
    client_id: String,
    state: SessionState,
    connection_epoch: u64,
    grant_epoch: u64,
    root_ids: Vec<String>,
    capabilities: BTreeMap<String, CapabilityState>,
    to_plugin: Mutex<Option<mpsc::Sender<BridgeFrame>>>,
    to_plugin_rx: Mutex<Option<mpsc::Receiver<BridgeFrame>>>,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<BridgeFrame>>>>,
    transfers: Arc<Mutex<HashMap<String, ChunkAssembler>>>,
    node_index: Arc<Mutex<NodeIndex>>,
    last_seen: Arc<Mutex<Instant>>,
}

/// An immutable capture.
#[derive(Debug, Clone)]
struct SnapshotRecord {
    snapshot_id: SnapshotId,
    session_id: SessionId,
    grant_epoch: u64,
    nodes: Vec<PluginNode>,
    content_hash: String,
    captured_at: DateTime<Utc>,
}

/// An exported asset bound to one grant.
#[derive(Debug, Clone)]
struct AssetRecord {
    session_id: SessionId,
    grant_epoch: u64,
    mime: String,
    width: u32,
    height: u32,
    bytes: Vec<u8>,
    hash: String,
}

/// One cache entry.
#[derive(Debug, Clone)]
struct CacheRecord {
    session_id: SessionId,
    grant_epoch: u64,
    envelope: Envelope,
    inserted_at: Instant,
}

/// One pagination cursor.
#[derive(Debug, Clone)]
struct CursorRecord {
    session_id: SessionId,
    grant_epoch: u64,
    root_id: String,
    last_node_id: String,
}

/// Counters of the harness daemon.
#[derive(Debug, Default)]
struct Counters {
    stale_frames: AtomicU64,
    oversized_frames: AtomicU64,
    requests: AtomicU64,
}

/// Shared state of the harness daemon.
struct DaemonState {
    instance_id: String,
    addr: SocketAddr,
    profile: DaemonProfile,
    secrets: Mutex<Vec<SecretRecord>>,
    clients: Mutex<HashMap<String, ClientRecord>>,
    sessions: Mutex<HashMap<SessionId, SessionRecord>>,
    snapshots: Mutex<HashMap<SnapshotId, SnapshotRecord>>,
    assets: Mutex<HashMap<AssetId, AssetRecord>>,
    cache: Mutex<HashMap<String, CacheRecord>>,
    cursors: Mutex<HashMap<String, CursorRecord>>,
    counters: Counters,
}

impl DaemonState {
    fn new(profile: DaemonProfile, addr: SocketAddr) -> Self {
        Self {
            instance_id: format!("daemon-{}", uuid::Uuid::new_v4()),
            addr,
            profile,
            secrets: Mutex::new(Vec::new()),
            clients: Mutex::new(HashMap::new()),
            sessions: Mutex::new(HashMap::new()),
            snapshots: Mutex::new(HashMap::new()),
            assets: Mutex::new(HashMap::new()),
            cache: Mutex::new(HashMap::new()),
            cursors: Mutex::new(HashMap::new()),
            counters: Counters::default(),
        }
    }

    fn mint_secret(&self, ttl: Duration) -> String {
        let secret = format!("ftsecret-{}", uuid::Uuid::new_v4().simple());
        let mut secrets = self.secrets.lock().expect("secret lock");
        secrets.push(SecretRecord {
            secret: secret.clone(),
            expires_at: Instant::now() + ttl,
            consumed: false,
        });
        secret
    }

    fn consume_secret(&self, secret: &str) -> Result<(), FigError> {
        let mut secrets = self.secrets.lock().expect("secret lock");
        let Some(record) = secrets.iter_mut().find(|record| record.secret == secret) else {
            return Err(FigError::new(
                ErrorCode::AuthRequired,
                "the pairing secret is unknown",
            ));
        };
        if record.consumed {
            return Err(FigError::new(
                ErrorCode::AuthRequired,
                "the pairing secret was already used",
            ));
        }
        if Instant::now() > record.expires_at {
            return Err(FigError::new(
                ErrorCode::AuthExpired,
                "the pairing secret expired, pair again",
            ));
        }
        record.consumed = true;
        Ok(())
    }

    fn authenticate_client(&self, auth: &ClientAuth) -> Result<String, FigError> {
        if auth.protocol_version != PROTOCOL_VERSION {
            return Err(FigError::new(
                ErrorCode::ProtocolMismatch,
                "the client speaks a different protocol version",
            ));
        }
        let clients = self.clients.lock().expect("client lock");
        match clients.get(&auth.credential) {
            Some(record) => Ok(record.client_id.clone()),
            None => Err(FigError::new(
                ErrorCode::AuthRequired,
                "the client credential is unknown, restart the client",
            )),
        }
    }

    fn register_client(&self, credential: &str, client_id: &str) {
        let mut clients = self.clients.lock().expect("client lock");
        clients.insert(
            credential.to_string(),
            ClientRecord {
                client_id: client_id.to_string(),
            },
        );
    }

    /// Authenticate a plugin connection and create the session.
    fn authenticate_plugin(
        &self,
        auth: &AuthRequest,
    ) -> Result<(SessionId, u64, BTreeMap<String, CapabilityState>), FigError> {
        if auth.protocol_version != PROTOCOL_VERSION {
            return Err(FigError::new(
                ErrorCode::ProtocolMismatch,
                "the plugin speaks a different protocol version",
            ));
        }
        if auth.secret.len() < 16 {
            return Err(FigError::new(
                ErrorCode::AuthRequired,
                "the pairing secret has an unexpected length",
            ));
        }
        self.consume_secret(&auth.secret)?;

        let (sender, receiver) = mpsc::channel::<BridgeFrame>(64);
        let session_id = SessionId::new();
        let mut capabilities = BTreeMap::new();
        capabilities.insert("nodes".to_string(), CapabilityState::Available);
        capabilities.insert("variables".to_string(), CapabilityState::Available);
        capabilities.insert("export_png".to_string(), CapabilityState::Available);

        {
            let mut sessions = self.sessions.lock().expect("session lock");
            // A new connection of the same client replaces the sessions that
            // went offline, so an old session id stops resolving after a
            // reopen of the plugin.
            let stale: Vec<SessionId> = sessions
                .iter()
                .filter(|(_, session)| {
                    session.client_id == auth.client_id && session.state == SessionState::Offline
                })
                .map(|(id, _)| *id)
                .collect();
            for id in stale {
                sessions.remove(&id);
            }
            sessions.insert(
                session_id,
                SessionRecord {
                    session_id,
                    client_id: auth.client_id.clone(),
                    state: SessionState::Unpaired,
                    connection_epoch: 0,
                    grant_epoch: 0,
                    root_ids: Vec::new(),
                    capabilities,
                    to_plugin: Mutex::new(Some(sender)),
                    to_plugin_rx: Mutex::new(Some(receiver)),
                    pending: Arc::new(Mutex::new(HashMap::new())),
                    transfers: Arc::new(Mutex::new(HashMap::new())),
                    node_index: Arc::new(Mutex::new(NodeIndex::new())),
                    last_seen: Arc::new(Mutex::new(Instant::now())),
                },
            );
        }
        // The first frame authenticated, so the session leaves `Unpaired`.
        self.apply_event(&session_id, SessionEvent::Authenticated)?;
        let sessions = self.sessions.lock().expect("session lock");
        let session = sessions.get(&session_id).expect("session exists");
        Ok((
            session_id,
            session.connection_epoch,
            session.capabilities.clone(),
        ))
    }

    fn session_epoch(&self, session_id: &SessionId) -> Option<u64> {
        let sessions = self.sessions.lock().expect("session lock");
        sessions
            .get(session_id)
            .map(|session| session.connection_epoch)
    }

    fn touch(&self, session_id: &SessionId) {
        let sessions = self.sessions.lock().expect("session lock");
        if let Some(session) = sessions.get(session_id) {
            if let Ok(mut last_seen) = session.last_seen.lock() {
                *last_seen = Instant::now();
            }
        }
    }

    /// Apply a state transition of the frozen core state machine.
    fn apply_event(
        &self,
        session_id: &SessionId,
        event: SessionEvent,
    ) -> Result<SessionState, FigError> {
        let mut sessions = self.sessions.lock().expect("session lock");
        let session = sessions.get_mut(session_id).ok_or_else(|| {
            FigError::new(ErrorCode::SessionNotFound, "the session does not exist")
        })?;
        let result = transition(session.state, event)?;
        session.state = result.to;
        if result.bump_grant_epoch {
            session.grant_epoch += 1;
        }
        if result.bump_connection_epoch {
            session.connection_epoch += 1;
        }
        Ok(session.state)
    }

    /// Create or replace the grant of a session.
    ///
    /// The function answers the effective session mode, so the plugin
    /// follows the daemon: the daemon is the authority on the pause.
    fn apply_grant(
        &self,
        session_id: &SessionId,
        payload: GrantUpdatePayload,
    ) -> Result<String, FigError> {
        let session_mode = payload.session_mode.clone();
        let mut sessions = self.sessions.lock().expect("session lock");
        let session = sessions.get_mut(session_id).ok_or_else(|| {
            FigError::new(ErrorCode::SessionNotFound, "the session does not exist")
        })?;
        // A session that comes back to life is resumed before the grant is
        // applied: a paused session refuses a new grant.
        if session_mode.as_deref() == Some(SESSION_MODE_LIVE)
            && session.state == SessionState::Paused
        {
            let result = transition(session.state, SessionEvent::Resumed)?;
            session.state = result.to;
            if result.bump_grant_epoch {
                session.grant_epoch += 1;
            }
        }
        match session.state {
            SessionState::Paired => {
                let result = transition(session.state, SessionEvent::GrantCreated)?;
                session.state = result.to;
                session.grant_epoch += 1;
            }
            SessionState::Ready | SessionState::AwaitingGrant => {
                // A scope change of a live session bumps the grant epoch.
                session.grant_epoch += 1;
            }
            SessionState::Revoked => {
                return Err(FigError::new(
                    ErrorCode::Revoked,
                    "the grant was revoked; only the user can create a new one",
                ))
            }
            _ => {
                return Err(FigError::new(
                    ErrorCode::SessionNotFound,
                    "the session is not serving reads",
                ))
            }
        }
        session.root_ids = payload.root_ids.clone();
        // The frozen core state machine has no edge into `Ready`. The daemon
        // owns that edge, so the state is set here. The gap is reported to
        // the core owner.
        session.state = SessionState::Ready;
        // The user may pause the session in the same frame.
        if session_mode.as_deref() == Some(SESSION_MODE_PAUSED) {
            let result = transition(session.state, SessionEvent::Paused)?;
            session.state = result.to;
            if result.bump_grant_epoch {
                session.grant_epoch += 1;
            }
        }
        Ok(match session.state {
            SessionState::Paused => SESSION_MODE_PAUSED.to_string(),
            _ => SESSION_MODE_LIVE.to_string(),
        })
    }

    fn mark_offline(&self, session_id: &SessionId) {
        let mut sessions = self.sessions.lock().expect("session lock");
        let Some(session) = sessions.get_mut(session_id) else {
            return;
        };
        if session.state == SessionState::Offline {
            return;
        }
        if let Ok(result) = transition(session.state, SessionEvent::HeartbeatLost) {
            session.state = result.to;
        }
    }

    /// Revoke a grant and purge everything derived from it.
    fn revoke(&self, session_id: &SessionId) -> Result<(), FigError> {
        let state = self.apply_event(session_id, SessionEvent::Revoked)?;
        if state != SessionState::Revoked {
            return Err(FigError::new(
                ErrorCode::Revoked,
                "the grant was revoked; only the user can create a new one",
            ));
        }
        let error = FigError::new(
            ErrorCode::Revoked,
            "the grant was revoked; only the user can create a new one",
        );
        self.fail_pending(session_id, &error);
        self.purge_session(session_id);
        self.notify_plugin_of_revoke(session_id);
        Ok(())
    }

    /// Drop the cache entries, the assets and the cursors of one session.
    fn purge_session(&self, session_id: &SessionId) {
        self.cache
            .lock()
            .expect("cache lock")
            .retain(|_, record| record.session_id != *session_id);
        self.assets
            .lock()
            .expect("asset lock")
            .retain(|_, record| record.session_id != *session_id);
        self.cursors
            .lock()
            .expect("cursor lock")
            .retain(|_, record| record.session_id != *session_id);
        self.snapshots
            .lock()
            .expect("snapshot lock")
            .retain(|_, record| record.session_id != *session_id);
    }

    /// Fail every request that is still in flight for a session.
    fn fail_pending(&self, session_id: &SessionId, error: &FigError) {
        let sessions = self.sessions.lock().expect("session lock");
        let Some(session) = sessions.get(session_id) else {
            return;
        };
        let mut pending = session.pending.lock().expect("pending lock");
        for (_, sender) in pending.drain() {
            let _ = sender.send(error_frame(&error.code, &error.message, &error.action));
        }
        session.transfers.lock().expect("transfer lock").clear();
    }

    fn notify_plugin_of_revoke(&self, session_id: &SessionId) {
        let sessions = self.sessions.lock().expect("session lock");
        let Some(session) = sessions.get(session_id) else {
            return;
        };
        let sender = session.to_plugin.lock().expect("channel lock").clone();
        if let Some(sender) = sender {
            let frame = BridgeFrame::new(
                MessageType::Revoke,
                format!("revoke-{session_id}"),
                serde_json::to_value(RevokePayload {
                    reason: "the user revoked the grant".to_string(),
                })
                .unwrap_or(Value::Null),
            )
            .with_session(*session_id);
            let _ = sender.try_send(frame);
        }
    }

    /// Send a request to the plugin and wait for the answer.
    async fn dispatch(
        &self,
        session_id: &SessionId,
        op: BridgeOp,
        params: Value,
        timeout: Duration,
    ) -> Result<BridgeFrame, FigError> {
        let request_id = format!("req-{}", uuid::Uuid::new_v4());
        let (sender, epoch, grant_epoch) = {
            let sessions = self.sessions.lock().expect("session lock");
            let session = sessions.get(session_id).ok_or_else(|| {
                FigError::new(ErrorCode::SessionNotFound, "the session does not exist")
            })?;
            let sender = session
                .to_plugin
                .lock()
                .expect("channel lock")
                .clone()
                .ok_or_else(|| {
                    FigError::new(ErrorCode::PluginOffline, "the plugin is not connected")
                })?;
            (sender, session.connection_epoch, session.grant_epoch)
        };
        let (tx, rx) = oneshot::channel();
        {
            let sessions = self.sessions.lock().expect("session lock");
            let session = sessions.get(session_id).expect("session exists");
            session
                .pending
                .lock()
                .expect("pending lock")
                .insert(request_id.clone(), tx);
        }
        let payload = serde_json::to_value(RequestPayload { op, params })
            .map_err(|err| FigError::new(ErrorCode::InternalError, err.to_string()))?;
        let frame = BridgeFrame::new(MessageType::Request, request_id.clone(), payload)
            .with_session(*session_id)
            .with_connection_epoch(epoch)
            .with_grant_epoch(grant_epoch)
            .with_deadline_ms(timeout.as_millis() as u64);
        if sender.send(frame).await.is_err() {
            self.drop_pending(session_id, &request_id);
            return Err(FigError::new(
                ErrorCode::PluginOffline,
                "the plugin closed the connection",
            ));
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(frame)) => Ok(frame),
            Ok(Err(_)) => {
                self.drop_pending(session_id, &request_id);
                Err(FigError::new(
                    ErrorCode::PluginOffline,
                    "the plugin closed the connection",
                ))
            }
            Err(_) => {
                self.drop_pending(session_id, &request_id);
                Err(FigError::new(
                    ErrorCode::Timeout,
                    "the plugin did not answer before the deadline",
                ))
            }
        }
    }

    fn drop_pending(&self, session_id: &SessionId, request_id: &str) {
        let sessions = self.sessions.lock().expect("session lock");
        if let Some(session) = sessions.get(session_id) {
            session
                .pending
                .lock()
                .expect("pending lock")
                .remove(request_id);
        }
    }

    fn resolve_pending(&self, session_id: &SessionId, request_id: &str, frame: BridgeFrame) {
        let sessions = self.sessions.lock().expect("session lock");
        let Some(session) = sessions.get(session_id) else {
            return;
        };
        let sender = session
            .pending
            .lock()
            .expect("pending lock")
            .remove(request_id);
        if let Some(sender) = sender {
            let _ = sender.send(frame);
        }
    }

    /// Handle one frame that arrived from the plugin.
    async fn handle_plugin_frame(
        &self,
        session_id: &SessionId,
        frame: &BridgeFrame,
        outbound: &mpsc::Sender<BridgeFrame>,
    ) {
        self.touch(session_id);
        match frame.message_type {
            MessageType::Result | MessageType::Error => {
                if let Some(epoch) = frame.connection_epoch {
                    if Some(epoch) != self.session_epoch(session_id) {
                        self.counters.stale_frames.fetch_add(1, Ordering::Relaxed);
                        return;
                    }
                }
                self.resolve_pending(session_id, &frame.request_id, frame.clone());
            }
            MessageType::Chunk => {
                let payload: ChunkPayload = match frame.payload_of() {
                    Ok(payload) => payload,
                    Err(_) => return,
                };
                let sessions = self.sessions.lock().expect("session lock");
                let Some(session) = sessions.get(session_id) else {
                    return;
                };
                let mut transfers = session.transfers.lock().expect("transfer lock");
                if !transfers.contains_key(&frame.request_id) {
                    transfers.insert(
                        frame.request_id.clone(),
                        ChunkAssembler::new(payload.transfer_id.clone(), MAX_TRANSFER_BYTES),
                    );
                }
                let assembler = transfers
                    .get_mut(&frame.request_id)
                    .expect("assembler exists");
                match assembler.accept(&payload) {
                    Ok(Some(bytes)) => {
                        transfers.remove(&frame.request_id);
                        drop(transfers);
                        let data = json!({
                            "bytes": base64_encode(&bytes),
                            "size": bytes.len(),
                        });
                        let result = BridgeFrame::new(
                            MessageType::Result,
                            frame.request_id.clone(),
                            serde_json::to_value(ResultPayload { data }).unwrap_or(Value::Null),
                        );
                        self.resolve_pending(session_id, &frame.request_id, result);
                    }
                    Ok(None) => {}
                    Err(ProtocolError::TransferTooLarge { .. }) => {
                        transfers.remove(&frame.request_id);
                        drop(transfers);
                        let error = error_frame(
                            &ErrorCode::PayloadTooLarge,
                            "the transfer exceeded the byte budget",
                            "narrow the request",
                        );
                        self.resolve_pending(session_id, &frame.request_id, error);
                    }
                    Err(_) => {
                        transfers.remove(&frame.request_id);
                        drop(transfers);
                        let error = error_frame(
                            &ErrorCode::InternalError,
                            "the transfer was aborted",
                            "retry once",
                        );
                        self.resolve_pending(session_id, &frame.request_id, error);
                    }
                }
            }
            MessageType::Heartbeat => {}
            MessageType::GrantUpdate => {
                let payload: GrantUpdatePayload = match frame.payload_of() {
                    Ok(payload) => payload,
                    Err(_) => return,
                };
                if let Ok(session_mode) = self.apply_grant(session_id, payload.clone()) {
                    let grant_epoch = {
                        let sessions = self.sessions.lock().expect("session lock");
                        let Some(session) = sessions.get(session_id) else {
                            return;
                        };
                        session.grant_epoch
                    };
                    let reply = BridgeFrame::new(
                        MessageType::GrantUpdate,
                        frame.request_id.clone(),
                        serde_json::to_value(GrantUpdatePayload {
                            root_ids: payload.root_ids,
                            grant_epoch,
                            mode: payload.mode,
                            session_mode: Some(session_mode),
                        })
                        .unwrap_or(Value::Null),
                    );
                    let _ = outbound.send(reply).await;
                }
            }
            MessageType::Capabilities => {
                let payload: CapabilitiesPayload = match frame.payload_of() {
                    Ok(payload) => payload,
                    Err(_) => return,
                };
                let mut sessions = self.sessions.lock().expect("session lock");
                if let Some(session) = sessions.get_mut(session_id) {
                    session.capabilities = payload.capabilities;
                }
            }
            MessageType::Revoke => {
                let _ = self.revoke(session_id);
            }
            MessageType::Request => {
                // A request from the plugin side keeps the frame ordering of
                // one connection.
                let _ = outbound.send(frame.clone()).await;
            }
            MessageType::Cancel | MessageType::Progress | MessageType::HelloAuth => {}
            MessageType::Approval | MessageType::UiControl => {}
        }
    }

    fn status_payload(&self) -> Value {
        let sessions = self.sessions.lock().expect("session lock");
        json!({
            "daemon_instance_id": self.instance_id,
            "protocol_version": PROTOCOL_VERSION,
            "schema_version": figtether_model::SCHEMA_VERSION,
            "sessions": sessions.len(),
            "limits": {
                "max_depth": self.profile.limits.max_depth,
                "max_batch_node_ids": self.profile.limits.max_batch_node_ids,
                "max_json_response_bytes": self.profile.limits.max_json_response_bytes,
                "max_traversal_nodes": self.profile.limits.max_traversal_nodes,
            },
        })
    }

    fn sessions_payload(&self, client_id: &str) -> Value {
        let sessions = self.sessions.lock().expect("session lock");
        let mut list = Vec::new();
        for session in sessions.values() {
            if session.client_id != client_id {
                continue;
            }
            list.push(json!({
                "session_id": session.session_id,
                "client_id": session.client_id,
                "state": session.state.as_str(),
                "grant_epoch": session.grant_epoch,
                "connection_epoch": session.connection_epoch,
                "root_ids": session.root_ids,
                "capabilities": session.capabilities,
            }));
        }
        json!({ "sessions": list })
    }

    /// Handle one request of the local client endpoint.
    async fn handle_client_request(
        &self,
        client_id: &str,
        request: ClientRequest,
    ) -> ClientResponse {
        self.counters.requests.fetch_add(1, Ordering::Relaxed);
        match request {
            ClientRequest::Status { request_id } => ok_response(request_id, self.status_payload()),
            ClientRequest::PairSecret { request_id } => {
                let secret = self.mint_secret(self.profile.secret_ttl);
                ok_response(
                    request_id,
                    json!({
                        "secret": secret,
                        "expires_in_ms": self.profile.secret_ttl.as_millis() as u64,
                    }),
                )
            }
            ClientRequest::ListSessions { request_id, .. } => {
                ok_response(request_id, self.sessions_payload(client_id))
            }
            ClientRequest::ListNodes { request_id, params } => {
                self.list_nodes(client_id, request_id, params).await
            }
            ClientRequest::GetNodes { request_id, params } => {
                self.get_nodes(client_id, request_id, params).await
            }
            ClientRequest::InspectDesign { request_id, params } => {
                self.inspect_design(client_id, request_id, params).await
            }
            ClientRequest::GetDesignTokens { request_id, params } => {
                self.get_design_tokens(client_id, request_id, params).await
            }
            ClientRequest::ExportAssets { request_id, params } => {
                self.export_assets(client_id, request_id, params).await
            }
            ClientRequest::GetSelection { request_id, params } => {
                self.get_selection(client_id, request_id, params).await
            }
            ClientRequest::GetAsset { request_id, .. } => error_response(
                request_id,
                FigError::new(
                    ErrorCode::CapabilityUnavailable,
                    "use shipped daemon asset tests",
                ),
            ),
            ClientRequest::GetSnapshotInfo { request_id, params } => {
                self.get_snapshot_info(request_id, params)
            }
            ClientRequest::WatchScope { request_id, .. }
            | ClientRequest::GetChanges { request_id, .. }
            | ClientRequest::GetJobStatus { request_id, .. } => error_response(
                request_id,
                FigError::new(
                    ErrorCode::CapabilityUnavailable,
                    "the phase P1 tools are not enabled in this build",
                ),
            ),
            ClientRequest::PreviewWrite { request_id, .. }
            | ClientRequest::ApplyWrite { request_id, .. }
            | ClientRequest::GetWriteApproval { request_id, .. } => error_response(
                request_id,
                FigError::new(
                    ErrorCode::CapabilityUnavailable,
                    "the write path is not enabled in this build",
                ),
            ),
            ClientRequest::Cancel { request_id, .. } => error_response(
                request_id,
                FigError::new(
                    ErrorCode::CapabilityUnavailable,
                    "job cancellation is not enabled in this build",
                ),
            ),
            ClientRequest::Revoke {
                request_id,
                session_id,
            } => match self.revoke(&session_id) {
                Ok(()) => ok_response(
                    request_id,
                    json!({ "session_id": session_id, "state": SessionState::Revoked.as_str() }),
                ),
                Err(error) => error_response(request_id, error),
            },
        }
    }

    /// Resolve the source of a read: a live session or an immutable capture.
    ///
    /// `root_id` is checked against the granted scope when it is known. A
    /// request that names node ids instead of a root is checked after the
    /// plugin answered, because the daemon only learns the parent relations
    /// of a node when the plugin reports them.
    fn resolve_source(
        &self,
        client_id: &str,
        session_id: Option<SessionId>,
        snapshot_id: Option<SnapshotId>,
        root_id: Option<&str>,
    ) -> Result<ReadSource, FigError> {
        if let Some(snapshot_id) = snapshot_id {
            let snapshots = self.snapshots.lock().expect("snapshot lock");
            let record = snapshots.get(&snapshot_id).ok_or_else(|| {
                FigError::new(ErrorCode::SessionNotFound, "the snapshot does not exist")
            })?;
            return Ok(ReadSource::Snapshot(record.clone()));
        }
        let session_id = session_id.ok_or_else(|| {
            FigError::new(
                ErrorCode::InvalidLimit,
                "a read names either a session or a snapshot",
            )
        })?;
        let sessions = self.sessions.lock().expect("session lock");
        let session = sessions.get(&session_id).ok_or_else(|| {
            FigError::new(ErrorCode::SessionNotFound, "the session does not exist")
        })?;
        if session.client_id != client_id {
            return Err(FigError::new(
                ErrorCode::PermissionDenied,
                "the session belongs to another client",
            ));
        }
        match session.state {
            SessionState::Ready => {}
            SessionState::Paused => {
                return Err(FigError::new(
                    ErrorCode::Paused,
                    "the user paused the session",
                ))
            }
            SessionState::Revoked => {
                return Err(FigError::new(
                    ErrorCode::Revoked,
                    "the grant was revoked; only the user can create a new one",
                ))
            }
            SessionState::Offline => {
                return Err(FigError::new(
                    ErrorCode::PluginOffline,
                    "the plugin is not connected",
                ))
            }
            _ => {
                return Err(FigError::new(
                    ErrorCode::SessionNotFound,
                    "the session is not serving reads",
                ))
            }
        }
        let roots = session.root_ids.clone();
        if let Some(root_id) = root_id {
            let index = session.node_index.lock().expect("index lock");
            let in_scope = roots
                .iter()
                .any(|root| root == root_id || index.contains(root, root_id));
            if !in_scope {
                return Err(FigError::new(
                    ErrorCode::OutOfScope,
                    format!("node {root_id} is outside the granted scope"),
                ));
            }
        }
        Ok(ReadSource::Live(LiveSource {
            session_id,
            grant_epoch: session.grant_epoch,
            connection_epoch: session.connection_epoch,
            root_ids: roots,
        }))
    }

    fn validate_cursor(
        &self,
        cursor: Option<&str>,
        source: &LiveSource,
        root_id: &str,
    ) -> Result<Option<String>, FigError> {
        let Some(cursor) = cursor else {
            return Ok(None);
        };
        let cursors = self.cursors.lock().expect("cursor lock");
        let record = cursors.get(cursor).ok_or_else(|| {
            FigError::new(
                ErrorCode::CursorExpired,
                "the cursor is unknown, restart the traversal",
            )
        })?;
        if record.session_id != source.session_id {
            return Err(FigError::new(
                ErrorCode::CursorExpired,
                "the cursor belongs to another session, restart the traversal",
            ));
        }
        if record.grant_epoch != source.grant_epoch {
            return Err(FigError::new(
                ErrorCode::CursorExpired,
                "the grant changed, restart the traversal",
            ));
        }
        if record.root_id != root_id {
            return Err(FigError::new(
                ErrorCode::CursorExpired,
                "the cursor belongs to another traversal, restart it",
            ));
        }
        Ok(Some(record.last_node_id.clone()))
    }

    fn store_cursor(&self, source: &LiveSource, root_id: &str, last_node_id: &str) -> String {
        let token = format!("cursor-{}", uuid::Uuid::new_v4().simple());
        let mut cursors = self.cursors.lock().expect("cursor lock");
        cursors.insert(
            token.clone(),
            CursorRecord {
                session_id: source.session_id,
                grant_epoch: source.grant_epoch,
                root_id: root_id.to_string(),
                last_node_id: last_node_id.to_string(),
            },
        );
        token
    }

    /// Record the parent relations the plugin reported and drop the nodes that
    /// are not inside the granted scope.
    fn filter_by_scope(&self, source: &LiveSource, nodes: Vec<PluginNode>) -> Vec<PluginNode> {
        let sessions = self.sessions.lock().expect("session lock");
        let Some(session) = sessions.get(&source.session_id) else {
            return Vec::new();
        };
        let mut index = session.node_index.lock().expect("index lock");
        for node in &nodes {
            if let Some(parent_id) = node.parent_id.as_ref() {
                index.insert(node.id.clone(), parent_id.clone());
            }
        }
        nodes
            .into_iter()
            .filter(|node| {
                source
                    .root_ids
                    .iter()
                    .any(|root| root == &node.id || index.contains(root, &node.id))
            })
            .collect()
    }

    /// Split a node list so the encoded payload stays inside the budget.
    ///
    /// The budget covers the whole envelope, so the node list gets the
    /// remaining space after a fixed reserve for the envelope itself.
    fn paginate(&self, nodes: Vec<PluginNode>) -> (Vec<PluginNode>, bool) {
        let reserve = 4096usize;
        let budget = self
            .profile
            .limits
            .max_json_response_bytes
            .saturating_sub(reserve);
        let mut kept: Vec<PluginNode> = Vec::new();
        let mut used = 2usize;
        for node in nodes {
            let size = serde_json::to_vec(&node)
                .map(|bytes| bytes.len())
                .unwrap_or(0)
                + 1;
            if used + size > budget && !kept.is_empty() {
                return (kept, true);
            }
            used += size;
            kept.push(node);
        }
        (kept, false)
    }

    fn cache_key(source: &LiveSource, op: BridgeOp, params: &Value) -> String {
        format!(
            "{}|{}|{}|{}",
            source.session_id,
            source.grant_epoch,
            op.as_str(),
            figtether_model::envelope::canonical_hash(params)
        )
    }

    /// Look up a cached answer for an explicitly cached read.
    ///
    /// An entry that was written under an older grant epoch is never served,
    /// because a scope change has to invalidate the cache.
    fn cache_lookup(
        &self,
        key: &str,
        source: &LiveSource,
        max_age_ms: Option<u64>,
    ) -> Option<Envelope> {
        let cache = self.cache.lock().expect("cache lock");
        let record = cache.get(key)?;
        if record.grant_epoch != source.grant_epoch {
            return None;
        }
        let age = record.inserted_at.elapsed().as_millis() as u64;
        let limit = max_age_ms.unwrap_or(self.profile.limits.live_cache_ttl.as_millis() as u64);
        if age > limit {
            return None;
        }
        let mut envelope = record.envelope.clone();
        envelope.observation.freshness = Freshness::CacheHit;
        envelope.observation.cache_age_ms = Some(age);
        Some(envelope)
    }

    fn cache_store(&self, key: String, source: &LiveSource, envelope: &Envelope) {
        let mut cache = self.cache.lock().expect("cache lock");
        cache.insert(
            key,
            CacheRecord {
                session_id: source.session_id,
                grant_epoch: source.grant_epoch,
                envelope: envelope.clone(),
                inserted_at: Instant::now(),
            },
        );
    }

    async fn list_nodes(
        &self,
        client_id: &str,
        request_id: String,
        params: Value,
    ) -> ClientResponse {
        let params: ListNodesParams = match serde_json::from_value(params) {
            Ok(params) => params,
            Err(err) => {
                return error_response(
                    request_id,
                    FigError::new(
                        ErrorCode::InvalidLimit,
                        format!("the parameters are invalid: {err}"),
                    ),
                )
            }
        };
        let source = match self.resolve_source(
            client_id,
            params.session_id,
            params.snapshot_id,
            Some(params.root_id.as_str()),
        ) {
            Ok(source) => source,
            Err(error) => return error_response(request_id, error),
        };
        if let Some(depth) = params.depth {
            if !self.profile.limits.depth_is_valid(depth) {
                return error_response(
                    request_id,
                    FigError::new(
                        ErrorCode::InvalidLimit,
                        format!("the depth {depth} exceeds the limit"),
                    ),
                );
            }
        }
        match source {
            ReadSource::Snapshot(snapshot) => {
                let (page, truncated) = self.paginate(snapshot.nodes.clone());
                let coverage = Coverage {
                    root_ids: vec![params.root_id.clone()],
                    returned_nodes: page.len() as u32,
                    truncated,
                    next_cursor: None,
                    unavailable_fields: Vec::new(),
                };
                let envelope = Envelope::new(
                    request_id.clone(),
                    SourceRef::plugin_snapshot(snapshot.snapshot_id),
                    observation(Consistency::BestEffort, Freshness::FreshRead, None),
                    coverage,
                    json!({ "nodes": page }),
                )
                .with_content_hash();
                ClientResponse::Ok {
                    request_id,
                    envelope,
                }
            }
            ReadSource::Live(source) => {
                let after = match self.validate_cursor(
                    params.cursor.as_deref(),
                    &source,
                    &params.root_id,
                ) {
                    Ok(after) => after,
                    Err(error) => return error_response(request_id, error),
                };
                let request_params = json!({
                    "root_id": params.root_id,
                    "depth": params.depth,
                    "after": after,
                    "limit": self.profile.page_size,
                });
                let cache_key = Self::cache_key(&source, BridgeOp::ListNodes, &request_params);
                if params.consistency == ConsistencyRequest::Cached {
                    if let Some(envelope) =
                        self.cache_lookup(&cache_key, &source, params.max_age_ms)
                    {
                        return ClientResponse::Ok {
                            request_id,
                            envelope,
                        };
                    }
                }
                let frame = match self
                    .dispatch(
                        &source.session_id,
                        BridgeOp::ListNodes,
                        request_params,
                        self.profile.limits.read_timeout,
                    )
                    .await
                {
                    Ok(frame) => frame,
                    Err(error) => return error_response(request_id, error),
                };
                let data = match result_data(&frame) {
                    Ok(data) => data,
                    Err(error) => return error_response(request_id, error),
                };
                let nodes: Vec<PluginNode> =
                    serde_json::from_value(data.get("nodes").cloned().unwrap_or(Value::Null))
                        .unwrap_or_default();
                let remaining = data
                    .get("remaining")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let scoped = self.filter_by_scope(&source, nodes);
                let (page, truncated) = self.paginate(scoped);
                let more = truncated || remaining;
                let next_cursor = if more {
                    page.last()
                        .map(|node| self.store_cursor(&source, &params.root_id, &node.id))
                } else {
                    None
                };
                let coverage = Coverage {
                    root_ids: source.root_ids.clone(),
                    returned_nodes: page.len() as u32,
                    truncated: more,
                    next_cursor,
                    unavailable_fields: Vec::new(),
                };
                let envelope = Envelope::new(
                    request_id.clone(),
                    live_source_ref(&source),
                    observation(Consistency::Fresh, Freshness::FreshRead, None),
                    coverage,
                    json!({ "nodes": page }),
                )
                .with_content_hash();
                if params.consistency == ConsistencyRequest::Cached {
                    self.cache_store(cache_key, &source, &envelope);
                }
                ClientResponse::Ok {
                    request_id,
                    envelope,
                }
            }
        }
    }

    async fn get_nodes(
        &self,
        client_id: &str,
        request_id: String,
        params: Value,
    ) -> ClientResponse {
        let params: GetNodesParams = match serde_json::from_value(params) {
            Ok(params) => params,
            Err(err) => {
                return error_response(
                    request_id,
                    FigError::new(
                        ErrorCode::InvalidLimit,
                        format!("the parameters are invalid: {err}"),
                    ),
                )
            }
        };
        let source =
            match self.resolve_source(client_id, params.session_id, params.snapshot_id, None) {
                Ok(source) => source,
                Err(error) => return error_response(request_id, error),
            };
        if !self.profile.limits.batch_is_valid(params.node_ids.len()) {
            return error_response(
                request_id,
                FigError::new(
                    ErrorCode::InvalidLimit,
                    format!(
                        "a batch of {} node ids exceeds the limit of {}",
                        params.node_ids.len(),
                        self.profile.limits.max_batch_node_ids
                    ),
                ),
            );
        }
        if let Some(depth) = params.depth {
            if !self.profile.limits.depth_is_valid(depth) {
                return error_response(
                    request_id,
                    FigError::new(
                        ErrorCode::InvalidLimit,
                        format!("the depth {depth} exceeds the limit"),
                    ),
                );
            }
        }
        match source {
            ReadSource::Snapshot(snapshot) => {
                let wanted: Vec<String> = params.node_ids.clone();
                let page: Vec<PluginNode> = snapshot
                    .nodes
                    .iter()
                    .filter(|node| wanted.contains(&node.id))
                    .cloned()
                    .collect();
                let coverage = Coverage {
                    root_ids: wanted.clone(),
                    returned_nodes: page.len() as u32,
                    truncated: false,
                    next_cursor: None,
                    unavailable_fields: Vec::new(),
                };
                let envelope = Envelope::new(
                    request_id.clone(),
                    SourceRef::plugin_snapshot(snapshot.snapshot_id),
                    observation(Consistency::BestEffort, Freshness::FreshRead, None),
                    coverage,
                    json!({ "nodes": page }),
                )
                .with_content_hash();
                ClientResponse::Ok {
                    request_id,
                    envelope,
                }
            }
            ReadSource::Live(source) => {
                let request_params = json!({
                    "node_ids": params.node_ids,
                    "depth": params.depth,
                });
                let cache_key = Self::cache_key(&source, BridgeOp::ReadNodes, &request_params);
                if params.consistency == ConsistencyRequest::Cached {
                    if let Some(envelope) =
                        self.cache_lookup(&cache_key, &source, params.max_age_ms)
                    {
                        return ClientResponse::Ok {
                            request_id,
                            envelope,
                        };
                    }
                }
                let frame = match self
                    .dispatch(
                        &source.session_id,
                        BridgeOp::ReadNodes,
                        request_params,
                        self.profile.limits.read_timeout,
                    )
                    .await
                {
                    Ok(frame) => frame,
                    Err(error) => return error_response(request_id, error),
                };
                let data = match result_data(&frame) {
                    Ok(data) => data,
                    Err(error) => return error_response(request_id, error),
                };
                let nodes: Vec<PluginNode> =
                    serde_json::from_value(data.get("nodes").cloned().unwrap_or(Value::Null))
                        .unwrap_or_default();
                let scoped = self.filter_by_scope(&source, nodes);
                let missing: Vec<String> = params
                    .node_ids
                    .iter()
                    .filter(|id| !scoped.iter().any(|node| &node.id == *id))
                    .cloned()
                    .collect();
                if !missing.is_empty() {
                    return error_response(
                        request_id,
                        FigError::new(
                            ErrorCode::OutOfScope,
                            format!("node {} is outside the granted scope", missing.join(", ")),
                        ),
                    );
                }
                let (page, truncated) = self.paginate(scoped);
                let coverage = Coverage {
                    root_ids: source.root_ids.clone(),
                    returned_nodes: page.len() as u32,
                    truncated,
                    next_cursor: None,
                    unavailable_fields: Vec::new(),
                };
                let envelope = Envelope::new(
                    request_id.clone(),
                    live_source_ref(&source),
                    observation(Consistency::Fresh, Freshness::FreshRead, None),
                    coverage,
                    json!({ "nodes": page }),
                )
                .with_content_hash();
                if params.consistency == ConsistencyRequest::Cached {
                    self.cache_store(cache_key, &source, &envelope);
                }
                ClientResponse::Ok {
                    request_id,
                    envelope,
                }
            }
        }
    }

    async fn inspect_design(
        &self,
        client_id: &str,
        request_id: String,
        params: Value,
    ) -> ClientResponse {
        let params: InspectDesignParams = match serde_json::from_value(params) {
            Ok(params) => params,
            Err(err) => {
                return error_response(
                    request_id,
                    FigError::new(
                        ErrorCode::InvalidLimit,
                        format!("the parameters are invalid: {err}"),
                    ),
                )
            }
        };
        let source = match self.resolve_source(
            client_id,
            params.session_id,
            params.snapshot_id,
            Some(params.root_id.as_str()),
        ) {
            Ok(source) => source,
            Err(error) => return error_response(request_id, error),
        };
        let ReadSource::Live(source) = source else {
            return error_response(
                request_id,
                FigError::new(
                    ErrorCode::CapabilityUnavailable,
                    "inspect_design is only available for live sessions",
                ),
            );
        };
        let request_params = json!({ "root_id": params.root_id, "detail": params.detail });
        let frame = match self
            .dispatch(
                &source.session_id,
                BridgeOp::InspectDesign,
                request_params,
                self.profile.limits.read_timeout,
            )
            .await
        {
            Ok(frame) => frame,
            Err(error) => return error_response(request_id, error),
        };
        let data = match result_data(&frame) {
            Ok(data) => data,
            Err(error) => return error_response(request_id, error),
        };
        let coverage = Coverage {
            root_ids: source.root_ids.clone(),
            returned_nodes: 1,
            truncated: false,
            next_cursor: None,
            unavailable_fields: Vec::new(),
        };
        let envelope = Envelope::new(
            request_id.clone(),
            live_source_ref(&source),
            observation(Consistency::Fresh, Freshness::FreshRead, None),
            coverage,
            data,
        )
        .with_content_hash();
        ClientResponse::Ok {
            request_id,
            envelope,
        }
    }

    async fn get_design_tokens(
        &self,
        client_id: &str,
        request_id: String,
        params: Value,
    ) -> ClientResponse {
        let params: GetDesignTokensParams = match serde_json::from_value(params) {
            Ok(params) => params,
            Err(err) => {
                return error_response(
                    request_id,
                    FigError::new(
                        ErrorCode::InvalidLimit,
                        format!("the parameters are invalid: {err}"),
                    ),
                )
            }
        };
        let source = match self.resolve_source(
            client_id,
            params.session_id,
            params.snapshot_id,
            Some(params.root_id.as_str()),
        ) {
            Ok(source) => source,
            Err(error) => return error_response(request_id, error),
        };
        let ReadSource::Live(source) = source else {
            return error_response(
                request_id,
                FigError::new(
                    ErrorCode::CapabilityUnavailable,
                    "get_design_tokens is only available for live sessions",
                ),
            );
        };
        let request_params = json!({ "root_id": params.root_id, "kinds": params.kinds });
        let frame = match self
            .dispatch(
                &source.session_id,
                BridgeOp::GetDesignTokens,
                request_params,
                self.profile.limits.read_timeout,
            )
            .await
        {
            Ok(frame) => frame,
            Err(error) => return error_response(request_id, error),
        };
        let data = match result_data(&frame) {
            Ok(data) => data,
            Err(error) => return error_response(request_id, error),
        };
        let coverage = Coverage {
            root_ids: source.root_ids.clone(),
            returned_nodes: 0,
            truncated: false,
            next_cursor: None,
            unavailable_fields: Vec::new(),
        };
        let envelope = Envelope::new(
            request_id.clone(),
            live_source_ref(&source),
            observation(Consistency::Fresh, Freshness::FreshRead, None),
            coverage,
            data,
        )
        .with_content_hash();
        ClientResponse::Ok {
            request_id,
            envelope,
        }
    }

    async fn export_assets(
        &self,
        client_id: &str,
        request_id: String,
        params: Value,
    ) -> ClientResponse {
        let params: ExportAssetsParams = match serde_json::from_value(params) {
            Ok(params) => params,
            Err(err) => {
                return error_response(
                    request_id,
                    FigError::new(
                        ErrorCode::InvalidLimit,
                        format!("the parameters are invalid: {err}"),
                    ),
                )
            }
        };
        let source =
            match self.resolve_source(client_id, params.session_id, params.snapshot_id, None) {
                Ok(source) => source,
                Err(error) => return error_response(request_id, error),
            };
        if let Some(scale) = params.scale {
            if !self.profile.limits.scale_is_valid(scale) {
                return error_response(
                    request_id,
                    FigError::new(
                        ErrorCode::InvalidLimit,
                        format!(
                            "the scale {scale} is outside the accepted range {} to {}",
                            self.profile.limits.min_export_scale,
                            self.profile.limits.max_export_scale
                        ),
                    ),
                );
            }
        }
        if !self.profile.limits.batch_is_valid(params.node_ids.len()) {
            return error_response(
                request_id,
                FigError::new(
                    ErrorCode::InvalidLimit,
                    format!(
                        "a batch of {} node ids exceeds the limit of {}",
                        params.node_ids.len(),
                        self.profile.limits.max_batch_node_ids
                    ),
                ),
            );
        }
        let ReadSource::Live(source) = source else {
            return error_response(
                request_id,
                FigError::new(
                    ErrorCode::CapabilityUnavailable,
                    "export_assets is only available for live sessions",
                ),
            );
        };
        let node_ids = params.node_ids.clone();
        let format = match params.format {
            AssetFormat::Png => "png",
            AssetFormat::Svg => "svg",
        };
        let request_params = json!({
            "node_ids": node_ids,
            "format": format,
            "scale": params.scale.unwrap_or(1.0),
        });
        let frame = match self
            .dispatch(
                &source.session_id,
                BridgeOp::ExportAssets,
                request_params,
                self.profile.limits.export_timeout,
            )
            .await
        {
            Ok(frame) => frame,
            Err(error) => return error_response(request_id, error),
        };
        let data = match result_data(&frame) {
            Ok(data) => data,
            Err(error) => return error_response(request_id, error),
        };
        let bytes = data
            .get("bytes")
            .and_then(Value::as_str)
            .and_then(|encoded| base64_decode(encoded).ok())
            .unwrap_or_default();
        let mime = data
            .get("mime")
            .and_then(Value::as_str)
            .unwrap_or("image/png")
            .to_string();
        let width = data.get("width").and_then(Value::as_u64).unwrap_or(0) as u32;
        let height = data.get("height").and_then(Value::as_u64).unwrap_or(0) as u32;
        if !self.profile.limits.pixels_are_valid(width, height) {
            return error_response(
                request_id,
                FigError::new(
                    ErrorCode::InvalidLimit,
                    "the export exceeds the pixel budget",
                ),
            );
        }
        if bytes.len() as u64 > self.profile.limits.max_asset_bytes {
            return error_response(
                request_id,
                FigError::new(
                    ErrorCode::PayloadTooLarge,
                    "the export exceeds the asset byte budget",
                ),
            );
        }
        let asset_id = AssetId::new();
        let token = uuid::Uuid::new_v4().simple().to_string();
        let hash = figtether_protocol::sha256_hex(&bytes);
        {
            let mut assets = self.assets.lock().expect("asset lock");
            assets.insert(
                asset_id,
                AssetRecord {
                    session_id: source.session_id,
                    grant_epoch: source.grant_epoch,
                    mime: mime.clone(),
                    width,
                    height,
                    bytes,
                    hash: hash.clone(),
                },
            );
        }
        let coverage = Coverage {
            root_ids: source.root_ids.clone(),
            returned_nodes: 1,
            truncated: false,
            next_cursor: None,
            unavailable_fields: Vec::new(),
        };
        let envelope = Envelope::new(
            request_id.clone(),
            live_source_ref(&source),
            observation(Consistency::Fresh, Freshness::FreshRead, None),
            coverage,
            json!({
                "node_id": node_ids.first(),
                "asset_id": asset_id,
                "mime": mime,
                "width": width,
                "height": height,
                "hash": hash,
                "resource_uri": format!("figtether://asset/{token}"),
            }),
        )
        .with_content_hash();
        ClientResponse::Ok {
            request_id,
            envelope,
        }
    }

    async fn get_selection(
        &self,
        client_id: &str,
        request_id: String,
        params: Value,
    ) -> ClientResponse {
        let params: GetSelectionParams = match serde_json::from_value(params) {
            Ok(params) => params,
            Err(err) => {
                return error_response(
                    request_id,
                    FigError::new(
                        ErrorCode::InvalidLimit,
                        format!("the parameters are invalid: {err}"),
                    ),
                )
            }
        };
        let source = match self.resolve_source(client_id, Some(params.session_id), None, None) {
            Ok(source) => source,
            Err(error) => return error_response(request_id, error),
        };
        let ReadSource::Live(source) = source else {
            return error_response(
                request_id,
                FigError::new(
                    ErrorCode::CapabilityUnavailable,
                    "get_selection is only available for live sessions",
                ),
            );
        };
        let frame = match self
            .dispatch(
                &source.session_id,
                BridgeOp::GetSelection,
                json!({}),
                self.profile.limits.read_timeout,
            )
            .await
        {
            Ok(frame) => frame,
            Err(error) => return error_response(request_id, error),
        };
        let data = match result_data(&frame) {
            Ok(data) => data,
            Err(error) => return error_response(request_id, error),
        };
        let coverage = Coverage {
            root_ids: source.root_ids.clone(),
            returned_nodes: 0,
            truncated: false,
            next_cursor: None,
            unavailable_fields: Vec::new(),
        };
        let envelope = Envelope::new(
            request_id.clone(),
            live_source_ref(&source),
            observation(Consistency::Fresh, Freshness::FreshRead, None),
            coverage,
            data,
        )
        .with_content_hash();
        ClientResponse::Ok {
            request_id,
            envelope,
        }
    }

    fn get_snapshot_info(&self, request_id: String, params: Value) -> ClientResponse {
        let params: GetSnapshotInfoParams = match serde_json::from_value(params) {
            Ok(params) => params,
            Err(err) => {
                return error_response(
                    request_id,
                    FigError::new(
                        ErrorCode::InvalidLimit,
                        format!("the parameters are invalid: {err}"),
                    ),
                )
            }
        };
        let snapshots = self.snapshots.lock().expect("snapshot lock");
        let Some(record) = snapshots.get(&params.snapshot_id) else {
            return error_response(
                request_id,
                FigError::new(ErrorCode::SessionNotFound, "the snapshot does not exist"),
            );
        };
        let coverage = Coverage {
            root_ids: Vec::new(),
            returned_nodes: record.nodes.len() as u32,
            truncated: false,
            next_cursor: None,
            unavailable_fields: Vec::new(),
        };
        let envelope = Envelope::new(
            request_id.clone(),
            SourceRef::plugin_snapshot(record.snapshot_id),
            observation(Consistency::BestEffort, Freshness::FreshRead, None),
            coverage,
            json!({
                "snapshot_id": record.snapshot_id,
                "grant_epoch": record.grant_epoch,
                "content_hash": record.content_hash,
                "captured_at": record.captured_at,
                "nodes": record.nodes.len(),
            }),
        )
        .with_content_hash();
        ClientResponse::Ok {
            request_id,
            envelope,
        }
    }
}

/// A live session bound to one grant.
#[derive(Debug, Clone)]
struct LiveSource {
    session_id: SessionId,
    grant_epoch: u64,
    connection_epoch: u64,
    root_ids: Vec<String>,
}

/// The source of a read.
#[derive(Debug, Clone)]
enum ReadSource {
    Live(LiveSource),
    Snapshot(SnapshotRecord),
}

fn live_source_ref(source: &LiveSource) -> SourceRef {
    SourceRef {
        kind: SourceKind::PluginLive,
        session_id: Some(source.session_id),
        snapshot_id: None,
        grant_epoch: Some(source.grant_epoch),
        connection_epoch: Some(source.connection_epoch),
    }
}

fn observation(
    consistency: Consistency,
    freshness: Freshness,
    cache_age_ms: Option<u64>,
) -> Observation {
    let now = Utc::now();
    Observation {
        observation_id: ObservationId::new(),
        started_at: now,
        observed_at: now,
        consistency,
        freshness,
        cache_age_ms,
        content_hash: None,
    }
}

fn ok_response(request_id: String, data: Value) -> ClientResponse {
    let envelope = Envelope::new(
        request_id.clone(),
        SourceRef {
            kind: SourceKind::PluginLive,
            session_id: None,
            snapshot_id: None,
            grant_epoch: None,
            connection_epoch: None,
        },
        observation(Consistency::Fresh, Freshness::FreshRead, None),
        Coverage::default(),
        data,
    )
    .with_content_hash();
    ClientResponse::Ok {
        request_id,
        envelope,
    }
}

fn error_response(request_id: String, error: FigError) -> ClientResponse {
    let error = error.with_request_id(request_id.clone());
    ClientResponse::Error { request_id, error }
}

/// Build a bridge error frame.
fn error_frame(code: &ErrorCode, message: &str, action: &str) -> BridgeFrame {
    let payload = ErrorPayload {
        code: code.as_str().to_string(),
        message: message.to_string(),
        retryable: code.retryable().unwrap_or(false),
        action: action.to_string(),
        request_id: String::new(),
        retry_after_ms: None,
    };
    BridgeFrame::new(
        MessageType::Error,
        String::new(),
        serde_json::to_value(payload).unwrap_or(Value::Null),
    )
}

/// Read the payload of a result frame, mapping a plugin error to a
/// structured error.
fn result_data(frame: &BridgeFrame) -> Result<Value, FigError> {
    match frame.message_type {
        MessageType::Error => {
            let payload: ErrorPayload = frame.payload_of().unwrap_or(ErrorPayload {
                code: ErrorCode::InternalError.as_str().to_string(),
                message: "the plugin reported an error".to_string(),
                retryable: false,
                action: "retry once".to_string(),
                request_id: String::new(),
                retry_after_ms: None,
            });
            Err(
                FigError::new(error_code_from_str(&payload.code), payload.message)
                    .with_retryable(payload.retryable)
                    .with_action(payload.action),
            )
        }
        MessageType::Result => {
            let payload: ResultPayload = frame.payload_of().map_err(|err| {
                FigError::new(
                    ErrorCode::InternalError,
                    format!("unreadable result: {err}"),
                )
            })?;
            Ok(payload.data)
        }
        _ => Err(FigError::new(
            ErrorCode::InternalError,
            "the plugin answered with an unexpected frame",
        )),
    }
}

/// Map a wire error code to the error taxonomy.
pub fn error_code_from_str(code: &str) -> ErrorCode {
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
        "WRITE_NOT_GRANTED" => ErrorCode::WriteNotGranted,
        "APPROVAL_REQUIRED" => ErrorCode::ApprovalRequired,
        "APPROVAL_EXPIRED" => ErrorCode::ApprovalExpired,
        "PLAN_HASH_MISMATCH" => ErrorCode::PlanHashMismatch,
        "PLAN_INVALID" => ErrorCode::PlanInvalid,
        "BUDGET_EXCEEDED" => ErrorCode::BudgetExceeded,
        "DESTRUCTIVE_NOT_ALLOWED" => ErrorCode::DestructiveNotAllowed,
        "MUTATION_REFUSED" => ErrorCode::MutationRefused,
        _ => ErrorCode::InternalError,
    }
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;
    STANDARD.encode(bytes)
}

fn base64_decode(text: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;
    STANDARD.decode(text)
}

/// The harness daemon.
pub struct HarnessDaemon {
    state: Arc<DaemonState>,
    addr: SocketAddr,
    shutdown: Option<tokio::sync::watch::Sender<bool>>,
    server: Option<tokio::task::JoinHandle<()>>,
    credential: String,
}

/// The harness daemon.
impl HarnessDaemon {
    /// Start the daemon on an ephemeral loopback port.
    pub async fn start(profile: DaemonProfile) -> Result<Self, HarnessError> {
        let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .map_err(HarnessError::Io)?;
        let addr = listener.local_addr().map_err(HarnessError::Io)?;
        let state = Arc::new(DaemonState::new(profile, addr));
        let credential = format!("ftclient-{}", uuid::Uuid::new_v4().simple());
        state.register_client(&credential, "harness-client");

        let app = Router::new()
            .route(HEALTH_PATH, get(health_handler))
            .route(BRIDGE_PATH, get(bridge_handler))
            .route(CLIENT_PATH, get(client_handler))
            .route(&format!("{ASSET_PATH}/{{asset_id}}"), get(asset_handler))
            .with_state(state.clone());

        let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);
        let server = tokio::spawn(async move {
            let signal = async move {
                let _ = shutdown_rx.changed().await;
            };
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(signal)
                .await;
        });
        spawn_heartbeat_monitor(state.clone());

        Ok(Self {
            state,
            addr,
            shutdown: Some(shutdown_tx),
            server: Some(server),
            credential,
        })
    }

    /// The address the daemon listens on.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// The bridge endpoint of the daemon.
    pub fn bridge_url(&self) -> String {
        format!("ws://{}{BRIDGE_PATH}", self.addr)
    }

    /// The client endpoint of the daemon.
    pub fn client_url(&self) -> String {
        format!("ws://{}{CLIENT_PATH}", self.addr)
    }

    /// The asset endpoint of the daemon.
    pub fn asset_url(&self, asset_id: &AssetId) -> String {
        format!("http://{}{ASSET_PATH}/{asset_id}", self.addr)
    }

    /// The instance identity of this daemon run.
    pub fn instance_id(&self) -> String {
        self.state.instance_id.clone()
    }

    /// The credential a local client uses on the client endpoint.
    pub fn client_credential(&self) -> String {
        self.credential.clone()
    }

    /// Mint a pairing secret with the default time to live.
    pub fn mint_pairing_secret(&self) -> String {
        self.state.mint_secret(self.state.profile.secret_ttl)
    }

    /// Mint a pairing secret with an explicit time to live.
    pub fn mint_pairing_secret_with_ttl(&self, ttl: Duration) -> String {
        self.state.mint_secret(ttl)
    }

    /// Number of cache entries.
    pub fn cache_entry_count(&self) -> usize {
        self.state.cache.lock().expect("cache lock").len()
    }

    /// Number of stored assets.
    pub fn asset_count(&self) -> usize {
        self.state.assets.lock().expect("asset lock").len()
    }

    /// Number of stored snapshots.
    pub fn snapshot_count(&self) -> usize {
        self.state.snapshots.lock().expect("snapshot lock").len()
    }

    /// The sessions the daemon knows.
    pub fn session_ids(&self) -> Vec<SessionId> {
        let sessions = self.state.sessions.lock().expect("session lock");
        let mut ids: Vec<SessionId> = sessions.keys().copied().collect();
        ids.sort();
        ids
    }

    /// The state of one session.
    pub fn session_state(&self, session_id: &SessionId) -> Option<SessionState> {
        let sessions = self.state.sessions.lock().expect("session lock");
        sessions.get(session_id).map(|session| session.state)
    }

    /// Pause a session. New reads are refused, the grant stays.
    pub fn pause(&self, session_id: &SessionId) -> Result<(), FigError> {
        self.state.apply_event(session_id, SessionEvent::Paused)?;
        Ok(())
    }

    /// Resume a paused session. The grant epoch moves, so the old cache
    /// entries and cursors stop resolving.
    pub fn resume(&self, session_id: &SessionId) -> Result<(), FigError> {
        self.state.apply_event(session_id, SessionEvent::Resumed)?;
        let mut sessions = self.state.sessions.lock().expect("session lock");
        let session = sessions.get_mut(session_id).ok_or_else(|| {
            FigError::new(ErrorCode::SessionNotFound, "the session does not exist")
        })?;
        // The frozen core state machine has no edge into `Ready`, the daemon
        // owns it.
        session.state = SessionState::Ready;
        Ok(())
    }

    /// Capture an immutable snapshot of the granted scope of a session.
    pub async fn capture_snapshot(&self, session_id: &SessionId) -> Result<SnapshotId, FigError> {
        let frame = self
            .state
            .dispatch(
                session_id,
                BridgeOp::CaptureSnapshot,
                json!({}),
                self.state.profile.limits.read_timeout,
            )
            .await?;
        let data = result_data(&frame)?;
        let nodes: Vec<PluginNode> =
            serde_json::from_value(data.get("nodes").cloned().unwrap_or(Value::Null))
                .unwrap_or_default();
        let snapshot_id = SnapshotId::new();
        let grant_epoch = self
            .state
            .sessions
            .lock()
            .expect("session lock")
            .get(session_id)
            .map(|session| session.grant_epoch)
            .unwrap_or(0);
        let content_hash = figtether_model::envelope::canonical_hash(&json!(nodes));
        let mut snapshots = self.state.snapshots.lock().expect("snapshot lock");
        snapshots.insert(
            snapshot_id,
            SnapshotRecord {
                snapshot_id,
                session_id: *session_id,
                grant_epoch,
                nodes,
                content_hash,
                captured_at: Utc::now(),
            },
        );
        Ok(snapshot_id)
    }

    /// Ask the daemon to stop and wait for the listener to close.
    pub async fn shutdown(mut self) -> Result<(), HarnessError> {
        self.request_shutdown();
        if let Some(server) = self.server.take() {
            let _ = tokio::time::timeout(Duration::from_secs(5), server).await;
        }
        Ok(())
    }

    fn request_shutdown(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(true);
        }
    }
}

impl Drop for HarnessDaemon {
    fn drop(&mut self) {
        self.request_shutdown();
    }
}

fn spawn_heartbeat_monitor(state: Arc<DaemonState>) {
    let interval = state.profile.heartbeat_interval;
    let missed = state.profile.missed_heartbeats;
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            let now = Instant::now();
            let mut sessions = state.sessions.lock().expect("session lock");
            for session in sessions.values_mut() {
                if session.state == SessionState::Offline {
                    continue;
                }
                let last_seen = *session.last_seen.lock().expect("last seen lock");
                if now.duration_since(last_seen) > interval * missed {
                    if let Ok(result) = transition(session.state, SessionEvent::HeartbeatLost) {
                        session.state = result.to;
                    }
                }
            }
        }
    });
}

async fn health_handler() -> Response {
    (StatusCode::OK, "ok").into_response()
}

async fn bridge_handler(
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
    State(state): State<Arc<DaemonState>>,
) -> Response {
    if let Err(reason) = check_peer(&headers, &state) {
        return (StatusCode::FORBIDDEN, reason).into_response();
    }
    upgrade.on_upgrade(move |socket| handle_bridge(socket, state))
}

async fn client_handler(
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
    State(state): State<Arc<DaemonState>>,
) -> Response {
    if let Err(reason) = check_peer(&headers, &state) {
        return (StatusCode::FORBIDDEN, reason).into_response();
    }
    upgrade.on_upgrade(move |socket| handle_client(socket, state))
}

/// Reject a connection whose host or origin is not allowed.
///
/// The check runs before the upgrade, so no session, capability or design
/// information can leak to a connection that failed the transport check.
fn check_peer(headers: &HeaderMap, state: &DaemonState) -> Result<(), String> {
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let port = state.addr.port();
    let host_ok = host == state.addr.to_string()
        || host == format!("127.0.0.1:{port}")
        || host == format!("localhost:{port}");
    if !host_ok {
        return Err(format!("the host header {host} is not allowed"));
    }
    if let Some(origin) = headers.get(axum::http::header::ORIGIN) {
        let origin = origin.to_str().unwrap_or_default();
        let allowed = origin == "null"
            || origin == format!("http://127.0.0.1:{port}")
            || origin == format!("http://localhost:{port}")
            || origin == "https://www.figma.com";
        if !allowed {
            return Err(format!("the origin {origin} is not allowed"));
        }
    }
    Ok(())
}

async fn handle_bridge(socket: WebSocket, state: Arc<DaemonState>) {
    let (mut sink, mut stream) = socket.split();

    let first = tokio::time::timeout(state.profile.auth_window, stream.next()).await;
    let Ok(Some(Ok(message))) = first else {
        let _ = sink.close().await;
        return;
    };
    let text = match message {
        Message::Text(text) => text.to_string(),
        Message::Binary(bytes) => String::from_utf8_lossy(&bytes).to_string(),
        _ => {
            let _ = sink.close().await;
            return;
        }
    };
    let frame = match BridgeFrame::from_bytes(text.as_bytes()) {
        Ok(frame) => frame,
        Err(_) => {
            let _ = sink.close().await;
            return;
        }
    };
    if frame.message_type != MessageType::HelloAuth {
        close_with_error(
            &mut sink,
            "PROTOCOL_MISMATCH",
            "the first frame must authenticate",
        )
        .await;
        return;
    }
    let auth: AuthRequest = match frame.payload_of() {
        Ok(auth) => auth,
        Err(_) => {
            close_with_error(
                &mut sink,
                "PROTOCOL_MISMATCH",
                "the auth frame is unreadable",
            )
            .await;
            return;
        }
    };
    let (session_id, connection_epoch, capabilities) = match state.authenticate_plugin(&auth) {
        Ok(result) => result,
        Err(error) => {
            close_with_error(&mut sink, error.code.as_str(), &error.message).await;
            return;
        }
    };
    let accepted = AuthAccepted {
        session_credential: format!("session-{session_id}"),
        session_id,
        connection_epoch,
        grant_epoch: None,
        capabilities,
    };
    let reply = BridgeFrame::new(
        MessageType::HelloAuth,
        frame.request_id.clone(),
        serde_json::to_value(accepted).unwrap_or(Value::Null),
    );
    if send_frame(&mut sink, &reply).await.is_err() {
        return;
    }
    bridge_loop(sink, stream, state, session_id).await;
}

async fn bridge_loop(
    mut sink: futures_util::stream::SplitSink<WebSocket, Message>,
    mut stream: futures_util::stream::SplitStream<WebSocket>,
    state: Arc<DaemonState>,
    session_id: SessionId,
) {
    let receiver = {
        let sessions = state.sessions.lock().expect("session lock");
        let Some(session) = sessions.get(&session_id) else {
            return;
        };
        let mut guard = session.to_plugin_rx.lock().expect("channel lock");
        guard.take()
    };
    let Some(mut receiver) = receiver else {
        return;
    };
    let outbound = {
        let sessions = state.sessions.lock().expect("session lock");
        let Some(session) = sessions.get(&session_id) else {
            return;
        };
        let guard = session.to_plugin.lock().expect("channel lock");
        guard.clone()
    };
    let Some(outbound) = outbound else {
        return;
    };

    let writer_outbound = outbound.clone();
    let writer = tokio::spawn(async move {
        while let Some(frame) = receiver.recv().await {
            if send_frame(&mut sink, &frame).await.is_err() {
                break;
            }
        }
        let _ = sink.close().await;
    });

    while let Some(Ok(message)) = stream.next().await {
        let text = match message {
            Message::Text(text) => text.to_string(),
            Message::Binary(bytes) => String::from_utf8_lossy(&bytes).to_string(),
            _ => continue,
        };
        if text.len() > MAX_FRAME_BYTES {
            state
                .counters
                .oversized_frames
                .fetch_add(1, Ordering::Relaxed);
            // The request id is read before the frame is rejected, so the
            // request that carried the oversized payload is failed with a
            // structured error instead of timing out.
            let request_id = serde_json::from_slice::<Value>(text.as_bytes())
                .ok()
                .and_then(|value| {
                    value
                        .get("request_id")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .unwrap_or_default();
            let error = error_frame(
                &ErrorCode::PayloadTooLarge,
                "the frame exceeded the byte budget",
                "narrow the request",
            );
            state.resolve_pending(&session_id, &request_id, error.clone());
            let _ = writer_outbound.send(error).await;
            break;
        }
        let frame = match BridgeFrame::from_bytes(text.as_bytes()) {
            Ok(frame) => frame,
            Err(_) => continue,
        };
        state
            .handle_plugin_frame(&session_id, &frame, &outbound)
            .await;
    }

    state.mark_offline(&session_id);
    writer.abort();
}

async fn handle_client(socket: WebSocket, state: Arc<DaemonState>) {
    let (mut sink, mut stream) = socket.split();
    let Some(Ok(message)) = stream.next().await else {
        return;
    };
    let text = match message {
        Message::Text(text) => text.to_string(),
        Message::Binary(bytes) => String::from_utf8_lossy(&bytes).to_string(),
        _ => return,
    };
    let auth: ClientAuth = match serde_json::from_str(&text) {
        Ok(auth) => auth,
        Err(_) => return,
    };
    let client_id = match state.authenticate_client(&auth) {
        Ok(client_id) => client_id,
        Err(error) => {
            close_with_error(&mut sink, error.code.as_str(), &error.message).await;
            return;
        }
    };
    // The daemon answers the authentication of a local client, so a refused
    // credential is visible at the handshake and not at the first request.
    let ack = ok_response("auth".to_string(), state.status_payload());
    let encoded = match serde_json::to_string(&ack) {
        Ok(encoded) => encoded,
        Err(_) => return,
    };
    if sink.send(Message::Text(encoded.into())).await.is_err() {
        return;
    }
    while let Some(Ok(message)) = stream.next().await {
        let text = match message {
            Message::Text(text) => text.to_string(),
            Message::Binary(bytes) => String::from_utf8_lossy(&bytes).to_string(),
            _ => continue,
        };
        let request: ClientRequest = match serde_json::from_str(&text) {
            Ok(request) => request,
            Err(_) => continue,
        };
        let response = state.handle_client_request(&client_id, request).await;
        let encoded = match serde_json::to_string(&response) {
            Ok(encoded) => encoded,
            Err(_) => continue,
        };
        if sink.send(Message::Text(encoded.into())).await.is_err() {
            return;
        }
    }
}

async fn asset_handler(
    headers: HeaderMap,
    Path(asset_id): Path<String>,
    Query(query): Query<HashMap<String, String>>,
    State(state): State<Arc<DaemonState>>,
) -> Response {
    let credential = headers
        .get("x-figtether-client-credential")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if let Err(error) = state.authenticate_client(&ClientAuth {
        credential: credential.to_string(),
        client_id: String::new(),
        protocol_version: PROTOCOL_VERSION.to_string(),
    }) {
        return (
            StatusCode::UNAUTHORIZED,
            serde_json::to_string(&error).unwrap_or_default(),
        )
            .into_response();
    }
    let Ok(asset_id) = asset_id.parse::<AssetId>() else {
        return (StatusCode::NOT_FOUND, "unknown asset").into_response();
    };
    let assets = state.assets.lock().expect("asset lock");
    let Some(record) = assets.get(&asset_id) else {
        return (StatusCode::NOT_FOUND, "unknown asset").into_response();
    };
    let session_id = query.get("session_id").cloned().unwrap_or_default();
    if record.session_id.to_string() != session_id {
        let error = FigError::new(
            ErrorCode::OutOfScope,
            "the asset belongs to another session",
        );
        return (
            StatusCode::FORBIDDEN,
            serde_json::to_string(&error).unwrap_or_default(),
        )
            .into_response();
    }
    let sessions = state.sessions.lock().expect("session lock");
    let session_state = sessions
        .get(&record.session_id)
        .map(|session| session.state);
    let grant_epoch = sessions
        .get(&record.session_id)
        .map(|session| session.grant_epoch);
    if session_state != Some(SessionState::Ready) || grant_epoch != Some(record.grant_epoch) {
        let error = FigError::new(ErrorCode::Revoked, "the grant is no longer active");
        return (
            StatusCode::FORBIDDEN,
            serde_json::to_string(&error).unwrap_or_default(),
        )
            .into_response();
    }
    Response::builder()
        .status(StatusCode::OK)
        .header(axum::http::header::CONTENT_TYPE, record.mime.clone())
        .header("x-figtether-asset-hash", record.hash.clone())
        .header("x-figtether-asset-width", record.width.to_string())
        .header("x-figtether-asset-height", record.height.to_string())
        .body(axum::body::Body::from(record.bytes.clone()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// Send one frame over the bridge.
async fn send_frame(
    sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    frame: &BridgeFrame,
) -> Result<(), ()> {
    let encoded = serde_json::to_string(frame).map_err(|_| ())?;
    sink.send(Message::Text(encoded.into()))
        .await
        .map_err(|_| ())
}

/// Close a connection with a machine readable reason.
async fn close_with_error(
    sink: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    code: &str,
    message: &str,
) {
    let reason = json!({ "code": code, "message": message }).to_string();
    let _ = sink
        .send(Message::Close(Some(CloseFrame {
            code: CLOSE_POLICY_VIOLATION,
            reason: reason.into(),
        })))
        .await;
}
