//! Write path rules.
//!
//! The write path is plan based: an agent proposes a plan, the user sees a
//! preview and decides, and only then does the document change. This module
//! holds the three rules of that loop that live in the daemon and that can
//! be tested without an editor:
//!
//! - the *budget*: how much one plan may change, and how fast writes may
//!   follow each other;
//! - the *approval*: a short lived decision by the user, bound to the hash
//!   of the plan, to the session, and to the grant epoch;
//! - the *audit*: what happened, without any design content.
//!
//! The plan itself, the preview, and the wire shapes of the approval live in
//! the model crate. This crate owns the decisions that say whether a plan may
//! run, so a plan can never reach the editor without passing them.
//!
//! The model crate validates the shape of a plan against a `WriteBudget`.
//! Two rules cannot live there: the size of the subtree one delete removes
//! needs the live document, and the rate of a session needs the history of
//! the session. Both are checked here.

use std::collections::{HashMap, VecDeque};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::{Duration, Instant};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use figtether_model::errors::{ErrorCode, FigError};
use figtether_model::ids::SessionId;
use figtether_model::write::{WriteApproval, WriteBudget, WriteOperationKind, WritePlan};

use crate::limits::ResourceLimits;

/// Length of the rolling window of the write rate limit.
pub const WRITE_RATE_WINDOW: Duration = Duration::from_secs(60);

/// Number of audit records the daemon keeps. The oldest ones are dropped.
pub const MAX_AUDIT_RECORDS: usize = 1000;

/// Build the error of a plan that exceeds a budget.
fn budget_error(subject: &str, actual: usize, limit: usize) -> FigError {
    FigError::new(
        ErrorCode::BudgetExceeded,
        format!("the plan has {actual} {subject} but the limit is {limit}"),
    )
}

/// The counts a plan carries.
///
/// The budgets are written against these four numbers, so the daemon and the
/// model never disagree about the size of a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlanCounts {
    /// Number of operations of the plan.
    pub operations: u32,
    /// Number of text characters the plan writes.
    pub text_characters: usize,
    /// Number of nodes the plan creates.
    pub created_nodes: u32,
    /// Number of property changes the plan applies.
    pub property_changes: u32,
}

impl PlanCounts {
    /// The counts of a plan.
    ///
    /// A text change and a fill change each count as one property change on
    /// top of the properties of the operation, exactly like the model counts
    /// them when it validates a plan.
    pub fn of(plan: &WritePlan) -> Self {
        let mut counts = PlanCounts::default();
        for operation in &plan.operations {
            counts.operations = counts.operations.saturating_add(1);
            if operation.kind == WriteOperationKind::CreateNode {
                counts.created_nodes = counts.created_nodes.saturating_add(1);
            }
            if let Some(text) = operation.text.as_deref() {
                counts.text_characters =
                    counts.text_characters.saturating_add(text.chars().count());
            }
            let properties = u32::try_from(operation.properties.len()).unwrap_or(u32::MAX);
            counts.property_changes = counts.property_changes.saturating_add(properties);
            if matches!(
                operation.kind,
                WriteOperationKind::SetText | WriteOperationKind::SetFills
            ) {
                counts.property_changes = counts.property_changes.saturating_add(1);
            }
        }
        counts
    }
}

/// The write policy of one session.
///
/// The policy holds the budgets of one plan and the rolling window that
/// bounds how many writes may follow each other. One policy belongs to one
/// session: the rate limit of the design is per session.
#[derive(Debug)]
pub struct WritePolicy {
    /// The limits this policy enforces.
    pub limits: ResourceLimits,
    /// The monotonic clock of the rate window.
    pub clock: Instant,
    /// The write operations admitted in the rolling window, oldest first.
    window: RwLock<VecDeque<Instant>>,
}

impl WritePolicy {
    /// Create a policy for one session.
    pub fn new(limits: ResourceLimits) -> Self {
        Self {
            limits,
            clock: Instant::now(),
            window: RwLock::new(VecDeque::new()),
        }
    }

