//! The local client IPC.
//!
//! The MCP adapter and the CLI talk to the daemon over an authenticated
//! loopback WebSocket. The first frame of every connection is the client
//! credential, which the daemon reads from the owner only runtime directory
//! and compares in constant time.
//!
//! The credential names the local installation of the client. The daemon
//! resolves the owner of the sessions from it, so every client of this
//! machine sees the same sessions and a client of another machine sees none.
//! The client id the client presents stays a label: it names the caller for
//! the diagnostics and for the per client queue, and it never decides the
//! scope of a request.
//!
//! A request travels through four stages:
//!
//! 1. admission: the session has to exist, the grant has to be active, and
//!    the request has to name a session;
//! 2. scheduling: the job enters the bounded queue of the scheduler, which
//!    refuses a saturated queue with `BUSY`;
//! 3. execution: the worker sends one allowlisted operation to the plugin and
//!    waits for the answer;
//! 4. normalization: the plugin payload becomes the normalized model and the
//!    response envelope.
//!
//! Every job leaves the scheduler on every path. The scheduler holds the lane
//! of a session from the moment a job starts until the moment it is
//! completed, so a job that is never completed would block every later
//! operation of that session forever.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::ws::Message;
use chrono::{DateTime, Utc};
use figtether_core::{
    normalize_document, AssetStore, CacheKey, Job, JobKind, JobStatus, Metrics, Priority,
    ResourceLimits, SourceKey,
};
use figtether_model::envelope::{
    Consistency, Coverage, Envelope, Freshness, Observation, SourceKind, SourceRef,
};
use figtether_model::errors::{ErrorCode, FigError};
use figtether_model::ids::{GrantId, JobId, ObservationId, SessionId};
use figtether_model::normalized::NormalizedNode;
use figtether_model::tools::{
    AssetFormat, ConsistencyRequest, ExportAssetsParams, GetChangesParams, GetDesignTokensParams,
    GetNodesParams, GetSelectionParams, InspectDesignParams, ListNodesParams, WatchScopeParams,
};
use figtether_protocol::client::{ClientAuth, ClientRequest, ClientResponse};
use figtether_protocol::plugin::{PluginDocument, PluginExport, PluginNode};
use figtether_protocol::{BridgeFrame, BridgeOp, MessageType, AUTH_WINDOW};
use futures_util::{SinkExt, StreamExt};

use crate::bridge::{protocol_version_matches, BridgeOutcome};
use crate::control;
use crate::cursor::{self, CursorRecord};
use crate::write_path;
use crate::DaemonState;

/// Correlation id of the authentication answer.
const AUTH_REQUEST_ID: &str = "auth";

/// Number of nodes one page of a traversal may carry.
const PAGE_LIMIT: u32 = 200;

/// Number of bytes the envelope reserves for everything but the node list.
const ENVELOPE_RESERVE: usize = 4096;

/// One job the daemon dispatched.
#[derive(Debug, Clone)]
pub struct JobRecord {
    /// Identity of the job.
    pub job_id: JobId,
    /// Kind of work.
    pub kind: JobKind,
    /// Operation the plugin has to perform.
    ///
    /// The core scheduler knows the kind of a job, not the operation: the
    /// operation travels in the ledger of the daemon.
    pub op: BridgeOp,
    /// Session the job belongs to.
    pub session_id: Option<SessionId>,
    /// Lifecycle state of the job.
    pub status: JobStatus,
    /// When the job was created.
    pub created_at: chrono::DateTime<Utc>,
}

/// The ledger of the jobs the daemon dispatched.
///
/// The scheduler of the core does not expose a job lookup, so the daemon
/// keeps its own ledger of the jobs it submitted.
#[derive(Debug, Default)]
pub struct JobLedger {
    /// Known jobs by id.
    inner: Mutex<HashMap<JobId, JobRecord>>,
}

impl JobLedger {
    /// Create an empty ledger.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a new job.
    pub fn insert(&self, record: JobRecord) {
        self.lock().insert(record.job_id, record);
    }

    /// Update the state of a job.
    ///
    /// The first terminal state wins: a job that was cancelled stays
    /// cancelled, even when its answer arrives later.
    pub fn set_status(&self, job_id: &JobId, status: JobStatus) {
        if let Some(record) = self.lock().get_mut(job_id) {
            if is_terminal(record.status) {
                return;
            }
            record.status = status;
        }
    }

    /// Look up a job.
    pub fn get(&self, job_id: &JobId) -> Option<JobRecord> {
        self.lock().get(job_id).cloned()
    }

    /// The queued jobs of a session.
    pub fn queued_of(&self, session_id: &SessionId) -> Vec<JobRecord> {
        self.lock()
            .values()
            .filter(|record| {
                record.session_id == Some(*session_id) && record.status == JobStatus::Queued
            })
            .cloned()
            .collect()
    }

    /// Every job of a session.
    pub fn of_session(&self, session_id: &SessionId) -> Vec<JobRecord> {
        self.lock()
            .values()
            .filter(|record| record.session_id == Some(*session_id))
            .cloned()
            .collect()
    }

