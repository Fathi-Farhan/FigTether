//! The write path of the tool surface.
//!
//! The write path is plan based (`ADR-013`, `docs/11-write-path/`): an agent
//! proposes a plan, the plugin resolves the plan against the live document
//! without mutating it, the user approves the preview in the plugin, and only
//! then is the plan applied.
//!
//! The adapter is a transport in that loop. It never mutates a document, it
//! never approves anything on behalf of the user, and it never widens a
//! grant. Three tools make the loop reachable for an agent:
//!
//! - `preview_write` proposes a plan and answers with the preview and the
//!   approval request the user has to answer in the plugin;
//! - `get_write_approval` reads the decision of the user and, when the plan
//!   was approved, the token that authorizes the apply;
//! - `apply_write` applies an approved plan with that token.
//!
//! The annotations follow the contract of the write path: `preview_write` and
//! `get_write_approval` change nothing and stay read only, `apply_write`
//! changes the document and is marked destructive.
//!
//! A failure of the write path is a business failure: it travels as a
//! structured tool result with `is_error` set, never as a protocol error and
//! never as a stack trace. The codes of the write taxonomy are
//! `WRITE_NOT_GRANTED`, `APPROVAL_REQUIRED`, `APPROVAL_EXPIRED`,
//! `PLAN_HASH_MISMATCH`, `PLAN_INVALID`, `BUDGET_EXCEEDED`,
//! `DESTRUCTIVE_NOT_ALLOWED`, and `MUTATION_REFUSED`.
//!
//! The plan, the preview, and the result of a plan are the types of the
//! model crate, so the published schema and the wire contract of a plan can
//! never drift apart. The parameter structs of the three tools live here,
//! next to the tools that use them.

use figtether_model::errors::{ErrorCode, FigError};
use figtether_model::ids::SessionId;
use figtether_model::write::WritePlan;
use figtether_protocol::client::ClientRequest;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use rmcp::service::RequestContext;
use rmcp::RoleServer;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::tools::{encode, FigTetherService, WRITE_TIMEOUT};

/// Parameters of `preview_write`.
///
/// The plan is a proposal, not a command. Nothing of it reaches the document
/// before the user approved the preview in the plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PreviewWriteParams {
    /// Live session the plan targets.
    pub session_id: SessionId,
    /// The plan to preview.
    pub plan: WritePlan,
}

/// Parameters of `get_write_approval`.
///
/// The approval id is the one the preview answered with. It names an
/// approval request, never a token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GetWriteApprovalParams {
    /// The approval request to describe.
    pub approval_id: String,
}

/// Parameters of `apply_write`.
///
/// The token comes from `get_write_approval` and is bound to exactly the
/// plan the user saw. A plan that differs in any field is refused.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ApplyWriteParams {
    /// The approved plan, exactly as it was previewed.
    pub plan: WritePlan,
    /// The token that proves the user approved this plan.
    pub approval_token: String,
    /// Live session that holds the write grant.
    ///
    /// The session may also be named by the plan itself. The daemon reads it
    /// from the plan first, so naming it in both places is only allowed when
    /// both names are the same.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
}

/// Check that a plan can be sent to the daemon.
///
/// The check is a courtesy, not the validation: the daemon validates the
/// plan against the write grant, the scope, the budget, and the destructive
/// policy. The adapter refuses the two mistakes that would be harmful and
/// that cost nothing to catch early, an unnamed plan, and a destructive
/// operation without the declaration that belongs to it.
fn validate_plan(plan: &WritePlan) -> Result<(), FigError> {
    if plan.plan_id.trim().is_empty() {
        return Err(FigError::new(
            ErrorCode::PlanInvalid,
            "the plan has no plan_id, so the preview could not name it",
        ));
    }
    if plan.operations.is_empty() {
        return Err(FigError::new(
            ErrorCode::PlanInvalid,
            "the plan has no operation, so there is nothing to preview",
        ));
    }
    if !plan.destructive && plan_holds_a_destructive_operation(plan) {
        return Err(FigError::new(
            ErrorCode::DestructiveNotAllowed,
            "the plan holds a destructive operation but does not declare itself destructive",
        ));
    }
    Ok(())
}

/// Whether a plan holds an operation the destructive policy covers.
fn plan_holds_a_destructive_operation(plan: &WritePlan) -> bool {
    plan.operations
        .iter()
        .any(|operation| operation.kind.is_destructive())
}

/// The session an apply belongs to.
///
/// The daemon reads the session from the plan or from the parameters of the
/// request, and the plan wins when both name one. The adapter resolves the
/// session the same way: the plan first, then the session the caller named,
/// then the session the approval token was minted for. The token is a public
/// string that both sides rebuild from the same inputs, never a secret.
fn session_of_the_apply(
    plan: &WritePlan,
    session_id: Option<SessionId>,
    approval_token: &str,
) -> Option<SessionId> {
    plan.session_id.or(session_id).or_else(|| {
        figtether_protocol::write::parse_approval_token(approval_token)
            .map(|parts| parts.session_id)
    })
}

