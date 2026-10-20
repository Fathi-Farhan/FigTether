/**
 * Error taxonomy of the plugin main sandbox.
 *
 * The codes are the stable wire strings of `figtether_model::errors::ErrorCode`
 * in `crates/model/src/errors.rs`. The plugin never invents a code, never
 * leaks a stack trace and never embeds design content in a message.
 */

import type { ErrorPayload } from "./types.js";

/** Canonical error codes used by this plugin. */
export const ERROR_CODES = {
  OUT_OF_SCOPE: "OUT_OF_SCOPE",
  CAPABILITY_UNAVAILABLE: "CAPABILITY_UNAVAILABLE",
  PAYLOAD_TOO_LARGE: "PAYLOAD_TOO_LARGE",
  SCOPE_TOO_LARGE: "SCOPE_TOO_LARGE",
  NODE_NOT_FOUND: "NODE_NOT_FOUND",
  EXPORT_FAILED: "EXPORT_FAILED",
  FONT_UNAVAILABLE: "FONT_UNAVAILABLE",
  INTERNAL_ERROR: "INTERNAL_ERROR",
  INVALID_LIMIT: "INVALID_LIMIT",
  CURSOR_EXPIRED: "CURSOR_EXPIRED",
  REVOKED: "REVOKED",
  PAUSED: "PAUSED",
  TIMEOUT: "TIMEOUT",
  CANCELLED: "CANCELLED",
  UNSUPPORTED_SCHEMA: "UNSUPPORTED_SCHEMA",
  WRITE_NOT_GRANTED: "WRITE_NOT_GRANTED",
  APPROVAL_REQUIRED: "APPROVAL_REQUIRED",
  PLAN_HASH_MISMATCH: "PLAN_HASH_MISMATCH",
  PLAN_INVALID: "PLAN_INVALID",
  BUDGET_EXCEEDED: "BUDGET_EXCEEDED",
  DESTRUCTIVE_NOT_ALLOWED: "DESTRUCTIVE_NOT_ALLOWED",
  MUTATION_REFUSED: "MUTATION_REFUSED",
} as const;

/** One canonical error code. */
export type ErrorCode = (typeof ERROR_CODES)[keyof typeof ERROR_CODES];

/** Recovery verbs from the error catalog, section 5. */
export const ERROR_ACTIONS = {
  WIDEN_GRANT: "widen_grant",
  USE_ALTERNATIVE: "use_alternative",
  NARROW_PARAMETERS: "narrow_parameters",
  REFRESH_TREE: "refresh_tree",
  CHOOSE_FALLBACK_FORMAT: "choose_fallback_format",
  USE_AVAILABLE_FONT: "use_available_font",
  CONTACT_SUPPORT: "contact_support",
  RESTART_TRAVERSAL: "restart_traversal",
  NEW_GRANT_REQUIRED: "new_grant_required",
  RESUME_BY_USER: "resume_by_user",
} as const;

/** One recovery verb. */
export type ErrorAction = (typeof ERROR_ACTIONS)[keyof typeof ERROR_ACTIONS];

/** Retry verdict per code, mirroring `ErrorCode::retryable`. */
const RETRYABLE: Readonly<Record<ErrorCode, boolean>> = {
  OUT_OF_SCOPE: false,
  CAPABILITY_UNAVAILABLE: false,
  PAYLOAD_TOO_LARGE: false,
  SCOPE_TOO_LARGE: false,
  NODE_NOT_FOUND: false,
  EXPORT_FAILED: false,
  FONT_UNAVAILABLE: false,
  INTERNAL_ERROR: false,
  INVALID_LIMIT: false,
  CURSOR_EXPIRED: false,
  REVOKED: false,
  PAUSED: false,
  TIMEOUT: true,
  CANCELLED: false,
  UNSUPPORTED_SCHEMA: false,
  WRITE_NOT_GRANTED: false,
  APPROVAL_REQUIRED: false,
  PLAN_HASH_MISMATCH: false,
  PLAN_INVALID: false,
  BUDGET_EXCEEDED: false,
  DESTRUCTIVE_NOT_ALLOWED: false,
  MUTATION_REFUSED: false,
};

/** Default recovery hint per code, mirroring `ErrorCode::default_action`. */
const DEFAULT_ACTION: Readonly<Record<ErrorCode, ErrorAction>> = {
  OUT_OF_SCOPE: ERROR_ACTIONS.WIDEN_GRANT,
  CAPABILITY_UNAVAILABLE: ERROR_ACTIONS.USE_ALTERNATIVE,
  PAYLOAD_TOO_LARGE: ERROR_ACTIONS.NARROW_PARAMETERS,
  SCOPE_TOO_LARGE: ERROR_ACTIONS.NARROW_PARAMETERS,
  NODE_NOT_FOUND: ERROR_ACTIONS.REFRESH_TREE,
  EXPORT_FAILED: ERROR_ACTIONS.CHOOSE_FALLBACK_FORMAT,
  FONT_UNAVAILABLE: ERROR_ACTIONS.USE_AVAILABLE_FONT,
  INTERNAL_ERROR: ERROR_ACTIONS.CONTACT_SUPPORT,
  INVALID_LIMIT: ERROR_ACTIONS.NARROW_PARAMETERS,
  CURSOR_EXPIRED: ERROR_ACTIONS.RESTART_TRAVERSAL,
  REVOKED: ERROR_ACTIONS.NEW_GRANT_REQUIRED,
  PAUSED: ERROR_ACTIONS.RESUME_BY_USER,
  TIMEOUT: ERROR_ACTIONS.NARROW_PARAMETERS,
  CANCELLED: ERROR_ACTIONS.RESTART_TRAVERSAL,
  UNSUPPORTED_SCHEMA: ERROR_ACTIONS.CONTACT_SUPPORT,
  WRITE_NOT_GRANTED: ERROR_ACTIONS.NEW_GRANT_REQUIRED,
  APPROVAL_REQUIRED: ERROR_ACTIONS.RESUME_BY_USER,
  PLAN_HASH_MISMATCH: ERROR_ACTIONS.NARROW_PARAMETERS,
  PLAN_INVALID: ERROR_ACTIONS.NARROW_PARAMETERS,
  BUDGET_EXCEEDED: ERROR_ACTIONS.NARROW_PARAMETERS,
  DESTRUCTIVE_NOT_ALLOWED: ERROR_ACTIONS.RESUME_BY_USER,
  MUTATION_REFUSED: ERROR_ACTIONS.USE_ALTERNATIVE,
};

