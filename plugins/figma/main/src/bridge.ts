/**
 * The postMessage bridge between the UI iframe and the main sandbox.
 *
 * The UI owns the socket to the daemon, so every message that reaches the
 * sandbox is untrusted data. Each inbound message is validated against a
 * strict envelope: an unknown channel, an unknown message kind, a malformed
 * payload or an oversized payload is rejected and counted. Nothing that fails
 * the validation ever reaches a handler.
 *
 * The envelope of the channel is the one the UI parses:
 * `{ channel, kind, request_id, payload }`. The discriminator is read from
 * `kind` and, for tolerance with older builds, from `type`.
 *
 * The channel, the envelope and the vocabulary of the answers live in the
 * shared package `@figtether/shared`, so the two sides of the channel cannot
 * drift apart.
 */

import { assertJsonValue, sha256Hex, isCompanionUrl } from "@figtether/shared";
import { BRIDGE_CHANNEL, MESSAGE_TYPES, OUTBOUND_MESSAGE_KINDS } from "@figtether/shared";
import type { BridgeEnvelope, GrantMode, SessionMode } from "@figtether/shared";
import { base64Encode, utf8ByteLength, utf8Encode } from "./encoding.js";
import {
  MAX_FRAME_BYTES,
  MAX_INBOUND_PAYLOAD_BYTES,
  PROTOCOL_VERSION,
  type BridgeFrame,
  type BridgeMessageType,
} from "./types.js";

/** Channel every message of this bridge carries. */
export { BRIDGE_CHANNEL };

/** Message kinds the sandbox accepts from the UI. */
export const INBOUND_KINDS = [
  "open_external",
  "frame",
  "bridge_frame",
  "grant_update",
  "grant_request",
  "cancel",
  "revoke",
  "capabilities",
  "selection_request",
  "status_request",
  "capture_request",
  "ui_ready",
] as const;

/** One accepted inbound message kind. */
export type InboundKind = (typeof INBOUND_KINDS)[number];

/**
 * Message kinds the sandbox sends to the UI.
 *
 * The first six mirror the message types of the Rust bridge frame, so the
 * user interface can forward every answer to the daemon without a rename.
 * The last three are local affordances of the grant flow; they never travel
 * to the daemon.
 */
export const OUTBOUND_KINDS = OUTBOUND_MESSAGE_KINDS;

/** One outbound message kind. */
export type OutboundKind = (typeof OUTBOUND_KINDS)[number];

/**
 * The envelope of every message of this bridge.
 *
 * The shape is the one of the shared contract, the name is kept for the code
 * of the sandbox.
 */
export type { BridgeEnvelope as RelayEnvelope };

/** A validated inbound message. */
export type InboundMessage =
  | { kind: "open_external"; url: string }
  | { kind: "frame"; frame: BridgeFrame }
  | {
      kind: "grant_update";
      rootIds: string[];
      grantEpoch: number;
      /** Mode of the grant: read or write. */
      writeMode: GrantMode;
      /** Session mode of the grant: live or paused. */
      sessionMode: SessionMode;
    }
  | { kind: "grant_request"; rootIds: string[] }
  | { kind: "cancel"; requestId: string }
  | { kind: "revoke"; reason: string }
  | { kind: "capabilities" }
  | { kind: "selection_request" }
  | { kind: "status_request" }
  | { kind: "capture_request" }
  | { kind: "ui_ready" };

/** Why an inbound message was rejected. */
export type RejectionReason =
  "unknown_channel" | "unknown_kind" | "oversized" | "malformed" | "protocol_mismatch";

/** Result of the envelope validation. */
export type ValidationResult =
  { ok: true; message: InboundMessage } | { ok: false; reason: RejectionReason; detail: string };

