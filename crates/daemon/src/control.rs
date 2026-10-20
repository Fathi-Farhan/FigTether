//! The control surface of the daemon.
//!
//! Three things live here:
//!
//! - the pairing secret, which the plugin UI shows to the user. It is minted
//!   on demand, lives in memory only, is bound to the local installation that
//!   asked for it, and is never logged;
//! - the sanitized status, which the health endpoint and the status tool
//!   read. It carries versions, counts, sizes, the capabilities of the known
//!   sessions, and the recent write audit records of the installation, never
//!   design content and never a secret;
//! - the revocation, which cancels the work of a session, drops its cache
//!   entries and assets, and refuses later reads.

use std::sync::Arc;

use chrono::Utc;
use figtether_core::{AssetStore, CacheStats, Metrics, SchedulerStats, SessionState, SourceKey};
use figtether_model::capabilities::CapabilityState;
use figtether_model::errors::{ErrorCode, FigError};
use figtether_model::ids::SessionId;
use figtether_protocol::{BridgeFrame, MessageType};
use serde::Serialize;

use crate::guarded;
use crate::secrets::PAIRING_SECRET_TTL;
use crate::DaemonState;

/// The longest text a sanitized field may carry.
const MAX_SANITIZED_TEXT: usize = 160;

/// The sanitized status of the daemon.
///
/// Every field is a version, a counter, a size, or an identifier. The struct
/// has no field for a credential, a node id, a layer name, or any other
/// design content.
#[derive(Debug, Clone, Serialize)]
pub struct DaemonStatus {
    /// Version of the daemon.
    pub daemon_version: String,
    /// Version of the core domain.
    pub core_version: String,
    /// Version of the bridge protocol.
    pub protocol_version: String,
    /// Version of the response schema.
    pub schema_version: String,
    /// Identity of this daemon instance.
    pub instance_id: String,
    /// Port of the listener.
    pub port: u16,
    /// Address the daemon binds. Always the loopback interface.
    pub bind_address: String,
    /// Seconds since the daemon started.
    pub uptime_seconds: u64,
    /// Number of registered sessions.
    pub sessions: usize,
    /// Number of live plugin connections.
    pub plugin_connections: usize,
    /// Requests waiting in the scheduler.
    pub queue_depth: usize,
    /// Requests waiting in the scheduler, peak value.
    pub queue_depth_peak: u64,
    /// Requests refused because the queue was full.
    pub busy_rejections: u64,
    /// Requests accepted in total.
    pub requests_total: u64,
    /// Requests served from the cache.
    pub cache_hits: u64,
    /// Requests that reached the plugin.
    pub fresh_reads: u64,
    /// Exports performed.
    pub exports: u64,
    /// Cancellations.
    pub cancellations: u64,
    /// Revocations.
    pub revocations: u64,
    /// Bytes received from the plugin.
    pub bytes_in: u64,
    /// Bytes served to the clients.
    pub bytes_out: u64,
    /// Mean latency in milliseconds.
    pub mean_latency_ms: f64,
    /// Cache entries held.
    pub cache_entries: usize,
    /// Cache bytes held.
    pub cache_bytes: u64,
    /// Assets held.
    pub asset_count: usize,
    /// Asset bytes held.
    pub asset_bytes: u64,
    /// Pairing secrets that are still usable.
    pub pairing_active: usize,
    /// The capabilities the known sessions reported.
    pub capabilities: Vec<SessionCapabilities>,
    /// The recent write audit records of the installation.
    ///
    /// The records hold identifiers, counts, hashes, and timestamps only.
    /// They are the records of the sessions of the installation the caller
    /// belongs to, so a client never sees the trail of another one.
    pub write_audit: Vec<WriteAuditReport>,
}

