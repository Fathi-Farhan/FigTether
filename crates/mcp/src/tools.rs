//! The tool surface.
//!
//! The adapter exposes fourteen read tools and the three
//! tools of the write path. The read tools are read only: they declare
//! `readOnlyHint=true` and `destructiveHint=false`, they name either a
//! session or a snapshot, and they never accept a parameter that widens a
//! grant, carries a secret, or names a bridge endpoint.
//!
//! The write tools are the write path of `ADR-013`: `preview_write` proposes
//! a plan and changes nothing, `get_write_approval` reads the decision of
//! the user, and `apply_write` applies an approved plan. The surface is not
//! uniformly read only any more, so the annotations tell the client which
//! tool can change a document.
//!
//! The input schema of every tool is generated from the parameter structs of
//! `figtether_model::tools` and `figtether_model::write`, so the published
//! contract and the wire contract can never drift apart.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use figtether_model::envelope::Envelope;
use figtether_model::errors::{ErrorCode, FigError};
use figtether_model::ids::JobId;
use figtether_model::tools::{
    ExportAssetsParams, GetBridgeStatusParams, GetChangesParams, GetDesignTokensParams,
    GetJobStatusParams, GetNodesParams, GetSelectionParams, GetSnapshotInfoParams,
    InspectDesignParams, ListNodesParams, ListSessionsParams, WatchScopeParams,
};
use figtether_protocol::client::{ClientRequest, ClientResponse};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Implementation, ServerCapabilities, ServerConfig};
use rmcp::service::RequestContext;
use rmcp::RoleServer;

use crate::daemon::DaemonClient;
use crate::error::McpError;
use crate::result;

/// Timeout of a status read.
pub const STATUS_TIMEOUT: Duration = Duration::from_secs(10);

/// Timeout of a read that has to reach the plugin.
pub const READ_TIMEOUT: Duration = Duration::from_secs(20);

/// Timeout of an export.
pub const EXPORT_TIMEOUT: Duration = Duration::from_secs(60);

/// Timeout of a write.
///
/// A write resolves the plan against the live document and, when it is
/// approved, applies it as one undo step. It is slower than a read, and it
/// stays bounded.
pub const WRITE_TIMEOUT: Duration = Duration::from_secs(120);

/// Interval between two polls of a background job.
pub const JOB_POLL_INTERVAL: Duration = Duration::from_millis(200);

/// The instructions handed to the client during the handshake.
pub const INSTRUCTIONS: &str = include_str!("../../../integrations/figtether/instructions.txt");

/// The cancellation source of one tool call.
///
/// The adapter never names the cancellation token of the SDK directly. The
/// token arrives through this trait, so that the path of a tool call can be
/// tested without a peer and so that the daemon client stays free of the
/// transport of the MCP lane.
pub trait CancelSource: Sync {
    /// A future that resolves when the caller cancels the call.
    fn cancelled(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

impl CancelSource for RequestContext<RoleServer> {
    fn cancelled(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(self.ct.cancelled())
    }
}

/// How the answer of the daemon is shaped into a tool result.
///
/// A read answers with the envelope of the daemon. A write answers with the
/// same envelope, and with the payload of the plan lifted next to it, so that
/// the preview and the result of a plan are readable without digging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AnswerShape {
    /// A read: the envelope is the structured content of the result.
    Read,
    /// A write: the payload of the envelope is lifted next to it.
    Write,
}

impl AnswerShape {
    /// Shape a successful answer.
    pub(crate) fn success(self, envelope: &Envelope) -> CallToolResult {
        match self {
            Self::Read => result::envelope_result(envelope),
            Self::Write => result::write_result(envelope),
        }
    }

    /// Shape a failed answer.
    pub(crate) fn failure(self, error: &FigError) -> CallToolResult {
        match self {
            Self::Read => result::error_result(error),
            Self::Write => result::write_error_result(error),
        }
    }
}

/// The service behind the tool surface.
///
/// The service holds the daemon client and answers the tool calls. The read
/// tools and the write tools share it, and both check their answer against
/// the session and the grant epoch on the daemon side.
pub struct FigTetherService {
    client: DaemonClient,
}

impl FigTetherService {
    /// Build the service on top of a daemon client.
    pub fn new(client: DaemonClient) -> Self {
        Self { client }
    }

    /// The daemon client behind the tools.
    pub fn client(&self) -> &DaemonClient {
        &self.client
    }

