import { describe, expect, it } from "vitest";
import { base64Decode, base64DecodeText, base64Encode, base64EncodeText } from "../src/base64";

describe("base64", () => {
  it("matches the Rust test vector", () => {
    expect(base64Encode(Uint8Array.from([0x00, 0x01, 0x02, 0x03]))).toBe("AAECAw==");
  });

  it("encodes the standard vectors", () => {
    expect(base64Encode(new Uint8Array([]))).toBe("");
    expect(base64EncodeText("f")).toBe("Zg==");
    expect(base64EncodeText("fo")).toBe("Zm8=");
    expect(base64EncodeText("foo")).toBe("Zm9v");
    expect(base64EncodeText("foob")).toBe("Zm9vYg==");
    expect(base64EncodeText("fooba")).toBe("Zm9vYmE=");
    expect(base64EncodeText("foobar")).toBe("Zm9vYmFy");
  });

  it("decodes the standard vectors", () => {
    expect(base64DecodeText("Zg==")).toBe("f");
    expect(base64DecodeText("Zm8=")).toBe("fo");
    expect(base64DecodeText("Zm9v")).toBe("foo");
    expect(base64DecodeText("Zm9vYmFy")).toBe("foobar");
  });

  it("round trips arbitrary bytes", () => {
    const bytes = Uint8Array.from({ length: 256 }, (_value, index) => index);
    expect(base64Decode(base64Encode(bytes))).toEqual(bytes);
  });

  it("round trips text with multi byte characters", () => {
    expect(base64DecodeText(base64EncodeText("aä€𝄞"))).toBe("aä€𝄞");
  });

  it("rejects malformed input", () => {
    expect(() => base64Decode("Zg=")).toThrow(/multiple of 4/);
    expect(() => base64Decode("Zm9!")).toThrow(/valid base64/);
    expect(() => base64Decode("Z=m9")).toThrow(/valid base64/);
  });
});
