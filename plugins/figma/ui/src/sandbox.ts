/**
 * The channel between the UI iframe and the main sandbox.
 *
 * The sandbox never opens a socket. It receives the frames that the daemon
 * sent and answers the requests of the editor. Every message on this channel
 * is a strict envelope: the channel marker has to match, the payload has to be
 * a typed object, the type has to be known and the message must stay inside
 * the size cap. A message that belongs to a request carries the request id,
 * so an answer can be correlated with the request that caused it.
 */

import { createEnvelope, parseInboundEnvelope } from "./envelope.js";
import { MAX_ENVELOPE_BYTES, UI_CHANNEL, UI_INBOUND_TYPES } from "./protocol.js";
import type { UiEnvelope } from "./protocol.js";

/** Message event surface used by the channel. */
export interface MessageEventLike {
  readonly data: unknown;
  readonly source: unknown;
  readonly origin: string;
}

/** Window surface used by the channel, so tests can supply a fake. */
export interface WindowLike {
  readonly parent: unknown;
  addEventListener(type: "message", listener: (event: MessageEventLike) => void): void;
  removeEventListener(type: "message", listener: (event: MessageEventLike) => void): void;
  postMessage(message: unknown, targetOrigin: string): void;
}

/** Callbacks of the sandbox channel. */
export interface SandboxHandlers {
  onEnvelope(envelope: UiEnvelope): void;
  onRejected(reason: string): void;
}

/** Options of the sandbox channel. */
export interface SandboxChannelOptions {
  readonly window: WindowLike;
  readonly handlers: SandboxHandlers;
  readonly channel?: string;
  readonly maxBytes?: number;
  readonly allowedTypes?: readonly string[];
}

/** The channel as seen by the controller. */
export interface SandboxChannel {
  /**
   * Send one message to the sandbox.
   *
   * The request id is optional and travels next to the payload, so the
   * sandbox can correlate an answer with the request it belongs to.
   */
  post<TPayload>(type: string, payload: TPayload, requestId?: string): void;
  dispose(): void;
}

/**
 * Create the channel.
 *
 * Messages that do not come from the parent window are ignored. The origin is
 * not part of the check because the editor may expose the plugin frame with a
 * `null` origin; the parent window check plus the channel marker are the
 * trust boundary of this channel.
 */
export function createSandboxChannel(options: SandboxChannelOptions): SandboxChannel {
  const channel = options.channel ?? UI_CHANNEL;
  const maxBytes = options.maxBytes ?? MAX_ENVELOPE_BYTES;
  const allowedTypes = options.allowedTypes ?? UI_INBOUND_TYPES;
  const target = options.window;

  const listener = (event: MessageEventLike): void => {
    if (event.source !== target.parent) {
      return;
    }
    const result = parseInboundEnvelope(event.data, { channel, maxBytes, allowedTypes });
    if (!result.ok) {
      options.handlers.onRejected(result.reason);
      return;
    }
    options.handlers.onEnvelope(result.envelope);
  };

  target.addEventListener("message", listener);

  return {
    post<TPayload>(type: string, payload: TPayload, requestId?: string): void {
      target.postMessage(createEnvelope(type, payload, requestId), "*");
    },
    dispose(): void {
      target.removeEventListener("message", listener);
    },
  };
}