    /// Send one request to the daemon and shape its answer.
    pub(crate) async fn execute<F>(
        &self,
        build: F,
        timeout: Duration,
        cancel: &dyn CancelSource,
    ) -> CallToolResult
    where
        F: FnOnce(String) -> Result<ClientRequest, FigError>,
    {
        self.execute_shaped(build, AnswerShape::Read, timeout, cancel)
            .await
    }

    /// Send one write request to the daemon and shape its answer.
    ///
    /// A write failure is a business failure: it travels as a structured tool
    /// result with the code of the write taxonomy, never as a protocol error.
    pub(crate) async fn execute_write<F>(
        &self,
        build: F,
        timeout: Duration,
        cancel: &dyn CancelSource,
    ) -> CallToolResult
    where
        F: FnOnce(String) -> Result<ClientRequest, FigError>,
    {
        self.execute_shaped(build, AnswerShape::Write, timeout, cancel)
            .await
    }

    /// Send one request to the daemon and shape its answer.
    async fn execute_shaped<F>(
        &self,
        build: F,
        shape: AnswerShape,
        timeout: Duration,
        cancel: &dyn CancelSource,
    ) -> CallToolResult
    where
        F: FnOnce(String) -> Result<ClientRequest, FigError>,
    {
        let job_id = JobId::new();
        let request_id = job_id.to_string();
        let request = match build(request_id.clone()) {
            Ok(request) => request,
            Err(error) => return shape.failure(&error.with_request_id(request_id)),
        };
        match self
            .client
            .call(request, job_id, timeout, cancel.cancelled())
            .await
        {
            Ok(ClientResponse::Ok { envelope, .. }) => shape.success(&envelope),
            Ok(ClientResponse::Error { error, .. }) => {
                // A daemon that stayed silent about the correlation id gets
                // the one of the request that asked.
                let error = if error.request_id.trim().is_empty() {
                    error.with_request_id(request_id.clone())
                } else {
                    error
                };
                shape.failure(&error)
            }
            Ok(ClientResponse::Accepted { job_id, .. }) => {
                self.await_job(job_id, shape, timeout, cancel).await
            }
            Err(error) => shape.failure(&result::daemon_error(&error, &request_id)),
        }
    }

    /// Wait for the result of a job the daemon accepted.
    async fn await_job(
        &self,
        job_id: JobId,
        shape: AnswerShape,
        timeout: Duration,
        cancel: &dyn CancelSource,
    ) -> CallToolResult {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                let error = FigError::new(
                    ErrorCode::Timeout,
                    format!("the daemon job {job_id} did not finish in time"),
                )
                .with_request_id(job_id.to_string());
                return shape.failure(&error);
            }
            let request_id = JobId::new().to_string();
            let request = ClientRequest::GetJobStatus {
                request_id: request_id.clone(),
                params: serde_json::json!({ "job_id": job_id.to_string() }),
            };
            match self
                .client
                .call(request, job_id, remaining, cancel.cancelled())
                .await
            {
                Ok(ClientResponse::Ok { envelope, .. }) => return shape.success(&envelope),
                Ok(ClientResponse::Error { error, .. }) => return shape.failure(&error),
                Ok(ClientResponse::Accepted { .. }) => {}
                Err(McpError::DaemonTimeout(_)) => {}
                Err(error) => {
                    return shape.failure(&result::daemon_error(&error, &request_id));
                }
            }
            tokio::select! {
                () = cancel.cancelled() => {
                    let error = result::daemon_error(&McpError::Cancelled, &request_id);
                    return shape.failure(&error);
                }
                () = tokio::time::sleep(JOB_POLL_INTERVAL) => {}
            }
        }
    }
}

