//! Write path contracts.
//!
//! The write path is plan based (`ADR-013`, `docs/11-write-path/`): an agent
//! proposes a plan, the plugin resolves it against the live document without
//! mutating anything, the user approves the preview, and only then is the plan
//! applied. The types in this module are the vocabulary of that loop.
//!
//! Rules that hold for every type in this module:
//! - a plan is data, never code, and never a direct mutation;
//! - the property allowlist is closed, so a property outside it is a plan
//!   error and not a best effort;
//! - the plan hash binds an approval to exactly the plan the user saw;
//! - every budget is enforced, and a plan that breaks one is refused whole,
//!   never trimmed.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ids::SessionId;

/// What a session grant allows.
///
/// A read grant never authorizes a mutation, and no tool parameter can widen
/// a grant. The mode is a decision of the user in the plugin UI, and a change
/// of the mode increments the grant epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GrantMode {
    /// Reads inside the granted roots, no mutation.
    #[default]
    Read,
    /// Reads inside the granted roots, and approved writes inside them.
    Write,
}

impl GrantMode {
    /// Stable wire string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
        }
    }

    /// Whether the mode authorizes a mutation.
    pub const fn allows_writes(self) -> bool {
        matches!(self, Self::Write)
    }
}

/// The kind of one write operation.
///
/// The set is closed. An agent cannot invent an operation, and the plugin
/// resolves exactly these kinds against the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WriteOperationKind {
    /// Create a new node of a type below a parent.
    CreateNode,
    /// Move an existing node below another parent.
    InsertChild,
    /// Set allowlisted properties on an existing node.
    SetProperties,
    /// Replace the characters of a text node.
    SetText,
    /// Replace the fills of a node.
    SetFills,
    /// Remove a node and its subtree.
    DeleteNode,
    /// Change the position of a node.
    MoveNode,
}

impl WriteOperationKind {
    /// Stable wire string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CreateNode => "create_node",
            Self::InsertChild => "insert_child",
            Self::SetProperties => "set_properties",
            Self::SetText => "set_text",
            Self::SetFills => "set_fills",
            Self::DeleteNode => "delete_node",
            Self::MoveNode => "move_node",
        }
    }

    /// Whether the operation changes the tree structure.
    ///
    /// A delete, a reparent, and a move are the operations the destructive
    /// policy covers. They need the destructive flag on the plan and a
    /// separate user toggle in the plugin UI.
    pub const fn is_destructive(self) -> bool {
        matches!(self, Self::DeleteNode | Self::InsertChild | Self::MoveNode)
    }

    /// Whether the operation names the node it acts on.
    pub const fn requires_node_id(self) -> bool {
        matches!(
            self,
            Self::SetProperties
                | Self::SetText
                | Self::SetFills
                | Self::DeleteNode
                | Self::MoveNode
        )
    }

    /// Whether the operation names the parent it works under.
    pub const fn requires_parent_id(self) -> bool {
        matches!(self, Self::CreateNode | Self::InsertChild)
    }

    /// Whether the operation names the type of the node it creates.
    pub const fn requires_node_type(self) -> bool {
        matches!(self, Self::CreateNode)
    }
}

/// One operation of a write plan.
///
/// The shape is deliberately wide and declarative: the same operation struct
/// carries every kind, and the fields that a kind does not use stay empty.
/// The daemon validates the combination, the plugin resolves it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WriteOperation {
    /// What the operation does.
    pub kind: WriteOperationKind,
    /// Parent the operation works under. Required for create and insert.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    /// Node the operation acts on. Required for every operation that changes
    /// an existing node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    /// Type of the node to create. Required for create, and limited to
    /// `ALLOWED_NODE_TYPES`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,
    /// Properties to set on the node. Only the names in
    /// `ALLOWED_WRITE_PROPERTIES` are accepted. Fills and strokes are not
    /// properties, they are set through the `SetFills` operation.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, serde_json::Value>,
    /// Paints to set on the node. Only the `SetFills` operation reads this
    /// field. The values are normalized by the plugin before they reach the
    /// editor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fills: Option<Vec<serde_json::Value>>,
    /// Characters of a text node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Horizontal position.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<f64>,
    /// Vertical position.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<f64>,
    /// Position of the node among the children of the parent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
    /// Font family of the characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    /// Font style of the characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_style: Option<String>,
}