/// Encode the parameters of an apply for the daemon.
///
/// The session of the apply travels next to the plan when the plan does not
/// name it, so that the daemon can bind the approval to the session it was
/// minted for. The plan itself never changes, because the approval is bound
/// to the hash of the plan.
pub(crate) fn apply_payload(params: &ApplyWriteParams) -> Result<serde_json::Value, FigError> {
    let mut payload = encode(params)?;
    if params.plan.session_id.is_none() {
        if let Some(session_id) =
            session_of_the_apply(&params.plan, params.session_id, &params.approval_token)
        {
            payload["session_id"] =
                serde_json::to_value(session_id).unwrap_or(serde_json::Value::Null);
        }
    }
    Ok(payload)
}

#[rmcp::tool_router(router = write_tool_router, vis = "pub")]
impl FigTetherService {
    /// Propose a write plan and read its preview.
    #[rmcp::tool(
        name = "preview_write",
        description = "Propose a write plan and read what it would change. The plan is a proposal: the document does not change until the user approves the preview in the plugin. The answer names the nodes that would be created, changed, moved, or removed, the number of operations, the warnings, whether the plan is blocked, and the approval_id the user approves. The operations are create_node, insert_child, set_properties, set_text, set_fills, delete_node, and move_node. Then call get_write_approval for the approval token, and apply_write with the plan and that token.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn preview_write(
        &self,
        Parameters(params): Parameters<PreviewWriteParams>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.execute_write(
            move |request_id| {
                validate_plan(&params.plan)?;
                let payload = encode(&params)?;
                Ok(ClientRequest::PreviewWrite {
                    request_id,
                    params: payload,
                })
            },
            WRITE_TIMEOUT,
            &context,
        )
        .await
    }

    /// Read the decision of the user about an approval request.
    #[rmcp::tool(
        name = "get_write_approval",
        description = "Read the status of an approval request: pending, approved, rejected, or expired. Call it after preview_write, once the user has decided in the plugin. When the plan is approved, the answer carries the approval token that apply_write needs. When it is not approved, the answer says so and no token is issued.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_write_approval(
        &self,
        Parameters(params): Parameters<GetWriteApprovalParams>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.execute_write(
            move |request_id| {
                if params.approval_id.trim().is_empty() {
                    return Err(FigError::new(
                        ErrorCode::ApprovalRequired,
                        "the approval request has no approval_id, so no approval can be read",
                    ));
                }
                let payload = encode(&params)?;
                Ok(ClientRequest::GetWriteApproval {
                    request_id,
                    params: payload,
                })
            },
            WRITE_TIMEOUT,
            &context,
        )
        .await
    }

    /// Apply a plan the user approved.
    #[rmcp::tool(
        name = "apply_write",
        description = "Apply a plan the user already approved in the plugin. The approval_token comes from get_write_approval, and the plan has to be exactly the plan that was previewed, because the approval is bound to its hash. Name the session in the plan or in session_id. The answer reports the node ids that were created, changed, removed, and moved, whether the change was committed as one undo step, the version id when a version checkpoint was written, and the warnings. A plan without a valid approval is refused and changes nothing.",
        annotations(read_only_hint = false, destructive_hint = true)
    )]
    async fn apply_write(
        &self,
        Parameters(params): Parameters<ApplyWriteParams>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.execute_write(
            move |request_id| {
                validate_plan(&params.plan)?;
                if params.approval_token.trim().is_empty() {
                    return Err(FigError::new(
                        ErrorCode::ApprovalRequired,
                        "the apply has no approval_token, so the plan was not approved by the user",
                    ));
                }
                let payload = apply_payload(&params)?;
                Ok(ClientRequest::ApplyWrite {
                    request_id,
                    params: payload,
                })
            },
            WRITE_TIMEOUT,
            &context,
        )
        .await
    }
}

/// The kinds of operation a plan may hold, as the tool description names them.
///
/// The list is closed, and it is the list of the write path design. The
/// adapter never invents an operation and never resolves one.
pub const OPERATION_KINDS: [&str; 7] = [
    "create_node",
    "insert_child",
    "set_properties",
    "set_text",
    "set_fills",
    "delete_node",
    "move_node",
];

#[cfg(test)]
mod tests {
    use super::*;
    use figtether_model::write::{WriteOperation, WriteOperationKind};

    use crate::tools::tool_router;

    fn plan(operations: Vec<WriteOperation>) -> WritePlan {
        WritePlan {
            plan_id: "plan_1".to_string(),
            session_id: None,
            operations,
            intent: None,
            destructive: false,
        }
    }

    fn create() -> WriteOperation {
        WriteOperation {
            kind: WriteOperationKind::CreateNode,
            parent_id: Some("1:2".to_string()),
            node_id: None,
            node_type: Some("FRAME".to_string()),
            properties: Default::default(),
            fills: None,
            text: None,
            x: None,
            y: None,
            index: None,
            font_family: None,
            font_style: None,
        }
    }

    fn delete() -> WriteOperation {
        WriteOperation {
            kind: WriteOperationKind::DeleteNode,
            parent_id: None,
            node_id: Some("1:3".to_string()),
            node_type: None,
            properties: Default::default(),
            fills: None,
            text: None,
            x: None,
            y: None,
            index: None,
            font_family: None,
            font_style: None,
        }
    }

