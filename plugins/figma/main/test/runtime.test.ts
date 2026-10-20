import { describe, expect, it } from "vitest";

import { BRIDGE_CHANNEL, type RelayEnvelope } from "../src/bridge.js";
import { PluginRuntime, type SandboxTransport } from "../src/runtime.js";
import { createFakeDocument } from "./fake-figma.js";

interface Harness {
  runtime: PluginRuntime;
  sent: RelayEnvelope[];
  deliver: (raw: unknown) => Promise<void>;
}

async function createHarness(): Promise<Harness> {
  const document = createFakeDocument([
    {
      id: "1:1",
      name: "Root",
      children: [{ id: "1:2", name: "Child" }],
    },
    { id: "2:1", name: "Other" },
  ]);
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
    request_id: "req_1",
    session_id: "session_1",
    connection_epoch: 1,
    grant_epoch: 1,
    deadline_ms: 20000,
    payload: { op, params },
  };
}

function grant(rootIds: string[]): Record<string, unknown> {
  return {
    protocol_version: "1.0",
    message_type: "grant_update",
    request_id: "grant_1",
    payload: { root_ids: rootIds, grant_epoch: 1 },
  };
}

describe("runtime dispatch", () => {
  it("answers a request of the daemon with a result", async () => {
    const harness = await createHarness();
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "bridge_frame",
      payload: grant(["1:1"]),
    });
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "bridge_frame",
      payload: request("list_nodes", { root_id: "1:1" }),
    });
    const result = harness.sent.find((envelope) => envelope.kind === "result");
    expect(result).toBeDefined();
    const payload = result?.payload as { ok: boolean; data: { nodes: unknown[] } };
    expect(payload.ok).toBe(true);
    expect(payload.data.nodes.length).toBe(2);
  });

  it("answers with the names of the bridge contract and with the request id", async () => {
    const harness = await createHarness();
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "bridge_frame",
      payload: grant(["1:1"]),
    });
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "bridge_frame",
      payload: request("list_nodes", { root_id: "1:1" }),
    });
    const answers = harness.sent.filter(
      (envelope) => envelope.kind === "result" || envelope.kind === "error",
    );
    expect(answers).toHaveLength(1);
    expect(answers[0]?.kind).toBe("result");
    expect(answers[0]?.type).toBe("result");
    expect(answers[0]?.request_id).toBe("req_1");
  });

  it("never sends the names of an earlier build", async () => {
    const harness = await createHarness();
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "bridge_frame",
      payload: grant(["1:1"]),
    });
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "bridge_frame",
      payload: request("list_nodes", { root_id: "1:1" }),
    });
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "bridge_frame",
      payload: request("watch_scope", { root_id: "1:1" }),
    });
    const names = harness.sent.map((envelope) => envelope.kind);
    expect(names).not.toContain("bridge_result");
    expect(names).not.toContain("bridge_error");
    expect(names.filter((name) => name === "result")).toHaveLength(1);
    expect(names.filter((name) => name === "error")).toHaveLength(1);
  });

  it("refuses a read outside the granted scope", async () => {
    const harness = await createHarness();
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "bridge_frame",
      payload: grant(["1:1"]),
    });
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "bridge_frame",
      payload: request("list_nodes", { root_id: "2:1" }),
    });
    const error = harness.sent.find((envelope) => envelope.kind === "error");
    const payload = error?.payload as { code: string; request_id: string };
    expect(payload.code).toBe("OUT_OF_SCOPE");
    expect(payload.request_id).toBe("req_1");
  });

  it("refuses a read without a grant", async () => {
    const harness = await createHarness();
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "bridge_frame",
      payload: request("list_nodes", { root_id: "1:1" }),
    });
    const error = harness.sent.find((envelope) => envelope.kind === "error");
    const payload = error?.payload as { code: string };
    expect(payload.code).toBe("OUT_OF_SCOPE");
  });

  it("refuses an operation that the build does not implement", async () => {
    const harness = await createHarness();
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "bridge_frame",
      payload: request("watch_scope", { root_id: "1:1" }),
    });
    const error = harness.sent.find((envelope) => envelope.kind === "error");
    const payload = error?.payload as { code: string };
    expect(payload.code).toBe("CAPABILITY_UNAVAILABLE");
  });

  it("answers the selection request of the user interface", async () => {
    const harness = await createHarness();
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "selection_request",
      payload: {},
    });
    const selection = harness.sent.find((envelope) => envelope.kind === "selection");
    expect(selection).toBeDefined();
    const payload = selection?.payload as { roots: unknown[] };
    expect(Array.isArray(payload.roots)).toBe(true);
  });

  it("verifies the roots of a grant request", async () => {
    const harness = await createHarness();
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "grant_request",
      payload: { root_ids: ["1:1"] },
    });
    const verified = harness.sent.find((envelope) => envelope.kind === "grant_verified");
    const payload = verified?.payload as { ok: boolean; root_ids: string[] };
    expect(payload.ok).toBe(true);
    expect(payload.root_ids).toEqual(["1:1"]);
  });

  it("refuses a grant request with a root that does not exist", async () => {
    const harness = await createHarness();
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "grant_request",
      payload: { root_ids: ["9:9"] },
    });
    const verified = harness.sent.find((envelope) => envelope.kind === "grant_verified");
    const payload = verified?.payload as { ok: boolean };
    expect(payload.ok).toBe(false);
  });
});

describe("runtime counters", () => {
  it("counts a message of another channel as rejected", async () => {
    const harness = await createHarness();
    await harness.deliver({
      channel: "some.other.channel",
      kind: "capabilities",
      payload: {},
    });
    expect(harness.runtime.counters.received).toBe(1);
    expect(harness.runtime.counters.rejected).toBe(1);
    expect(harness.runtime.counters.rejectedUnknownChannel).toBe(1);
    expect(harness.sent).toHaveLength(0);
  });

  it("counts an unknown message kind as rejected", async () => {
    const harness = await createHarness();
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "not_a_kind",
      payload: {},
    });
    expect(harness.runtime.counters.rejectedUnknownKind).toBe(1);
    expect(harness.sent).toHaveLength(0);
  });

  it("counts an oversized payload as rejected", async () => {
    const harness = await createHarness();
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "capabilities",
      payload: { blob: "x".repeat(1024 * 1024 + 64) },
    });
    expect(harness.runtime.counters.rejectedOversized).toBe(1);
    expect(harness.sent).toHaveLength(0);
  });

  it("counts a malformed payload as rejected", async () => {
    const harness = await createHarness();
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "capabilities",
      payload: "not an object",
    });
    expect(harness.runtime.counters.rejectedMalformed).toBe(1);
    expect(harness.sent).toHaveLength(0);
  });

  it("counts an accepted message", async () => {
    const harness = await createHarness();
    await harness.deliver({
      channel: BRIDGE_CHANNEL,
      kind: "capabilities",
      payload: {},
    });
    expect(harness.runtime.counters.accepted).toBe(1);
    expect(harness.runtime.counters.rejected).toBe(0);
    expect(harness.sent.some((envelope) => envelope.kind === "capabilities")).toBe(true);
  });
});
