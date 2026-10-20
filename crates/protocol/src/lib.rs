//! FigTether bridge protocol.
//!
//! The bridge is the authenticated loopback channel between the daemon and
//! the Figma plugin. It is deliberately small: a fixed set of frame types, a
//! first-frame authentication exchange, monotonic epochs, and a chunked
//! transfer path with hard byte budgets.
//!
//! Security properties enforced here (see `docs/02-architecture/02-bridge-design.md`):
//! - no design data, no session metadata, and no work is accepted before the
//!   first frame authenticates;
//! - the pairing secret is single use and never travels in a URL, a log line,
//!   or a persistent store;
//! - answers produced under an older connection or grant epoch are refused.

pub mod client;
pub mod plugin;

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use figtether_model::capabilities::CapabilityState;
use figtether_model::ids::SessionId;
use serde::{Deserialize, Serialize};

/// Wire protocol version spoken by the daemon and the plugin.
pub const PROTOCOL_VERSION: &str = "1.0";

/// Maximum size of a single bridge frame in bytes.
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

/// Maximum size of one chunked transfer in bytes.
pub const MAX_TRANSFER_BYTES: u64 = 32 * 1024 * 1024;

/// Time the daemon waits for the first authentication frame.
pub const AUTH_WINDOW: Duration = Duration::from_secs(3);

/// Interval between heartbeats.
pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

/// Number of missed heartbeats after which a session is considered offline.
pub const MISSED_HEARTBEATS_BEFORE_OFFLINE: u32 = 3;

/// Kinds of frame exchanged over the bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    /// First frame of a connection: authentication.
    HelloAuth,
    /// Capabilities of the plugin runtime.
    Capabilities,
    /// Update of the granted scope.
    GrantUpdate,
    /// A read request towards the plugin.
    Request,
    /// Progress report for a long operation.
    Progress,
    /// One chunk of a chunked transfer.
    Chunk,
    /// The result of an operation.
    Result,
    /// A failure.
    Error,
    /// Cancellation of an in-flight operation.
    Cancel,
    /// Keepalive.
    Heartbeat,
    /// Revocation of a grant.
    Revoke,
}

impl MessageType {
    /// Stable wire string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HelloAuth => "hello_auth",
            Self::Capabilities => "capabilities",
            Self::GrantUpdate => "grant_update",
            Self::Request => "request",
            Self::Progress => "progress",
            Self::Chunk => "chunk",
            Self::Result => "result",
            Self::Error => "error",
            Self::Cancel => "cancel",
            Self::Heartbeat => "heartbeat",
            Self::Revoke => "revoke",
        }
    }
}

/// Operations the plugin can perform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BridgeOp {
    /// List children of a node.
    ListNodes,
    /// Read specific nodes.
    ReadNodes,
    /// Inspect a subtree for implementation context.
    InspectDesign,
    /// Read styles and variables in scope.
    GetDesignTokens,
    /// Export assets as PNG or SVG.
    ExportAssets,
    /// Read the current selection, bounded by the grant.
    GetSelection,
    /// Capture an immutable snapshot.
    CaptureSnapshot,
    /// List the sessions the plugin knows about.
    ListSessions,
    /// Start watching a scope for invalidation events.
    WatchScope,
    /// Read change events for a watch.
    GetChanges,
}

impl BridgeOp {
    /// Stable wire string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ListNodes => "list_nodes",
            Self::ReadNodes => "read_nodes",
            Self::InspectDesign => "inspect_design",
            Self::GetDesignTokens => "get_design_tokens",
            Self::ExportAssets => "export_assets",
            Self::GetSelection => "get_selection",
            Self::CaptureSnapshot => "capture_snapshot",
            Self::ListSessions => "list_sessions",
            Self::WatchScope => "watch_scope",
            Self::GetChanges => "get_changes",
        }
    }
}

/// One bridge frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BridgeFrame {
    /// Protocol version the sender speaks.
    pub protocol_version: String,
    /// Frame kind.
    pub message_type: MessageType,
    /// Correlation id of the request this frame belongs to.
    pub request_id: String,
    /// Session the frame belongs to, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    /// Connection epoch of the sender.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_epoch: Option<u64>,
    /// Grant epoch in force.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant_epoch: Option<u64>,
    /// Deadline for the operation, in milliseconds since the epoch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline_ms: Option<u64>,
    /// Typed payload.
    pub payload: serde_json::Value,
}

