/**
 * Pagination cursors.
 *
 * A cursor is an opaque token that binds a resumable traversal to the grant
 * that produced it. The daemon never interprets it, it only stores and
 * returns it. The plugin encodes the traversal position together with the
 * grant epoch, so a cursor that outlives its grant is refused instead of
 * silently reading a different scope.
 */

import { assertJsonValue } from "@figtether/shared";
import { base64UrlDecode, base64UrlEncode, utf8Decode, utf8Encode } from "./encoding.js";
import { cursorExpired } from "./errors.js";
import { canonicalJson } from "./canonical.js";
import { asRecord } from "./figma-api.js";

/** Version marker of the cursor format. */
export const CURSOR_VERSION = 1;

/**
 * One level of the traversal stack.
 *
 * `visited` marks a frame whose node was already reported by an earlier
 * page, so a resumed traversal never counts or reports it twice.
 */
export interface CursorFrame {
  /** Node id of the frame. */
  id: string;
  /** Index of the next child to process. */
  index: number;
  /** Whether the node of this frame was already visited. */
  visited: boolean;
  /** Depth of the node in the original traversal. */
  depth: number;
}

/** Everything a cursor carries. */
export interface CursorState {
  /** Format version. */
  v: number;
  /** Grant epoch the cursor is bound to. */
  grant_epoch: number;
  /** Root the traversal started from. */
  root_id: string;
  /** Remaining traversal stack. */
  stack: CursorFrame[];
  /** Nodes visited so far. */
  visited: number;
}

/** Encode a cursor state into an opaque token. */
export function encodeCursor(state: CursorState): string {
  const payload = {
    v: CURSOR_VERSION,
    grant_epoch: state.grant_epoch,
    root_id: state.root_id,
    stack: state.stack.map((frame) => ({
      id: frame.id,
      index: frame.index,
      visited: frame.visited,
      depth: frame.depth,
    })),
    visited: state.visited,
  };
  return base64UrlEncode(utf8Encode(canonicalJson(assertJsonValue(payload))));
}

/**
 * Decode a cursor token.
 *
 * Throws a `CURSOR_EXPIRED` plugin error when the token is malformed or when
 * it was issued under a different grant epoch.
 */
export function decodeCursor(token: string, grantEpoch: number): CursorState {
  const parsed = parseCursorPayload(token);
  if (parsed.grant_epoch !== grantEpoch) {
    throw cursorExpired("The pagination cursor was issued under an older grant epoch");
  }
  return parsed;
}

/** Decode a cursor token without checking the grant epoch. */
export function parseCursorPayload(token: string): CursorState {
  let text: string;
  try {
    text = utf8Decode(base64UrlDecode(token));
  } catch {
    throw cursorExpired("The pagination cursor is malformed");
  }
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    throw cursorExpired("The pagination cursor is malformed");
  }
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw cursorExpired("The pagination cursor is malformed");
  }
  const record = value as Record<string, unknown>;
  if (record["v"] !== CURSOR_VERSION) {
    throw cursorExpired("The pagination cursor uses an unsupported format");
  }
  const grantEpoch = record["grant_epoch"];
  const rootId = record["root_id"];
  const stack = record["stack"];
  const visited = record["visited"];
  if (
    typeof grantEpoch !== "number" ||
    !Number.isFinite(grantEpoch) ||
    typeof rootId !== "string" ||
    rootId.length === 0 ||
    !Array.isArray(stack) ||
    typeof visited !== "number" ||
    !Number.isFinite(visited)
  ) {
    throw cursorExpired("The pagination cursor is malformed");
  }
  const frames: CursorFrame[] = [];
  for (const entry of stack) {
    const record = asRecord(entry);
    if (record === null) {
      throw cursorExpired("The pagination cursor is malformed");
    }
    const id = record["id"];
    const index = record["index"];
    const visited = record["visited"];
    const depth = record["depth"];
    if (
      typeof id !== "string" ||
      id.length === 0 ||
      typeof index !== "number" ||
      !Number.isFinite(index) ||
      typeof visited !== "boolean" ||
      typeof depth !== "number" ||
      !Number.isFinite(depth)
    ) {
      throw cursorExpired("The pagination cursor is malformed");
    }
    frames.push({ id, index, visited, depth });
  }
  return {
    v: CURSOR_VERSION,
    grant_epoch: grantEpoch,
    root_id: rootId,
    stack: frames,
    visited,
  };
}
