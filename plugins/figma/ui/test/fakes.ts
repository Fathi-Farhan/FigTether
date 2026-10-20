/**
 * Test doubles of the user interface.
 *
 * The doubles replace the three ports of the controller: the view of the
 * interface, the channel to the main sandbox and the socket to the local
 * daemon. They record what the controller does, so a test can assert the
 * traffic of the message path without a browser and without a socket.
 */

import { createFrame, isJsonObject } from "@figtether/shared";
import type { BridgeFrame, JsonValue, MessageType } from "@figtether/shared";

import type { BridgeHandlers } from "../src/bridge.js";
import { AppController } from "../src/controller.js";
import type { ActionName, BridgePort, UiModel, UiPort } from "../src/controller.js";
import type { SandboxChannel } from "../src/sandbox.js";
import { createSecretStore } from "../src/secret.js";
import { UI_CHANNEL } from "../src/protocol.js";
import type { GrantMode, UiEnvelope } from "../src/protocol.js";

/** The double of the interface of the plugin. */
export class FakeUi implements UiPort {
  /** The model of the last render. */
  public model: UiModel | null = null;
  /** The sanitized log lines. */
  public readonly logs: string[] = [];
  /** The results of the actions. */
  public readonly results: { action: ActionName; ok: boolean; message: string }[] = [];
  /** The message of the pairing field. */
  public pairError: string | null = null;
  /** Whether the confirmation of the revoke is visible. */
  public revokeVisible = false;
  /** Whether the pairing field was focused. */
  public focused = false;

  render(model: UiModel): void {
    this.model = model;
  }
  appendLog(line: string): void {
    this.logs.push(line);
  }
  setPairError(message: string | null): void {
    this.pairError = message;
  }
  setActionResult(action: ActionName, ok: boolean, message: string): void {
    this.results.push({ action, ok, message });
  }
  setRevokeConfirmVisible(visible: boolean): void {
    this.revokeVisible = visible;
  }
  focusPairInput(): void {
    this.focused = true;
  }
}

/** One message that the controller posted to the sandbox. */
export interface SandboxPost {
  /** The kind of the message. */
  readonly type: string;
  /** The payload of the message. */
  readonly payload: unknown;
  /** The request the message belongs to. */
  readonly requestId: string | null;
}

/** The double of the channel to the main sandbox. */
export class FakeSandbox implements SandboxChannel {
  /** Every message the controller posted. */
  public readonly posted: SandboxPost[] = [];
  /** Whether the channel was disposed. */
  public disposed = false;
  /** Called for every message the controller posted. */
  public onPost: ((post: SandboxPost) => void) | null = null;

  post<TPayload>(type: string, payload: TPayload, requestId?: string): void {
    const post: SandboxPost = {
      type,
      payload,
      requestId: requestId ?? null,
    };
    this.posted.push(post);
    if (this.onPost !== null) {
      this.onPost(post);
    }
  }
  dispose(): void {
    this.disposed = true;
  }

  /** The last message of a kind. */
  lastOf(type: string): SandboxPost | undefined {
    for (let index = this.posted.length - 1; index >= 0; index -= 1) {
      const entry = this.posted[index] as SandboxPost;
      if (entry.type === type) {
        return entry;
      }
    }
    return undefined;
  }

  /** Every message of a kind. */
  allOf(type: string): SandboxPost[] {
    return this.posted.filter((entry) => entry.type === type);
  }
}

/** The double of the socket to the local daemon. */
export class FakeBridge implements BridgePort {
  /** The session of the handshake. */
  public sessionId: string | null = null;
  /** The epoch of the connection. */
  public connectionEpoch = 0;
  /** The epoch of the grant. */
  public grantEpoch = 0;
  /** Every frame the controller sent, as the controller wrote it. */
  public readonly frames: { type: string; payload: unknown; requestId?: string }[] = [];
  /** Every frame the controller sent, as the daemon reads it on the wire. */
  public readonly wire: BridgeFrame[] = [];
  /** The secret the user pasted. */
  public pairedWith: string | null = null;

  private readonly pending = new Map<string, (value: JsonValue) => void>();
  private requestCounter = 0;

  constructor(readonly handlers: BridgeHandlers) {}