    /// Lock the ledger, recovering from a poisoned lock.
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<JobId, JobRecord>> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Whether a job status is a terminal one.
fn is_terminal(status: JobStatus) -> bool {
    matches!(
        status,
        JobStatus::Completed
            | JobStatus::Failed
            | JobStatus::Cancelled
            | JobStatus::TimedOut
            | JobStatus::Superseded
    )
}

/// What the daemon does with one client request.
#[derive(Debug, Clone)]
pub struct Plan {
    /// Operation the plugin has to perform.
    pub op: BridgeOp,
    /// Kind of work.
    pub kind: JobKind,
    /// Session the work belongs to.
    pub session_id: Option<SessionId>,
    /// Parameters of the request, as the client sent them.
    ///
    /// The page size of a traversal is the one exception: the daemon clamps
    /// it to the page it is willing to serve.
    pub params: serde_json::Value,
    /// Priority of the job.
    pub priority: Priority,
    /// What the daemon needs to know about the request besides its parameters.
    pub context: ReadContext,
    /// Whether the answer of the job may be kept in the cache.
    ///
    /// A write never caches: the answer of a write is not a read of the
    /// document.
    pub cacheable: bool,
    /// Whether the job mutates the document.
    ///
    /// The core knows no write job kind yet, so a write travels as a job
    /// that the daemon marks as a write.
    pub write: bool,
}

/// What the daemon needs to know about a read besides the plugin parameters.
#[derive(Debug, Clone, Default)]
pub struct ReadContext {
    /// Root node of the traversal.
    pub root_id: Option<String>,
    /// Node ids the request names.
    pub node_ids: Vec<String>,
    /// Fingerprint of the field projection of the request.
    pub fields: Vec<String>,
    /// Cursor the client presented.
    pub cursor: Option<String>,
    /// Consistency lane of the request.
    pub consistency: ConsistencyRequest,
    /// Maximum age of a cached answer, in milliseconds.
    pub max_age_ms: Option<u64>,
    /// Fingerprint of the export settings of the request.
    pub export_settings: Option<String>,
}

/// The identity of an authenticated local client.
///
/// The owner decides which sessions the client may see. It is the local
/// installation the presented credential belongs to. The label is the client
/// id the client presented: it names the caller for the diagnostics and for
/// the per client queue, and it never decides the scope of a request.
#[derive(Debug, Clone)]
pub struct ClientIdentity {
    /// The local installation that owns the sessions of this client.
    pub owner: String,
    /// The client id the client presented.
    pub label: String,
}

/// Serve one client connection.
pub async fn serve_client(socket: axum::extract::ws::WebSocket, state: Arc<DaemonState>) {
    let (mut sender, mut receiver) = socket.split();

    let auth = match read_client_auth(&mut receiver).await {
        Ok(auth) => auth,
        Err(code) => {
            tracing::warn!(code = code, "client authentication refused");
            let _ = sender.send(Message::Close(None)).await;
            return;
        }
    };

    if !protocol_version_matches(&auth.protocol_version) {
        tracing::warn!(code = "PROTOCOL_MISMATCH", "client authentication refused");
        let _ = sender.send(Message::Close(None)).await;
        return;
    }

    // The credential decides the owner of the sessions. A client that
    // presents a wrong credential has no owner, so it never sees a session
    // and it never revokes one.
    let Some(owner) = state.owner_of(&auth.credential) else {
        tracing::warn!(code = "AUTH_REQUIRED", "client authentication refused");
        let _ = sender.send(Message::Close(None)).await;
        return;
    };
    let identity = ClientIdentity {
        owner,
        label: auth.client_id.clone(),
    };

    let status = control::status_for(&state, &identity.owner);
    let envelope = status_envelope(AUTH_REQUEST_ID, status);
    let response = ClientResponse::Ok {
        request_id: AUTH_REQUEST_ID.to_string(),
        envelope,
    };
    if let Err(err) = send_response(&mut sender, &response).await {
        tracing::warn!(code = "INTERNAL_ERROR", "client answer failed: {err}");
        return;
    }

    tracing::info!(code = "CLIENT_ACCEPTED", "local client authenticated");

    while let Some(incoming) = receiver.next().await {
        let bytes = match incoming {
            Ok(Message::Text(text)) => text.as_str().as_bytes().to_vec(),
            Ok(Message::Binary(binary)) => binary.to_vec(),
            Ok(Message::Close(_)) => break,
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => continue,
            Err(_) => break,
        };

        let request: ClientRequest = match serde_json::from_slice(&bytes) {
            Ok(request) => request,
            Err(_) => {
                let response = ClientResponse::Error {
                    request_id: String::new(),
                    error: FigError::new(ErrorCode::ProtocolMismatch, "the request is malformed"),
                };
                if send_response(&mut sender, &response).await.is_err() {
                    break;
                }
                continue;
            }
        };

        let response = dispatch(&state, &identity, request).await;
        if send_response(&mut sender, &response).await.is_err() {
            break;
        }
    }

    tracing::info!(code = "CLIENT_CLOSED", "local client disconnected");
}

/// The counters of the scheduler.
fn scheduler_stats(state: &Arc<DaemonState>) -> figtether_core::SchedulerStats {
    crate::guarded(figtether_core::SchedulerStats::default(), || {
        state.scheduler.stats()
    })
}

/// Compare a presented client credential with the stored one.
///
/// The comparison runs in constant time, so a caller cannot learn the
/// credential by timing the answer.
pub fn client_credential_matches(state: &DaemonState, presented: &str) -> bool {
    state.credential_matches(presented)
}

/// Read the first frame of a client connection.
///
/// The first frame has to be the client authentication. Nothing else is
/// accepted before it.
async fn read_client_auth<S>(receiver: &mut S) -> Result<ClientAuth, &'static str>
where
    S: futures_util::Stream<Item = Result<Message, axum::Error>> + Unpin,
{
    let message = match tokio::time::timeout(AUTH_WINDOW, receiver.next()).await {
        Err(_elapsed) => return Err("AUTH_REQUIRED"),
        Ok(None) => return Err("AUTH_REQUIRED"),
        Ok(Some(Err(_err))) => return Err("INTERNAL_ERROR"),
        Ok(Some(Ok(message))) => message,
    };

    let bytes = match message {
        Message::Text(text) => text.as_str().as_bytes().to_vec(),
        Message::Binary(binary) => binary.to_vec(),
        _ => return Err("AUTH_REQUIRED"),
    };

    serde_json::from_slice(&bytes).map_err(|_| "AUTH_REQUIRED")
}

/// Send one response to the client.
async fn send_response(
    sender: &mut futures_util::stream::SplitSink<axum::extract::ws::WebSocket, Message>,
    response: &ClientResponse,
) -> Result<(), serde_json::Error> {
    let body = serde_json::to_vec(response)?;
    let text =
        String::from_utf8(body).map_err(|err| serde_json::Error::io(std::io::Error::other(err)))?;
    sender
        .send(Message::Text(text.into()))
        .await
        .map_err(|err| serde_json::Error::io(std::io::Error::other(err.to_string())))
}

/// Handle one client request.
pub async fn dispatch(
    state: &Arc<DaemonState>,
    identity: &ClientIdentity,
    request: ClientRequest,
) -> ClientResponse {
    match request {
        ClientRequest::GetAsset { request_id, params } => {
            match crate::domain_call(ErrorCode::PermissionDenied, || {
                crate::asset_delivery::read(state, identity, &params)
            }) {
                Ok(data) => ClientResponse::Ok {
                    envelope: envelope_from_value(&request_id, None, data, Coverage::default()),
                    request_id,
                },
                Err(error) => ClientResponse::Error {
                    error: error.with_request_id(request_id.clone()),
                    request_id,
                },
            }
        }
        ClientRequest::Status { request_id } => {
            // The status of a client carries the audit records of the
            // sessions of its own installation only.
            let status = control::status_for(state, &identity.owner);
            ClientResponse::Ok {
                request_id: request_id.clone(),
                envelope: status_envelope(&request_id, status),
            }
        }
        ClientRequest::PairSecret { request_id } => {
            let pair = control::mint_pairing_secret(state);
            let envelope = envelope_from_value(
                &request_id,
                None,
                serde_json::json!({
                    "secret": pair.secret,
                    "expires_in_seconds": pair.expires_in_seconds,
                }),
                Coverage::default(),
            );
            ClientResponse::Ok {
                request_id,
                envelope,
            }
        }
        ClientRequest::ListSessions { request_id, .. } => {
            // The list is scoped to the owner of the credential. A client of
            // another installation never sees a session of this one.
            let sessions: Vec<serde_json::Value> = crate::guarded(Vec::new(), || {
                state.registry.list_for_client(&identity.owner)
            })
            .into_iter()
            .map(|session| {
                serde_json::json!({
                    "session_id": session.session_id.to_string(),
                    "client_id": session.client_id,
                    "state": session.state.as_str(),
                    "grant_epoch": session.grant_epoch,
                    "connection_epoch": session.connection_epoch,
                    "root_ids": session.root_ids.len(),
                    "granted_root_ids": session.root_ids,
                    "grant_mode": state.registry.grant_mode(&session.session_id).map(|mode| mode.as_str()),
                })
            })
            .collect();
            let envelope = envelope_from_value(
                &request_id,
                None,
                serde_json::json!({ "sessions": sessions, "write_budgets": {
                    "max_operations_per_plan":state.limits.max_operations_per_plan,
                    "max_created_nodes_per_plan":state.limits.max_created_nodes_per_plan,
                    "max_property_changes_per_plan":state.limits.max_property_changes_per_plan,
                    "max_text_characters_per_plan":state.limits.max_text_characters_per_plan,
                    "max_plans_per_minute":state.limits.max_write_operations_per_minute
                } }),
                Coverage::default(),
            );
            ClientResponse::Ok {
                request_id,
                envelope,
            }
        }
        ClientRequest::GetJobStatus { request_id, params } => {
            let job_id = params
                .get("job_id")
                .and_then(|value| value.as_str())
                .and_then(|value| JobId::from_str(value).ok());
            match job_id.and_then(|job_id| state.jobs.get(&job_id)) {
                Some(record) => {
                    let envelope = envelope_from_value(
                        &request_id,
                        None,
                        serde_json::json!({
                            "job": {
                                "job_id": record.job_id.to_string(),
                                "kind": format!("{:?}", record.kind),
                                "op": record.op.as_str(),
                                "session_id": record.session_id.map(|id| id.to_string()),
                                "status": format!("{:?}", record.status),
                                "created_at": record.created_at.to_rfc3339(),
                            }
                        }),
                        Coverage::default(),
                    );
                    ClientResponse::Ok {
                        request_id,
                        envelope,
                    }
                }
                None => ClientResponse::Error {
                    request_id,
                    error: FigError::new(ErrorCode::SessionNotFound, "the job does not exist"),
                },
            }
        }
        ClientRequest::Cancel { request_id, job_id } => {
            let cancelled = crate::guarded(false, || state.scheduler.cancel(&job_id));
            if cancelled {
                // The job leaves the scheduler now, so the lane of the
                // session is free even when the plugin is still working on
                // the operation.
                complete_job(state, &job_id, JobStatus::Cancelled);
                state.jobs.set_status(&job_id, JobStatus::Cancelled);
                // The waiting client learns about the cancellation at once.
                state.hub.complete(
                    &job_id.to_string(),
                    BridgeOutcome::Error(FigError::new(
                        ErrorCode::Cancelled,
                        "the client cancelled the operation",
                    )),
                );
                if let Some(session_id) =
                    state.jobs.get(&job_id).and_then(|record| record.session_id)
                {
                    let notice = BridgeFrame::new(
                        MessageType::Cancel,
                        job_id.to_string(),
                        serde_json::json!({ "reason": "the client cancelled the operation" }),
                    )
                    .with_session(session_id);
                    let _ = state.hub.try_send(&session_id, notice);
                }
            }
            let envelope = envelope_from_value(
                &request_id,
                None,
                serde_json::json!({ "cancelled": cancelled }),
                Coverage::default(),
            );
            ClientResponse::Ok {
                request_id,
                envelope,
            }
        }
        ClientRequest::Revoke {
            request_id,
            session_id,
        } => match control::revoke_session_of(state, &identity.owner, &session_id) {
            Ok(outcome) => {
                let envelope = envelope_from_value(
                    &request_id,
                    Some(session_id),
                    serde_json::json!({ "revoked": outcome }),
                    Coverage::default(),
                );
                ClientResponse::Ok {
                    request_id,
                    envelope,
                }
            }
            Err(err) => ClientResponse::Error {
                request_id: request_id.clone(),
                error: err.with_request_id(request_id),
            },
        },
        ClientRequest::GetSnapshotInfo { request_id, .. } => ClientResponse::Error {
            request_id: request_id.clone(),
            error: FigError::new(
                ErrorCode::CapabilityUnavailable,
                "this build does not keep snapshots",
            ),
        },
        ClientRequest::PreviewWrite { request_id, params } => {
            match preview_write(state, identity, &request_id, &params).await {
                Ok(envelope) => ClientResponse::Ok {
                    request_id,
                    envelope,
                },
                Err(err) => ClientResponse::Error {
                    request_id: request_id.clone(),
                    error: err.with_request_id(request_id),
                },
            }
        }
        ClientRequest::ApplyWrite { request_id, params } => {
            match apply_write(state, identity, &request_id, &params).await {
                Ok(envelope) => ClientResponse::Ok {
                    request_id,
                    envelope,
                },
                Err(err) => ClientResponse::Error {
                    request_id: request_id.clone(),
                    error: err.with_request_id(request_id),
                },
            }
        }
        ClientRequest::GetWriteApproval { request_id, params } => {
            match approval_status(state, identity, &request_id, &params) {
                Ok(envelope) => ClientResponse::Ok {
                    request_id,
                    envelope,
                },
                Err(err) => ClientResponse::Error {
                    request_id: request_id.clone(),
                    error: err.with_request_id(request_id),
                },
            }
        }
        other => {
            let request_id = request_id_of(&other);
            let plan = match plan_of(state, &other) {
                Ok(plan) => plan,
                Err(err) => {
                    return ClientResponse::Error {
                        request_id: request_id.clone(),
                        error: err.with_request_id(request_id),
                    }
                }
            };
            match run_job(state, &identity.label, &request_id, plan).await {
                Ok(envelope) => ClientResponse::Ok {
                    request_id,
                    envelope,
                },
                Err(err) => ClientResponse::Error {
                    request_id: request_id.clone(),
                    error: err.with_request_id(request_id),
                },
            }
        }
    }
}

/// Preview a write plan.
///
/// The daemon validates the plan against the write grant of the session, the
/// granted scope, and the write budgets, mints an approval for it, and asks
/// the plugin to resolve the plan against the live document without
/// mutating it. The answer carries the preview of the plugin and the
/// approval id the user has to approve.
async fn preview_write(
    state: &Arc<DaemonState>,
    identity: &ClientIdentity,
    request_id: &str,
    params: &serde_json::Value,
) -> Result<Envelope, FigError> {
    let budget = figtether_model::write::WriteBudget::from(&state.limits);
    let request = write_path::parse_plan(params, &budget)?;
    write_path::require_write_grant(state, &request.session_id, &identity.owner)?;
    write_path::require_scope(state, &request)?;

    let grant_epoch = grant_epoch_of(state, &request.session_id)?;
    let approval_id = write_path::mint_approval_id();
    let issued_at = Utc::now();
    let expires_at =
        issued_at + chrono::Duration::from_std(state.writes.approval_ttl()).unwrap_or_default();
    state.writes.record(write_path::ApprovalRequest::pending(
        approval_id.clone(),
        request.session_id,
        request.plan_hash.clone(),
        grant_epoch,
        issued_at,
        expires_at,
        identity.label.clone(),
        identity.owner.clone(),
        request.plan.clone(),
    ));

    let plan = Plan {
        op: BridgeOp::PreviewWrite,
        kind: JobKind::Read,
        session_id: Some(request.session_id),
        // The plan travels with the approval id and the hash the daemon
        // computed, so the plugin can bind the decision of the user to
        // exactly this plan and refuse a plan that changed in transit.
        params: serde_json::to_value(figtether_protocol::write::WritePlanPayload {
            plan: request.plan.clone(),
            approval_id: Some(approval_id.clone()),
            plan_hash: Some(request.plan_hash.clone()),
        })
        .unwrap_or_default(),
        priority: Priority::SmallRead,
        context: ReadContext::default(),
        cacheable: false,
        write: true,
    };
    let envelope = run_job(state, &identity.label, request_id, plan).await?;
    let preview = write_path::preview_of(&envelope.data);
    let mut data = serde_json::json!({
        "preview": preview,
        "approval_id": approval_id,
        "plan_id": request.plan_id(),
        "plan_hash": request.plan_hash,
    });
    if let Some(expires_at) = state.writes.expires_at(&approval_id) {
        data["expires_at"] = serde_json::Value::String(expires_at.to_rfc3339());
    }
    Ok(with_data(envelope, data))
}

/// Apply a write plan the user approved.
///
/// The daemon checks the approval token before the plugin is contacted: a
/// missing token, a token of another plan, and an approval that a grant
/// change invalidated are all refused here. After a successful apply the
/// daemon appends a record to the audit trail.
async fn apply_write(
    state: &Arc<DaemonState>,
    identity: &ClientIdentity,
    request_id: &str,
    params: &serde_json::Value,
) -> Result<Envelope, FigError> {
    let budget = figtether_model::write::WriteBudget::from(&state.limits);
    let request = write_path::parse_plan(params, &budget)?;
    write_path::require_write_grant(state, &request.session_id, &identity.owner)?;
    write_path::require_scope(state, &request)?;

    let grant_epoch = grant_epoch_of(state, &request.session_id)?;
    let token = params
        .get("approval_token")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    // The approval is verified before the plan is admitted and before the
    // plugin is contacted.
    let parts =
        state
            .writes
            .check_token(token, &request.plan_hash, &request.session_id, grant_epoch)?;
    write_path::admit(state, &request)?;

    let plan = Plan {
        op: BridgeOp::ApplyWrite,
        kind: JobKind::Read,
        session_id: Some(request.session_id),
        params: serde_json::to_value(figtether_protocol::write::WriteApplyPayload {
            plan: request.plan.clone(),
            approval_token: token.to_string(),
        })
        .unwrap_or_default(),
        priority: Priority::SmallRead,
        context: ReadContext::default(),
        cacheable: false,
        write: true,
    };
    let envelope = run_job(state, &identity.label, request_id, plan).await?;
    let result = write_path::result_of(&envelope.data);
    remember_write_creations(state, &request, grant_epoch, &result);
    state.writes.record_audit(
        &request.session_id,
        write_path::audit_record(&request, &parts.approval_id, &result),
    );
    let data = serde_json::json!({
        "result": result,
        "approval_id": parts.approval_id,
        "plan_id": request.plan_id(),
        "plan_hash": request.plan_hash,
    });
    Ok(with_data(envelope, data))
}

/// Learn only complete, authenticated creation receipts against the same grant epoch.
fn remember_write_creations(
    state: &Arc<DaemonState>,
    request: &write_path::WriteRequest,
    grant_epoch: u64,
    result: &serde_json::Value,
) {
    if result.get("status").and_then(serde_json::Value::as_str) != Some("applied") {
        return;
    }
    let Some(created) = result.get("created").and_then(serde_json::Value::as_array) else {
        return;
    };
    let operations: Vec<_> = request
        .parsed
        .operations
        .iter()
        .filter(|operation| {
            operation.kind == figtether_model::write::WriteOperationKind::CreateNode
        })
        .collect();
    if operations.len() != created.len() || created.is_empty() {
        return;
    }
    let mut relations = Vec::new();
    for (operation, id) in operations.iter().zip(created) {
        let (Some(id), Some(parent)) = (id.as_str(), operation.parent_id.as_deref()) else {
            return;
        };
        // Current daemon contracts require real, already learned parents. Do not
        // treat a synthetic operation alias as a document parent identifier.
        if parent.starts_with('#') {
            return;
        }
        relations.push((id.to_string(), parent.to_string()));
    }
    state
        .registry
        .record_created_nodes(&request.session_id, grant_epoch, &relations);
}

/// Answer the question about an approval.
///
/// Only the client that asked for the preview learns the answer: the token
/// is a capability, and a capability never leaks to another client.
fn approval_status(
    state: &Arc<DaemonState>,
    identity: &ClientIdentity,
    request_id: &str,
    params: &serde_json::Value,
) -> Result<Envelope, FigError> {
    let approval_id = params
        .get("approval_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            FigError::new(
                ErrorCode::ApprovalRequired,
                "the request does not name an approval",
            )
        })?;
    let status = state
        .writes
        .status(approval_id, &identity.label, &identity.owner)?;
    let mut data = serde_json::json!({
        "approval_id": approval_id,
        "status": status.as_str(),
    });
    if let Some(expires_at) = state.writes.expires_at(approval_id) {
        data["expires_at"] = serde_json::Value::String(expires_at.to_rfc3339());
    }
    match &status {
        write_path::ApprovalStatus::Approved { token } => {
            data["approval_token"] = serde_json::Value::String(token.clone());
        }
        write_path::ApprovalStatus::Rejected {
            reason: Some(reason),
        } => {
            data["reason"] = serde_json::Value::String(reason.clone());
        }
        _ => {}
    }
    Ok(envelope_from_value(
        request_id,
        None,
        data,
        Coverage::default(),
    ))
}