impl BridgeFrame {
    /// Build a frame with the current protocol version.
    pub fn new(
        message_type: MessageType,
        request_id: impl Into<String>,
        payload: serde_json::Value,
    ) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION.to_string(),
            message_type,
            request_id: request_id.into(),
            session_id: None,
            connection_epoch: None,
            grant_epoch: None,
            deadline_ms: None,
            payload,
        }
    }

    /// Attach the session id.
    pub fn with_session(mut self, session_id: SessionId) -> Self {
        self.session_id = Some(session_id);
        self
    }

    /// Attach the connection epoch.
    pub fn with_connection_epoch(mut self, epoch: u64) -> Self {
        self.connection_epoch = Some(epoch);
        self
    }

    /// Attach the grant epoch.
    pub fn with_grant_epoch(mut self, epoch: u64) -> Self {
        self.grant_epoch = Some(epoch);
        self
    }

    /// Attach a deadline.
    pub fn with_deadline_ms(mut self, deadline_ms: u64) -> Self {
        self.deadline_ms = Some(deadline_ms);
        self
    }

    /// Encode the frame to the wire.
    pub fn to_bytes(&self) -> Result<Vec<u8>, ProtocolError> {
        let bytes =
            serde_json::to_vec(self).map_err(|err| ProtocolError::Encode(err.to_string()))?;
        if bytes.len() > MAX_FRAME_BYTES {
            return Err(ProtocolError::FrameTooLarge {
                size: bytes.len(),
                limit: MAX_FRAME_BYTES,
            });
        }
        Ok(bytes)
    }

    /// Decode a frame from the wire.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() > MAX_FRAME_BYTES {
            return Err(ProtocolError::FrameTooLarge {
                size: bytes.len(),
                limit: MAX_FRAME_BYTES,
            });
        }
        let frame: BridgeFrame =
            serde_json::from_slice(bytes).map_err(|err| ProtocolError::Decode(err.to_string()))?;
        if frame.protocol_version != PROTOCOL_VERSION {
            return Err(ProtocolError::ProtocolMismatch {
                expected: PROTOCOL_VERSION.to_string(),
                found: frame.protocol_version,
            });
        }
        Ok(frame)
    }

    /// Decode the payload into a concrete type.
    pub fn payload_of<T: serde::de::DeserializeOwned>(&self) -> Result<T, ProtocolError> {
        serde_json::from_value(self.payload.clone())
            .map_err(|err| ProtocolError::Decode(err.to_string()))
    }
}

/// Failures of the bridge protocol.
#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    /// A frame could not be encoded.
    #[error("frame encoding failed: {0}")]
    Encode(String),
    /// A frame could not be decoded.
    #[error("frame decoding failed: {0}")]
    Decode(String),
    /// The frame exceeded the frame budget.
    #[error("frame of {size} bytes exceeds the {limit} byte limit")]
    FrameTooLarge {
        /// Observed size.
        size: usize,
        /// Configured limit.
        limit: usize,
    },
    /// The peer speaks a different protocol version.
    #[error("protocol mismatch: expected {expected}, found {found}")]
    ProtocolMismatch {
        /// Version the daemon speaks.
        expected: String,
        /// Version the peer speaks.
        found: String,
    },
    /// A transfer exceeded the transfer budget.
    #[error("transfer of {size} bytes exceeds the {limit} byte limit")]
    TransferTooLarge {
        /// Observed size.
        size: u64,
        /// Configured limit.
        limit: u64,
    },
    /// The checksum of a completed transfer did not match.
    #[error("transfer checksum mismatch")]
    ChecksumMismatch,
    /// The transfer was not finished before its deadline.
    #[error("transfer did not complete before its deadline")]
    TransferTimeout,
}

/// Authentication request, always the first frame of a connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthRequest {
    /// One time pairing secret shown to the user by the CLI.
    pub secret: String,
    /// Stable id of the client installation.
    pub client_id: String,
    /// Protocol version the plugin speaks.
    pub protocol_version: String,
}

