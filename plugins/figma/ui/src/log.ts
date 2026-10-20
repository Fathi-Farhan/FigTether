/**
 * Sanitized activity log.
 *
 * A log line may carry an operation name, a count, a duration and an error
 * code. Nothing else. Layer names, text content, file identifiers, node
 * identifiers and credentials are structurally impossible to place in a line
 * because every part passes an allowlist before it is rendered.
 */

/** Operations the UI is allowed to name in a log line. */
export const LOG_OPERATIONS: readonly string[] = [
  "pair",
  "reauth",
  "reconnect",
  "heartbeat",
  "grant_update",
  "pause",
  "resume",
  "revoke",
  "capture",
  "read",
  "list_nodes",
  "inspect",
  "export",
  "other",
];

/** Error codes of the catalog, used to normalize unknown codes. */
export const ERROR_CODES: readonly string[] = [
  "AUTH_REQUIRED",
  "AUTH_EXPIRED",
  "PERMISSION_DENIED",
  "OUT_OF_SCOPE",
  "PLUGIN_OFFLINE",
  "SESSION_NOT_FOUND",
  "PAUSED",
  "REVOKED",
  "NODE_NOT_FOUND",
  "CAPABILITY_UNAVAILABLE",
  "PAYLOAD_TOO_LARGE",
  "SCOPE_TOO_LARGE",
  "BUSY",
  "TIMEOUT",
  "CANCELLED",
  "DOCUMENT_CHANGED",
  "CURSOR_EXPIRED",
  "RESYNC_REQUIRED",
  "UNSUPPORTED_SCHEMA",
  "PROTOCOL_MISMATCH",
  "EXPORT_FAILED",
  "FONT_UNAVAILABLE",
  "SESSION_LIMIT",
  "INVALID_LIMIT",
  "INTERNAL_ERROR",
];

/** One sanitized log entry. */
export interface LogEntry {
  readonly operation: string;
  readonly count?: number | null;
  readonly durationMs?: number | null;
  readonly errorCode?: string | null;
}

/** Fallback operation name for an operation outside the allowlist. */
export const FALLBACK_OPERATION = "other";

/** Fallback error code for a code outside the allowlist. */
export const FALLBACK_ERROR_CODE = "INTERNAL_ERROR";

/** Map an arbitrary operation name onto the allowlist. */
export function sanitizeOperation(operation: string): string {
  const normalized = operation.trim().toLowerCase();
  return LOG_OPERATIONS.includes(normalized) ? normalized : FALLBACK_OPERATION;
}

/** Map an arbitrary error code onto the catalog, unknown codes become INTERNAL_ERROR. */
export function sanitizeErrorCode(code: string | null | undefined): string | null {
  if (typeof code !== "string") {
    return null;
  }
  const normalized = code.trim().toUpperCase();
  if (normalized.length === 0) {
    return null;
  }
  return ERROR_CODES.includes(normalized) ? normalized : FALLBACK_ERROR_CODE;
}

function sanitizeCount(count: number | null | undefined): number | null {
  if (typeof count !== "number" || !Number.isFinite(count)) {
    return null;
  }
  return Math.max(0, Math.trunc(count));
}

function sanitizeDuration(durationMs: number | null | undefined): number | null {
  if (typeof durationMs !== "number" || !Number.isFinite(durationMs)) {
    return null;
  }
  return Math.max(0, Math.trunc(durationMs));
}

/**
 * Format a log entry.
 *
 * The format is fixed: `op=read count=12 duration=340ms error=BUSY`. Parts that
 * are absent are omitted, and a part that does not pass its allowlist is
 * dropped or replaced by a neutral value.
 */
export function formatLogEntry(entry: LogEntry): string {
  const parts: string[] = [`op=${sanitizeOperation(entry.operation)}`];
  const count = sanitizeCount(entry.count);
  if (count !== null) {
    parts.push(`count=${count}`);
  }
  const duration = sanitizeDuration(entry.durationMs);
  if (duration !== null) {
    parts.push(`duration=${duration}ms`);
  }
  const errorCode = sanitizeErrorCode(entry.errorCode);
  if (errorCode !== null) {
    parts.push(`error=${errorCode}`);
  }
  return parts.join(" ");
}

/** Format a log entry with a leading wall clock label. */
export function formatLogLine(entry: LogEntry, now: number): string {
  const date = new Date(now);
  const hours = String(date.getHours()).padStart(2, "0");
  const minutes = String(date.getMinutes()).padStart(2, "0");
  const seconds = String(date.getSeconds()).padStart(2, "0");
  return `${hours}:${minutes}:${seconds} ${formatLogEntry(entry)}`;
}
