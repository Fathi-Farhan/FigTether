import { describe, expect, it } from "vitest";

import {
  BRIDGE_CHANNEL,
  buildEnvelope,
  OUTBOUND_KINDS,
  payloadSize,
  sendChunked,
  validateInbound,
  type RelayEnvelope,
} from "../src/bridge.js";
import { MAX_INBOUND_PAYLOAD_BYTES } from "../src/types.js";
import { base64Decode } from "../src/encoding.js";

function frame(messageType = "request"): Record<string, unknown> {
  return {
    protocol_version: "1.0",
    message_type: messageType,
    request_id: "req_1",
    session_id: "session_1",
    connection_epoch: 2,
    grant_epoch: 3,
    deadline_ms: 20000,
    payload: { op: "list_nodes", params: { root_id: "1:2" } },
  };
}

describe("inbound envelope validation", () => {
  it("accepts a relayed bridge frame", () => {
    const result = validateInbound({
      channel: BRIDGE_CHANNEL,
      kind: "bridge_frame",
      request_id: null,
      payload: frame(),
    });
    expect(result.ok).toBe(true);
    if (!result.ok) {
      return;
    }
    expect(result.message.kind).toBe("frame");
    if (result.message.kind !== "frame") {
      return;
    }
    expect(result.message.frame.message_type).toBe("request");
    expect(result.message.frame.request_id).toBe("req_1");
    expect(result.message.frame.grant_epoch).toBe(3);
  });

  it("accepts the frame kind as well", () => {
    const result = validateInbound({
      channel: BRIDGE_CHANNEL,
      kind: "frame",
      payload: frame("grant_update"),
    });
    expect(result.ok).toBe(true);
  });

  it("accepts the type alias of the discriminator", () => {
    const result = validateInbound({
      channel: BRIDGE_CHANNEL,
      type: "capabilities",
      payload: {},
    });
    expect(result.ok).toBe(true);
  });

  it("refuses a message of another channel", () => {
    const result = validateInbound({
      channel: "some.other.channel",
      kind: "capabilities",
      payload: {},
    });
    expect(result.ok).toBe(false);
    if (result.ok) {
      return;
    }
    expect(result.reason).toBe("unknown_channel");
  });

  it("refuses an unknown message kind", () => {
    const result = validateInbound({
      channel: BRIDGE_CHANNEL,
      kind: "delete_everything",
      payload: {},
    });
    expect(result.ok).toBe(false);
    if (result.ok) {
      return;
    }
    expect(result.reason).toBe("unknown_kind");
  });

  it("refuses a payload that is not an object", () => {
    const result = validateInbound({
      channel: BRIDGE_CHANNEL,
      kind: "capabilities",
      payload: "not an object",
    });
    expect(result.ok).toBe(false);
    if (result.ok) {
      return;
    }
    expect(result.reason).toBe("malformed");
  });

  it("refuses a message without a payload", () => {
    const result = validateInbound({
      channel: BRIDGE_CHANNEL,
      kind: "capabilities",
    });
    expect(result.ok).toBe(false);
    if (result.ok) {
      return;
    }
    expect(result.reason).toBe("malformed");
  });

  it("refuses an oversized payload", () => {
    const result = validateInbound({
      channel: BRIDGE_CHANNEL,
      kind: "capabilities",
      payload: { blob: "x".repeat(MAX_INBOUND_PAYLOAD_BYTES + 16) },
    });
    expect(result.ok).toBe(false);
    if (result.ok) {
      return;
    }
    expect(result.reason).toBe("oversized");
  });

  it("refuses a frame of another protocol version", () => {
    const result = validateInbound({
      channel: BRIDGE_CHANNEL,
      kind: "bridge_frame",
      payload: { ...frame(), protocol_version: "0.9" },
    });
    expect(result.ok).toBe(false);
    if (result.ok) {
      return;
    }
    expect(result.reason).toBe("protocol_mismatch");
  });

  it("refuses a frame with an unknown message type", () => {
    const result = validateInbound({
      channel: BRIDGE_CHANNEL,
      kind: "bridge_frame",
      payload: { ...frame(), message_type: "teleport" },
    });
    expect(result.ok).toBe(false);
    if (result.ok) {
      return;
    }
    expect(result.reason).toBe("malformed");
  });

  it("accepts a grant update with the roots and the epoch", () => {
    const result = validateInbound({
      channel: BRIDGE_CHANNEL,
      kind: "grant_update",
      payload: {
        root_ids: ["1:2", "1:3"],
        grant_epoch: 4,
        mode: "write",
        session_mode: "paused",
      },
    });
    expect(result.ok).toBe(true);
    if (!result.ok || result.message.kind !== "grant_update") {
      return;
    }
    expect(result.message.rootIds).toEqual(["1:2", "1:3"]);
    expect(result.message.grantEpoch).toBe(4);
    expect(result.message.writeMode).toBe("write");
    expect(result.message.sessionMode).toBe("paused");
  });

  it("keeps the read only behaviour of a grant that names no mode", () => {
    const result = validateInbound({
      channel: BRIDGE_CHANNEL,
      kind: "grant_update",
      payload: { root_ids: ["1:2"], grant_epoch: 4 },
    });
    expect(result.ok).toBe(true);
    if (!result.ok || result.message.kind !== "grant_update") {
      return;
    }
    expect(result.message.writeMode).toBe("read");
    expect(result.message.sessionMode).toBe("live");
  });

  it("refuses a grant update with an invalid root", () => {
    const result = validateInbound({
      channel: BRIDGE_CHANNEL,
      kind: "grant_update",
      payload: { root_ids: [42], grant_epoch: 4 },
    });
    expect(result.ok).toBe(false);
  });

  it("accepts a cancel and a revoke", () => {
    expect(
      validateInbound({
        channel: BRIDGE_CHANNEL,
        kind: "cancel",
        payload: { request_id: "req_1" },
      }).ok,
    ).toBe(true);
    expect(
      validateInbound({
        channel: BRIDGE_CHANNEL,
        kind: "revoke",
        payload: { reason: "user_revoked" },
      }).ok,
    ).toBe(true);
  });

  it("accepts the local requests of the user interface", () => {
    for (const kind of [
      "selection_request",
      "status_request",
      "capture_request",
      "grant_request",
      "ui_ready",
    ]) {
      const payload = kind === "grant_request" ? { root_ids: ["1:2"] } : {};
      const result = validateInbound({
        channel: BRIDGE_CHANNEL,
        kind,
        payload,
      });
      expect(result.ok).toBe(true);
    }
  });
});