/// A declarative change set proposed for one session.
///
/// The plan is the only thing an agent submits. It is reviewed, hashed,
/// approved, and applied as a whole.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WritePlan {
    /// Opaque plan id, minted by the agent, echoed back in the preview.
    pub plan_id: String,
    /// Session the plan targets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    /// Ordered list of operations.
    #[serde(default)]
    pub operations: Vec<WriteOperation>,
    /// Short free text shown to the user. It is never executed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<String>,
    /// Whether the plan declares itself destructive.
    #[serde(default)]
    pub destructive: bool,
}

/// What a plan would change, resolved against the live document.
///
/// The preview never mutates. A plan that cannot be applied as written
/// produces a blocked preview with the reason, never a partial apply.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WritePreview {
    /// Plan the preview belongs to.
    pub plan_id: String,
    /// Hash of the canonical plan. The approval is bound to this hash.
    pub plan_hash: String,
    /// Node ids that would be created.
    #[serde(default)]
    pub created: Vec<String>,
    /// Number of nodes the plan would create.
    ///
    /// A node that does not exist yet has no id, so the preview reports the
    /// count here and the applied result reports the real ids.
    #[serde(default)]
    pub created_count: u32,
    /// Approval the user has to grant for this plan.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
    /// Node ids whose properties would change.
    #[serde(default)]
    pub changed: Vec<String>,
    /// Node ids that would be removed.
    #[serde(default)]
    pub removed: Vec<String>,
    /// Node ids that would be reparented or repositioned.
    #[serde(default)]
    pub moved: Vec<String>,
    /// Number of operations in the plan.
    #[serde(default)]
    pub operations_count: u32,
    /// Reasons a part of the plan could not be resolved.
    #[serde(default)]
    pub warnings: Vec<String>,
    /// Whether the plan cannot be applied as written.
    #[serde(default)]
    pub blocked: bool,
}

/// The token that authorizes one approved plan.
///
/// The token is bound to the plan hash, the session, the grant epoch, and an
/// expiry. A grant change invalidates every outstanding approval.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WriteApproval {
    /// Opaque approval id.
    pub approval_id: String,
    /// Hash of the canonical plan the user approved.
    pub plan_hash: String,
    /// Session the approval is bound to.
    pub session_id: SessionId,
    /// Grant epoch the approval is bound to.
    pub grant_epoch: u64,
    /// When the approval was issued.
    pub issued_at: DateTime<Utc>,
    /// When the approval expires.
    pub expires_at: DateTime<Utc>,
    /// Who approved. Always the user, never the agent.
    pub approved_by: String,
}

impl WriteApproval {
    /// Whether the approval is expired at the given moment.
    pub fn is_expired_at(&self, now: DateTime<Utc>) -> bool {
        now >= self.expires_at
    }

    /// Whether the approval binds to the given plan, session, and grant.
    ///
    /// A grant change increments the grant epoch, so an approval minted under
    /// an older epoch never matches and the plan has to be approved again.
    pub fn binds_to(&self, plan_hash: &str, session_id: &SessionId, grant_epoch: u64) -> bool {
        self.plan_hash == plan_hash
            && self.session_id == *session_id
            && self.grant_epoch == grant_epoch
    }
}