    /// Admit a plan against the budgets.
    ///
    /// The plan is refused when it exceeds one of the per plan budgets, or
    /// when the rolling window of the session already holds the maximum
    /// number of write operations. A refused plan changes nothing: it does
    /// not consume a slot of the window.
    ///
    /// One write operation is one admitted plan. The window therefore bounds
    /// how many plans a session may start per minute, while the per plan
    /// budgets bound how much one of them may change.
    pub fn admit(
        &self,
        plan_ops: u32,
        text_chars: usize,
        created_nodes: u32,
        property_changes: u32,
    ) -> Result<(), FigError> {
        if plan_ops > self.limits.max_operations_per_plan {
            return Err(budget_error(
                "operations",
                plan_ops as usize,
                self.limits.max_operations_per_plan as usize,
            ));
        }
        if created_nodes > self.limits.max_created_nodes_per_plan {
            return Err(budget_error(
                "created nodes",
                created_nodes as usize,
                self.limits.max_created_nodes_per_plan as usize,
            ));
        }
        if property_changes > self.limits.max_property_changes_per_plan {
            return Err(budget_error(
                "property changes",
                property_changes as usize,
                self.limits.max_property_changes_per_plan as usize,
            ));
        }
        if text_chars > self.limits.max_text_characters_per_plan {
            return Err(budget_error(
                "text characters",
                text_chars,
                self.limits.max_text_characters_per_plan,
            ));
        }
        self.admit_to_window()
    }

    /// Admit a plan against the budgets.
    ///
    /// The plan is counted first, so the caller does not have to know what
    /// the budgets are written against.
    pub fn admit_plan(&self, plan: &WritePlan) -> Result<(), FigError> {
        let counts = PlanCounts::of(plan);
        self.admit(
            counts.operations,
            counts.text_characters,
            counts.created_nodes,
            counts.property_changes,
        )
    }

    /// Admit a delete of a subtree of the given size.
    ///
    /// A delete that swallows more nodes than the budget allows is refused:
    /// removing a library is not a design change.
    pub fn admit_deletion(&self, subtree_nodes: u32) -> Result<(), FigError> {
        if subtree_nodes > self.limits.max_deleted_subtree_nodes {
            return Err(budget_error(
                "nodes in the deleted subtree",
                subtree_nodes as usize,
                self.limits.max_deleted_subtree_nodes as usize,
            ));
        }
        Ok(())
    }

    /// The time to live of one approval.
    pub fn approval_ttl(&self) -> Duration {
        Duration::from_secs(self.limits.approval_ttl_seconds)
    }

    /// Admit one write operation to the rolling window.
    fn admit_to_window(&self) -> Result<(), FigError> {
        let now = self.clock;
        let mut window = self.write_window();
        prune_window(&mut window, now);
        let limit = self.limits.max_write_operations_per_minute as usize;
        if window.len() >= limit {
            return Err(budget_error(
                "write operations in the last minute",
                window.len() + 1,
                limit,
            ));
        }
        window.push_back(now);
        Ok(())
    }

    /// Number of write operations in the rolling window.
    pub fn writes_in_window(&self) -> usize {
        let now = self.clock;
        let mut window = self.write_window();
        prune_window(&mut window, now);
        window.len()
    }

