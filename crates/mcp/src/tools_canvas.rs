//! Model-independent canvas entry point and standard MCP image blocks.
use crate::tools::{CancelSource, FigTetherService, EXPORT_TIMEOUT, READ_TIMEOUT};
use figtether_model::errors::{ErrorCode, FigError};
use figtether_model::tools::{GetAssetParams, OpenDesignParams};
use figtether_protocol::client::ClientRequest;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ContentBlock},
    service::RequestContext,
    RoleServer,
};
use serde_json::{json, Value};
use std::time::Duration;

fn reply(value: Value) -> CallToolResult {
    let text = serde_json::to_string(&value).unwrap_or_default();
    let mut result = CallToolResult::structured(value);
    result.content = vec![ContentBlock::text(text)];
    result
}

fn failed(code: ErrorCode, message: &str) -> CallToolResult {
    crate::result::error_result(&FigError::new(code, message))
}

fn payload(result: &CallToolResult) -> Value {
    result
        .structured_content
        .as_ref()
        .and_then(|v| v.get("data"))
        .cloned()
        .unwrap_or(Value::Null)
}

/// Remove the encoded image from text/structured data before adding its image block.
pub(crate) fn image_result(mut data: Value) -> CallToolResult {
    let encoded = data
        .as_object_mut()
        .and_then(|v| v.remove("image_base64"))
        .and_then(|v| v.as_str().map(str::to_owned));
    let Some(encoded) = encoded else {
        return failed(
            ErrorCode::ExportFailed,
            "no image was delivered; do not claim visual verification",
        );
    };
    let mime = data
        .get("mime")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    if mime != "image/png" {
        return failed(ErrorCode::ExportFailed, "expected a PNG image");
    }
    let mut result = reply(data);
    result.content.push(ContentBlock::image(encoded, mime));
    result
}

impl FigTetherService {
    async fn canvas_call(
        &self,
        kind: &str,
        value: Value,
        timeout: Duration,
        cancel: &dyn CancelSource,
    ) -> CallToolResult {
        self.execute(
            |request_id| {
                Ok(match kind {
                    "sessions" => ClientRequest::ListSessions {
                        request_id,
                        cursor: None,
                    },
                    "nodes" => ClientRequest::GetNodes {
                        request_id,
                        params: value,
                    },
                    "tree" => ClientRequest::ListNodes {
                        request_id,
                        params: value,
                    },
                    "export" => ClientRequest::ExportAssets {
                        request_id,
                        params: value,
                    },
                    "asset" => ClientRequest::GetAsset {
                        request_id,
                        params: value,
                    },
                    _ => {
                        return Err(FigError::new(
                            ErrorCode::InternalError,
                            "unknown canvas operation",
                        ))
                    }
                })
            },
            timeout,
            cancel,
        )
        .await
    }

    pub(crate) async fn asset(
        &self,
        p: GetAssetParams,
        cancel: &dyn CancelSource,
    ) -> CallToolResult {
        let response = self
            .canvas_call(
                "asset",
                serde_json::to_value(p).unwrap_or_default(),
                READ_TIMEOUT,
                cancel,
            )
            .await;
        if response.is_error == Some(true) {
            return response;
        }
        image_result(payload(&response))
    }

