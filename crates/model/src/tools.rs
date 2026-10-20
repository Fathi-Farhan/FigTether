//! Tool parameter shapes.
//!
//! These structs are the input contracts of the twelve read tools. They are
//! the single source of truth for the JSON Schema published to MCP clients:
//! the schema is generated from these types, never written twice.
//!
//! Rules that hold for every tool:
//! - a data operation names either `session_id` or `snapshot_id`, never both;
//! - identifiers are strings, never parsed numbers;
//! - no parameter is a secret, a credential, or a bridge endpoint;
//! - the agent can never widen its own grant through a parameter.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ids::{JobId, SessionId, SnapshotId};

/// Which consistency lane a read may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConsistencyRequest {
    /// The default: the request must reach the plugin.
    #[default]
    Fresh,
    /// Serve from the cache when the entry is younger than `max_age_ms`.
    Cached,
}

/// How much detail an inspection returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DetailLevel {
    /// Structure and layout only.
    #[default]
    Summary,
    /// Layout, typography, and paints.
    Layout,
    /// Everything the API exposes, within the response budget.
    Full,
}

/// Kinds of design tokens to return.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TokenKind {
    /// Variables.
    Variables,
    /// Styles.
    Styles,
    /// Both.
    All,
}

/// Export format of an asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AssetFormat {
    /// Portable Network Graphics.
    Png,
    /// Scalable Vector Graphics.
    Svg,
}

/// Parameters of `get_bridge_status`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GetBridgeStatusParams {
    /// Reserved for future use; no parameter is accepted today.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verbose: Option<bool>,
}

/// Parameters of `list_sessions`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListSessionsParams {
    /// Cursor for the next page of sessions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// Parameters of `get_selection`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GetSelectionParams {
    /// Live session to read.
    pub session_id: SessionId,
}

/// Parameters of `list_nodes`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListNodesParams {
    /// Live session to read. Mutually exclusive with `snapshot_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    /// Snapshot to read. Mutually exclusive with `session_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<SnapshotId>,
    /// Root node to list from.
    pub root_id: String,
    /// Cursor returned by a previous truncated traversal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    /// Maximum depth to traverse.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,
    /// Fields to project onto the response.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
    /// Maximum number of nodes to return.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Consistency lane.
    #[serde(default)]
    pub consistency: ConsistencyRequest,
    /// Maximum age of a cached answer, in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_age_ms: Option<u64>,
}

/// Parameters of `get_nodes`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GetNodesParams {
    /// Live session to read. Mutually exclusive with `snapshot_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    /// Snapshot to read. Mutually exclusive with `session_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<SnapshotId>,
    /// Node ids to read.
    pub node_ids: Vec<String>,
    /// Maximum depth to traverse below each node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<u32>,
    /// Fields to project onto the response.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
    /// Consistency lane.
    #[serde(default)]
    pub consistency: ConsistencyRequest,
    /// Maximum age of a cached answer, in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_age_ms: Option<u64>,
}

/// Parameters of `inspect_design`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InspectDesignParams {
    /// Live session to read. Mutually exclusive with `snapshot_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    /// Snapshot to read. Mutually exclusive with `session_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<SnapshotId>,
    /// Root node to inspect.
    pub root_id: String,
    /// How much detail to return.
    #[serde(default)]
    pub detail: DetailLevel,
    /// Consistency lane.
    #[serde(default)]
    pub consistency: ConsistencyRequest,
    /// Maximum age of a cached answer, in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_age_ms: Option<u64>,
}

/// Parameters of `get_design_tokens`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GetDesignTokensParams {
    /// Live session to read. Mutually exclusive with `snapshot_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    /// Snapshot to read. Mutually exclusive with `session_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<SnapshotId>,
    /// Root node whose scope bounds the token search.
    pub root_id: String,
    /// Which token kinds to return.
    #[serde(default)]
    pub kinds: Vec<TokenKind>,
    /// Consistency lane.
    #[serde(default)]
    pub consistency: ConsistencyRequest,
    /// Maximum age of a cached answer, in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_age_ms: Option<u64>,
}

/// Parameters of `export_assets`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ExportAssetsParams {
    /// Live session to read. Mutually exclusive with `snapshot_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    /// Snapshot to read. Mutually exclusive with `session_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<SnapshotId>,
    /// Nodes to export.
    pub node_ids: Vec<String>,
    /// Output format.
    pub format: AssetFormat,
    /// Export scale factor, between 0.25 and 2.0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
}

/// Parameters of `get_snapshot_info`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GetSnapshotInfoParams {
    /// Snapshot to describe.
    pub snapshot_id: SnapshotId,
}

/// Parameters of `watch_scope`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WatchScopeParams {
    /// Live session to watch.
    pub session_id: SessionId,
    /// Root node that bounds the watch.
    pub root_id: String,
}

/// Parameters of `get_changes`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GetChangesParams {
    /// Live session to watch.
    pub session_id: SessionId,
    /// Watch created by `watch_scope`.
    pub watch_id: String,
    /// Cursor returned by a previous call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// Parameters of `get_job_status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GetJobStatusParams {
    /// Job to describe.
    pub job_id: JobId,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_nodes_accepts_a_session_and_a_root() {
        let params: ListNodesParams = serde_json::from_value(serde_json::json!({
            "session_id": "11111111-1111-4111-8111-111111111111",
            "root_id": "1:2",
            "depth": 3
        }))
        .expect("parses");
        assert_eq!(params.root_id, "1:2");
        assert_eq!(params.depth, Some(3));
        assert!(params.session_id.is_some());
        assert!(params.snapshot_id.is_none());
    }

    #[test]
    fn consistency_defaults_to_fresh() {
        let params: ListNodesParams =
            serde_json::from_value(serde_json::json!({ "root_id": "1:2" })).expect("parses");
        assert_eq!(params.consistency, ConsistencyRequest::Fresh);
    }

    #[test]
    fn export_assets_requires_a_format() {
        let params: ExportAssetsParams = serde_json::from_value(serde_json::json!({
            "node_ids": ["1:2"],
            "format": "png"
        }))
        .expect("parses");
        assert_eq!(params.format, AssetFormat::Png);
    }

    #[test]
    fn tool_parameters_generate_a_json_schema() {
        let schema = schemars::schema_for!(ListNodesParams);
        let value = serde_json::to_value(schema).expect("schema serializes");
        assert!(value.get("properties").is_some());
    }
}