#[rmcp::tool_router(router = tool_router, vis = "pub")]
impl FigTetherService {
    /// Report the health of the local bridge and the contract versions in
    /// force. The answer carries no secret and no design content.
    #[rmcp::tool(
        name = "get_bridge_status",
        description = "Report the health of the local FigTether bridge, the schema version, and the protocol version in force. The answer carries no secret and no design content and works while the editor is closed.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_bridge_status(
        &self,
        Parameters(_params): Parameters<GetBridgeStatusParams>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.execute(
            |request_id| Ok(ClientRequest::Status { request_id }),
            STATUS_TIMEOUT,
            &context,
        )
        .await
    }

    /// List the sessions this client may see.
    #[rmcp::tool(
        name = "list_sessions",
        description = "List the live FigTether sessions that this client may see, with their granted scope roots and their runtime capabilities. A session that is paused or revoked is listed with that status. The answer is paginated.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn list_sessions(
        &self,
        Parameters(params): Parameters<ListSessionsParams>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        let cursor = params.cursor;
        self.execute(
            move |request_id| Ok(ClientRequest::ListSessions { request_id, cursor }),
            READ_TIMEOUT,
            &context,
        )
        .await
    }

    /// Show which nodes the user currently selects.
    #[rmcp::tool(
        name = "get_selection",
        description = "Summarize the nodes the user currently selects in a live session, limited to the granted scope. A selection outside the grant is never leaked and never counted. Use the result to pick the roots of the following reads.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_selection(
        &self,
        Parameters(params): Parameters<GetSelectionParams>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        let payload = encode(&params);
        self.execute(
            move |request_id| {
                Ok(ClientRequest::GetSelection {
                    request_id,
                    params: payload?,
                })
            },
            READ_TIMEOUT,
            &context,
        )
        .await
    }

    /// Walk the layer tree inside a granted root.
    #[rmcp::tool(
        name = "list_nodes",
        description = "List the children of a node, or walk a subtree of a granted root, with a field projection, a depth bound, and a pagination cursor. The answer states the coverage of the traversal and never hides a truncation. Name either a session_id or a snapshot_id, never both.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn list_nodes(
        &self,
        Parameters(params): Parameters<ListNodesParams>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        let payload = encode(&params);
        self.execute(
            move |request_id| {
                Ok(ClientRequest::ListNodes {
                    request_id,
                    params: payload?,
                })
            },
            READ_TIMEOUT,
            &context,
        )
        .await
    }

    /// Read the full detail of a bounded batch of nodes.
    #[rmcp::tool(
        name = "get_nodes",
        description = "Read the normalized detail of at most fifty nodes of one session or snapshot, with an optional depth bound, a field projection, and an explicit consistency lane. A node that is unknown or out of scope is reported per item. Name either a session_id or a snapshot_id, never both.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_nodes(
        &self,
        Parameters(params): Parameters<GetNodesParams>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        let payload = encode(&params);
        self.execute(
            move |request_id| {
                Ok(ClientRequest::GetNodes {
                    request_id,
                    params: payload?,
                })
            },
            READ_TIMEOUT,
            &context,
        )
        .await
    }

    /// Inspect a subtree for implementation context.
    #[rmcp::tool(
        name = "inspect_design",
        description = "Inspect a subtree of a granted root and return the implementation context an agent needs: layout, typography, paints, effects, component references, the features the platform did not expose, and the asset references. The detail level bounds the answer. Name either a session_id or a snapshot_id, never both.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn inspect_design(
        &self,
        Parameters(params): Parameters<InspectDesignParams>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        let payload = encode(&params);
        self.execute(
            move |request_id| {
                Ok(ClientRequest::InspectDesign {
                    request_id,
                    params: payload?,
                })
            },
            READ_TIMEOUT,
            &context,
        )
        .await
    }

    /// Read the styles and variables in scope.
    #[rmcp::tool(
        name = "get_design_tokens",
        description = "Read the styles and the variables that are in scope of a granted root, with their collections, modes, values, and resolved aliases. An alias that is unresolved, cyclic, or ambiguous is reported, never guessed. Name either a session_id or a snapshot_id, never both.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_design_tokens(
        &self,
        Parameters(params): Parameters<GetDesignTokensParams>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        let payload = encode(&params);
        self.execute(
            move |request_id| {
                Ok(ClientRequest::GetDesignTokens {
                    request_id,
                    params: payload?,
                })
            },
            READ_TIMEOUT,
            &context,
        )
        .await
    }

    /// Export nodes as PNG or SVG.
    #[rmcp::tool(
        name = "export_assets",
        description = "Export granted nodes as PNG or SVG and return asset metadata. Call get_asset with the returned asset_id and session_id to receive a PNG as an MCP image block. PNG delivery is limited to 1 MiB of image bytes and expires after five minutes or a grant change; lower the scale or export a section if needed. SVG byte delivery is unavailable. Snapshot storage is unavailable in this daemon; use a live session_id.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn export_assets(
        &self,
        Parameters(params): Parameters<ExportAssetsParams>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        let payload = encode(&params);
        self.execute(
            move |request_id| {
                Ok(ClientRequest::ExportAssets {
                    request_id,
                    params: payload?,
                })
            },
            EXPORT_TIMEOUT,
            &context,
        )
        .await
    }

    /// Describe a stored capture.
    #[rmcp::tool(
        name = "get_snapshot_info",
        description = "Reserved snapshot contract. The shipped daemon has no snapshot store and returns CAPABILITY_UNAVAILABLE. Use open_design or bounded live-session reads instead.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_snapshot_info(
        &self,
        Parameters(params): Parameters<GetSnapshotInfoParams>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        let payload = encode(&params);
        self.execute(
            move |request_id| {
                Ok(ClientRequest::GetSnapshotInfo {
                    request_id,
                    params: payload?,
                })
            },
            READ_TIMEOUT,
            &context,
        )
        .await
    }

    /// Subscribe to the invalidation of one scoped root.
    #[rmcp::tool(
        name = "watch_scope",
        description = "Reserved watch contract. Watch registration is unavailable in the shipped daemon. Read the relevant granted nodes again after editing instead.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn watch_scope(
        &self,
        Parameters(params): Parameters<WatchScopeParams>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        let payload = encode(&params);
        self.execute(
            move |request_id| {
                Ok(ClientRequest::WatchScope {
                    request_id,
                    params: payload?,
                })
            },
            READ_TIMEOUT,
            &context,
        )
        .await
    }

    /// Drain the journal of a watch.
    #[rmcp::tool(
        name = "get_changes",
        description = "Reserved change-journal contract. Watch/change delivery is unavailable in the shipped daemon. Read the relevant granted nodes again instead.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_changes(
        &self,
        Parameters(params): Parameters<GetChangesParams>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        let payload = encode(&params);
        self.execute(
            move |request_id| {
                Ok(ClientRequest::GetChanges {
                    request_id,
                    params: payload?,
                })
            },
            READ_TIMEOUT,
            &context,
        )
        .await
    }

    /// Describe a long running operation.
    #[rmcp::tool(
        name = "get_job_status",
        description = "Describe a long running operation: whether it is queued, running, succeeded, or failed, with its progress and the reference to its result. This is observability only and never makes a Figma operation transactional.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_job_status(
        &self,
        Parameters(params): Parameters<GetJobStatusParams>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        let payload = encode(&params);
        self.execute(
            move |request_id| {
                Ok(ClientRequest::GetJobStatus {
                    request_id,
                    params: payload?,
                })
            },
            READ_TIMEOUT,
            &context,
        )
        .await
    }
}

