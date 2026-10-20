/**
 * Transport between the main sandbox and the UI iframe.
 *
 * Figma delivers messages of the UI iframe through `figma.ui.onmessage` and
 * sends messages to the UI through `figma.ui.postMessage`. The iframe may
 * also use `window.parent.postMessage`, so the window channel is accepted as
 * well. Both paths deliver the same envelope and both are validated by the
 * bridge before a handler sees them.
 */

import { unwrapPluginMessage } from "@figtether/shared";
import type { BridgeEnvelope } from "@figtether/shared";

import type { FigmaApi } from "./figma-api.js";
import type { SandboxTransport } from "./runtime.js";

/**
 * Unwrap the payload of a message posted by the UI.
 *
 * The Figma convention wraps the payload in `pluginMessage`. A message that
 * does not follow the convention is delivered as it is. The implementation
 * lives in the shared package, so the sandbox and the user interface unwrap
 * a message of the editor in the same way.
 */
export { unwrapPluginMessage };

/**
 * Build the transport of the sandbox.
 *
 * The returned transport listens on the Figma channel and on the window
 * channel, and posts through the Figma channel.
 */
export function createSandboxTransport(
  api: FigmaApi,
  scope: unknown = globalThis,
): SandboxTransport {
  const handlers: Array<(raw: unknown) => void> = [];
  const deliver = (raw: unknown): void => {
    const payload = unwrapPluginMessage(raw);
    for (const handler of handlers) {
      handler(payload);
    }
  };

  const ui = api.ui;
  if (ui !== undefined) {
    // The handler is registered by assigning it. The property is empty until
    // the first assignment, so the assignment is never conditional.
    ui.onmessage = (message: unknown): void => {
      deliver(message);
    };
  }

  const windowScope = asWindow(scope);
  if (windowScope !== null) {
    const listener = (event: MessageEventLike): void => {
      const source = event.source;
      if (source !== undefined && source !== null && source !== windowScope) {
        return;
      }
      deliver(event.data);
    };
    windowScope.addEventListener("message", listener);
  }

  return {
    onMessage(handler: (raw: unknown) => void): void {
      handlers.push(handler);
    },
    post(envelope: BridgeEnvelope): void {
      if (ui !== undefined && typeof ui.postMessage === "function") {
        ui.postMessage(envelope);
      }
    },
  };
}

/** The message event of the window channel, without the DOM library. */
interface MessageEventLike {
  /** The payload of the event. */
  data: unknown;
  /** The window that sent the message, when the runtime reports it. */
  source?: unknown;
}

interface WindowLike {
  addEventListener: (type: string, listener: (event: MessageEventLike) => void) => void;
}

function asWindow(scope: unknown): WindowLike | null {
  if (typeof scope !== "object" || scope === null) {
    return null;
  }
  const candidate = scope as { addEventListener?: unknown };
  if (typeof candidate.addEventListener !== "function") {
    return null;
  }
  return candidate as unknown as WindowLike;
}