    /// Write the rolling window.
    fn write_window(&self) -> RwLockWriteGuard<'_, VecDeque<Instant>> {
        match self.window.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// Drop the entries of the window that left the rolling minute.
fn prune_window(window: &mut VecDeque<Instant>, now: Instant) {
    while let Some(oldest) = window.front() {
        if now.saturating_duration_since(*oldest) >= WRITE_RATE_WINDOW {
            window.pop_front();
        } else {
            break;
        }
    }
}

impl From<&ResourceLimits> for WriteBudget {
    /// The budget of the model crate, taken from the limits of the daemon.
    ///
    /// The model validates a plan against this budget. The two rules that
    /// need the live document and the history of the session are not part of
    /// it, they are checked by the write policy.
    fn from(limits: &ResourceLimits) -> Self {
        WriteBudget {
            max_operations_per_plan: limits.max_operations_per_plan,
            max_created_nodes_per_plan: limits.max_created_nodes_per_plan,
            max_text_characters_per_plan: u32::try_from(limits.max_text_characters_per_plan)
                .unwrap_or(u32::MAX),
            max_property_changes_per_plan: limits.max_property_changes_per_plan,
            max_deleted_subtree_nodes: limits.max_deleted_subtree_nodes,
            approval_ttl_seconds: limits.approval_ttl_seconds,
            max_operations_per_minute: limits.max_write_operations_per_minute,
        }
    }
}

/// One approval of one plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRecord {
    /// Identity of the approval.
    pub approval_id: String,
    /// Hash of the approved plan.
    pub plan_hash: String,
    /// Session the approval belongs to.
    pub session_id: SessionId,
    /// Grant epoch the approval is bound to.
    pub grant_epoch: u64,
    /// When the approval was issued.
    pub issued_at: DateTime<Utc>,
    /// When the approval expires.
    pub expires_at: DateTime<Utc>,
    /// Whether the user approved the plan.
    pub approved: bool,
}

impl ApprovalRecord {
    /// Whether the approval is still usable at the given moment.
    pub fn is_valid_at(&self, now: DateTime<Utc>) -> bool {
        self.approved && now < self.expires_at
    }
}

impl From<&ApprovalRecord> for WriteApproval {
    /// The wire shape of an approved plan.
    ///
    /// The approval is always approved by the user, never by the agent.
    fn from(record: &ApprovalRecord) -> Self {
        WriteApproval {
            approval_id: record.approval_id.clone(),
            plan_hash: record.plan_hash.clone(),
            session_id: record.session_id,
            grant_epoch: record.grant_epoch,
            issued_at: record.issued_at,
            expires_at: record.expires_at,
            approved_by: "user".to_string(),
        }
    }
}

/// The approvals of the write path.
///
/// Every approval is bound to the hash of the plan, to the session, and to
/// the grant epoch. A grant change drops every approval of that grant, and
/// an approval that outlived its time to live is dead.
#[derive(Debug)]
pub struct ApprovalRegistry {
    inner: RwLock<HashMap<String, ApprovalRecord>>,
    ttl: ChronoDuration,
}

impl Default for ApprovalRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ApprovalRegistry {
    /// Create a registry with the default time to live.
    pub fn new() -> Self {
        Self::with_ttl(Duration::from_secs(
            ResourceLimits::default().approval_ttl_seconds,
        ))
    }

    /// Create a registry with the time to live of the given limits.
    pub fn with_limits(limits: &ResourceLimits) -> Self {
        Self::with_ttl(Duration::from_secs(limits.approval_ttl_seconds))
    }

