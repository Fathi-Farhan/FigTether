//! The end to end check of the server.
//!
//! The test drives the adapter over an in memory pipe with the raw JSON-RPC
//! of the MCP specification: the handshake, the tool list, and the shutdown.
//! The client feature of the SDK is not part of the enabled feature set of
//! this crate, so the test speaks the protocol directly.

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use serde_json::{json, Value};
    use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader};

    use crate::{Endpoint, McpServer};

    fn server() -> McpServer {
        McpServer::builder()
            .with_credential("test-credential", "test-client")
            .with_endpoint(Endpoint::parse("ws://127.0.0.1:1/client").expect("an endpoint"))
            .with_connect_timeout(Duration::from_millis(200))
            .with_auth_timeout(Duration::from_millis(1))
            .build()
            .expect("the server is built")
    }

    async fn write_line<W: AsyncWrite + Unpin>(writer: &mut W, value: &Value) {
        let mut line = serde_json::to_vec(value).expect("the message serializes");
        line.push(b'\n');
        writer
            .write_all(&line)
            .await
            .expect("the message is written");
        writer.flush().await.expect("the message is flushed");
    }

    async fn read_line<R: AsyncBufRead + Unpin>(reader: &mut R) -> Value {
        let mut line = String::new();
        let read = tokio::time::timeout(Duration::from_secs(10), reader.read_line(&mut line))
            .await
            .expect("the server answers in time")
            .expect("the answer is read");
        assert!(read > 0, "the server closed the connection");
        serde_json::from_str(line.trim()).expect("the answer is JSON")
    }

    #[tokio::test]
    async fn the_server_answers_the_initialize_handshake() {
        let (server_side, client_side) = tokio::io::duplex(1024 * 1024);
        let (reader, writer) = tokio::io::split(client_side);
        let mut reader = BufReader::new(reader);
        let mut writer = writer;
        let handle = tokio::spawn(async move { server().serve(server_side).await });

        write_line(
            &mut writer,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": { "name": "figtether-test", "version": "0.0.1" }
                }
            }),
        )
        .await;

        let answer = read_line(&mut reader).await;
        assert_eq!(answer["jsonrpc"], "2.0");
        assert_eq!(answer["id"], 1);
        let result = &answer["result"];
        assert!(
            result["protocolVersion"].is_string(),
            "the answer names the protocol version: {answer}"
        );
        assert_eq!(result["serverInfo"]["name"], "figtether-mcp");
        assert!(
            result["capabilities"]["tools"].is_object(),
            "the server advertises the tools: {answer}"
        );
        assert!(
            result["instructions"]
                .as_str()
                .unwrap_or_default()
                .contains("FigTether"),
            "the server explains the surface: {answer}"
        );

        write_line(
            &mut writer,
            &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        )
        .await;
        write_line(
            &mut writer,
            &json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        )
        .await;

        let answer = read_line(&mut reader).await;
        assert_eq!(answer["id"], 2);
        let tools = answer["result"]["tools"]
            .as_array()
            .expect("the tool list is an array");
        assert_eq!(tools.len(), 15);
        for tool in tools {
            let name = tool["name"].as_str().expect("the tool has a name");
            if name == "apply_write" {
                assert_eq!(tool["annotations"]["readOnlyHint"], false, "{tool}");
                assert_eq!(tool["annotations"]["destructiveHint"], true, "{tool}");
                continue;
            }
            assert_eq!(tool["annotations"]["readOnlyHint"], true, "{tool}");
            assert_eq!(tool["annotations"]["destructiveHint"], false, "{tool}");
        }

        drop(writer);
        drop(reader);
        let outcome = tokio::time::timeout(Duration::from_secs(10), handle)
            .await
            .expect("the server stops in time")
            .expect("the server task does not panic");
        assert!(outcome.is_ok(), "{outcome:?}");
    }

    #[tokio::test]
    async fn a_refused_write_is_a_tool_result_and_not_a_protocol_error() {
        let (server_side, client_side) = tokio::io::duplex(1024 * 1024);
        let (reader, writer) = tokio::io::split(client_side);
        let mut reader = BufReader::new(reader);
        let mut writer = writer;
        let handle = tokio::spawn(async move { server().serve(server_side).await });

        write_line(
            &mut writer,
            &json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": { "name": "figtether-test", "version": "0.0.1" }
                }
            }),
        )
        .await;
        let answer = read_line(&mut reader).await;
        assert_eq!(answer["id"], 1);

        write_line(
            &mut writer,
            &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        )
        .await;

        // An apply without an approval token is refused by the adapter
        // itself, before the daemon is asked.
        write_line(
            &mut writer,
            &json!({
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": "apply_write",
                    "arguments": {
                        "plan": {
                            "plan_id": "plan_1",
                            "operations": [
                                { "kind": "create_node", "parent_id": "1:2", "node_type": "FRAME" }
                            ]
                        },
                        "approval_token": "   "
                    }
                }
            }),
        )
        .await;

        let answer = read_line(&mut reader).await;
        assert_eq!(answer["id"], 2);
        assert!(
            answer.get("error").is_none(),
            "a business failure is never a protocol error: {answer}"
        );
        let result = &answer["result"];
        assert_eq!(result["isError"], true, "{answer}");
        assert_eq!(result["structuredContent"]["code"], "APPROVAL_REQUIRED");
        assert!(result["structuredContent"]["action"].is_string());

        // An approval request without an approval id is refused the same
        // way, and the answer names the code of the write taxonomy.
        write_line(
            &mut writer,
            &json!({
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {
                    "name": "get_write_approval",
                    "arguments": { "approval_id": "" }
                }
            }),
        )
        .await;

        let answer = read_line(&mut reader).await;
        assert_eq!(answer["id"], 3);
        assert!(answer.get("error").is_none(), "{answer}");
        assert_eq!(answer["result"]["isError"], true, "{answer}");
        assert_eq!(
            answer["result"]["structuredContent"]["code"],
            "APPROVAL_REQUIRED"
        );

        drop(writer);
        drop(reader);
        let outcome = tokio::time::timeout(Duration::from_secs(10), handle)
            .await
            .expect("the server stops in time")
            .expect("the server task does not panic");
        assert!(outcome.is_ok(), "{outcome:?}");
    }
}
