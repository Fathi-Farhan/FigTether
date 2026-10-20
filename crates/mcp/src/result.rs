//! Shaping of the tool results.
//!
//! A successful read carries the envelope of the daemon as text content and
//! as a structured content object. A failed read carries the [`FigError`] of
//! the daemon as a structured content object with `is_error` set, never a
//! stack trace and never a protocol level error.
//!
//! The write path uses the same two shapes. A successful write carries the
//! envelope and, next to it, the payload of the plan, so that the preview,
//! the approval, and the result of an apply are readable without digging.
//! A failed write carries the error of the daemon with the code of the write
//! taxonomy.

use figtether_model::envelope::Envelope;
use figtether_model::errors::{ErrorCode, FigError};
use rmcp::model::{CallToolResult, ContentBlock};

use crate::error::McpError;

/// The payload keys of the write path that the adapter lifts to the top level
/// of the tool result.
///
/// The keys are the ones an agent needs to know what happened: what the plan
/// would change, whether it is blocked, which approval it belongs to, and
/// whether the undo was committed.
const LIFTED_KEYS: [&str; 16] = [
    "plan_id",
    "plan_hash",
    "approval_id",
    "approval_token",
    "status",
    "reason",
    "expires_at",
    "preview",
    "result",
    "created",
    "changed",
    "removed",
    "moved",
    "operations_count",
    "blocked",
    "undo_committed",
];

/// The payload keys that hold a nested preview or result of a plan.
///
/// The daemon answers a preview with the preview of the plugin under
/// `preview`, and an apply with the result of the plugin under `result`. The
/// fields of both are lifted as well, so that the node ids of the plan are
/// readable at the top level of the tool result.
const NESTED_KEYS: [&str; 2] = ["preview", "result"];

/// Build the successful tool result of one read.
pub fn envelope_result(envelope: &Envelope) -> CallToolResult {
    let value = serde_json::to_value(envelope).unwrap_or_else(|_| serde_json::json!({}));
    let text = serde_json::to_string_pretty(envelope).unwrap_or_default();
    let mut result = CallToolResult::structured(value);
    result.content = vec![ContentBlock::text(text)];
    result
}

/// Build the successful tool result of one write.
///
/// The result carries the envelope of the daemon and, next to it, the payload
/// of the plan: the preview, the approval, or the result of the apply. The
/// warnings of the plan travel as `plan_warnings` and inside `data`, so that
/// they never hide the caveats of the envelope.
pub fn write_result(envelope: &Envelope) -> CallToolResult {
    let mut value = serde_json::to_value(envelope).unwrap_or_else(|_| serde_json::json!({}));
    if let (Some(object), Some(data)) = (value.as_object_mut(), envelope.data.as_object()) {
        for nested in NESTED_KEYS {
            let Some(inner) = data.get(nested).and_then(serde_json::Value::as_object) else {
                continue;
            };
            for (key, item) in inner {
                if key == "warnings" {
                    object.insert("plan_warnings".to_string(), item.clone());
                    continue;
                }
                object.insert(key.clone(), item.clone());
            }
        }
        for key in LIFTED_KEYS {
            if let Some(item) = data.get(key) {
                object.insert(key.to_string(), item.clone());
            }
        }
        if let Some(warnings) = data.get("warnings") {
            object.insert("plan_warnings".to_string(), warnings.clone());
        }
    }
    let text = serde_json::to_string_pretty(&value).unwrap_or_default();
    let mut result = CallToolResult::structured(value);
    result.content = vec![ContentBlock::text(text)];
    result
}

/// Build the failed tool result of one read.
pub fn error_result(error: &FigError) -> CallToolResult {
    let value = serde_json::to_value(error).unwrap_or_else(|_| serde_json::json!({}));
    let text = format!(
        "{code}: {message}\naction: {action}\nrequest_id: {request_id}",
        code = error.code.as_str(),
        message = error.message,
        action = error.action,
        request_id = error.request_id,
    );
    let mut result = CallToolResult::structured_error(value);
    result.content = vec![ContentBlock::text(text)];
    result
}

