import { describe, expect, it } from "vitest";

import { BridgeClient } from "../src/bridge.js";
import type { BridgeHandlers, SocketLike, TimerApi } from "../src/bridge.js";
import type { LogEntry } from "../src/log.js";

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
  deliver(data: string): void {
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
  readonly states: string[];
  readonly logs: LogEntry[];
  readonly accepted: unknown[];
  readonly rejected: { code: string; message: string }[];
  readonly errors: { code: string }[];
}

function recorder(): { handlers: BridgeHandlers; record: Recorder } {
  const record: Recorder = {
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
    onFrame: (): void => undefined,
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

  it("sends the grant update with the roots, the epoch and the mode", () => {
    const socket = new FakeSocket("ws://127.0.0.1:38947/bridge");
    const { bridge } = build([socket]);
    bridge.pair("pairing-secret");
    socket.open();
    socket.deliver(authAnswer());
    expect(bridge.sendGrantUpdate(["1:2"], 1, "paused")).toBe(true);
    const frames = socket.frames();
    const update = frames[frames.length - 1];
    expect(update?.message_type).toBe("grant_update");
    expect(update?.payload["root_ids"]).toEqual(["1:2"]);
    expect(update?.payload["grant_epoch"]).toBe(1);
    expect(update?.payload["mode"]).toBe("paused");
  });
});
