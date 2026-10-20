//! Scheduler and backpressure.
//!
//! The editor is a shared, fragile resource: at most one operation may run
//! inside a session at a time, exports are serialised globally, and the
//! queue is bounded. When the queue is full the caller receives `BUSY` with
//! a `retry_after_ms` hint instead of an unbounded wait.
//!
//! Public signatures in this file are frozen for the v0.1 implementation round.

use std::collections::{HashMap, HashSet};
use std::sync::{RwLock, RwLockWriteGuard};
use std::time::{Duration, Instant};

use figtether_model::errors::{ErrorCode, FigError};
use figtether_model::ids::{JobId, SessionId};
use figtether_protocol::BridgeOp;

use crate::limits::ResourceLimits;
use crate::metrics::Metrics;

/// Delay a caller should wait before it retries a rejected job.
pub const BUSY_RETRY_AFTER_MS: u64 = 250;

/// Time a queued job waits before aging promotes it one priority level.
///
/// Aging keeps a large job from starving forever: the longer it waits, the
/// more priority levels it gains. Control traffic never ages, it preempts.
pub const AGING_STEP: Duration = Duration::from_millis(500);

/// Kind of work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    /// A metadata read.
    Read,
    /// A subtree inspection.
    Inspect,
    /// An asset export.
    Export,
    /// A snapshot capture.
    Snapshot,
    /// A watch registration.
    Watch,
}

impl JobKind {
    /// The bridge operation of this job kind.
    pub const fn op(self) -> BridgeOp {
        match self {
            Self::Read => BridgeOp::ReadNodes,
            Self::Inspect => BridgeOp::InspectDesign,
            Self::Export => BridgeOp::ExportAssets,
            Self::Snapshot => BridgeOp::CaptureSnapshot,
            Self::Watch => BridgeOp::WatchScope,
        }
    }

    /// Whether the job occupies an export lane.
    pub const fn is_export(self) -> bool {
        matches!(self, Self::Export)
    }
}

/// Scheduling priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    /// Revoke, cancel, and heartbeat handling.
    Control,
    /// Small reads.
    SmallRead,
    /// Long running exports.
    Export,
}

/// One scheduled job.
#[derive(Debug, Clone)]
pub struct Job {
    /// Identity of the job.
    pub job_id: JobId,
    /// Kind of work.
    pub kind: JobKind,
    /// Session the job belongs to.
    pub session_id: Option<SessionId>,
    /// Client that submitted the job.
    pub client_id: String,
    /// Parameters of the operation.
    pub params: serde_json::Value,
    /// Deadline of the job, queue wait included.
    pub deadline: Instant,
    /// When the job was enqueued.
    pub enqueued_at: Instant,
    /// Scheduling priority.
    pub priority: Priority,
    /// Number of attempts so far.
    pub attempt: u32,
}

impl Job {
    /// Whether the job already passed its deadline.
    pub fn is_expired(&self, now: Instant) -> bool {
        now >= self.deadline
    }

    /// Time the job waited in the queue.
    pub fn queue_wait(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.enqueued_at)
    }
}

/// Lifecycle of a job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    /// Waiting in the queue.
    Queued,
    /// Executing against the plugin.
    Running,
    /// Finished successfully.
    Completed,
    /// Finished with an error.
    Failed,
    /// Cancelled by the client or by a revoke.
    Cancelled,
    /// Passed its deadline.
    TimedOut,
    /// Replaced by a newer identical request.
    Superseded,
}

/// Counters of the scheduler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SchedulerStats {
    /// Jobs waiting.
    pub queued: usize,
    /// Jobs running.
    pub running: usize,
    /// Jobs rejected because the queue was full.
    pub rejected: u64,
    /// Jobs cancelled.
    pub cancelled: u64,
    /// Jobs that timed out.
    pub timed_out: u64,
    /// Jobs superseded by a coalesced request.
    pub superseded: u64,
}

/// The scheduler.
#[derive(Debug)]
pub struct Scheduler {
    limits: ResourceLimits,
    metrics: Metrics,
    inner: RwLock<SchedulerInner>,
}

#[derive(Debug, Default)]
struct SchedulerInner {
    queue: Vec<Job>,
    running: Vec<Job>,
    per_client: HashMap<String, usize>,
    export_lanes_in_use: usize,
    rejected: u64,
    cancelled: u64,
    timed_out: u64,
    superseded: u64,
    /// Running jobs that were cancelled. They keep their lane until the plugin
    /// confirms the end of the operation.
    cancelled_running: HashSet<JobId>,
}

