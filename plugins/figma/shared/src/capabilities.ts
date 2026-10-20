/**
 * Capability states probed in the plugin runtime.
 *
 * Mirror of `CapabilityState` in `crates/model/src/capabilities.rs`. The Rust
 * type is an internally tagged enum, so the wire shape carries a `status`
 * discriminator. An unavailable or unsupported capability is a valid result and
 * never an error (PRD 8.7, AT12).
 */

/** A capability that can be used right now. */
export interface CapabilityAvailable {
  status: "available";
}

/** A capability that exists but cannot be used in this runtime. */
export interface CapabilityUnavailable {
  status: "unavailable";
  /** Why the capability is unavailable. */
  reason: string;
}

/** A capability that is not supported by this platform or seat. */
export interface CapabilityUnsupported {
  status: "unsupported";
  /** Where the capability would be supported. */
  reason: string;
}

/** State of one probed capability. */
export type CapabilityState = CapabilityAvailable | CapabilityUnavailable | CapabilityUnsupported;

/** Probed capabilities by name. */
export type CapabilityMap = Record<string, CapabilityState>;

/** Build the `available` capability state. */
export function capabilityAvailable(): CapabilityAvailable {
  return { status: "available" };
}

/** Build the `unavailable` capability state with a reason. */
export function capabilityUnavailable(reason: string): CapabilityUnavailable {
  return { status: "unavailable", reason };
}

/** Build the `unsupported` capability state with a reason. */
export function capabilityUnsupported(reason: string): CapabilityUnsupported {
  return { status: "unsupported", reason };
}

/** Whether a capability can be used right now. */
export function isCapabilityAvailable(state: CapabilityState): boolean {
  return state.status === "available";
}

/** Whether an unknown value is a capability state. */
export function isCapabilityState(value: unknown): value is CapabilityState {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const candidate = value as { status?: unknown; reason?: unknown };
  if (candidate.status === "available") {
    return true;
  }
  if (candidate.status === "unavailable" || candidate.status === "unsupported") {
    return typeof candidate.reason === "string";
  }
  return false;
}

/** Short reason when the capability is not available. */
export function capabilityReason(state: CapabilityState): string | undefined {
  return state.status === "available" ? undefined : state.reason;
}
