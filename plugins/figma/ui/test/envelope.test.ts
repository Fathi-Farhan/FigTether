import { describe, expect, it } from "vitest";

import { byteLength, createEnvelope, parseInboundEnvelope } from "../src/envelope.js";
import { UI_CHANNEL } from "../src/protocol.js";
function envelopeOf(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    channel: UI_CHANNEL,
    type: "selection",
    payload: { roots: [] },
    ...overrides,
  };
}

describe("parseInboundEnvelope", () => {
  it("accepts a well formed envelope", () => {
    const result = parseInboundEnvelope(envelopeOf());
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.envelope.type).toBe("selection");
      expect(result.envelope.channel).toBe(UI_CHANNEL);
    }
  });

  it("accepts a JSON string", () => {
    const result = parseInboundEnvelope(JSON.stringify(envelopeOf()));
    expect(result.ok).toBe(true);
  });

  it("refuses an envelope of another channel", () => {
    const result = parseInboundEnvelope(envelopeOf({ channel: "other.channel" }));
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.reason).toBe("channel_mismatch");
    }
  });

  it("refuses invalid JSON", () => {
    const result = parseInboundEnvelope("{not json");
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.reason).toBe("json_invalid");
    }
  });

  it("refuses a payload that is not a typed object", () => {
    const result = parseInboundEnvelope(envelopeOf({ payload: [1, 2, 3] }));
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.reason).toBe("payload_invalid");
    }
  });

  it("refuses an envelope that is not an object at all", () => {
    expect(parseInboundEnvelope("null").ok).toBe(false);
    expect(parseInboundEnvelope(42).ok).toBe(false);
    expect(parseInboundEnvelope(undefined).ok).toBe(false);
  });

  it("refuses an envelope above the size cap", () => {
    const huge = envelopeOf({ payload: { roots: ["x".repeat(4096)] } });
    const result = parseInboundEnvelope(huge, { maxBytes: 256 });
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.reason).toBe("too_large");
    }
  });

  it("refuses an unknown message type", () => {
    const result = parseInboundEnvelope(envelopeOf({ type: "surprise" }));
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.reason).toBe("type_unknown");
    }
  });

  it("refuses a missing type", () => {
    const withoutType = envelopeOf();
    delete withoutType["type"];
    const result = parseInboundEnvelope(withoutType);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.reason).toBe("type_missing");
    }
  });

  it("refuses a missing payload", () => {
    const withoutPayload = envelopeOf();
    delete withoutPayload["payload"];
    const result = parseInboundEnvelope(withoutPayload);
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.reason).toBe("payload_missing");
    }
  });

  it("accepts every message type of the sandbox contract", () => {
    for (const type of ["result", "error", "progress", "chunk", "capabilities", "selection"]) {
      expect(parseInboundEnvelope(envelopeOf({ type })).ok, type).toBe(true);
    }
  });

  it("accepts an envelope that the editor wrapped in pluginMessage", () => {
    const result = parseInboundEnvelope({ pluginMessage: envelopeOf() });
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.envelope.type).toBe("selection");
      expect(result.envelope.channel).toBe(UI_CHANNEL);
    }
  });

  it("accepts a wrapped envelope that arrives as a JSON string", () => {
    const result = parseInboundEnvelope(JSON.stringify({ pluginMessage: envelopeOf() }));
    expect(result.ok).toBe(true);
  });

  it("refuses a wrapped envelope of another channel", () => {
    const result = parseInboundEnvelope({
      pluginMessage: envelopeOf({ channel: "other.channel" }),
    });
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.reason).toBe("channel_mismatch");
    }
  });

  it("keeps the request id of the envelope", () => {
    const result = parseInboundEnvelope(envelopeOf({ type: "result", request_id: "req-42" }));
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.envelope.request_id).toBe("req-42");
    }
  });

  it("keeps the request id of a wrapped envelope", () => {
    const result = parseInboundEnvelope({
      pluginMessage: envelopeOf({ type: "result", request_id: "req-42" }),
    });
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.envelope.request_id).toBe("req-42");
    }
  });

  it("reports no request id when the envelope carries none", () => {
    const result = parseInboundEnvelope(envelopeOf());
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.envelope.request_id).toBeUndefined();
    }
  });

  it("refuses a request id that is not a string", () => {
    const result = parseInboundEnvelope(envelopeOf({ request_id: 42 }));
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.reason).toBe("request_id_invalid");
    }
  });

  it("accepts the kind alias of the discriminator", () => {
    const envelope = envelopeOf();
    delete envelope["type"];
    envelope["kind"] = "result";
    const result = parseInboundEnvelope(envelope);
    expect(result.ok).toBe(true);
    if (result.ok) {
      expect(result.envelope.type).toBe("result");
    }
  });
});

describe("createEnvelope", () => {
  it("marks the envelope with the bridge channel", () => {
    const envelope = createEnvelope("selection_request", {});
    expect(envelope.channel).toBe(UI_CHANNEL);
    expect(envelope.type).toBe("selection_request");
  });

  it("carries the request id when there is one", () => {
    const envelope = createEnvelope("frame", { op: "list_nodes" }, "req-42");
    expect(envelope.request_id).toBe("req-42");
  });

  it("carries no request id when there is none", () => {
    const envelope = createEnvelope("selection_request", {});
    expect(envelope.request_id).toBeUndefined();
  });

  it("round trips through the validation", () => {
    const envelope = createEnvelope("selection", { roots: [] });
    const result = parseInboundEnvelope(JSON.stringify(envelope));
    expect(result.ok).toBe(true);
  });
});

describe("byteLength", () => {
  it("counts the bytes of a multi byte text", () => {
    expect(byteLength("abc")).toBe(3);
    expect(byteLength("äöü")).toBe(6);
    expect(byteLength("🙂")).toBe(4);
  });
});