/// One audit record of an applied plan, as a client sees it.
///
/// The report carries the identity of the plan, its hash, the approval that
/// allowed it, the counts of the affected nodes, and the moment of the
/// apply. It never carries a layer name, a text, or any other design
/// content.
#[derive(Debug, Clone, Serialize)]
pub struct WriteAuditReport {
    /// The session the plan was applied in.
    pub session_id: String,
    /// The identity of the applied plan.
    pub plan_id: String,
    /// The hash of the applied plan.
    pub plan_hash: String,
    /// The approval that allowed the plan.
    pub approval_id: String,
    /// The number of operations of the plan.
    pub operations_count: u32,
    /// The number of created nodes.
    pub created: usize,
    /// The number of changed nodes.
    pub changed: usize,
    /// The number of removed nodes.
    pub removed: usize,
    /// The number of moved nodes.
    pub moved: usize,
    /// When the plan was applied.
    pub applied_at: String,
}

/// The capabilities of one session.
///
/// The report names the session and the capabilities of the plugin that
/// serves it. It never carries design content.
#[derive(Debug, Clone, Serialize)]
pub struct SessionCapabilities {
    /// The session the capabilities belong to.
    pub session_id: String,
    /// The capabilities of the session.
    pub capabilities: Vec<CapabilityReport>,
}

/// One reported capability.
#[derive(Debug, Clone, Serialize)]
pub struct CapabilityReport {
    /// The name of the capability.
    pub capability: String,
    /// The state of the capability.
    ///
    /// One of `available`, `unavailable`, or `unsupported`.
    pub state: String,
    /// Why the capability is not usable.
    ///
    /// The field is only present when the capability is not available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl DaemonStatus {
    /// A status without any activity, used before the daemon is running.
    pub fn empty(port: u16) -> Self {
        Self {
            daemon_version: crate::DAEMON_VERSION.to_string(),
            core_version: figtether_core::CORE_VERSION.to_string(),
            protocol_version: figtether_protocol::PROTOCOL_VERSION.to_string(),
            schema_version: figtether_model::SCHEMA_VERSION.to_string(),
            instance_id: String::new(),
            port,
            bind_address: "127.0.0.1".to_string(),
            uptime_seconds: 0,
            sessions: 0,
            plugin_connections: 0,
            queue_depth: 0,
            queue_depth_peak: 0,
            busy_rejections: 0,
            requests_total: 0,
            cache_hits: 0,
            fresh_reads: 0,
            exports: 0,
            cancellations: 0,
            revocations: 0,
            bytes_in: 0,
            bytes_out: 0,
            mean_latency_ms: 0.0,
            cache_entries: 0,
            cache_bytes: 0,
            asset_count: 0,
            asset_bytes: 0,
            pairing_active: 0,
            capabilities: Vec::new(),
            write_audit: Vec::new(),
        }
    }
}

/// The answer of the pairing endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct PairSecretResponse {
    /// The one time pairing secret.
    pub secret: String,
    /// Lifetime of the secret in seconds.
    pub expires_in_seconds: u64,
}

/// What a revocation changed.
#[derive(Debug, Clone, Serialize)]
pub struct RevokeOutcome {
    /// Jobs that were cancelled.
    pub cancelled_jobs: usize,
    /// Cache entries that were dropped.
    pub invalidated_cache_entries: usize,
    /// Assets that were dropped.
    pub invalidated_assets: usize,
    /// Traversal cursors that were dropped.
    pub invalidated_cursors: usize,
}

/// The header that a local control request has to carry.
///
/// A native client sends it. A browser cannot send it in a simple request: a
/// custom header forces a preflight, and the daemon never answers one. That
/// is what keeps a foreign web page away from the control endpoints.
pub const CONTROL_HEADER: &str = "x-figtether-control";

/// The value the control header has to carry.
pub const CONTROL_HEADER_VALUE: &str = "1";

/// Build the raw HTTP request that mints a one time pairing secret.
///
/// The request builder lives here, next to the guard that checks the header,
/// so that no caller can forget it. Every caller in the workspace uses this
/// function.
pub fn pair_request(port: u16) -> String {
    control_request(port, "/control/pair")
}

/// Build the raw HTTP request that asks the daemon to stop.
pub fn shutdown_request(port: u16) -> String {
    control_request(port, "/control/shutdown")
}

