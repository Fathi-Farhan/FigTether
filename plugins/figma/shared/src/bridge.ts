/**
 * Contract of the postMessage channel between the UI iframe and the main
 * sandbox.
 *
 * The two sides of the channel used to carry their own copy of this contract:
 * the sandbox declared the channel, the envelope and the vocabulary of its
 * answers, the user interface declared the channel, the envelope and the
 * vocabulary of the answers it accepts. The copies drifted apart, so the
 * contract is defined once here and both packages derive their lists from it.
 *
 * The channel is not part of the contract with the daemon, it is the path the
 * daemon frames take through the editor. The daemon contract itself lives in
 * `crates/protocol` and is mirrored in `protocol.ts` of this package.
 *
 * Nothing in this file uses a browser API or a Node.js API: the Figma main
 * sandbox has neither.
 */

/** Channel every message of the bridge carries. */
export const BRIDGE_CHANNEL = "figtether.bridge";

/**
 * Message kinds that belong to the contract with the daemon.
 *
 * The names are the message types of the Rust bridge frame, so an answer of
 * the sandbox can be forwarded to the daemon without a rename.
 */
export const DAEMON_MESSAGE_KINDS = [
  "result",
  "error",
  "progress",
  "chunk",
  "capabilities",
  "selection",
] as const;

/** One message kind of the contract with the daemon. */
export type DaemonMessageKind = (typeof DAEMON_MESSAGE_KINDS)[number];

/**
 * Message kinds that never leave the plugin.
 *
 * They are local affordances of the grant flow and of the capture flow. The
 * daemon does not know them, so they must not be mixed up with the kinds of
 * the daemon contract.
 */
export const LOCAL_MESSAGE_KINDS = ["grant_verified", "status", "capture_chunk"] as const;

/** One message kind that never leaves the plugin. */
export type LocalMessageKind = (typeof LOCAL_MESSAGE_KINDS)[number];

/**
 * Every message kind the sandbox sends to the user interface.
 *
 * The list is the vocabulary of the daemon contract followed by the local
 * kinds, so the two halves stay in the order the sandbox uses them.
 */
export const OUTBOUND_MESSAGE_KINDS = [...DAEMON_MESSAGE_KINDS, ...LOCAL_MESSAGE_KINDS] as const;

/** One message kind the sandbox sends to the user interface. */
export type OutboundMessageKind = (typeof OUTBOUND_MESSAGE_KINDS)[number];

/**
 * Message kinds the user interface accepts from the sandbox.
 *
 * The list is the part of the outbound vocabulary that crosses the boundary
 * towards the daemon, so it is exactly the vocabulary of the daemon contract.
 * The local kinds stay inside the plugin and are not accepted here.
 */
export const INBOUND_MESSAGE_KINDS = DAEMON_MESSAGE_KINDS;

/** One message kind the user interface accepts from the sandbox. */
export type InboundMessageKind = (typeof INBOUND_MESSAGE_KINDS)[number];

/**
 * The envelope of every message of the bridge.
 *
 * The discriminator travels in `kind` and, for tolerance with builds that
 * read the other spelling, in `type`. Both sides carry the same value in
 * both fields, and the sandbox accepts either of them.
 *
 * The request id is optional. A message that does not belong to a request,
 * for example the selection of the editor, carries none. A message that
 * answers a request carries the id of that request, so the receiver can
 * forward the answer to the daemon without guessing.
 */
export interface BridgeEnvelope<TPayload = unknown> {
  /** Channel the message belongs to. */
  readonly channel: string;
  /** Message kind. */
  readonly kind: string;
  /** Same value as the kind, kept for the protocol view of the envelope. */
  readonly type: string;
  /** Correlation id, when the message belongs to a request. */
  readonly request_id?: string | null;
  /** JSON payload. */
  readonly payload: TPayload;
}

/**
 * Unwrap the payload of a message posted by the editor.
 *
 * The Figma convention wraps the payload of a message in `pluginMessage`. The
 * wrapper is removed before the validation, so a message that arrives wrapped
 * and a message that arrives raw are both accepted.
 */
export function unwrapPluginMessage(raw: unknown): unknown {
  if (typeof raw === "object" && raw !== null && !Array.isArray(raw) && "pluginMessage" in raw) {
    return (raw as Record<string, unknown>)["pluginMessage"];
  }
  return raw;
}
