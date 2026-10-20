/**
 * The end to end path of a read.
 *
 * The test wires the three parts of the plugin together: the daemon sends a
 * request frame over the socket, the user interface forwards it to the main
 * sandbox, the sandbox executes the read and answers, and the user interface
 * forwards the answer back to the daemon. The message of the editor arrives in
 * the shape the editor uses, wrapped inside `pluginMessage`.
 *
 * The doubles are the ones of the other tests, so the test exercises the real
 * runtime of the sandbox and the real controller of the interface.
 */

import { isJsonObject } from "@figtether/shared";
import { describe, expect, it } from "vitest";

import { BRIDGE_CHANNEL } from "../../main/src/bridge.js";
import type { RelayEnvelope } from "../../main/src/bridge.js";
import { PluginRuntime } from "../../main/src/runtime.js";
import type { SandboxTransport } from "../../main/src/runtime.js";
import { createFakeDocument } from "../../main/test/fake-figma.js";
import { parseInboundEnvelope } from "../src/envelope.js";
import type { BridgeFrame } from "../src/protocol.js";
import { acceptHandshake, bridgeFrame, harness, sandboxEnvelope } from "./fakes.js";
import type { FakeBridge, FakeSandbox } from "./fakes.js";

/** The parts of the chain the test drives. */
interface Chain {
  /** The socket to the daemon. */
  readonly bridge: FakeBridge;
  /** The channel to the sandbox. */
  readonly sandbox: FakeSandbox;
  /** The answers the sandbox sent to the interface. */
  readonly answers: RelayEnvelope[];
  /** Deliver a frame of the daemon to the interface. */
  daemon(frame: BridgeFrame): void;
  /** Let the sandbox finish its work. */
  settle(): Promise<void>;
}

/**
 * Build the chain and grant the scope of the read.
 *
 * The grant travels the same way it travels in the editor: the interface
 * posts it to the sandbox, and the sandbox applies it before the first read.
 */
async function createChain(): Promise<Chain> {
  const document = createFakeDocument([
    {
      id: "1:1",
      name: "Root",
      children: [{ id: "1:2", name: "Child" }],
    },
    { id: "2:1", name: "Other" },
  ]);
  const answers: RelayEnvelope[] = [];
  const { controller, bridge, sandbox } = harness();

  const transport: SandboxTransport = {
    onMessage: (): void => undefined,
    post: (envelope): void => {
      answers.push(envelope);
      // The editor delivers the payload of a message inside pluginMessage.
      const parsed = parseInboundEnvelope({ pluginMessage: envelope });
      if (!parsed.ok) {
        throw new Error(`the editor sent an invalid envelope: ${parsed.reason}`);
      }
      controller.onSandboxEnvelope(parsed.envelope);
    },
  };

  const runtime = new PluginRuntime({ api: document.api, transport });
  await runtime.start();

  sandbox.onPost = (post): void => {
    void runtime.handleRaw({
      channel: BRIDGE_CHANNEL,
      kind: post.type,
      type: post.type,
      request_id: post.requestId,
      payload: post.payload,
    });
  };

  controller.onPairRequested("pairing-secret-value");
  acceptHandshake(bridge);
  controller.onSandboxEnvelope(
    sandboxEnvelope("selection", {
      roots: [{ node_id: "1:1", node_type: "FRAME", child_count: 1 }],
    }),
  );
  controller.onRootSelectionChanged(["1:1"]);
  controller.onAllowRequested();
  await settle();

  return {
    bridge,
    sandbox,
    answers,
    daemon: (frame: BridgeFrame): void => {
      bridge.handlers.onFrame(frame);
    },
    settle,
  };
}