    pub(crate) async fn open(
        &self,
        p: OpenDesignParams,
        cancel: &dyn CancelSource,
    ) -> CallToolResult {
        let sessions = self
            .canvas_call("sessions", json!({}), READ_TIMEOUT, cancel)
            .await;
        if sessions.is_error == Some(true) {
            return sessions;
        }
        let session_data = payload(&sessions);
        let mut choices = Vec::new();
        let mut discovery_warnings = Vec::new();
        for session in session_data["sessions"]
            .as_array()
            .into_iter()
            .flatten()
            .take(4)
        {
            let sid = session["session_id"].as_str().unwrap_or_default();
            if p.session_id.is_some_and(|id| id.to_string() != sid) {
                continue;
            }
            if session["state"] != "ready" {
                continue;
            }
            let roots: Vec<_> = session["granted_root_ids"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .filter(|id| p.frame_id.as_deref().is_none_or(|wanted| wanted == *id))
                .take(24)
                .collect();
            if roots.is_empty() {
                continue;
            }
            let nodes = self.canvas_call("nodes",json!({"session_id":sid,"node_ids":roots,"fields":["id","name","type"],"depth":0}),READ_TIMEOUT,cancel).await;
            if nodes.is_error == Some(true) {
                discovery_warnings.push(json!({"session_id":sid,"error":nodes.structured_content}));
                continue;
            }
            let node_data = payload(&nodes);
            for id in roots {
                let node = node_data["nodes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|n| n["id"] == id || n["node_id"] == id);
                let name = node
                    .and_then(|n| {
                        n.get("name")
                            .or_else(|| n.get("properties").and_then(|v| v.get("name")))
                    })
                    .and_then(Value::as_str)
                    .unwrap_or(id);
                if p.frame_name.as_deref().is_some_and(|wanted| wanted != name) {
                    continue;
                }
                choices.push(json!({"session_id":sid,"frame_id":id,"name":name,"grant_mode":session["grant_mode"],"grant_epoch":session["grant_epoch"]}));
            }
        }
        if choices.len() != 1 {
            return reply(
                json!({"status":if choices.is_empty(){"needs_access_or_target"}else{"choose_frame"},"choices":choices,"warnings":discovery_warnings,"discovery_limit":"at most 4 sessions and 24 roots per session; use an explicit frame_id for other roots","next_action":if choices.is_empty(){"Check the named frame, pairing, and Currently shared in the Bridge. Share the intended frame; never expand access yourself."}else{"Ask which frame to use, then call open_design with its session_id and frame_id. Do not choose another agent's frame."}}),
            );
        }
        let target = &choices[0];
        let sid = target["session_id"].as_str().unwrap_or_default();
        let root = target["frame_id"].as_str().unwrap_or_default();
        let tree = self.canvas_call("tree",json!({"session_id":sid,"root_id":root,"depth":2,"limit":60,"fields":["id","type","name","parent_id","geometry","text","fills"]}),READ_TIMEOUT,cancel).await;
        if tree.is_error == Some(true) {
            return tree;
        }
        let exported = self
            .canvas_call(
                "export",
                json!({"session_id":sid,"node_ids":[root],"format":"png","scale":0.5}),
                EXPORT_TIMEOUT,
                cancel,
            )
            .await;
        let mut image = None;
        let mut render_error = Value::Null;
        if exported.is_error == Some(true) {
            render_error = exported.structured_content.unwrap_or_default();
        } else {
            let data = payload(&exported);
            let asset = data.get("asset_id").or_else(|| {
                data.get("assets")
                    .and_then(Value::as_array)
                    .and_then(|a| a.first())
                    .and_then(|a| a.get("asset_id"))
            });
            if let Some(asset) = asset {
                let bytes = self
                    .canvas_call(
                        "asset",
                        json!({"session_id":sid,"asset_id":asset}),
                        READ_TIMEOUT,
                        cancel,
                    )
                    .await;
                if bytes.is_error == Some(true) {
                    render_error = bytes.structured_content.unwrap_or_default();
                } else {
                    image = Some(image_result(payload(&bytes)));
                }
            } else {
                render_error =
                    json!({"code":"EXPORT_FAILED","message":"export returned no asset id"});
            }
        }
        // A compound read never labels data from a changed grant as a coherent view.
        let current = self
            .canvas_call("sessions", json!({}), READ_TIMEOUT, cancel)
            .await;
        if current.is_error == Some(true) {
            return current;
        }
        let same = payload(&current)["sessions"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|s| {
                s["session_id"] == sid
                    && s["state"] == "ready"
                    && s["grant_epoch"] == target["grant_epoch"]
            });
        if !same {
            return failed(
                ErrorCode::DocumentChanged,
                "shared access changed during open_design; open the intended frame again",
            );
        }
        let mut result = reply(
            json!({"status":"ready","target":target,"editing_allowed":target["grant_mode"] == "write","tree":tree.structured_content,"render_available":image.as_ref().is_some_and(|i|i.is_error != Some(true)),"render_error":render_error,"warnings":discovery_warnings,"write_budgets":session_data["write_budgets"],"next_action":if target["grant_mode"] == "write" {"Inspect the image and the brief. Propose a coherent native edit with preview_write, check get_write_approval, then apply the exact approved plan. Read back and render again."}else{"Reading is available. Ask the user to Allow editing in the Bridge before proposing a change. Never grant or approve for the user."}}),
        );
        if let Some(image) = image {
            result.content.extend(
                image
                    .content
                    .into_iter()
                    .filter(|c| matches!(c, ContentBlock::Image(_))),
            );
        }
        result
    }
}

#[rmcp::tool_router(router = canvas_tool_router, vis = "pub")]
impl FigTetherService {
    /// Start a design task with the actual shared canvas.
    #[rmcp::tool(
        name = "open_design",
        description = "Start here to design, create a website, or edit a Figma frame. Resolve a named or unambiguous granted frame and return bounded canvas context, a PNG image, editing access, budgets, and the next action. Multiple matches return choices. This reads only; it never shares frames or approves changes. Source-code exploration is unnecessary for normal design work.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn open_design(
        &self,
        Parameters(p): Parameters<OpenDesignParams>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.open(p, &context).await
    }

    /// Retrieve the render through standard MCP image content.
    #[rmcp::tool(
        name = "get_asset",
        description = "See a Figma PNG exported by export_assets. Returns the actual image inline, with size and hash metadata. Requires the same live session and active grant; images expire after five minutes. Oversized images require a smaller section or lower export scale. Never fetch a local path or credential.",
        annotations(read_only_hint = true, destructive_hint = false)
    )]
    async fn get_asset(
        &self,
        Parameters(p): Parameters<GetAssetParams>,
        context: RequestContext<RoleServer>,
    ) -> CallToolResult {
        self.asset(p, &context).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encoded_images_are_never_duplicated_into_model_text() {
        let result =
            image_result(json!({"mime":"image/png","image_base64":"AQID","width":1,"height":1}));
        let value = serde_json::to_value(result).unwrap();
        assert_eq!(value["content"][1]["type"], "image");
        assert_eq!(value["content"][1]["data"], "AQID");
        assert!(value["structuredContent"].get("image_base64").is_none());
        assert!(!value["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("AQID"));
        assert_eq!(
            image_result(json!({"mime":"image/png"})).is_error,
            Some(true)
        );
    }
}