/// Successful authentication response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthAccepted {
    /// Temporary session credential used for later frames.
    pub session_credential: String,
    /// Session the plugin run represents.
    pub session_id: SessionId,
    /// Connection epoch assigned to this connection.
    pub connection_epoch: u64,
    /// Grant epoch in force, when a grant already exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grant_epoch: Option<u64>,
    /// Capabilities detected in the plugin runtime.
    #[serde(default)]
    pub capabilities: BTreeMap<String, CapabilityState>,
}

/// Rejected authentication response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthRejected {
    /// Stable error code.
    pub code: String,
    /// Human readable reason.
    pub message: String,
}

/// A read request towards the plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestPayload {
    /// Operation to perform.
    pub op: BridgeOp,
    /// Operation parameters.
    pub params: serde_json::Value,
}

/// Chunk of a larger transfer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkPayload {
    /// Transfer identity.
    pub transfer_id: String,
    /// Zero based sequence number.
    pub sequence: u32,
    /// Raw bytes of this chunk.
    #[serde(with = "base64_bytes")]
    pub bytes: Vec<u8>,
    /// Total number of bytes expected for the transfer.
    pub total_bytes: u64,
    /// Checksum of the complete transfer, sent on the final chunk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
}

/// Result of an operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultPayload {
    /// Operation result.
    pub data: serde_json::Value,
}

/// Failure of an operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorPayload {
    /// Stable error code.
    pub code: String,
    /// Human readable message.
    pub message: String,
    /// Whether the caller may retry.
    pub retryable: bool,
    /// Recovery hint.
    pub action: String,
    /// Correlation id.
    pub request_id: String,
    /// Suggested delay before retrying.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after_ms: Option<u64>,
}

/// Keepalive frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeartbeatPayload {
    /// Monotonic sequence number.
    pub sequence: u64,
    /// Wall clock timestamp.
    pub timestamp: DateTime<Utc>,
}

/// Update of the granted scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrantUpdatePayload {
    /// Root ids the user granted.
    pub root_ids: Vec<String>,
    /// Epoch of the new grant.
    pub grant_epoch: u64,
}

/// Revocation of a grant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevokePayload {
    /// Why the grant was revoked.
    pub reason: String,
}

/// Capabilities reported by the plugin runtime.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitiesPayload {
    /// Probed capabilities.
    pub capabilities: BTreeMap<String, CapabilityState>,
}

/// base64 encoding for byte payloads.
pub mod base64_bytes {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine as _;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&STANDARD.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        let text = String::deserialize(deserializer)?;
        STANDARD.decode(text).map_err(serde::de::Error::custom)
    }
}

/// Bounded reassembly of a chunked transfer.
#[derive(Debug)]
pub struct ChunkAssembler {
    transfer_id: String,
    buffer: Vec<u8>,
    expected: Option<u64>,
    received: u32,
    deadline: Option<Instant>,
    limit: u64,
}

impl ChunkAssembler {
    /// Start a new transfer.
    pub fn new(transfer_id: impl Into<String>, limit: u64) -> Self {
        Self {
            transfer_id: transfer_id.into(),
            buffer: Vec::new(),
            expected: None,
            received: 0,
            deadline: None,
            limit,
        }
    }

    /// Set the deadline after which the transfer is discarded.
    pub fn with_deadline(mut self, deadline: Instant) -> Self {
        self.deadline = Some(deadline);
        self
    }

    /// The transfer this assembler is collecting.
    pub fn transfer_id(&self) -> &str {
        &self.transfer_id
    }

    /// Accept one chunk.
    ///
    /// Returns the complete payload when the final chunk arrives, or `None`
    /// while the transfer is still in flight.
    pub fn accept(&mut self, chunk: &ChunkPayload) -> Result<Option<Vec<u8>>, ProtocolError> {
        if chunk.transfer_id != self.transfer_id {
            return Err(ProtocolError::Decode(
                "chunk belongs to another transfer".to_string(),
            ));
        }
        if let Some(deadline) = self.deadline {
            if Instant::now() > deadline {
                return Err(ProtocolError::TransferTimeout);
            }
        }
        if self.expected.is_none() {
            self.expected = Some(chunk.total_bytes);
        }
        let projected = self.buffer.len() as u64 + chunk.bytes.len() as u64;
        if projected > self.limit || chunk.total_bytes > self.limit {
            return Err(ProtocolError::TransferTooLarge {
                size: projected.max(chunk.total_bytes),
                limit: self.limit,
            });
        }
        self.buffer.extend_from_slice(&chunk.bytes);
        self.received = self.received.saturating_add(1);

        if self.buffer.len() as u64 == chunk.total_bytes {
            if let Some(expected) = chunk.checksum.as_deref() {
                let actual = sha256_hex(&self.buffer);
                if actual != expected {
                    return Err(ProtocolError::ChecksumMismatch);
                }
            }
            return Ok(Some(std::mem::take(&mut self.buffer)));
        }
        Ok(None)
    }