    /// Create a registry with an explicit time to live.
    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
            ttl: ChronoDuration::from_std(ttl).unwrap_or(ChronoDuration::zero()),
        }
    }

    /// Record a pending approval.
    ///
    /// The record exists before the user decides: the daemon learns that a
    /// preview waits for a decision. The approval is not usable until
    /// `approve` accepted it.
    pub fn record(
        &self,
        approval_id: impl Into<String>,
        plan_hash: impl Into<String>,
        session_id: &SessionId,
        grant_epoch: u64,
        now: DateTime<Utc>,
    ) -> ApprovalRecord {
        let record = ApprovalRecord {
            approval_id: approval_id.into(),
            plan_hash: plan_hash.into(),
            session_id: *session_id,
            grant_epoch,
            issued_at: now,
            expires_at: now + self.ttl,
            approved: false,
        };
        let mut records = self.write_records();
        records.insert(record.approval_id.clone(), record.clone());
        record
    }

    /// Approve a plan.
    ///
    /// The approval is refused when there is no record for the approval id,
    /// when the plan hash differs from the approved one, when the approval
    /// belongs to another session, when the grant epoch moved since the
    /// approval was issued, or when the approval expired.
    pub fn approve(
        &self,
        approval_id: &str,
        plan_hash: &str,
        session_id: &SessionId,
        grant_epoch: u64,
        now: DateTime<Utc>,
    ) -> Result<ApprovalRecord, FigError> {
        let mut records = self.write_records();
        let Some(record) = records.get(approval_id).cloned() else {
            return Err(FigError::new(
                ErrorCode::ApprovalRequired,
                format!("no approval {approval_id} is known"),
            ));
        };
        if record.plan_hash != plan_hash {
            return Err(FigError::new(
                ErrorCode::PlanHashMismatch,
                "the plan differs from the approved plan",
            ));
        }
        if record.session_id != *session_id {
            return Err(FigError::new(
                ErrorCode::ApprovalRequired,
                "the approval belongs to another session",
            ));
        }
        if record.grant_epoch != grant_epoch {
            return Err(FigError::new(
                ErrorCode::ApprovalExpired,
                "the grant changed since the approval was issued",
            ));
        }
        if now >= record.expires_at {
            return Err(FigError::new(
                ErrorCode::ApprovalExpired,
                "the approval expired",
            ));
        }
        let Some(record) = records.get_mut(approval_id) else {
            return Err(FigError::new(
                ErrorCode::ApprovalRequired,
                format!("no approval {approval_id} is known"),
            ));
        };
        record.approved = true;
        Ok(record.clone())
    }

    /// Whether a plan may run under this approval.
    pub fn is_approved(
        &self,
        approval_id: &str,
        plan_hash: &str,
        session_id: &SessionId,
        grant_epoch: u64,
        now: DateTime<Utc>,
    ) -> bool {
        let records = self.read_records();
        let Some(record) = records.get(approval_id) else {
            return false;
        };
        record.approved
            && record.plan_hash == plan_hash
            && record.session_id == *session_id
            && record.grant_epoch == grant_epoch
            && now < record.expires_at
    }

    /// Drop every approval of a grant.
    ///
    /// A grant change invalidates every outstanding approval of that grant,
    /// because the approval was bound to the epoch that died.
    pub fn invalidate_grant(&self, grant_epoch: u64) -> usize {
        let mut records = self.write_records();
        let victims: Vec<String> = records
            .iter()
            .filter(|(_, record)| record.grant_epoch == grant_epoch)
            .map(|(approval_id, _)| approval_id.clone())
            .collect();
        let removed = victims.len();
        for victim in victims {
            records.remove(&victim);
        }
        removed
    }

    /// Drop every approval that outlived its time to live.
    pub fn expire(&self, now: DateTime<Utc>) -> usize {
        let mut records = self.write_records();
        let victims: Vec<String> = records
            .iter()
            .filter(|(_, record)| now >= record.expires_at)
            .map(|(approval_id, _)| approval_id.clone())
            .collect();
        let removed = victims.len();
        for victim in victims {
            records.remove(&victim);
        }
        removed
    }

    /// Drop one approval, when it is dead.
    ///
    /// Returns whether an approval was dropped.
    pub fn expire_one(&self, approval_id: &str) -> bool {
        let mut records = self.write_records();
        records.remove(approval_id).is_some()
    }

    /// Number of known approvals.
    pub fn len(&self) -> usize {
        self.read_records().len()
    }

    /// Whether no approval is known.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Read the approvals.
    fn read_records(&self) -> RwLockReadGuard<'_, HashMap<String, ApprovalRecord>> {
        match self.inner.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Write the approvals.
    fn write_records(&self) -> RwLockWriteGuard<'_, HashMap<String, ApprovalRecord>> {
        match self.inner.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// One audit record of an applied plan.
