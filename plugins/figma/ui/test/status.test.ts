import { describe, expect, it } from "vitest";

import {
  connectionLabel,
  connectionMarker,
  derivePhase,
  fingerprintSessionId,
  formatTimestamp,
  modeLabel,
  phaseLabel,
  reduceConnection,
} from "../src/status.js";
import type { ConnectionState } from "../src/status.js";

describe("reduceConnection", () => {
  it("walks from unpaired to connected", () => {
    let state: ConnectionState = "unpaired";
    state = reduceConnection(state, { type: "dial_started" });
    expect(state).toBe("connecting");
    state = reduceConnection(state, { type: "auth_accepted" });
    expect(state).toBe("connected");
  });

  it("returns to unpaired when the handshake is refused", () => {
    let state: ConnectionState = "unpaired";
    state = reduceConnection(state, { type: "dial_started" });
    state = reduceConnection(state, { type: "auth_rejected" });
    expect(state).toBe("unpaired");
  });

  it("goes offline when the socket dies", () => {
    let state: ConnectionState = "connected";
    state = reduceConnection(state, { type: "socket_closed" });
    expect(state).toBe("offline");
  });

  it("goes offline when the heartbeat is missed", () => {
    let state: ConnectionState = "connected";
    state = reduceConnection(state, { type: "heartbeat_timeout" });
    expect(state).toBe("offline");
  });

  it("allows a reconnect from offline but not from revoked", () => {
    expect(reduceConnection("offline", { type: "dial_started" })).toBe("connecting");
    expect(reduceConnection("revoked", { type: "dial_started" })).toBe("revoked");
  });

  it("keeps the revoked state after a socket close", () => {
    expect(reduceConnection("revoked", { type: "socket_closed" })).toBe("revoked");
    expect(reduceConnection("revoked", { type: "heartbeat_timeout" })).toBe("revoked");
  });

  it("leaves the revoked state only through a reset", () => {
    expect(reduceConnection("revoked", { type: "reset" })).toBe("unpaired");
  });

  it("ignores an auth acceptance that did not follow a dial", () => {
    expect(reduceConnection("unpaired", { type: "auth_accepted" })).toBe("unpaired");
    expect(reduceConnection("offline", { type: "auth_accepted" })).toBe("offline");
  });

  it("never returns an unknown state", () => {
    const states: ConnectionState[] = ["unpaired", "connecting", "connected", "offline", "revoked"];
    for (const state of states) {
      expect(states).toContain(reduceConnection(state, { type: "dial_started" }));
      expect(states).toContain(reduceConnection(state, { type: "auth_accepted" }));
      expect(states).toContain(reduceConnection(state, { type: "socket_closed" }));
    }
  });
});

describe("derivePhase", () => {
  it("maps the connection state and the grant onto the session phase", () => {
    expect(derivePhase("unpaired", false, false)).toBe("unpaired");
    expect(derivePhase("connecting", false, false)).toBe("pairing");
    expect(derivePhase("connected", false, false)).toBe("awaiting_grant");
    expect(derivePhase("connected", false, true)).toBe("ready");
    expect(derivePhase("connected", true, true)).toBe("paused");
    expect(derivePhase("offline", false, true)).toBe("offline");
    expect(derivePhase("revoked", false, false)).toBe("revoked");
  });
});

describe("labels", () => {
  it("names every state with text and a text marker", () => {
    const states: ConnectionState[] = ["unpaired", "connecting", "connected", "offline", "revoked"];
    for (const state of states) {
      expect(connectionLabel(state).length).toBeGreaterThan(0);
      expect(connectionMarker(state).length).toBeGreaterThan(0);
    }
    expect(connectionLabel("connected")).toBe("Connected");
  });

  it("names the read modes", () => {
    expect(modeLabel("live")).toBe("Live");
    expect(modeLabel("snapshot")).toBe("Snapshot");
    expect(modeLabel("offline_bundle")).toBe("Offline bundle");
    expect(modeLabel("something else")).toBe("Unknown");
  });

  it("names the session phases", () => {
    expect(phaseLabel("ready")).toBe("Ready");
    expect(phaseLabel("paused")).toBe("Paused");
  });
});

describe("fingerprintSessionId", () => {
  it("is stable and short", () => {
    const fingerprint = fingerprintSessionId("session-01");
    expect(fingerprint).toBe(fingerprintSessionId("session-01"));
    expect(fingerprint.length).toBeGreaterThanOrEqual(8);
    expect(fingerprint.length).toBeLessThanOrEqual(16);
  });

  it("separates two different sessions", () => {
    expect(fingerprintSessionId("session-01")).not.toBe(fingerprintSessionId("session-02"));
  });

  it("does not contain the session identifier", () => {
    const fingerprint = fingerprintSessionId("super-secret-session-id");
    expect(fingerprint).not.toContain("super-secret-session-id");
  });
});

describe("formatTimestamp", () => {
  it("reports a readable time", () => {
    const stamp = new Date(2024, 0, 2, 3, 4, 5).getTime();
    expect(formatTimestamp(stamp)).toBe("03:04:05");
  });

  it("reports the absence of a read", () => {
    expect(formatTimestamp(null)).toBe("No read yet");
    expect(formatTimestamp(Number.NaN)).toBe("No read yet");
  });
});
