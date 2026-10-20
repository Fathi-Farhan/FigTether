import { describe, expect, it } from "vitest";
import { canonicalStringify, parseJsonValue } from "../src/canonical";
import type { JsonValue } from "../src/json";

describe("canonicalStringify", () => {
  it("sorts object keys", () => {
    const value: JsonValue = { b: 1, a: 2, c: 3 };
    expect(canonicalStringify(value)).toBe('{"a":2,"b":1,"c":3}');
  });

  it("sorts nested object keys", () => {
    const value: JsonValue = { outer: { z: 1, a: { y: 2, b: 3 } } };
    expect(canonicalStringify(value)).toBe('{"outer":{"a":{"b":3,"y":2},"z":1}}');
  });

  it("keeps array order", () => {
    const value: JsonValue = { list: [3, 1, 2] };
    expect(canonicalStringify(value)).toBe('{"list":[3,1,2]}');
  });

  it("encodes primitives", () => {
    expect(canonicalStringify(null)).toBe("null");
    expect(canonicalStringify(true)).toBe("true");
    expect(canonicalStringify(false)).toBe("false");
    expect(canonicalStringify(42)).toBe("42");
    expect(canonicalStringify("text")).toBe('"text"');
  });

  it("escapes strings like JSON", () => {
    expect(canonicalStringify('a"b\\c\nd')).toBe('"a\\"b\\\\c\\nd"');
  });

  it("is stable across key insertion order", () => {
    const first: JsonValue = { alpha: 1, beta: { x: 1, y: 2 } };
    const second: JsonValue = { beta: { y: 2, x: 1 }, alpha: 1 };
    expect(canonicalStringify(first)).toBe(canonicalStringify(second));
  });

  it("rejects non finite numbers", () => {
    expect(() => canonicalStringify(Number.NaN)).toThrow(/finite/);
    expect(() => canonicalStringify(Number.POSITIVE_INFINITY)).toThrow(/finite/);
  });
});

describe("parseJsonValue", () => {
  it("parses JSON into a JSON value", () => {
    const parsed = parseJsonValue('{"b":[1,2],"a":null}');
    expect(parsed).toEqual({ b: [1, 2], a: null });
  });

  it("rejects invalid JSON", () => {
    expect(() => parseJsonValue("{")).toThrow(/invalid JSON/);
  });
});
