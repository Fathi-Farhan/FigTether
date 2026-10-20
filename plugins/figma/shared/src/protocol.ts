/**
 * Bridge frame contract.
 *
 * Mirror of `crates/protocol/src/lib.rs`. Field names and optionality follow
 * the Rust definitions exactly: a Rust `Option<T>` with
 * `skip_serializing_if = "Option::is_none"` becomes an optional TypeScript
 * property, and a Rust field with `#[serde(default)]` stays required in the
 * TypeScript shape because the Rust serializer always writes it.
 */
import type { CapabilityMap } from "./capabilities";
import type { JsonValue } from "./json";

/** Wire protocol version spoken by the daemon and the plugin. */
export const PROTOCOL_VERSION = "1.0";

/** Maximum size of a single bridge frame in bytes. */
export const MAX_FRAME_BYTES = 1024 * 1024;

/** Maximum size of one chunked transfer in bytes. */
export const MAX_TRANSFER_BYTES = 32 * 1024 * 1024;

/** Time the daemon waits for the first authentication frame, in milliseconds. */
export const AUTH_WINDOW_MS = 3000;

/** Interval between heartbeats, in milliseconds. */
export const HEARTBEAT_INTERVAL_MS = 5000;

/** Number of missed heartbeats after which a session is considered offline. */
export const MISSED_HEARTBEATS_BEFORE_OFFLINE = 3;

/** Kinds of frame exchanged over the bridge. */
export type MessageType =
  | "hello_auth"
  | "capabilities"
  | "grant_update"
  | "request"
  | "progress"
  | "chunk"
  | "result"
  | "error"
  | "cancel"
  | "heartbeat"
  | "revoke";

/** Every message type in wire order. */
export const MESSAGE_TYPES: readonly MessageType[] = [
  "hello_auth",
  "capabilities",
  "grant_update",
  "request",
  "progress",
  "chunk",
  "result",
  "error",
  "cancel",
  "heartbeat",
  "revoke",
];

/** Operations the plugin can perform. */
export type BridgeOp =
  | "list_nodes"
  | "read_nodes"
  | "inspect_design"
  | "get_design_tokens"
  | "export_assets"
  | "get_selection"
  | "capture_snapshot"
  | "list_sessions"
  | "watch_scope"
  | "get_changes";

/** Every bridge operation in wire order. */
export const BRIDGE_OPS: readonly BridgeOp[] = [
  "list_nodes",
  "read_nodes",
  "inspect_design",
  "get_design_tokens",
  "export_assets",
  "get_selection",
  "capture_snapshot",
  "list_sessions",
  "watch_scope",
  "get_changes",
];

/** One bridge frame. */
export interface BridgeFrame {
  /** Protocol version the sender speaks. */
  protocol_version: string;
  /** Frame kind. */
  message_type: MessageType;
  /** Correlation id of the request this frame belongs to. */
  request_id: string;
  /** Session the frame belongs to, when known. */
  session_id?: string;
  /** Connection epoch of the sender. */
  connection_epoch?: number;
  /** Grant epoch in force. */
  grant_epoch?: number;
  /** Deadline for the operation, in milliseconds since the epoch. */
  deadline_ms?: number;
  /** Typed payload. */
  payload: JsonValue;
}

/** Authentication request, always the first frame of a connection. */
export interface AuthRequest {
  /** One time pairing secret shown to the user by the CLI. */
  secret: string;
  /** Stable id of the client installation. */
  client_id: string;
  /** Protocol version the plugin speaks. */
  protocol_version: string;
}

/** Successful authentication response. */
export interface AuthAccepted {
  /** Temporary session credential used for later frames. */
  session_credential: string;
  /** Session the plugin run represents. */
  session_id: string;
  /** Connection epoch assigned to this connection. */
  connection_epoch: number;
  /** Grant epoch in force, when a grant already exists. */
  grant_epoch?: number;
  /** Capabilities detected in the plugin runtime. */
  capabilities: CapabilityMap;
}

/** Rejected authentication response. */
export interface AuthRejected {
  /** Stable error code. */
  code: string;
  /** Human readable reason. */
  message: string;
}

/** A read request towards the plugin. */
export interface RequestPayload {
  /** Operation to perform. */
  op: BridgeOp;
  /** Operation parameters. */
  params: JsonValue;
}

/** Chunk of a larger transfer. Bytes travel base64 encoded. */
export interface ChunkPayload {
  /** Transfer identity. */
  transfer_id: string;
  /** Zero based sequence number. */
  sequence: number;
  /** Raw bytes of this chunk, base64 encoded. */
  bytes: string;
  /** Total number of bytes expected for the transfer. */
  total_bytes: number;
  /** Checksum of the complete transfer, sent on the final chunk. */
  checksum?: string;
}

/** Result of an operation. */
export interface ResultPayload {
  /** Operation result. */
  data: JsonValue;
}

/** Failure of an operation. */
export interface ErrorPayload {
  /** Stable error code. */
  code: string;
  /** Human readable message. */
  message: string;
  /** Whether the caller may retry. */
  retryable: boolean;
  /** Recovery hint. */
  action: string;
  /** Correlation id. */
  request_id: string;
  /** Suggested delay before retrying. */
  retry_after_ms?: number;
}

/** Keepalive frame. */
export interface HeartbeatPayload {
  /** Monotonic sequence number. */
  sequence: number;
  /** Wall clock timestamp, RFC 3339 in UTC. */
  timestamp: string;
}

/** Update of the granted scope. */
export interface GrantUpdatePayload {
  /** Root ids the user granted. */
  root_ids: string[];
  /** Epoch of the new grant. */
  grant_epoch: number;
}

/** Revocation of a grant. */
export interface RevokePayload {
  /** Why the grant was revoked. */
  reason: string;
}

/** Capabilities reported by the plugin runtime. */
export interface CapabilitiesPayload {
  /** Probed capabilities. */
  capabilities: CapabilityMap;
}

/** Whether a string is a known message type. */
export function isMessageType(value: unknown): value is MessageType {
  return typeof value === "string" && (MESSAGE_TYPES as readonly string[]).includes(value);
}

/** Whether a string is a known bridge operation. */
export function isBridgeOp(value: unknown): value is BridgeOp {
  return typeof value === "string" && (BRIDGE_OPS as readonly string[]).includes(value);
}