#[rmcp::tool_handler(router = crate::tools::tool_router())]
impl rmcp::ServerHandler for FigTetherService {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("figtether-mcp", crate::MCP_VERSION))
            .with_instructions(INSTRUCTIONS.to_string())
    }
}

/// Encode the parameters of a tool for the daemon.
pub(crate) fn encode<T: serde::Serialize>(params: &T) -> Result<serde_json::Value, FigError> {
    serde_json::to_value(params).map_err(|error| {
        FigError::new(
            ErrorCode::InternalError,
            format!("the tool parameters could not be encoded: {error}"),
        )
    })
}

/// The names of the fourteen read tools of the surface.
///
/// The read tools are the ones that never change a document.
pub const READ_TOOL_NAMES: [&str; 14] = [
    "open_design",
    "get_asset",
    "get_bridge_status",
    "list_sessions",
    "get_selection",
    "list_nodes",
    "get_nodes",
    "inspect_design",
    "get_design_tokens",
    "export_assets",
    "get_snapshot_info",
    "watch_scope",
    "get_changes",
    "get_job_status",
];

/// The names of the three tools of the write path.
///
/// The write path is plan based: a proposal, an approval, and an apply.
pub const WRITE_TOOL_NAMES: [&str; 3] = ["preview_write", "get_write_approval", "apply_write"];

/// The names of the seventeen tools of the surface.
pub const TOOL_NAMES: [&str; 17] = [
    "open_design",
    "get_asset",
    "get_bridge_status",
    "list_sessions",
    "get_selection",
    "list_nodes",
    "get_nodes",
    "inspect_design",
    "get_design_tokens",
    "export_assets",
    "get_snapshot_info",
    "watch_scope",
    "get_changes",
    "get_job_status",
    "preview_write",
    "get_write_approval",
    "apply_write",
];

