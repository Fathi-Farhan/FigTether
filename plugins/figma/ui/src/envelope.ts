/**
 * Strict postMessage envelope validation for the channel to the main sandbox.
 *
 * Every message that arrives from the sandbox passes through
 * {@link parseInboundEnvelope}. The function refuses anything that is not a
 * well formed envelope on the expected channel, that exceeds the size cap, or
 * that carries an unknown message type. Nothing is rendered and nothing is
 * forwarded before the envelope passed.
 *
 * The editor wraps the payload of a message inside `pluginMessage`, so the
 * parser unwraps that shape first. A message that arrives raw is accepted as
 * well.
 */

import { unwrapPluginMessage } from "@figtether/shared";

import { MAX_ENVELOPE_BYTES, UI_CHANNEL, UI_INBOUND_TYPES } from "./protocol.js";
import type { UiEnvelope } from "./protocol.js";

/**
 * Unwrap the payload of a message of the editor.
 *
 * The editor delivers the payload of a message of the plugin inside
 * `pluginMessage`. A message that does not follow the convention is delivered
 * as it is, so both shapes reach the validation. The implementation lives in
 * the shared package, so the sandbox and the user interface unwrap a message
 * of the editor in the same way.
 */
export { unwrapPluginMessage };

/** Reasons an inbound envelope can be refused. */
export type EnvelopeRejection =
  | "not_an_object"
  | "json_invalid"
  | "too_large"
  | "channel_mismatch"
  | "type_missing"
  | "type_unknown"
  | "request_id_invalid"
  | "payload_missing"
  | "payload_invalid";

/** Result of an envelope validation. */
export type EnvelopeResult =
  | { readonly ok: true; readonly envelope: UiEnvelope }
  | { readonly ok: false; readonly reason: EnvelopeRejection; readonly detail: string };

/** Options accepted by {@link parseInboundEnvelope}. */
export interface EnvelopeOptions {
  readonly channel?: string;
  readonly maxBytes?: number;
  readonly allowedTypes?: readonly string[];
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** Byte length of a string, used for the size cap. */
export function byteLength(text: string): number {
  if (typeof TextEncoder !== "undefined") {
    return new TextEncoder().encode(text).length;
  }
  let bytes = 0;
  for (let index = 0; index < text.length; index += 1) {
    const code = text.charCodeAt(index);
    if (code <= 0x7f) {
      bytes += 1;
    } else if (code <= 0x7ff) {
      bytes += 2;
    } else if (code >= 0xd800 && code <= 0xdbff) {
      bytes += 4;
      index += 1;
    } else {
      bytes += 3;
    }
  }
  return bytes;
}

function rejection(reason: EnvelopeRejection, detail: string): EnvelopeResult {
  return { ok: false, reason, detail };
}

/**
 * Validate an inbound message.
 *
 * The input may be the raw string of a message event or an already structured
 * value. Strings are parsed with a guarded JSON parse, and the size cap is
 * applied before any payload handling. The payload of the editor is unwrapped
 * first, so a message that arrives inside `pluginMessage` is validated like a
 * message that arrives raw.
 */
export function parseInboundEnvelope(raw: unknown, options: EnvelopeOptions = {}): EnvelopeResult {
  const channel = options.channel ?? UI_CHANNEL;
  const maxBytes = options.maxBytes ?? MAX_ENVELOPE_BYTES;
  const allowedTypes = options.allowedTypes ?? UI_INBOUND_TYPES;

  let candidate: unknown = raw;

  if (typeof raw === "string") {
    if (byteLength(raw) > maxBytes) {
      return rejection("too_large", `envelope exceeds ${maxBytes} bytes`);
    }
    try {
      candidate = JSON.parse(raw) as unknown;
    } catch {
      return rejection("json_invalid", "envelope is not valid JSON");
    }
  }

  candidate = unwrapPluginMessage(candidate);

  if (!isPlainObject(candidate)) {
    return rejection("not_an_object", "envelope is not a JSON object");
  }

  let serialized: string;
  try {
    serialized = JSON.stringify(candidate);
  } catch {
    return rejection("not_an_object", "envelope is not serializable");
  }
  if (byteLength(serialized) > maxBytes) {
    return rejection("too_large", `envelope exceeds ${maxBytes} bytes`);
  }

  if (candidate["channel"] !== channel) {
    return rejection("channel_mismatch", "envelope channel is not the UI channel");
  }

  const type = readType(candidate);
  if (type === null) {
    return rejection("type_missing", "envelope type is missing");
  }
  if (!allowedTypes.includes(type)) {
    return rejection("type_unknown", `envelope type ${type} is not accepted`);
  }

  const requestId = readRequestId(candidate);
  if (requestId === undefined) {
    return rejection("request_id_invalid", "envelope request id is not a string");
  }

  if (!("payload" in candidate)) {
    return rejection("payload_missing", "envelope payload is missing");
  }

  const payload = candidate["payload"];
  if (!isPlainObject(payload)) {
    return rejection("payload_invalid", "envelope payload is not a typed object");
  }

  return {
    ok: true,
    envelope: {
      channel,
      kind: type,
      type,
      payload,
      ...(requestId === null ? {} : { request_id: requestId }),
    },
  };
}

/**
 * Read the discriminator of an envelope.
 *
 * The discriminator is read from `type` and, for tolerance with the other
 * side of the channel, from `kind`.
 */
function readType(candidate: Record<string, unknown>): string | null {
  const type = candidate["type"];
  if (typeof type === "string" && type.length > 0) {
    return type;
  }
  const kind = candidate["kind"];
  if (typeof kind === "string" && kind.length > 0) {
    return kind;
  }
  return null;
}

/**
 * Read the correlation id of an envelope.
 *
 * The function returns the id, null when the envelope carries none and
 * undefined when the envelope carries something that is not a string.
 */
function readRequestId(candidate: Record<string, unknown>): string | null | undefined {
  const requestId = candidate["request_id"];
  if (requestId === undefined || requestId === null) {
    return null;
  }
  if (typeof requestId !== "string") {
    return undefined;
  }
  return requestId.length === 0 ? null : requestId;
}

/**
 * Build an outbound envelope for the sandbox channel.
 *
 * The request id is optional and travels next to the payload, so the sandbox
 * can correlate an answer with the request it belongs to.
 */
export function createEnvelope<TPayload>(
  type: string,
  payload: TPayload,
  requestId?: string,
): UiEnvelope<TPayload> {
  return {
    channel: UI_CHANNEL,
    kind: type,
    type,
    payload,
    ...(requestId === undefined ? {} : { request_id: requestId }),
  };
}
