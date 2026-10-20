import { describe, expect, it } from "vitest";

import {
  base64Decode,
  base64Encode,
  base64UrlDecode,
  base64UrlEncode,
  utf8ByteLength,
  utf8Decode,
  utf8Encode,
} from "../src/encoding.js";

describe("utf8", () => {
  it("encodes and decodes ascii", () => {
    expect(Array.from(utf8Encode("abc"))).toEqual([97, 98, 99]);
    expect(utf8Decode(utf8Encode("abc"))).toBe("abc");
  });

  it("encodes and decodes a multibyte character", () => {
    const bytes = utf8Encode("é");
    expect(Array.from(bytes)).toEqual([0xc3, 0xa9]);
    expect(utf8Decode(bytes)).toBe("é");
  });

  it("encodes and decodes a character outside the basic plane", () => {
    const bytes = utf8Encode("\u{1F600}");
    expect(bytes.length).toBe(4);
    expect(utf8Decode(bytes)).toBe("\u{1F600}");
  });

  it("counts the bytes of a string", () => {
    expect(utf8ByteLength("abc")).toBe(3);
    expect(utf8ByteLength("é")).toBe(2);
    expect(utf8ByteLength("\u{1F600}")).toBe(4);
  });
});

describe("base64", () => {
  it("round trips the bytes", () => {
    const bytes = new Uint8Array([0, 1, 2, 250, 251, 252, 253, 254, 255]);
    expect(Array.from(base64Decode(base64Encode(bytes)))).toEqual(Array.from(bytes));
  });

  it("encodes url safe without padding", () => {
    const bytes = new Uint8Array([251, 255, 190]);
    const encoded = base64UrlEncode(bytes);
    expect(encoded).not.toContain("+");
    expect(encoded).not.toContain("/");
    expect(encoded).not.toContain("=");
    expect(Array.from(base64UrlDecode(encoded))).toEqual(Array.from(bytes));
  });
});
