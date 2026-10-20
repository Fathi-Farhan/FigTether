//! A JSON-RPC over stdio MCP server used by the MCP lifecycle test.
//!
//! The server speaks the line delimited JSON-RPC dialect of the MCP stdio
//! transport: one JSON object per line on stdin, one JSON object per line on
//! stdout, and diagnostics on stderr only. The tool surface is the frozen
//! read only surface of `docs/04-data-contracts/02-tool-catalog.md`, and every
//! tool is marked `readOnlyHint` and not `destructiveHint`.
//!
//! This module is a **stand-in** for `figtether-mcp`. The shipped adapter is
//! exercised by the acceptance tests of `crates/e2e/tests/acceptance.rs`:
//! the handshake, the server info, the tools capability and the twelve read
//! only tools are proven against `figtether_mcp::McpServer`.
//!
//! The stand-in stays because of one behaviour only it can produce: the
//! **separation of the two streams of a stdio transport**. The shipped
//! adapter serves one transport, so a test that drives it over a pipe cannot
//! tell protocol traffic from diagnostic traffic. The rule of `AT14` that
//! stdout carries the protocol only, and that a notification produces no
//! answer but a diagnostic on stderr, needs a server that writes to two
//! separate streams, and that is what this module is.

use figtether_model::errors::{ErrorCode, FigError};
use figtether_model::tools::{
    ExportAssetsParams, GetDesignTokensParams, GetNodesParams, GetSelectionParams,
    GetSnapshotInfoParams, InspectDesignParams, ListNodesParams,
};
use figtether_protocol::client::{ClientRequest, ClientResponse};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream};

use crate::fake_client::FakeLocalClient;
use crate::harness::HarnessError;

/// The protocol version the harness server negotiates.
pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// The frozen tool surface of the read only catalog.
pub const MCP_TOOLS: [&str; 12] = [
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

/// The stdio side of the MCP server.
pub struct McpStdioHandle {
    stdin: DuplexStream,
    stdout: BufReader<DuplexStream>,
    stderr: BufReader<DuplexStream>,
}

impl McpStdioHandle {
    /// Write one JSON-RPC message to the server.
    pub async fn write(&mut self, message: &Value) -> Result<(), HarnessError> {
        let line = serde_json::to_string(message)
            .map_err(|err| HarnessError::Protocol(err.to_string()))?;
        self.stdin
            .write_all(format!("{line}\n").as_bytes())
            .await
            .map_err(HarnessError::Io)?;
        self.stdin.flush().await.map_err(HarnessError::Io)
    }

    /// Write a request and read its answer.
    pub async fn call(
        &mut self,
        id: u64,
        method: &str,
        params: Value,
    ) -> Result<Value, HarnessError> {
        self.write(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))
        .await?;
        loop {
            let message = self.read_stdout().await?;
            if message.get("id").and_then(Value::as_u64) == Some(id) {
                return Ok(message);
            }
        }
    }

    /// Read the next line of the stdout stream.
    pub async fn read_stdout(&mut self) -> Result<Value, HarnessError> {
        let mut line = String::new();
        let read = self
            .stdout
            .read_line(&mut line)
            .await
            .map_err(HarnessError::Io)?;
        if read == 0 {
            return Err(HarnessError::Connection(
                "the MCP server closed stdout".to_string(),
            ));
        }
        serde_json::from_str(line.trim_end()).map_err(|err| HarnessError::Protocol(err.to_string()))
    }

    /// Read the next line of the stderr stream.
    pub async fn read_stderr(&mut self) -> Result<String, HarnessError> {
        let mut line = String::new();
        let read = self
            .stderr
            .read_line(&mut line)
            .await
            .map_err(HarnessError::Io)?;
        if read == 0 {
            return Err(HarnessError::Connection(
                "the MCP server closed stderr".to_string(),
            ));
        }
        Ok(line.trim_end().to_string())
    }
}

/// Start the harness MCP server on a pair of in-memory pipes.
pub fn spawn(client: FakeLocalClient) -> McpStdioHandle {
    let (client_stdin, server_stdin) = tokio::io::duplex(64 * 1024);
    let (server_stdout, client_stdout) = tokio::io::duplex(64 * 1024);
    let (server_stderr, client_stderr) = tokio::io::duplex(64 * 1024);

    tokio::spawn(async move {
        let mut reader = BufReader::new(server_stdin);
        let mut stdout = server_stdout;
        let mut stderr = server_stderr;
        let mut client = client;
        loop {
            let mut line = String::new();
            let Ok(read) = reader.read_line(&mut line).await else {
                break;
            };
            if read == 0 {
                break;
            }
            let trimmed = line.trim_end();
            if trimmed.is_empty() {
                continue;
            }
            let message: Value = match serde_json::from_str(trimmed) {
                Ok(message) => message,
                Err(err) => {
                    let diagnostic = format!(
                        "{}\n",
                        json!({ "level": "error", "message": err.to_string() })
                    );
                    let _ = stderr.write_all(diagnostic.as_bytes()).await;
                    continue;
                }
            };
            let id = message.get("id").cloned();
            let method = message
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let params = message.get("params").cloned().unwrap_or(Value::Null);
            if id.is_none() {
                // A notification has no answer, only a diagnostic.
                let diagnostic = format!(
                    "{}\n",
                    json!({ "level": "debug", "message": format!("notification {method}") })
                );
                let _ = stderr.write_all(diagnostic.as_bytes()).await;
                continue;
            }
            let id = id.unwrap_or(Value::Null);
            let response = match method.as_str() {
                "initialize" => json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocolVersion": MCP_PROTOCOL_VERSION,
                        "capabilities": {
                            "tools": { "listChanged": false },
                            "resources": { "subscribe": false, "listChanged": false },
                        },
                        "serverInfo": { "name": "figtether-e2e", "version": "0.1.0" },
                    },
                }),
                "ping" => json!({ "jsonrpc": "2.0", "id": id, "result": {} }),
                "tools/list" => json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "tools": tool_definitions() },
                }),
                "resources/list" => json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": { "resources": [] },
                }),
                "tools/call" => {
                    let name = params
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
                    match call_tool(&mut client, &name, arguments).await {
                        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
                        Err(error) => json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": {
                                "code": -32000,
                                "message": error.message,
                                "data": { "code": error.code.as_str() },
                            },
                        }),
                    }
                }
                other => json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": -32601, "message": format!("unknown method {other}") },
                }),
            };
            let encoded = serde_json::to_string(&response).unwrap_or_default();
            if stdout
                .write_all(format!("{encoded}\n").as_bytes())
                .await
                .is_err()
            {
                break;
            }
            let _ = stdout.flush().await;
        }
    });

    McpStdioHandle {
        stdin: client_stdin,
        stdout: BufReader::new(client_stdout),
        stderr: BufReader::new(client_stderr),
    }
}

