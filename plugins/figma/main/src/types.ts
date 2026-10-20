/**
 * Wire and payload types of the main sandbox.
 *
 * The contract itself lives in the shared package `@figtether/shared`, which
 * mirrors `crates/protocol`. This module re-exports the parts the sandbox
 * uses and adds the two pieces that belong to the adapter only: the closed
 * vocabulary of the availability reasons and the helper that renders one
 * `unavailable_fields` entry.
 */

export {
  BRIDGE_OPS,
  MAX_FRAME_BYTES,
  MAX_TRANSFER_BYTES,
  PROTOCOL_VERSION,
} from "@figtether/shared";

export type {
  BridgeFrame,
  BridgeOp,
  ChunkPayload,
  ErrorPayload,
  GrantUpdatePayload,
  MessageType,
  MessageType as BridgeMessageType,
  PluginComponent,
  PluginDocument,
  PluginEffect,
  PluginExport,
  PluginGeometry,
  PluginGradientStop,
  PluginLayout,
  PluginNode,
  PluginPaint,
  PluginText,
  PluginTextRun,
  PluginVariable,
  PluginVariantProperty,
  ResultPayload,
  RevokePayload,
} from "@figtether/shared";

export type { CapabilityMap, CapabilityState } from "@figtether/shared";

/** Maximum payload size accepted on the postMessage channel, in bytes. */
export const MAX_INBOUND_PAYLOAD_BYTES = 1024 * 1024;

/** Reason vocabulary for an unavailable field. */
export const UNAVAILABLE_REASON = {
  /** API, editor, or seat does not expose the property. */
  CAPABILITY_UNAVAILABLE: "capability_unavailable",
  /** Omitted by the caller's field projection, not a data gap. */
  PROJECTION_OMITTED: "projection_omitted",
  /** Property is meaningless for this node type. */
  NOT_APPLICABLE: "not_applicable",
  /** Representable only visually. */
  VISUAL_GAP: "visual_gap",
  /** Entity is outside the grant. */
  NOT_IN_GRANT: "not_in_grant",
} as const;

/** One reason of the closed availability vocabulary. */
export type UnavailableReason = (typeof UNAVAILABLE_REASON)[keyof typeof UNAVAILABLE_REASON];

/**
 * Build one `unavailable_fields` entry.
 *
 * The Rust contract carries `unavailable_fields` as a list of strings, so the
 * path and the reason travel together: `"typography.runs[0].font_size:
 * capability_unavailable"`.
 */
export function unavailable(path: string, reason: UnavailableReason): string {
  return `${path}:${reason}`;
}
