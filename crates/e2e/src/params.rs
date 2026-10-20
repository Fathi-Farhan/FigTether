//! Convenience constructors for the tool parameters.
//!
//! The parameter structs of `figtether_model` have no `Default`, because
//! every read names its source explicitly. The constructors below fill the
//! fields a harness read needs and leave the rest at their neutral value, so
//! a test only states what matters for its scenario.

use figtether_model::ids::{SessionId, SnapshotId};
use figtether_model::tools::{
    AssetFormat, ConsistencyRequest, DetailLevel, ExportAssetsParams, GetDesignTokensParams,
    GetNodesParams, GetSelectionParams, GetSnapshotInfoParams, InspectDesignParams,
    ListNodesParams,
};

/// Parameters of a live `list_nodes` read.
pub fn list_nodes(session_id: SessionId, root_id: &str) -> ListNodesParams {
    ListNodesParams {
        session_id: Some(session_id),
        snapshot_id: None,
        root_id: root_id.to_string(),
        cursor: None,
        depth: None,
        fields: Vec::new(),
        limit: None,
        consistency: ConsistencyRequest::Fresh,
        max_age_ms: None,
    }
}

/// Parameters of a snapshot `list_nodes` read.
pub fn list_nodes_of_snapshot(snapshot_id: SnapshotId, root_id: &str) -> ListNodesParams {
    ListNodesParams {
        session_id: None,
        snapshot_id: Some(snapshot_id),
        root_id: root_id.to_string(),
        cursor: None,
        depth: None,
        fields: Vec::new(),
        limit: None,
        consistency: ConsistencyRequest::Fresh,
        max_age_ms: None,
    }
}

/// Parameters of a live `get_nodes` read.
pub fn get_nodes(session_id: SessionId, node_ids: &[&str]) -> GetNodesParams {
    GetNodesParams {
        session_id: Some(session_id),
        snapshot_id: None,
        node_ids: node_ids.iter().map(|id| (*id).to_string()).collect(),
        depth: None,
        fields: Vec::new(),
        consistency: ConsistencyRequest::Fresh,
        max_age_ms: None,
    }
}

/// Parameters of a live `inspect_design` read.
pub fn inspect_design(session_id: SessionId, root_id: &str) -> InspectDesignParams {
    InspectDesignParams {
        session_id: Some(session_id),
        snapshot_id: None,
        root_id: root_id.to_string(),
        detail: DetailLevel::Summary,
        consistency: ConsistencyRequest::Fresh,
        max_age_ms: None,
    }
}

/// Parameters of a live `get_design_tokens` read.
pub fn design_tokens(session_id: SessionId, root_id: &str) -> GetDesignTokensParams {
    GetDesignTokensParams {
        session_id: Some(session_id),
        snapshot_id: None,
        root_id: root_id.to_string(),
        kinds: Vec::new(),
        consistency: ConsistencyRequest::Fresh,
        max_age_ms: None,
    }
}

/// Parameters of a live `export_assets` call.
pub fn export_assets(
    session_id: SessionId,
    node_ids: &[&str],
    format: AssetFormat,
) -> ExportAssetsParams {
    ExportAssetsParams {
        session_id: Some(session_id),
        snapshot_id: None,
        node_ids: node_ids.iter().map(|id| (*id).to_string()).collect(),
        format,
        scale: Some(1.0),
    }
}

/// Parameters of a live `get_selection` read.
pub fn selection(session_id: SessionId) -> GetSelectionParams {
    GetSelectionParams { session_id }
}

/// Parameters of a `get_snapshot_info` read.
pub fn snapshot_info(snapshot_id: SnapshotId) -> GetSnapshotInfoParams {
    GetSnapshotInfoParams { snapshot_id }
}
