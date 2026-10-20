import { describe, expect, it } from "vitest";
import { sha256, sha256Hex, sha256HexOfText } from "../src/sha256";
import { utf8Encode } from "../src/utf8";

const VECTORS: ReadonlyArray<readonly [string, string]> = [
  ["", "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"],
  ["abc", "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"],
  [
    "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
    "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
  ],
  [
    "The quick brown fox jumps over the lazy dog",
    "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592",
  ],
  ["figtether", "466872a4c910fb1a74aa164811e183aedde1ab463b81e1843b133f89009c8260"],
  ["daemon-side", "f4f7b6c1f6305f5d445e22ec1d06e4004770f342252574ffffe5c3dcd0e0ddd6"],
  ["aä€𝄞", "ea02abd3340fb348ef36ad534e3282a06f7861e6c727d9ba5b2a2172723ab695"],
];

describe("sha256", () => {
  it("matches the published test vectors", () => {
    for (const [input, expected] of VECTORS) {
      expect(sha256HexOfText(input), `vector for ${JSON.stringify(input)}`).toBe(expected);
    }
  });

  it("matches the vector for one million a characters", () => {
    expect(sha256HexOfText("a".repeat(1000000))).toBe(
      "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0",
    );
  });

  it("hashes raw bytes", () => {
    const bytes = Uint8Array.from([0x00, 0x01, 0xc3, 0xbf]);
    expect(sha256Hex(bytes)).toBe(sha256HexOfText("\u0000\u0001\u00ff"));
  });

  it("returns a 32 byte digest", () => {
    expect(sha256(utf8Encode("figtether"))).toHaveLength(32);
  });

  it("produces the same digest for the same input", () => {
    expect(sha256HexOfText("figtether")).toBe(sha256HexOfText("figtether"));
  });

  it("produces a different digest for a different input", () => {
    expect(sha256HexOfText("figtether")).not.toBe(sha256HexOfText("figtethee"));
  });
});