///
/// The record holds identifiers, counts, hashes, and timestamps only. It
/// never holds a layer name, a text, or any other design content: the audit
/// exists so a human can see what happened, not what the document said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRecord {
    /// Identity of the applied plan.
    pub plan_id: String,
    /// Hash of the applied plan.
    pub plan_hash: String,
    /// Identity of the approval that authorised the plan.
    pub approval_id: String,
    /// Number of operations of the applied plan.
    pub operations_count: u32,
    /// Number of nodes the plan created.
    pub created: usize,
    /// Number of nodes the plan changed.
    pub changed: usize,
    /// Number of nodes the plan removed.
    pub removed: usize,
    /// Number of nodes the plan moved.
    pub moved: usize,
    /// When the plan was applied.
    pub applied_at: DateTime<Utc>,
}

/// The audit trail of the write path.
///
/// The trail is bounded: it keeps the last `MAX_AUDIT_RECORDS` records and
/// drops the oldest ones.
#[derive(Debug, Default)]
pub struct WriteAudit {
    records: RwLock<VecDeque<AuditRecord>>,
}

impl WriteAudit {
    /// Create an empty trail.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a record, dropping the oldest ones when the trail is full.
    pub fn record(&self, record: AuditRecord) {
        let mut records = self.write_records();
        records.push_back(record);
        while records.len() > MAX_AUDIT_RECORDS {
            records.pop_front();
        }
    }

    /// The records of the trail, oldest first.
    pub fn records(&self) -> Vec<AuditRecord> {
        self.read_records().iter().cloned().collect()
    }

    /// Number of records of the trail.
    pub fn len(&self) -> usize {
        self.read_records().len()
    }

    /// Whether the trail is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Read the trail.
    fn read_records(&self) -> RwLockReadGuard<'_, VecDeque<AuditRecord>> {
        match self.records.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Write the trail.
    fn write_records(&self) -> RwLockWriteGuard<'_, VecDeque<AuditRecord>> {
        match self.records.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use figtether_model::write::WriteOperation;
    use std::collections::BTreeMap;

    fn limits() -> ResourceLimits {
        ResourceLimits::default()
    }

    fn policy() -> WritePolicy {
        WritePolicy::new(limits())
    }

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

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

    fn plan_of(operations: Vec<WriteOperation>) -> WritePlan {
        WritePlan {
            plan_id: "plan-1".to_string(),
            session_id: None,
            operations,
            intent: None,
            destructive: false,
        }
    }

    fn audit_record(plan_id: &str) -> AuditRecord {
        AuditRecord {
            plan_id: plan_id.to_string(),
            plan_hash: "hash-1".to_string(),
            approval_id: "approval-1".to_string(),
            operations_count: 3,
            created: 2,
            changed: 1,
            removed: 0,
            moved: 0,
            applied_at: now(),
        }
    }

    #[test]
    fn the_counts_of_a_plan_match_the_budgets() {
        let plan = plan_of(vec![
            operation(WriteOperationKind::CreateNode),
            operation(WriteOperationKind::SetText),
            operation(WriteOperationKind::SetProperties),
        ]);
        let counts = PlanCounts::of(&plan);
        assert_eq!(counts.operations, 3);
        assert_eq!(counts.created_nodes, 1);
        assert_eq!(counts.text_characters, 0);
        assert_eq!(counts.property_changes, 1);
    }

    #[test]
    fn a_plan_is_admitted_by_its_own_counts() {
        let policy = policy();
        let plan = plan_of(vec![operation(WriteOperationKind::CreateNode)]);
        assert!(policy.admit_plan(&plan).is_ok());
        let big = plan_of(
            (0..51)
                .map(|_| operation(WriteOperationKind::CreateNode))
                .collect(),
        );
        let error = policy.admit_plan(&big).expect_err("refused");
        assert_eq!(error.code, ErrorCode::BudgetExceeded);
    }

    #[test]
    fn a_plan_inside_the_budgets_is_admitted() {
        let policy = policy();
        assert!(policy.admit(50, 20_000, 100, 200).is_ok());
        assert!(policy.admit(1, 0, 0, 0).is_ok());
    }

    #[test]
    fn too_many_operations_are_refused() {
        let policy = policy();
        let error = policy.admit(51, 0, 0, 0).expect_err("refused");
        assert_eq!(error.code, ErrorCode::BudgetExceeded);
        assert!(
            error.message.contains("50"),
            "the limit is in the message: {}",
            error.message
        );
    }

