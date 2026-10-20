//! Resource budgets.
//!
//! The defaults below come from PRD section 8.5.5 and are the contract the
//! product is measured against. They are deliberately conservative: an
//! installation may lower any limit, but raising one marks the installation as
//! `custom` and `unvalidated`, because the published benchmarks only cover the
//! default profile.

use std::time::Duration;

/// Default resource budgets.
#[derive(Debug, Clone, PartialEq)]
pub struct ResourceLimits {
    /// Live sessions a single daemon serves.
    pub max_sessions: usize,
    /// Editor operations that may run at the same time inside one session.
    pub max_active_ops_per_session: usize,
    /// Asset exports that may run at the same time across all sessions.
    pub global_export_lanes: usize,
    /// Requests waiting in the scheduler.
    pub max_queue_total: usize,
    /// Requests waiting per client.
    pub max_queue_per_client: usize,
    /// Node ids accepted by one batch request.
    pub max_batch_node_ids: usize,
    /// Maximum traversal depth of one request.
    pub max_depth: u32,
    /// Maximum number of nodes one operation may visit.
    pub max_traversal_nodes: usize,
    /// Maximum size of one encoded MCP response.
    pub max_json_response_bytes: usize,
    /// Maximum size of one inline image.
    pub max_inline_image_bytes: u64,
    /// Maximum size of one encoded MCP response including inline images.
    pub max_encoded_response_bytes: u64,
    /// Maximum size of one bridge frame.
    pub max_frame_bytes: usize,
    /// Maximum size of one chunked transfer.
    pub max_transfer_bytes: u64,
    /// Total byte budget of the design cache and the asset store.
    pub max_cache_bytes: u64,
    /// Maximum size of a single cached asset.
    pub max_asset_bytes: u64,
    /// Maximum pixel count of one export.
    pub max_export_pixels: u64,
    /// Smallest accepted export scale.
    pub min_export_scale: f64,
    /// Largest accepted export scale.
    pub max_export_scale: f64,
    /// Deadline of a read, queue wait included.
    pub read_timeout: Duration,
    /// Deadline of an export, queue wait included.
    pub export_timeout: Duration,
    /// Target duration of one traversal slice before yielding to the editor.
    pub traversal_slice: Duration,
    /// Number of retries allowed for an idempotent fresh read.
    pub max_fresh_read_retries: u32,
    /// Time to live of a live cache entry. Only used for explicit cached reads.
    pub live_cache_ttl: Duration,
    /// Idle time after which a snapshot is dropped.
    pub snapshot_idle_ttl: Duration,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_sessions: 4,
            max_active_ops_per_session: 1,
            global_export_lanes: 1,
            max_queue_total: 64,
            max_queue_per_client: 16,
            max_batch_node_ids: 50,
            max_depth: 32,
            max_traversal_nodes: 10_000,
            max_json_response_bytes: 64 * 1024,
            max_inline_image_bytes: 1024 * 1024,
            max_encoded_response_bytes: 2 * 1024 * 1024,
            max_frame_bytes: 1024 * 1024,
            max_transfer_bytes: 32 * 1024 * 1024,
            max_cache_bytes: 128 * 1024 * 1024,
            max_asset_bytes: 16 * 1024 * 1024,
            max_export_pixels: 16_000_000,
            min_export_scale: 0.25,
            max_export_scale: 2.0,
            read_timeout: Duration::from_secs(20),
            export_timeout: Duration::from_secs(60),
            traversal_slice: Duration::from_millis(8),
            max_fresh_read_retries: 1,
            live_cache_ttl: Duration::from_secs(2),
            snapshot_idle_ttl: Duration::from_secs(30 * 60),
        }
    }
}

impl ResourceLimits {
    /// A copy with every limit lowered to the given fraction of its default.
    ///
    /// Used by the `custom` profile so that a user can shrink the footprint
    /// without ever exceeding the tested profile.
    pub fn lowered(&self, factor: f64) -> Self {
        let f = factor.clamp(0.0, 1.0);
        let mut limits = self.clone();
        limits.max_sessions = ((self.max_sessions as f64) * f).floor().max(1.0) as usize;
        limits.max_queue_total = ((self.max_queue_total as f64) * f).floor().max(1.0) as usize;
        limits.max_queue_per_client =
            ((self.max_queue_per_client as f64) * f).floor().max(1.0) as usize;
        limits.max_batch_node_ids =
            ((self.max_batch_node_ids as f64) * f).floor().max(1.0) as usize;
        limits.max_depth = ((self.max_depth as f64) * f).floor().max(1.0) as u32;
        limits.max_traversal_nodes =
            ((self.max_traversal_nodes as f64) * f).floor().max(1.0) as usize;
        limits.max_cache_bytes = ((self.max_cache_bytes as f64) * f) as u64;
        limits.max_asset_bytes = ((self.max_asset_bytes as f64) * f) as u64;
        limits
    }

    /// Whether the scale of an export is inside the accepted range.
    pub fn scale_is_valid(&self, scale: f64) -> bool {
        scale >= self.min_export_scale && scale <= self.max_export_scale
    }

    /// Whether a pixel count is inside the accepted range.
    pub fn pixels_are_valid(&self, width: u32, height: u32) -> bool {
        (width as u64).saturating_mul(height as u64) <= self.max_export_pixels
    }

    /// Whether a batch of node ids is inside the accepted size.
    pub fn batch_is_valid(&self, count: usize) -> bool {
        count <= self.max_batch_node_ids
    }

    /// Whether a depth is inside the accepted range.
    pub fn depth_is_valid(&self, depth: u32) -> bool {
        depth <= self.max_depth
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_documented_profile() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.max_sessions, 4);
        assert_eq!(limits.max_queue_total, 64);
        assert_eq!(limits.max_queue_per_client, 16);
        assert_eq!(limits.max_batch_node_ids, 50);
        assert_eq!(limits.max_depth, 32);
        assert_eq!(limits.max_traversal_nodes, 10_000);
        assert_eq!(limits.max_json_response_bytes, 64 * 1024);
        assert_eq!(limits.max_cache_bytes, 128 * 1024 * 1024);
        assert_eq!(limits.max_export_pixels, 16_000_000);
        assert_eq!(limits.read_timeout, Duration::from_secs(20));
        assert_eq!(limits.export_timeout, Duration::from_secs(60));
    }

    #[test]
    fn lowering_never_raises_a_limit() {
        let default = ResourceLimits::default();
        let lowered = default.lowered(0.5);
        assert!(lowered.max_queue_total <= default.max_queue_total);
        assert!(lowered.max_cache_bytes <= default.max_cache_bytes);
        assert!(lowered.max_sessions >= 1);
    }

    #[test]
    fn scale_and_pixels_are_validated() {
        let limits = ResourceLimits::default();
        assert!(limits.scale_is_valid(1.0));
        assert!(!limits.scale_is_valid(0.1));
        assert!(!limits.scale_is_valid(4.0));
        assert!(limits.pixels_are_valid(1024, 1024));
        assert!(!limits.pixels_are_valid(8000, 8000));
    }
}