/// The tool router of the surface, built without a daemon.
///
/// The router carries the tool definitions and their schemas; the tools
/// themselves need a daemon client, which the server owns. The read tools
/// and the write tools live in two routers that are merged here.
pub fn tool_router() -> ToolRouter<FigTetherService> {
    FigTetherService::tool_router()
        + FigTetherService::write_tool_router()
        + FigTetherService::canvas_tool_router()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::DaemonClient;
    use crate::endpoint::Endpoint;
    use crate::fake_daemon::FakeDaemon;
    use figtether_model::ids::SessionId;
    use figtether_protocol::client::ClientAuth;
    use figtether_protocol::PROTOCOL_VERSION;

    /// A call that is never cancelled.
    struct Never;

    impl CancelSource for Never {
        fn cancelled(&self) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
            Box::pin(std::future::pending())
        }
    }

    fn router() -> ToolRouter<FigTetherService> {
        tool_router()
    }

    /// The tool of the surface with the given name.
    fn tool(name: &str) -> rmcp::model::Tool {
        router()
            .list_all()
            .into_iter()
            .find(|tool| tool.name == name)
            .unwrap_or_else(|| panic!("{name} is published"))
    }

    /// Whether the tool belongs to the write surface.
    fn is_write_tool(name: &str) -> bool {
        WRITE_TOOL_NAMES.contains(&name)
    }

    /// A plan with one operation, used by the tests of the write path.
    fn sample_plan() -> figtether_model::write::WritePlan {
        figtether_model::write::WritePlan {
            plan_id: "plan_1".to_string(),
            session_id: Some(SessionId::new()),
            operations: vec![figtether_model::write::WriteOperation {
                kind: figtether_model::write::WriteOperationKind::CreateNode,
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
            }],
            intent: Some("add a frame".to_string()),
            destructive: false,
        }
    }

    /// Resolve a schema that the generator put behind a reference.
    fn resolve<'a>(
        root: &'a serde_json::Value,
        value: &'a serde_json::Value,
    ) -> &'a serde_json::Value {
        let Some(reference) = value.get("$ref").and_then(serde_json::Value::as_str) else {
            return value;
        };
        let pointer = reference.strip_prefix("#/").unwrap_or(reference);
        let mut current = root;
        for part in pointer.split('/') {
            let part = part.replace("~1", "/").replace("~0", "~");
            match current.get(&part) {
                Some(next) => current = next,
                None => return value,
            }
        }
        current
    }

    #[test]
    fn the_surface_holds_fourteen_read_tools_and_three_write_tools() {
        let names: Vec<String> = router()
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect();
        assert_eq!(names.len(), 17);
        for expected in TOOL_NAMES {
            assert!(
                names.contains(&expected.to_string()),
                "{expected} is missing"
            );
        }
        assert_eq!(READ_TOOL_NAMES.len(), 14);
        assert_eq!(WRITE_TOOL_NAMES.len(), 3);
    }

    #[test]
    fn every_read_tool_is_read_only() {
        for tool in router().list_all() {
            if is_write_tool(&tool.name) {
                continue;
            }
            let annotations = tool
                .annotations
                .as_ref()
                .unwrap_or_else(|| panic!("{} has no annotations", tool.name));
            assert_eq!(annotations.read_only_hint, Some(true), "{}", tool.name);
            assert_eq!(annotations.destructive_hint, Some(false), "{}", tool.name);
        }
    }

    #[test]
    fn exactly_three_tools_are_annotated_as_mutating() {
        let mutating: Vec<String> = router()
            .list_all()
            .into_iter()
            .filter(|tool| is_write_tool(&tool.name))
            .map(|tool| tool.name.to_string())
            .collect();
        assert_eq!(mutating.len(), 3);
        for expected in WRITE_TOOL_NAMES {
            assert!(
                mutating.contains(&expected.to_string()),
                "{expected} is missing from the write surface"
            );
        }
    }

    #[test]
    fn the_preview_and_the_approval_change_nothing() {
        for name in ["preview_write", "get_write_approval"] {
            let published = tool(name);
            let annotations = published
                .annotations
                .as_ref()
                .unwrap_or_else(|| panic!("{name} has no annotations"));
            assert_eq!(annotations.read_only_hint, Some(true), "{name}");
            assert_eq!(annotations.destructive_hint, Some(false), "{name}");
        }
    }

    #[test]
    fn the_apply_is_the_only_tool_that_changes_the_document() {
        let mutating: Vec<String> = router()
            .list_all()
            .into_iter()
            .filter(|tool| {
                tool.annotations
                    .as_ref()
                    .and_then(|annotations| annotations.read_only_hint)
                    == Some(false)
            })
            .map(|tool| tool.name.to_string())
            .collect();
        assert_eq!(mutating, vec!["apply_write".to_string()]);
        let published = tool("apply_write");
        let annotations = published
            .annotations
            .as_ref()
            .expect("the apply has annotations");
        assert_eq!(annotations.read_only_hint, Some(false));
        assert_eq!(annotations.destructive_hint, Some(true));
    }

    #[test]
    fn every_mutating_tool_requires_an_approval_token() {
        for tool in router().list_all() {
            let annotations = tool
                .annotations
                .as_ref()
                .unwrap_or_else(|| panic!("{} has no annotations", tool.name));
            if annotations.read_only_hint != Some(false) {
                continue;
            }
            let schema = tool.schema_as_json_value();
            let properties = schema["properties"].as_object().expect("properties");
            assert!(
                properties.contains_key("approval_token"),
                "{} takes no approval token",
                tool.name
            );
            let required = schema["required"].as_array().expect("required");
            assert!(
                required.contains(&serde_json::json!("approval_token")),
                "{} does not require the approval token",
                tool.name
            );
        }
    }

    #[test]
    fn the_input_schema_of_a_tool_comes_from_the_model() {
        let tools = router().list_all();
        let list_nodes = tools
            .iter()
            .find(|tool| tool.name == "list_nodes")
            .expect("list_nodes is exposed");
        let schema = list_nodes.schema_as_json_value();
        assert_eq!(schema["type"], "object");
        let properties = schema["properties"].as_object().expect("properties");
        assert!(properties.contains_key("session_id"));
        assert!(properties.contains_key("snapshot_id"));
        assert!(properties.contains_key("root_id"));
        assert!(properties.contains_key("cursor"));
        assert!(properties.contains_key("depth"));
        assert!(properties.contains_key("fields"));
        assert!(properties.contains_key("limit"));
        assert!(properties.contains_key("consistency"));
        assert!(properties.contains_key("max_age_ms"));
        let required = schema["required"].as_array().expect("required");
        assert!(required.contains(&serde_json::json!("root_id")));
    }

    #[test]
    fn the_schema_of_the_preview_describes_the_plan() {
        let schema = tool("preview_write").schema_as_json_value();
        assert_eq!(schema["type"], "object");
        let properties = schema["properties"].as_object().expect("properties");
        assert!(properties.contains_key("session_id"));
        assert!(properties.contains_key("plan"));
        let plan = resolve(&schema, &properties["plan"]);
        let plan_properties = plan["properties"].as_object().expect("plan properties");
        assert!(plan_properties.contains_key("plan_id"));
        assert!(plan_properties.contains_key("operations"));
        assert!(plan_properties.contains_key("intent"));
        assert!(plan_properties.contains_key("destructive"));
        let required = schema["required"].as_array().expect("required");
        assert!(required.contains(&serde_json::json!("session_id")));
        assert!(required.contains(&serde_json::json!("plan")));
    }

    #[test]
    fn the_schema_of_the_apply_requires_the_plan_and_the_token() {
        let schema = tool("apply_write").schema_as_json_value();
        let properties = schema["properties"].as_object().expect("properties");
        assert!(properties.contains_key("plan"));
        assert!(properties.contains_key("approval_token"));
        let required = schema["required"].as_array().expect("required");
        assert!(required.contains(&serde_json::json!("plan")));
        assert!(required.contains(&serde_json::json!("approval_token")));
    }

    #[test]
    fn the_schema_of_the_approval_names_the_approval_id() {
        let schema = tool("get_write_approval").schema_as_json_value();
        let properties = schema["properties"].as_object().expect("properties");
        assert!(properties.contains_key("approval_id"));
        let required = schema["required"].as_array().expect("required");
        assert!(required.contains(&serde_json::json!("approval_id")));
    }

    #[test]
    fn the_read_tools_take_no_write_like_parameter() {
        for tool in router().list_all() {
            if is_write_tool(&tool.name) {
                continue;
            }
            let schema = tool.schema_as_json_value();
            let properties = schema["properties"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            for name in properties.keys() {
                let lowered = name.to_lowercase();
                for banned in [
                    "secret",
                    "credential",
                    "token",
                    "endpoint",
                    "port",
                    "url",
                    "grant",
                    "plan",
                    "operation",
                    "approval",
                    "destructive",
                    "intent",
                ] {
                    assert!(!lowered.contains(banned), "{} takes {name}", tool.name);
                }
            }
        }
    }

    #[test]
    fn the_write_tools_take_no_secret_and_no_endpoint() {
        for tool in router().list_all() {
            if !is_write_tool(&tool.name) {
                continue;
            }
            let schema = tool.schema_as_json_value();
            let properties = schema["properties"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            for name in properties.keys() {
                let lowered = name.to_lowercase();
                for banned in ["secret", "credential", "endpoint", "port", "url", "grant"] {
                    assert!(!lowered.contains(banned), "{} takes {name}", tool.name);
                }
            }
        }
    }

    #[test]
    fn the_parameters_of_a_tool_round_trip_into_the_wire_contract() {
        let params = ListNodesParams {
            session_id: Some(figtether_model::ids::SessionId::new()),
            snapshot_id: None,
            root_id: "1:2".to_string(),
            cursor: None,
            depth: Some(3),
            fields: vec!["id".to_string()],
            limit: Some(10),
            consistency: figtether_model::tools::ConsistencyRequest::Fresh,
            max_age_ms: None,
        };
        let value = encode(&params).expect("encodes");
        assert_eq!(value["root_id"], "1:2");
        assert_eq!(value["depth"], 3);
        assert_eq!(value["consistency"], "fresh");
        assert!(value["session_id"].is_string());
        assert!(value.get("snapshot_id").is_none());
    }

    #[tokio::test]
    async fn a_tool_call_reaches_the_daemon_and_comes_back_as_an_envelope() {
        let daemon = FakeDaemon::start(true).await;
        let client = DaemonClient::new(
            Endpoint::new("127.0.0.1", daemon.port, "/client"),
            ClientAuth {
                credential: "s3cret".to_string(),
                client_id: "test-client".to_string(),
                protocol_version: PROTOCOL_VERSION.to_string(),
            },
        )
        .with_auth_timeout(Duration::from_millis(50));
        let service = FigTetherService::new(client);
        let params = ListNodesParams {
            session_id: Some(SessionId::new()),
            snapshot_id: None,
            root_id: "1:2".to_string(),
            cursor: None,
            depth: Some(2),
            fields: vec!["id".to_string()],
            limit: None,
            consistency: figtether_model::tools::ConsistencyRequest::Fresh,
            max_age_ms: None,
        };
        let result = service
            .execute(
                |request_id| {
                    Ok(ClientRequest::ListNodes {
                        request_id,
                        params: encode(&params)?,
                    })
                },
                READ_TIMEOUT,
                &Never,
            )
            .await;
        assert_eq!(result.is_error, Some(false));
        let structured = result
            .structured_content
            .as_ref()
            .expect("the envelope is the structured content");
        assert_eq!(structured["source"]["kind"], "plugin_live");
        assert_eq!(structured["coverage"]["root_ids"][0], "1:2");
        assert_eq!(structured["data"]["nodes"], serde_json::json!([]));
        let frames = daemon.wait_for_frames(2).await;
        assert_eq!(frames[1]["type"], "list_nodes");
        assert_eq!(frames[1]["params"]["root_id"], "1:2");
        assert_eq!(frames[1]["params"]["depth"], 2);
    }

    #[tokio::test]
    async fn a_write_call_reaches_the_daemon_and_comes_back_as_a_preview() {
        let daemon = FakeDaemon::start(true).await;
        let client = DaemonClient::new(
            Endpoint::new("127.0.0.1", daemon.port, "/client"),
            ClientAuth {
                credential: "s3cret".to_string(),
                client_id: "test-client".to_string(),
                protocol_version: PROTOCOL_VERSION.to_string(),
            },
        )
        .with_auth_timeout(Duration::from_millis(50));
        let service = FigTetherService::new(client);
        let params = crate::tools_write::PreviewWriteParams {
            session_id: SessionId::new(),
            plan: sample_plan(),
        };
        let result = service
            .execute_write(
                |request_id| {
                    Ok(ClientRequest::PreviewWrite {
                        request_id,
                        params: encode(&params)?,
                    })
                },
                WRITE_TIMEOUT,
                &Never,
            )
            .await;
        assert_eq!(result.is_error, Some(false));
        let structured = result
            .structured_content
            .as_ref()
            .expect("the preview is the structured content");
        assert_eq!(structured["source"]["kind"], "plugin_live");
        assert_eq!(structured["operations_count"], 1);
        assert_eq!(structured["blocked"], false);
        assert_eq!(structured["approval_id"], "apr_1");
        assert_eq!(structured["created"], serde_json::json!(["1:9"]));
        assert_eq!(
            structured["plan_warnings"],
            serde_json::json!(["the parent is a component instance"])
        );
        let frames = daemon.wait_for_frames(2).await;
        assert_eq!(frames[1]["type"], "preview_write");
        assert_eq!(frames[1]["params"]["plan"]["plan_id"], "plan_1");
    }

    #[tokio::test]
    async fn an_apply_names_the_session_of_the_approval() {
        let daemon = FakeDaemon::start(true).await;
        let client = DaemonClient::new(
            Endpoint::new("127.0.0.1", daemon.port, "/client"),
            ClientAuth {
                credential: "s3cret".to_string(),
                client_id: "test-client".to_string(),
                protocol_version: PROTOCOL_VERSION.to_string(),
            },
        )
        .with_auth_timeout(Duration::from_millis(50));
        let service = FigTetherService::new(client);
        let session = SessionId::new();
        let token = figtether_protocol::write::approval_token(
            "apr_1",
            "abc123",
            &session,
            3,
            chrono::Utc::now(),
        );
        let mut plan = sample_plan();
        plan.session_id = None;
        let params = crate::tools_write::ApplyWriteParams {
            plan,
            approval_token: token.clone(),
            session_id: None,
        };
        service
            .execute_write(
                |request_id| {
                    Ok(ClientRequest::ApplyWrite {
                        request_id,
                        params: crate::tools_write::apply_payload(&params)?,
                    })
                },
                WRITE_TIMEOUT,
                &Never,
            )
            .await;
        let frames = daemon.wait_for_frames(2).await;
        assert_eq!(frames[1]["type"], "apply_write");
        assert_eq!(frames[1]["params"]["session_id"], session.to_string());
        assert_eq!(frames[1]["params"]["approval_token"], token);
    }

    #[tokio::test]
    async fn a_refused_write_comes_back_as_a_structured_error() {
        let daemon = FakeDaemon::start_failing("WRITE_NOT_GRANTED").await;
        let client = DaemonClient::new(
            Endpoint::new("127.0.0.1", daemon.port, "/client"),
            ClientAuth {
                credential: "s3cret".to_string(),
                client_id: "test-client".to_string(),
                protocol_version: PROTOCOL_VERSION.to_string(),
            },
        )
        .with_auth_timeout(Duration::from_millis(50));
        let service = FigTetherService::new(client);
        let params = crate::tools_write::ApplyWriteParams {
            plan: sample_plan(),
            approval_token: "w:apr_1:hash:session:3:2026-10-20T08:00:00Z".to_string(),
            session_id: None,
        };
        let result = service
            .execute_write(
                |request_id| {
                    Ok(ClientRequest::ApplyWrite {
                        request_id,
                        params: encode(&params)?,
                    })
                },
                WRITE_TIMEOUT,
                &Never,
            )
            .await;
        assert_eq!(result.is_error, Some(true));
        let structured = result
            .structured_content
            .as_ref()
            .expect("the error is the structured content");
        assert_eq!(structured["code"], "WRITE_NOT_GRANTED");
        assert_eq!(structured["retryable"], false);
        assert!(structured["action"].is_string());
        let frames = daemon.wait_for_frames(2).await;
        assert_eq!(frames[1]["type"], "apply_write");
        assert_eq!(frames[1]["params"]["approval_token"], params.approval_token);
    }

    #[tokio::test]
    async fn a_daemon_that_is_not_running_becomes_a_structured_error() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("a free port");
        let port = listener.local_addr().expect("an address").port();
        drop(listener);
        let client = DaemonClient::new(
            Endpoint::new("127.0.0.1", port, "/client"),
            ClientAuth {
                credential: "s3cret".to_string(),
                client_id: "test-client".to_string(),
                protocol_version: PROTOCOL_VERSION.to_string(),
            },
        )
        .with_connect_timeout(Duration::from_millis(500))
        .with_auth_timeout(Duration::from_millis(10));
        let service = FigTetherService::new(client);
        let result = service
            .execute(
                |request_id| Ok(ClientRequest::Status { request_id }),
                STATUS_TIMEOUT,
                &Never,
            )
            .await;
        assert_eq!(result.is_error, Some(true));
        let structured = result
            .structured_content
            .as_ref()
            .expect("the error is the structured content");
        assert_eq!(structured["code"], "AUTH_REQUIRED");
        assert!(structured["action"]
            .as_str()
            .unwrap_or_default()
            .contains("daemon"));
    }
}
