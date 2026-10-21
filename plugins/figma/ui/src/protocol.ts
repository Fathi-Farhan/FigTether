/**
 * Contract of the plugin user interface.
 *
 * The wire types come from the shared package `@figtether/shared`, which mirrors
 * `crates/protocol`. Only the parts that belong to the user interface live
 * here: the postMessage channel between the UI and the main sandbox, and the
 * payloads of the messages that the two sides exchange on that channel.
 */

import { BRIDGE_CHANNEL, INBOUND_MESSAGE_KINDS, LOCAL_BRIDGE_PORT } from "@figtether/shared";
import type {
  BridgeFrame,
  CapabilityMap,
  GrantMode as WriteGrantMode,
  GrantUpdatePayload,
  MessageType,
  SessionMode,
} from "@figtether/shared";

export {
  AUTH_WINDOW_MS,
  BRIDGE_CHANNEL,
  HEARTBEAT_INTERVAL_MS,
  MAX_FRAME_BYTES,
  MAX_TRANSFER_BYTES,
  MISSED_HEARTBEATS_BEFORE_OFFLINE,
  PROTOCOL_VERSION,
  createFrame,
  isBridgeOp,
  isMessageType,
  serializeFrame,
} from "@figtether/shared";

export type {
  AuthAccepted,
  AuthRejected,
  AuthRequest,
  BridgeFrame,
  CapabilityState,
  ChunkPayload,
  ErrorPayload,
  JsonValue,
  MessageType,
  ResultPayload,
  RevokePayload,
} from "@figtether/shared";

export type {
  WriteApproval,
  WriteOperation,
  WriteOperationKind,
  WritePlan,
  WritePlanPayload,
  WritePreview,
  WriteResult,
} from "@figtether/shared";

/**
 * Mode of a grant on the write path.
 *
 * The mode of a grant on the read path, which is `live` or `paused`, is a
 * different thing and lives in this module under the name {@link SessionMode}.
 * The shared contract names the mode of the write path `GrantMode` as well,
 * so it is renamed here to keep the two apart.
 */
export type { GrantMode as WriteGrantMode } from "@figtether/shared";

/**
 * The envelope of every message on the channel.
 *
 * The shape is the one of the shared contract, the name is kept for the code
 * of the user interface.
 */
export type { BridgeEnvelope as UiEnvelope } from "@figtether/shared";

export type { CapabilityMap, GrantUpdatePayload };

/**
 * A structured error as the user interface uses it.
 *
 * The wire shape is the error payload of the shared contract. The user
 * interface keeps the optional fields nullable, so that a daemon that leaves
 * them out never produces an undefined value in the interface.
 */
export interface UiError {
  readonly code: string;
  readonly message: string;
  readonly retryable: boolean;
  readonly action: string;
  readonly request_id: string | null;
  readonly retry_after_ms: number | null;
}

/**
 * The handshake answer as the user interface uses it.
 *
 * The wire shape is the accepted handshake of the shared contract. The user
 * interface adds the optional session name, which the daemon may report with
 * the capabilities of the session.
 */
export interface AuthAcceptedPayload {
  readonly ui_features?: readonly string[];
  readonly session_credential: string;
  readonly session_id: string;
  readonly connection_epoch: number;
  readonly grant_epoch: number;
  readonly capabilities: CapabilityMap;
  readonly session_name: string | null;
}

/** Loopback bridge endpoint owned by the daemon. */
export const BRIDGE_URL = `ws://localhost:${LOCAL_BRIDGE_PORT}/bridge`;

/**
 * Channel of the postMessage bridge between the UI and the main sandbox.
 *
 * The value comes from the shared contract, so the two sides of the channel
 * cannot drift apart.
 */
export const UI_CHANNEL = BRIDGE_CHANNEL;

/** Maximum size of a postMessage payload that the UI accepts. */
export const MAX_ENVELOPE_BYTES = 1_048_576;

/** Message types the UI sends to the main sandbox. */
export const UI_OUTBOUND_TYPES = [
  "frame",
  "grant_update",
  "cancel",
  "revoke",
  "capabilities",
  "selection_request",
] as const;

/** One message type the UI sends to the main sandbox. */
export type UiOutboundType = (typeof UI_OUTBOUND_TYPES)[number];

/**
 * Message types the UI accepts from the main sandbox.
 *
 * The list comes from the shared contract, so it is exactly the part of the
 * vocabulary of the sandbox that crosses the boundary towards the daemon:
 * every answer of the sandbox can be forwarded to the daemon with the same
 * name. `selection` is the answer to `selection_request` and belongs to the
 * local grant flow.
 */
export const UI_INBOUND_TYPES = INBOUND_MESSAGE_KINDS;

/** One message type the UI accepts from the main sandbox. */
export type UiInboundType = (typeof UI_INBOUND_TYPES)[number];

/** A candidate root of the current selection. */
export interface CandidateRoot {
  readonly name?: string;
  readonly node_id: string;
  readonly node_type: string;
  readonly child_count: number | null;
}

/** The candidate roots of the current selection. */
export interface SelectionPayload {
  readonly roots: readonly CandidateRoot[];
}

/** Read mode of a session. */
export type ReadMode = "live" | "snapshot" | "offline_bundle" | "unknown";

/**
 * Session mode of a grant.
 *
 * The session mode is the pause extension of the read path: it says whether
 * the session is live or paused. It is not the mode of the grant, which says
 * whether the session may write.
 */
export type { SessionMode };

/** The live status facts of a session. */
export interface StatusPayload {
  readonly queue_depth: number | null;
  readonly last_read_at: number | null;
  readonly mode: ReadMode | null;
  readonly capabilities: CapabilityMap | null;
}

/**
 * The grant payload that the UI sends.
 *
 * The frozen contract carries the root ids and the epoch. The two modes are
 * two different fields: the mode of the grant says whether the session may
 * write, and the session mode says whether the session is live or paused.
 * Both fields are additive, so a peer that does not know them keeps the
 * safe behaviour: the read mode and the live session.
 */
export interface UiGrantUpdate extends GrantUpdatePayload {
  /** Mode of the grant: read or write. */
  readonly mode: WriteGrantMode;
  /** Session mode of the grant: live or paused. */
  readonly session_mode: SessionMode;
}

/**
 * The message types the interface may send on the socket.
 *
 * The list is the vocabulary of the shared contract, and it carries the
 * approval decision of the user: the kind travels from the interface to the
 * daemon, so the interface needs it and the sandbox does not.
 */
export type UiMessageType = MessageType;

/** A frame the interface sends on the socket. */
export type UiBridgeFrame = BridgeFrame;

/** Options of a frame the interface sends. */
export interface UiFrameOptions {
  /** Session the frame belongs to. */
  readonly session_id?: string;
  /** Connection epoch of the sender. */
  readonly connection_epoch?: number;
  /** Grant epoch in force. */
  readonly grant_epoch?: number;
}
