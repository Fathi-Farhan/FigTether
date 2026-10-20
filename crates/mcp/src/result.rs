//! Shaping of the tool results.
//!
//! A successful read carries the envelope of the daemon as text content and
//! as a structured content object. A failed read carries the [`FigError`] of
//! the daemon as a structured content object with `is_error` set, never a
//! stack trace and never a protocol level error.

use figtether_model::envelope::Envelope;
use figtether_model::errors::{ErrorCode, FigError};
use rmcp::model::{CallToolResult, ContentBlock};

use crate::error::McpError;

/// Build the successful tool result of one read.
pub fn envelope_result(envelope: &Envelope) -> CallToolResult {
    let value = serde_json::to_value(envelope).unwrap_or_else(|_| serde_json::json!({}));
    let text = serde_json::to_string_pretty(envelope).unwrap_or_default();
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
}