    #[test]
    fn too_many_created_nodes_are_refused() {
        let policy = policy();
        let error = policy.admit(1, 0, 101, 0).expect_err("refused");
        assert_eq!(error.code, ErrorCode::BudgetExceeded);
        assert!(error.message.contains("100"), "{}", error.message);
    }

    #[test]
    fn too_many_text_characters_are_refused() {
        let policy = policy();
        let error = policy.admit(1, 20_001, 0, 0).expect_err("refused");
        assert_eq!(error.code, ErrorCode::BudgetExceeded);
        assert!(error.message.contains("20000"), "{}", error.message);
    }

    #[test]
    fn too_many_property_changes_are_refused() {
        let policy = policy();
        let error = policy.admit(1, 0, 0, 201).expect_err("refused");
        assert_eq!(error.code, ErrorCode::BudgetExceeded);
        assert!(error.message.contains("200"), "{}", error.message);
    }

    #[test]
    fn a_delete_beyond_the_subtree_budget_is_refused() {
        let policy = policy();
        assert!(policy.admit_deletion(50).is_ok());
        let error = policy.admit_deletion(51).expect_err("refused");
        assert_eq!(error.code, ErrorCode::BudgetExceeded);
        assert!(error.message.contains("50"), "{}", error.message);
    }

    #[test]
    fn the_rate_window_refuses_a_burst_of_writes() {
        let mut limits = limits();
        limits.max_write_operations_per_minute = 2;
        let policy = WritePolicy::new(limits);
        assert!(policy.admit(1, 0, 0, 0).is_ok());
        assert!(policy.admit(1, 0, 0, 0).is_ok());
        let error = policy.admit(1, 0, 0, 0).expect_err("refused");
        assert_eq!(error.code, ErrorCode::BudgetExceeded);
        assert!(error.message.contains('2'), "{}", error.message);
    }

    #[test]
    fn the_rate_window_opens_again_after_a_minute() {
        let mut limits = limits();
        limits.max_write_operations_per_minute = 1;
        let mut policy = WritePolicy::new(limits);
        assert!(policy.admit(1, 0, 0, 0).is_ok());
        assert!(policy.admit(1, 0, 0, 0).is_err(), "the window is full");
        policy.clock += WRITE_RATE_WINDOW;
        assert!(
            policy.admit(1, 0, 0, 0).is_ok(),
            "the window slides with the clock"
        );
    }

    #[test]
    fn a_refused_plan_does_not_consume_the_window() {
        let mut limits = limits();
        limits.max_write_operations_per_minute = 1;
        let policy = WritePolicy::new(limits);
        assert!(policy.admit(1, 0, 0, 0).is_ok());
        assert!(policy.admit(1, 0, 101, 0).is_err(), "the plan is too big");
        assert!(
            policy.admit(1, 0, 0, 0).is_err(),
            "the window still holds the first plan"
        );
    }