/** Counters of the bridge, one per rejection reason. */
export interface BridgeCounters {
  /** Messages received from the UI. */
  received: number;
  /** Messages that passed the validation. */
  accepted: number;
  /** Messages that failed the validation. */
  rejected: number;
  /** Rejections because of an unknown channel. */
  rejectedUnknownChannel: number;
  /** Rejections because of an unknown message kind. */
  rejectedUnknownKind: number;
  /** Rejections because the payload was too large. */
  rejectedOversized: number;
  /** Rejections because the payload was malformed. */
  rejectedMalformed: number;
  /** Rejections because the protocol version did not match. */
  rejectedProtocolMismatch: number;
  /** Results sent to the UI. */
  resultsSent: number;
  /** Errors sent to the UI. */
  errorsSent: number;
  /** Chunk frames sent to the UI. */
  chunksSent: number;
  /** Progress frames sent to the UI. */
  progressSent: number;
}

/** Create a counter set with every value at zero. */
export function createCounters(): BridgeCounters {
  return {
    received: 0,
    accepted: 0,
    rejected: 0,
    rejectedUnknownChannel: 0,
    rejectedUnknownKind: 0,
    rejectedOversized: 0,
    rejectedMalformed: 0,
    rejectedProtocolMismatch: 0,
    resultsSent: 0,
    errorsSent: 0,
    chunksSent: 0,
    progressSent: 0,
  };
}

/**
 * The message types a bridge frame may carry.
 *
 * The list is the one of the shared contract, so the sandbox and the daemon
 * cannot drift apart.
 */
export const BRIDGE_MESSAGE_TYPES: readonly BridgeMessageType[] = MESSAGE_TYPES;

/**
 * Validate an inbound envelope.
 *
 * The function is pure: it never throws and never touches the editor.
 */
export function validateInbound(raw: unknown): ValidationResult {
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) {
    return {
      ok: false,
      reason: "malformed",
      detail: "the message is not an object",
    };
  }
  const envelope = raw as Record<string, unknown>;

  const channel = envelope["channel"];
  if (typeof channel !== "string" || channel !== BRIDGE_CHANNEL) {
    return {
      ok: false,
      reason: "unknown_channel",
      detail: "the channel is not part of this bridge",
    };
  }

  const kind = envelope["kind"] ?? envelope["type"];
  if (typeof kind !== "string" || !isInboundKind(kind)) {
    return {
      ok: false,
      reason: "unknown_kind",
      detail: "the message kind is not allowlisted",
    };
  }

  if (!("payload" in envelope)) {
    return {
      ok: false,
      reason: "malformed",
      detail: "the message has no payload",
    };
  }
  const payload = envelope["payload"];
  if (typeof payload !== "object" || payload === null || Array.isArray(payload)) {
    return {
      ok: false,
      reason: "malformed",
      detail: "the payload is not a typed object",
    };
  }
  if (payloadSize(payload) > MAX_INBOUND_PAYLOAD_BYTES) {
    return {
      ok: false,
      reason: "oversized",
      detail: "the payload exceeds the inbound byte budget",
    };
  }

  const requestId = envelope["request_id"];
  if (requestId !== undefined && requestId !== null && typeof requestId !== "string") {
    return {
      ok: false,
      reason: "malformed",
      detail: "the request id is not a string",
    };
  }

  const record = payload as Record<string, unknown>;
  switch (kind) {
    case "open_external":
      if (typeof record["url"] === "string" && isCompanionUrl(record["url"])) {
        return { ok: true, message: { kind: "open_external", url: record["url"] } };
      }
      return {
        ok: false,
        reason: "malformed",
        detail: "external window URL is not the local companion",
      };
    case "frame":
    case "bridge_frame":
      return validateFramePayload(record);
    case "grant_update":
      return validateGrantPayload(record);
    case "grant_request":
      return validateGrantRequestPayload(record);
    case "cancel":
      return validateCancelPayload(record);
    case "revoke":
      return validateRevokePayload(record);
    case "capabilities":
    case "selection_request":
    case "status_request":
    case "capture_request":
    case "ui_ready":
      return { ok: true, message: { kind } };
    default:
      return {
        ok: false,
        reason: "unknown_kind",
        detail: "the message kind is not allowlisted",
      };
  }
}