/// The definitions of the read only tool surface.
fn tool_definitions() -> Vec<Value> {
    MCP_TOOLS
        .iter()
        .map(|name| {
            json!({
                "name": name,
                "description": format!("the read only tool {name}"),
                "inputSchema": { "type": "object", "properties": {} },
                "annotations": {
                    "readOnlyHint": true,
                    "destructiveHint": false,
                    "idempotentHint": true,
                    "openWorldHint": false,
                },
            })
        })
        .collect()
}

/// Call one tool of the read only surface.
async fn call_tool(
    client: &mut FakeLocalClient,
    name: &str,
    arguments: Value,
) -> Result<Value, FigError> {
    let request = match name {
        "get_bridge_status" => ClientRequest::Status {
            request_id: client.next_request_id(),
        },
        "list_sessions" => ClientRequest::ListSessions {
            request_id: client.next_request_id(),
            cursor: None,
        },
        "list_nodes" => {
            let params: ListNodesParams =
                serde_json::from_value(arguments).map_err(|err| invalid(err.to_string()))?;
            ClientRequest::ListNodes {
                request_id: client.next_request_id(),
                params: serde_json::to_value(params).unwrap_or(Value::Null),
            }
        }
        "get_nodes" => {
            let params: GetNodesParams =
                serde_json::from_value(arguments).map_err(|err| invalid(err.to_string()))?;
            ClientRequest::GetNodes {
                request_id: client.next_request_id(),
                params: serde_json::to_value(params).unwrap_or(Value::Null),
            }
        }
        "inspect_design" => {
            let params: InspectDesignParams =
                serde_json::from_value(arguments).map_err(|err| invalid(err.to_string()))?;
            ClientRequest::InspectDesign {
                request_id: client.next_request_id(),
                params: serde_json::to_value(params).unwrap_or(Value::Null),
            }
        }
        "get_design_tokens" => {
            let params: GetDesignTokensParams =
                serde_json::from_value(arguments).map_err(|err| invalid(err.to_string()))?;
            ClientRequest::GetDesignTokens {
                request_id: client.next_request_id(),
                params: serde_json::to_value(params).unwrap_or(Value::Null),
            }
        }
        "export_assets" => {
            let params: ExportAssetsParams =
                serde_json::from_value(arguments).map_err(|err| invalid(err.to_string()))?;
            ClientRequest::ExportAssets {
                request_id: client.next_request_id(),
                params: serde_json::to_value(params).unwrap_or(Value::Null),
            }
        }
        "get_selection" => {
            let params: GetSelectionParams =
                serde_json::from_value(arguments).map_err(|err| invalid(err.to_string()))?;
            ClientRequest::GetSelection {
                request_id: client.next_request_id(),
                params: serde_json::to_value(params).unwrap_or(Value::Null),
            }
        }
        "get_snapshot_info" => {
            let params: GetSnapshotInfoParams =
                serde_json::from_value(arguments).map_err(|err| invalid(err.to_string()))?;
            ClientRequest::GetSnapshotInfo {
                request_id: client.next_request_id(),
                params: serde_json::to_value(params).unwrap_or(Value::Null),
            }
        }
        other => {
            return Err(FigError::new(
                ErrorCode::CapabilityUnavailable,
                format!("the tool {other} is not enabled in this build"),
            ))
        }
    };
    match client.request(request).await {
        Ok(ClientResponse::Ok { envelope, .. }) => {
            let payload = serde_json::to_value(&envelope).unwrap_or(Value::Null);
            Ok(json!({
                "content": [{ "type": "text", "text": payload.to_string() }],
                "structuredContent": payload,
                "isError": false,
            }))
        }
        Ok(ClientResponse::Error { error, .. }) => Err(error),
        Ok(ClientResponse::Accepted { .. }) => Err(FigError::new(
            ErrorCode::InternalError,
            "the request was accepted in the background",
        )),
        Err(err) => Err(FigError::new(ErrorCode::InternalError, err.to_string())),
    }
}

fn invalid(message: String) -> FigError {
    FigError::new(ErrorCode::InvalidLimit, message)
}