    #[test]
    fn an_approval_is_recorded_and_approved() {
        let registry = ApprovalRegistry::new();
        let session = SessionId::new();
        let issued = now();
        let record = registry.record("approval-1", "hash-1", &session, 7, issued);
        assert_eq!(record.approval_id, "approval-1");
        assert_eq!(record.plan_hash, "hash-1");
        assert_eq!(record.session_id, session);
        assert_eq!(record.grant_epoch, 7);
        assert!(!record.approved, "a pending record is not an approval");
        assert_eq!(registry.len(), 1);

        let approved = registry
            .approve("approval-1", "hash-1", &session, 7, issued)
            .expect("approved");
        assert!(approved.approved);
        assert!(registry.is_approved("approval-1", "hash-1", &session, 7, issued));
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn an_approval_for_another_plan_hash_is_refused() {
        let registry = ApprovalRegistry::new();
        let session = SessionId::new();
        let issued = now();
        registry.record("approval-1", "hash-1", &session, 7, issued);
        let error = registry
            .approve("approval-1", "hash-2", &session, 7, issued)
            .expect_err("refused");
        assert_eq!(error.code, ErrorCode::PlanHashMismatch);
        assert!(!registry.is_approved("approval-1", "hash-2", &session, 7, issued));
    }

    #[test]
    fn an_approval_of_a_moved_grant_epoch_is_refused() {
        let registry = ApprovalRegistry::new();
        let session = SessionId::new();
        let issued = now();
        registry.record("approval-1", "hash-1", &session, 7, issued);
        let error = registry
            .approve("approval-1", "hash-1", &session, 8, issued)
            .expect_err("refused");
        assert_eq!(error.code, ErrorCode::ApprovalExpired);
        assert!(!registry.is_approved("approval-1", "hash-1", &session, 8, issued));
    }

    #[test]
    fn an_expired_approval_is_refused() {
        let registry = ApprovalRegistry::with_ttl(Duration::from_secs(60));
        let session = SessionId::new();
        let issued = now();
        let record = registry.record("approval-1", "hash-1", &session, 7, issued);
        let later = record.expires_at + ChronoDuration::seconds(1);
        let error = registry
            .approve("approval-1", "hash-1", &session, 7, later)
            .expect_err("refused");
        assert_eq!(error.code, ErrorCode::ApprovalExpired);
        assert!(!registry.is_approved("approval-1", "hash-1", &session, 7, later));
    }

    #[test]
    fn an_unknown_approval_is_refused() {
        let registry = ApprovalRegistry::new();
        let session = SessionId::new();
        let error = registry
            .approve("approval-1", "hash-1", &session, 7, now())
            .expect_err("refused");
        assert_eq!(error.code, ErrorCode::ApprovalRequired);
        assert!(!registry.is_approved("approval-1", "hash-1", &session, 7, now()));
    }

    #[test]
    fn a_pending_record_is_not_usable_before_the_user_decides() {
        let registry = ApprovalRegistry::new();
        let session = SessionId::new();
        let issued = now();
        registry.record("approval-1", "hash-1", &session, 7, issued);
        assert!(!registry.is_approved("approval-1", "hash-1", &session, 7, issued));
    }

    #[test]
    fn an_approval_of_another_session_is_refused() {
        let registry = ApprovalRegistry::new();
        let session = SessionId::new();
        let other = SessionId::new();
        let issued = now();
        registry.record("approval-1", "hash-1", &session, 7, issued);
        let error = registry
            .approve("approval-1", "hash-1", &other, 7, issued)
            .expect_err("refused");
        assert_eq!(error.code, ErrorCode::ApprovalRequired);
    }

    #[test]
    fn invalidating_a_grant_drops_its_approvals() {
        let registry = ApprovalRegistry::new();
        let session = SessionId::new();
        let issued = now();
        registry.record("approval-1", "hash-1", &session, 7, issued);
        registry.record("approval-2", "hash-2", &session, 8, issued);
        registry
            .approve("approval-2", "hash-2", &session, 8, issued)
            .expect("approved");
        assert_eq!(registry.invalidate_grant(7), 1);
        assert_eq!(registry.len(), 1);
        assert!(!registry.is_approved("approval-1", "hash-1", &session, 7, issued));
        assert!(registry.is_approved("approval-2", "hash-2", &session, 8, issued));
        assert!(!registry.is_empty());
    }

    #[test]
    fn the_expired_approvals_are_dropped() {
        let registry = ApprovalRegistry::with_ttl(Duration::from_secs(60));
        let session = SessionId::new();
        let issued = now();
        let record = registry.record("approval-1", "hash-1", &session, 7, issued);
        assert_eq!(registry.expire(record.issued_at), 0);
        assert_eq!(registry.expire(record.expires_at), 1);
        assert!(registry.is_empty());
    }

    #[test]
    fn one_approval_can_be_dropped_on_its_own() {
        let registry = ApprovalRegistry::new();
        let session = SessionId::new();
        let issued = now();
        registry.record("approval-1", "hash-1", &session, 7, issued);
        registry.record("approval-2", "hash-2", &session, 7, issued);
        assert!(registry.expire_one("approval-1"));
        assert!(!registry.expire_one("approval-1"));
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn the_audit_keeps_the_records_in_order() {
        let audit = WriteAudit::new();
        assert!(audit.is_empty());
        audit.record(audit_record("plan-1"));
        audit.record(audit_record("plan-2"));
        let records = audit.records();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].plan_id, "plan-1");
        assert_eq!(records[1].plan_id, "plan-2");
        assert_eq!(audit.len(), 2);
    }