function validateFramePayload(payload: Record<string, unknown>): ValidationResult {
  const version = payload["protocol_version"];
  if (typeof version !== "string") {
    return {
      ok: false,
      reason: "malformed",
      detail: "the frame has no protocol version",
    };
  }
  if (version !== PROTOCOL_VERSION) {
    return {
      ok: false,
      reason: "protocol_mismatch",
      detail: "the frame speaks a different protocol version",
    };
  }
  const messageType = payload["message_type"];
  if (
    typeof messageType !== "string" ||
    !BRIDGE_MESSAGE_TYPES.includes(messageType as BridgeMessageType)
  ) {
    return {
      ok: false,
      reason: "malformed",
      detail: "the frame carries an unknown message type",
    };
  }
  const requestId = payload["request_id"];
  if (requestId !== null && requestId !== undefined && typeof requestId !== "string") {
    return {
      ok: false,
      reason: "malformed",
      detail: "the frame has no usable request id",
    };
  }
  if (!("payload" in payload)) {
    return {
      ok: false,
      reason: "malformed",
      detail: "the frame has no payload",
    };
  }
  const validated: BridgeFrame = {
    protocol_version: version,
    message_type: messageType as BridgeMessageType,
    request_id: typeof requestId === "string" ? requestId : "",
    payload: null,
  };
  try {
    validated.payload = assertJsonValue(payload["payload"]);
  } catch {
    return {
      ok: false,
      reason: "malformed",
      detail: "the frame payload is not a JSON value",
    };
  }
  const sessionId = payload["session_id"];
  if (typeof sessionId === "string") {
    validated.session_id = sessionId;
  }
  const connectionEpoch = payload["connection_epoch"];
  if (typeof connectionEpoch === "number" && Number.isFinite(connectionEpoch)) {
    validated.connection_epoch = connectionEpoch;
  }
  const grantEpoch = payload["grant_epoch"];
  if (typeof grantEpoch === "number" && Number.isFinite(grantEpoch)) {
    validated.grant_epoch = grantEpoch;
  }
  const deadline = payload["deadline_ms"];
  if (typeof deadline === "number" && Number.isFinite(deadline)) {
    validated.deadline_ms = deadline;
  }
  return { ok: true, message: { kind: "frame", frame: validated } };
}

/**
 * Read the modes of a grant update.
 *
 * The mode of the grant says whether the session may write, the session mode
 * says whether the session is live or paused. A payload that names neither
 * keeps the safe behaviour: the read mode and the live session.
 */
function validateGrantPayload(payload: Record<string, unknown>): ValidationResult {
  const rootIds = readRootIds(payload["root_ids"]);
  if (rootIds === null) {
    return {
      ok: false,
      reason: "malformed",
      detail: "the grant payload carries an invalid root id",
    };
  }
  const grantEpoch = payload["grant_epoch"];
  if (typeof grantEpoch !== "number" || !Number.isFinite(grantEpoch)) {
    return {
      ok: false,
      reason: "malformed",
      detail: "the grant payload carries an invalid epoch",
    };
  }
  const writeMode = readGrantMode(payload["mode"]);
  const sessionMode = readSessionMode(payload["session_mode"]);
  return {
    ok: true,
    message: { kind: "grant_update", rootIds, grantEpoch, writeMode, sessionMode },
  };
}

/** Read the mode of a grant, where a missing mode means the read mode. */
function readGrantMode(value: unknown): GrantMode {
  return value === "write" ? "write" : "read";
}

/** Read the session mode of a grant, where a missing mode means live. */
function readSessionMode(value: unknown): SessionMode {
  return value === "paused" ? "paused" : "live";
}

function validateGrantRequestPayload(payload: Record<string, unknown>): ValidationResult {
  const rootIds = readRootIds(payload["root_ids"]);
  if (rootIds === null) {
    return {
      ok: false,
      reason: "malformed",
      detail: "the grant request carries an invalid root id",
    };
  }
  return { ok: true, message: { kind: "grant_request", rootIds } };
}

