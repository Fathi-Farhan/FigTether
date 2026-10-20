//! FigTether contract model.
//!
//! This crate is the single source of truth for every value that crosses a
//! process boundary: the MCP response envelope, the normalized design model,
//! tool parameter shapes, the capability model, and the error taxonomy.
//!
//! Design rules (see `docs/04-data-contracts/`):
//! - every payload carries source, freshness, coverage, and consistency;
//! - unknown data is reported as unavailable with a reason, never as a silent
//!   `null` and never silently dropped;
//! - identifiers are opaque strings on the wire and typed newtypes in Rust;
//! - the schema version is explicit and travels with every response.

pub mod capabilities;
pub mod envelope;
pub mod errors;
pub mod ids;
pub mod normalized;
pub mod tools;

pub use capabilities::{Capabilities, CapabilityState, ProbeOutcome};
pub use envelope::{
    canonical_hash, Consistency, Coverage, Envelope, Freshness, Observation, SourceKind, SourceRef,
    Warning,
};
pub use errors::{ErrorCode, FigError};
pub use ids::{AssetId, GrantId, JobId, ObservationId, SessionId, SnapshotId};

/// Schema version of the FigTether response envelope and tool contracts.
///
/// Bumped when a breaking change lands in an envelope or tool shape. Unknown
/// optional fields are tolerated by readers (forward compatibility), unknown
/// major versions are rejected with `ErrorCode::UnsupportedSchema`.
pub const SCHEMA_VERSION: &str = "1.0";
