//! Local metrics.
//!
//! Metrics are counters kept in memory. They never contain design content:
//! only operation kinds, counts, sizes, and durations. `figtether doctor` and
//! `figtether status` print a sanitized snapshot of them.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Counters of the running daemon.
#[derive(Debug, Default)]
pub struct Metrics {
    /// Requests accepted, by lane.
    pub requests_total: AtomicU64,
    /// Requests served from the cache.
    pub cache_hits: AtomicU64,
    /// Requests that reached the plugin.
    pub fresh_reads: AtomicU64,
    /// Snapshot reads served.
    pub snapshot_reads: AtomicU64,
    /// Asset exports performed.
    pub exports: AtomicU64,
    /// Requests rejected because the queue was full.
    pub busy_rejections: AtomicU64,
    /// Requests cancelled by the client or by a revoke.
    pub cancellations: AtomicU64,
    /// Grants revoked.
    pub revocations: AtomicU64,
    /// Bytes served to clients.
    pub bytes_out: AtomicU64,
    /// Bytes received from the plugin.
    pub bytes_in: AtomicU64,
    /// Total observed latency in milliseconds.
    pub latency_ms_total: AtomicU64,
    /// Number of latency samples.
    pub latency_samples: AtomicU64,
    /// Current queue depth.
    pub queue_depth: AtomicU64,
    /// Peak queue depth.
    pub queue_depth_peak: AtomicU64,
}

impl Metrics {
    /// Create empty counters.
    pub fn new() -> Self {
        Self::default()
    }

    /// Increment a counter.
    pub fn incr(counter: &AtomicU64) {
        counter.fetch_add(1, Ordering::Relaxed);
    }

    /// Add to a counter.
    pub fn add(counter: &AtomicU64, value: u64) {
        counter.fetch_add(value, Ordering::Relaxed);
    }

    /// Record one request outcome.
    pub fn record_request(&self, lane: Lane) {
        Self::incr(&self.requests_total);
        match lane {
            Lane::CacheHit => Self::incr(&self.cache_hits),
            Lane::FreshRead => Self::incr(&self.fresh_reads),
            Lane::Snapshot => Self::incr(&self.snapshot_reads),
            Lane::Export => Self::incr(&self.exports),
        };
    }

    /// Record an observed latency.
    pub fn record_latency(&self, latency: Duration) {
        Self::add(&self.latency_ms_total, latency.as_millis() as u64);
        Self::incr(&self.latency_samples);
    }

    /// Set the current queue depth, keeping the peak in sync.
    pub fn set_queue_depth(&self, depth: u64) {
        self.queue_depth.store(depth, Ordering::Relaxed);
        self.queue_depth_peak.fetch_max(depth, Ordering::Relaxed);
    }

    /// Read all counters.
    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            requests_total: self.requests_total.load(Ordering::Relaxed),
            cache_hits: self.cache_hits.load(Ordering::Relaxed),
            fresh_reads: self.fresh_reads.load(Ordering::Relaxed),
            snapshot_reads: self.snapshot_reads.load(Ordering::Relaxed),
            exports: self.exports.load(Ordering::Relaxed),
            busy_rejections: self.busy_rejections.load(Ordering::Relaxed),
            cancellations: self.cancellations.load(Ordering::Relaxed),
            revocations: self.revocations.load(Ordering::Relaxed),
            bytes_out: self.bytes_out.load(Ordering::Relaxed),
            bytes_in: self.bytes_in.load(Ordering::Relaxed),
            latency_ms_total: self.latency_ms_total.load(Ordering::Relaxed),
            latency_samples: self.latency_samples.load(Ordering::Relaxed),
            queue_depth: self.queue_depth.load(Ordering::Relaxed),
            queue_depth_peak: self.queue_depth_peak.load(Ordering::Relaxed),
        }
    }
}

/// Which lane served a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lane {
    /// Served from the cache.
    CacheHit,
    /// Reached the plugin.
    FreshRead,
    /// Served from a snapshot.
    Snapshot,
    /// Asset export.
    Export,
}

/// A point in time view of the counters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricsSnapshot {
    /// Requests accepted.
    pub requests_total: u64,
    /// Cache hits.
    pub cache_hits: u64,
    /// Fresh reads.
    pub fresh_reads: u64,
    /// Snapshot reads.
    pub snapshot_reads: u64,
    /// Exports.
    pub exports: u64,
    /// Requests rejected as busy.
    pub busy_rejections: u64,
    /// Cancellations.
    pub cancellations: u64,
    /// Revocations.
    pub revocations: u64,
    /// Bytes served.
    pub bytes_out: u64,
    /// Bytes received.
    pub bytes_in: u64,
    /// Total latency.
    pub latency_ms_total: u64,
    /// Latency samples.
    pub latency_samples: u64,
    /// Current queue depth.
    pub queue_depth: u64,
    /// Peak queue depth.
    pub queue_depth_peak: u64,
}

impl MetricsSnapshot {
    /// Mean latency in milliseconds.
    pub fn mean_latency_ms(&self) -> f64 {
        if self.latency_samples == 0 {
            0.0
        } else {
            self.latency_ms_total as f64 / self.latency_samples as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lanes_are_counted_separately() {
        let metrics = Metrics::new();
        metrics.record_request(Lane::FreshRead);
        metrics.record_request(Lane::CacheHit);
        metrics.record_request(Lane::Export);
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.requests_total, 3);
        assert_eq!(snapshot.fresh_reads, 1);
        assert_eq!(snapshot.cache_hits, 1);
        assert_eq!(snapshot.exports, 1);
    }

    #[test]
    fn queue_peak_is_kept() {
        let metrics = Metrics::new();
        metrics.set_queue_depth(4);
        metrics.set_queue_depth(2);
        assert_eq!(metrics.snapshot().queue_depth, 2);
        assert_eq!(metrics.snapshot().queue_depth_peak, 4);
    }

    #[test]
    fn mean_latency_is_zero_without_samples() {
        assert_eq!(
            MetricsSnapshot {
                requests_total: 0,
                cache_hits: 0,
                fresh_reads: 0,
                snapshot_reads: 0,
                exports: 0,
                busy_rejections: 0,
                cancellations: 0,
                revocations: 0,
                bytes_out: 0,
                bytes_in: 0,
                latency_ms_total: 0,
                latency_samples: 0,
                queue_depth: 0,
                queue_depth_peak: 0,
            }
            .mean_latency_ms(),
            0.0
        );
    }
}