function validateCancelPayload(payload: Record<string, unknown>): ValidationResult {
  const requestId = payload["request_id"];
  if (typeof requestId !== "string" || requestId.length === 0) {
    return {
      ok: false,
      reason: "malformed",
      detail: "the cancel payload carries no request id",
    };
  }
  return { ok: true, message: { kind: "cancel", requestId } };
}

function validateRevokePayload(payload: Record<string, unknown>): ValidationResult {
  const reason = payload["reason"];
  if (reason !== undefined && typeof reason !== "string") {
    return {
      ok: false,
      reason: "malformed",
      detail: "the revoke payload carries an invalid reason",
    };
  }
  return {
    ok: true,
    message: { kind: "revoke", reason: typeof reason === "string" ? reason : "user_revoked" },
  };
}

function readRootIds(value: unknown): string[] | null {
  if (!Array.isArray(value)) {
    return null;
  }
  const roots: string[] = [];
  for (const entry of value) {
    if (typeof entry !== "string" || entry.length === 0) {
      return null;
    }
    roots.push(entry);
  }
  return roots;
}

function isInboundKind(value: string): value is InboundKind {
  return (INBOUND_KINDS as readonly string[]).includes(value);
}

/** Size of a payload in bytes, measured the way it travels. */
export function payloadSize(payload: unknown): number {
  if (typeof payload === "string") {
    return utf8ByteLength(payload);
  }
  if (payload === undefined) {
    return 0;
  }
  try {
    return utf8ByteLength(JSON.stringify(payload) ?? "");
  } catch {
    return Number.POSITIVE_INFINITY;
  }
}

/** Raw bytes carried by one chunk frame. */
export const CHUNK_RAW_BYTES = 192 * 1024;

/** Build the envelope of an outbound message. */
export function buildEnvelope(
  kind: OutboundKind,
  payload: unknown,
  requestId: string | null = null,
): BridgeEnvelope {
  return {
    channel: BRIDGE_CHANNEL,
    kind,
    type: kind,
    request_id: requestId,
    payload,
  };
}

/**
 * Send a payload to the UI.
 *
 * A payload that fits into one frame travels as it is. A larger payload
 * travels as chunk frames, and the final chunk carries the checksum of the
 * whole transfer.
 */
export function sendPayload(
  send: (envelope: BridgeEnvelope) => void,
  kind: OutboundKind,
  payload: unknown,
  transferId: string,
  requestId: string | null = null,
): void {
  const serialized = JSON.stringify(payload ?? null);
  const raw = utf8Encode(serialized);
  if (raw.length <= MAX_FRAME_BYTES) {
    send(buildEnvelope(kind, payload, requestId));
    return;
  }
  sendChunked(send, "chunk", raw, transferId, requestId);
}

/**
 * Send a byte payload as chunk frames.
 *
 * The payload of a chunk carries the fields of the bridge chunk contract and,
 * for the capture flow, the field names the UI reassembler reads. Every chunk
 * carries the request id of the transfer, so the user interface can answer
 * the daemon with the request the transfer belongs to.
 */
export function sendChunked(
  send: (envelope: BridgeEnvelope) => void,
  kind: OutboundKind,
  raw: Uint8Array,
  transferId: string,
  requestId: string | null = null,
): void {
  const totalChunks = Math.max(1, Math.ceil(raw.length / CHUNK_RAW_BYTES));
  for (let index = 0; index < totalChunks; index += 1) {
    const start = index * CHUNK_RAW_BYTES;
    const end = Math.min(start + CHUNK_RAW_BYTES, raw.length);
    const slice = raw.subarray(start, end);
    const isLast = index === totalChunks - 1;
    const encoded = base64Encode(slice);
    send(
      buildEnvelope(
        kind,
        {
          transfer_id: transferId,
          sequence: index,
          bytes: encoded,
          bytes_base64: encoded,
          total_bytes: raw.length,
          byte_count: slice.length,
          final: isLast,
          ...(isLast ? { checksum: sha256Hex(raw) } : {}),
        },
        requestId,
      ),
    );
  }
}
