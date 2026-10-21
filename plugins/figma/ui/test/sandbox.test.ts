import { expect, it, vi } from "vitest";
import { BRIDGE_CHANNEL } from "@figtether/shared";
import { createSandboxChannel } from "../src/sandbox.js";
import type { MessageEventLike, WindowLike } from "../src/sandbox.js";

it("sends requests to the Figma parent with the pluginMessage wrapper", () => {
  const parentPost = vi.fn();
  const selfPost = vi.fn();
  const channel = createSandboxChannel({
    window: {
      parent: { postMessage: parentPost },
      postMessage: selfPost,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    },
    handlers: { onEnvelope: vi.fn(), onRejected: vi.fn() },
  });
  channel.post("selection_request", {}, "selection-1");
  expect(selfPost).not.toHaveBeenCalled();
  expect(parentPost).toHaveBeenCalledWith(
    {
      pluginMessage: expect.objectContaining({
        kind: "selection_request",
        request_id: "selection-1",
        payload: {},
      }),
    },
    "*",
  );
});

it("accepts the top-level Figma relay and rejects unrelated windows and origins", () => {
  let receive: ((event: MessageEventLike) => void) | undefined;
  const onEnvelope = vi.fn();
  const window: WindowLike = {
    parent: { postMessage: vi.fn() },
    top: {},
    postMessage: vi.fn(),
    addEventListener: (_type, listener) => {
      receive = listener;
    },
    removeEventListener: vi.fn(),
  };
  createSandboxChannel({ window, handlers: { onEnvelope, onRejected: vi.fn() } });
  const data = {
    pluginMessage: { channel: BRIDGE_CHANNEL, kind: "selection", payload: { roots: [] } },
  };
  receive?.({ source: {}, origin: "null", data });
  expect(onEnvelope).not.toHaveBeenCalled();
  receive?.({ source: window.top, origin: "https://example.com", data });
  expect(onEnvelope).not.toHaveBeenCalled();
  receive?.({ source: window.top, origin: "https://www.figma.com", data });
  expect(onEnvelope).toHaveBeenCalledOnce();
});
