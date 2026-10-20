//! The write path of the daemon.
//!
//! The write path is plan based: the daemon validates a plan against the
//! write grant of the session, against the granted scope, and against the
//! write budgets, the plugin resolves the plan against the live document
//! without mutating it, the user approves or rejects the preview in the
//! plugin UI, and only then may the plan be applied
//! (`docs/11-write-path/01-write-path-design.md`, `ADR-013`).
//!
//! The daemon owns the state of that loop:
//!
//! - the approvals, so a write without a valid approval never reaches the
//!   document;
//! - the approval tokens, which are bound to the plan hash, the session, the
//!   grant epoch, and the approval id;
//! - the audit trail of the applied plans, which carries the session, the
//!   counts, and the hashes, and never design content.
//!
//! The daemon never mutates the document itself. It forwards plans, it
//! refuses the plans it does not trust, and it records what happened.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, RwLock, RwLockWriteGuard};
use std::time::Duration;

use chrono::{DateTime, Utc};
use figtether_core::write::{
    ApprovalRegistry, AuditRecord, WriteAudit, WritePolicy, MAX_AUDIT_RECORDS,
};
use figtether_core::ResourceLimits;
use figtether_model::errors::{ErrorCode, FigError};
use figtether_model::ids::SessionId;
use figtether_model::write::{
    canonical_plan_hash, validate_plan, write_operations_count, WriteBudget, WriteOperationKind,
    WritePlan, WriteViolation,
};
use figtether_protocol::write::{
    approval_token, parse_approval_token, ApprovalDecisionPayload, ApprovalTokenParts,
};
use uuid::Uuid;

use crate::DaemonState;

/// Whether two hashes name the same plan.
///
/// The canonical hash of a plan carries its `sha256:` prefix, and the digest
/// travels bare inside an approval token, because the fields of a token are
/// separated by colons. The comparison accepts both forms, so a token of one
/// build still names the plan of another.
pub fn same_hash(left: &str, right: &str) -> bool {
    left == right || bare_hash(left) == bare_hash(right)
}

/// The digest of a hash without its prefix.
fn bare_hash(hash: &str) -> &str {
    hash.strip_prefix("sha256:").unwrap_or(hash)
}

/// The decision the user took about an approval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalDecision {
    /// The user did not decide yet.
    Pending,
    /// The user approved the plan.
    Approved,
    /// The user rejected the plan.
    Rejected,
    /// The approval is not usable any more.
    Expired,
}

impl ApprovalDecision {
    /// Stable wire string of the decision.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Expired => "expired",
        }
    }
}

/// The answer to a question about an approval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalStatus {
    /// The user did not decide yet.
    Pending,
    /// The user approved the plan, and the token applies it.
    Approved {
        /// The token the client presents with the apply request.
        token: String,
    },
    /// The user rejected the plan.
    Rejected {
        /// Why the user said no, when they said why.
        reason: Option<String>,
    },
    /// The approval is not usable any more.
    Expired,
}

impl ApprovalStatus {
    /// Stable wire string of the status.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Approved { .. } => "approved",
            Self::Rejected { .. } => "rejected",
            Self::Expired => "expired",
        }
    }
}

/// One approval request of the daemon.
///
/// The request names the plan, the session it targets, the grant epoch it is
/// bound to, and the client that asked for the preview. It holds no design
/// content: the plan travels as the JSON the client sent.
#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    /// Identity of the approval.
    pub approval_id: String,
    /// Session the plan targets.
    pub session_id: SessionId,
    /// Canonical hash of the plan.
    pub plan_hash: String,
    /// Grant epoch the approval is bound to.
    pub grant_epoch: u64,
    /// When the approval was asked for.
    pub issued_at: DateTime<Utc>,
    /// When the approval stops being usable.
    pub expires_at: DateTime<Utc>,
    /// The client that asked for the preview.
    pub requester: String,
    /// The installation the requesting client belongs to.
    pub owner: String,
    /// The plan, as the client sent it.
    pub plan: serde_json::Value,
    /// The decision the user took.
    pub decision: ApprovalDecision,
    /// The token the client presents with the apply request.
    pub token: Option<String>,
    /// Why the user rejected the plan, when they said why.
    pub reason: Option<String>,
}

impl ApprovalRequest {
    /// Build a pending approval request.
    #[allow(clippy::too_many_arguments)]
    pub fn pending(
        approval_id: impl Into<String>,
        session_id: SessionId,
        plan_hash: impl Into<String>,
        grant_epoch: u64,
        issued_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
        requester: impl Into<String>,
        owner: impl Into<String>,
        plan: serde_json::Value,
    ) -> Self {
        Self {
            approval_id: approval_id.into(),
            session_id,
            plan_hash: plan_hash.into(),
            grant_epoch,
            issued_at,
            expires_at,
            requester: requester.into(),
            owner: owner.into(),
            plan,
            decision: ApprovalDecision::Pending,
            token: None,
            reason: None,
        }
    }

    /// Whether the approval outlived its lifetime.
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        now >= self.expires_at
    }
}

