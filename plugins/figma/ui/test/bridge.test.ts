import { describe, expect, it } from "vitest";

import {
  BridgeClient,
  CONNECT_TIMEOUT_MS,
  isBridgePolicyViolation,
  loadedLoopbackPorts,
} from "../src/bridge.js";
import type { BridgeHandlers, SocketLike, TimerApi } from "../src/bridge.js";
import type { LogEntry } from "../src/log.js";
import { AppController } from "../src/controller.js";
import { FakeSandbox, FakeUi, sandboxEnvelope } from "./fakes.js";

class FakeSocket implements SocketLike {
  onopen: ((event: unknown) => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: ((event: { code?: number; reason?: string }) => void) | null = null;
  onerror: ((event: unknown) => void) | null = null;
  readonly sent: string[] = [];
  closed = false;

  constructor(readonly url: string) {}

  send(data: string): void {
    this.sent.push(data);
  }
  close(): void {
    this.closed = true;
  }
  open(): void {
    this.onopen?.(null);
  }
  deliver(data: unknown): void {
    this.onmessage?.({ data });
  }
  drop(): void {
    this.onerror?.(null);
    this.onclose?.({ code: 1006, reason: "lost" });
  }

  frames(): { message_type: string; request_id: string; payload: Record<string, unknown> }[] {
    return this.sent.map(
      (raw) =>
        JSON.parse(raw) as {
          message_type: string;
          request_id: string;
          payload: Record<string, unknown>;
        },
    );
  }
}

class FakeTimers implements TimerApi {
  private nextId = 1;
  private current = 0;
  private readonly timers = new Map<
    number,
    { handler: () => void; due: number; every: number | null }
  >();

  now(): number {
    return this.current;
  }

