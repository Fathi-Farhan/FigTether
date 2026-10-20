import { describe, expect, it } from "vitest";
import { base64Decode, base64Encode } from "../src/base64";
import {
  BridgeContractError,
  createChunk,
  createFrame,
  parseFrame,
  serializeFrame,
} from "../src/frame";
import type { JsonValue } from "../src/json";
import { PROTOCOL_VERSION } from "../src/protocol";

describe("bridge frame", () => {
  it("round trips through the canonical encoding", () => {
    const frame = createFrame("heartbeat", "req_1", {
      sequence: 1,
      timestamp: "2026-01-01T00:00:00Z",
    });
    const decoded = parseFrame(serializeFrame(frame));
    expect(decoded).toEqual(frame);
  });

  it("keeps the optional envelope fields", () => {
    const frame = createFrame(
      "result",
      "req_2",
      { data: null },
      {
        session_id: "session-1",
        connection_epoch: 2,
        grant_epoch: 3,
        deadline_ms: 20000,
      },
    );
    const decoded = parseFrame(serializeFrame(frame));
    expect(decoded.session_id).toBe("session-1");
    expect(decoded.connection_epoch).toBe(2);
    expect(decoded.grant_epoch).toBe(3);
    expect(decoded.deadline_ms).toBe(20000);
  });

  it("omits the optional envelope fields that are absent", () => {
    const frame = createFrame("result", "req_3", { data: null });
    const text = serializeFrame(frame);
    expect(text).not.toContain("session_id");
    expect(text).not.toContain("grant_epoch");
    expect(parseFrame(text)).toEqual(frame);
  });

  it("carries the protocol version", () => {
    const frame = createFrame("capabilities", "req_4", { capabilities: {} });
    expect(frame.protocol_version).toBe(PROTOCOL_VERSION);
    expect(serializeFrame(frame)).toContain('"protocol_version":"1.0"');
  });

  it("rejects a protocol mismatch", () => {
    const text = JSON.stringify({
      protocol_version: "0.9",
      message_type: "heartbeat",
      request_id: "req_5",
      payload: {},
    });
    expect(() => parseFrame(text)).toThrow(BridgeContractError);
    expect(() => parseFrame(text)).toThrow(/protocol version/);
  });

  it("rejects an unknown message type", () => {
    const text = JSON.stringify({
      protocol_version: PROTOCOL_VERSION,
      message_type: "not_a_message",
      request_id: "req_6",
      payload: {},
    });
    expect(() => parseFrame(text)).toThrow(/unknown message type/);
  });

  it("rejects a frame without a payload", () => {
    const text = JSON.stringify({
      protocol_version: PROTOCOL_VERSION,
      message_type: "heartbeat",
      request_id: "req_7",
    });
    expect(() => parseFrame(text)).toThrow(/payload is required/);
  });

  it("sorts the keys of the canonical encoding", () => {
    const frame = createFrame("result", "req_8", { b: 1, a: 2 } satisfies JsonValue);
    expect(serializeFrame(frame)).toBe(
      '{"message_type":"result","payload":{"a":2,"b":1},"protocol_version":"1.0","request_id":"req_8"}',
    );
  });
});

describe("chunk payload", () => {
  it("round trips base64 encoded bytes", () => {
    const bytes = Uint8Array.from([0x00, 0x01, 0xff]);
    const chunk = createChunk("t1", 0, base64Encode(bytes), bytes.length);
    const decoded = base64Decode(chunk.bytes);
    expect(decoded).toEqual(bytes);
    expect(chunk.total_bytes).toBe(3);
  });
});