/// A write request of a client, parsed and validated.
#[derive(Debug, Clone)]
pub struct WriteRequest {
    /// The plan, as the client sent it.
    pub plan: serde_json::Value,
    /// The parsed plan.
    pub parsed: WritePlan,
    /// The session the plan targets.
    pub session_id: SessionId,
    /// The canonical hash of the plan.
    pub plan_hash: String,
}

impl WriteRequest {
    /// The identity of the plan.
    pub fn plan_id(&self) -> String {
        self.parsed.plan_id.clone()
    }

    /// The number of operations of the plan.
    pub fn operation_count(&self) -> u32 {
        write_operations_count(&self.parsed)
    }

    /// The node ids the plan names.
    ///
    /// Every node a plan touches has to be inside the granted scope of the
    /// session, so the daemon checks the node and the parent of every
    /// operation.
    pub fn named_nodes(&self) -> Vec<String> {
        let mut found: Vec<String> = Vec::new();
        for operation in &self.parsed.operations {
            if let Some(node_id) = operation.node_id.as_ref() {
                found.push(node_id.clone());
            }
            if let Some(parent_id) = operation.parent_id.as_ref() {
                found.push(parent_id.clone());
            }
        }
        found.sort();
        found.dedup();
        found
    }
}

/// One audit entry of the daemon.
///
/// The record of the core carries no session, because the trail of the core
/// is a trail of plans. The daemon keeps the session next to the record, so
/// a client can be shown the records of its own installation only. The entry
/// holds identifiers, counts, hashes, and a timestamp, never design content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEntry {
    /// The session the plan was applied in.
    pub session_id: SessionId,
    /// The record of the applied plan.
    pub record: AuditRecord,
}

/// The write path of one daemon instance.
///
/// The struct holds the approvals, the audit trail, and the write policy of
/// every session. It is shared by the client routing and by the bridge, so
/// the decision of the user and the answer of a client always see the same
/// state.
#[derive(Debug)]
pub struct WritePath {
    /// The approvals the core keeps.
    approvals: ApprovalRegistry,
    /// The audit trail of the applied plans.
    audit: WriteAudit,
    /// The audit trail with the session of every plan.
    entries: Mutex<VecDeque<AuditEntry>>,
    /// The write policy of every session.
    policies: RwLock<HashMap<SessionId, Arc<WritePolicy>>>,
    /// The approval requests of the daemon.
    requests: Mutex<HashMap<String, ApprovalRequest>>,
    /// The budgets of the daemon.
    limits: ResourceLimits,
}

impl WritePath {
    /// Build an empty write path.
    pub fn new(limits: ResourceLimits) -> Self {
        let approvals = ApprovalRegistry::with_limits(&limits);
        Self {
            approvals,
            audit: WriteAudit::new(),
            entries: Mutex::new(VecDeque::new()),
            policies: RwLock::new(HashMap::new()),
            requests: Mutex::new(HashMap::new()),
            limits,
        }
    }

    /// The approvals of the core.
    pub fn approvals(&self) -> &ApprovalRegistry {
        &self.approvals
    }

    /// The audit trail of the applied plans.
    pub fn audit(&self) -> &WriteAudit {
        &self.audit
    }

    /// The budgets of the daemon.
    pub fn limits(&self) -> &ResourceLimits {
        &self.limits
    }

    /// The write policy of a session.
    ///
    /// The rate window belongs to a session, so every session has its own
    /// policy.
    pub fn policy_for(&self, session_id: &SessionId) -> Arc<WritePolicy> {
        let mut guard = self.write_policies();
        guard
            .entry(*session_id)
            .or_insert_with(|| Arc::new(WritePolicy::new(self.limits.clone())))
            .clone()
    }

    /// The lifetime of one approval.
    pub fn approval_ttl(&self) -> Duration {
        Duration::from_secs(self.limits.approval_ttl_seconds)
    }

    /// Record an approval request.
    ///
    /// The request is recorded in the core registry, which holds the
    /// authoritative answer of the approval, and in the table of the daemon,
    /// which holds the client the answer belongs to.
    pub fn record(&self, request: ApprovalRequest) {
        self.approvals.record(
            &request.approval_id,
            &request.plan_hash,
            &request.session_id,
            request.grant_epoch,
            request.issued_at,
        );
        self.lock().insert(request.approval_id.clone(), request);
    }

    /// Record the decision the user took in the plugin UI.
    ///
    /// The decision answers the preview the plugin showed. An approved plan
    /// yields a token that is bound to the plan hash, the session, the grant
    /// epoch, and the approval id.
    pub fn decide(&self, decision: &ApprovalDecisionPayload) -> Result<Option<String>, FigError> {
        let mut guard = self.lock();
        let Some(request) = guard.get_mut(&decision.approval_id) else {
            return Err(FigError::new(
                ErrorCode::ApprovalRequired,
                "the approval is unknown to the daemon",
            ));
        };
        if !same_hash(&request.plan_hash, &decision.plan_hash) {
            return Err(FigError::new(
                ErrorCode::PlanHashMismatch,
                "the decision answers another plan than the one that was previewed",
            )
            .with_action("preview the plan again and let the user approve the new preview"));
        }
        if request.decision == ApprovalDecision::Approved {
            // The first decision wins: a second decision never replaces the
            // token the user already holds.
            return Ok(request.token.clone());
        }
        if !decision.approved {
            request.decision = ApprovalDecision::Rejected;
            request.reason = decision.reason.clone();
            return Ok(None);
        }
        let now = Utc::now();
        let record = self.approvals.approve(
            &request.approval_id,
            &request.plan_hash,
            &request.session_id,
            request.grant_epoch,
            now,
        )?;
        let token = approval_token(
            &record.approval_id,
            &record.plan_hash,
            &record.session_id,
            record.grant_epoch,
            record.issued_at,
        );
        request.decision = ApprovalDecision::Approved;
        request.token = Some(token.clone());
        Ok(Some(token))
    }

