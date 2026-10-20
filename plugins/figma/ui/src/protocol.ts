/**
 * Contract of the plugin user interface.
 *
 * The wire types come from the shared package `@figtether/shared`, which mirrors
 * `crates/protocol`. Only the parts that belong to the user interface live
 * here: the postMessage channel between the UI and the main sandbox, and the
 * payloads of the messages that the two sides exchange on that channel.
 */

import { BRIDGE_CHANNEL, INBOUND_MESSAGE_KINDS } from "@figtether/shared";
import type { CapabilityMap, GrantUpdatePayload } from "@figtether/shared";

export {
  AUTH_WINDOW_MS,
  BRIDGE_CHANNEL,
  HEARTBEAT_INTERVAL_MS,
  MAX_FRAME_BYTES,
  MAX_TRANSFER_BYTES,
  MISSED_HEARTBEATS_BEFORE_OFFLINE,
  PROTOCOL_VERSION,
  isBridgeOp,
  isMessageType,
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
  readonly session_credential: string;
  readonly session_id: string;
  readonly connection_epoch: number;
  readonly grant_epoch: number;
  readonly capabilities: CapabilityMap;
  readonly session_name: string | null;
}

/** Loopback bridge endpoint owned by the daemon. */
export const BRIDGE_URL = "ws://127.0.0.1:38947/bridge";

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

/** Operating mode of a grant. */
export type GrantMode = "live" | "paused";

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
 * The frozen contract carries the root ids and the epoch. The mode is the
 * extension of `docs/02-architecture/02-bridge-design.md` section 6, which
 * describes a grant update that carries the live or the paused mode.
 */
export interface UiGrantUpdate extends GrantUpdatePayload {
  readonly mode: GrantMode;
}