/// The grant epoch in force for a session.
fn grant_epoch_of(state: &Arc<DaemonState>, session_id: &SessionId) -> Result<u64, FigError> {
    crate::guarded(None, || state.registry.get(session_id))
        .and_then(|session| session.grant_epoch())
        .ok_or_else(|| FigError::new(ErrorCode::SessionNotFound, "the session holds no grant"))
}

/// Rebuild an envelope with another payload.
fn with_data(envelope: Envelope, data: serde_json::Value) -> Envelope {
    Envelope::new(
        envelope.request_id.clone(),
        envelope.source.clone(),
        envelope.observation.clone(),
        envelope.coverage.clone(),
        data,
    )
    .with_content_hash()
}

/// The correlation id of a client request.
fn request_id_of(request: &ClientRequest) -> String {
    match request {
        ClientRequest::Status { request_id }
        | ClientRequest::PairSecret { request_id }
        | ClientRequest::ListSessions { request_id, .. }
        | ClientRequest::ListNodes { request_id, .. }
        | ClientRequest::GetNodes { request_id, .. }
        | ClientRequest::InspectDesign { request_id, .. }
        | ClientRequest::GetDesignTokens { request_id, .. }
        | ClientRequest::ExportAssets { request_id, .. }
        | ClientRequest::GetAsset { request_id, .. }
        | ClientRequest::GetSelection { request_id, .. }
        | ClientRequest::GetSnapshotInfo { request_id, .. }
        | ClientRequest::WatchScope { request_id, .. }
        | ClientRequest::GetChanges { request_id, .. }
        | ClientRequest::PreviewWrite { request_id, .. }
        | ClientRequest::ApplyWrite { request_id, .. }
        | ClientRequest::GetWriteApproval { request_id, .. }
        | ClientRequest::GetJobStatus { request_id, .. }
        | ClientRequest::Cancel { request_id, .. }
        | ClientRequest::Revoke { request_id, .. } => request_id.clone(),
    }
}

/// The parameters of a client request.
fn params_of(request: &ClientRequest) -> Option<&serde_json::Value> {
    match request {
        ClientRequest::ListNodes { params, .. }
        | ClientRequest::GetNodes { params, .. }
        | ClientRequest::InspectDesign { params, .. }
        | ClientRequest::GetDesignTokens { params, .. }
        | ClientRequest::ExportAssets { params, .. }
        | ClientRequest::GetSelection { params, .. }
        | ClientRequest::WatchScope { params, .. }
        | ClientRequest::GetChanges { params, .. } => Some(params),
        _ => None,
    }
}

/// Translate a client request into a job plan.
///
/// The plan carries the operation of the request, so the plugin receives the
/// operation the client asked for and not the operation of the job kind.
fn plan_of(state: &Arc<DaemonState>, request: &ClientRequest) -> Result<Plan, FigError> {
    let limits = &state.limits;
    // The session is read before the parameters, so a request without a
    // session is refused as a missing session and not as a broken parameter.
    let raw = params_of(request).ok_or_else(|| {
        FigError::new(
            ErrorCode::InvalidLimit,
            "the request does not describe an editor operation",
        )
    })?;
    let session_id = session_of(raw)?;
    let (op, kind, params, context) = match request {
        ClientRequest::ListNodes { params, .. } => {
            let parsed: ListNodesParams = parse_params(params)?;
            validate_depth(limits, parsed.depth)?;
            let fields = projection(&parsed.fields);
            let mut params = params.clone();
            set_limit(&mut params, page_limit(parsed.limit));
            let context = ReadContext {
                root_id: Some(parsed.root_id.clone()),
                node_ids: vec![parsed.root_id.clone()],
                fields,
                cursor: parsed.cursor.clone(),
                consistency: parsed.consistency,
                max_age_ms: parsed.max_age_ms,
                export_settings: None,
            };
            (BridgeOp::ListNodes, JobKind::Read, params, context)
        }
        ClientRequest::GetNodes { params, .. } => {
            let parsed: GetNodesParams = parse_params(params)?;
            validate_depth(limits, parsed.depth)?;
            validate_batch(limits, parsed.node_ids.len())?;
            let context = ReadContext {
                root_id: None,
                node_ids: parsed.node_ids.clone(),
                fields: projection(&parsed.fields),
                cursor: None,
                consistency: parsed.consistency,
                max_age_ms: parsed.max_age_ms,
                export_settings: None,
            };
            (BridgeOp::ReadNodes, JobKind::Read, params.clone(), context)
        }
        ClientRequest::InspectDesign { params, .. } => {
            let parsed: InspectDesignParams = parse_params(params)?;
            let context = ReadContext {
                root_id: Some(parsed.root_id.clone()),
                node_ids: vec![parsed.root_id.clone()],
                ..ReadContext::default()
            };
            (
                BridgeOp::InspectDesign,
                JobKind::Inspect,
                params.clone(),
                context,
            )
        }
        ClientRequest::GetDesignTokens { params, .. } => {
            let parsed: GetDesignTokensParams = parse_params(params)?;
            let context = ReadContext {
                root_id: Some(parsed.root_id.clone()),
                node_ids: vec![parsed.root_id.clone()],
                ..ReadContext::default()
            };
            (
                BridgeOp::GetDesignTokens,
                JobKind::Read,
                params.clone(),
                context,
            )
        }
        ClientRequest::ExportAssets { params, .. } => {
            let parsed: ExportAssetsParams = parse_params(params)?;
            validate_batch(limits, parsed.node_ids.len())?;
            validate_scale(limits, parsed.scale)?;
            let context = ReadContext {
                root_id: None,
                node_ids: parsed.node_ids.clone(),
                export_settings: Some(export_settings(&parsed)),
                ..ReadContext::default()
            };
            (
                BridgeOp::ExportAssets,
                JobKind::Export,
                params.clone(),
                context,
            )
        }
        ClientRequest::GetSelection { params, .. } => {
            let _: GetSelectionParams = parse_params(params)?;
            (
                BridgeOp::GetSelection,
                JobKind::Read,
                params.clone(),
                ReadContext::default(),
            )
        }
        ClientRequest::WatchScope { params, .. } => {
            let parsed: WatchScopeParams = parse_params(params)?;
            let context = ReadContext {
                root_id: Some(parsed.root_id.clone()),
                node_ids: vec![parsed.root_id.clone()],
                ..ReadContext::default()
            };
            (
                BridgeOp::WatchScope,
                JobKind::Watch,
                params.clone(),
                context,
            )
        }
        ClientRequest::GetChanges { params, .. } => {
            let parsed: GetChangesParams = parse_params(params)?;
            let context = ReadContext {
                root_id: None,
                node_ids: vec![parsed.watch_id.clone()],
                ..ReadContext::default()
            };
            (
                BridgeOp::GetChanges,
                JobKind::Watch,
                params.clone(),
                context,
            )
        }
        _ => {
            return Err(FigError::new(
                ErrorCode::InvalidLimit,
                "the request does not describe an editor operation",
            ))
        }
    };

    let priority = match kind {
        JobKind::Export => Priority::Export,
        _ => Priority::SmallRead,
    };
    // The daemon enforces the scope as well as the plugin (FR03, double
    // enforcement). A node is inside the grant when it is one of the granted
    // roots or a descendant of a root that the daemon has already seen in an
    // answer. A node the daemon has never seen is refused: a traversal has to
    // start at a granted root, and the daemon learns the shape of the granted
    // tree from the answers it receives.
    enforce_scope(state, &session_id, op, &context)?;
    Ok(Plan {
        op,
        kind,
        session_id: Some(session_id),
        params,
        priority,
        context,
        cacheable: true,
        write: false,
    })
}

/// Refuse a request that names a node outside the granted scope.
///
/// The check is the daemon half of the double enforcement: the plugin checks
/// the scope against the live document, the daemon checks it against the
/// tree it has proven to be inside the grant. A node that is unknown to the
/// daemon is refused, not forwarded on trust.
fn enforce_scope(
    state: &Arc<DaemonState>,
    session_id: &SessionId,
    op: BridgeOp,
    context: &ReadContext,
) -> Result<(), FigError> {
    if matches!(
        op,
        BridgeOp::GetSelection | BridgeOp::GetChanges | BridgeOp::ListSessions
    ) {
        return Ok(());
    }
    // A session that does not exist is reported by the session check, which
    // knows the better error. A session that exists without a grant has no
    // scope at all, so nothing is inside it.
    let Some(session) = crate::guarded(None, || state.registry.get(session_id)) else {
        return Ok(());
    };
    // A paused or revoked grant is reported with its own code, never as a
    // scope question: the user decided, and the answer has to say so.
    if let Some(grant) = session.grant.as_ref() {
        match grant.state {
            figtether_core::registry::GrantState::Revoked => {
                return Err(FigError::new(
                    ErrorCode::Revoked,
                    "the grant was revoked; only the user can create a new one",
                ));
            }
            figtether_core::registry::GrantState::Paused => {
                return Err(FigError::new(
                    ErrorCode::Paused,
                    "the user paused the session; new reads are refused",
                ));
            }
            figtether_core::registry::GrantState::Active => {}
        }
    }
    if session.root_ids().is_empty() {
        return Err(
            FigError::new(ErrorCode::OutOfScope, "the session has no granted scope")
                .with_action("ask the user to grant a root before reading"),
        );
    }
    let mut named: Vec<&str> = Vec::new();
    if let Some(root) = context.root_id.as_deref() {
        named.push(root);
    }
    named.extend(context.node_ids.iter().map(String::as_str));

    for node_id in named {
        let inside = crate::guarded(false, || state.registry.is_in_scope(session_id, node_id));
        if !inside {
            return Err(FigError::new(
                ErrorCode::OutOfScope,
                format!("node {node_id} is outside the granted scope"),
            )
            .with_action(
                "list the children of a granted root first, or ask the user to widen the grant",
            ));
        }
    }
    Ok(())
}

/// Learn the shape of the granted tree from a plugin answer.
///
/// Every answer carries the parent relation of the nodes it contains. The
/// daemon records them, so a later request for one of those nodes can be
/// checked on the daemon side as well.
fn remember_nodes(state: &Arc<DaemonState>, session_id: &SessionId, data: &serde_json::Value) {
    let mut relations: Vec<(String, Option<String>)> = Vec::new();
    let mut collect = |nodes: &serde_json::Value| {
        if let Some(list) = nodes.as_array() {
            for node in list {
                let Some(id) = node.get("id").and_then(serde_json::Value::as_str) else {
                    continue;
                };
                let parent = node
                    .get("parent_id")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string);
                relations.push((id.to_string(), parent));
            }
        }
    };
    if let Some(nodes) = data.get("nodes") {
        collect(nodes);
    }
    if let Some(node) = data.get("node") {
        collect(&serde_json::Value::Array(vec![node.clone()]));
    }
    if relations.is_empty() {
        return;
    }
    crate::guarded((), || {
        let _ = state.registry.record_nodes(session_id, &relations);
    });
}

/// Parse the parameters of a client request.
fn parse_params<T: serde::de::DeserializeOwned>(params: &serde_json::Value) -> Result<T, FigError> {
    serde_json::from_value(params.clone()).map_err(|err| {
        FigError::new(
            ErrorCode::InvalidLimit,
            format!("the parameters are invalid: {err}"),
        )
    })
}