describe("outbound envelope", () => {
  it("carries the channel, the kind and the payload", () => {
    const envelope = buildEnvelope("result", { data: 1 }, "req_1");
    expect(envelope.channel).toBe(BRIDGE_CHANNEL);
    expect(envelope.kind).toBe("result");
    expect(envelope.type).toBe("result");
    expect(envelope.request_id).toBe("req_1");
    expect(envelope.payload).toEqual({ data: 1 });
  });

  it("names the answers like the message types of the bridge contract", () => {
    for (const kind of ["result", "error", "progress", "chunk", "capabilities", "selection"]) {
      expect(OUTBOUND_KINDS, kind).toContain(kind);
    }
  });

  it("does not offer the names of an earlier build", () => {
    expect(OUTBOUND_KINDS).not.toContain("bridge_result");
    expect(OUTBOUND_KINDS).not.toContain("bridge_error");
  });
});

describe("chunked transfer", () => {
  it("splits a payload and carries the checksum on the last chunk", () => {
    const sent: RelayEnvelope[] = [];
    const raw = new Uint8Array(500000).fill(7);
    sendChunked((envelope) => sent.push(envelope), "capture_chunk", raw, "tr_1");
    expect(sent.length).toBeGreaterThan(1);
    const payloads = sent.map((envelope) => envelope.payload as Record<string, unknown>);
    expect(payloads[0]?.["sequence"]).toBe(0);
    expect(payloads[0]?.["checksum"]).toBeUndefined();
    const last = payloads[payloads.length - 1];
    expect(last?.["checksum"]).toMatch(/^[0-9a-f]{64}$/);
    expect(last?.["total_bytes"]).toBe(raw.length);
    const decoded = base64Decode(String(last?.["bytes"]));
    expect(decoded.length).toBeLessThanOrEqual(raw.length);
  });

  it("measures the payload size the way it travels", () => {
    expect(payloadSize({ a: 1 })).toBe(7);
    expect(payloadSize("abc")).toBe(3);
  });

  it("carries the request id on every chunk of a transfer", () => {
    const sent: RelayEnvelope[] = [];
    const raw = new Uint8Array(500000).fill(7);
    sendChunked((envelope) => sent.push(envelope), "chunk", raw, "tr_1", "req_9");
    expect(sent.length).toBeGreaterThan(1);
    for (const envelope of sent) {
      expect(envelope.request_id).toBe("req_9");
    }
  });
});