    /// The status of an approval.
    ///
    /// Only the client that asked for the preview may read the answer: the
    /// token is a capability, and a capability never leaks to another client.
    pub fn status(
        &self,
        approval_id: &str,
        requester: &str,
        owner: &str,
    ) -> Result<ApprovalStatus, FigError> {
        let mut guard = self.lock();
        let Some(request) = guard.get_mut(approval_id) else {
            return Err(FigError::new(
                ErrorCode::ApprovalRequired,
                "the approval is unknown to the daemon",
            ));
        };
        if request.requester != requester || request.owner != owner {
            return Err(FigError::new(
                ErrorCode::PermissionDenied,
                "the approval belongs to another client",
            ));
        }
        match request.decision {
            ApprovalDecision::Pending => {
                if request.is_expired(Utc::now()) {
                    request.decision = ApprovalDecision::Expired;
                    drop(guard);
                    self.expire(approval_id);
                    return Ok(ApprovalStatus::Expired);
                }
                Ok(ApprovalStatus::Pending)
            }
            ApprovalDecision::Approved => Ok(ApprovalStatus::Approved {
                token: request.token.clone().unwrap_or_default(),
            }),
            ApprovalDecision::Rejected => Ok(ApprovalStatus::Rejected {
                reason: request.reason.clone(),
            }),
            ApprovalDecision::Expired => Ok(ApprovalStatus::Expired),
        }
    }

    /// When an approval stops being usable.
    pub fn expires_at(&self, approval_id: &str) -> Option<DateTime<Utc>> {
        self.lock()
            .get(approval_id)
            .map(|request| request.expires_at)
    }

    /// Check an approval token before a plan is applied.
    ///
    /// The token has to parse, it has to belong to the session, it has to be
    /// bound to the grant epoch in force, it has to carry the hash of the
    /// plan that is applied, and the core has to know that the user approved
    /// it. Every other case is refused before the plugin is contacted.
    pub fn check_token(
        &self,
        token: &str,
        plan_hash: &str,
        session_id: &SessionId,
        grant_epoch: u64,
    ) -> Result<ApprovalTokenParts, FigError> {
        let now = Utc::now();
        let parts = parse_approval_token(token).ok_or_else(|| {
            FigError::new(
                ErrorCode::ApprovalRequired,
                "the request carries no valid approval token",
            )
            .with_action("ask the user to approve the preview before applying the plan")
        })?;
        if parts.session_id != *session_id {
            return Err(FigError::new(
                ErrorCode::ApprovalRequired,
                "the approval belongs to another session",
            ));
        }
        if parts.grant_epoch != grant_epoch {
            return Err(FigError::new(
                ErrorCode::ApprovalExpired,
                "the grant changed since the plan was approved",
            )
            .with_action("preview the plan again, the user has to approve the new preview"));
        }
        if !same_hash(&parts.plan_hash, plan_hash) {
            return Err(FigError::new(
                ErrorCode::PlanHashMismatch,
                "the plan differs from the plan the user approved",
            ));
        }
        if now >= parts.expires_at() {
            self.expire(&parts.approval_id);
            return Err(FigError::new(
                ErrorCode::ApprovalExpired,
                "the approval expired before the plan was applied",
            ));
        }
        if !self
            .approvals
            .is_approved(&parts.approval_id, plan_hash, session_id, grant_epoch, now)
        {
            return Err(FigError::new(
                ErrorCode::ApprovalRequired,
                "the plan was not approved by the user",
            ));
        }
        Ok(parts)
    }

    /// Expire one approval.
    pub fn expire(&self, approval_id: &str) {
        self.approvals.expire_one(approval_id);
        let mut guard = self.lock();
        if let Some(request) = guard.get_mut(approval_id) {
            if request.decision == ApprovalDecision::Pending {
                request.decision = ApprovalDecision::Expired;
            }
            request.token = None;
        }
    }

    /// Invalidate every approval of a grant epoch.
    ///
    /// A grant change moves the scope and the authority, so every approval
    /// that was issued under the old epoch dies with it.
    pub fn invalidate_grant(&self, grant_epoch: u64) -> usize {
        let invalidated = self.approvals.invalidate_grant(grant_epoch);
        let mut guard = self.lock();
        let victims: Vec<String> = guard
            .values()
            .filter(|request| request.grant_epoch == grant_epoch)
            .map(|request| request.approval_id.clone())
            .collect();
        let removed = victims.len();
        for victim in victims {
            if let Some(request) = guard.get_mut(&victim) {
                request.decision = ApprovalDecision::Expired;
                request.token = None;
            }
        }
        invalidated.max(removed)
    }