/// Build a raw HTTP request against one of the control endpoints.
fn control_request(port: u16, path: &str) -> String {
    format!(
        "POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{CONTROL_HEADER}: {CONTROL_HEADER_VALUE}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
}

/// The sanitized status of the running daemon.
///
/// The status of the health endpoint carries no audit record: the health
/// endpoint is not authenticated, and the trail of the write path belongs
/// to the installation of the client that asks for it. A client reads the
/// trail through [`status_for`].
pub fn status(state: &Arc<DaemonState>) -> DaemonStatus {
    // The empty owner names no installation, so no record of the trail
    // belongs to it.
    status_for(state, "")
}

/// The sanitized status of the daemon for one installation.
///
/// The audit records of the trail belong to the sessions of an installation,
/// so a client only ever sees the records of its own installation. The
/// status of every other field is the same for every caller: the daemon
/// serves one installation.
pub fn status_for(state: &Arc<DaemonState>, owner: &str) -> DaemonStatus {
    let metrics = state.metrics.snapshot();
    let scheduler = guarded(SchedulerStats::default(), || state.scheduler.stats());
    let cache = guarded(CacheStats::default(), || state.cache.stats());
    let (asset_count, asset_bytes) = guarded((0, 0), || state.assets.stats());
    let sessions = guarded(0, || state.registry.len());
    let uptime = Utc::now()
        .signed_duration_since(state.started_at)
        .num_seconds()
        .max(0) as u64;

    DaemonStatus {
        daemon_version: crate::DAEMON_VERSION.to_string(),
        core_version: figtether_core::CORE_VERSION.to_string(),
        protocol_version: figtether_protocol::PROTOCOL_VERSION.to_string(),
        schema_version: figtether_model::SCHEMA_VERSION.to_string(),
        instance_id: state.instance_id.to_string(),
        port: state.port,
        bind_address: state.bind_address.clone(),
        uptime_seconds: uptime,
        sessions,
        plugin_connections: state.hub.connection_count(),
        queue_depth: scheduler.queued,
        queue_depth_peak: metrics.queue_depth_peak,
        busy_rejections: metrics.busy_rejections,
        requests_total: metrics.requests_total,
        cache_hits: metrics.cache_hits,
        fresh_reads: metrics.fresh_reads,
        exports: metrics.exports,
        cancellations: metrics.cancellations,
        revocations: metrics.revocations,
        bytes_in: metrics.bytes_in,
        bytes_out: metrics.bytes_out,
        mean_latency_ms: metrics.mean_latency_ms(),
        cache_entries: cache.entries,
        cache_bytes: cache.bytes,
        asset_count,
        asset_bytes,
        pairing_active: state.pairing.active_count(),
        capabilities: capability_report(state),
        write_audit: audit_report(state, owner),
    }
}

/// Build the audit report of the sessions of one installation.
///
/// The report holds the records of the trail whose session belongs to the
/// installation of the caller. A record of another installation never
/// reaches a client.
fn audit_report(state: &Arc<DaemonState>, owner: &str) -> Vec<WriteAuditReport> {
    let sessions = guarded(Vec::new(), || state.registry.list_for_client(owner));
    let owned: std::collections::HashSet<SessionId> = sessions
        .into_iter()
        .map(|session| session.session_id)
        .collect();
    guarded(Vec::new(), || state.writes.audit_entries())
        .into_iter()
        .filter(|entry| owned.contains(&entry.session_id))
        .map(|entry| WriteAuditReport {
            session_id: entry.session_id.to_string(),
            // The plan id and the approval id are minted by the agent and by
            // the daemon, so they are sanitized before they leave the
            // daemon: an id stays a short label.
            plan_id: sanitize(&entry.record.plan_id),
            plan_hash: entry.record.plan_hash,
            approval_id: sanitize(&entry.record.approval_id),
            operations_count: entry.record.operations_count,
            created: entry.record.created,
            changed: entry.record.changed,
            removed: entry.record.removed,
            moved: entry.record.moved,
            applied_at: entry.record.applied_at.to_rfc3339(),
        })
        .collect()
}

/// Build the capability report of the known sessions.
///
/// The report carries the capability name, its state, and the reason of a
/// capability that is not usable. Every value comes from the plugin, so it is
/// sanitized before it leaves the daemon: control characters become spaces
/// and the value is cut at a fixed length. A capability report never carries
/// design content.
fn capability_report(state: &Arc<DaemonState>) -> Vec<SessionCapabilities> {
    state
        .hub
        .capability_reports()
        .into_iter()
        .map(|(session_id, probes)| {
            let capabilities = probes
                .into_iter()
                .map(|(name, capability_state)| {
                    let (state_name, reason) = match &capability_state {
                        CapabilityState::Available => ("available", None),
                        CapabilityState::Unavailable { reason } => {
                            ("unavailable", Some(sanitize(reason)))
                        }
                        CapabilityState::Unsupported { reason } => {
                            ("unsupported", Some(sanitize(reason)))
                        }
                    };
                    CapabilityReport {
                        capability: sanitize(&name),
                        state: state_name.to_string(),
                        reason,
                    }
                })
                .collect();
            SessionCapabilities {
                session_id: session_id.to_string(),
                capabilities,
            }
        })
        .collect()
}

/// Sanitize a value that the plugin sent.
///
/// The plugin is not trusted with the shape of its own text. Control
/// characters become spaces and the value is cut at a fixed length, so a
/// capability name or a reason stays a short label.
fn sanitize(value: &str) -> String {
    let mut out = String::with_capacity(value.len().min(MAX_SANITIZED_TEXT));
    for (count, ch) in value.chars().enumerate() {
        if count >= MAX_SANITIZED_TEXT {
            break;
        }
        if ch.is_control() {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out.trim().to_string()
}

/// Mint a one time pairing secret.
///
/// The secret is bound to the local installation of the daemon. The plugin
/// that spends it opens a session of that installation, so the local clients
/// of this machine see the session and a client of another machine never
/// does.
pub fn mint_pairing_secret(state: &Arc<DaemonState>) -> PairSecretResponse {
    let secret = state.pairing.mint(state.owner_key());
    PairSecretResponse {
        secret: secret.as_str().to_string(),
        expires_in_seconds: PAIRING_SECRET_TTL.as_secs(),
    }
}

/// Revoke the grant of a session of one owner.
///
/// The check is the access control: a local client may only revoke the
/// sessions of the installation it belongs to. A session of another
/// installation is refused with `PERMISSION_DENIED`, and a session nobody
/// owns is never revoked through this path.
pub fn revoke_session_of(
    state: &Arc<DaemonState>,
    owner: &str,
    session_id: &SessionId,
) -> Result<RevokeOutcome, FigError> {
    let session = match guarded(None::<figtether_core::Session>, || {
        state.registry.get(session_id)
    }) {
        Some(session) => session,
        None => {
            return Err(FigError::new(
                ErrorCode::SessionNotFound,
                "the session does not exist",
            ))
        }
    };
    if session.client_id != owner {
        return Err(FigError::new(
            ErrorCode::PermissionDenied,
            "the session does not belong to this installation",
        ));
    }
    revoke_session(state, session_id)
}

/// Revoke the grant of a session.
///
/// The revocation cancels the queued work of the session, drops the cache
/// entries and the assets of the grant epoch, and refuses later reads.
///
/// The path is for the session itself: the plugin reports its own revoked
/// grant, and the daemon tears the session down. A local client goes through
/// [`revoke_session_of`], which checks the owner first.
pub fn revoke_session(
    state: &Arc<DaemonState>,
    session_id: &SessionId,
) -> Result<RevokeOutcome, FigError> {
    let session = match guarded(None::<figtether_core::Session>, || {
        state.registry.get(session_id)
    }) {
        Some(session) => session,
        None => {
            return Err(FigError::new(
                ErrorCode::SessionNotFound,
                "the session does not exist",
            ))
        }
    };

    let grant_epoch = session.grant_epoch();
    let grant_id = session.grant.as_ref().map(|grant| grant.grant_id);

    // The approvals of the session die with the grant: an approval never
    // outlives the grant it was issued under.
    state.writes.invalidate_session(session_id);
    if let Some(epoch) = grant_epoch {
        state.writes.invalidate_grant(epoch);
    }

    let cancelled_jobs = guarded(0, || state.scheduler.cancel_session(session_id));

    let mut invalidated_cache_entries = 0;
    if let Some(epoch) = grant_epoch {
        invalidated_cache_entries += guarded(0, || state.cache.invalidate_grant(epoch));
    }
    invalidated_cache_entries += guarded(0, || {
        state
            .cache
            .invalidate_source(&SourceKey::Session(*session_id))
    });

    let mut invalidated_assets = 0;
    if let Some(grant_id) = grant_id {
        invalidated_assets = guarded(0, || state.assets.invalidate_grant(&grant_id));
    }
    // A session can hold several grants: every scope change issues a new
    // one. The assets of the older grants die with the session as well.
    for grant_id in guarded(Vec::new(), || state.grants.forget(session_id)) {
        invalidated_assets += guarded(0, || state.assets.invalidate_grant(&grant_id));
    }

    // The cursors of the session die with the grant: a traversal that
    // continues under a revoked grant would read nothing anyway.
    let invalidated_cursors = guarded(0, || state.cursors.drop_session(session_id));
    if let Some(epoch) = grant_epoch {
        let _ = guarded(0, || state.cursors.drop_grant(epoch));
    }

    let notice = BridgeFrame::new(
        MessageType::Revoke,
        "revoke",
        serde_json::json!({ "reason": "the user revoked the grant" }),
    )
    .with_session(*session_id);
    let _ = state.hub.try_send(session_id, notice);

    state.hub.revoke(session_id);

    guarded((), || {
        let _ = state.registry.revoke(session_id);
        let _ = state.registry.set_state(session_id, SessionState::Revoked);
    });

    Metrics::incr(&state.metrics.revocations);

    tracing::info!(code = "REVOKED", "grant revoked");

    Ok(RevokeOutcome {
        cancelled_jobs,
        invalidated_cache_entries,
        invalidated_assets,
        invalidated_cursors,
    })
}

/// Drop the design memory of the daemon.
///
/// The cache entries and the assets of every session are dropped. The daemon
/// keeps no design data after it stopped.
pub fn wipe_design_memory(state: &Arc<DaemonState>) {
    let sessions = guarded(Vec::new(), || state.registry.list_all());
    for session in sessions {
        if let Some(epoch) = session.grant_epoch {
            let _ = guarded(0, || state.cache.invalidate_grant(epoch));
            let _ = guarded(0, || state.cursors.drop_grant(epoch));
        }
        let _ = guarded(0, || {
            state
                .cache
                .invalidate_source(&SourceKey::Session(session.session_id))
        });
        let _ = guarded(0, || state.cursors.drop_session(&session.session_id));
    }
    let _ = guarded(0, || state.cache.evict_expired(std::time::Instant::now()));
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::bridge::SessionLink;
    use crate::secrets::Secret;
    use uuid::Uuid;

    /// A state with one registered session.
    fn state_with_session(owner: &str) -> (Arc<DaemonState>, SessionId) {
        let state = Arc::new(DaemonState::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            38947,
            figtether_core::ResourceLimits::default(),
            Secret::generate(),
        ));
        let session_id = SessionId::new();
        let probes = figtether_model::capabilities::Capabilities::new();
        state
            .registry
            .register(session_id, owner.to_string(), probes)
            .expect("registers");
        (state, session_id)
    }

    /// Attach a plugin connection that reported capabilities.
    fn attach_plugin(
        state: &Arc<DaemonState>,
        session_id: SessionId,
        capabilities: BTreeMap<String, CapabilityState>,
    ) {
        let (tx, _rx) = tokio::sync::mpsc::channel(1);
        state.hub.attach(
            session_id,
            SessionLink {
                epoch: 1,
                client_id: "figma-plugin-ui".to_string(),
                credential: Secret::generate(),
                tx,
                capabilities,
            },
        );
    }

    #[test]
    fn the_status_carries_no_secret_and_no_design_content() {
        let status = DaemonStatus::empty(38947);
        let json = serde_json::to_string(&status).expect("serializes");
        let lower = json.to_lowercase();
        for forbidden in [
            "secret",
            "credential",
            "token",
            "layer",
            "node",
            "text",
            "file_key",
            "filekey",
            "document",
            "name",
        ] {
            assert!(
                !lower.contains(forbidden),
                "the status must not carry {forbidden}: {json}"
            );
        }
    }

    #[test]
    fn the_status_carries_the_expected_fields() {
        let status = DaemonStatus::empty(38947);
        let json = serde_json::to_value(&status).expect("serializes");
        assert_eq!(json["port"], 38947);
        assert_eq!(json["bind_address"], "127.0.0.1");
        assert_eq!(json["sessions"], 0);
        assert_eq!(json["queue_depth"], 0);
        assert!(json["daemon_version"].is_string());
        assert!(json["protocol_version"].is_string());
        assert!(
            json["capabilities"].as_array().expect("a list").is_empty(),
            "a daemon without a plugin reports no capability"
        );
    }

    #[test]
    fn the_status_never_names_a_session() {
        let status = DaemonStatus::empty(38947);
        let json = serde_json::to_value(&status).expect("serializes");
        assert!(json.get("session_id").is_none());
        assert!(json.get("sessions").is_some());
    }

    #[test]
    fn the_status_carries_the_capability_report_with_the_reason() {
        let (state, session_id) = state_with_session("installation-1");
        let mut capabilities = BTreeMap::new();
        capabilities.insert("nodes".to_string(), CapabilityState::Available);
        capabilities.insert(
            "export_svg".to_string(),
            CapabilityState::Unavailable {
                reason: "the seat does not expose SVG export".to_string(),
            },
        );
        attach_plugin(&state, session_id, capabilities);

        let status = status(&state);
        assert_eq!(status.capabilities.len(), 1);
        let report = &status.capabilities[0];
        assert_eq!(report.session_id, session_id.to_string());
        assert_eq!(report.capabilities.len(), 2);
        let nodes = report
            .capabilities
            .iter()
            .find(|entry| entry.capability == "nodes")
            .expect("the nodes capability");
        assert_eq!(nodes.state, "available");
        assert!(nodes.reason.is_none());
        let export = report
            .capabilities
            .iter()
            .find(|entry| entry.capability == "export_svg")
            .expect("the export capability");
        assert_eq!(export.state, "unavailable");
        assert_eq!(
            export.reason.as_deref(),
            Some("the seat does not expose SVG export")
        );
    }

    #[test]
    fn the_capability_report_is_sanitized() {
        let (state, session_id) = state_with_session("installation-1");
        let long = "x".repeat(400);
        let mut capabilities = BTreeMap::new();
        capabilities.insert(
            "export_svg".to_string(),
            CapabilityState::Unsupported {
                reason: format!("{long}\nsecond line"),
            },
        );
        attach_plugin(&state, session_id, capabilities);

        let status = status(&state);
        let reason = status.capabilities[0].capabilities[0]
            .reason
            .clone()
            .expect("a reason");
        assert!(reason.len() <= MAX_SANITIZED_TEXT, "{}", reason.len());
        assert!(!reason.contains('\n'), "{reason}");
    }

    #[test]
    fn a_session_of_another_owner_cannot_be_revoked() {
        let (state, session_id) = state_with_session("installation-1");
        let error = revoke_session_of(&state, "installation-2", &session_id)
            .expect_err("the session of another installation is refused");
        assert_eq!(error.code, ErrorCode::PermissionDenied);
    }

    #[test]
    fn the_owner_of_a_session_may_revoke_it() {
        let (state, session_id) = state_with_session("installation-1");
        let outcome = revoke_session_of(&state, "installation-1", &session_id).expect("revoked");
        assert_eq!(outcome.cancelled_jobs, 0);
        let session = state.registry.get(&session_id).expect("known session");
        assert_eq!(session.state, SessionState::Revoked);
    }

    #[test]
    fn an_unknown_session_is_not_revoked() {
        let (state, _session_id) = state_with_session("installation-1");
        let error = revoke_session_of(&state, "installation-1", &SessionId::new())
            .expect_err("the session does not exist");
        assert_eq!(error.code, ErrorCode::SessionNotFound);
    }

    #[test]
    fn a_pairing_secret_belongs_to_the_installation() {
        let state = Arc::new(DaemonState::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            38947,
            figtether_core::ResourceLimits::default(),
            Secret::generate(),
        ));
        let pair = mint_pairing_secret(&state);
        let paired = state
            .pairing
            .consume(&pair.secret)
            .expect("the secret is accepted");
        assert_eq!(paired.owner, state.owner_key());
    }

    /// An audit record of an applied plan.
    fn audit_record(plan_id: &str) -> figtether_core::write::AuditRecord {
        figtether_core::write::AuditRecord {
            plan_id: plan_id.to_string(),
            plan_hash: "sha256:abc".to_string(),
            approval_id: "apr_1".to_string(),
            operations_count: 1,
            created: 1,
            changed: 0,
            removed: 0,
            moved: 0,
            applied_at: chrono::Utc::now(),
        }
    }

    /// A state with two sessions of two installations.
    ///
    /// The first session belongs to the installation of the daemon, the
    /// second one to another installation.
    fn state_with_two_installations() -> (Arc<DaemonState>, SessionId, SessionId) {
        let state = Arc::new(DaemonState::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            38947,
            figtether_core::ResourceLimits::default(),
            Secret::generate(),
        ));
        let own = SessionId::new();
        let other = SessionId::new();
        let probes = figtether_model::capabilities::Capabilities::new();
        state
            .registry
            .register(own, state.owner_key(), probes.clone())
            .expect("registers");
        state
            .registry
            .register(other, "installation-2".to_string(), probes)
            .expect("registers");
        (state, own, other)
    }

    #[test]
    fn the_audit_report_holds_the_records_of_the_installation_only() {
        let (state, own, other) = state_with_two_installations();
        state.writes.record_audit(&own, audit_record("plan_own"));
        state
            .writes
            .record_audit(&other, audit_record("plan_other"));

        let status = status_for(&state, &state.owner_key());
        assert_eq!(status.write_audit.len(), 1);
        assert_eq!(status.write_audit[0].plan_id, "plan_own");
        assert_eq!(status.write_audit[0].session_id, own.to_string());

        let other_status = status_for(&state, "installation-2");
        assert_eq!(other_status.write_audit.len(), 1);
        assert_eq!(other_status.write_audit[0].plan_id, "plan_other");
        assert_eq!(other_status.write_audit[0].session_id, other.to_string());
    }

    #[test]
    fn the_audit_report_holds_no_design_content() {
        let (state, own, _other) = state_with_two_installations();
        state.writes.record_audit(&own, audit_record("plan_own"));

        let status = status_for(&state, &state.owner_key());
        let json = serde_json::to_string(&status).expect("serializes");
        let lower = json.to_lowercase();
        for forbidden in ["secret", "credential", "token", "layer", "text", "document"] {
            assert!(
                !lower.contains(forbidden),
                "the audit report must not carry {forbidden}: {json}"
            );
        }
        assert!(json.contains("plan_own"), "{json}");
    }

    #[test]
    fn the_health_status_carries_no_audit_record() {
        let (state, own, _other) = state_with_two_installations();
        state.writes.record_audit(&own, audit_record("plan_own"));

        // The health endpoint is not authenticated, so it carries no record
        // of the trail.
        let status = status(&state);
        assert!(status.write_audit.is_empty());

        // The in-process view of the daemon carries the records of its own
        // installation.
        let status = status_for(&state, &state.owner_key());
        assert_eq!(status.write_audit.len(), 1);
        assert_eq!(status.write_audit[0].plan_id, "plan_own");
    }
}
