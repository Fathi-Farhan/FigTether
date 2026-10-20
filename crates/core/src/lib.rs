//! FigTether core domain.
//!
//! This crate holds the rules of the product: what a session is, what a grant
//! allows, how work is scheduled against the editor, how results are cached,
//! and how the plugin payload becomes the normalized model.
//!
//! Two invariants shape the whole design:
//!
//! 1. The core never talks to Figma. It talks to the plugin through the
//!    bridge protocol and to agents through the MCP adapter. There is no REST
//!    client and no official MCP client in this crate (`FR19`).
//! 2. Every resource is bounded. Queues, caches, transfers, response sizes,
//!    traversal depth, and export sizes all have explicit limits, and exceeding
//!    a limit is a structured error, never a silent truncation
//!    (`docs/04-data-contracts/06-resource-limits.md`).
//!
//! The write path follows the same rules: a plan is budgeted, approved by
//! the user, and audited without any design content
//! (`docs/11-write-path/01-write-path-design.md`).

pub mod assets;
pub mod cache;
pub mod limits;
pub mod metrics;
pub mod normalize;
pub mod registry;
pub mod scheduler;
pub mod scope;
pub mod state;
pub mod write;

pub use assets::{AssetEntry, AssetStore, InMemoryAssetStore};
pub use cache::{Cache, CacheEntry, CacheKey, CacheStats, SourceKey};
pub use limits::ResourceLimits;
pub use metrics::{Metrics, MetricsSnapshot};
pub use normalize::normalize_document;
pub use registry::{Grant, GrantMode, GrantState, Session, SessionRegistry, SessionSnapshot};
pub use scheduler::{Job, JobKind, JobStatus, Priority, Scheduler, SchedulerStats};
pub use scope::NodeIndex;
pub use state::{SessionEvent, SessionState, Transition};
pub use write::{
    ApprovalRecord, ApprovalRegistry, AuditRecord, PlanCounts, WriteAudit, WritePolicy,
};

/// Default TCP port of the local bridge.
///
/// The daemon binds the loopback interface only. A port collision is reported
/// to the user with an explicit `--port` hint; the port is never changed
/// silently, because the plugin manifest references it.
pub const DEFAULT_BRIDGE_PORT: u16 = 38947;

/// Version of the FigTether core, reported by `figtether doctor`.
pub const CORE_VERSION: &str = env!("CARGO_PKG_VERSION");
