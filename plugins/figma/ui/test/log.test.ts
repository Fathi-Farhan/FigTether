import { describe, expect, it } from "vitest";

import {
  ERROR_CODES,
  FALLBACK_ERROR_CODE,
  FALLBACK_OPERATION,
  formatLogEntry,
  formatLogLine,
  sanitizeErrorCode,
  sanitizeOperation,
} from "../src/log.js";

describe("formatLogEntry", () => {
  it("uses the fixed template", () => {
    expect(formatLogEntry({ operation: "read", count: 12, durationMs: 340 })).toBe(
      "op=read count=12 duration=340ms",
    );
  });

  it("omits the parts that are absent", () => {
    expect(formatLogEntry({ operation: "heartbeat" })).toBe("op=heartbeat");
    expect(formatLogEntry({ operation: "read", count: 3 })).toBe("op=read count=3");
  });

  it("adds the error code when there is one", () => {
    expect(formatLogEntry({ operation: "read", errorCode: "BUSY" })).toBe("op=read error=BUSY");
  });

  it("never renders a value that is not a number", () => {
    expect(formatLogEntry({ operation: "read", count: Number.NaN })).toBe("op=read");
    expect(formatLogEntry({ operation: "read", durationMs: Number.POSITIVE_INFINITY })).toBe(
      "op=read",
    );
  });

  it("never renders a negative count", () => {
    expect(formatLogEntry({ operation: "read", count: -4 })).toBe("op=read count=0");
  });
});

describe("sanitizeOperation", () => {
  it("keeps a known operation", () => {
    expect(sanitizeOperation("grant_update")).toBe("grant_update");
  });

  it("replaces an unknown operation", () => {
    expect(sanitizeOperation("Layer name")).toBe(FALLBACK_OPERATION);
    expect(sanitizeOperation("")).toBe(FALLBACK_OPERATION);
  });

  it("drops a design value that was passed as an operation", () => {
    const result = sanitizeOperation("Hero section");
    expect(result).not.toContain("Hero");
  });
});

describe("sanitizeErrorCode", () => {
  it("keeps a catalog code", () => {
    expect(sanitizeErrorCode("BUSY")).toBe("BUSY");
    expect(sanitizeErrorCode("busy")).toBe("BUSY");
  });

  it("normalizes an unknown code", () => {
    expect(sanitizeErrorCode("SOMETHING_ELSE")).toBe(FALLBACK_ERROR_CODE);
  });

  it("returns null when there is no code", () => {
    expect(sanitizeErrorCode(null)).toBeNull();
    expect(sanitizeErrorCode(undefined)).toBeNull();
    expect(sanitizeErrorCode("")).toBeNull();
  });

  it("covers the codes of the catalog", () => {
    for (const code of ERROR_CODES) {
      expect(sanitizeErrorCode(code)).toBe(code);
    }
  });
});

describe("formatLogLine", () => {
  it("prefixes the entry with a wall clock label", () => {
    const stamp = new Date(2024, 0, 2, 3, 4, 5).getTime();
    expect(formatLogLine({ operation: "revoke", count: 1 }, stamp)).toBe(
      "03:04:05 op=revoke count=1",
    );
  });
});