/// Whether a depth is inside the accepted range.
fn validate_depth(limits: &ResourceLimits, depth: Option<u32>) -> Result<(), FigError> {
    match depth {
        Some(depth) if !limits.depth_is_valid(depth) => Err(FigError::new(
            ErrorCode::InvalidLimit,
            format!(
                "the depth {depth} exceeds the limit of {}",
                limits.max_depth
            ),
        )),
        _ => Ok(()),
    }
}

/// Whether a batch of node ids is inside the accepted size.
fn validate_batch(limits: &ResourceLimits, count: usize) -> Result<(), FigError> {
    if limits.batch_is_valid(count) {
        return Ok(());
    }
    Err(FigError::new(
        ErrorCode::InvalidLimit,
        format!(
            "a batch of {count} node ids exceeds the limit of {}",
            limits.max_batch_node_ids
        ),
    ))
}

/// Whether an export scale is inside the accepted range.
fn validate_scale(limits: &ResourceLimits, scale: Option<f64>) -> Result<(), FigError> {
    match scale {
        Some(scale) if !limits.scale_is_valid(scale) => Err(FigError::new(
            ErrorCode::InvalidLimit,
            format!(
                "the scale {scale} is outside the accepted range {} to {}",
                limits.min_export_scale, limits.max_export_scale
            ),
        )),
        _ => Ok(()),
    }
}

/// The number of nodes one page of a traversal may carry.
fn page_limit(requested: Option<u32>) -> u32 {
    requested.unwrap_or(PAGE_LIMIT).clamp(1, PAGE_LIMIT)
}

/// Set the page size of the parameters the plugin receives.
fn set_limit(params: &mut serde_json::Value, limit: u32) {
    if let Some(object) = params.as_object_mut() {
        object.insert("limit".to_string(), serde_json::json!(limit));
    }
}

/// The fingerprint of the field projection of a request.
fn projection(fields: &[String]) -> Vec<String> {
    let mut fingerprint: Vec<String> = fields
        .iter()
        .filter(|field| !field.trim().is_empty())
        .cloned()
        .collect();
    fingerprint.sort();
    fingerprint.dedup();
    fingerprint
}

/// The fingerprint of the export settings of a request.
fn export_settings(params: &ExportAssetsParams) -> String {
    let format = match params.format {
        AssetFormat::Png => "png",
        AssetFormat::Svg => "svg",
    };
    format!("{}@{}", format, params.scale.unwrap_or(1.0))
}

/// Read the session id of a request.
fn session_of(params: &serde_json::Value) -> Result<SessionId, FigError> {
    let raw = params
        .get("session_id")
        .and_then(|value| value.as_str())
        .ok_or_else(|| {
            FigError::new(
                ErrorCode::SessionNotFound,
                "the request does not name a session",
            )
        })?;
    SessionId::from_str(raw)
        .map_err(|_| FigError::new(ErrorCode::SessionNotFound, "the session id is not valid"))
}

/// Build the parameters the plugin receives.
///
/// The daemon owns the fields of the local protocol: the session id, the
/// snapshot id, the traversal cursor, the consistency lane, and the cache
/// bound. The plugin never sees them.
fn plugin_params(plan: &Plan, position: Option<&str>) -> serde_json::Value {
    let mut params = plan.params.clone();
    if let Some(object) = params.as_object_mut() {
        for key in ["session_id", "snapshot_id", "consistency", "max_age_ms"] {
            object.remove(key);
        }
        if plan.op == BridgeOp::ListNodes {
            object.remove("cursor");
            if let Some(position) = position {
                if let Some(cursor) = position.strip_prefix("plugin:") {
                    object.insert(
                        "cursor".into(),
                        serde_json::Value::String(cursor.to_owned()),
                    );
                } else {
                    object.insert(
                        "after".into(),
                        serde_json::Value::String(position.to_owned()),
                    );
                }
            }
        }
    }
    params
}

/// Build the cache key of a read.
///
/// The key binds the source, the grant epoch, the node ids, the field
/// projection, the export settings, and the schema version, so a revoked
/// grant can never be served from a stale entry (`ADR-007`).
fn cache_key_of(
    session_id: &SessionId,
    grant_epoch: u64,
    context: &ReadContext,
    position: Option<&str>,
) -> CacheKey {
    let mut node_ids = context.node_ids.clone();
    if let Some(position) = position {
        node_ids.push(position.to_string());
    }
    CacheKey {
        source: SourceKey::Session(*session_id),
        grant_epoch,
        node_ids,
        fields: context.fields.clone(),
        export_settings: context.export_settings.clone(),
        schema_version: figtether_model::SCHEMA_VERSION.to_string(),
    }
}

/// The completion of one job.
///
/// The scheduler holds the lane of a session from the moment a job starts
/// until the moment it is completed. The guard completes the job on every
/// path out of the daemon, so a job that is never completed can never hold a
/// lane forever.
struct JobGuard {
    /// The state of the daemon.
    state: Arc<DaemonState>,
    /// Identity of the guarded job.
    job_id: JobId,
    /// Whether the job already reached a terminal state.
    finished: bool,
}

impl JobGuard {
    /// Guard the job of a request.
    fn new(state: &Arc<DaemonState>, job_id: JobId) -> Self {
        Self {
            state: state.clone(),
            job_id,
            finished: false,
        }
    }

    /// Complete the job with a terminal status.
    fn finish(&mut self, status: JobStatus) {
        if self.finished {
            return;
        }
        self.finished = true;
        complete_job(&self.state, &self.job_id, status);
        self.state.jobs.set_status(&self.job_id, status);
    }
}

impl Drop for JobGuard {
    fn drop(&mut self) {
        if self.finished {
            return;
        }
        // The daemon leaves the job behind, for example because the client
        // disconnected. The job still leaves the scheduler, so the lane of
        // the session is free again.
        complete_job(&self.state, &self.job_id, JobStatus::Failed);
        self.state.jobs.set_status(&self.job_id, JobStatus::Failed);
    }
}

/// Complete the queued jobs of a session that a new grant replaced.
///
/// The scope of the session moved, so a queued read of the old scope never
/// runs. The job leaves the scheduler, so the lane of the session stays
/// free, and the waiting client learns about the cancellation at once.
pub fn supersede_queued_jobs(state: &Arc<DaemonState>, session_id: &SessionId) -> usize {
    let queued = state.jobs.queued_of(session_id);
    let mut superseded = 0;
    for record in queued {
        complete_job(state, &record.job_id, JobStatus::Superseded);
        state.jobs.set_status(&record.job_id, JobStatus::Superseded);
        state.hub.complete(
            &record.job_id.to_string(),
            BridgeOutcome::Error(FigError::new(
                ErrorCode::Cancelled,
                "the grant changed before the operation started",
            )),
        );
        superseded += 1;
    }
    superseded
}

/// Run one job and build the response envelope.
async fn run_job(
    state: &Arc<DaemonState>,
    client_label: &str,
    request_id: &str,
    plan: Plan,
) -> Result<Envelope, FigError> {
    let session_id = plan.session_id.ok_or_else(|| {
        FigError::new(
            ErrorCode::SessionNotFound,
            "the request does not name a session",
        )
    })?;

    let session = match crate::guarded(None::<figtether_core::Session>, || {
        state.registry.get(&session_id)
    }) {
        Some(session) => session,
        None => {
            return Err(FigError::new(
                ErrorCode::SessionNotFound,
                "the session does not exist",
            ))
        }
    };

    if state.hub.is_revoked(&session_id) {
        return Err(FigError::new(
            ErrorCode::Revoked,
            "the grant was revoked; only the user can create a new one",
        ));
    }

    match session.state {
        figtether_core::SessionState::Ready => {}
        figtether_core::SessionState::Paused => {
            return Err(FigError::new(
                ErrorCode::Paused,
                "the user paused the session; new reads are refused",
            ))
        }
        figtether_core::SessionState::Revoked => {
            return Err(FigError::new(
                ErrorCode::Revoked,
                "the grant was revoked; only the user can create a new one",
            ))
        }
        figtether_core::SessionState::Offline => {
            return Err(FigError::new(
                ErrorCode::PluginOffline,
                "the plugin is not connected",
            ))
        }
        other => {
            return Err(FigError::new(
                ErrorCode::SessionNotFound,
                format!("the session is not ready: {}", other.as_str()),
            ))
        }
    }

    // A session without a grant reads nothing.
    let grant_epoch = session
        .grant_epoch()
        .ok_or_else(|| FigError::new(ErrorCode::SessionNotFound, "the session holds no grant"))?;
    let grant_id = session.grant.as_ref().map(|grant| grant.grant_id);
    let root_ids = session.root_ids();
    let connection_epoch = session.connection_epoch;

    // The cursor of a continuation request. A cursor that no longer matches
    // the observation, the scope, or the grant epoch is refused.
    let position = match &plan.context.cursor {
        Some(token) => {
            let record = cursor::validate(
                &state.cursors,
                token,
                &session_id,
                grant_epoch,
                plan.context.root_id.as_deref().unwrap_or_default(),
                &plan.context.fields,
            )?;
            Some(record.position)
        }
        None => None,
    };

    // The cached lane. A fresh request never consults the cache.
    let cache_key = cache_key_of(&session_id, grant_epoch, &plan.context, position.as_deref());
    if plan.context.consistency == ConsistencyRequest::Cached {
        let entry = crate::guarded(None, || state.cache.get(&cache_key, Instant::now()));
        if let Some(entry) = entry {
            let age = entry.age(Instant::now()).as_millis() as u64;
            let fresh_enough = plan.context.max_age_ms.is_none_or(|max_age| age <= max_age);
            if fresh_enough {
                state
                    .metrics
                    .record_request(figtether_core::metrics::Lane::CacheHit);
                return Ok(cached_envelope(
                    request_id,
                    &session_id,
                    grant_epoch,
                    connection_epoch,
                    &root_ids,
                    &entry.value,
                    age,
                ));
            }
        }
    }

    let timeout = match (plan.write, plan.kind) {
        // A write may take longer than a read: the plugin applies the whole
        // plan and commits it as one undo step.
        (true, _) => state.limits.export_timeout,
        (false, JobKind::Export) => state.limits.export_timeout,
        (false, _) => state.limits.read_timeout,
    };
    let deadline = Instant::now() + timeout;

    let job = Job {
        job_id: JobId::new(),
        kind: plan.kind,
        session_id: Some(session_id),
        client_id: client_label.to_string(),
        params: plugin_params(&plan, position.as_deref()),
        deadline,
        enqueued_at: Instant::now(),
        priority: plan.priority,
        attempt: 0,
    };
    let job_id = job.job_id;

    state.jobs.insert(JobRecord {
        job_id,
        kind: job.kind,
        op: plan.op,
        session_id: job.session_id,
        status: JobStatus::Queued,
        created_at: Utc::now(),
    });

    let request_key = job_id.to_string();
    let answer = state.hub.expect(&request_key, Some(session_id));
    let mut guard = JobGuard::new(state, job_id);

    if let Err(err) = crate::domain_call(ErrorCode::InternalError, || state.scheduler.submit(job)) {
        state.hub.cancel_expect(&request_key);
        guard.finish(JobStatus::Failed);
        if err.code == ErrorCode::Busy {
            Metrics::incr(&state.metrics.busy_rejections);
        }
        return Err(err.with_request_id(request_id.to_string()));
    }
    state
        .metrics
        .set_queue_depth(scheduler_stats(state).queued as u64);

    let outcome = match tokio::time::timeout_at(deadline.into(), answer).await {
        Err(_elapsed) => {
            state.hub.cancel_expect(&request_key);
            guard.finish(JobStatus::TimedOut);
            return Err(
                FigError::new(ErrorCode::Timeout, "the operation exceeded its deadline")
                    .with_request_id(request_id.to_string()),
            );
        }
        Ok(Err(_closed)) => {
            guard.finish(JobStatus::Cancelled);
            return Err(
                FigError::new(ErrorCode::Cancelled, "the operation was cancelled")
                    .with_request_id(request_id.to_string()),
            );
        }
        Ok(Ok(outcome)) => outcome,
    };

    let data = match outcome {
        BridgeOutcome::Error(err) => {
            guard.finish(JobStatus::Failed);
            return Err(err.with_request_id(request_id.to_string()));
        }
        BridgeOutcome::Result(data) => data,
    };

    let observation = observation_of(Utc::now());
    let normalize_context = NormalizeContext {
        grant_id,
        root_ids: root_ids.clone(),
        limits: &state.limits,
        assets: &state.assets,
    };
    let normalized = match normalize_answer(&plan, &data, &normalize_context) {
        Ok(normalized) => normalized,
        Err(err) => {
            guard.finish(JobStatus::Failed);
            return Err(err.with_request_id(request_id.to_string()));
        }
    };
    // The answer teaches the daemon the shape of the granted tree, which is
    // what lets the daemon check the scope of a later request on its own.
    remember_nodes(state, &session_id, &data);
    guard.finish(JobStatus::Completed);

    if plan.kind == JobKind::Export {
        state
            .metrics
            .record_request(figtether_core::metrics::Lane::Export);
    } else {
        state
            .metrics
            .record_request(figtether_core::metrics::Lane::FreshRead);
    }

    // The cursor of a truncated traversal. It is bound to the observation,
    // the scope, and the grant epoch, so it can be validated when the client
    // continues the traversal.
    let mut coverage = normalized.coverage;
    if coverage.truncated {
        if let Some(position) = normalized.position {
            let token = state.cursors.mint(CursorRecord {
                session_id,
                grant_epoch,
                root_id: plan.context.root_id.clone().unwrap_or_default(),
                fields: plan.context.fields.clone(),
                observation_id: observation.observation_id,
                position,
            });
            coverage.next_cursor = Some(token);
        }
    }

    let envelope = live_envelope(
        request_id,
        session_id,
        grant_epoch,
        connection_epoch,
        observation,
        coverage.clone(),
        normalized.payload.clone(),
    );

    // The read fills the cache under a key that binds the authorization and
    // the shape of the request. A write never fills the cache: its answer
    // describes a change, not a state of the document.
    if plan.cacheable {
        let cached = serde_json::json!({
            "data": envelope.data,
            "coverage": coverage,
        });
        let ttl = state.limits.live_cache_ttl;
        let stored = crate::guarded(
            Err(FigError::new(
                ErrorCode::InternalError,
                "the cache refused the entry",
            )),
            || state.cache.put(cache_key, cached, ttl, Instant::now()),
        );
        if let Err(err) = stored {
            tracing::warn!(code = err.code.as_str(), "the read was not cached");
        }
    }

    Ok(envelope)
}

