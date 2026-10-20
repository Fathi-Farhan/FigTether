import { sha256Hex } from "@figtether/shared";
import { describe, expect, it } from "vitest";

import { splitIntoChunks, TransferAssembler } from "../src/transfer.js";

function bytes(length: number, seed = 7): Uint8Array {
  const out = new Uint8Array(length);
  for (let index = 0; index < length; index += 1) {
    out[index] = (index * seed) % 251;
  }
  return out;
}

describe("TransferAssembler", () => {
  it("reassembles an ordered transfer", () => {
    const assembler = new TransferAssembler();
    const payload = bytes(10);
    expect(assembler.begin("tr-01", payload.length)).toBe(true);
    expect(assembler.accept({ sequence: 0, bytes: payload.slice(0, 4), checksum: null })).toBe(
      "accepted",
    );
    expect(
      assembler.accept({ sequence: 1, bytes: payload.slice(4), checksum: sha256Hex(payload) }),
    ).toBe("accepted");
    expect(assembler.complete).toBe(true);
    const result = assembler.finish();
    expect(result).not.toBeNull();
    expect(Array.from(result as Uint8Array)).toEqual(Array.from(payload));
  });

  it("refuses a chunk that arrives out of order", () => {
    const assembler = new TransferAssembler();
    assembler.begin("tr-01", 8);
    expect(assembler.accept({ sequence: 1, bytes: bytes(4), checksum: null })).toBe("out_of_order");
  });

  it("refuses a chunk that arrives twice", () => {
    const assembler = new TransferAssembler();
    assembler.begin("tr-01", 8);
    assembler.accept({ sequence: 0, bytes: bytes(4), checksum: null });
    expect(assembler.accept({ sequence: 0, bytes: bytes(4), checksum: null })).toBe("duplicate");
  });

  it("refuses a chunk that exceeds the declared size", () => {
    const assembler = new TransferAssembler();
    assembler.begin("tr-01", 4);
    expect(assembler.accept({ sequence: 0, bytes: bytes(8), checksum: null })).toBe("overflow");
  });

  it("refuses a transfer that was never started", () => {
    const assembler = new TransferAssembler();
    expect(assembler.accept({ sequence: 0, bytes: bytes(1), checksum: null })).toBe(
      "unknown_transfer",
    );
  });

  it("refuses a declared size above the transfer ceiling", () => {
    const assembler = new TransferAssembler();
    expect(assembler.begin("tr-01", 64 * 1024 * 1024)).toBe(false);
    expect(assembler.open).toBe(false);
  });

  it("discards a transfer with a wrong checksum", () => {
    const assembler = new TransferAssembler();
    const payload = bytes(4);
    assembler.begin("tr-01", payload.length);
    assembler.accept({ sequence: 0, bytes: payload, checksum: "not-the-checksum" });
    expect(assembler.finish()).toBeNull();
    expect(assembler.open).toBe(false);
  });
});

describe("splitIntoChunks", () => {
  it("keeps every chunk inside the frame cap", () => {
    const payload = bytes(2500);
    const chunks = splitIntoChunks(payload, "tr-01", 1000);
    expect(chunks).toHaveLength(3);
    for (const chunk of chunks) {
      expect(chunk.bytes.length).toBeLessThanOrEqual(1000);
      expect(chunk.totalBytes).toBe(payload.length);
    }
    expect(chunks[0]?.checksum).toBeNull();
    expect(chunks[2]?.checksum).toBe(sha256Hex(payload));
  });

  it("puts the checksum of the whole payload on the final chunk", () => {
    const payload = bytes(100);
    const chunks = splitIntoChunks(payload, "tr-01", 60);
    expect(chunks).toHaveLength(2);
    expect(chunks[0]?.checksum).toBeNull();
    expect(chunks[chunks.length - 1]?.checksum).toBe(sha256Hex(payload));
  });

  it("handles an empty payload", () => {
    const chunks = splitIntoChunks(new Uint8Array(0), "tr-01");
    expect(chunks).toHaveLength(1);
    expect(chunks[0]?.bytes.length).toBe(0);
    expect(chunks[0]?.checksum).toBe(sha256Hex(new Uint8Array(0)));
  });
});