    /// Invalidate every approval of a session.
    pub fn invalidate_session(&self, session_id: &SessionId) -> usize {
        let mut guard = self.lock();
        let victims: Vec<String> = guard
            .values()
            .filter(|request| request.session_id == *session_id)
            .map(|request| request.approval_id.clone())
            .collect();
        let removed = victims.len();
        for victim in victims {
            if let Some(request) = guard.get_mut(&victim) {
                request.decision = ApprovalDecision::Expired;
                request.token = None;
            }
        }
        removed
    }

    /// Append a record to the audit trail.
    ///
    /// The session is kept next to the record, so a client only ever sees
    /// the records of the sessions of its own installation.
    pub fn record_audit(&self, session_id: &SessionId, record: AuditRecord) {
        self.audit.record(record.clone());
        let mut entries = self.lock_entries();
        entries.push_back(AuditEntry {
            session_id: *session_id,
            record,
        });
        while entries.len() > MAX_AUDIT_RECORDS {
            entries.pop_front();
        }
    }

    /// The records of the audit trail.
    pub fn audit_records(&self) -> Vec<AuditRecord> {
        self.audit.records()
    }

    /// The entries of the audit trail, oldest first.
    ///
    /// Every entry names the session the plan was applied in, so the caller
    /// can decide which records a client may see.
    pub fn audit_entries(&self) -> Vec<AuditEntry> {
        self.lock_entries().iter().cloned().collect()
    }