/** Let the pending work of the sandbox finish. */
async function settle(): Promise<void> {
  await new Promise((resolve) => setTimeout(resolve, 0));
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("the message path from the daemon to the editor and back", () => {
  it("delivers the result of the sandbox to the daemon", async () => {
    const chain = await createChain();
    const answer = chain.bridge.awaitAnswer("req-1");

    chain.daemon(bridgeFrame("request", { op: "list_nodes", params: { root_id: "1:1" } }, "req-1"));
    await chain.settle();

    const result = chain.bridge.wire.find((frame) => frame.message_type === "result");
    expect(result).toBeDefined();
    expect(result?.request_id).toBe("req-1");
    expect(isJsonObject(result?.payload)).toBe(true);
    const data = await answer;
    expect(isJsonObject(data)).toBe(true);
    const nodes = (data as { nodes?: unknown[] }).nodes;
    expect(Array.isArray(nodes)).toBe(true);
    expect(nodes?.length).toBe(2);
  });

  it("keeps the request id on every hop of the path", async () => {
    const chain = await createChain();

    chain.daemon(bridgeFrame("request", { op: "list_nodes", params: { root_id: "1:1" } }, "req-7"));
    await chain.settle();

    const relayed = chain.sandbox.posted.find((post) => post.type === "frame");
    expect(relayed?.requestId).toBe("req-7");
    expect((relayed?.payload as BridgeFrame).request_id).toBe("req-7");

    const answer = chain.answers.find((envelope) => envelope.kind === "result");
    expect(answer?.request_id).toBe("req-7");

    // The interface keeps the request id of the answer it parsed.
    const parsed = parseInboundEnvelope({ pluginMessage: answer });
    expect(parsed.ok).toBe(true);
    if (parsed.ok) {
      expect(parsed.envelope.request_id).toBe("req-7");
    }

    const result = chain.bridge.wire.find((frame) => frame.message_type === "result");
    expect(result?.request_id).toBe("req-7");
  });

  it("delivers the error of the sandbox to the daemon", async () => {
    const chain = await createChain();

    chain.daemon(bridgeFrame("request", { op: "list_nodes", params: { root_id: "2:1" } }, "req-9"));
    await chain.settle();

    const error = chain.bridge.wire.find((frame) => frame.message_type === "error");
    expect(error).toBeDefined();
    expect(error?.request_id).toBe("req-9");
    const payload = error?.payload as { code?: unknown; request_id?: unknown };
    expect(payload.code).toBe("OUT_OF_SCOPE");
    expect(payload.request_id).toBe("req-9");
  });

  it("accepts an answer that the editor wrapped in pluginMessage", async () => {
    const chain = await createChain();

    chain.daemon(bridgeFrame("request", { op: "list_nodes", params: { root_id: "1:1" } }, "req-3"));
    await chain.settle();

    // The sandbox sent the answer inside the wrapper of the editor.
    const answer = chain.answers.find((envelope) => envelope.kind === "result");
    expect(answer).toBeDefined();
    const parsed = parseInboundEnvelope({ pluginMessage: answer });
    expect(parsed.ok).toBe(true);
    if (parsed.ok) {
      expect(parsed.envelope.type).toBe("result");
      expect(parsed.envelope.request_id).toBe("req-3");
    }
    // The wrapper did not stop the answer on its way to the daemon.
    const result = chain.bridge.wire.find((frame) => frame.message_type === "result");
    expect(result?.request_id).toBe("req-3");
  });

  it("refuses a read outside the grant without an answer of the daemon", async () => {
    const chain = await createChain();

    chain.daemon(bridgeFrame("request", { op: "list_nodes", params: { root_id: "2:1" } }, "req-8"));
    await chain.settle();

    const result = chain.bridge.wire.find((frame) => frame.message_type === "result");
    expect(result).toBeUndefined();
  });

  it("forwards a cancel of the daemon to the sandbox", async () => {
    const chain = await createChain();

    chain.daemon(bridgeFrame("cancel", {}, "req-5"));
    await chain.settle();

    const cancel = chain.sandbox.posted.find((post) => post.type === "cancel");
    expect(cancel).toBeDefined();
    expect(cancel?.requestId).toBe("req-5");
    expect((cancel?.payload as { request_id?: unknown }).request_id).toBe("req-5");
  });

  it("answers a read of the daemon with the data of the editor", async () => {
    const chain = await createChain();
    const answer = chain.bridge.awaitAnswer("req-2");

    chain.daemon(
      bridgeFrame("request", { op: "get_selection", params: {} }, "req-2"),
    );
    await chain.settle();

    const data = (await answer) as { selection_count?: unknown };
    expect(typeof data.selection_count).toBe("number");
  });
});
