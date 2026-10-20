import { describe, expect, it } from "vitest";

import { sha256Hex } from "@figtether/shared";
import type { JsonValue } from "@figtether/shared";
import { canonicalJson, contentHash } from "../src/canonical.js";
import { utf8Encode } from "../src/encoding.js";

describe("canonical json", () => {
  it("sorts the keys of an object", () => {
    expect(canonicalJson({ b: 1, a: 2 })).toBe('{"a":2,"b":1}');
  });

  it("produces the same text for two equal values", () => {
    const left = { name: "frame", children: ["1:2"], size: 10 };
    const right = { size: 10, children: ["1:2"], name: "frame" };
    expect(canonicalJson(left)).toBe(canonicalJson(right));
  });

  it("keeps the order of the array members", () => {
    expect(canonicalJson([3, 1, 2])).toBe("[3,1,2]");
  });

  it("refuses a member that has no value", () => {
    const broken = { a: 1, b: undefined } as unknown as JsonValue;
    expect(() => canonicalJson(broken)).toThrowError();
  });

  it("refuses a number that is not finite", () => {
    expect(() => canonicalJson({ a: Number.NaN })).toThrowError();
  });

  it("escapes the strings", () => {
    expect(canonicalJson({ a: 'quote"' })).toBe('{"a":"quote\\""}');
  });
});

describe("content hash", () => {
  it("is the sha256 of the canonical json", () => {
    const value = { b: 2, a: 1 };
    const expected = sha256Hex(utf8Encode(canonicalJson(value)));
    expect(contentHash(value)).toBe(`sha256:${expected}`);
  });

  it("does not depend on the key order", () => {
    expect(contentHash({ a: 1, b: 2 })).toBe(contentHash({ b: 2, a: 1 }));
  });

  it("changes when a value changes", () => {
    expect(contentHash({ a: 1 })).not.toBe(contentHash({ a: 2 }));
  });

  it("uses the lower case hexadecimal digest", () => {
    expect(contentHash({ a: 1 })).toMatch(/^sha256:[0-9a-f]{64}$/);
  });
});
