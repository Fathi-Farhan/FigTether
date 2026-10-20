//! The read only tool surface.
//!
//! The adapter exposes exactly the twelve read tools of the contract. Every
//! tool is read only: it declares `readOnlyHint=true` and
//! `destructiveHint=false`, it names either a session or a snapshot, and it
//! never accepts a parameter that widens a grant, carries a secret, or names
//! a bridge endpoint.
//!
//! The input schema of every tool is generated from the parameter structs of
//! `figtether_model::tools`, so the published contract and the wire contract
//! can never drift apart.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

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

/// Interval between two polls of a background job.
pub const JOB_POLL_INTERVAL: Duration = Duration::from_millis(200);

/// The instructions handed to the client during the handshake.
pub const INSTRUCTIONS: &str = "FigTether exposes the design of the file the user has open in \
Figma as read only data. Call list_sessions to find a session, get_selection to find the \
selected nodes, and then read the tree with list_nodes or get_nodes. Every answer carries a \
provenance envelope: source, observation, coverage, data, and warnings. The tools never change a \
document, never widen a grant, and never expose a secret.";

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

/// The read only service behind the tool surface.
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
    async fn execute<F>(
        &self,
        build: F,
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
            Err(error) => return result::error_result(&error.with_request_id(request_id)),
        };
        match self
            .client
            .call(request, job_id, timeout, cancel.cancelled())
            .await
        {
            Ok(ClientResponse::Ok { envelope, .. }) => result::envelope_result(&envelope),
            Ok(ClientResponse::Error { error, .. }) => result::error_result(&error),
            Ok(ClientResponse::Accepted { job_id, .. }) => {
                self.await_job(job_id, timeout, cancel).await
            }
            Err(error) => result::error_result(&result::daemon_error(&error, &request_id)),
        }
    }

    /// Wait for the result of a job the daemon accepted.
    async fn await_job(
        &self,
        job_id: JobId,
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
                return result::error_result(&error);
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
                Ok(ClientResponse::Ok { envelope, .. }) => {
                    return result::envelope_result(&envelope)
                }
                Ok(ClientResponse::Error { error, .. }) => return result::error_result(&error),
                Ok(ClientResponse::Accepted { .. }) => {}
                Err(McpError::DaemonTimeout(_)) => {}
                Err(error) => {
                    return result::error_result(&result::daemon_error(&error, &request_id));
                }
            }
            tokio::select! {
                () = cancel.cancelled() => {
                    let error = result::daemon_error(&McpError::Cancelled, &request_id);
                    return result::error_result(&error);
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
        description = "Export nodes of a granted root as PNG or SVG and return one asset per node with its content hash, its MIME type, its byte size, and an opaque grant bound resource URI. A node that cannot be exported is reported per item. Name either a session_id or a snapshot_id, never both.",
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
        description = "Describe an immutable FigTether capture: its capture window, its stable content hash, its origin session and grant, its coverage, and its idle expiry. The answer works while the editor is closed and is never labeled live.",
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
        description = "Register a watch on one granted root of a live session and return the watch id with its first cursor. The watch is an invalidation source, never a truth source: read the changed nodes again after an event.",
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
        description = "Read the change events of a watch: the ids of the changed nodes and the tombstones of the deleted ones. A cursor that the journal can no longer serve answers RESYNC_REQUIRED or CURSOR_EXPIRED. The journal never carries full payloads.",
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

#[rmcp::tool_handler(router = FigTetherService::tool_router())]
impl rmcp::ServerHandler for FigTetherService {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("figtether-mcp", crate::MCP_VERSION))
            .with_instructions(INSTRUCTIONS.to_string())
    }
}

/// Encode the parameters of a tool for the daemon.
fn encode<T: serde::Serialize>(params: &T) -> Result<serde_json::Value, FigError> {
    serde_json::to_value(params).map_err(|error| {
        FigError::new(
            ErrorCode::InternalError,
            format!("the tool parameters could not be encoded: {error}"),
        )
    })
}

/// The names of the twelve tools of the contract.
pub const TOOL_NAMES: [&str; 12] = [
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

/// The tool router of the read only surface, built without a daemon.
///
/// The router carries the tool definitions and their schemas; the tools
/// themselves need a daemon client, which the server owns.
pub fn tool_router() -> ToolRouter<FigTetherService> {
    FigTetherService::tool_router()
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

    #[test]
    fn the_surface_holds_exactly_the_twelve_tools() {
        let names: Vec<String> = router()
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect();
        assert_eq!(names.len(), 12);
        for expected in TOOL_NAMES {
            assert!(
                names.contains(&expected.to_string()),
                "{expected} is missing"
            );
        }
    }

    #[test]
    fn every_tool_is_read_only() {
        for tool in router().list_all() {
            let annotations = tool
                .annotations
                .as_ref()
                .unwrap_or_else(|| panic!("{} has no annotations", tool.name));
            assert_eq!(annotations.read_only_hint, Some(true), "{}", tool.name);
            assert_eq!(annotations.destructive_hint, Some(false), "{}", tool.name);
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
    fn no_tool_takes_a_secret_or_an_endpoint() {
        for tool in router().list_all() {
            let schema = tool.schema_as_json_value();
            let properties = schema["properties"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            for name in properties.keys() {
                let lowered = name.to_lowercase();
                assert!(!lowered.contains("secret"), "{} takes {name}", tool.name);
                assert!(
                    !lowered.contains("credential"),
                    "{} takes {name}",
                    tool.name
                );
                assert!(!lowered.contains("token"), "{} takes {name}", tool.name);
                assert!(!lowered.contains("endpoint"), "{} takes {name}", tool.name);
                assert!(!lowered.contains("port"), "{} takes {name}", tool.name);
                assert!(!lowered.contains("url"), "{} takes {name}", tool.name);
                assert!(!lowered.contains("grant"), "{} takes {name}", tool.name);
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