/// Build the failed tool result of one write.
///
/// The shape is the shape of every business failure: a machine readable code,
/// a message, a retryability verdict, a recovery action, the request id, and
/// an optional retry delay. The code is the one of the write taxonomy, and
/// the action is the documented one of that code when the daemon did not
/// name one.
pub fn write_error_result(error: &FigError) -> CallToolResult {
    error_result(&normalize_write_error(error))
}

/// Complete a write error with the documented default of its code.
///
/// The daemon names the action of a failure it owns. A failure the adapter
/// raises itself, or a failure of a peer that stayed silent about the
/// recovery, gets the action the taxonomy documents for the code.
pub fn normalize_write_error(error: &FigError) -> FigError {
    let mut normalized = error.clone();
    if normalized.action.trim().is_empty() {
        normalized = normalized.with_action(error.code.default_action());
    }
    normalized
}

/// Map a failure of the adapter onto the error taxonomy.
///
/// A business failure of the daemon never travels this way: it arrives as a
/// [`FigError`] already and keeps its own code, message, and action.
pub fn daemon_error(error: &McpError, request_id: &str) -> FigError {
    let mapped = match error {
        McpError::Cancelled => FigError::new(
            ErrorCode::Cancelled,
            "the MCP client cancelled the request before the daemon answered",
        ),
        McpError::DaemonTimeout(timeout) => FigError::new(
            ErrorCode::Timeout,
            format!("the daemon did not answer within {timeout:?}"),
        ),
        McpError::DaemonUnavailable { .. } | McpError::DaemonConnection { .. } => FigError::new(
            ErrorCode::AuthRequired,
            "the local FigTether daemon is not reachable",
        )
        .with_action("start the daemon with `figtether daemon`, then run `figtether doctor`"),
        McpError::DaemonHandshake { reason, .. } => FigError::new(
            ErrorCode::AuthRequired,
            format!("the local FigTether daemon refused the connection: {reason}"),
        )
        .with_action("start the daemon with `figtether daemon`, then run `figtether doctor`"),
        McpError::DaemonProtocol(reason) => FigError::new(
            ErrorCode::ProtocolMismatch,
            format!("the daemon answered with an unusable frame: {reason}"),
        )
        .with_action("update the client or the daemon, then run `figtether doctor`"),
        McpError::DaemonRejected(rejected) => rejected.clone(),
        McpError::RuntimeDirectory(reason) => FigError::new(
            ErrorCode::AuthRequired,
            format!("the runtime directory of the daemon is not usable: {reason}"),
        )
        .with_action("start the daemon with `figtether daemon`, then run `figtether doctor`"),
        McpError::RuntimeFileUnreadable { path, .. }
        | McpError::RuntimeFileInvalid { path, .. } => FigError::new(
            ErrorCode::AuthRequired,
            format!(
                "the runtime file of the daemon is not usable: {}",
                path.display()
            ),
        )
        .with_action("start the daemon with `figtether daemon`, then run `figtether doctor`"),
        McpError::InvalidEndpoint(reason) => FigError::new(
            ErrorCode::InternalError,
            format!("the daemon endpoint is not usable: {reason}"),
        ),
        McpError::Transport(reason) | McpError::ServerStopped(reason) => {
            FigError::new(ErrorCode::InternalError, reason.clone())
        }
    };
    mapped.with_request_id(request_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use figtether_model::envelope::{Consistency, Coverage, Freshness, Observation, SourceRef};
    use figtether_model::ids::SessionId;

    /// The codes of the write taxonomy.
    const WRITE_CODES: [ErrorCode; 8] = [
        ErrorCode::WriteNotGranted,
        ErrorCode::ApprovalRequired,
        ErrorCode::ApprovalExpired,
        ErrorCode::PlanHashMismatch,
        ErrorCode::PlanInvalid,
        ErrorCode::BudgetExceeded,
        ErrorCode::DestructiveNotAllowed,
        ErrorCode::MutationRefused,
    ];

    fn envelope() -> Envelope {
        let now = chrono::Utc::now();
        Envelope::new(
            "req_01",
            SourceRef::plugin_live(SessionId::new()),
            Observation {
                observation_id: figtether_model::ids::ObservationId::new(),
                started_at: now,
                observed_at: now,
                consistency: Consistency::Fresh,
                freshness: Freshness::FreshRead,
                cache_age_ms: None,
                content_hash: None,
            },
            Coverage::default(),
            serde_json::json!({ "nodes": [] }),
        )
    }

    #[test]
    fn a_success_carries_the_envelope_as_text_and_as_structured_content() {
        let envelope = envelope();
        let result = envelope_result(&envelope);
        assert_eq!(result.is_error, Some(false));
        let structured = result
            .structured_content
            .as_ref()
            .expect("the envelope is structured content");
        assert_eq!(structured["source"]["kind"], "plugin_live");
        assert_eq!(structured["observation"]["consistency"], "fresh");
        assert!(structured["coverage"].is_object());
        assert!(structured["data"].is_object());
        assert!(structured["warnings"].is_array());
        let text = result
            .content
            .first()
            .and_then(ContentBlock::as_text)
            .expect("the envelope is text content");
        assert!(text.text.contains("plugin_live"));
        assert!(text.text.contains("\"coverage\""));
    }

    #[test]
    fn a_failure_carries_the_error_shape() {
        let error = FigError::new(ErrorCode::OutOfScope, "the node is outside the grant")
            .with_request_id("req_02")
            .with_retry_after_ms(250);
        let result = error_result(&error);
        assert_eq!(result.is_error, Some(true));
        let structured = result
            .structured_content
            .as_ref()
            .expect("the error is structured content");
        assert_eq!(structured["code"], "OUT_OF_SCOPE");
        assert_eq!(structured["retryable"], false);
        assert_eq!(structured["request_id"], "req_02");
        assert_eq!(structured["retry_after_ms"], 250);
        assert!(structured["action"].is_string());
        let text = result
            .content
            .first()
            .and_then(ContentBlock::as_text)
            .expect("the error is text content");
        assert!(text.text.contains("OUT_OF_SCOPE"));
    }

    #[test]
    fn a_cancelled_request_maps_to_the_cancelled_code() {
        let error = daemon_error(&McpError::Cancelled, "req_03");
        assert_eq!(error.code, ErrorCode::Cancelled);
        assert_eq!(error.request_id, "req_03");
        assert!(!error.retryable);
    }

    #[test]
    fn a_timeout_maps_to_the_timeout_code() {
        let error = daemon_error(
            &McpError::DaemonTimeout(std::time::Duration::from_secs(20)),
            "req_04",
        );
        assert_eq!(error.code, ErrorCode::Timeout);
        assert!(error.retryable);
    }

    #[test]
    fn a_missing_daemon_maps_to_auth_required() {
        let error = daemon_error(
            &McpError::DaemonUnavailable {
                endpoint: "ws://127.0.0.1:38947/client".to_string(),
                reason: "connection refused".to_string(),
            },
            "req_05",
        );
        assert_eq!(error.code, ErrorCode::AuthRequired);
        assert!(error.action.contains("figtether daemon"));
    }

    #[test]
    fn every_code_of_the_write_taxonomy_travels_as_a_structured_result() {
        for code in WRITE_CODES {
            let error = FigError::new(code, "the daemon refused the write")
                .with_request_id("req_06")
                .with_retry_after_ms(250);
            let result = write_error_result(&error);
            assert_eq!(result.is_error, Some(true), "{code}");
            let structured = result
                .structured_content
                .as_ref()
                .unwrap_or_else(|| panic!("{code} is structured content"));
            assert_eq!(structured["code"], code.as_str());
            assert_eq!(structured["message"], "the daemon refused the write");
            assert_eq!(structured["retryable"], error.retryable);
            assert!(!structured["action"].as_str().unwrap_or_default().is_empty());
            assert_eq!(structured["request_id"], "req_06");
            assert_eq!(structured["retry_after_ms"], 250);
        }
    }

    #[test]
    fn the_codes_of_the_write_taxonomy_have_the_documented_names() {
        assert_eq!(WRITE_CODES[0].as_str(), "WRITE_NOT_GRANTED");
        assert_eq!(WRITE_CODES[1].as_str(), "APPROVAL_REQUIRED");
        assert_eq!(WRITE_CODES[2].as_str(), "APPROVAL_EXPIRED");
        assert_eq!(WRITE_CODES[3].as_str(), "PLAN_HASH_MISMATCH");
        assert_eq!(WRITE_CODES[4].as_str(), "PLAN_INVALID");
        assert_eq!(WRITE_CODES[5].as_str(), "BUDGET_EXCEEDED");
        assert_eq!(WRITE_CODES[6].as_str(), "DESTRUCTIVE_NOT_ALLOWED");
        assert_eq!(WRITE_CODES[7].as_str(), "MUTATION_REFUSED");
    }

    #[test]
    fn a_write_failure_is_never_a_stack_trace() {
        let error = FigError::new(ErrorCode::MutationRefused, "the editor refused the move")
            .with_request_id("req_07");
        let result = write_error_result(&error);
        let text = result
            .content
            .first()
            .and_then(ContentBlock::as_text)
            .expect("the error is text content");
        assert!(text.text.contains("MUTATION_REFUSED"));
        assert!(!text.text.contains("panic"));
        assert!(!text.text.contains("backtrace"));
    }

    #[test]
    fn a_write_error_without_an_action_gets_the_documented_one() {
        let error = FigError {
            code: ErrorCode::ApprovalRequired,
            message: "no approval for this plan".to_string(),
            retryable: false,
            action: String::new(),
            request_id: "req_08".to_string(),
            retry_after_ms: None,
        };
        let normalized = normalize_write_error(&error);
        assert_eq!(
            normalized.action,
            ErrorCode::ApprovalRequired.default_action()
        );
        assert_eq!(normalized.request_id, "req_08");
    }

    #[test]
    fn a_write_error_keeps_the_action_of_the_daemon() {
        let error = FigError::new(ErrorCode::PlanInvalid, "the plan is broken")
            .with_action("fix the parent id and preview the plan again");
        let normalized = normalize_write_error(&error);
        assert_eq!(
            normalized.action,
            "fix the parent id and preview the plan again"
        );
    }

    #[test]
    fn a_write_success_lifts_the_payload_next_to_the_envelope() {
        let mut envelope = envelope();
        envelope.data = serde_json::json!({
            "plan_id": "plan_1",
            "approval_id": "apr_1",
            "created": ["1:9"],
            "changed": [],
            "removed": [],
            "moved": [],
            "operations_count": 2,
            "warnings": ["the parent is an instance"],
            "blocked": false
        });
        let result = write_result(&envelope);
        assert_eq!(result.is_error, Some(false));
        let structured = result
            .structured_content
            .as_ref()
            .expect("the preview is structured content");
        assert_eq!(structured["plan_id"], "plan_1");
        assert_eq!(structured["approval_id"], "apr_1");
        assert_eq!(structured["created"], serde_json::json!(["1:9"]));
        assert_eq!(structured["operations_count"], 2);
        assert_eq!(structured["blocked"], false);
        assert_eq!(
            structured["plan_warnings"],
            serde_json::json!(["the parent is an instance"])
        );
        assert!(structured["data"].is_object());
        assert!(structured["source"].is_object());
    }

    #[test]
    fn a_write_success_lifts_the_nested_preview_of_the_daemon() {
        let mut envelope = envelope();
        envelope.data = serde_json::json!({
            "preview": {
                "plan_id": "plan_1",
                "plan_hash": "sha256:abc",
                "created": ["1:9"],
                "changed": [],
                "removed": [],
                "moved": [],
                "operations_count": 2,
                "warnings": ["the parent is an instance"],
                "blocked": false
            },
            "approval_id": "apr_1",
            "plan_id": "plan_1",
            "plan_hash": "sha256:abc"
        });
        let result = write_result(&envelope);
        let structured = result
            .structured_content
            .as_ref()
            .expect("the preview is structured content");
        assert_eq!(structured["created"], serde_json::json!(["1:9"]));
        assert_eq!(structured["operations_count"], 2);
        assert_eq!(structured["blocked"], false);
        assert_eq!(structured["approval_id"], "apr_1");
        assert_eq!(structured["plan_hash"], "sha256:abc");
        assert_eq!(
            structured["plan_warnings"],
            serde_json::json!(["the parent is an instance"])
        );
        assert_eq!(structured["preview"]["created"], serde_json::json!(["1:9"]));
        assert!(structured["warnings"].is_array());
    }
}
