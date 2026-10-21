/**
 * Connection state machine and the status model shown in the UI.
 *
 * The state machine is a pure reducer so that the transitions can be tested
 * without a browser and without a socket. The states mirror the session
 * states of `docs/02-architecture/05-state-machines.md` as seen from the UI.
 */

/** Connection state of the plugin UI. */
export type ConnectionState = "unpaired" | "connecting" | "connected" | "offline" | "revoked";

/** Phase of the session as presented to the designer. */
export type SessionPhase =
  "unpaired" | "pairing" | "awaiting_grant" | "ready" | "paused" | "offline" | "revoked";

/** Events that drive the connection state machine. */
export type ConnectionEvent =
  | { readonly type: "dial_started" }
  | { readonly type: "auth_accepted" }
  | { readonly type: "auth_rejected" }
  | { readonly type: "socket_closed" }
  | { readonly type: "heartbeat_timeout" }
  | { readonly type: "revoked" }
  | { readonly type: "reset" };

/**
 * Reduce one connection event.
 *
 * Unknown or impossible transitions keep the current state. The machine never
 * leaves the terminal state `revoked` except through an explicit `reset`,
 * which an explicit Pair action uses before authenticating a new session.
 */
export function reduceConnection(state: ConnectionState, event: ConnectionEvent): ConnectionState {
  switch (event.type) {
    case "dial_started":
      if (state === "unpaired" || state === "offline") {
        return "connecting";
      }
      return state;
    case "auth_accepted":
      if (state === "connecting") {
        return "connected";
      }
      return state;
    case "auth_rejected":
      if (state === "connecting") {
        return "unpaired";
      }
      return state;
    case "socket_closed":
    case "heartbeat_timeout":
      if (state === "connecting" || state === "connected") {
        return "offline";
      }
      return state;
    case "revoked":
      if (state === "revoked") {
        return state;
      }
      return "revoked";
    case "reset":
      return "unpaired";
    default:
      return state;
  }
}

/** Human readable label of a connection state. The label never relies on color. */
export function connectionLabel(state: ConnectionState): string {
  switch (state) {
    case "unpaired":
      return "Not paired";
    case "connecting":
      return "Connecting";
    case "connected":
      return "Connected";
    case "offline":
      return "Offline";
    case "revoked":
      return "Revoked";
    default:
      return "Unknown";
  }
}

/** Text marker that repeats the state without color. */
export function connectionMarker(state: ConnectionState): string {
  switch (state) {
    case "unpaired":
      return "o";
    case "connecting":
      return "~";
    case "connected":
      return "+";
    case "offline":
      return "!";
    case "revoked":
      return "x";
    default:
      return "?";
  }
}

/** Human readable label of a read mode. */
export function modeLabel(mode: string): string {
  switch (mode) {
    case "live":
      return "Live";
    case "snapshot":
      return "Snapshot";
    case "offline_bundle":
      return "Offline bundle";
    default:
      return "Unknown";
  }
}

/** Derive the session phase from the connection state and the grant mode. */
export function derivePhase(
  connection: ConnectionState,
  paused: boolean,
  hasGrant: boolean,
): SessionPhase {
  switch (connection) {
    case "unpaired":
      return "unpaired";
    case "connecting":
      return "pairing";
    case "offline":
      return "offline";
    case "revoked":
      return "revoked";
    case "connected":
      if (paused) {
        return "paused";
      }
      return hasGrant ? "ready" : "awaiting_grant";
    default:
      return "unpaired";
  }
}

/** Human readable label of a session phase. */
export function phaseLabel(phase: SessionPhase): string {
  switch (phase) {
    case "unpaired":
      return "Not paired";
    case "pairing":
      return "Pairing";
    case "awaiting_grant":
      return "Waiting for a grant";
    case "ready":
      return "Ready";
    case "paused":
      return "Paused";
    case "offline":
      return "Offline";
    case "revoked":
      return "Revoked";
    default:
      return "Unknown";
  }
}

/**
 * Short, stable fingerprint of a session identifier.
 *
 * The fingerprint lets a user confirm that the session on screen is the one
 * they expect without exposing the identifier itself.
 */
export function fingerprintSessionId(sessionId: string): string {
  let hash = 0x811c9dc5;
  for (let index = 0; index < sessionId.length; index += 1) {
    hash ^= sessionId.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  const first = hash.toString(16).padStart(8, "0");
  let second = 0x9e3779b9;
  for (let index = sessionId.length - 1; index >= 0; index -= 1) {
    second ^= sessionId.charCodeAt(index);
    second = Math.imul(second, 0x85ebca6b) >>> 0;
  }
  return `${first}${second.toString(16).padStart(8, "0").slice(0, 4)}`;
}

/** Format a timestamp for the status strip. */
export function formatTimestamp(value: number | null): string {
  if (value === null || !Number.isFinite(value)) {
    return "No read yet";
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return "No read yet";
  }
  const hours = String(date.getHours()).padStart(2, "0");
  const minutes = String(date.getMinutes()).padStart(2, "0");
  const seconds = String(date.getSeconds()).padStart(2, "0");
  return `${hours}:${minutes}:${seconds}`;
}