    /// Lock the table of the approval requests, recovering from a poisoned lock.
    fn lock(&self) -> MutexGuard<'_, HashMap<String, ApprovalRequest>> {
        self.requests
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Lock the table of the audit entries, recovering from a poisoned lock.
    fn lock_entries(&self) -> MutexGuard<'_, VecDeque<AuditEntry>> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Write the table of the policies, recovering from a poisoned lock.
    fn write_policies(&self) -> RwLockWriteGuard<'_, HashMap<SessionId, Arc<WritePolicy>>> {
        match self.policies.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// Mint the identity of an approval.
pub fn mint_approval_id() -> String {
    format!("apr_{}", Uuid::new_v4().simple())
}

/// Parse the plan of a write request.
///
/// The plan travels under `plan`. The session may be named by the plan
/// itself or by the parameters of the request. The plan is validated before
/// it is hashed, so a malformed plan never reaches the plugin.
pub fn parse_plan(
    params: &serde_json::Value,
    budget: &WriteBudget,
) -> Result<WriteRequest, FigError> {
    let raw = params.get("plan").ok_or_else(|| {
        FigError::new(
            ErrorCode::PlanInvalid,
            "the request does not carry a write plan",
        )
    })?;
    let parsed: WritePlan = serde_json::from_value(raw.clone()).map_err(|err| {
        FigError::new(
            ErrorCode::PlanInvalid,
            format!("the write plan is invalid: {err}"),
        )
    })?;
    let session_id = session_of(params, &parsed)?;
    validate_plan(&parsed, budget).map_err(|violation| violation_error(&violation))?;
    let plan_hash = canonical_plan_hash(&parsed);
    Ok(WriteRequest {
        plan: raw.clone(),
        parsed,
        session_id,
        plan_hash,
    })
}

/// Read the session a write request targets.
fn session_of(params: &serde_json::Value, plan: &WritePlan) -> Result<SessionId, FigError> {
    let named = plan.session_id.or_else(|| {
        params
            .get("session_id")
            .and_then(serde_json::Value::as_str)
            .and_then(|raw| raw.parse::<SessionId>().ok())
    });
    named.ok_or_else(|| {
        FigError::new(
            ErrorCode::SessionNotFound,
            "the write request does not name a session",
        )
    })
}

/// The error of a plan that broke a rule of the write path.
fn violation_error(violation: &WriteViolation) -> FigError {
    match violation {
        WriteViolation::TooManyOperations { .. }
        | WriteViolation::TooManyCreatedNodes { .. }
        | WriteViolation::TooMuchText { .. }
        | WriteViolation::TooManyPropertyChanges { .. } => {
            FigError::new(ErrorCode::BudgetExceeded, violation.message())
        }
        WriteViolation::DestructiveFlagMissing => {
            FigError::new(ErrorCode::DestructiveNotAllowed, violation.message())
        }
        _ => FigError::new(ErrorCode::PlanInvalid, violation.message()),
    }
}

/// The number of nodes a plan creates.
fn created_nodes(plan: &WritePlan) -> u32 {
    let count = plan
        .operations
        .iter()
        .filter(|operation| operation.kind == WriteOperationKind::CreateNode)
        .count();
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// The number of text characters a plan carries.
fn text_characters(plan: &WritePlan) -> usize {
    plan.operations
        .iter()
        .filter_map(|operation| operation.text.as_deref())
        .map(str::chars)
        .map(Iterator::count)
        .sum()
}

/// The number of property changes a plan carries.
fn property_changes(plan: &WritePlan) -> u32 {
    let count: usize = plan
        .operations
        .iter()
        .map(|operation| {
            operation.properties.len()
                + match operation.kind {
                    WriteOperationKind::SetText | WriteOperationKind::SetFills => 1,
                    _ => 0,
                }
        })
        .sum();
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// Refuse a write of a session that holds no write grant.
///
/// The check runs before the plugin is contacted: a session without a write
/// grant never reaches the document, not even for a preview. The session has
/// to belong to the installation of the client as well, so a client of
/// another installation never mutates a document.
pub fn require_write_grant(
    state: &DaemonState,
    session_id: &SessionId,
    owner: &str,
) -> Result<(), FigError> {
    let session = crate::guarded(None, || state.registry.get(session_id));
    let Some(session) = session else {
        return Err(FigError::new(
            ErrorCode::SessionNotFound,
            "the session does not exist",
        ));
    };
    if session.client_id != owner {
        return Err(FigError::new(
            ErrorCode::PermissionDenied,
            "the session does not belong to this installation",
        ));
    }
    // A paused or revoked session is reported with its own code, never as a
    // missing grant: the user decided, and the answer has to say so.
    match session.state {
        figtether_core::SessionState::Paused => {
            return Err(FigError::new(
                ErrorCode::Paused,
                "the user paused the session; new writes are refused",
            ))
        }
        figtether_core::SessionState::Revoked => {
            return Err(FigError::new(
                ErrorCode::Revoked,
                "the grant was revoked; only the user can create a new one",
            ))
        }
        _ => {}
    }
    let may_write = crate::guarded(false, || state.registry.may_write(session_id));
    if !may_write {
        return Err(FigError::new(
            ErrorCode::WriteNotGranted,
            "the session holds no write grant",
        )
        .with_action("ask the user to grant write access to the session in the plugin"));
    }
    Ok(())
}

/// Refuse a plan that names a node outside the granted scope.
///
/// The check reuses the scope index of the registry, so the daemon and the
/// plugin agree on what is inside the grant.
pub fn require_scope(state: &DaemonState, request: &WriteRequest) -> Result<(), FigError> {
    for node_id in request.named_nodes() {
        let inside = crate::guarded(false, || {
            state.registry.is_in_scope(&request.session_id, &node_id)
        });
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

/// Validate a plan against the structural rules and the per plan budgets.
///
/// The validation happens while the plan is parsed, so a malformed plan and
/// an oversized plan never reach the plugin.
pub fn validate(state: &DaemonState, request: &WriteRequest) -> Result<(), FigError> {
    let budget = WriteBudget::from(&state.limits);
    validate_plan(&request.parsed, &budget).map_err(|violation| violation_error(&violation))
}

/// Admit a plan against the write budgets of the daemon.
///
/// The admission covers the rolling window of the session as well, so it
/// belongs to the apply path: a preview changes nothing, and a preview is
/// not a write operation. The plugin repeats the same check on its side. The
/// daemon refuses first, so an oversized plan never reaches the document.
pub fn admit(state: &DaemonState, request: &WriteRequest) -> Result<(), FigError> {
    validate(state, request)?;
    let policy = state.writes.policy_for(&request.session_id);
    policy.admit(
        write_operations_count(&request.parsed),
        text_characters(&request.parsed),
        created_nodes(&request.parsed),
        property_changes(&request.parsed),
    )
}

/// Count the entries of a list of a plugin answer.
fn count_of(result: &serde_json::Value, key: &str) -> usize {
    match result.get(key) {
        Some(serde_json::Value::Array(items)) => items.len(),
        Some(serde_json::Value::Number(number)) => number.as_u64().unwrap_or(0) as usize,
        _ => 0,
    }
}

/// Build the audit record of an applied plan.
///
/// The record carries the identity of the plan, its hash, the approval that
/// allowed it, the number of operations, and the counts of the affected
/// nodes. It never carries a layer name, a text, or any other design
/// content.
pub fn audit_record(
    request: &WriteRequest,
    approval_id: &str,
    result: &serde_json::Value,
) -> AuditRecord {
    AuditRecord {
        plan_id: request.plan_id(),
        plan_hash: request.plan_hash.clone(),
        approval_id: approval_id.to_string(),
        operations_count: request.operation_count(),
        created: count_of(result, "created"),
        changed: count_of(result, "changed"),
        removed: count_of(result, "removed"),
        moved: count_of(result, "moved"),
        applied_at: Utc::now(),
    }
}

/// The preview of a plugin answer.
///
/// The plugin may answer with the preview itself or with a wrapper that
/// carries it under `preview`.
pub fn preview_of(data: &serde_json::Value) -> serde_json::Value {
    data.get("preview").cloned().unwrap_or_else(|| data.clone())
}

/// The result of a plugin answer.
///
/// The plugin may answer with the result itself or with a wrapper that
/// carries it under `result`.
pub fn result_of(data: &serde_json::Value) -> serde_json::Value {
    data.get("result").cloned().unwrap_or_else(|| data.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use figtether_model::write::WriteOperation;
    use std::collections::BTreeMap;

    fn session() -> SessionId {
        SessionId::new()
    }

    fn budget() -> WriteBudget {
        WriteBudget::default()
    }

    /// An operation that names nothing.
    fn operation(kind: WriteOperationKind) -> WriteOperation {
        WriteOperation {
            kind,
            parent_id: None,
            node_id: None,
            node_type: None,
            properties: BTreeMap::new(),
            fills: None,
            text: None,
            x: None,
            y: None,
            index: None,
            font_family: None,
            font_style: None,
        }
    }

    fn set_text(node_id: &str, text: &str) -> WriteOperation {
        WriteOperation {
            node_id: Some(node_id.to_string()),
            text: Some(text.to_string()),
            ..operation(WriteOperationKind::SetText)
        }
    }

    fn plan_json() -> serde_json::Value {
        plan_json_of(session())
    }

    fn plan_json_of(session_id: SessionId) -> serde_json::Value {
        serde_json::json!({
            "plan_id": "plan_1",
            "session_id": session_id.to_string(),
            "operations": [
                { "kind": "set_text", "node_id": "1:2", "text": "hello" }
            ],
            "destructive": false,
        })
    }

    fn request_of(path: &WritePath, session_id: SessionId) -> ApprovalRequest {
        let request = ApprovalRequest::pending(
            "apr_1",
            session_id,
            "hash_1",
            3,
            Utc::now(),
            Utc::now() + chrono::Duration::minutes(10),
            "client-a",
            "installation-1",
            plan_json(),
        );
        path.record(request.clone());
        request
    }

    #[test]
    fn a_plan_names_the_nodes_it_touches() {
        let plan = WritePlan {
            plan_id: "plan_1".to_string(),
            session_id: Some(session()),
            operations: vec![
                WriteOperation {
                    kind: WriteOperationKind::CreateNode,
                    parent_id: Some("1:2".to_string()),
                    node_type: Some("FRAME".to_string()),
                    ..operation(WriteOperationKind::CreateNode)
                },
                WriteOperation {
                    kind: WriteOperationKind::DeleteNode,
                    node_id: Some("1:3".to_string()),
                    ..operation(WriteOperationKind::DeleteNode)
                },
            ],
            intent: None,
            destructive: true,
        };
        let request = WriteRequest {
            plan: serde_json::json!({}),
            parsed: plan,
            session_id: session(),
            plan_hash: "hash_1".to_string(),
        };
        assert_eq!(
            request.named_nodes(),
            vec!["1:2".to_string(), "1:3".to_string()]
        );
    }

    #[test]
    fn the_plan_id_is_not_a_node_id() {
        let plan = WritePlan {
            plan_id: "plan_1".to_string(),
            session_id: Some(session()),
            operations: Vec::new(),
            intent: None,
            destructive: false,
        };
        let request = WriteRequest {
            plan: serde_json::json!({}),
            parsed: plan,
            session_id: session(),
            plan_hash: "hash_1".to_string(),
        };
        assert!(request.named_nodes().is_empty());
    }

    #[test]
    fn a_plan_is_parsed_and_hashed() {
        let session_id = session();
        let params = serde_json::json!({ "plan": plan_json_of(session_id) });
        let request = parse_plan(&params, &budget()).expect("parsed");
        assert_eq!(request.plan_id(), "plan_1");
        assert_eq!(request.operation_count(), 1);
        assert!(request.plan_hash.starts_with("sha256:"));
        assert_eq!(request.session_id, session_id);
    }

    #[test]
    fn a_request_without_a_plan_is_refused() {
        let error = parse_plan(&serde_json::json!({}), &budget()).expect_err("refused");
        assert_eq!(error.code, ErrorCode::PlanInvalid);
    }

    #[test]
    fn a_plan_without_a_session_is_refused() {
        let params = serde_json::json!({
            "plan": {
                "plan_id": "plan_1",
                "operations": [{ "kind": "set_text", "node_id": "1:2", "text": "hi" }],
            }
        });
        let error = parse_plan(&params, &budget()).expect_err("refused");
        assert_eq!(error.code, ErrorCode::SessionNotFound);
    }

    #[test]
    fn an_oversized_plan_is_refused_with_the_limit() {
        let operations: Vec<serde_json::Value> = (0..60)
            .map(|index| {
                serde_json::json!({ "kind": "set_text", "node_id": format!("1:{index}"), "text": "hi" })
            })
            .collect();
        let params = serde_json::json!({
            "plan": {
                "plan_id": "plan_1",
                "session_id": session().to_string(),
                "operations": operations,
            }
        });
        let error = parse_plan(&params, &budget()).expect_err("refused");
        assert_eq!(error.code, ErrorCode::BudgetExceeded);
        assert!(error.message.contains("50"), "{}", error.message);
    }

    #[test]
    fn a_destructive_plan_without_the_flag_is_refused() {
        let params = serde_json::json!({
            "plan": {
                "plan_id": "plan_1",
                "session_id": session().to_string(),
                "operations": [{ "kind": "delete_node", "node_id": "1:2" }],
            }
        });
        let error = parse_plan(&params, &budget()).expect_err("refused");
        assert_eq!(error.code, ErrorCode::DestructiveNotAllowed);
    }

    #[test]
    fn an_approval_is_bound_to_its_client() {
        let path = WritePath::new(ResourceLimits::default());
        request_of(&path, session());

        let status = path
            .status("apr_1", "client-a", "installation-1")
            .expect("the owner of the approval reads the status");
        assert_eq!(status, ApprovalStatus::Pending);

        let error = path
            .status("apr_1", "client-b", "installation-1")
            .expect_err("another client reads nothing");
        assert_eq!(error.code, ErrorCode::PermissionDenied);
    }

    #[test]
    fn an_unknown_approval_is_refused() {
        let path = WritePath::new(ResourceLimits::default());
        let error = path
            .status("apr_unknown", "client-a", "installation-1")
            .expect_err("the approval is unknown");
        assert_eq!(error.code, ErrorCode::ApprovalRequired);
    }

    #[test]
    fn an_approved_plan_yields_a_token() {
        let path = WritePath::new(ResourceLimits::default());
        let session_id = session();
        request_of(&path, session_id);

        let token = path
            .decide(&ApprovalDecisionPayload {
                approval_id: "apr_1".to_string(),
                plan_hash: "hash_1".to_string(),
                approved: true,
                reason: None,
            })
            .expect("decided")
            .expect("a token");

        let parts = parse_approval_token(&token).expect("the token parses");
        assert_eq!(parts.approval_id, "apr_1");
        assert_eq!(parts.plan_hash, "sha256:hash_1");
        assert_eq!(parts.session_id, session_id);
        assert_eq!(parts.grant_epoch, 3);

        let status = path
            .status("apr_1", "client-a", "installation-1")
            .expect("the status is known");
        assert_eq!(
            status,
            ApprovalStatus::Approved {
                token: token.clone()
            }
        );
    }

    #[test]
    fn a_rejected_plan_yields_no_token() {
        let path = WritePath::new(ResourceLimits::default());
        request_of(&path, session());

        let token = path
            .decide(&ApprovalDecisionPayload {
                approval_id: "apr_1".to_string(),
                plan_hash: "hash_1".to_string(),
                approved: false,
                reason: Some("wrong parent".to_string()),
            })
            .expect("decided");
        assert!(token.is_none());

        let status = path
            .status("apr_1", "client-a", "installation-1")
            .expect("the status is known");
        assert_eq!(
            status,
            ApprovalStatus::Rejected {
                reason: Some("wrong parent".to_string())
            }
        );
    }

    #[test]
    fn a_decision_that_answers_another_plan_is_refused() {
        let path = WritePath::new(ResourceLimits::default());
        request_of(&path, session());

        let error = path
            .decide(&ApprovalDecisionPayload {
                approval_id: "apr_1".to_string(),
                plan_hash: "hash_other".to_string(),
                approved: true,
                reason: None,
            })
            .expect_err("the decision answers another plan");
        assert_eq!(error.code, ErrorCode::PlanHashMismatch);
    }

    #[test]
    fn a_token_without_a_prefix_is_refused() {
        let path = WritePath::new(ResourceLimits::default());
        let error = path
            .check_token("not-a-token", "hash_1", &session(), 3)
            .expect_err("the token does not parse");
        assert_eq!(error.code, ErrorCode::ApprovalRequired);
    }

    #[test]
    fn an_expired_token_is_refused() {
        let path = WritePath::new(ResourceLimits::default());
        let session_id = session();
        let token = approval_token(
            "apr_1",
            "hash_1",
            &session_id,
            3,
            Utc::now() - chrono::Duration::minutes(11),
        );
        let error = path
            .check_token(&token, "hash_1", &session_id, 3)
            .expect_err("the approval expired");
        assert_eq!(error.code, ErrorCode::ApprovalExpired);
    }

    #[test]
    fn a_token_of_another_session_is_refused() {
        let path = WritePath::new(ResourceLimits::default());
        let token = approval_token("apr_1", "hash_1", &session(), 3, Utc::now());
        let error = path
            .check_token(&token, "hash_1", &session(), 3)
            .expect_err("the token belongs to another session");
        assert_eq!(error.code, ErrorCode::ApprovalRequired);
    }

    #[test]
    fn a_token_of_an_older_grant_is_refused() {
        let path = WritePath::new(ResourceLimits::default());
        let session_id = session();
        let token = approval_token("apr_1", "hash_1", &session_id, 3, Utc::now());
        let error = path
            .check_token(&token, "hash_1", &session_id, 4)
            .expect_err("the grant moved");
        assert_eq!(error.code, ErrorCode::ApprovalExpired);
    }

    #[test]
    fn a_token_of_another_plan_is_refused() {
        let path = WritePath::new(ResourceLimits::default());
        let session_id = session();
        let token = approval_token("apr_1", "hash_1", &session_id, 3, Utc::now());
        let error = path
            .check_token(&token, "hash_other", &session_id, 3)
            .expect_err("the plan differs");
        assert_eq!(error.code, ErrorCode::PlanHashMismatch);
    }

    #[test]
    fn a_token_without_an_approval_is_refused() {
        let path = WritePath::new(ResourceLimits::default());
        let session_id = session();
        let token = approval_token("apr_1", "hash_1", &session_id, 3, Utc::now());
        let error = path
            .check_token(&token, "hash_1", &session_id, 3)
            .expect_err("the user did not approve the plan");
        assert_eq!(error.code, ErrorCode::ApprovalRequired);
    }

    #[test]
    fn an_approved_token_is_accepted() {
        let path = WritePath::new(ResourceLimits::default());
        let session_id = session();
        request_of(&path, session_id);
        let token = path
            .decide(&ApprovalDecisionPayload {
                approval_id: "apr_1".to_string(),
                plan_hash: "hash_1".to_string(),
                approved: true,
                reason: None,
            })
            .expect("decided")
            .expect("a token");
        let parts = path
            .check_token(&token, "hash_1", &session_id, 3)
            .expect("the token is accepted");
        assert_eq!(parts.approval_id, "apr_1");
    }

    #[test]
    fn a_grant_change_invalidates_the_approvals_of_the_epoch() {
        let path = WritePath::new(ResourceLimits::default());
        request_of(&path, session());

        path.invalidate_grant(3);

        let status = path
            .status("apr_1", "client-a", "installation-1")
            .expect("the status is known");
        assert_eq!(status, ApprovalStatus::Expired);
    }

    #[test]
    fn the_counts_of_a_result_are_read_from_the_lists() {
        let result = serde_json::json!({
            "created": ["1:9", "1:10"],
            "changed": ["1:2"],
            "removed": [],
            "moved": ["1:3"],
        });
        assert_eq!(count_of(&result, "created"), 2);
        assert_eq!(count_of(&result, "changed"), 1);
        assert_eq!(count_of(&result, "removed"), 0);
        assert_eq!(count_of(&result, "moved"), 1);
    }

    #[test]
    fn the_preview_is_read_from_a_wrapped_answer() {
        let wrapped = serde_json::json!({ "preview": { "plan_hash": "hash_1" } });
        assert_eq!(preview_of(&wrapped)["plan_hash"], "hash_1");
        let plain = serde_json::json!({ "plan_hash": "hash_2" });
        assert_eq!(preview_of(&plain)["plan_hash"], "hash_2");
    }

    #[test]
    fn the_audit_record_holds_no_content() {
        let plan = WritePlan {
            plan_id: "plan_1".to_string(),
            session_id: Some(session()),
            operations: vec![set_text("1:2", "Buy now")],
            intent: Some("make it blue".to_string()),
            destructive: false,
        };
        let request = WriteRequest {
            plan: serde_json::json!({}),
            parsed: plan,
            session_id: session(),
            plan_hash: "hash_1".to_string(),
        };
        let result = serde_json::json!({
            "created": ["1:9"],
            "changed": ["1:2"],
        });
        let record = audit_record(&request, "apr_1", &result);
        assert_eq!(record.plan_id, "plan_1");
        assert_eq!(record.plan_hash, "hash_1");
        assert_eq!(record.approval_id, "apr_1");
        assert_eq!(record.operations_count, 1);
        assert_eq!(record.created, 1);
        assert_eq!(record.changed, 1);
        assert_eq!(record.removed, 0);
        assert_eq!(record.moved, 0);

        let rendered = format!("{record:?}");
        assert!(!rendered.contains("Buy now"), "{rendered}");
        assert!(!rendered.contains("make it blue"), "{rendered}");
    }

    #[test]
    fn an_approval_id_is_unique() {
        let first = mint_approval_id();
        let second = mint_approval_id();
        assert_ne!(first, second);
        assert!(first.starts_with("apr_"));
    }

    #[test]
    fn the_token_round_trips_through_the_protocol_functions() {
        let path = WritePath::new(ResourceLimits::default());
        let session_id = session();
        let request = request_of(&path, session_id);

        let token = path
            .decide(&ApprovalDecisionPayload {
                approval_id: "apr_1".to_string(),
                plan_hash: "hash_1".to_string(),
                approved: true,
                reason: None,
            })
            .expect("decided")
            .expect("a token");

        // The token is the one of the protocol crate, and the parser of the
        // protocol crate reads it back.
        assert_eq!(
            token,
            approval_token("apr_1", "hash_1", &session_id, 3, request.issued_at,),
            "the daemon mints the token with the protocol function"
        );
        let parts = parse_approval_token(&token).expect("the token parses");
        assert_eq!(parts.approval_id, "apr_1");
        assert_eq!(parts.session_id, session_id);
        assert_eq!(parts.grant_epoch, 3);
        assert_eq!(parts.issued_at, request.issued_at);
        // The parsed hash carries the canonical prefix, and the daemon stays
        // tolerant when it compares it with the hash of the plan.
        assert_eq!(parts.plan_hash, "sha256:hash_1");
        assert!(same_hash(&parts.plan_hash, "hash_1"));
        assert!(same_hash(&parts.plan_hash, "sha256:hash_1"));
        assert!(!same_hash(&parts.plan_hash, "hash_2"));
    }

    #[test]
    fn the_audit_entries_name_the_session_of_the_plan() {
        let path = WritePath::new(ResourceLimits::default());
        let session_id = session();
        let record = AuditRecord {
            plan_id: "plan_1".to_string(),
            plan_hash: "sha256:hash_1".to_string(),
            approval_id: "apr_1".to_string(),
            operations_count: 1,
            created: 1,
            changed: 0,
            removed: 0,
            moved: 0,
            applied_at: Utc::now(),
        };

        path.record_audit(&session_id, record.clone());

        let entries = path.audit_entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].session_id, session_id);
        assert_eq!(entries[0].record, record);
        assert_eq!(path.audit_records().len(), 1);
    }
}
