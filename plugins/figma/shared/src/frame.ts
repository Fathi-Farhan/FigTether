/**
 * Frame helpers shared by the plugin sandbox and the UI iframe.
 *
 * The UI iframe owns the socket, the main sandbox never opens one. Both sides
 * build and parse frames with these helpers so the wire shape cannot drift.
 */
import { canonicalStringify, parseJsonValue } from "./canonical";
import type { JsonValue } from "./json";
import {
  isMessageType,
  PROTOCOL_VERSION,
  type BridgeFrame,
  type ChunkPayload,
  type MessageType,
} from "./protocol";

/** Error raised when a frame or payload does not match the contract. */
export class BridgeContractError extends Error {
  /** Stable error code, for example `PROTOCOL_MISMATCH`. */
  readonly code: string;

  constructor(code: string, message: string) {
    super(message);
    this.name = "BridgeContractError";
    this.code = code;
  }
}

/** Options accepted when a frame is created. */
export interface FrameOptions {
  /** Session the frame belongs to. */
  session_id?: string;
  /** Connection epoch of the sender. */
  connection_epoch?: number;
  /** Grant epoch in force. */
  grant_epoch?: number;
  /** Deadline for the operation, in milliseconds. */
  deadline_ms?: number;
}

/** Build a frame with the current protocol version. */
export function createFrame(
  message_type: MessageType,
  request_id: string,
  payload: JsonValue,
  options: FrameOptions = {},
): BridgeFrame {
  const frame: BridgeFrame = {
    protocol_version: PROTOCOL_VERSION,
    message_type,
    request_id,
    payload,
  };
  if (options.session_id !== undefined) {
    frame.session_id = options.session_id;
  }
  if (options.connection_epoch !== undefined) {
    frame.connection_epoch = options.connection_epoch;
  }
  if (options.grant_epoch !== undefined) {
    frame.grant_epoch = options.grant_epoch;
  }
  if (options.deadline_ms !== undefined) {
    frame.deadline_ms = options.deadline_ms;
  }
  return frame;
}

/** Encode a frame to canonical JSON. */
export function serializeFrame(frame: BridgeFrame): string {
  const payload: Record<string, JsonValue> = {
    protocol_version: frame.protocol_version,
    message_type: frame.message_type,
    request_id: frame.request_id,
    payload: frame.payload,
  };
  if (frame.session_id !== undefined) {
    payload.session_id = frame.session_id;
  }
  if (frame.connection_epoch !== undefined) {
    payload.connection_epoch = frame.connection_epoch;
  }
  if (frame.grant_epoch !== undefined) {
    payload.grant_epoch = frame.grant_epoch;
  }
  if (frame.deadline_ms !== undefined) {
    payload.deadline_ms = frame.deadline_ms;
  }
  return canonicalStringify(payload);
}

/** Decode a frame from JSON text. */
export function parseFrame(text: string): BridgeFrame {
  const value = parseJsonValue(text);
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new BridgeContractError("INVALID_FRAME", "parseFrame: frame must be a JSON object");
  }
  const frame = value as Record<string, JsonValue>;
  if (frame.protocol_version !== PROTOCOL_VERSION) {
    throw new BridgeContractError(
      "PROTOCOL_MISMATCH",
      `parseFrame: expected protocol version ${PROTOCOL_VERSION}, found ${String(frame.protocol_version)}`,
    );
  }
  if (!isMessageType(frame.message_type)) {
    throw new BridgeContractError(
      "INVALID_FRAME",
      `parseFrame: unknown message type "${String(frame.message_type)}"`,
    );
  }
  if (typeof frame.request_id !== "string") {
    throw new BridgeContractError("INVALID_FRAME", "parseFrame: request_id must be a string");
  }
  if (frame.payload === undefined) {
    throw new BridgeContractError("INVALID_FRAME", "parseFrame: payload is required");
  }
  const decoded: BridgeFrame = {
    protocol_version: frame.protocol_version,
    message_type: frame.message_type,
    request_id: frame.request_id,
    payload: frame.payload,
  };
  if (typeof frame.session_id === "string") {
    decoded.session_id = frame.session_id;
  }
  if (typeof frame.connection_epoch === "number") {
    decoded.connection_epoch = frame.connection_epoch;
  }
  if (typeof frame.grant_epoch === "number") {
    decoded.grant_epoch = frame.grant_epoch;
  }
  if (typeof frame.deadline_ms === "number") {
    decoded.deadline_ms = frame.deadline_ms;
  }
  return decoded;
}

/** Read a typed payload out of a frame. */
export function readPayload<T extends JsonValue>(
  frame: BridgeFrame,
  guard: (value: JsonValue) => value is T,
  code = "INVALID_PAYLOAD",
): T {
  if (!guard(frame.payload)) {
    throw new BridgeContractError(
      code,
      `readPayload: payload does not match ${code.toLowerCase()}`,
    );
  }
  return frame.payload;
}

/** Build a chunk payload with base64 encoded bytes. */
export function createChunk(
  transfer_id: string,
  sequence: number,
  bytes: string,
  total_bytes: number,
  checksum?: string,
): ChunkPayload {
  const chunk: ChunkPayload = {
    transfer_id,
    sequence,
    bytes,
    total_bytes,
  };
  if (checksum !== undefined) {
    chunk.checksum = checksum;
  }
  return chunk;
}
