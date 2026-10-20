//! Response envelope.
//!
//! Every FigTether answer is wrapped in the same envelope so an agent can
//! always answer three questions without guessing: where did this data come
//! from, how fresh is it, and how much of the request did it actually cover.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ids::{ObservationId, SessionId, SnapshotId};
use crate::SCHEMA_VERSION;

/// Where a payload originated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// A live read that reached the running plugin.
    PluginLive,
    /// An immutable capture taken earlier.
    PluginSnapshot,
    /// A bundle imported from disk, never a live session.
    OfflineBundle,
}

impl SourceKind {
    /// Stable wire string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PluginLive => "plugin_live",
            Self::PluginSnapshot => "plugin_snapshot",
            Self::OfflineBundle => "offline_bundle",
        }
    }
}

/// Provenance of one result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SourceRef {
    /// Origin of the data.
    pub kind: SourceKind,
    /// Live session the data came from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    /// Capture the data came from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<SnapshotId>,
    /// Authorization epoch in force when the data was read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant_epoch: Option<u64>,
    /// Connection epoch in force when the data was read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_epoch: Option<u64>,
}

impl SourceRef {
    /// A live source bound to a session.
    pub fn plugin_live(session_id: SessionId) -> Self {
        Self {
            kind: SourceKind::PluginLive,
            session_id: Some(session_id),
            snapshot_id: None,
            grant_epoch: None,
            connection_epoch: None,
        }
    }

    /// A snapshot source.
    pub fn plugin_snapshot(snapshot_id: SnapshotId) -> Self {
        Self {
            kind: SourceKind::PluginSnapshot,
            session_id: None,
            snapshot_id: Some(snapshot_id),
            grant_epoch: None,
            connection_epoch: None,
        }
    }

    /// An offline bundle source.
    pub fn offline_bundle(snapshot_id: SnapshotId) -> Self {
        Self {
            kind: SourceKind::OfflineBundle,
            session_id: None,
            snapshot_id: Some(snapshot_id),
            grant_epoch: None,
            connection_epoch: None,
        }
    }
}

/// How consistent the observed data is with the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Consistency {
    /// The request reached the plugin and observed the document directly.
    Fresh,
    /// Served from the cache under an explicit age bound.
    Cached,
    /// Best effort: the document may have changed during the observation.
    BestEffort,
}

impl Consistency {
    /// Stable wire string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::Cached => "cached",
            Self::BestEffort => "best_effort",
        }
    }
}

/// Which read lane produced the payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Freshness {
    /// The request was executed against the editor.
    FreshRead,
    /// The answer came from the local cache.
    CacheHit,
}

impl Freshness {
    /// Stable wire string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FreshRead => "fresh_read",
            Self::CacheHit => "cache_hit",
        }
    }
}

/// The observation window of one read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Observation {
    /// Identity of this observation.
    pub observation_id: ObservationId,
    /// When the read started.
    pub started_at: DateTime<Utc>,
    /// When the data was observed.
    pub observed_at: DateTime<Utc>,
    /// Consistency of the observation.
    pub consistency: Consistency,
    /// Which lane produced the payload.
    pub freshness: Freshness,
    /// Age of the cached payload in milliseconds, when the lane was a cache hit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_age_ms: Option<u64>,
    /// Hash of the canonical payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

/// How much of the request the payload actually covers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Coverage {
    /// Roots the read was scoped to.
    #[serde(default)]
    pub root_ids: Vec<String>,
    /// Number of nodes returned.
    #[serde(default)]
    pub returned_nodes: u32,
    /// Whether the result was cut short.
    #[serde(default)]
    pub truncated: bool,
    /// Cursor that continues a truncated traversal.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    /// Fields that could not be provided, with the reason appended.
    #[serde(default)]
    pub unavailable_fields: Vec<String>,
}

/// A non-fatal caveat attached to a result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Warning {
    /// Stable warning code.
    pub code: String,
    /// Human readable explanation.
    pub message: String,
}

impl Warning {
    /// Build a warning.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

/// The common response envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Envelope {
    /// Contract version of this envelope.
    pub schema_version: String,
    /// Correlation id of the request.
    pub request_id: String,
    /// Provenance of the payload.
    pub source: SourceRef,
    /// Observation window.
    pub observation: Observation,
    /// Coverage of the request.
    pub coverage: Coverage,
    /// The payload itself.
    pub data: serde_json::Value,
    /// Non-fatal caveats.
    #[serde(default)]
    pub warnings: Vec<Warning>,
}

impl Envelope {
    /// Build an envelope with the current schema version.
    pub fn new(
        request_id: impl Into<String>,
        source: SourceRef,
        observation: Observation,
        coverage: Coverage,
        data: serde_json::Value,
    ) -> Self {
        Self {
            schema_version: SCHEMA_VERSION.to_string(),
            request_id: request_id.into(),
            source,
            observation,
            coverage,
            data,
            warnings: Vec::new(),
        }
    }

    /// Attach a warning.
    pub fn with_warning(mut self, warning: Warning) -> Self {
        self.warnings.push(warning);
        self
    }

    /// Compute and attach the canonical content hash of `data`.
    pub fn with_content_hash(mut self) -> Self {
        self.observation.content_hash = Some(canonical_hash(&self.data));
        self
    }

    /// Whether the payload claims to be a live read.
    pub fn is_live(&self) -> bool {
        self.source.kind == SourceKind::PluginLive
    }
}

/// Hash the canonical form of a payload.
///
/// The canonical form is the compact JSON encoding with object keys in
/// lexicographic order and no insignificant whitespace. The hash proves that
/// two payloads are byte identical; it is not evidence that Figma stored that
/// version of the document.
pub fn canonical_hash(value: &serde_json::Value) -> String {
    let canonical = serde_json::to_vec(value).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(&canonical);
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn observation() -> Observation {
        let now = Utc::now();
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

    #[test]
    fn envelope_defaults_to_current_schema_version() {
        let env = Envelope::new(
            "req_01",
            SourceRef::plugin_live(SessionId::new()),
            observation(),
            Coverage::default(),
            json!({}),
        );
        assert_eq!(env.schema_version, "1.0");
        assert!(env.is_live());
    }

    #[test]
    fn canonical_hash_is_key_order_independent() {
        let a = json!({"b": 1, "a": 2});
        let b = json!({"a": 2, "b": 1});
        assert_eq!(canonical_hash(&a), canonical_hash(&b));
        assert!(canonical_hash(&a).starts_with("sha256:"));
    }

    #[test]
    fn content_hash_is_attached_to_observation() {
        let env = Envelope::new(
            "req_02",
            SourceRef::plugin_live(SessionId::new()),
            observation(),
            Coverage::default(),
            json!({"nodes": []}),
        )
        .with_content_hash();
        assert!(env.observation.content_hash.is_some());
    }

    #[test]
    fn snapshot_source_is_not_live() {
        let env = Envelope::new(
            "req_03",
            SourceRef::plugin_snapshot(SnapshotId::new()),
            observation(),
            Coverage::default(),
            json!({}),
        );
        assert!(!env.is_live());
    }
}