/// Tell the scheduler that a job ended.
fn complete_job(state: &Arc<DaemonState>, job_id: &JobId, status: JobStatus) {
    crate::guarded((), || state.scheduler.complete(job_id, status));
}

/// What the normalizer needs to know about one read.
struct NormalizeContext<'a> {
    /// Grant the read belongs to.
    grant_id: Option<GrantId>,
    /// Roots the read is scoped to.
    root_ids: Vec<String>,
    /// Budgets of the daemon.
    limits: &'a ResourceLimits,
    /// The store of the exported assets.
    assets: &'a figtether_core::InMemoryAssetStore,
}

/// The normalized answer of the plugin.
struct Normalized {
    /// The payload the client receives.
    payload: serde_json::Value,
    /// The coverage of the read.
    coverage: Coverage,
    /// The last node of the page, when the read is a traversal.
    position: Option<String>,
}

/// Normalize the payload the plugin returned.
fn normalize_answer(
    plan: &Plan,
    data: &serde_json::Value,
    context: &NormalizeContext<'_>,
) -> Result<Normalized, FigError> {
    match plan.op {
        BridgeOp::ListNodes | BridgeOp::ReadNodes => {
            let document: PluginDocument = serde_json::from_value(data.clone()).map_err(|err| {
                FigError::new(
                    ErrorCode::ProtocolMismatch,
                    format!("the plugin answer is malformed: {err}"),
                )
            })?;
            let (nodes, unavailable) = normalize_document(&document);
            // The plugin reports that the traversal is not finished.
            let truncated = data
                .get("truncated")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
                || data
                    .get("remaining")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
            let (nodes, trimmed) = fit_to_budget(nodes, context.limits.max_json_response_bytes);
            // Preserve the plugin's opaque traversal stack. A node id is not its cursor.
            let native_cursor = data.get("next_cursor").and_then(serde_json::Value::as_str);
            if trimmed && data.get("next_cursor").is_some() {
                return Err(FigError::new(ErrorCode::PayloadTooLarge,"the requested page exceeds the byte budget; retry with a smaller limit or fewer fields"));
            }
            let position = native_cursor
                .map(|c| format!("plugin:{c}"))
                .or_else(|| nodes.last().map(|node| node.summary.id.clone()));
            let payload = serde_json::json!({
                "nodes": nodes.iter().map(flatten_node).collect::<Vec<_>>(),
            });
            let coverage = Coverage {
                root_ids: context.root_ids.clone(),
                returned_nodes: nodes.len() as u32,
                truncated: truncated || trimmed,
                next_cursor: None,
                unavailable_fields: unavailable,
            };
            Ok(Normalized {
                payload,
                coverage,
                position,
            })
        }
        BridgeOp::InspectDesign => {
            let node: PluginNode =
                serde_json::from_value(data.get("node").cloned().ok_or_else(|| {
                    FigError::new(
                        ErrorCode::ProtocolMismatch,
                        "the plugin answered without a node",
                    )
                })?)
                .map_err(|err| {
                    FigError::new(
                        ErrorCode::ProtocolMismatch,
                        format!("the plugin answer is malformed: {err}"),
                    )
                })?;
            let document = PluginDocument {
                document_label: String::new(),
                page_name: String::new(),
                nodes: vec![node],
                capabilities: Default::default(),
            };
            let (nodes, unavailable) = normalize_document(&document);
            let payload = serde_json::json!({
                "node": nodes.first().map(flatten_node).unwrap_or(serde_json::Value::Null),
            });
            let coverage = Coverage {
                root_ids: context.root_ids.clone(),
                returned_nodes: nodes.len() as u32,
                truncated: false,
                next_cursor: None,
                unavailable_fields: unavailable,
            };
            Ok(Normalized {
                payload,
                coverage,
                position: None,
            })
        }
        BridgeOp::ExportAssets => {
            let exports = export_payloads(data)?;
            let mut assets = Vec::with_capacity(exports.len());
            for value in exports {
                let export: PluginExport = serde_json::from_value(value).map_err(|err| {
                    FigError::new(
                        ErrorCode::ProtocolMismatch,
                        format!("the plugin answer is malformed: {err}"),
                    )
                })?;
                if !context.limits.pixels_are_valid(export.width, export.height) {
                    return Err(FigError::new(
                        ErrorCode::InvalidLimit,
                        "the export exceeds the pixel budget",
                    ));
                }
                let grant_id = context.grant_id.ok_or_else(|| {
                    FigError::new(
                        ErrorCode::Revoked,
                        "the session holds no grant for the export",
                    )
                })?;
                let asset_id = context.assets.put(
                    grant_id,
                    &export.mime,
                    export.width,
                    export.height,
                    export.bytes.clone(),
                )?;
                assets.push(serde_json::json!({
                    "node_id": export.node_id,
                    "asset_id": asset_id,
                    "mime": export.mime,
                    "width": export.width,
                    "height": export.height,
                    "hash": export.hash,
                    "size": export.bytes.len(),
                }));
            }
            let returned_nodes = assets.len() as u32;
            let payload = if assets.len() == 1 {
                assets.remove(0)
            } else {
                serde_json::json!({ "assets": assets })
            };
            let coverage = Coverage {
                root_ids: context.root_ids.clone(),
                returned_nodes,
                truncated: false,
                next_cursor: None,
                unavailable_fields: Vec::new(),
            };
            Ok(Normalized {
                payload,
                coverage,
                position: None,
            })
        }
        _ => {
            // The remaining operations answer with a payload the daemon does
            // not shape: the tokens, the selection, and the watch events.
            let coverage = Coverage {
                root_ids: context.root_ids.clone(),
                returned_nodes: 0,
                truncated: false,
                next_cursor: None,
                unavailable_fields: Vec::new(),
            };
            Ok(Normalized {
                payload: data.clone(),
                coverage,
                position: None,
            })
        }
    }
}

/// Accept the plugin's batch envelope and the legacy single-export shape.
/// A failed batch stays a failure instead of becoming a successful empty asset.
fn export_payloads(data: &serde_json::Value) -> Result<Vec<serde_json::Value>, FigError> {
    let Some(items) = data.get("items").and_then(serde_json::Value::as_array) else {
        return Ok(vec![data.clone()]);
    };
    if items.is_empty() {
        return Err(FigError::new(
            ErrorCode::ExportFailed,
            "the plugin returned no exports",
        ));
    }
    items
        .iter()
        .map(|item| {
            if item.get("status").and_then(serde_json::Value::as_str) != Some("ok") {
                return Err(FigError::new(
                    ErrorCode::ExportFailed,
                    "a requested asset could not be exported",
                ));
            }
            item.get("export").cloned().ok_or_else(|| {
                FigError::new(
                    ErrorCode::ProtocolMismatch,
                    "the plugin batch item has no export",
                )
            })
        })
        .collect()
}

/// Flatten the identity of a node into the top level of the payload.
///
/// The normalized model keeps the identity of a node in its summary. The
/// payload of a read carries the identity at the top level, so a client can
/// address a node without knowing the shape of the model.
fn flatten_node(node: &NormalizedNode) -> serde_json::Value {
    let mut value = serde_json::to_value(node).unwrap_or(serde_json::Value::Null);
    if let Some(object) = value.as_object_mut() {
        if let Some(summary) = object.remove("summary") {
            if let Some(summary) = summary.as_object() {
                for (key, item) in summary {
                    object.insert(key.clone(), item.clone());
                }
            }
        }
    }
    value
}

/// Trim a page so the encoded envelope stays inside the response budget.
///
/// The budget covers the whole envelope, so the node list gets the remaining
/// space after a fixed reserve for the envelope itself. A trimmed page is a
/// partial page, so it carries a cursor.
fn fit_to_budget(nodes: Vec<NormalizedNode>, budget: usize) -> (Vec<NormalizedNode>, bool) {
    let room = budget.saturating_sub(ENVELOPE_RESERVE);
    let mut sizes: Vec<usize> = nodes
        .iter()
        .map(|node| {
            serde_json::to_vec(node)
                .map(|bytes| bytes.len() + 1)
                .unwrap_or(1)
        })
        .collect();
    let mut used: usize = sizes.iter().sum();
    let mut trimmed = false;
    while used > room && sizes.len() > 1 {
        let Some(last) = sizes.pop() else {
            break;
        };
        used = used.saturating_sub(last);
        trimmed = true;
    }
    let mut kept = nodes;
    kept.truncate(sizes.len());
    (kept, trimmed)
}

/// Build the observation of one read.
fn observation_of(now: DateTime<Utc>) -> Observation {
    Observation {
        observation_id: ObservationId::new(),
        started_at: now,
        observed_at: now,
        consistency: Consistency::Fresh,
        freshness: Freshness::FreshRead,
        cache_age_ms: None,
        content_hash: None,
    }
}

/// Build the envelope of a live read.
fn live_envelope(
    request_id: &str,
    session_id: SessionId,
    grant_epoch: u64,
    connection_epoch: u64,
    observation: Observation,
    coverage: Coverage,
    data: serde_json::Value,
) -> Envelope {
    let source = SourceRef {
        kind: SourceKind::PluginLive,
        session_id: Some(session_id),
        snapshot_id: None,
        grant_epoch: Some(grant_epoch),
        connection_epoch: Some(connection_epoch),
    };
    Envelope::new(request_id, source, observation, coverage, data).with_content_hash()
}

/// Build the envelope of a cached answer.
fn cached_envelope(
    request_id: &str,
    session_id: &SessionId,
    grant_epoch: u64,
    connection_epoch: u64,
    root_ids: &[String],
    cached: &serde_json::Value,
    age_ms: u64,
) -> Envelope {
    let data = cached.get("data").cloned().unwrap_or(serde_json::json!({}));
    let coverage: Coverage = serde_json::from_value(
        cached
            .get("coverage")
            .cloned()
            .unwrap_or(serde_json::json!({})),
    )
    .unwrap_or_default();
    let now = Utc::now();
    let observation = Observation {
        observation_id: ObservationId::new(),
        started_at: now,
        observed_at: now,
        consistency: Consistency::Cached,
        freshness: Freshness::CacheHit,
        cache_age_ms: Some(age_ms),
        content_hash: None,
    };
    let source = SourceRef {
        kind: SourceKind::PluginLive,
        session_id: Some(*session_id),
        snapshot_id: None,
        grant_epoch: Some(grant_epoch),
        connection_epoch: Some(connection_epoch),
    };
    let mut coverage = coverage;
    if coverage.root_ids.is_empty() {
        coverage.root_ids = root_ids.to_vec();
    }
    Envelope::new(request_id, source, observation, coverage, data).with_content_hash()
}