  setTimeout(handler: () => void, ms: number): number {
    const id = this.nextId;
    this.nextId += 1;
    this.timers.set(id, { handler, due: this.current + ms, every: null });
    return id;
  }
  clearTimeout(id: number): void {
    this.timers.delete(id);
  }
  setInterval(handler: () => void, ms: number): number {
    const id = this.nextId;
    this.nextId += 1;
    this.timers.set(id, { handler, due: this.current + ms, every: ms });
    return id;
  }
  clearInterval(id: number): void {
    this.timers.delete(id);
  }
  advance(ms: number): void {
    const target = this.current + ms;
    for (;;) {
      let nextId: number | null = null;
      let nextDue = Number.POSITIVE_INFINITY;
      for (const [id, timer] of this.timers) {
        if (timer.due <= target && timer.due < nextDue) {
          nextDue = timer.due;
          nextId = id;
        }
      }
      if (nextId === null) {
        break;
      }
      const timer = this.timers.get(nextId) as {
        handler: () => void;
        due: number;
        every: number | null;
      };
      this.current = timer.due;
      if (timer.every === null) {
        this.timers.delete(nextId);
      } else {
        timer.due = timer.due + timer.every;
      }
      timer.handler();
    }
    this.current = target;
  }
}

interface Recorder {
  readonly frames: unknown[];
  readonly states: string[];
  readonly logs: LogEntry[];
  readonly accepted: unknown[];
  readonly rejected: { code: string; message: string }[];
  readonly errors: { code: string }[];
}

function recorder(): { handlers: BridgeHandlers; record: Recorder } {
  const record: Recorder = {
    frames: [],
    states: [],
    logs: [],
    accepted: [],
    rejected: [],
    errors: [],
  };
  const handlers: BridgeHandlers = {
    onConnection: (state): void => {
      record.states.push(state);
    },
    onAuthAccepted: (payload): void => {
      record.accepted.push(payload);
    },
    onAuthRejected: (code, message): void => {
      record.rejected.push({ code, message });
    },
    onFrame: (frame): void => {
      record.frames.push(frame);
    },
    onError: (error): void => {
      record.errors.push({ code: error.code });
    },
    onLog: (entry): void => {
      record.logs.push(entry);
    },
  };
  return { handlers, record };
}

function build(sockets: FakeSocket[]): {
  bridge: BridgeClient;
  timers: FakeTimers;
  record: Recorder;
} {
  const { handlers, record } = recorder();
  const timers = new FakeTimers();
  let index = 0;
  const bridge = new BridgeClient({
    clientId: "client-01",
    handlers,
    timers,
    requestIdFactory: (): string => `ui_test_${index}`,
    socketFactory: (): SocketLike => {
      const socket = sockets[index];
      index += 1;
      if (socket === undefined) {
        throw new Error("no socket available");
      }
      return socket;
    },
  });
  return { bridge, timers, record };
}

/** The answer of the daemon to the handshake, in the frozen wire shape. */
function authAnswer(sessionId = "session-01"): string {
  return JSON.stringify({
    protocol_version: "1.0",
    message_type: "result",
    request_id: "auth",
    session_id: sessionId,
    connection_epoch: 1,
    grant_epoch: null,
    payload: {
      session_credential: "session-credential",
      session_id: sessionId,
      connection_epoch: 1,
      grant_epoch: null,
      capabilities: { list_nodes: { status: "available" } },
    },
  });
}

describe("BridgeClient", () => {
  it.each(["ui_control", "capabilities", "request", "progress", "chunk", "cancel"])(
    "relays %s from the socket to the controller",
    (type) => {
      const socket = new FakeSocket("ws://localhost:38947/bridge");
      const { bridge, record } = build([socket]);
      bridge.pair("synthetic-secret");
      socket.open();
      socket.deliver(authAnswer());
      socket.deliver(
        JSON.stringify({
          protocol_version: "1.0",
          message_type: type,
          request_id: "relay-test",
          connection_epoch: 1,
          payload: { action: "opened" },
        }),
      );
      expect(record.frames).toHaveLength(1);
      expect(record.frames[0]).toMatchObject({ message_type: type });
      bridge.stop();
    },
  );
  it("leaves connecting when a socket reports only an error without close", () => {
    const socket = new FakeSocket("ws://localhost:38947/bridge");
    const { bridge } = build([socket]);
    bridge.pair("synthetic-secret");
    socket.onerror?.(null);
    expect(bridge.connectionState).toBe("offline");
    expect(socket.closed).toBe(true);
    socket.open();
    expect(socket.sent).toHaveLength(0);
    bridge.stop();
  });

  it("times out a socket that never opens or reports an error", () => {
    const socket = new FakeSocket("ws://localhost:38947/bridge");
    const { bridge, timers } = build([socket]);
    bridge.pair("synthetic-secret");
    timers.advance(CONNECT_TIMEOUT_MS);
    expect(bridge.connectionState).toBe("offline");
    expect(socket.closed).toBe(true);
    bridge.stop();
  });

  it("bounds retries even when socket construction throws", () => {
    const { bridge, timers, record } = build([]);
    bridge.pair("synthetic-secret");
    expect(bridge.connectionState).toBe("offline");
    timers.advance(120000);
    expect(bridge.connectionState).toBe("offline");
    expect(record.logs.filter((entry) => entry.operation === "reconnect")).toHaveLength(6);
    bridge.stop();
  });

  it("blocks repeated pairing and shows the canonical import after a policy failure", () => {
    const ui = new FakeUi();
    const timers = new FakeTimers();
    let attempts = 0;
    const controller = new AppController({
      ui,
      sandbox: new FakeSandbox(),
      bridgeFactory: (handlers) =>
        new BridgeClient({
          clientId: "test-client",
          handlers,
          timers,
          socketFactory: () => {
            attempts++;
            throw new DOMException("blocked", "SecurityError");
          },
        }),
    });
    controller.onPairRequested("synthetic-secret");
    expect(ui.model?.connection).toBe("offline");
    expect(ui.model?.canPair).toBe(false);
    expect(ui.pairError).toContain("import steps");
    expect(ui.model?.connectionRecovery?.manifestPath).toBe("plugins/figma/dist/manifest.json");
    expect(ui.results.at(-1)?.message).toContain("Reimport");
    expect(ui.results.at(-1)?.ok).toBe(false);
    controller.onPairRequested("another-secret");
    timers.advance(120000);
    expect(attempts).toBe(1);
    controller.dispose();
  });

  it("checks reachability without authenticating or consuming a pairing secret", () => {
    const probe = new FakeSocket("ws://localhost:38947/bridge");
    const paired = new FakeSocket("ws://localhost:38947/bridge");
    const { bridge, timers, record } = build([probe, paired]);
    bridge.checkConnection();
    probe.open();
    expect(probe.sent).toHaveLength(0);
    expect(probe.closed).toBe(true);
    expect(bridge.connectionState).toBe("unpaired");
    expect(record.accepted).toHaveLength(0);
    timers.advance(60000);
    expect(record.rejected).toHaveLength(0);
    bridge.pair("fresh-one-use-secret");
    paired.open();
    expect(paired.frames()[0]?.payload["secret"]).toBe("fresh-one-use-secret");
    bridge.stop();
  });

  it("detects stale Figma permissions before asking for a secret and stops retries", () => {
    const socket = new FakeSocket("ws://localhost:38947/bridge");
    const ui = new FakeUi();
    const timers = new FakeTimers();
    const controller = new AppController({
      ui,
      sandbox: new FakeSandbox(),
      bridgeFactory: (handlers) =>
        new BridgeClient({ clientId: "probe", handlers, timers, socketFactory: () => socket }),
    });
    controller.start();
    controller.reportPolicyViolation(
      "default-src data: blob: http://localhost:39029 ws://localhost:39029",
    );
    expect(ui.model?.connectionRecovery?.allowedPorts).toEqual([39029]);
    expect(ui.model?.canPair).toBe(false);
    expect(ui.model?.nextStepText).toContain("import");
    expect(socket.sent).toHaveLength(0);
    timers.advance(120000);
    expect(ui.model?.connection).toBe("offline");
    controller.dispose();
  });

  it("reads connect-src before default-src and deduplicates valid loopback ports", () => {
    expect(
      loadedLoopbackPorts(
        "default-src ws://localhost:38947; connect-src http://localhost:39029 ws://localhost:39029 ws://example.com:5555",
      ),
    ).toEqual([39029]);
    expect(
      loadedLoopbackPorts("default-src ws://127.0.0.1:39029 ws://localhost:0 ws://localhost:65536"),
    ).toEqual([39029]);
    expect(loadedLoopbackPorts("default-src ws://localhost:38947; connect-src 'none'")).toEqual([]);
  });

  it("recognises policy failures for localhost and numeric loopback on this port only", () => {
    expect(isBridgePolicyViolation("ws://localhost:38947/bridge")).toBe(true);
    expect(isBridgePolicyViolation("ws://127.0.0.1:38947/bridge")).toBe(true);
    for (const url of [
      "ws://localhost:39029/bridge",
      "ws://example.com:38947/bridge",
      "https://localhost:38947/image",
      "not a URL",
    ]) {
      expect(isBridgePolicyViolation(url)).toBe(false);
    }
  });
  it("uses a new pairing secret instead of a credential from an old daemon", () => {
    const first = new FakeSocket("ws://localhost:38947/bridge");
    const second = new FakeSocket("ws://localhost:38947/bridge");
    const { bridge } = build([first, second]);
    bridge.pair("old-pairing-secret");
    first.open();
    first.deliver(authAnswer());
    bridge.pair("new-pairing-secret");
    second.open();
    expect(second.frames()[0]?.payload["secret"]).toBe("new-pairing-secret");
    expect(bridge.sessionId).toBeNull();
  });
  it("sends the authentication frame when the socket opens", () => {
    const socket = new FakeSocket("ws://127.0.0.1:38947/bridge");
    const { bridge } = build([socket]);
    bridge.pair("pairing-secret");
    socket.open();
    const frames = socket.frames();
    expect(frames).toHaveLength(1);
    expect(frames[0]?.message_type).toBe("hello_auth");
    expect(frames[0]?.payload["secret"]).toBe("pairing-secret");
    expect(frames[0]?.payload["client_id"]).toBe("client-01");
    expect(frames[0]?.payload["protocol_version"]).toBe("1.0");
    expect(frames[0]?.request_id.length).toBeGreaterThan(0);
  });

  it("refuses an empty secret", () => {
    const socket = new FakeSocket("ws://127.0.0.1:38947/bridge");
    const { bridge, record } = build([socket]);
    bridge.pair("   ");
    expect(record.rejected).toHaveLength(1);
    expect(record.rejected[0]?.code).toBe("AUTH_REQUIRED");
    expect(bridge.connectionState).toBe("unpaired");
    expect(socket.sent).toHaveLength(0);
  });

  it("reports a handshake that did not complete inside the window", () => {
    const socket = new FakeSocket("ws://127.0.0.1:38947/bridge");
    const { bridge, timers, record } = build([socket]);
    bridge.pair("pairing-secret");
    socket.open();
    timers.advance(3000);
    expect(record.rejected).toHaveLength(1);
    expect(record.rejected[0]?.code).toBe("AUTH_REQUIRED");
    expect(bridge.connectionState).toBe("unpaired");
    expect(socket.closed).toBe(true);
  });

  it("accepts the answer of the daemon and moves to connected", () => {
    const socket = new FakeSocket("ws://127.0.0.1:38947/bridge");
    const { bridge, record } = build([socket]);
    bridge.pair("pairing-secret");
    socket.open();
    socket.deliver(authAnswer());
    expect(record.accepted).toHaveLength(1);
    expect(record.states).toContain("connected");
  });

  it("sends a heartbeat every five seconds", () => {
    const socket = new FakeSocket("ws://127.0.0.1:38947/bridge");
    const { bridge, timers } = build([socket]);
    bridge.pair("pairing-secret");
    socket.open();
    socket.deliver(authAnswer());
    timers.advance(5000);
    timers.advance(5000);
    const heartbeats = socket.frames().filter((frame) => frame.message_type === "heartbeat");
    expect(heartbeats.length).toBe(2);
  });

  it("echoes the connection epoch on every frame after the handshake", () => {
    const socket = new FakeSocket("ws://127.0.0.1:38947/bridge");
    const { bridge, timers } = build([socket]);
    bridge.pair("pairing-secret");
    socket.open();
    socket.deliver(authAnswer());
    timers.advance(5000);
    const frames = socket.frames();
    expect(frames.length).toBeGreaterThan(1);
    for (const frame of frames.slice(1)) {
      expect(frame).toHaveProperty("connection_epoch", 1);
    }
  });

  it("reconnects with the session credential and not with the secret", () => {
    const first = new FakeSocket("ws://127.0.0.1:38947/bridge");
    const second = new FakeSocket("ws://127.0.0.1:38947/bridge");
    const { bridge, timers } = build([first, second]);
    bridge.pair("pairing-secret");
    first.open();
    first.deliver(authAnswer());

    first.drop();
    timers.advance(1000);
    second.open();
    const frames = second.frames();
    expect(frames).toHaveLength(1);
    expect(frames[0]?.message_type).toBe("hello_auth");
    expect(frames[0]?.payload["secret"]).toBe("session-credential");
  });

  it("stops after the bounded number of reconnect attempts", () => {
    const sockets = [
      new FakeSocket("ws://127.0.0.1:38947/bridge"),
      new FakeSocket("ws://127.0.0.1:38947/bridge"),
      new FakeSocket("ws://127.0.0.1:38947/bridge"),
    ];
    const { bridge, timers, record } = build(sockets);
    bridge.pair("pairing-secret");
    sockets[0]?.open();
    sockets[0]?.deliver(authAnswer());
    sockets[0]?.drop();
    timers.advance(60000);
    expect(
      record.logs.filter((entry) => entry.operation === "reconnect").length,
    ).toBeLessThanOrEqual(6);
  });

  it("drops a frame that exceeds the frame limit", () => {
    const socket = new FakeSocket("ws://127.0.0.1:38947/bridge");
    const { bridge, record } = build([socket]);
    bridge.pair("pairing-secret");
    socket.open();
    socket.deliver(`{"message_type":"result","payload":{"blob":"${"x".repeat(1100000)}"}}`);
    expect(record.errors).toHaveLength(1);
    expect(record.errors[0]?.code).toBe("PAYLOAD_TOO_LARGE");
  });

  it("drops a message that is not valid JSON", () => {
    const socket = new FakeSocket("ws://127.0.0.1:38947/bridge");
    const { bridge, record } = build([socket]);
    bridge.pair("pairing-secret");
    socket.open();
    socket.deliver("{not json");
    expect(record.errors).toHaveLength(0);
    expect(record.rejected).toHaveLength(0);
  });

  it("drops a frame of an older connection epoch", () => {
    const socket = new FakeSocket("ws://127.0.0.1:38947/bridge");
    const { bridge, record } = build([socket]);
    bridge.pair("pairing-secret");
    socket.open();
    socket.deliver(authAnswer());
    socket.deliver(
      JSON.stringify({
        protocol_version: "1.0",
        message_type: "request",
        request_id: "req-old",
        connection_epoch: 0,
        payload: { op: "list_nodes", params: {} },
      }),
    );
    expect(record.states).toContain("connected");
  });

  it("refuses a refused handshake and forgets the credential", () => {
    const socket = new FakeSocket("ws://127.0.0.1:38947/bridge");
    const { bridge, record } = build([socket]);
    bridge.pair("pairing-secret");
    socket.open();
    socket.deliver(
      JSON.stringify({
        protocol_version: "1.0",
        message_type: "error",
        request_id: "auth",
        payload: {
          code: "AUTH_EXPIRED",
          message: "The secret expired.",
          retryable: false,
          action: "repair_pairing",
          request_id: "auth",
        },
      }),
    );
    expect(record.rejected).toHaveLength(1);
    expect(record.rejected[0]?.code).toBe("AUTH_EXPIRED");
    expect(record.states).toContain("unpaired");
  });

  it("refuses to send a frame of a message type the UI may not send", () => {
    const socket = new FakeSocket("ws://127.0.0.1:38947/bridge");
    const { bridge } = build([socket]);
    bridge.pair("pairing-secret");
    socket.open();
    expect(bridge.sendFrame("request", { op: "list_nodes", params: {} })).toBe(false);
    expect(bridge.sendFrame("result", { data: null })).toBe(true);
    expect(bridge.sendFrame("hello_auth", {})).toBe(true);
  });

  it("keeps the revoked state after the socket closes", () => {
    const socket = new FakeSocket("ws://127.0.0.1:38947/bridge");
    const { bridge, record } = build([socket]);
    bridge.pair("pairing-secret");
    socket.open();
    socket.deliver(authAnswer());
    bridge.revoke();
    expect(record.states).toContain("revoked");
    socket.drop();
    expect(record.states[record.states.length - 1]).toBe("revoked");
  });

  it.each(["local", "daemon"])("allows explicit fresh pairing after a %s revoke", (source) => {
    const first = new FakeSocket("ws://127.0.0.1:38947/bridge");
    const second = new FakeSocket("ws://127.0.0.1:38947/bridge");
    const { bridge, record, timers } = build([first, second]);
    bridge.pair("first-one-use-secret");
    first.open();
    first.deliver(authAnswer());
    if (source === "local") bridge.revoke();
    else
      first.deliver(
        JSON.stringify({
          protocol_version: "1.0",
          message_type: "revoke",
          request_id: "revoke",
          payload: { reason: "client_revoked" },
        }),
      );
    expect(record.states.at(-1)).toBe("revoked");
    expect(first.closed).toBe(true);
    expect(bridge.sessionId).toBeNull();
    expect(bridge.sendGrantUpdate(["1:2"], 2, "live", "write")).toBe(false);
    timers.advance(60000);
    expect(record.logs.filter((log) => log.operation === "reconnect")).toHaveLength(0);
    bridge.pair("new-one-use-secret");
    expect(record.states.at(-1)).toBe("connecting");
    second.open();
    expect(second.frames()[0]?.payload["secret"]).toBe("new-one-use-secret");
    second.deliver(authAnswer());
    expect(record.states.at(-1)).toBe("connected");
    expect(bridge.sendGrantUpdate(["1:9"], 1, "live", "read")).toBe(true);
  });

  it("ignores callbacks from a socket replaced by explicit pairing", () => {
    const first = new FakeSocket("ws://127.0.0.1:38947/bridge");
    const second = new FakeSocket("ws://127.0.0.1:38947/bridge");
    const { bridge, record } = build([first, second]);
    bridge.pair("first-secret");
    first.open();
    first.deliver(authAnswer());
    bridge.pair("replacement-secret");
    second.open();
    second.deliver(authAnswer());
    const accepted = record.accepted.length;
    first.drop();
    first.open();
    first.deliver(authAnswer());
    expect(record.states.at(-1)).toBe("connected");
    expect(record.accepted).toHaveLength(accepted);
    expect(bridge.sendGrantUpdate(["1:9"], 1, "live", "read")).toBe(true);
  });

  it("discards an old socket message that finishes decoding after a new pairing", async () => {
    const first = new FakeSocket("ws://127.0.0.1:38947/bridge");
    const second = new FakeSocket("ws://127.0.0.1:38947/bridge");
    const { bridge, record } = build([first, second]);
    bridge.pair("first-secret");
    first.open();
    first.deliver(authAnswer());
    let finish: (value: string) => void = () => undefined;
    const old = new Blob();
    old.text = () =>
      new Promise((resolve) => {
        finish = resolve;
      });
    first.deliver(old);
    bridge.pair("replacement-secret");
    second.open();
    second.deliver(authAnswer("new-session"));
    const accepted = record.accepted.length;
    finish(authAnswer("old-session"));
    await Promise.resolve();
    expect(record.accepted).toHaveLength(accepted);
    expect(bridge.sessionId).toBe("new-session");
  });

  it.each(["local", "daemon"])(
    "restores the complete UI sharing flow after %s revocation",
    (source) => {
      const sockets = [
        new FakeSocket("ws://127.0.0.1:38947/bridge"),
        new FakeSocket("ws://127.0.0.1:38947/bridge"),
      ];
      const ui = new FakeUi();
      const sandbox = new FakeSandbox();
      let next = 0;
      let realBridge: BridgeClient;
      const controller = new AppController({
        ui,
        sandbox,
        bridgeFactory: (handlers) => {
          realBridge = new BridgeClient({
            clientId: "integration-client",
            handlers,
            timers: new FakeTimers(),
            socketFactory: () => sockets[next++]!,
          });
          return realBridge;
        },
      });
      try {
        controller.onPairRequested("first-secret");
        sockets[0]!.open();
        sockets[0]!.deliver(authAnswer());
        controller.onSandboxEnvelope(
          sandboxEnvelope("selection", {
            roots: [{ node_id: "1:2", node_type: "FRAME", child_count: 0 }],
          }),
        );
        controller.onRootSelectionChanged(["1:2"]);
        controller.onAllowRequested();
        controller.onWriteGrantRequested();
        if (source === "local") controller.onRevokeConfirmed();
        else
          sockets[0]!.deliver(
            JSON.stringify({
              protocol_version: "1.0",
              message_type: "revoke",
              request_id: "revoke",
              payload: { reason: "client_revoked" },
            }),
          );
        expect(ui.model?.connection).toBe("revoked");
        controller.onPairRequested("fresh-secret");
        sockets[1]!.open();
        sockets[1]!.deliver(authAnswer("fresh-session"));
        expect(ui.model?.connection).toBe("connected");
        expect(ui.model?.grantedRootCount).toBe(0);
        expect(ui.model?.canGrantWrite).toBe(false);
        controller.onSandboxEnvelope(
          sandboxEnvelope("selection", {
            roots: [{ node_id: "1:9", node_type: "FRAME", child_count: 0 }],
          }),
        );
        controller.onRootSelectionChanged(["1:9"]);
        expect(ui.model?.canGrant).toBe(true);
        controller.onAllowRequested();
        expect(ui.model?.phase).toBe("ready");
        expect(
          sockets[1]!.frames().find((frame) => frame.message_type === "grant_update")?.payload,
        ).toMatchObject({ root_ids: ["1:9"], mode: "read" });
      } finally {
        realBridge!.stop();
        controller.dispose();
      }
    },
  );

  it("sends the grant update with the roots, the epoch and the two modes", () => {
    const socket = new FakeSocket("ws://127.0.0.1:38947/bridge");
    const { bridge } = build([socket]);
    bridge.pair("pairing-secret");
    socket.open();
    socket.deliver(authAnswer());
    expect(bridge.sendGrantUpdate(["1:2"], 1, "paused", "write")).toBe(true);
    const frames = socket.frames();
    const update = frames[frames.length - 1];
    expect(update?.message_type).toBe("grant_update");
    expect(update?.payload["root_ids"]).toEqual(["1:2"]);
    expect(update?.payload["grant_epoch"]).toBe(1);
    expect(update?.payload["mode"]).toBe("write");
    expect(update?.payload["session_mode"]).toBe("paused");
  });
});