/**
 * A structured plugin failure.
 *
 * The message is a fixed template per code. Only numeric counts and opaque
 * ids that the caller already knows may appear in the dynamic parts.
 */
export class PluginError extends Error {
  /** Stable error code. */
  readonly code: ErrorCode;
  /** Recovery hint. */
  readonly action: ErrorAction;
  /** Whether the caller may retry. */
  readonly retryable: boolean;
  /** Suggested delay before retrying, in milliseconds. */
  readonly retryAfterMs: number | undefined;

  constructor(
    code: ErrorCode,
    message: string,
    options?: {
      action?: ErrorAction;
      retryable?: boolean;
      retryAfterMs?: number;
    },
  ) {
    super(message);
    this.name = "PluginError";
    this.code = code;
    this.action = options?.action ?? DEFAULT_ACTION[code];
    this.retryable = options?.retryable ?? RETRYABLE[code];
    this.retryAfterMs = options?.retryAfterMs;
  }

  /** Render the error into the wire payload of an `error` frame. */
  toPayload(requestId: string): ErrorPayload {
    const payload: ErrorPayload = {
      code: this.code,
      message: this.message,
      retryable: this.retryable,
      action: this.action,
      request_id: requestId,
    };
    if (this.retryAfterMs !== undefined) {
      payload.retry_after_ms = this.retryAfterMs;
    }
    return payload;
  }
}

/** The requested node is outside the granted scope. */
export function outOfScope(message: string): PluginError {
  return new PluginError(ERROR_CODES.OUT_OF_SCOPE, message);
}

/** The API, editor version or seat does not expose the capability. */
export function capabilityUnavailable(message: string): PluginError {
  return new PluginError(ERROR_CODES.CAPABILITY_UNAVAILABLE, message);
}

/** The payload exceeded a byte budget. */
export function payloadTooLarge(message: string): PluginError {
  return new PluginError(ERROR_CODES.PAYLOAD_TOO_LARGE, message);
}

/** The traversal exceeded the node budget and cannot be resumed. */
export function scopeTooLarge(message: string): PluginError {
  return new PluginError(ERROR_CODES.SCOPE_TOO_LARGE, message);
}

/** The node does not exist in this document. */
export function nodeNotFound(message: string): PluginError {
  return new PluginError(ERROR_CODES.NODE_NOT_FOUND, message);
}

/** The export failed for the requested node or format. */
export function exportFailed(message: string): PluginError {
  return new PluginError(ERROR_CODES.EXPORT_FAILED, message);
}

/** A font required by the export is unavailable. */
export function fontUnavailable(message: string): PluginError {
  return new PluginError(ERROR_CODES.FONT_UNAVAILABLE, message);
}

/** An unexpected failure. The original message never leaves the sandbox. */
export function internalError(message: string): PluginError {
  return new PluginError(ERROR_CODES.INTERNAL_ERROR, message);
}

/** A requested limit is invalid. */
export function invalidLimit(message: string): PluginError {
  return new PluginError(ERROR_CODES.INVALID_LIMIT, message);
}

/** The pagination cursor is no longer valid for the current grant. */
export function cursorExpired(message: string): PluginError {
  return new PluginError(ERROR_CODES.CURSOR_EXPIRED, message);
}

/** The grant was revoked. */
export function revoked(message: string): PluginError {
  return new PluginError(ERROR_CODES.REVOKED, message);
}

/** The session is paused and the user has to resume it. */
export function paused(message: string): PluginError {
  return new PluginError(ERROR_CODES.PAUSED, message);
}

/** The session holds no write grant, so a write is refused. */
export function writeNotGranted(message: string): PluginError {
  return new PluginError(ERROR_CODES.WRITE_NOT_GRANTED, message);
}

/** The operation exceeded its deadline. */
export function timeout(message: string): PluginError {
  return new PluginError(ERROR_CODES.TIMEOUT, message);
}

/**
 * Convert any thrown value into a `PluginError`.
 *
 * Unknown throwables become `INTERNAL_ERROR` with a fixed message so no
 * stack trace or design content can reach the agent.
 */
export function toPluginError(error: unknown): PluginError {
  if (error instanceof PluginError) {
    return error;
  }
  if (error instanceof Error) {
    return internalError("The plugin hit an unexpected internal failure");
  }
  return internalError("The plugin hit an unexpected internal failure");
}