impl Scheduler {
    /// Create a scheduler.
    pub fn new(limits: ResourceLimits, metrics: Metrics) -> Self {
        Self {
            limits,
            metrics,
            inner: RwLock::new(SchedulerInner::default()),
        }
    }

    /// Enqueue a job.
    ///
    /// Returns `BUSY` with a `retry_after_ms` hint when the queue is full.
    pub fn submit(&self, job: Job) -> Result<JobId, FigError> {
        let now = Instant::now();
        let mut inner = self.write_inner();
        self.drop_expired(&mut inner, now);

        if inner.queue.len() >= self.limits.max_queue_total {
            inner.rejected += 1;
            Metrics::incr(&self.metrics.busy_rejections);
            return Err(busy_error(format!(
                "the queue holds the maximum of {} jobs",
                self.limits.max_queue_total
            )));
        }
        let queued_for_client = inner.per_client.get(&job.client_id).copied().unwrap_or(0);
        if queued_for_client >= self.limits.max_queue_per_client {
            inner.rejected += 1;
            Metrics::incr(&self.metrics.busy_rejections);
            return Err(busy_error(format!(
                "the queue of client {} holds the maximum of {} jobs",
                job.client_id, self.limits.max_queue_per_client
            )));
        }

        let job_id = job.job_id;
        *inner.per_client.entry(job.client_id.clone()).or_insert(0) += 1;
        inner.queue.push(job);
        self.metrics.set_queue_depth(inner.queue.len() as u64);
        Ok(job_id)
    }

    /// Take the next job that may run right now.
    ///
    /// Control work wins over reads, reads win over exports, and aging keeps a
    /// large job from starving forever.
    pub fn next_ready(&self) -> Option<Job> {
        let now = Instant::now();
        let mut inner = self.write_inner();
        self.drop_expired(&mut inner, now);

        let mut running_per_session: HashMap<SessionId, usize> = HashMap::new();
        let mut export_lanes_in_use = 0usize;
        for job in &inner.running {
            if job.kind.is_export() {
                export_lanes_in_use += 1;
            }
            if let Some(session_id) = job.session_id {
                *running_per_session.entry(session_id).or_insert(0) += 1;
            }
        }
        inner.export_lanes_in_use = export_lanes_in_use;

        let mut best: Option<(usize, usize)> = None;
        for (index, job) in inner.queue.iter().enumerate() {
            if !self.may_run(job, &running_per_session, export_lanes_in_use) {
                continue;
            }
            let rank = effective_rank(job, now);
            match best {
                None => best = Some((rank, index)),
                Some((best_rank, best_index)) => {
                    if rank < best_rank {
                        best = Some((rank, index));
                    } else if rank == best_rank {
                        // The oldest job of a rank goes first, so an aged job
                        // wins against a fresh one of the same rank.
                        if job.enqueued_at < inner.queue[best_index].enqueued_at {
                            best = Some((rank, index));
                        }
                    }
                }
            }
        }

        let (_, index) = best?;
        let job = inner.queue.remove(index);
        decrement_client(&mut inner, &job.client_id);
        // The job leaves the queue and enters the running set: it is in flight
        // from now on and holds its lane until the daemon completes it.
        inner.running.push(job.clone());
        self.metrics.set_queue_depth(inner.queue.len() as u64);
        Some(job)
    }

    /// Mark a job as running.
    ///
    /// A job that `next_ready` handed out is already running, so the call
    /// only confirms it. A job that is still queued is moved into the running
    /// set.
    pub fn mark_running(&self, job_id: &JobId) -> bool {
        let mut inner = self.write_inner();
        if inner.running.iter().any(|job| job.job_id == *job_id) {
            return true;
        }
        let Some(index) = inner.queue.iter().position(|job| job.job_id == *job_id) else {
            return false;
        };
        let job = inner.queue.remove(index);
        decrement_client(&mut inner, &job.client_id);
        inner.running.push(job);
        self.metrics.set_queue_depth(inner.queue.len() as u64);
        true
    }