/// What an approved plan changed.
///
/// The result reports what happened, never what was intended. A plan that
/// was refused or only partly applied is reported as such.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WriteResult {
    /// Plan that was applied.
    pub plan_id: String,
    /// Hash of the canonical plan that was applied.
    pub plan_hash: String,
    /// Approval the apply was made with.
    pub approval_id: String,
    /// Node ids that exist after the apply.
    #[serde(default)]
    pub created: Vec<String>,
    /// Node ids that were changed.
    #[serde(default)]
    pub changed: Vec<String>,
    /// Node ids that were removed.
    #[serde(default)]
    pub removed: Vec<String>,
    /// Node ids that were moved.
    #[serde(default)]
    pub moved: Vec<String>,
    /// Whether the plugin committed the plan as one undo step.
    #[serde(default)]
    pub undo_committed: bool,
    /// Version id, when a version checkpoint was written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_saved: Option<String>,
    /// Non fatal caveats of the apply.
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// The closed allowlist of property names a plan may set.
///
/// The list holds the geometry, size, layout, appearance, and naming
/// properties a design system needs, and nothing that changes the structure
/// of the document implicitly. Fills and strokes are not in the list, they
/// are set through the `SetFills` operation.
///
/// This list is the single source of truth. The TypeScript mirror in
/// `plugins/figma/shared/src/write.ts` has to carry exactly the same names, and
/// a test in that package compares it against this list.
pub const ALLOWED_WRITE_PROPERTIES: &[&str] = &[
    "x",
    "y",
    "width",
    "height",
    "rotation",
    "opacity",
    "visible",
    "locked",
    "name",
    "fontSize",
    "cornerRadius",
    "topLeftRadius",
    "topRightRadius",
    "bottomLeftRadius",
    "bottomRightRadius",
    "layoutMode",
    "primaryAxisAlignItems",
    "counterAxisAlignItems",
    "layoutSizingHorizontal",
    "layoutSizingVertical",
    "itemSpacing",
    "paddingLeft",
    "paddingRight",
    "paddingTop",
    "paddingBottom",
    "clipsContent",
    "blendMode",
    "description",
];

/// The node types a plan may create.
///
/// The names are the node types of the editor and are matched exactly, so a
/// plan cannot create a page, an instance, or a component set.
pub const ALLOWED_NODE_TYPES: &[&str] = &[
    "FRAME",
    "TEXT",
    "RECTANGLE",
    "COMPONENT",
    "VECTOR",
    "SECTION",
    "GROUP",
    "LINE",
    "ELLIPSE",
];

/// Whether a property name is on the allowlist.
pub fn is_allowed_write_property(name: &str) -> bool {
    ALLOWED_WRITE_PROPERTIES.contains(&name)
}

/// Whether a node type may be created by a plan.
pub fn is_allowed_node_type(node_type: &str) -> bool {
    ALLOWED_NODE_TYPES.contains(&node_type)
}

/// Hash the canonical form of a plan.
///
/// The canonical form is the compact JSON encoding of the plan with every
/// object key in lexicographic order, which is the encoding `serde_json`
/// produces for a value tree. The hash binds an approval to exactly the plan
/// the user saw, so a plan that differs in any field hashes differently.
pub fn canonical_plan_hash(plan: &WritePlan) -> String {
    let value = serde_json::to_value(plan).unwrap_or(serde_json::Value::Null);
    let canonical = serde_json::to_vec(&value).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(&canonical);
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

/// The number of operations in a plan.
pub fn write_operations_count(plan: &WritePlan) -> u32 {
    u32::try_from(plan.operations.len()).unwrap_or(u32::MAX)
}

/// Whether a plan is destructive.
///
/// A plan is destructive when it carries a delete, a reparent, or a move, or
/// when it declares itself destructive. The destructive flag on the plan is
/// the declaration, the operations are the truth.
pub fn plan_is_destructive(plan: &WritePlan) -> bool {
    plan.destructive
        || plan
            .operations
            .iter()
            .any(|operation| operation.kind.is_destructive())
}

/// The write budgets of one installation.
///
/// The defaults come from the design of the write path. An installation may
/// lower any budget, never raise one, because the published behaviour was
/// measured against the default profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct WriteBudget {
    /// Maximum number of operations in one plan.
    pub max_operations_per_plan: u32,
    /// Maximum number of nodes one plan may create.
    pub max_created_nodes_per_plan: u32,
    /// Maximum number of text characters one plan may carry.
    pub max_text_characters_per_plan: u32,
    /// Maximum number of property changes one plan may carry.
    pub max_property_changes_per_plan: u32,
    /// Maximum number of nodes one delete may remove with its subtree.
    pub max_deleted_subtree_nodes: u32,
    /// Lifetime of an approval, in seconds.
    pub approval_ttl_seconds: u64,
    /// Maximum number of write operations one session may run per minute.
    pub max_operations_per_minute: u32,
}

impl Default for WriteBudget {
    fn default() -> Self {
        Self {
            max_operations_per_plan: 50,
            max_created_nodes_per_plan: 100,
            max_text_characters_per_plan: 20_000,
            max_property_changes_per_plan: 200,
            max_deleted_subtree_nodes: 50,
            approval_ttl_seconds: 600,
            max_operations_per_minute: 10,
        }
    }
}