    /// An approval token of the session.
    fn approval_token(session: &SessionId) -> String {
        figtether_protocol::write::approval_token("apr_1", "abc123", session, 3, chrono::Utc::now())
    }

    fn description_of(name: &str) -> String {
        tool_router()
            .list_all()
            .into_iter()
            .find(|tool| tool.name == name)
            .expect("the tool is published")
            .description
            .map(|text| text.to_string())
            .unwrap_or_default()
    }

    #[test]
    fn a_plan_without_a_plan_id_is_refused() {
        let mut plan = plan(vec![create()]);
        plan.plan_id = "  ".to_string();
        let error = validate_plan(&plan).expect_err("the plan is refused");
        assert_eq!(error.code, ErrorCode::PlanInvalid);
        assert!(!error.action.is_empty());
    }

    #[test]
    fn a_plan_without_an_operation_is_refused() {
        let error = validate_plan(&plan(Vec::new())).expect_err("the plan is refused");
        assert_eq!(error.code, ErrorCode::PlanInvalid);
    }

    #[test]
    fn a_destructive_operation_without_the_declaration_is_refused() {
        let error = validate_plan(&plan(vec![delete()])).expect_err("the plan is refused");
        assert_eq!(error.code, ErrorCode::DestructiveNotAllowed);
    }

    #[test]
    fn a_declared_destructive_plan_is_accepted() {
        let mut plan = plan(vec![delete()]);
        plan.destructive = true;
        assert!(validate_plan(&plan).is_ok());
    }

    #[test]
    fn an_ordinary_plan_is_accepted() {
        assert!(validate_plan(&plan(vec![create()])).is_ok());
    }

    #[test]
    fn the_session_of_the_apply_comes_from_the_plan_first() {
        let session = SessionId::new();
        let other = SessionId::new();
        let mut plan = plan(vec![create()]);
        plan.session_id = Some(session);
        let token = approval_token(&other);
        assert_eq!(session_of_the_apply(&plan, None, &token), Some(session));
    }

    #[test]
    fn the_session_of_the_apply_comes_from_the_caller_when_the_plan_is_silent() {
        let session = SessionId::new();
        let plan = plan(vec![create()]);
        assert_eq!(
            session_of_the_apply(&plan, Some(session), "not-a-token"),
            Some(session)
        );
    }

    #[test]
    fn the_session_of_the_apply_comes_from_the_token_as_a_last_resort() {
        let session = SessionId::new();
        let plan = plan(vec![create()]);
        let token = approval_token(&session);
        assert_eq!(session_of_the_apply(&plan, None, &token), Some(session));
    }

    #[test]
    fn a_token_that_does_not_parse_leaves_the_session_to_the_daemon() {
        let plan = plan(vec![create()]);
        assert_eq!(session_of_the_apply(&plan, None, "not-a-token"), None);
    }

    #[test]
    fn the_apply_names_the_session_of_the_approval() {
        let session = SessionId::new();
        let token = approval_token(&session);
        let params = ApplyWriteParams {
            plan: plan(vec![create()]),
            approval_token: token.clone(),
            session_id: None,
        };
        let payload = apply_payload(&params).expect("encodes");
        assert_eq!(payload["session_id"], session.to_string());
        assert_eq!(payload["approval_token"], token);
        assert_eq!(payload["plan"]["plan_id"], "plan_1");
    }

    #[test]
    fn the_apply_keeps_the_plan_untouched() {
        let session = SessionId::new();
        let params = ApplyWriteParams {
            plan: plan(vec![create()]),
            approval_token: approval_token(&session),
            session_id: None,
        };
        let payload = apply_payload(&params).expect("encodes");
        let encoded = serde_json::to_value(&params.plan).expect("encodes");
        assert_eq!(payload["plan"], encoded);
    }

    #[test]
    fn the_description_of_the_preview_teaches_the_flow() {
        let description = description_of("preview_write");
        assert!(description.contains("proposal"), "{description}");
        assert!(description.contains("does not change"), "{description}");
        assert!(description.contains("approves"), "{description}");
        assert!(description.contains("get_write_approval"), "{description}");
        assert!(description.contains("apply_write"), "{description}");
    }

    #[test]
    fn the_description_of_the_preview_names_the_operations() {
        let description = description_of("preview_write");
        for kind in OPERATION_KINDS {
            assert!(
                description.contains(kind),
                "{kind} is missing: {description}"
            );
        }
    }

    #[test]
    fn the_description_of_the_approval_names_the_states() {
        let description = description_of("get_write_approval");
        for state in ["pending", "approved", "rejected", "expired"] {
            assert!(
                description.contains(state),
                "{state} is missing: {description}"
            );
        }
        assert!(description.contains("apply_write"), "{description}");
    }

    #[test]
    fn the_description_of_the_apply_names_the_token() {
        let description = description_of("apply_write");
        assert!(description.contains("approval_token"), "{description}");
        assert!(description.contains("get_write_approval"), "{description}");
        assert!(description.contains("undo"), "{description}");
    }
}