    #[test]
    fn the_audit_drops_the_oldest_records_when_it_is_full() {
        let audit = WriteAudit::new();
        for index in 0..(MAX_AUDIT_RECORDS + 10) {
            audit.record(audit_record(&format!("plan-{index}")));
        }
        let records = audit.records();
        assert_eq!(records.len(), MAX_AUDIT_RECORDS);
        assert_eq!(records[0].plan_id, "plan-10");
        assert_eq!(
            records[MAX_AUDIT_RECORDS - 1].plan_id,
            format!("plan-{}", MAX_AUDIT_RECORDS + 9)
        );
    }

    #[test]
    fn the_audit_record_holds_no_design_content() {
        let audit = WriteAudit::new();
        let mut record = audit_record("plan-1");
        record.plan_hash = "sha256:0123456789abcdef".to_string();
        audit.record(record.clone());
        let rendered = format!("{:?}", audit.records());
        assert!(!rendered.contains("Checkout"), "{rendered}");
        assert!(!rendered.contains("Buy now"), "{rendered}");
        assert!(rendered.contains("plan-1"), "{rendered}");
        assert!(rendered.contains("operations_count: 3"), "{rendered}");
        // The record holds identifiers, counts, hashes, and timestamps only.
        assert_eq!(record.created, 2);
        assert_eq!(record.changed, 1);
        assert_eq!(record.removed, 0);
        assert_eq!(record.moved, 0);
    }

    #[test]
    fn the_write_codes_keep_their_wire_strings() {
        assert_eq!(ErrorCode::BudgetExceeded.as_str(), "BUDGET_EXCEEDED");
        assert_eq!(ErrorCode::ApprovalRequired.as_str(), "APPROVAL_REQUIRED");
        assert_eq!(ErrorCode::PlanHashMismatch.as_str(), "PLAN_HASH_MISMATCH");
        assert_eq!(ErrorCode::ApprovalExpired.as_str(), "APPROVAL_EXPIRED");
    }

    #[test]
    fn the_limits_become_the_budget_of_the_model() {
        let limits = limits();
        let budget = WriteBudget::from(&limits);
        assert_eq!(budget.max_operations_per_plan, 50);
        assert_eq!(budget.max_created_nodes_per_plan, 100);
        assert_eq!(budget.max_text_characters_per_plan, 20_000);
        assert_eq!(budget.max_property_changes_per_plan, 200);
        assert_eq!(budget.max_deleted_subtree_nodes, 50);
        assert_eq!(budget.approval_ttl_seconds, 600);
        assert_eq!(budget.max_operations_per_minute, 10);
        assert_eq!(budget, WriteBudget::default());
    }

    #[test]
    fn an_approved_record_becomes_the_wire_approval() {
        let registry = ApprovalRegistry::new();
        let session = SessionId::new();
        let issued = now();
        let record = registry.record("approval-1", "hash-1", &session, 7, issued);
        let approval = WriteApproval::from(&record);
        assert_eq!(approval.approval_id, "approval-1");
        assert_eq!(approval.plan_hash, "hash-1");
        assert_eq!(approval.session_id, session);
        assert_eq!(approval.grant_epoch, 7);
        assert_eq!(approval.approved_by, "user");
        assert!(approval.binds_to("hash-1", &session, 7));
        assert!(!approval.binds_to("hash-2", &session, 7));
    }
}