/// One rule a plan broke.
///
/// The variants are the rules of the write path. Every budget violation
/// carries the limit and the actual count, so the message names the number
/// the caller has to beat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum WriteViolation {
    /// The plan carries no operation at all.
    EmptyPlan,
    /// The plan carries more operations than the budget allows.
    TooManyOperations {
        /// The limit of the budget.
        limit: u32,
        /// The count in the plan.
        actual: u32,
    },
    /// The plan creates more nodes than the budget allows.
    TooManyCreatedNodes {
        /// The limit of the budget.
        limit: u32,
        /// The count in the plan.
        actual: u32,
    },
    /// The plan carries more text characters than the budget allows.
    TooMuchText {
        /// The limit of the budget.
        limit: u32,
        /// The count in the plan.
        actual: u32,
    },
    /// The plan changes more properties than the budget allows.
    TooManyPropertyChanges {
        /// The limit of the budget.
        limit: u32,
        /// The count in the plan.
        actual: u32,
    },
    /// A destructive operation without the destructive flag on the plan.
    DestructiveFlagMissing,
    /// An operation misses the node id it acts on.
    MissingNodeId {
        /// The operation kind that needs a node id.
        kind: WriteOperationKind,
    },
    /// An operation misses the parent id it works under.
    MissingParentId {
        /// The operation kind that needs a parent id.
        kind: WriteOperationKind,
    },
    /// A create operation misses the node type.
    MissingNodeType,
    /// A create operation names a node type outside the allowlist.
    UnknownNodeType {
        /// The rejected node type.
        node_type: String,
    },
    /// An operation sets a property outside the allowlist.
    UnknownProperty {
        /// The rejected property name.
        name: String,
    },
}

impl WriteViolation {
    /// Human readable explanation of the violation.
    ///
    /// Every budget violation names the limit and the actual count, so the
    /// caller can see which number has to change.
    pub fn message(&self) -> String {
        match self {
            Self::EmptyPlan => "the plan carries no operation".to_string(),
            Self::TooManyOperations { limit, actual } => {
                format!("the plan carries {actual} operations, the limit is {limit}")
            }
            Self::TooManyCreatedNodes { limit, actual } => {
                format!("the plan creates {actual} nodes, the limit is {limit}")
            }
            Self::TooMuchText { limit, actual } => {
                format!("the plan carries {actual} text characters, the limit is {limit}")
            }
            Self::TooManyPropertyChanges { limit, actual } => {
                format!("the plan changes {actual} properties, the limit is {limit}")
            }
            Self::DestructiveFlagMissing => {
                "the plan carries a destructive operation without the destructive flag".to_string()
            }
            Self::MissingNodeId { kind } => {
                format!("the {} operation does not name a node", kind.as_str())
            }
            Self::MissingParentId { kind } => {
                format!("the {} operation does not name a parent", kind.as_str())
            }
            Self::MissingNodeType => {
                "the create_node operation does not name a node type".to_string()
            }
            Self::UnknownNodeType { node_type } => {
                format!("the node type {node_type} is not allowed")
            }
            Self::UnknownProperty { name } => {
                format!("the property {name} is not allowed")
            }
        }
    }
}

impl std::fmt::Display for WriteViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for WriteViolation {}