/// Build an envelope for a payload that belongs to a session.
pub fn envelope_from_value(
    request_id: &str,
    session_id: Option<SessionId>,
    data: serde_json::Value,
    coverage: Coverage,
) -> Envelope {
    let source = match session_id {
        Some(session_id) => SourceRef {
            kind: SourceKind::PluginLive,
            session_id: Some(session_id),
            snapshot_id: None,
            grant_epoch: None,
            connection_epoch: None,
        },
        None => SourceRef {
            kind: SourceKind::OfflineBundle,
            session_id: None,
            snapshot_id: None,
            grant_epoch: None,
            connection_epoch: None,
        },
    };
    let now = Utc::now();
    let observation = observation_of(now);
    Envelope::new(request_id, source, observation, coverage, data).with_content_hash()
}

/// Build the envelope of a status answer.
pub fn status_envelope(request_id: &str, status: control::DaemonStatus) -> Envelope {
    let data = serde_json::to_value(&status).unwrap_or(serde_json::json!({}));
    envelope_from_value(request_id, None, data, Coverage::default())
}

/// The worker that runs the queued jobs.
pub async fn run_worker(state: Arc<DaemonState>) {
    loop {
        if state.shutdown_requested() {
            break;
        }
        match crate::guarded(None, || state.scheduler.next_ready()) {
            Some(job) => {
                execute_job(&state, job).await;
            }
            None => {
                tokio::select! {
                    _ = state.work_notify.notified() => {}
                    _ = tokio::time::sleep(Duration::from_millis(50)) => {}
                }
            }
        }
        state
            .metrics
            .set_queue_depth(scheduler_stats(&state).queued as u64);
    }
}

