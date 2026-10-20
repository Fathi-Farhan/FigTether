//! Local client IPC contract.
//!
//! The MCP adapter and the CLI talk to the daemon over an authenticated
//! loopback channel. This module is the wire contract between them.
//!
//! The contract is intentionally request/response with a correlation id. A
//! long running operation answers with a job id first, and the client polls
//! `get_job_status` or asks for the result with the same request id.
//!
//! Trust model: the client credential lives in the owner-only runtime
//! directory and is bound to one client installation. A client can never
//! widen its own grant; every request is re-checked against the session and
//! the grant epoch inside the daemon.

use figtether_model::envelope::Envelope;
use figtether_model::errors::FigError;
use figtether_model::ids::JobId;
use serde::{Deserialize, Serialize};

/// A request from a local client to the daemon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientRequest {
    /// Liveness and sanitized status.
    Status {
        /// Correlation id.
        request_id: String,
    },
    /// Mint a one time pairing secret for the plugin UI.
    PairSecret {
        /// Correlation id.
        request_id: String,
    },
    /// List the sessions this client may see.
    ListSessions {
        /// Correlation id.
        request_id: String,
        /// Cursor for the next page.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<String>,
    },
    /// List the children of a node.
    ListNodes {
        /// Correlation id.
        request_id: String,
        /// Tool parameters.
        params: serde_json::Value,
    },
    /// Read specific nodes.
    GetNodes {
        /// Correlation id.
        request_id: String,
        /// Tool parameters.
        params: serde_json::Value,
    },
    /// Inspect a subtree.
    InspectDesign {
        /// Correlation id.
        request_id: String,
        /// Tool parameters.
        params: serde_json::Value,
    },
    /// Read styles and variables in scope.
    GetDesignTokens {
        /// Correlation id.
        request_id: String,
        /// Tool parameters.
        params: serde_json::Value,
    },
    /// Export assets.
    ExportAssets {
        /// Correlation id.
        request_id: String,
        /// Tool parameters.
        params: serde_json::Value,
    },
    /// Read the current selection.
    GetSelection {
        /// Correlation id.
        request_id: String,
        /// Tool parameters.
        params: serde_json::Value,
    },
    /// Describe a snapshot.
    GetSnapshotInfo {
        /// Correlation id.
        request_id: String,
        /// Tool parameters.
        params: serde_json::Value,
    },
    /// Register a watch (phase P1).
    WatchScope {
        /// Correlation id.
        request_id: String,
        /// Tool parameters.
        params: serde_json::Value,
    },
    /// Read watch events (phase P1).
    GetChanges {
        /// Correlation id.
        request_id: String,
        /// Tool parameters.
        params: serde_json::Value,
    },
    /// Describe a job.
    GetJobStatus {
        /// Correlation id.
        request_id: String,
        /// Tool parameters.
        params: serde_json::Value,
    },
    /// Cancel a job.
    Cancel {
        /// Correlation id.
        request_id: String,
        /// Job to cancel.
        job_id: JobId,
    },
    /// Revoke the grant of a session.
    Revoke {
        /// Correlation id.
        request_id: String,
        /// Session whose grant is revoked.
        session_id: figtether_model::ids::SessionId,
    },
}

/// A response from the daemon to a local client.
///
/// The envelope is boxed on purpose: the variants differ a lot in size and the
/// lint that protects the stack stays satisfied without changing the wire
/// shape.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientResponse {
    /// The request succeeded.
    Ok {
        /// Correlation id.
        request_id: String,
        /// The result envelope.
        envelope: Envelope,
    },
    /// The request failed.
    Error {
        /// Correlation id.
        request_id: String,
        /// The structured error.
        error: FigError,
    },
    /// The request was accepted and runs in the background.
    Accepted {
        /// Correlation id.
        request_id: String,
        /// The scheduled job.
        job_id: JobId,
    },
}

/// The first frame a local client sends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientAuth {
    /// Credential issued by the daemon for this client installation.
    pub credential: String,
    /// Stable id of the client installation.
    pub client_id: String,
    /// Protocol version the client speaks.
    pub protocol_version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_round_trip() {
        let request = ClientRequest::ListNodes {
            request_id: "req_1".to_string(),
            params: serde_json::json!({ "root_id": "1:2" }),
        };
        let json = serde_json::to_string(&request).expect("encodes");
        let decoded: ClientRequest = serde_json::from_str(&json).expect("decodes");
        assert_eq!(decoded, request);
    }

    #[test]
    fn responses_are_tagged() {
        let response = ClientResponse::Accepted {
            request_id: "req_1".to_string(),
            job_id: JobId::new(),
        };
        let json = serde_json::to_value(&response).expect("encodes");
        assert_eq!(json["type"], "accepted");
    }
}