    /// Number of chunks accepted so far.
    pub fn chunks_received(&self) -> u32 {
        self.received
    }

    /// Bytes buffered so far.
    pub fn bytes_buffered(&self) -> usize {
        self.buffer.len()
    }
}

/// Hex encoded sha256 of a byte slice.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(transfer: &str, sequence: u32, bytes: &[u8], total: u64) -> ChunkPayload {
        ChunkPayload {
            transfer_id: transfer.to_string(),
            sequence,
            bytes: bytes.to_vec(),
            total_bytes: total,
            checksum: None,
        }
    }

    #[test]
    fn frame_round_trips() {
        let frame = BridgeFrame::new(MessageType::Heartbeat, "req_1", serde_json::json!({"a": 1}));
        let bytes = frame.to_bytes().expect("encodes");
        let decoded = BridgeFrame::from_bytes(&bytes).expect("decodes");
        assert_eq!(decoded, frame);
    }

    #[test]
    fn protocol_mismatch_is_rejected() {
        let mut frame = BridgeFrame::new(MessageType::Heartbeat, "req_1", serde_json::json!({}));
        frame.protocol_version = "0.9".to_string();
        let bytes = serde_json::to_vec(&frame).expect("encodes");
        let err = BridgeFrame::from_bytes(&bytes).expect_err("rejected");
        assert!(matches!(err, ProtocolError::ProtocolMismatch { .. }));
    }

    #[test]
    fn oversized_frames_are_rejected() {
        let frame = BridgeFrame::new(
            MessageType::Result,
            "req_1",
            serde_json::json!({"blob": "x".repeat(MAX_FRAME_BYTES)}),
        );
        assert!(matches!(
            frame.to_bytes().expect_err("too large"),
            ProtocolError::FrameTooLarge { .. }
        ));
    }

    #[test]
    fn chunks_reassemble_in_order() {
        let mut assembler = ChunkAssembler::new("t1", MAX_TRANSFER_BYTES);
        assert!(assembler
            .accept(&chunk("t1", 0, b"abc", 6))
            .expect("accepts")
            .is_none());
        let done = assembler
            .accept(&chunk("t1", 1, b"def", 6))
            .expect("accepts");
        assert_eq!(done, Some(b"abcdef".to_vec()));
    }

    #[test]
    fn transfers_beyond_the_limit_are_refused() {
        let mut assembler = ChunkAssembler::new("t2", 4);
        let err = assembler
            .accept(&chunk("t2", 0, b"abcdefgh", 8))
            .expect_err("refused");
        assert!(matches!(err, ProtocolError::TransferTooLarge { .. }));
    }

    #[test]
    fn checksum_mismatch_is_detected() {
        let mut assembler = ChunkAssembler::new("t3", 64);
        let mut final_chunk = chunk("t3", 0, b"payload", 7);
        final_chunk.checksum = Some("deadbeef".to_string());
        assert!(matches!(
            assembler.accept(&final_chunk).expect_err("mismatch"),
            ProtocolError::ChecksumMismatch
        ));
    }

    #[test]
    fn chunk_payload_uses_base64_on_the_wire() {
        let payload = chunk("t4", 0, b"\x00\x01\xff", 3);
        let json = serde_json::to_string(&payload).expect("encodes");
        assert!(json.contains("\"bytes\""), "{json}");
        let decoded: ChunkPayload = serde_json::from_str(&json).expect("decodes");
        assert_eq!(decoded.bytes, vec![0x00, 0x01, 0xff]);
    }
}
