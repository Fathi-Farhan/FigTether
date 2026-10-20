/**
 * The bridge socket client.
 *
 * The UI iframe owns the only network path of the plugin: it dials the local
 * daemon, performs the first frame authentication, keeps the heartbeat alive,
 * reconnects with a bounded backoff and hands every frame that belongs to the
 * editor to the caller. It never opens a second socket and it never keeps a
 * credential in a persistent store.
 */

import { isCapabilityState, parseFrame } from "@figtether/shared";
import type { BridgeFrame, CapabilityMap, JsonValue } from "@figtether/shared";

import { DEFAULT_BACKOFF } from "./backoff.js";
import type { BackoffConfig } from "./backoff.js";
import { byteLength } from "./envelope.js";
import type { LogEntry } from "./log.js";
import {
  AUTH_WINDOW_MS,
  BRIDGE_URL,
  HEARTBEAT_INTERVAL_MS,
  MAX_FRAME_BYTES,
  MISSED_HEARTBEATS_BEFORE_OFFLINE,
  PROTOCOL_VERSION,
  createFrame,
  serializeFrame,
} from "./protocol.js";
import type {
  AuthAcceptedPayload,
  UiError,
  SessionMode,
  UiMessageType,
  WriteGrantMode,
} from "./protocol.js";
import { createSecretStore } from "./secret.js";
import type { SecretStore } from "./secret.js";
import { reduceConnection } from "./status.js";
import type { ConnectionEvent, ConnectionState } from "./status.js";
import type { ApprovalDecision } from "./write.js";

/** Request id that the daemon uses for the handshake frames. */
export const AUTH_REQUEST_ID = "auth";

/** Minimal socket surface used by the bridge, so tests can supply a fake. */
export interface SocketLike {
  send(data: string): void;
  close(code?: number, reason?: string): void;
  onopen: ((event: unknown) => void) | null;
  onmessage: ((event: { data: unknown }) => void) | null;
  onclose: ((event: { code?: number; reason?: string }) => void) | null;
  onerror: ((event: unknown) => void) | null;
}

/** Factory that creates a socket for the bridge endpoint. */
export type SocketFactory = (url: string) => SocketLike;

/** Timer surface used by the bridge, so tests can drive time manually. */
export interface TimerApi {
  setTimeout(handler: () => void, ms: number): number;
  clearTimeout(id: number): void;
  setInterval(handler: () => void, ms: number): number;
  clearInterval(id: number): void;
  now(): number;
}

/** Callbacks the owner of the bridge uses to drive the UI. */
export interface BridgeHandlers {
  onConnection(state: ConnectionState, detail: string | null): void;
  onAuthAccepted(payload: AuthAcceptedPayload): void;
  onAuthRejected(code: string, message: string): void;
  onFrame(frame: BridgeFrame): void;
  onError(error: UiError): void;
  onLog(entry: LogEntry): void;
}

/** Construction options of the bridge client. */
export interface BridgeOptions {
  readonly url?: string;
  readonly socketFactory?: SocketFactory;
  readonly timers?: TimerApi;
  readonly backoff?: BackoffConfig;
  readonly random?: () => number;
  readonly clientId: string;
  readonly handlers: BridgeHandlers;
  /** Factory of the local request ids. Tests can supply a fixed one. */
  readonly requestIdFactory?: () => string;
}

/** Message that tells the user the editor policy blocked the socket. */
export const POLICY_BLOCKED_MESSAGE =
  "The editor blocked the connection to the local daemon. Regenerate the plugin manifest with " +
  "'figtether manifest' so that networkAccess allows ws://127.0.0.1:38947, then reopen the plugin.";

/** Message that tells the user the daemon is not reachable. */
export const DAEMON_UNREACHABLE_MESSAGE =
  "The local daemon did not accept a connection on ws://127.0.0.1:38947. Start it with " +
  "'figtether daemon' and pair again.";