/// Send one job to the plugin.
async fn execute_job(state: &Arc<DaemonState>, job: Job) {
    let _ = crate::guarded(false, || state.scheduler.mark_running(&job.job_id));
    // The job runs now, so the ledger knows that it is not queued anymore.
    state.jobs.set_status(&job.job_id, JobStatus::Running);
    let request_id = job.job_id.to_string();

    let Some(session_id) = job.session_id else {
        state.hub.complete(
            &request_id,
            BridgeOutcome::Error(FigError::new(
                ErrorCode::SessionNotFound,
                "the job does not belong to a session",
            )),
        );
        return;
    };

    let Some(link) = state.hub.link(&session_id) else {
        state.hub.complete(
            &request_id,
            BridgeOutcome::Error(FigError::new(
                ErrorCode::PluginOffline,
                "the plugin is not connected",
            )),
        );
        return;
    };

    // The operation of the request travels with the job: the plugin receives
    // the operation the client asked for.
    let op = crate::guarded(None, || state.jobs.get(&job.job_id))
        .map(|record| record.op)
        .unwrap_or_else(|| job.kind.op());

    let remaining = job
        .deadline
        .saturating_duration_since(Instant::now())
        .as_millis() as u64;
    let payload = serde_json::json!({
        "op": op.as_str(),
        "params": job.params,
    });
    let mut frame = BridgeFrame::new(MessageType::Request, request_id.clone(), payload)
        .with_session(session_id)
        .with_connection_epoch(link.epoch)
        .with_deadline_ms(remaining);
    if let Some(grant_epoch) = crate::guarded(None, || {
        state
            .registry
            .get(&session_id)
            .and_then(|session| session.grant_epoch())
    }) {
        frame = frame.with_grant_epoch(grant_epoch);
    }

    if state.hub.send(&session_id, frame).await.is_err() {
        state.hub.complete(
            &request_id,
            BridgeOutcome::Error(FigError::new(
                ErrorCode::PluginOffline,
                "the plugin is not connected",
            )),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_export_batches_reach_the_asset_normalizer() {
        let export = serde_json::json!({ "node_id": "1:2", "mime": "image/png", "width": 1,
            "height": 1, "bytes": "AA==", "hash": "sha256:test" });
        let batch = serde_json::json!({ "batch_status": "complete", "items": [
            { "node_id": "1:2", "status": "ok", "export": export.clone() }
        ] });
        assert_eq!(
            export_payloads(&batch).expect("batch parses"),
            vec![export.clone()]
        );
        assert_eq!(
            export_payloads(&export).expect("legacy parses"),
            vec![export]
        );
    }

    #[test]
    fn failed_editor_export_batches_do_not_report_success() {
        let batch = serde_json::json!({ "items": [{ "node_id": "1:2", "status": "error" }] });
        assert_eq!(
            export_payloads(&batch).expect_err("failure retained").code,
            ErrorCode::ExportFailed
        );
        assert!(export_payloads(&serde_json::json!({ "items": [] })).is_err());
    }

    fn state() -> Arc<DaemonState> {
        Arc::new(DaemonState::new(
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            38947,
            figtether_core::ResourceLimits::default(),
            crate::secrets::Secret::generate(),
        ))
    }

    #[test]
    fn request_ids_are_read_from_every_variant() {
        let request = ClientRequest::ListNodes {
            request_id: "req_1".to_string(),
            params: serde_json::json!({}),
        };
        assert_eq!(request_id_of(&request), "req_1");
    }

    #[test]
    fn a_plan_needs_a_session() {
        let request = ClientRequest::GetNodes {
            request_id: "req_2".to_string(),
            params: serde_json::json!({ "node_ids": ["1:2"] }),
        };
        let err = plan_of(&state(), &request).expect_err("refused");
        assert_eq!(err.code, ErrorCode::SessionNotFound);
    }

    #[test]
    fn a_plan_reads_the_session_of_the_request() {
        let session = SessionId::new();
        let request = ClientRequest::GetNodes {
            request_id: "req_3".to_string(),
            params: serde_json::json!({
                "session_id": session.to_string(),
                "node_ids": ["1:2"],
            }),
        };
        let plan = plan_of(&state(), &request).expect("planned");
        assert_eq!(plan.session_id, Some(session));
        assert_eq!(plan.kind, JobKind::Read);
        assert_eq!(plan.op, BridgeOp::ReadNodes);
    }

    #[test]
    fn every_operation_reaches_the_plugin_as_its_own_operation() {
        let session = SessionId::new();
        let session_id = session.to_string();
        let cases = [
            (
                ClientRequest::ListNodes {
                    request_id: "req_a".to_string(),
                    params: serde_json::json!({ "session_id": session_id, "root_id": "1:2" }),
                },
                BridgeOp::ListNodes,
            ),
            (
                ClientRequest::GetNodes {
                    request_id: "req_b".to_string(),
                    params: serde_json::json!({ "session_id": session_id, "node_ids": ["1:2"] }),
                },
                BridgeOp::ReadNodes,
            ),
            (
                ClientRequest::InspectDesign {
                    request_id: "req_c".to_string(),
                    params: serde_json::json!({ "session_id": session_id, "root_id": "1:2" }),
                },
                BridgeOp::InspectDesign,
            ),
            (
                ClientRequest::GetDesignTokens {
                    request_id: "req_d".to_string(),
                    params: serde_json::json!({ "session_id": session_id, "root_id": "1:2" }),
                },
                BridgeOp::GetDesignTokens,
            ),
            (
                ClientRequest::ExportAssets {
                    request_id: "req_e".to_string(),
                    params: serde_json::json!({ "session_id": session_id, "node_ids": ["1:2"], "format": "png" }),
                },
                BridgeOp::ExportAssets,
            ),
            (
                ClientRequest::GetSelection {
                    request_id: "req_f".to_string(),
                    params: serde_json::json!({ "session_id": session_id }),
                },
                BridgeOp::GetSelection,
            ),
            (
                ClientRequest::WatchScope {
                    request_id: "req_g".to_string(),
                    params: serde_json::json!({ "session_id": session_id, "root_id": "1:2" }),
                },
                BridgeOp::WatchScope,
            ),
            (
                ClientRequest::GetChanges {
                    request_id: "req_h".to_string(),
                    params: serde_json::json!({ "session_id": session_id, "watch_id": "w1" }),
                },
                BridgeOp::GetChanges,
            ),
        ];
        for (request, expected) in cases {
            let plan = plan_of(&state(), &request).expect("planned");
            assert_eq!(plan.op, expected, "{}", request_id_of(&request));
        }
    }

    #[test]
    fn exports_use_the_export_priority() {
        let session = SessionId::new();
        let request = ClientRequest::ExportAssets {
            request_id: "req_4".to_string(),
            params: serde_json::json!({
                "session_id": session.to_string(),
                "node_ids": ["1:2"],
                "format": "png",
            }),
        };
        let plan = plan_of(&state(), &request).expect("planned");
        assert_eq!(plan.kind, JobKind::Export);
        assert_eq!(plan.priority, Priority::Export);
        assert_eq!(plan.op, BridgeOp::ExportAssets);
        assert_eq!(
            plan.context.export_settings.as_deref(),
            Some("png@1"),
            "the export settings are part of the cache key"
        );
    }

    #[test]
    fn a_depth_beyond_the_limit_is_refused() {
        let session = SessionId::new();
        let request = ClientRequest::ListNodes {
            request_id: "req_5".to_string(),
            params: serde_json::json!({
                "session_id": session.to_string(),
                "root_id": "1:2",
                "depth": 33,
            }),
        };
        let err = plan_of(&state(), &request).expect_err("refused");
        assert_eq!(err.code, ErrorCode::InvalidLimit);
    }

    #[test]
    fn a_batch_beyond_the_limit_is_refused() {
        let session = SessionId::new();
        let batch: Vec<String> = (0..51).map(|index| format!("1:{index}")).collect();
        let request = ClientRequest::GetNodes {
            request_id: "req_6".to_string(),
            params: serde_json::json!({
                "session_id": session.to_string(),
                "node_ids": batch,
            }),
        };
        let err = plan_of(&state(), &request).expect_err("refused");
        assert_eq!(err.code, ErrorCode::InvalidLimit);
    }

    #[test]
    fn the_plugin_parameters_carry_the_operation_position() {
        let session = SessionId::new();
        let request = ClientRequest::ListNodes {
            request_id: "req_7".to_string(),
            params: serde_json::json!({
                "session_id": session.to_string(),
                "root_id": "1:2",
                "cursor": "cursor-1",
            }),
        };
        let plan = plan_of(&state(), &request).expect("planned");
        let params = plugin_params(&plan, Some("1:9"));
        assert_eq!(params["after"], "1:9");
        assert!(params.get("cursor").is_none());
        assert!(params.get("session_id").is_none());
        assert_eq!(params["root_id"], "1:2");
        let native = plugin_params(&plan, Some("plugin:opaque-plugin-stack"));
        assert_eq!(native["cursor"], "opaque-plugin-stack");
        assert!(native.get("after").is_none());
    }

    #[test]
    fn the_cache_key_binds_the_grant_and_the_scope() {
        let session = SessionId::new();
        let context = ReadContext {
            root_id: Some("1:2".to_string()),
            node_ids: vec!["1:2".to_string()],
            fields: vec!["name".to_string()],
            ..ReadContext::default()
        };
        let first = cache_key_of(&session, 1, &context, Some("1:9"));
        let second = cache_key_of(&session, 2, &context, Some("1:9"));
        assert_ne!(first, second, "the grant epoch is part of the key");
        assert_eq!(first.node_ids, vec!["1:2".to_string(), "1:9".to_string()]);
        assert_eq!(first.fields, vec!["name".to_string()]);
        assert_eq!(first.schema_version, figtether_model::SCHEMA_VERSION);
    }

    #[test]
    fn a_status_envelope_carries_no_session() {
        let status = control::DaemonStatus::empty(38947);
        let envelope = status_envelope("req_8", status);
        assert!(envelope.source.session_id.is_none());
        assert_eq!(envelope.request_id, "req_8");
        assert!(envelope.observation.content_hash.is_some());
    }

    #[test]
    fn only_the_stored_client_credential_is_accepted() {
        let credential = crate::secrets::Secret::generate();
        let state = DaemonState::new(
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            38947,
            figtether_core::ResourceLimits::default(),
            credential.clone(),
        );
        assert!(client_credential_matches(&state, credential.as_str()));
        assert!(!client_credential_matches(&state, "wrong-credential"));
        assert!(!client_credential_matches(&state, ""));
    }

    #[test]
    fn the_ledger_tracks_jobs() {
        let ledger = JobLedger::new();
        let job_id = JobId::new();
        ledger.insert(JobRecord {
            job_id,
            kind: JobKind::Read,
            op: BridgeOp::ReadNodes,
            session_id: None,
            status: JobStatus::Queued,
            created_at: Utc::now(),
        });
        assert_eq!(
            ledger.get(&job_id).expect("known").status,
            JobStatus::Queued
        );
        ledger.set_status(&job_id, JobStatus::Completed);
        assert_eq!(
            ledger.get(&job_id).expect("known").status,
            JobStatus::Completed
        );
    }

    #[test]
    fn the_first_terminal_state_of_a_job_wins() {
        let ledger = JobLedger::new();
        let job_id = JobId::new();
        ledger.insert(JobRecord {
            job_id,
            kind: JobKind::Read,
            op: BridgeOp::ReadNodes,
            session_id: None,
            status: JobStatus::Queued,
            created_at: Utc::now(),
        });
        ledger.set_status(&job_id, JobStatus::Cancelled);
        ledger.set_status(&job_id, JobStatus::Completed);
        assert_eq!(
            ledger.get(&job_id).expect("known").status,
            JobStatus::Cancelled
        );
    }

    #[test]
    fn a_normalized_node_carries_its_identity_at_the_top_level() {
        let node: PluginNode = serde_json::from_value(serde_json::json!({
            "id": "1:2",
            "node_type": "FRAME",
            "name": "cart",
        }))
        .expect("parses");
        let document = PluginDocument {
            document_label: String::new(),
            page_name: String::new(),
            nodes: vec![node],
            capabilities: Default::default(),
        };
        let (nodes, _unavailable) = normalize_document(&document);
        let flat = flatten_node(&nodes[0]);
        assert_eq!(flat["id"], "1:2");
        assert_eq!(flat["node_type"], "FRAME");
    }

    #[test]
    fn a_page_is_trimmed_to_the_response_budget() {
        let node: PluginNode = serde_json::from_value(serde_json::json!({
            "id": "1:2",
            "node_type": "FRAME",
            "name": "cart",
        }))
        .expect("parses");
        let document = PluginDocument {
            document_label: String::new(),
            page_name: String::new(),
            nodes: vec![node; 64],
            capabilities: Default::default(),
        };
        let (nodes, _unavailable) = normalize_document(&document);
        let (kept, trimmed) = fit_to_budget(nodes, 4096);
        assert!(trimmed, "the page does not fit into the budget");
        assert!(kept.len() < 64);
    }

    /// A state with one session that serves reads.
    fn ready_state(read_timeout: Duration) -> (Arc<DaemonState>, SessionId) {
        ready_state_with(read_timeout, figtether_model::write::GrantMode::Read)
    }

    /// A state with one session that holds a grant of the given mode.
    fn ready_state_with(
        read_timeout: Duration,
        mode: figtether_model::write::GrantMode,
    ) -> (Arc<DaemonState>, SessionId) {
        let limits = figtether_core::ResourceLimits {
            read_timeout,
            ..figtether_core::ResourceLimits::default()
        };
        let state = Arc::new(DaemonState::new(
            uuid::Uuid::new_v4(),
            uuid::Uuid::new_v4(),
            38947,
            limits,
            crate::secrets::Secret::generate(),
        ));
        let session_id = SessionId::new();
        state
            .registry
            .register(
                session_id,
                "installation-1".to_string(),
                figtether_model::capabilities::Capabilities::new(),
            )
            .expect("registers");
        state
            .registry
            .set_state(&session_id, figtether_core::SessionState::Paired)
            .expect("pairs");
        state
            .registry
            .grant(&session_id, vec!["1:2".to_string()], mode)
            .expect("grants");
        state
            .registry
            .set_state(&session_id, figtether_core::SessionState::Ready)
            .expect("serves reads");
        (state, session_id)
    }

    /// Attach a plugin that never answers.
    ///
    /// The receiver of the queue is returned, so the connection of the plugin
    /// stays open while the test runs.
    fn attach_silent_plugin(
        state: &Arc<DaemonState>,
        session_id: SessionId,
    ) -> tokio::sync::mpsc::Receiver<figtether_protocol::BridgeFrame> {
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        state.hub.attach(
            session_id,
            crate::bridge::SessionLink {
                epoch: 1,
                client_id: "plugin".to_string(),
                credential: crate::secrets::Secret::generate(),
                tx,
                capabilities: Default::default(),
            },
        );
        rx
    }

    #[tokio::test]
    async fn a_job_that_never_ends_leaves_the_lane_of_the_session_free() {
        let (state, session_id) = ready_state(Duration::from_millis(200));
        let _plugin = attach_silent_plugin(&state, session_id);
        let worker = tokio::spawn(run_worker(state.clone()));

        let request = ClientRequest::ListNodes {
            request_id: "req_timeout".to_string(),
            params: serde_json::json!({
                "session_id": session_id.to_string(),
                "root_id": "1:2",
            }),
        };
        let plan = plan_of(&state, &request).expect("planned");
        let error = run_job(&state, "client", "req_timeout", plan)
            .await
            .expect_err("the read misses its deadline");
        assert_eq!(error.code, ErrorCode::Timeout);

        let stats = state.scheduler.stats();
        assert_eq!(stats.running, 0, "the lane of the session is free");
        assert_eq!(stats.queued, 0, "the queue is empty");
        let jobs = state.jobs.of_session(&session_id);
        assert_eq!(jobs.len(), 1);
        assert_eq!(
            jobs[0].status,
            JobStatus::TimedOut,
            "the job ended in a terminal state"
        );

        worker.abort();
    }

    #[tokio::test]
    async fn a_job_of_a_revoked_session_leaves_the_lane_free() {
        let (state, session_id) = ready_state(Duration::from_millis(500));
        let _plugin = attach_silent_plugin(&state, session_id);
        let worker = tokio::spawn(run_worker(state.clone()));

        let request = ClientRequest::ListNodes {
            request_id: "req_revoke".to_string(),
            params: serde_json::json!({
                "session_id": session_id.to_string(),
                "root_id": "1:2",
            }),
        };
        let plan = plan_of(&state, &request).expect("planned");
        let job_state = state.clone();
        let job =
            tokio::spawn(async move { run_job(&job_state, "client", "req_revoke", plan).await });
        // The grant is revoked while the job runs.
        tokio::time::sleep(Duration::from_millis(50)).await;
        crate::control::revoke_session(&state, &session_id).expect("revoked");

        let error = job
            .await
            .expect("the job finishes")
            .expect_err("the read is refused");
        assert_eq!(error.code, ErrorCode::Revoked);
        assert_eq!(state.scheduler.stats().running, 0);

        worker.abort();
    }

    /// The identity of the client that asks for the writes.
    fn identity() -> ClientIdentity {
        ClientIdentity {
            owner: "installation-1".to_string(),
            label: "client-a".to_string(),
        }
    }

    /// The identity of another client of the same installation.
    fn other_identity() -> ClientIdentity {
        ClientIdentity {
            owner: "installation-1".to_string(),
            label: "client-b".to_string(),
        }
    }

    /// The parameters of a write request.
    #[test]
    fn complete_creation_receipts_are_learned_but_partial_or_mismatched_ones_are_not() {
        let (state, session_id) = ready_state_with(
            Duration::from_secs(2),
            figtether_model::write::GrantMode::Write,
        );
        let request = write_path::parse_plan(
            &serde_json::json!({
                "session_id":session_id,"plan":{"plan_id":"create","operations":[
                    {"kind":"create_node","parent_id":"1:2","node_type":"TEXT","text":"Hello"}
                ]}
            }),
            &figtether_model::write::WriteBudget::default(),
        )
        .expect("valid plan");
        let epoch = grant_epoch_of(&state, &session_id).expect("epoch");
        remember_write_creations(
            &state,
            &request,
            epoch,
            &serde_json::json!({"status":"partial","created":["1:9"]}),
        );
        assert!(!state.registry.is_in_scope(&session_id, "1:9"));
        remember_write_creations(
            &state,
            &request,
            epoch,
            &serde_json::json!({"status":"applied","created":["1:9","1:10"]}),
        );
        assert!(!state.registry.is_in_scope(&session_id, "1:9"));
        remember_write_creations(
            &state,
            &request,
            epoch,
            &serde_json::json!({"status":"applied","created":["1:9"]}),
        );
        assert!(state.registry.is_in_scope(&session_id, "1:9"));
    }

    #[tokio::test]
    async fn session_discovery_supplies_granted_ids_and_mode_without_editor_calls() {
        let (state, session_id) = ready_state_with(
            Duration::from_secs(2),
            figtether_model::write::GrantMode::Write,
        );
        let response = dispatch(
            &state,
            &identity(),
            ClientRequest::ListSessions {
                request_id: "discover".to_string(),
                cursor: None,
            },
        )
        .await;
        let data = data_of(&response);
        assert_eq!(
            data["sessions"][0]["granted_root_ids"],
            serde_json::json!(["1:2"])
        );
        assert_eq!(data["sessions"][0]["grant_mode"], "write");
        assert_eq!(data["sessions"][0]["session_id"], session_id.to_string());
    }

    fn write_params(session_id: SessionId, node_id: &str) -> serde_json::Value {
        serde_json::json!({
            "plan": {
                "plan_id": "plan_1",
                "session_id": session_id.to_string(),
                "operations": [
                    { "kind": "set_text", "node_id": node_id, "text": "hello" }
                ],
            }
        })
    }

    /// The parameters of an apply request.
    fn apply_params(session_id: SessionId, node_id: &str, token: &str) -> serde_json::Value {
        let mut params = write_params(session_id, node_id);
        params["approval_token"] = serde_json::Value::String(token.to_string());
        params
    }

    /// The payload of a successful answer.
    fn data_of(response: &ClientResponse) -> serde_json::Value {
        match response {
            ClientResponse::Ok { envelope, .. } => envelope.data.clone(),
            ClientResponse::Error { error, .. } => panic!("the request failed: {error:?}"),
            ClientResponse::Accepted { .. } => panic!("the request was accepted"),
        }
    }

    /// The error of a refused answer.
    fn error_of(response: &ClientResponse) -> figtether_model::errors::FigError {
        match response {
            ClientResponse::Error { error, .. } => error.clone(),
            other => panic!("the request was refused: {other:?}"),
        }
    }

    /// Attach a plugin that answers every request with the given payload.
    ///
    /// The frames the plugin received are collected, so a test can prove
    /// which operation reached the plugin.
    fn attach_answering_plugin(
        state: &Arc<DaemonState>,
        session_id: SessionId,
        answer: serde_json::Value,
    ) -> Arc<Mutex<Vec<figtether_protocol::BridgeFrame>>> {
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        state.hub.attach(
            session_id,
            crate::bridge::SessionLink {
                epoch: 1,
                client_id: "plugin".to_string(),
                credential: crate::secrets::Secret::generate(),
                tx,
                capabilities: Default::default(),
            },
        );
        let received: Arc<Mutex<Vec<figtether_protocol::BridgeFrame>>> =
            Arc::new(Mutex::new(Vec::new()));
        let seen = received.clone();
        // The plugin answers every request the daemon sends.
        let hub_state = state.clone();
        tokio::spawn(async move {
            while let Some(frame) = rx.recv().await {
                if frame.message_type != figtether_protocol::MessageType::Request {
                    continue;
                }
                seen.lock().expect("lock").push(frame.clone());
                hub_state
                    .hub
                    .complete(&frame.request_id, BridgeOutcome::Result(answer.clone()));
            }
        });
        received
    }

    #[tokio::test]
    async fn a_write_without_a_write_grant_is_refused() {
        let (state, session_id) = ready_state(Duration::from_secs(2));
        let response = dispatch(
            &state,
            &identity(),
            ClientRequest::PreviewWrite {
                request_id: "req_preview".to_string(),
                params: write_params(session_id, "1:2"),
            },
        )
        .await;
        let error = error_of(&response);
        assert_eq!(error.code, ErrorCode::WriteNotGranted);
        assert!(
            state.writes.audit_records().is_empty(),
            "a refused write leaves no trace"
        );
    }

    #[tokio::test]
    async fn a_write_without_an_approval_token_is_refused() {
        let (state, session_id) = ready_state_with(
            Duration::from_secs(2),
            figtether_model::write::GrantMode::Write,
        );
        let _plugin = attach_answering_plugin(
            &state,
            session_id,
            serde_json::json!({ "preview": { "plan_hash": "hash" } }),
        );
        let worker = tokio::spawn(run_worker(state.clone()));

        let response = dispatch(
            &state,
            &identity(),
            ClientRequest::ApplyWrite {
                request_id: "req_apply".to_string(),
                params: apply_params(session_id, "1:2", "not-a-token"),
            },
        )
        .await;
        let error = error_of(&response);
        assert_eq!(error.code, ErrorCode::ApprovalRequired);

        worker.abort();
    }

    #[tokio::test]
    async fn a_plan_hash_mismatch_is_refused() {
        let (state, session_id) = ready_state_with(
            Duration::from_secs(2),
            figtether_model::write::GrantMode::Write,
        );
        let _plugin = attach_answering_plugin(
            &state,
            session_id,
            serde_json::json!({ "preview": { "plan_hash": "hash" } }),
        );
        let worker = tokio::spawn(run_worker(state.clone()));

        let preview = dispatch(
            &state,
            &identity(),
            ClientRequest::PreviewWrite {
                request_id: "req_preview".to_string(),
                params: write_params(session_id, "1:2"),
            },
        )
        .await;
        let preview = data_of(&preview);
        let approval_id = preview["approval_id"]
            .as_str()
            .expect("an approval id")
            .to_string();
        let plan_hash = preview["plan_hash"]
            .as_str()
            .expect("a plan hash")
            .to_string();
        state
            .writes
            .decide(&figtether_protocol::write::ApprovalDecisionPayload {
                approval_id: approval_id.clone(),
                plan_hash: plan_hash.clone(),
                approved: true,
                reason: None,
            })
            .expect("decided");

        // The plan that is applied differs from the plan that was approved.
        let mut params = write_params(session_id, "1:2");
        params["plan"]["operations"][0]["text"] = serde_json::Value::String("other".to_string());
        let token = state
            .writes
            .status(&approval_id, "client-a", "installation-1")
            .expect("the status is known");
        let token = match token {
            write_path::ApprovalStatus::Approved { token } => token,
            other => panic!("the plan was approved: {other:?}"),
        };
        params["approval_token"] = serde_json::Value::String(token);

        let response = dispatch(
            &state,
            &identity(),
            ClientRequest::ApplyWrite {
                request_id: "req_apply".to_string(),
                params,
            },
        )
        .await;
        let error = error_of(&response);
        assert_eq!(error.code, ErrorCode::PlanHashMismatch);
        assert!(state.writes.audit_records().is_empty());

        worker.abort();
    }

    #[tokio::test]
    async fn an_expired_approval_is_refused() {
        let (state, session_id) = ready_state_with(
            Duration::from_secs(2),
            figtether_model::write::GrantMode::Write,
        );
        let _plugin = attach_answering_plugin(
            &state,
            session_id,
            serde_json::json!({ "preview": { "plan_hash": "hash" } }),
        );
        let worker = tokio::spawn(run_worker(state.clone()));

        let preview = dispatch(
            &state,
            &identity(),
            ClientRequest::PreviewWrite {
                request_id: "req_preview".to_string(),
                params: write_params(session_id, "1:2"),
            },
        )
        .await;
        let preview = data_of(&preview);
        let approval_id = preview["approval_id"]
            .as_str()
            .expect("an approval id")
            .to_string();
        let plan_hash = preview["plan_hash"]
            .as_str()
            .expect("a plan hash")
            .to_string();

        // The token was issued long ago, so the approval outlived its life.
        let token = figtether_protocol::write::approval_token(
            &approval_id,
            &plan_hash,
            &session_id,
            1,
            Utc::now() - chrono::Duration::minutes(11),
        );

        let response = dispatch(
            &state,
            &identity(),
            ClientRequest::ApplyWrite {
                request_id: "req_apply".to_string(),
                params: apply_params(session_id, "1:2", &token),
            },
        )
        .await;
        let error = error_of(&response);
        assert_eq!(error.code, ErrorCode::ApprovalExpired);

        worker.abort();
    }

    #[tokio::test]
    async fn the_budget_refuses_an_oversized_plan() {
        let (state, session_id) = ready_state_with(
            Duration::from_secs(2),
            figtether_model::write::GrantMode::Write,
        );
        let operations: Vec<serde_json::Value> = (0..60)
            .map(|index| {
                serde_json::json!({
                    "kind": "set_text",
                    "node_id": format!("1:{index}"),
                    "text": "hi",
                })
            })
            .collect();
        let response = dispatch(
            &state,
            &identity(),
            ClientRequest::PreviewWrite {
                request_id: "req_preview".to_string(),
                params: serde_json::json!({
                    "plan": {
                        "plan_id": "plan_1",
                        "session_id": session_id.to_string(),
                        "operations": operations,
                    }
                }),
            },
        )
        .await;
        let error = error_of(&response);
        assert_eq!(error.code, ErrorCode::BudgetExceeded);
        assert!(
            error.message.contains('5'),
            "the limit is in the message: {}",
            error.message
        );
    }

    #[tokio::test]
    async fn an_out_of_scope_node_is_refused() {
        let (state, session_id) = ready_state_with(
            Duration::from_secs(2),
            figtether_model::write::GrantMode::Write,
        );
        let response = dispatch(
            &state,
            &identity(),
            ClientRequest::PreviewWrite {
                request_id: "req_preview".to_string(),
                params: write_params(session_id, "9:9"),
            },
        )
        .await;
        let error = error_of(&response);
        assert_eq!(error.code, ErrorCode::OutOfScope);
    }

    #[tokio::test]
    async fn an_approved_apply_reaches_the_plugin_and_leaves_an_audit_record() {
        let (state, session_id) = ready_state_with(
            Duration::from_secs(2),
            figtether_model::write::GrantMode::Write,
        );
        let plugin = attach_answering_plugin(
            &state,
            session_id,
            serde_json::json!({
                "preview": { "plan_hash": "hash", "operations_count": 1 },
                "result": { "created": ["1:9"], "changed": ["1:2"], "undo_committed": true },
            }),
        );
        let worker = tokio::spawn(run_worker(state.clone()));

        let preview = dispatch(
            &state,
            &identity(),
            ClientRequest::PreviewWrite {
                request_id: "req_preview".to_string(),
                params: write_params(session_id, "1:2"),
            },
        )
        .await;
        let preview = data_of(&preview);
        let approval_id = preview["approval_id"]
            .as_str()
            .expect("an approval id")
            .to_string();
        let plan_hash = preview["plan_hash"]
            .as_str()
            .expect("a plan hash")
            .to_string();
        assert_eq!(preview["preview"]["operations_count"], 1);

        // The user approves the preview in the plugin UI.
        let token = state
            .writes
            .decide(&figtether_protocol::write::ApprovalDecisionPayload {
                approval_id: approval_id.clone(),
                plan_hash: plan_hash.clone(),
                approved: true,
                reason: None,
            })
            .expect("decided")
            .expect("a token");

        let response = dispatch(
            &state,
            &identity(),
            ClientRequest::ApplyWrite {
                request_id: "req_apply".to_string(),
                params: apply_params(session_id, "1:2", &token),
            },
        )
        .await;
        let data = data_of(&response);
        assert_eq!(data["result"]["undo_committed"], true);
        assert_eq!(data["approval_id"], approval_id);

        // The plugin received the apply operation.
        let operations: Vec<String> = plugin
            .lock()
            .expect("lock")
            .iter()
            .map(|frame| {
                let payload: figtether_protocol::RequestPayload =
                    frame.payload_of().expect("the request payload");
                payload.op.as_str().to_string()
            })
            .collect();
        assert_eq!(
            operations,
            vec!["preview_write".to_string(), "apply_write".to_string()]
        );

        // The audit trail holds the record of the applied plan.
        let records = state.writes.audit_records();
        assert_eq!(records.len(), 1);
        let record = &records[0];
        assert_eq!(record.plan_id, "plan_1");
        assert_eq!(record.plan_hash, plan_hash);
        assert_eq!(record.approval_id, approval_id);
        assert_eq!(record.operations_count, 1);
        assert_eq!(record.created, 1);
        assert_eq!(record.changed, 1);
        assert_eq!(record.removed, 0);
        assert_eq!(record.moved, 0);

        worker.abort();
    }

    #[tokio::test]
    async fn the_audit_record_holds_no_design_content() {
        let (state, session_id) = ready_state_with(
            Duration::from_secs(2),
            figtether_model::write::GrantMode::Write,
        );
        let _plugin = attach_answering_plugin(
            &state,
            session_id,
            serde_json::json!({
                "preview": { "plan_hash": "hash" },
                "result": { "created": ["1:9"] },
            }),
        );
        let worker = tokio::spawn(run_worker(state.clone()));

        let preview = dispatch(
            &state,
            &identity(),
            ClientRequest::PreviewWrite {
                request_id: "req_preview".to_string(),
                params: write_params(session_id, "1:2"),
            },
        )
        .await;
        let preview = data_of(&preview);
        let approval_id = preview["approval_id"]
            .as_str()
            .expect("an approval id")
            .to_string();
        let plan_hash = preview["plan_hash"]
            .as_str()
            .expect("a plan hash")
            .to_string();
        let token = state
            .writes
            .decide(&figtether_protocol::write::ApprovalDecisionPayload {
                approval_id,
                plan_hash,
                approved: true,
                reason: None,
            })
            .expect("decided")
            .expect("a token");

        dispatch(
            &state,
            &identity(),
            ClientRequest::ApplyWrite {
                request_id: "req_apply".to_string(),
                params: apply_params(session_id, "1:2", &token),
            },
        )
        .await;

        let records = state.writes.audit_records();
        assert_eq!(records.len(), 1);
        let rendered = format!("{:?}", records[0]);
        assert!(!rendered.contains("hello"), "{rendered}");
        assert!(rendered.contains("plan_1"), "{rendered}");
        assert!(rendered.contains("operations_count: 1"), "{rendered}");
        // The record holds identifiers, counts, hashes, and timestamps only.
        let record = &records[0];
        assert_eq!(record.plan_id, "plan_1");
        assert_eq!(record.created, 1);
        assert_eq!(record.changed, 0);
        assert_eq!(record.removed, 0);
        assert_eq!(record.moved, 0);

        worker.abort();
    }

    #[tokio::test]
    async fn the_approval_answer_goes_only_to_the_requesting_client() {
        let (state, session_id) = ready_state_with(
            Duration::from_secs(2),
            figtether_model::write::GrantMode::Write,
        );
        let _plugin = attach_answering_plugin(
            &state,
            session_id,
            serde_json::json!({ "preview": { "plan_hash": "hash" } }),
        );
        let worker = tokio::spawn(run_worker(state.clone()));

        let preview = dispatch(
            &state,
            &identity(),
            ClientRequest::PreviewWrite {
                request_id: "req_preview".to_string(),
                params: write_params(session_id, "1:2"),
            },
        )
        .await;
        let preview = data_of(&preview);
        let approval_id = preview["approval_id"]
            .as_str()
            .expect("an approval id")
            .to_string();

        // The client that asked for the preview reads the status.
        let response = dispatch(
            &state,
            &identity(),
            ClientRequest::GetWriteApproval {
                request_id: "req_status".to_string(),
                params: serde_json::json!({ "approval_id": approval_id.clone() }),
            },
        )
        .await;
        let data = data_of(&response);
        assert_eq!(data["status"], "pending");
        assert!(data.get("approval_token").is_none());

        // Another client learns nothing.
        let response = dispatch(
            &state,
            &other_identity(),
            ClientRequest::GetWriteApproval {
                request_id: "req_status".to_string(),
                params: serde_json::json!({ "approval_id": approval_id.clone() }),
            },
        )
        .await;
        let error = error_of(&response);
        assert_eq!(error.code, ErrorCode::PermissionDenied);

        worker.abort();
    }

    #[tokio::test]
    async fn an_approved_plan_yields_a_token_the_client_can_read() {
        let (state, session_id) = ready_state_with(
            Duration::from_secs(2),
            figtether_model::write::GrantMode::Write,
        );
        let _plugin = attach_answering_plugin(
            &state,
            session_id,
            serde_json::json!({ "preview": { "plan_hash": "hash" } }),
        );
        let worker = tokio::spawn(run_worker(state.clone()));

        let preview = dispatch(
            &state,
            &identity(),
            ClientRequest::PreviewWrite {
                request_id: "req_preview".to_string(),
                params: write_params(session_id, "1:2"),
            },
        )
        .await;
        let preview = data_of(&preview);
        let approval_id = preview["approval_id"]
            .as_str()
            .expect("an approval id")
            .to_string();
        let plan_hash = preview["plan_hash"]
            .as_str()
            .expect("a plan hash")
            .to_string();

        state
            .writes
            .decide(&figtether_protocol::write::ApprovalDecisionPayload {
                approval_id: approval_id.clone(),
                plan_hash,
                approved: true,
                reason: None,
            })
            .expect("decided");

        let response = dispatch(
            &state,
            &identity(),
            ClientRequest::GetWriteApproval {
                request_id: "req_status".to_string(),
                params: serde_json::json!({ "approval_id": approval_id }),
            },
        )
        .await;
        let data = data_of(&response);
        assert_eq!(data["status"], "approved");
        assert!(
            data["approval_token"]
                .as_str()
                .is_some_and(|token| token.starts_with("w:")),
            "{data}"
        );

        worker.abort();
    }
}
