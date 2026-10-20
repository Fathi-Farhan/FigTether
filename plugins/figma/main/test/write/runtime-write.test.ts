import { describe, expect, it } from "vitest";

import { BRIDGE_CHANNEL, type RelayEnvelope } from "../../src/bridge.js";
import { PluginRuntime, type SandboxTransport } from "../../src/runtime.js";
import { planPayload, sampleDocument } from "./write-fixture.js";

interface Harness {
  runtime: PluginRuntime;
  sent: RelayEnvelope[];
  deliver: (raw: unknown) => Promise<void>;
}

async function createHarness(): Promise<Harness> {
  const document = sampleDocument();
  const sent: RelayEnvelope[] = [];
  let handler: (raw: unknown) => void = () => undefined;
  const transport: SandboxTransport = {
    onMessage: (listener) => {
      handler = listener;
    },
    post: (envelope) => {
      sent.push(envelope);
    },
  };
  const runtime = new PluginRuntime({ api: document.api, transport });
  await runtime.start();
  return {
    runtime,
    sent,
    deliver: async (raw) => {
      handler(raw);
      // The runtime answers asynchronously.
      await new Promise((resolve) => setTimeout(resolve, 0));
    },
  };
}

function request(op: string, params: unknown): Record<string, unknown> {
  return {
    protocol_version: "1.0",
    message_type: "request",
    request_id: "req_write",
    session_id: "session_1",
    connection_epoch: 1,
    grant_epoch: 1,
    deadline_ms: 20000,
    payload: { op, params },
  };
}

function grant(
  rootIds: string[],
  modes: { mode?: string; sessionMode?: string } = {},
): Record<string, unknown> {
  return {
    protocol_version: "1.0",
    message_type: "grant_update",
    request_id: "grant_1",
    payload: {
      root_ids: rootIds,
      grant_epoch: 1,
      ...(modes.mode === undefined ? {} : { mode: modes.mode }),
      ...(modes.sessionMode === undefined ? {} : { session_mode: modes.sessionMode }),
    },
  };
}

describe("the write path in the runtime", () => {
  it("answers a preview request with the request id and the envelope of the bridge", async () => {
    const harness = await createHarness();
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "bridge_frame",
      payload: grant(["1:1"], { mode: "write" }),
    });
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "bridge_frame",
      payload: request("preview_write", planPayload([
        { kind: "create_node", parent_id: "1:1", node_type: "FRAME" },
      ])),
    });
    const answer = harness.sent.find((envelope) => envelope.kind === "result");
    expect(answer).toBeDefined();
    expect(answer?.kind).toBe("result");
    expect(answer?.type).toBe("result");
    expect(answer?.channel).toBe(BRIDGE_CHANNEL);
    expect(answer?.request_id).toBe("req_write");
    const payload = answer?.payload as { request_id: string; ok: boolean; data: { blocked: boolean } };
    expect(payload.request_id).toBe("req_write");
    expect(payload.ok).toBe(true);
    expect(payload.data.blocked).toBe(false);
  });

  it("answers an apply request with the ids of the created nodes", async () => {
    const harness = await createHarness();
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "bridge_frame",
      payload: grant(["1:1"], { mode: "write" }),
    });
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "bridge_frame",
      payload: request("apply_write", planPayload([
        { kind: "create_node", parent_id: "1:1", node_type: "FRAME", properties: { name: "New" } },
      ])),
    });
    const answer = harness.sent.find((envelope) => envelope.kind === "result");
    expect(answer).toBeDefined();
    const payload = answer?.payload as {
      request_id: string;
      ok: boolean;
      data: { created: string[]; undo_committed: boolean; status: string };
    };
    expect(payload.request_id).toBe("req_write");
    expect(payload.ok).toBe(true);
    expect(payload.data.status).toBe("applied");
    expect(payload.data.created).toHaveLength(1);
    expect(payload.data.undo_committed).toBe(true);
  });

  it("answers a refused plan with an error frame", async () => {
    const harness = await createHarness();
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "bridge_frame",
      payload: grant(["1:1"], { mode: "write" }),
    });
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "bridge_frame",
      payload: request("apply_write", planPayload([
        { kind: "set_properties", node_id: "2:2", properties: { x: 1 } },
      ])),
    });
    const answer = harness.sent.find((envelope) => envelope.kind === "error");
    expect(answer).toBeDefined();
    expect(answer?.request_id).toBe("req_write");
    const payload = answer?.payload as { code: string; request_id: string };
    expect(payload.code).toBe("OUT_OF_SCOPE");
    expect(payload.request_id).toBe("req_write");
  });

  it("refuses a write while the grant is read only", async () => {
    const harness = await createHarness();
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "bridge_frame",
      payload: grant(["1:1"], { mode: "read" }),
    });
    expect(harness.runtime.mode).toBe("read");
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "bridge_frame",
      payload: request("preview_write", planPayload([
        { kind: "create_node", parent_id: "1:1", node_type: "FRAME" },
      ])),
    });
    const answer = harness.sent.find((envelope) => envelope.kind === "error");
    expect(answer).toBeDefined();
    const payload = answer?.payload as { code: string; request_id: string };
    expect(payload.code).toBe("WRITE_NOT_GRANTED");
    expect(payload.request_id).toBe("req_write");
    expect(harness.sent.some((envelope) => envelope.kind === "result")).toBe(false);
  });

  it("refuses a write while the grant names no mode at all", async () => {
    const harness = await createHarness();
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "bridge_frame",
      payload: grant(["1:1"]),
    });
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "bridge_frame",
      payload: request("apply_write", planPayload([
        { kind: "create_node", parent_id: "1:1", node_type: "FRAME" },
      ])),
    });
    const answer = harness.sent.find((envelope) => envelope.kind === "error");
    const payload = answer?.payload as { code: string };
    expect(payload.code).toBe("WRITE_NOT_GRANTED");
  });

  it("never mistakes the session mode for the mode of the grant", async () => {
    const harness = await createHarness();
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "bridge_frame",
      payload: grant(["1:1"], { mode: "write", sessionMode: "paused" }),
    });
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "bridge_frame",
      payload: request("preview_write", planPayload([
        { kind: "create_node", parent_id: "1:1", node_type: "FRAME" },
      ])),
    });
    const answer = harness.sent.find((envelope) => envelope.kind === "error");
    const payload = answer?.payload as { code: string };
    expect(payload.code).toBe("PAUSED");
  });

  it("accepts a grant that names the live session mode", async () => {
    const harness = await createHarness();
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "bridge_frame",
      payload: grant(["1:1"], { mode: "write", sessionMode: "live" }),
    });
    expect(harness.runtime.mode).toBe("write");
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "bridge_frame",
      payload: request("preview_write", planPayload([
        { kind: "create_node", parent_id: "1:1", node_type: "FRAME" },
      ])),
    });
    const answer = harness.sent.find((envelope) => envelope.kind === "result");
    expect(answer).toBeDefined();
  });
});