  pair(secret: string): void {
    this.pairedWith = secret;
  }
  sendGrantUpdate(rootIds: readonly string[], grantEpoch: number, mode: GrantMode): boolean {
    this.frames.push({
      type: "grant_update",
      payload: { root_ids: [...rootIds], grant_epoch: grantEpoch, mode },
    });
    return true;
  }
  revoke(): boolean {
    this.frames.push({ type: "revoke", payload: { reason: "user_revoked" } });
    return true;
  }
  sendFrame(messageType: MessageType, payload: JsonValue, requestId?: string): boolean {
    const entry: { type: string; payload: unknown; requestId?: string } = {
      type: messageType,
      payload,
    };
    if (requestId !== undefined) {
      entry.requestId = requestId;
    }
    this.frames.push(entry);
    this.requestCounter += 1;
    const id = requestId ?? `ui_${this.requestCounter}`;
    this.wire.push(createFrame(messageType, id, payload));
    this.complete(messageType, id, payload);
    return true;
  }

  /**
   * Wait for the answer of a request.
   *
   * The promise resolves with the data of the result frame, the way the
   * daemon completes a pending request.
   */
  awaitAnswer(requestId: string): Promise<JsonValue> {
    return new Promise((resolve) => {
      this.pending.set(requestId, resolve);
    });
  }

  /** The grant updates the controller sent. */
  grantUpdates(): { root_ids: string[]; grant_epoch: number; mode: GrantMode }[] {
    return this.frames
      .filter((frame) => frame.type === "grant_update")
      .map(
        (frame) => frame.payload as { root_ids: string[]; grant_epoch: number; mode: GrantMode },
      );
  }

  private complete(messageType: MessageType, requestId: string, payload: JsonValue): void {
    if (messageType !== "result") {
      return;
    }
    const resolve = this.pending.get(requestId);
    if (resolve === undefined) {
      return;
    }
    this.pending.delete(requestId);
    resolve(isJsonObject(payload) ? (payload["data"] ?? null) : null);
  }
}

/** The parts of the controller a test drives. */
export interface Harness {
  /** The controller under test. */
  readonly controller: AppController;
  /** The double of the interface. */
  readonly ui: FakeUi;
  /** The double of the channel to the sandbox. */
  readonly sandbox: FakeSandbox;
  /** The double of the socket to the daemon. */
  readonly bridge: FakeBridge;
  /** The store of the pairing secret. */
  readonly secret: ReturnType<typeof createSecretStore>;
}

/** Build a controller with the three doubles. */
export function harness(): Harness {
  const ui = new FakeUi();
  const sandbox = new FakeSandbox();
  const secret = createSecretStore();
  let bridge: FakeBridge | null = null;
  const controller = new AppController({
    sandbox,
    ui,
    secret,
    now: (): number => 0,
    bridgeFactory: (handlers: BridgeHandlers): BridgePort => {
      bridge = new FakeBridge(handlers);
      return bridge;
    },
  });
  return { controller, ui, sandbox, bridge: bridge as unknown as FakeBridge, secret };
}

/** Build an envelope of the sandbox, the way the sandbox sends it. */
export function sandboxEnvelope(
  type: string,
  payload: unknown,
  requestId?: string,
): UiEnvelope {
  return {
    channel: UI_CHANNEL,
    kind: type,
    type,
    payload,
    ...(requestId === undefined ? {} : { request_id: requestId }),
  };
}

/** Let the controller believe that the handshake was accepted. */
export function acceptHandshake(bridge: FakeBridge): void {
  bridge.handlers.onAuthAccepted({
    session_credential: "session-credential",
    session_id: "session-01",
    connection_epoch: 1,
    grant_epoch: 0,
    capabilities: { list_nodes: { status: "available" } },
    session_name: "Design review",
  });
}

/** Build a frame of the daemon. */
export function bridgeFrame(
  messageType: MessageType,
  payload: unknown,
  requestId = "req-01",
): BridgeFrame {
  return {
    protocol_version: "1.0",
    message_type: messageType,
    request_id: requestId,
    session_id: "session-01",
    connection_epoch: 1,
    grant_epoch: 1,
    payload: payload as JsonValue,
  };
}
