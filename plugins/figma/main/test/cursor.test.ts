import { describe, expect, it } from "vitest";

import {
  CURSOR_VERSION,
  decodeCursor,
  encodeCursor,
  parseCursorPayload,
  type CursorState,
} from "../src/cursor.js";
import { PluginError } from "../src/errors.js";
import { base64UrlEncode, utf8Encode } from "../src/encoding.js";

const state: CursorState = {
  v: CURSOR_VERSION,
  grant_epoch: 7,
  root_id: "1:2",
  stack: [
    { id: "1:2", index: 3, visited: true, depth: 1 },
    { id: "1:9", index: 0, visited: false, depth: 2 },
  ],
  visited: 120,
};

describe("pagination cursor", () => {
  it("round trips the traversal state", () => {
    const token = encodeCursor(state);
    const decoded = decodeCursor(token, 7);
    expect(decoded.grant_epoch).toBe(7);
    expect(decoded.root_id).toBe("1:2");
    expect(decoded.visited).toBe(120);
    expect(decoded.stack).toEqual(state.stack);
  });

  it("produces an opaque token that is not the plain state", () => {
    const token = encodeCursor(state);
    expect(token).not.toContain("1:2");
    expect(token).toMatch(/^[A-Za-z0-9_-]+$/);
  });

  it("is stable across calls", () => {
    expect(encodeCursor(state)).toBe(encodeCursor(state));
  });

  it("refuses a cursor of an older grant epoch", () => {
    const token = encodeCursor(state);
    const error = (() => {
      try {
        decodeCursor(token, 8);
        return null;
      } catch (reason) {
        return reason as PluginError;
      }
    })();
    expect(error).toBeInstanceOf(PluginError);
    expect(error?.code).toBe("CURSOR_EXPIRED");
  });

  it("refuses a malformed token", () => {
    const error = (() => {
      try {
        decodeCursor("not-a-cursor", 7);
        return null;
      } catch (reason) {
        return reason as PluginError;
      }
    })();
    expect(error?.code).toBe("CURSOR_EXPIRED");
  });

  it("refuses a cursor of an unknown format version", () => {
    const token = base64UrlEncode(
      utf8Encode(
        JSON.stringify({
          v: 99,
          grant_epoch: 7,
          root_id: "1:2",
          stack: [],
          visited: 0,
        }),
      ),
    );
    expect(() => parseCursorPayload(token)).toThrowError(/unsupported format/i);
  });
});