/// Validate a plan against the structural rules and the write budget.
///
/// The checks run in a fixed order, so the same plan always produces the
/// same violation: an empty plan first, then the structure of every
/// operation, then the destructive policy, then the budgets.
///
/// The subtree size of a delete and the rate of a session are not decided
/// here. The first needs the live document, the second needs the history of
/// the session, so both are checked where that knowledge lives.
pub fn validate_plan(plan: &WritePlan, budget: &WriteBudget) -> Result<(), WriteViolation> {
    if plan.operations.is_empty() {
        return Err(WriteViolation::EmptyPlan);
    }

    for operation in &plan.operations {
        if operation.kind.requires_node_id() && operation.node_id.is_none() {
            return Err(WriteViolation::MissingNodeId {
                kind: operation.kind,
            });
        }
        if operation.kind.requires_parent_id() && operation.parent_id.is_none() {
            return Err(WriteViolation::MissingParentId {
                kind: operation.kind,
            });
        }
        if operation.kind.requires_node_type() {
            let Some(node_type) = operation.node_type.as_deref() else {
                return Err(WriteViolation::MissingNodeType);
            };
            if !is_allowed_node_type(node_type) {
                return Err(WriteViolation::UnknownNodeType {
                    node_type: node_type.to_string(),
                });
            }
        }
        for name in operation.properties.keys() {
            if !is_allowed_write_property(name) {
                return Err(WriteViolation::UnknownProperty { name: name.clone() });
            }
        }
    }

    if plan.operations.iter().any(|op| op.kind.is_destructive()) && !plan.destructive {
        return Err(WriteViolation::DestructiveFlagMissing);
    }

    let operations_count = write_operations_count(plan);
    if operations_count > budget.max_operations_per_plan {
        return Err(WriteViolation::TooManyOperations {
            limit: budget.max_operations_per_plan,
            actual: operations_count,
        });
    }

    let created_count = count_created_nodes(plan);
    if created_count > budget.max_created_nodes_per_plan {
        return Err(WriteViolation::TooManyCreatedNodes {
            limit: budget.max_created_nodes_per_plan,
            actual: created_count,
        });
    }

    let text_characters = count_text_characters(plan);
    if text_characters > budget.max_text_characters_per_plan {
        return Err(WriteViolation::TooMuchText {
            limit: budget.max_text_characters_per_plan,
            actual: text_characters,
        });
    }

    let property_changes = count_property_changes(plan);
    if property_changes > budget.max_property_changes_per_plan {
        return Err(WriteViolation::TooManyPropertyChanges {
            limit: budget.max_property_changes_per_plan,
            actual: property_changes,
        });
    }

    Ok(())
}