const AUTH_ERROR_CODES = ["AUTH_REQUIRED", "AUTH_EXPIRED", "PROTOCOL_MISMATCH"];

/** Message types that the UI is allowed to send on the socket. */
const ALLOWED_OUTBOUND_TYPES: readonly UiMessageType[] = [
  "hello_auth",
  "grant_update",
  "revoke",
  "heartbeat",
  "result",
  "chunk",
  "progress",
  "error",
  "approval",
];

function defaultSocketFactory(url: string): SocketLike {
  const Ctor = globalThis.WebSocket;
  if (typeof Ctor !== "function") {
    throw new Error("WebSocket is not available in this runtime");
  }
  return new Ctor(url) as unknown as SocketLike;
}

const defaultTimers: TimerApi = {
  setTimeout: (handler, ms) => globalThis.setTimeout(handler, ms) as unknown as number,
  clearTimeout: (id) => globalThis.clearTimeout(id),
  setInterval: (handler, ms) => globalThis.setInterval(handler, ms) as unknown as number,
  clearInterval: (id) => globalThis.clearInterval(id),
  now: () => Date.now(),
};

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function asString(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

function asInteger(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? Math.trunc(value) : null;
}

/** The answer of the daemon to an authentication frame. */
export interface AuthAcceptedFramePayload {
  readonly session_credential: string;
  readonly session_id: string;
  readonly connection_epoch: number;
  readonly grant_epoch: number | null;
  readonly capabilities: CapabilityMap;
}

/**
 * Parse the answer of the daemon to an authentication frame.
 *
 * The daemon answers with a result frame whose payload is the accepted
 * handshake. A future daemon may use a dedicated message type, so both shapes
 * are accepted.
 */
export function parseAuthAccepted(payload: unknown): AuthAcceptedFramePayload | null {
  if (!isPlainObject(payload)) {
    return null;
  }
  const credential = asString(payload["session_credential"]);
  const sessionId = asString(payload["session_id"]);
  const connectionEpoch = asInteger(payload["connection_epoch"]);
  if (credential === null || sessionId === null || connectionEpoch === null) {
    return null;
  }
  const rawCapabilities = payload["capabilities"];
  const capabilities: CapabilityMap = {};
  if (isPlainObject(rawCapabilities)) {
    for (const [name, value] of Object.entries(rawCapabilities)) {
      if (isCapabilityState(value)) {
        capabilities[name] = value;
      }
    }
  }
  return {
    session_credential: credential,
    session_id: sessionId,
    connection_epoch: connectionEpoch,
    grant_epoch: asInteger(payload["grant_epoch"]),
    capabilities,
  };
}

/** Parse a structured error frame payload. */
export function parseBridgeError(payload: unknown, fallbackCode = "INTERNAL_ERROR"): UiError {
  if (!isPlainObject(payload)) {
    return {
      code: fallbackCode,
      message: "The daemon reported an error.",
      retryable: false,
      action: "contact_support",
      request_id: null,
      retry_after_ms: null,
    };
  }
  const retryAfter = payload["retry_after_ms"];
  return {
    code: asString(payload["code"]) ?? fallbackCode,
    message: asString(payload["message"]) ?? "The daemon reported an error.",
    retryable: payload["retryable"] === true,
    action: asString(payload["action"]) ?? "contact_support",
    request_id: asString(payload["request_id"]),
    retry_after_ms:
      typeof retryAfter === "number" && Number.isFinite(retryAfter) ? retryAfter : null,
  };
}

/**
 * The bridge client.
 *
 * One instance per plugin run. It holds the pairing secret and the session
 * credential in memory only and clears both when they are consumed or when the
 * session ends.
 */
export class BridgeClient {
  private readonly url: string;
  private readonly socketFactory: SocketFactory;
  private readonly timers: TimerApi;
  private readonly backoff: BackoffConfig;
  private readonly random: () => number;
  private readonly handlers: BridgeHandlers;
  private readonly clientId: string;
  private readonly requestIdFactory: () => string;
  private readonly pairingSecret: SecretStore = createSecretStore();
  private readonly credential: SecretStore = createSecretStore();

  private socket: SocketLike | null = null;
  private state: ConnectionState = "unpaired";
  private authTimer: number | null = null;
  private heartbeatTimer: number | null = null;
  private reconnectTimer: number | null = null;
  private lastInboundAt = 0;
  private attempt = 0;
  private opened = false;
  private policyBlocked = false;
  private heartbeatSequence = 0;
  private sessionIdValue: string | null = null;
  private connectionEpochValue = 0;
  private grantEpochValue = 0;
  private stopped = false;

  constructor(options: BridgeOptions) {
    this.url = options.url ?? BRIDGE_URL;
    this.socketFactory = options.socketFactory ?? defaultSocketFactory;
    this.timers = options.timers ?? defaultTimers;
    this.backoff = options.backoff ?? DEFAULT_BACKOFF;
    this.random = options.random ?? Math.random;
    this.handlers = options.handlers;
    this.clientId = options.clientId;
    this.requestIdFactory =
      options.requestIdFactory ?? ((): string => `ui_${Date.now().toString(36)}`);
  }

  /** Current connection state. */
  get connectionState(): ConnectionState {
    return this.state;
  }

  /** Session identifier known after a successful authentication. */
  get sessionId(): string | null {
    return this.sessionIdValue;
  }

  /** Connection epoch of the live socket. */
  get connectionEpoch(): number {
    return this.connectionEpochValue;
  }

  /** Grant epoch known from the daemon. */
  get grantEpoch(): number {
    return this.grantEpochValue;
  }

  /** True while a socket is open. */
  get isOpen(): boolean {
    return this.socket !== null && this.opened;
  }

  /**
   * Start pairing. The secret is copied into the credential memory and used
   * once for the authentication frame.
   */
  pair(secret: string): void {
    if (secret.trim().length === 0) {
      this.handlers.onAuthRejected("AUTH_REQUIRED", "Paste the pairing secret shown by the CLI.");
      return;
    }
    this.stopped = false;
    this.policyBlocked = false;
    this.attempt = 0;
    this.pairingSecret.write(secret);
    this.transition({ type: "dial_started" });
    this.dial();
  }

  /** Stop the bridge and release every credential. */
  stop(): void {
    this.stopped = true;
    this.clearTimers();
    this.closeSocket();
    this.pairingSecret.clear();
    this.credential.clear();
    this.sessionIdValue = null;
  }

  /** Drop the local state after a revoke. */
  reset(): void {
    this.clearTimers();
    this.closeSocket();
    this.pairingSecret.clear();
    this.credential.clear();
    this.sessionIdValue = null;
    this.connectionEpochValue = 0;
    this.grantEpochValue = 0;
    this.attempt = 0;
    this.transition({ type: "reset" });
  }

  /**
   * Called by the UI layer when the editor reported a content security policy
   * violation while the socket was dialing.
   */
  reportPolicyViolation(): void {
    this.policyBlocked = true;
    this.handlers.onConnection(this.state, POLICY_BLOCKED_MESSAGE);
  }

  /**
   * Send a grant update to the daemon.
   *
   * The two modes travel on their own fields: the mode of the grant says
   * whether the session may write, and the session mode says whether the
   * session is live or paused.
   */
  sendGrantUpdate(
    rootIds: readonly string[],
    grantEpoch: number,
    sessionMode: SessionMode,
    writeMode: WriteGrantMode = "read",
  ): boolean {
    return this.sendFrame("grant_update", {
      root_ids: [...rootIds],
      grant_epoch: grantEpoch,
      mode: writeMode,
      session_mode: sessionMode,
    });
  }

  /**
   * Send the decision of the user about a write plan.
   *
   * The decision goes straight to the daemon: the sandbox never sees it and
   * never answers it. The payload carries the approval the decision answers,
   * the hash of the plan the user was shown and the decision itself.
   */
  sendApproval(decision: ApprovalDecision): boolean {
    return this.sendFrame("approval", {
      approval_id: decision.approval_id,
      plan_hash: decision.plan_hash,
      approved: decision.approved,
      ...(decision.reason === undefined ? {} : { reason: decision.reason }),
    });
  }

  /**
   * Revoke the session.
   *
   * The revoke frame is the last frame of the session. The credential is
   * dropped, the heartbeat stops and the connection state becomes terminal, so
   * that the close of the socket cannot turn the session back into offline.
   */
  revoke(): boolean {
    const sent = this.sendFrame("revoke", { reason: "user_revoked" });
    this.clearTimers();
    this.credential.clear();
    this.pairingSecret.clear();
    this.transition({ type: "revoked" });
    return sent;
  }

  /**
   * Send an arbitrary frame of the allowlist to the daemon.
   *
   * The message type must be one of the types of the bridge allowlist. An
   * unknown type is refused here so that no free form blob can reach the
   * socket.
   */
  sendFrame(messageType: UiMessageType, payload: JsonValue, requestId?: string): boolean {
    if (!ALLOWED_OUTBOUND_TYPES.includes(messageType)) {
      return false;
    }
    const socket = this.socket;
    if (socket === null || !this.opened) {
      return false;
    }
    const frame = createFrame(messageType, requestId ?? this.requestIdFactory(), payload, {
      ...(this.sessionIdValue === null ? {} : { session_id: this.sessionIdValue }),
      connection_epoch: this.connectionEpochValue,
      grant_epoch: this.grantEpochValue,
    });
    try {
      socket.send(serializeFrame(frame));
      return true;
    } catch {
      return false;
    }
  }

  /** Send one heartbeat frame. */
  sendHeartbeat(): boolean {
    this.heartbeatSequence += 1;
    return this.sendFrame("heartbeat", {
      sequence: this.heartbeatSequence,
      timestamp: new Date(this.timers.now()).toISOString(),
    });
  }

  private transition(event: ConnectionEvent): void {
    const next = reduceConnection(this.state, event);
    if (next !== this.state) {
      this.state = next;
    }
    this.handlers.onConnection(this.state, null);
  }

  private dial(): void {
    if (this.stopped) {
      return;
    }
    this.opened = false;
    let socket: SocketLike;
    try {
      socket = this.socketFactory(this.url);
    } catch {
      this.handlers.onConnection(this.state, DAEMON_UNREACHABLE_MESSAGE);
      this.scheduleReconnect();
      return;
    }
    this.socket = socket;

    socket.onopen = (): void => {
      this.opened = true;
      this.attempt = 0;
      this.lastInboundAt = this.timers.now();
      this.sendAuthentication();
      this.startAuthTimer();
      this.startHeartbeat();
      this.handlers.onLog({ operation: "pair", count: 1 });
    };

    socket.onmessage = (event: { data: unknown }): void => {
      void this.handleMessage(event.data);
    };

    socket.onerror = (): void => {
      this.handlers.onLog({ operation: "pair", errorCode: "PLUGIN_OFFLINE" });
    };

    socket.onclose = (): void => {
      this.clearTimers();
      this.socket = null;
      this.opened = false;
      this.transition({ type: "socket_closed" });
      if (this.state === "revoked" || this.stopped) {
        return;
      }
      if (this.policyBlocked) {
        this.handlers.onConnection(this.state, POLICY_BLOCKED_MESSAGE);
        return;
      }
      this.handlers.onConnection(this.state, DAEMON_UNREACHABLE_MESSAGE);
      this.scheduleReconnect();
    };
  }

  private sendAuthentication(): void {
    const clientId = this.clientId;
    const credential = this.credential.read();
    if (credential !== null) {
      this.sendFrame("hello_auth", {
        secret: credential,
        client_id: clientId,
        protocol_version: PROTOCOL_VERSION,
      });
      this.handlers.onLog({ operation: "reauth", count: 1 });
      return;
    }
    const secret = this.pairingSecret.read();
    if (secret === null) {
      this.handlers.onAuthRejected(
        "AUTH_REQUIRED",
        "The pairing secret is not available. Paste it again.",
      );
      this.closeSocket();
      return;
    }
    this.sendFrame("hello_auth", {
      secret,
      client_id: clientId,
      protocol_version: PROTOCOL_VERSION,
    });
    this.pairingSecret.clear();
  }

  private startAuthTimer(): void {
    this.clearAuthTimer();
    this.authTimer = this.timers.setTimeout(() => {
      this.authTimer = null;
      this.handlers.onAuthRejected(
        "AUTH_REQUIRED",
        "The daemon closed the connection before the handshake completed. Pair again.",
      );
      this.closeSocket();
    }, AUTH_WINDOW_MS);
  }

  private clearAuthTimer(): void {
    if (this.authTimer !== null) {
      this.timers.clearTimeout(this.authTimer);
      this.authTimer = null;
    }
  }

  private startHeartbeat(): void {
    this.clearHeartbeatTimer();
    this.heartbeatTimer = this.timers.setInterval(() => {
      if (
        this.timers.now() - this.lastInboundAt >=
        HEARTBEAT_INTERVAL_MS * MISSED_HEARTBEATS_BEFORE_OFFLINE
      ) {
        this.handlers.onLog({ operation: "heartbeat", errorCode: "PLUGIN_OFFLINE" });
        this.transition({ type: "heartbeat_timeout" });
        this.closeSocket();
        return;
      }
      this.sendHeartbeat();
    }, HEARTBEAT_INTERVAL_MS);
  }

  private clearHeartbeatTimer(): void {
    if (this.heartbeatTimer !== null) {
      this.timers.clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
  }

  private clearTimers(): void {
    this.clearAuthTimer();
    this.clearHeartbeatTimer();
    if (this.reconnectTimer !== null) {
      this.timers.clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }

  private scheduleReconnect(): void {
    if (this.stopped || this.policyBlocked) {
      return;
    }
    if (this.credential.read() === null && this.pairingSecret.read() === null) {
      return;
    }
    if (this.attempt >= this.backoff.maxAttempts) {
      this.handlers.onConnection(
        this.state,
        "The connection to the local daemon did not recover. Pair again to continue.",
      );
      return;
    }
    this.attempt += 1;
    const delay = this.delayFor(this.attempt);
    this.reconnectTimer = this.timers.setTimeout(() => {
      this.reconnectTimer = null;
      this.transition({ type: "dial_started" });
      this.handlers.onLog({ operation: "reconnect", count: this.attempt });
      this.dial();
    }, delay);
  }

  private delayFor(attempt: number): number {
    const base = this.backoff.baseMs * Math.pow(this.backoff.factor, attempt - 1);
    const capped = Math.min(base, this.backoff.maxMs);
    const ratio = Math.min(Math.max(this.backoff.jitterRatio, 0), 1);
    const offset = capped * ratio * (this.random() - 0.5) * 2;
    return Math.max(0, Math.round(capped + offset));
  }

  private closeSocket(): void {
    const socket = this.socket;
    this.socket = null;
    this.opened = false;
    if (socket !== null) {
      try {
        socket.close();
      } catch {
        // A socket that refuses to close is already unusable.
      }
    }
  }

  private async handleMessage(data: unknown): Promise<void> {
    this.lastInboundAt = this.timers.now();
    let text: string | null = null;
    if (typeof data === "string") {
      text = data;
    } else if (data instanceof ArrayBuffer) {
      text = new TextDecoder().decode(new Uint8Array(data));
    } else if (ArrayBuffer.isView(data)) {
      const view = data as ArrayBufferView;
      text = new TextDecoder().decode(
        new Uint8Array(view.buffer, view.byteOffset, view.byteLength),
      );
    } else if (typeof Blob !== "undefined" && data instanceof Blob) {
      text = await data.text();
    }
    if (text === null) {
      return;
    }
    if (byteLength(text) > MAX_FRAME_BYTES) {
      this.handlers.onError({
        code: "PAYLOAD_TOO_LARGE",
        message: "The daemon sent a frame that exceeds the frame limit.",
        retryable: false,
        action: "narrow_parameters",
        request_id: null,
        retry_after_ms: null,
      });
      return;
    }
    let frame: BridgeFrame;
    try {
      frame = parseFrame(text);
    } catch {
      return;
    }
    this.handleFrame(frame);
  }

  private handleFrame(frame: BridgeFrame): void {
    const epoch = frame.connection_epoch;
    if (
      typeof epoch === "number" &&
      this.connectionEpochValue > 0 &&
      epoch < this.connectionEpochValue
    ) {
      // A frame of an older connection is discarded before it is parsed.
      return;
    }
    if (typeof epoch === "number" && epoch > this.connectionEpochValue) {
      this.connectionEpochValue = epoch;
    }
    switch (frame.message_type) {
      case "result": {
        if (frame.request_id === AUTH_REQUEST_ID) {
          this.handleAuthAccepted(frame.payload);
          return;
        }
        this.handlers.onFrame(frame);
        return;
      }
      case "error": {
        if (frame.request_id === AUTH_REQUEST_ID) {
          this.handleAuthRejected(frame.payload);
          return;
        }
        const error = parseBridgeError(frame.payload);
        if (AUTH_ERROR_CODES.includes(error.code)) {
          this.handleAuthRejected(frame.payload);
          return;
        }
        this.handlers.onError(error);
        return;
      }
      case "grant_update": {
        const payload = frame.payload as { grant_epoch?: unknown };
        if (isPlainObject(payload) && typeof payload.grant_epoch === "number") {
          this.grantEpochValue = payload.grant_epoch;
        }
        this.handlers.onFrame(frame);
        return;
      }
      case "revoke": {
        this.handlers.onFrame(frame);
        return;
      }
      case "capabilities":
      case "request":
      case "progress":
      case "chunk":
      case "cancel":
        this.handlers.onFrame(frame);
        return;
      case "heartbeat":
        return;
      default:
        return;
    }
  }

  private handleAuthAccepted(payload: unknown): void {
    this.clearAuthTimer();
    const accepted = parseAuthAccepted(payload);
    if (accepted === null) {
      this.handlers.onAuthRejected(
        "INTERNAL_ERROR",
        "The daemon sent an incomplete handshake answer. Pair again.",
      );
      return;
    }
    this.credential.write(accepted.session_credential);
    this.sessionIdValue = accepted.session_id;
    this.connectionEpochValue = accepted.connection_epoch;
    if (accepted.grant_epoch !== null) {
      this.grantEpochValue = accepted.grant_epoch;
    }
    this.transition({ type: "auth_accepted" });
    this.handlers.onAuthAccepted({
      session_credential: accepted.session_credential,
      session_id: accepted.session_id,
      connection_epoch: accepted.connection_epoch,
      grant_epoch: accepted.grant_epoch ?? 0,
      capabilities: accepted.capabilities,
      session_name: null,
    });
  }

  private handleAuthRejected(payload: unknown): void {
    this.clearAuthTimer();
    this.pairingSecret.clear();
    this.credential.clear();
    this.transition({ type: "auth_rejected" });
    const error = parseBridgeError(payload, "AUTH_REQUIRED");
    this.handlers.onAuthRejected(error.code, error.message);
  }
}
