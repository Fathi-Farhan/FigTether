//! Error taxonomy.
//!
//! The codes below are the contract described in `docs/04-data-contracts/05-error-catalog.md`
//! and `ADR-011`. Business failures travel as structured MCP tool results,
//! never as protocol errors and never as stack traces. Every error carries a
//! machine readable code, a human readable message, a retryability verdict, a
//! recovery action, the originating request id, and an optional retry delay.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Canonical error codes.
///
/// `retryable` is `true` only for conditions where an identical request may
/// succeed later without any user action. `Some` means "conditional" and is
/// documented in the per-code guidance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    /// Credential missing, expired, or replayed.
    AuthRequired,
    /// Credential was valid but has expired.
    AuthExpired,
    /// Platform permission or organization policy denied the operation.
    PermissionDenied,
    /// The requested node is outside the granted scope.
    OutOfScope,
    /// The plugin is not connected; the editor is closed or asleep.
    PluginOffline,
    /// The requested session does not exist or is no longer valid.
    SessionNotFound,
    /// The user paused the session; new reads are refused.
    Paused,
    /// The user revoked the grant.
    Revoked,
    /// The node does not exist in this session.
    NodeNotFound,
    /// The API, editor version, or seat does not expose this capability.
    CapabilityUnavailable,
    /// A payload exceeded the byte budget.
    PayloadTooLarge,
    /// The requested scope is larger than the traversal budget.
    ScopeTooLarge,
    /// The queue is saturated; retry after the given delay.
    Busy,
    /// The operation exceeded its deadline.
    Timeout,
    /// The operation was cancelled.
    Cancelled,
    /// The document changed during the observation window.
    DocumentChanged,
    /// The pagination cursor is no longer valid.
    CursorExpired,
    /// A resynchronisation is required before reading again.
    ResyncRequired,
    /// The client, daemon, and plugin versions are incompatible.
    UnsupportedSchema,
    /// The wire protocol version is incompatible.
    ProtocolMismatch,
    /// The export failed for the requested node or format.
    ExportFailed,
    /// A font required by the export is unavailable.
    FontUnavailable,
    /// The session limit is reached.
    SessionLimit,
    /// A requested limit is invalid (depth, batch size, scale).
    InvalidLimit,
    /// An unexpected internal error.
    InternalError,
}