    /// Mark a job as finished.
    pub fn complete(&self, job_id: &JobId, status: JobStatus) {
        let mut inner = self.write_inner();
        let was_cancelled = inner.cancelled_running.remove(job_id);
        if let Some(index) = inner.running.iter().position(|job| job.job_id == *job_id) {
            inner.running.remove(index);
        } else if let Some(index) = inner.queue.iter().position(|job| job.job_id == *job_id) {
            let job = inner.queue.remove(index);
            decrement_client(&mut inner, &job.client_id);
        } else {
            return;
        }
        match status {
            // A job that was cancelled before is counted once, not twice.
            JobStatus::Cancelled if !was_cancelled => {
                inner.cancelled += 1;
                Metrics::incr(&self.metrics.cancellations);
            }
            JobStatus::TimedOut => inner.timed_out += 1,
            JobStatus::Superseded => inner.superseded += 1,
            _ => {}
        }
        self.metrics.set_queue_depth(inner.queue.len() as u64);
    }

    /// Cancel a job that is queued or running.
    ///
    /// A queued job is dropped. A running job is marked cancelled and keeps
    /// its lane until the plugin confirms that it stopped.
    pub fn cancel(&self, job_id: &JobId) -> bool {
        let mut inner = self.write_inner();
        if let Some(index) = inner.queue.iter().position(|job| job.job_id == *job_id) {
            let job = inner.queue.remove(index);
            decrement_client(&mut inner, &job.client_id);
            inner.cancelled += 1;
            Metrics::incr(&self.metrics.cancellations);
            self.metrics.set_queue_depth(inner.queue.len() as u64);
            return true;
        }
        if inner.running.iter().any(|job| job.job_id == *job_id) {
            if inner.cancelled_running.insert(*job_id) {
                inner.cancelled += 1;
                Metrics::incr(&self.metrics.cancellations);
            }
            return true;
        }
        false
    }

    /// Cancel every job of a session, used when a grant is revoked.
    pub fn cancel_session(&self, session_id: &SessionId) -> usize {
        let mut inner = self.write_inner();
        let mut cancelled = 0usize;

        let mut keep: Vec<Job> = Vec::with_capacity(inner.queue.len());
        let queued = std::mem::take(&mut inner.queue);
        for job in queued {
            if job.session_id == Some(*session_id) {
                decrement_client(&mut inner, &job.client_id);
                cancelled += 1;
            } else {
                keep.push(job);
            }
        }
        inner.queue = keep;

        let running: Vec<JobId> = inner
            .running
            .iter()
            .filter(|job| job.session_id == Some(*session_id))
            .map(|job| job.job_id)
            .collect();
        for job_id in running {
            if inner.cancelled_running.insert(job_id) {
                cancelled += 1;
            }
        }

        if cancelled > 0 {
            inner.cancelled += cancelled as u64;
            Metrics::add(&self.metrics.cancellations, cancelled as u64);
            self.metrics.set_queue_depth(inner.queue.len() as u64);
        }
        cancelled
    }

    /// Counters of the scheduler.
    pub fn stats(&self) -> SchedulerStats {
        let inner = self.read_inner();
        SchedulerStats {
            queued: inner.queue.len(),
            running: inner.running.len(),
            rejected: inner.rejected,
            cancelled: inner.cancelled,
            timed_out: inner.timed_out,
            superseded: inner.superseded,
        }
    }

    /// The limits this scheduler enforces.
    pub fn limits(&self) -> &ResourceLimits {
        &self.limits
    }

    /// Whether a queued job may start right now.
    fn may_run(
        &self,
        job: &Job,
        running_per_session: &HashMap<SessionId, usize>,
        export_lanes_in_use: usize,
    ) -> bool {
        if let Some(session_id) = job.session_id {
            let used = running_per_session.get(&session_id).copied().unwrap_or(0);
            if used >= self.limits.max_active_ops_per_session {
                return false;
            }
        }
        if job.kind.is_export() && export_lanes_in_use >= self.limits.global_export_lanes {
            return false;
        }
        true
    }

    /// Drop the queued jobs that passed their deadline.
    ///
    /// A job whose deadline lapsed while it waited is never executed; it is
    /// counted as timed out.
    fn drop_expired(&self, inner: &mut SchedulerInner, now: Instant) {
        let expired: Vec<Job> = inner
            .queue
            .iter()
            .filter(|job| job.is_expired(now))
            .cloned()
            .collect();
        if expired.is_empty() {
            return;
        }
        inner.queue.retain(|job| !job.is_expired(now));
        for job in &expired {
            decrement_client(inner, &job.client_id);
        }
        inner.timed_out += expired.len() as u64;
    }