/// The number of nodes a plan creates.
fn count_created_nodes(plan: &WritePlan) -> u32 {
    let count = plan
        .operations
        .iter()
        .filter(|operation| operation.kind == WriteOperationKind::CreateNode)
        .count();
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// The number of text characters a plan carries.
fn count_text_characters(plan: &WritePlan) -> u32 {
    let count: usize = plan
        .operations
        .iter()
        .filter_map(|operation| operation.text.as_deref())
        .map(str::chars)
        .map(Iterator::count)
        .sum();
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// The number of property changes a plan carries.
///
/// Every property of an operation is one change, and a text change and a
/// fill change each count as one change on top of the properties.
fn count_property_changes(plan: &WritePlan) -> u32 {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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

    fn create_node(parent_id: &str, node_type: &str) -> WriteOperation {
        WriteOperation {
            parent_id: Some(parent_id.to_string()),
            node_type: Some(node_type.to_string()),
            ..operation(WriteOperationKind::CreateNode)
        }
    }

    fn set_properties(node_id: &str) -> WriteOperation {
        WriteOperation {
            node_id: Some(node_id.to_string()),
            ..operation(WriteOperationKind::SetProperties)
        }
    }

    fn plan(operations: Vec<WriteOperation>) -> WritePlan {
        WritePlan {
            plan_id: "plan_01".to_string(),
            session_id: Some(SessionId::new()),
            operations,
            intent: Some("make it blue".to_string()),
            destructive: false,
        }
    }

    #[test]
    fn plan_hash_is_stable_for_the_same_plan() {
        let plan = plan(vec![create_node("1:2", "FRAME")]);
        let first = canonical_plan_hash(&plan);
        let second = canonical_plan_hash(&plan);
        assert_eq!(first, second);
        assert!(first.starts_with("sha256:"));
        assert_eq!(first.len(), "sha256:".len() + 64);
    }

    #[test]
    fn plan_hash_changes_when_a_field_changes() {
        let base = plan(vec![create_node("1:2", "FRAME")]);
        let mut changed = base.clone();
        changed.operations[0].x = Some(12.0);
        assert_ne!(canonical_plan_hash(&base), canonical_plan_hash(&changed));

        let mut other_id = base.clone();
        other_id.plan_id = "plan_02".to_string();
        assert_ne!(canonical_plan_hash(&base), canonical_plan_hash(&other_id));
    }

    #[test]
    fn plan_hash_ignores_the_order_of_the_properties() {
        let mut first = set_properties("1:2");
        first.properties.insert("x".to_string(), json!(1.0));
        first.properties.insert("name".to_string(), json!("card"));

        let mut second = set_properties("1:2");
        second.properties.insert("name".to_string(), json!("card"));
        second.properties.insert("x".to_string(), json!(1.0));

        let base = plan(vec![first]);
        let mut other = base.clone();
        other.operations = vec![second];

        assert_eq!(canonical_plan_hash(&base), canonical_plan_hash(&other));
    }

    #[test]
    fn operations_count_matches_the_plan() {
        let empty = plan(Vec::new());
        assert_eq!(write_operations_count(&empty), 0);
        let three = plan(vec![
            create_node("1:2", "FRAME"),
            create_node("1:2", "TEXT"),
            set_properties("1:3"),
        ]);
        assert_eq!(write_operations_count(&three), 3);
    }

    #[test]
    fn destructive_operations_are_detected() {
        let delete = plan(vec![WriteOperation {
            node_id: Some("1:2".to_string()),
            ..operation(WriteOperationKind::DeleteNode)
        }]);
        assert!(plan_is_destructive(&delete));

        let insert = plan(vec![WriteOperation {
            parent_id: Some("1:2".to_string()),
            node_id: Some("1:3".to_string()),
            ..operation(WriteOperationKind::InsertChild)
        }]);
        assert!(plan_is_destructive(&insert));

        let move_node = plan(vec![WriteOperation {
            node_id: Some("1:3".to_string()),
            x: Some(4.0),
            y: Some(8.0),
            ..operation(WriteOperationKind::MoveNode)
        }]);
        assert!(plan_is_destructive(&move_node));
    }

    #[test]
    fn a_create_is_not_destructive() {
        let plan = plan(vec![create_node("1:2", "FRAME")]);
        assert!(!plan_is_destructive(&plan));
    }

    #[test]
    fn the_destructive_flag_marks_the_plan() {
        let mut plan = plan(vec![create_node("1:2", "FRAME")]);
        plan.destructive = true;
        assert!(plan_is_destructive(&plan));
    }

    #[test]
    fn allowlist_holds_the_documented_properties() {
        assert_eq!(ALLOWED_WRITE_PROPERTIES.len(), 28);
        assert!(is_allowed_write_property("x"));
        assert!(is_allowed_write_property("layoutMode"));
        assert!(is_allowed_write_property("description"));
        assert!(
            is_allowed_write_property("fontSize"),
            "the text size is a property of a text node"
        );
        assert!(!is_allowed_write_property("characters"));
    }

    #[test]
    fn fills_and_strokes_are_not_allowlisted_properties() {
        assert!(!is_allowed_write_property("fills"));
        assert!(!is_allowed_write_property("strokes"));
        assert!(!is_allowed_write_property("effects"));
    }

    #[test]
    fn an_empty_plan_is_refused() {
        let budget = WriteBudget::default();
        let err = validate_plan(&plan(Vec::new()), &budget).expect_err("refused");
        assert_eq!(err, WriteViolation::EmptyPlan);
    }

    #[test]
    fn the_operation_count_budget_is_enforced() {
        let budget = WriteBudget {
            max_operations_per_plan: 1,
            ..WriteBudget::default()
        };
        let plan = plan(vec![
            create_node("1:2", "FRAME"),
            create_node("1:2", "TEXT"),
        ]);
        let err = validate_plan(&plan, &budget).expect_err("refused");
        assert_eq!(
            err,
            WriteViolation::TooManyOperations {
                limit: 1,
                actual: 2
            }
        );
        assert!(err.message().contains("1"));
    }

    #[test]
    fn the_created_node_budget_is_enforced() {
        let budget = WriteBudget {
            max_created_nodes_per_plan: 1,
            ..WriteBudget::default()
        };
        let plan = plan(vec![
            create_node("1:2", "FRAME"),
            create_node("1:2", "TEXT"),
        ]);
        let err = validate_plan(&plan, &budget).expect_err("refused");
        assert_eq!(
            err,
            WriteViolation::TooManyCreatedNodes {
                limit: 1,
                actual: 2
            }
        );
    }

    #[test]
    fn the_text_budget_is_enforced() {
        let budget = WriteBudget {
            max_text_characters_per_plan: 4,
            ..WriteBudget::default()
        };
        let plan = plan(vec![WriteOperation {
            node_id: Some("1:2".to_string()),
            text: Some("hello".to_string()),
            ..operation(WriteOperationKind::SetText)
        }]);
        let err = validate_plan(&plan, &budget).expect_err("refused");
        assert_eq!(
            err,
            WriteViolation::TooMuchText {
                limit: 4,
                actual: 5
            }
        );
    }

    #[test]
    fn the_property_change_budget_is_enforced() {
        let budget = WriteBudget {
            max_property_changes_per_plan: 1,
            ..WriteBudget::default()
        };
        let mut operation = set_properties("1:2");
        operation.properties.insert("x".to_string(), json!(1.0));
        operation.properties.insert("y".to_string(), json!(2.0));
        let plan = plan(vec![operation]);
        let err = validate_plan(&plan, &budget).expect_err("refused");
        assert_eq!(
            err,
            WriteViolation::TooManyPropertyChanges {
                limit: 1,
                actual: 2
            }
        );
    }

    #[test]
    fn a_text_change_counts_as_one_property_change() {
        let budget = WriteBudget {
            max_property_changes_per_plan: 1,
            ..WriteBudget::default()
        };
        let plan = plan(vec![WriteOperation {
            node_id: Some("1:2".to_string()),
            text: Some("hi".to_string()),
            ..operation(WriteOperationKind::SetText)
        }]);
        assert!(validate_plan(&plan, &budget).is_ok());
    }

    #[test]
    fn a_destructive_plan_without_the_flag_is_refused() {
        let budget = WriteBudget::default();
        let plan = plan(vec![WriteOperation {
            node_id: Some("1:2".to_string()),
            ..operation(WriteOperationKind::DeleteNode)
        }]);
        let err = validate_plan(&plan, &budget).expect_err("refused");
        assert_eq!(err, WriteViolation::DestructiveFlagMissing);
    }

    #[test]
    fn a_destructive_plan_with_the_flag_passes() {
        let budget = WriteBudget::default();
        let mut plan = plan(vec![WriteOperation {
            node_id: Some("1:2".to_string()),
            ..operation(WriteOperationKind::DeleteNode)
        }]);
        plan.destructive = true;
        assert!(validate_plan(&plan, &budget).is_ok());
    }

    #[test]
    fn a_missing_node_id_is_refused() {
        let budget = WriteBudget::default();
        let plan = plan(vec![operation(WriteOperationKind::SetText)]);
        let err = validate_plan(&plan, &budget).expect_err("refused");
        assert_eq!(
            err,
            WriteViolation::MissingNodeId {
                kind: WriteOperationKind::SetText
            }
        );
    }

    #[test]
    fn a_missing_parent_id_is_refused() {
        let budget = WriteBudget::default();
        let plan = plan(vec![WriteOperation {
            node_type: Some("FRAME".to_string()),
            ..operation(WriteOperationKind::CreateNode)
        }]);
        let err = validate_plan(&plan, &budget).expect_err("refused");
        assert_eq!(
            err,
            WriteViolation::MissingParentId {
                kind: WriteOperationKind::CreateNode
            }
        );
    }

    #[test]
    fn a_missing_node_type_is_refused() {
        let budget = WriteBudget::default();
        let plan = plan(vec![WriteOperation {
            parent_id: Some("1:2".to_string()),
            ..operation(WriteOperationKind::CreateNode)
        }]);
        let err = validate_plan(&plan, &budget).expect_err("refused");
        assert_eq!(err, WriteViolation::MissingNodeType);
    }

    #[test]
    fn an_unknown_node_type_is_refused() {
        let budget = WriteBudget::default();
        let plan = plan(vec![create_node("1:2", "PAGE")]);
        let err = validate_plan(&plan, &budget).expect_err("refused");
        assert_eq!(
            err,
            WriteViolation::UnknownNodeType {
                node_type: "PAGE".to_string()
            }
        );
    }

    #[test]
    fn an_unknown_property_is_refused() {
        let budget = WriteBudget::default();
        let mut operation = set_properties("1:2");
        operation
            .properties
            .insert("characters".to_string(), json!("hi"));
        let plan = plan(vec![operation]);
        let err = validate_plan(&plan, &budget).expect_err("refused");
        assert_eq!(
            err,
            WriteViolation::UnknownProperty {
                name: "characters".to_string()
            }
        );
    }

    #[test]
    fn a_valid_plan_passes_validation() {
        let budget = WriteBudget::default();
        let mut properties = set_properties("1:2");
        properties.properties.insert("x".to_string(), json!(1.0));
        properties
            .properties
            .insert("name".to_string(), json!("card"));
        let plan = plan(vec![
            create_node("1:2", "FRAME"),
            properties,
            WriteOperation {
                node_id: Some("1:3".to_string()),
                text: Some("hello".to_string()),
                font_family: Some("Inter".to_string()),
                font_style: Some("Regular".to_string()),
                ..operation(WriteOperationKind::SetText)
            },
        ]);
        assert!(validate_plan(&plan, &budget).is_ok());
    }

    #[test]
    fn budget_defaults_match_the_design() {
        let budget = WriteBudget::default();
        assert_eq!(budget.max_operations_per_plan, 50);
        assert_eq!(budget.max_created_nodes_per_plan, 100);
        assert_eq!(budget.max_text_characters_per_plan, 20_000);
        assert_eq!(budget.max_property_changes_per_plan, 200);
        assert_eq!(budget.max_deleted_subtree_nodes, 50);
        assert_eq!(budget.approval_ttl_seconds, 600);
        assert_eq!(budget.max_operations_per_minute, 10);
    }

    #[test]
    fn grant_mode_serializes_as_snake_case() {
        assert_eq!(
            serde_json::to_string(&GrantMode::Read).expect("serializes"),
            "\"read\""
        );
        assert_eq!(
            serde_json::to_string(&GrantMode::Write).expect("serializes"),
            "\"write\""
        );
        assert!(!GrantMode::Read.allows_writes());
        assert!(GrantMode::Write.allows_writes());
    }

    #[test]
    fn operation_kinds_serialize_as_snake_case() {
        let kinds = [
            (WriteOperationKind::CreateNode, "create_node"),
            (WriteOperationKind::InsertChild, "insert_child"),
            (WriteOperationKind::SetProperties, "set_properties"),
            (WriteOperationKind::SetText, "set_text"),
            (WriteOperationKind::SetFills, "set_fills"),
            (WriteOperationKind::DeleteNode, "delete_node"),
            (WriteOperationKind::MoveNode, "move_node"),
        ];
        for (kind, wire) in kinds {
            assert_eq!(kind.as_str(), wire);
            assert_eq!(
                serde_json::to_string(&kind).expect("serializes"),
                format!("\"{wire}\"")
            );
        }
    }

    #[test]
    fn a_plan_round_trips_through_json() {
        let plan = plan(vec![create_node("1:2", "FRAME")]);
        let json = serde_json::to_string(&plan).expect("serializes");
        let parsed: WritePlan = serde_json::from_str(&json).expect("parses");
        assert_eq!(parsed, plan);
        assert_eq!(canonical_plan_hash(&parsed), canonical_plan_hash(&plan));
    }

    #[test]
    fn an_approval_binds_to_the_plan_and_the_grant() {
        let session_id = SessionId::new();
        let now = Utc::now();
        let approval = WriteApproval {
            approval_id: "approval_01".to_string(),
            plan_hash: "sha256:abc".to_string(),
            session_id,
            grant_epoch: 7,
            issued_at: now,
            expires_at: now + chrono::Duration::seconds(600),
            approved_by: "user".to_string(),
        };
        assert!(approval.binds_to("sha256:abc", &session_id, 7));
        assert!(!approval.binds_to("sha256:def", &session_id, 7));
        assert!(!approval.binds_to("sha256:abc", &session_id, 8));
        assert!(!approval.is_expired_at(now));
        assert!(approval.is_expired_at(now + chrono::Duration::seconds(601)));
    }

    #[test]
    fn a_violation_message_names_the_limit() {
        let violation = WriteViolation::TooManyCreatedNodes {
            limit: 100,
            actual: 101,
        };
        let message = violation.message();
        assert!(message.contains("100"));
        assert!(message.contains("101"));
        assert_eq!(message, violation.to_string());
    }
}