impl ErrorCode {
    /// Stable wire string of the code.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AuthRequired => "AUTH_REQUIRED",
            Self::AuthExpired => "AUTH_EXPIRED",
            Self::PermissionDenied => "PERMISSION_DENIED",
            Self::OutOfScope => "OUT_OF_SCOPE",
            Self::PluginOffline => "PLUGIN_OFFLINE",
            Self::SessionNotFound => "SESSION_NOT_FOUND",
            Self::Paused => "PAUSED",
            Self::Revoked => "REVOKED",
            Self::NodeNotFound => "NODE_NOT_FOUND",
            Self::CapabilityUnavailable => "CAPABILITY_UNAVAILABLE",
            Self::PayloadTooLarge => "PAYLOAD_TOO_LARGE",
            Self::ScopeTooLarge => "SCOPE_TOO_LARGE",
            Self::Busy => "BUSY",
            Self::Timeout => "TIMEOUT",
            Self::Cancelled => "CANCELLED",
            Self::DocumentChanged => "DOCUMENT_CHANGED",
            Self::CursorExpired => "CURSOR_EXPIRED",
            Self::ResyncRequired => "RESYNC_REQUIRED",
            Self::UnsupportedSchema => "UNSUPPORTED_SCHEMA",
            Self::ProtocolMismatch => "PROTOCOL_MISMATCH",
            Self::ExportFailed => "EXPORT_FAILED",
            Self::FontUnavailable => "FONT_UNAVAILABLE",
            Self::SessionLimit => "SESSION_LIMIT",
            Self::InvalidLimit => "INVALID_LIMIT",
            Self::InternalError => "INTERNAL_ERROR",
        }
    }

    /// Whether the same request may succeed later without user action.
    ///
    /// `Some(true)` means retryable under a condition the caller can evaluate,
    /// for example `BUSY` once the advertised delay elapsed.
    pub const fn retryable(self) -> Option<bool> {
        match self {
            Self::Busy | Self::Timeout => Some(true),
            Self::DocumentChanged
            | Self::PluginOffline
            | Self::CursorExpired
            | Self::ResyncRequired => Some(false),
            Self::AuthRequired
            | Self::AuthExpired
            | Self::PermissionDenied
            | Self::OutOfScope
            | Self::SessionNotFound
            | Self::Paused
            | Self::Revoked
            | Self::NodeNotFound
            | Self::CapabilityUnavailable
            | Self::PayloadTooLarge
            | Self::ScopeTooLarge
            | Self::Cancelled
            | Self::UnsupportedSchema
            | Self::ProtocolMismatch
            | Self::ExportFailed
            | Self::FontUnavailable
            | Self::SessionLimit
            | Self::InvalidLimit => Some(false),
            Self::InternalError => None,
        }
    }

    /// Default recovery hint shown to an agent and to a human.
    pub const fn default_action(self) -> &'static str {
        match self {
            Self::AuthRequired => {
                "Pair the session again from the plugin or CLI; agents never look for secrets."
            }
            Self::AuthExpired => "Pair again; the temporary credential expired.",
            Self::PermissionDenied => {
                "The user or the organization must grant access; agents cannot escalate."
            }
            Self::OutOfScope => "Ask the user to widen the grant or request a different root.",
            Self::PluginOffline => {
                "Open Figma and start the plugin session, then list sessions again."
            }
            Self::SessionNotFound => "Call list_sessions and use a live session id.",
            Self::Paused => "The user must resume the session before reads are accepted.",
            Self::Revoked => "The grant was revoked; only the user can create a new one.",
            Self::NodeNotFound => "Refresh the tree; the node id may be stale.",
            Self::CapabilityUnavailable => "Use the alternative listed in the capability report.",
            Self::PayloadTooLarge => "Narrow the root, reduce depth, or request fewer fields.",
            Self::ScopeTooLarge => "Split the traversal or choose a smaller root.",
            Self::Busy => "Wait for retry_after_ms and retry once; never spin.",
            Self::Timeout => "Retry once, or request a smaller scope.",
            Self::Cancelled => "The result was discarded; issue a fresh request if still needed.",
            Self::DocumentChanged => "Retry once, or use a snapshot for a stable view.",
            Self::CursorExpired => "Restart the traversal from the beginning.",
            Self::ResyncRequired => "Take a fresh baseline before reading again.",
            Self::UnsupportedSchema => {
                "Update the client or plugin; run doctor for compatible versions."
            }
            Self::ProtocolMismatch => {
                "Update the client or plugin; run doctor for compatible versions."
            }
            Self::ExportFailed => "Try another supported format or a smaller node.",
            Self::FontUnavailable => "Use a format that does not require the missing font.",
            Self::SessionLimit => "Close an unused session or raise the configured limit.",
            Self::InvalidLimit => "Use a supported limit value.",
            Self::InternalError => {
                "Report the correlation id; do not retry without classification."
            }
        }
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A structured FigTether error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FigError {
    /// Machine readable code.
    pub code: ErrorCode,
    /// Human readable summary. Must never contain design content or secrets.
    pub message: String,
    /// Whether the caller may retry.
    pub retryable: bool,
    /// Recovery hint.
    pub action: String,
    /// Correlation id of the originating request.
    pub request_id: String,
    /// Suggested delay before retrying, in milliseconds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
}

impl FigError {
    /// Build an error with the code's default retryability and action.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retryable: code.retryable().unwrap_or(false),
            action: code.default_action().to_string(),
            request_id: String::new(),
            retry_after_ms: None,
        }
    }

    /// Attach the originating request id.
    pub fn with_request_id(mut self, request_id: impl Into<String>) -> Self {
        self.request_id = request_id.into();
        self
    }

    /// Attach an explicit retry delay.
    pub fn with_retry_after_ms(mut self, delay: u64) -> Self {
        self.retry_after_ms = Some(delay);
        self
    }

    /// Override the retryability verdict.
    pub fn with_retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }

    /// Override the recovery hint.
    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = action.into();
        self
    }
}

impl std::fmt::Display for FigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for FigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_have_stable_strings() {
        assert_eq!(ErrorCode::Busy.as_str(), "BUSY");
        assert_eq!(ErrorCode::OutOfScope.as_str(), "OUT_OF_SCOPE");
        assert_eq!(ErrorCode::SessionLimit.as_str(), "SESSION_LIMIT");
    }

    #[test]
    fn busy_is_retryable_and_offline_is_not() {
        assert_eq!(ErrorCode::Busy.retryable(), Some(true));
        assert_eq!(ErrorCode::PluginOffline.retryable(), Some(false));
        assert_eq!(ErrorCode::Revoked.retryable(), Some(false));
    }

    #[test]
    fn error_carries_request_id_and_delay() {
        let err = FigError::new(ErrorCode::Busy, "queue full")
            .with_request_id("req_01")
            .with_retry_after_ms(250);
        assert_eq!(err.request_id, "req_01");
        assert_eq!(err.retry_after_ms, Some(250));
        assert!(err.retryable);
    }

    #[test]
    fn error_serializes_without_null_delay() {
        let err = FigError::new(ErrorCode::Revoked, "grant revoked");
        let json = serde_json::to_value(&err).expect("serializes");
        assert!(json.get("retry_after_ms").is_none());
        assert_eq!(json["code"], "REVOKED");
    }
}