    /// Read the scheduler state.
    fn read_inner(&self) -> std::sync::RwLockReadGuard<'_, SchedulerInner> {
        match self.inner.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Write the scheduler state.
    fn write_inner(&self) -> RwLockWriteGuard<'_, SchedulerInner> {
        match self.inner.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// The rank of a job, where a smaller number means more important.
fn base_rank(priority: Priority) -> usize {
    match priority {
        Priority::Control => 0,
        Priority::SmallRead => 1,
        Priority::Export => 2,
    }
}

/// The rank of a job after aging.
///
/// A job that waited long enough gains priority levels. Control traffic never
/// ages: it preempts instead of queueing.
fn effective_rank(job: &Job, now: Instant) -> usize {
    let base = base_rank(job.priority);
    if base == 0 {
        return 0;
    }
    let wait = job.queue_wait(now);
    let steps = (wait.as_nanos() / AGING_STEP.as_nanos()) as usize;
    base.saturating_sub(steps).max(1)
}

/// Lower the queued count of a client.
fn decrement_client(inner: &mut SchedulerInner, client_id: &str) {
    let mut remove = false;
    if let Some(count) = inner.per_client.get_mut(client_id) {
        *count = count.saturating_sub(1);
        remove = *count == 0;
    }
    if remove {
        inner.per_client.remove(client_id);
    }
}

/// The error of a saturated queue.
fn busy_error(message: String) -> FigError {
    FigError::new(ErrorCode::Busy, message).with_retry_after_ms(BUSY_RETRY_AFTER_MS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scheduler() -> Scheduler {
        Scheduler::new(ResourceLimits::default(), Metrics::new())
    }

    fn job(kind: JobKind, priority: Priority, client: &str, session: Option<SessionId>) -> Job {
        let now = Instant::now();
        Job {
            job_id: JobId::new(),
            kind,
            session_id: session,
            client_id: client.to_string(),
            params: json!({}),
            deadline: now + Duration::from_secs(20),
            enqueued_at: now,
            priority,
            attempt: 0,
        }
    }

    #[test]
    fn a_submitted_job_is_queued() {
        let scheduler = scheduler();
        let job = job(JobKind::Read, Priority::SmallRead, "client-a", None);
        let job_id = scheduler.submit(job).expect("accepted");
        let stats = scheduler.stats();
        assert_eq!(stats.queued, 1);
        assert_eq!(stats.rejected, 0);
        assert_eq!(scheduler.next_ready().map(|job| job.job_id), Some(job_id));
    }

    #[test]
    fn the_total_queue_cap_is_enforced() {
        let limits = ResourceLimits {
            max_queue_total: 2,
            max_queue_per_client: 16,
            ..ResourceLimits::default()
        };
        let scheduler = Scheduler::new(limits, Metrics::new());
        scheduler
            .submit(job(JobKind::Read, Priority::SmallRead, "client-a", None))
            .expect("first");
        scheduler
            .submit(job(JobKind::Read, Priority::SmallRead, "client-b", None))
            .expect("second");
        let error = scheduler
            .submit(job(JobKind::Read, Priority::SmallRead, "client-c", None))
            .expect_err("refused");
        assert_eq!(error.code, ErrorCode::Busy);
        assert_eq!(error.retry_after_ms, Some(250));
        assert!(error.retryable);
        assert_eq!(scheduler.stats().rejected, 1);
    }

    #[test]
    fn the_per_client_queue_cap_is_enforced() {
        let limits = ResourceLimits {
            max_queue_total: 16,
            max_queue_per_client: 2,
            ..ResourceLimits::default()
        };
        let scheduler = Scheduler::new(limits, Metrics::new());
        scheduler
            .submit(job(JobKind::Read, Priority::SmallRead, "client-a", None))
            .expect("first");
        scheduler
            .submit(job(JobKind::Read, Priority::SmallRead, "client-a", None))
            .expect("second");
        let error = scheduler
            .submit(job(JobKind::Read, Priority::SmallRead, "client-a", None))
            .expect_err("refused");
        assert_eq!(error.code, ErrorCode::Busy);
        assert_eq!(error.retry_after_ms, Some(250));
        scheduler
            .submit(job(JobKind::Read, Priority::SmallRead, "client-b", None))
            .expect("another client is admitted");
    }

    #[test]
    fn control_work_wins_over_reads_and_exports() {
        let scheduler = scheduler();
        let export = job(JobKind::Export, Priority::Export, "client-a", None);
        let read = job(JobKind::Read, Priority::SmallRead, "client-a", None);
        let control = job(JobKind::Read, Priority::Control, "client-a", None);
        scheduler.submit(export).expect("accepted");
        scheduler.submit(read).expect("accepted");
        scheduler.submit(control).expect("accepted");
        assert_eq!(
            scheduler.next_ready().map(|job| job.priority),
            Some(Priority::Control)
        );
        assert_eq!(
            scheduler.next_ready().map(|job| job.priority),
            Some(Priority::SmallRead)
        );
        assert_eq!(
            scheduler.next_ready().map(|job| job.priority),
            Some(Priority::Export)
        );
        assert!(scheduler.next_ready().is_none());
    }

    #[test]
    fn one_operation_runs_per_session() {
        let scheduler = scheduler();
        let session = SessionId::new();
        let first = job(
            JobKind::Read,
            Priority::SmallRead,
            "client-a",
            Some(session),
        );
        let second = job(
            JobKind::Read,
            Priority::SmallRead,
            "client-a",
            Some(session),
        );
        let other = job(
            JobKind::Read,
            Priority::SmallRead,
            "client-a",
            Some(SessionId::new()),
        );
        scheduler.submit(first).expect("accepted");
        scheduler.submit(second).expect("accepted");
        scheduler.submit(other).expect("accepted");
        let taken = scheduler.next_ready().expect("a job is ready");
        scheduler.mark_running(&taken.job_id);
        let next = scheduler.next_ready().expect("another job is ready");
        assert_ne!(next.session_id, Some(session), "the session lane is busy");
        assert!(scheduler.next_ready().is_none());
    }

    #[test]
    fn only_one_export_runs_at_a_time() {
        let scheduler = scheduler();
        let first = job(JobKind::Export, Priority::Export, "client-a", None);
        let second = job(JobKind::Export, Priority::Export, "client-b", None);
        scheduler.submit(first).expect("accepted");
        scheduler.submit(second).expect("accepted");
        let taken = scheduler.next_ready().expect("a job is ready");
        assert!(taken.kind.is_export());
        scheduler.mark_running(&taken.job_id);
        assert!(scheduler.next_ready().is_none(), "the export lane is busy");
        scheduler.complete(&taken.job_id, JobStatus::Completed);
        assert!(scheduler.next_ready().is_some(), "the lane is free again");
    }

    #[test]
    fn an_aged_job_is_not_starved_by_fresh_reads() {
        let scheduler = scheduler();
        let mut export = job(JobKind::Export, Priority::Export, "client-a", None);
        export.enqueued_at = Instant::now() - (AGING_STEP * 3);
        let read = job(JobKind::Read, Priority::SmallRead, "client-a", None);
        scheduler.submit(export).expect("accepted");
        scheduler.submit(read).expect("accepted");
        let taken = scheduler.next_ready().expect("a job is ready");
        assert_eq!(
            taken.priority,
            Priority::Export,
            "the aged export goes first"
        );
    }

    #[test]
    fn control_traffic_preempts_an_aged_export() {
        let scheduler = scheduler();
        let mut export = job(JobKind::Export, Priority::Export, "client-a", None);
        export.enqueued_at = Instant::now() - (AGING_STEP * 10);
        let control = job(JobKind::Read, Priority::Control, "client-a", None);
        scheduler.submit(export).expect("accepted");
        scheduler.submit(control).expect("accepted");
        let taken = scheduler.next_ready().expect("a job is ready");
        assert_eq!(taken.priority, Priority::Control);
    }

    #[test]
    fn expired_jobs_are_dropped_and_counted_as_timed_out() {
        let scheduler = scheduler();
        let mut expired = job(JobKind::Read, Priority::SmallRead, "client-a", None);
        expired.deadline = Instant::now() - Duration::from_secs(1);
        scheduler.submit(expired).expect("accepted");
        assert!(scheduler.next_ready().is_none());
        let stats = scheduler.stats();
        assert_eq!(stats.queued, 0);
        assert_eq!(stats.timed_out, 1);
    }

    #[test]
    fn an_expired_job_never_runs() {
        let scheduler = scheduler();
        let mut expired = job(JobKind::Read, Priority::SmallRead, "client-a", None);
        expired.deadline = Instant::now() - Duration::from_secs(1);
        scheduler.submit(expired).expect("accepted");
        assert!(scheduler.next_ready().is_none());
        assert_eq!(scheduler.stats().running, 0);
    }

    #[test]
    fn cancel_removes_a_queued_job() {
        let scheduler = scheduler();
        let job = job(JobKind::Read, Priority::SmallRead, "client-a", None);
        let job_id = scheduler.submit(job).expect("accepted");
        assert!(scheduler.cancel(&job_id));
        assert_eq!(scheduler.stats().queued, 0);
        assert_eq!(scheduler.stats().cancelled, 1);
        assert!(!scheduler.cancel(&job_id), "the job is gone");
    }

    #[test]
    fn cancel_marks_a_running_job_and_keeps_its_lane() {
        let scheduler = scheduler();
        let job = job(JobKind::Export, Priority::Export, "client-a", None);
        let job_id = scheduler.submit(job).expect("accepted");
        let taken = scheduler.next_ready().expect("a job is ready");
        scheduler.mark_running(&taken.job_id);
        assert!(scheduler.cancel(&job_id));
        assert_eq!(scheduler.stats().running, 1);
        assert_eq!(scheduler.stats().cancelled, 1);
        assert!(
            scheduler.next_ready().is_none(),
            "a cancelled export still holds the lane"
        );
        scheduler.complete(&job_id, JobStatus::Cancelled);
        assert_eq!(scheduler.stats().running, 0);
        assert_eq!(scheduler.stats().cancelled, 1);
    }

    #[test]
    fn cancel_session_drops_the_queued_and_running_jobs() {
        let scheduler = scheduler();
        let session = SessionId::new();
        let queued = job(
            JobKind::Read,
            Priority::SmallRead,
            "client-a",
            Some(session),
        );
        let running = job(
            JobKind::Read,
            Priority::SmallRead,
            "client-a",
            Some(session),
        );
        let foreign = job(
            JobKind::Read,
            Priority::SmallRead,
            "client-b",
            Some(SessionId::new()),
        );
        scheduler.submit(queued).expect("accepted");
        scheduler.submit(running).expect("accepted");
        scheduler.submit(foreign).expect("accepted");
        let taken = scheduler.next_ready().expect("a job is ready");
        scheduler.mark_running(&taken.job_id);
        let cancelled = scheduler.cancel_session(&session);
        assert_eq!(cancelled, 2);
        assert_eq!(scheduler.stats().queued, 1);
        assert_eq!(scheduler.stats().running, 1);
    }

    #[test]
    fn the_queue_depth_is_reported() {
        let scheduler = scheduler();
        scheduler
            .submit(job(JobKind::Read, Priority::SmallRead, "client-a", None))
            .expect("accepted");
        scheduler
            .submit(job(JobKind::Read, Priority::SmallRead, "client-a", None))
            .expect("accepted");
        assert_eq!(scheduler.stats().queued, 2);
        let taken = scheduler.next_ready().expect("a job is ready");
        scheduler.mark_running(&taken.job_id);
        let stats = scheduler.stats();
        assert_eq!(stats.queued, 1);
        assert_eq!(stats.running, 1);
    }

    #[test]
    fn a_finished_job_is_counted_by_status() {
        let scheduler = scheduler();
        let timed_out = job(JobKind::Read, Priority::SmallRead, "client-a", None);
        let timed_out_id = scheduler.submit(timed_out).expect("accepted");
        let taken = scheduler.next_ready().expect("a job is ready");
        scheduler.mark_running(&taken.job_id);
        scheduler.complete(&timed_out_id, JobStatus::TimedOut);
        assert_eq!(scheduler.stats().timed_out, 1);

        let superseded = job(JobKind::Read, Priority::SmallRead, "client-a", None);
        let superseded_id = scheduler.submit(superseded).expect("accepted");
        let taken = scheduler.next_ready().expect("a job is ready");
        scheduler.mark_running(&taken.job_id);
        scheduler.complete(&superseded_id, JobStatus::Superseded);
        assert_eq!(scheduler.stats().superseded, 1);
        assert_eq!(scheduler.stats().running, 0);
    }

    #[test]
    fn mark_running_reports_an_unknown_job() {
        let scheduler = scheduler();
        assert!(!scheduler.mark_running(&JobId::new()));
    }

    #[test]
    fn the_limits_are_exposed() {
        let scheduler = scheduler();
        assert_eq!(scheduler.limits().max_queue_total, 64);
    }
}
