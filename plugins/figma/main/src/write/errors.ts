/**
 * Failures of the write path.
 *
 * The write path has two levels of failure. The plugin taxonomy of
 * `src/errors.ts` is the level that travels on the wire, so it stays closed
 * and it never invents a code. The write codes and reasons of
 * `docs/11-write-path/01-write-path-design.md` section 2.6 are the level that
 * describes what went wrong, so a caller can react to a reason without
 * parsing a message.
 *
 * A `WritePlanError` carries both: the canonical code for the wire and the
 * structured detail for the caller.
 */

import {
  ERROR_ACTIONS,
  ERROR_CODES,
  PluginError,
  type ErrorAction,
  type ErrorCode,
} from "../errors.js";
import {
  WRITE_ERROR_CODES,
  WRITE_ERROR_REASONS,
  type WriteErrorCode,
  type WriteErrorDetail,
  type WriteErrorReason,
} from "./types.js";

/**
 * The canonical code every write code maps to.
 *
 * The write codes are codes of the Rust taxonomy of `crates/model/src/errors.rs`
 * as well, so they travel unchanged: the daemon reads them the way it reads
 * every other code.
 */
const CANONICAL_CODES: Readonly<Record<WriteErrorCode, ErrorCode>> = {
  WRITE_NOT_GRANTED: ERROR_CODES.WRITE_NOT_GRANTED,
  PLAN_INVALID: ERROR_CODES.PLAN_INVALID,
  PLAN_HASH_MISMATCH: ERROR_CODES.PLAN_HASH_MISMATCH,
  BUDGET_EXCEEDED: ERROR_CODES.BUDGET_EXCEEDED,
  DESTRUCTIVE_NOT_ALLOWED: ERROR_CODES.DESTRUCTIVE_NOT_ALLOWED,
  MUTATION_REFUSED: ERROR_CODES.MUTATION_REFUSED,
  FONT_UNAVAILABLE: ERROR_CODES.FONT_UNAVAILABLE,
  OUT_OF_SCOPE: ERROR_CODES.OUT_OF_SCOPE,
  NODE_NOT_FOUND: ERROR_CODES.NODE_NOT_FOUND,
  APPROVAL_REQUIRED: ERROR_CODES.APPROVAL_REQUIRED,
};

/** The recovery hint every write code maps to. */
const CANONICAL_ACTIONS: Readonly<Record<WriteErrorCode, ErrorAction>> = {
  WRITE_NOT_GRANTED: ERROR_ACTIONS.NEW_GRANT_REQUIRED,
  PLAN_INVALID: ERROR_ACTIONS.NARROW_PARAMETERS,
  PLAN_HASH_MISMATCH: ERROR_ACTIONS.NARROW_PARAMETERS,
  BUDGET_EXCEEDED: ERROR_ACTIONS.NARROW_PARAMETERS,
  DESTRUCTIVE_NOT_ALLOWED: ERROR_ACTIONS.RESUME_BY_USER,
  MUTATION_REFUSED: ERROR_ACTIONS.USE_ALTERNATIVE,
  FONT_UNAVAILABLE: ERROR_ACTIONS.USE_AVAILABLE_FONT,
  OUT_OF_SCOPE: ERROR_ACTIONS.WIDEN_GRANT,
  NODE_NOT_FOUND: ERROR_ACTIONS.REFRESH_TREE,
  APPROVAL_REQUIRED: ERROR_ACTIONS.RESUME_BY_USER,
};

/** Options of one structured failure. */
export interface WriteErrorOptions {
  /** The operation of the plan that failed. */
  operation?: number | null;
  /** Whether the caller may retry the same plan. */
  retryable?: boolean;
}

/**
 * Build one structured failure of the write path.
 *
 * The message never carries design content, so it can travel to the daemon
 * and to the user interface unchanged.
 */
export function writeError(
  code: WriteErrorCode,
  reason: WriteErrorReason,
  message: string,
  options?: WriteErrorOptions,
): WriteErrorDetail {
  return {
    code,
    reason,
    message,
    retryable: options?.retryable ?? false,
    action: CANONICAL_ACTIONS[code],
    operation: options?.operation ?? null,
  };
}

/** A plan failure that the plugin throws. */
export class WritePlanError extends PluginError {
  /** The structured detail of the failure. */
  readonly detail: WriteErrorDetail;
  /** Every reason the plan was refused, when there is more than one. */
  readonly reasons: readonly WriteErrorDetail[];

  constructor(detail: WriteErrorDetail, reasons?: readonly WriteErrorDetail[]) {
    super(CANONICAL_CODES[detail.code], detail.message, {
      action: CANONICAL_ACTIONS[detail.code],
      retryable: detail.retryable,
    });
    this.name = "WritePlanError";
    this.detail = detail;
    this.reasons = reasons ?? [detail];
  }
}

/** Build and throw a plan failure. */
export function fail(
  code: WriteErrorCode,
  reason: WriteErrorReason,
  message: string,
  options?: WriteErrorOptions,
): never {
  throw new WritePlanError(writeError(code, reason, message, options));
}

/**
 * Map a refusal of the editor to a structured failure.
 *
 * The Plugin API reports the refusal cases of a reparenting as plain errors,
 * so the reason is read from the message of the error. The message of the
 * editor never reaches the daemon: only the reason and a fixed text do.
 */
export function mapEditorError(error: unknown, operation: number | null): WriteErrorDetail {
  const message = error instanceof Error ? error.message : "";
  const lowered = message.toLowerCase();
  if (lowered.includes("font")) {
    return writeError(
      WRITE_ERROR_CODES.FONT_UNAVAILABLE,
      WRITE_ERROR_REASONS.FONT_UNAVAILABLE,
      "The editor could not load the font the text change needs",
      { operation },
    );
  }
  if (lowered.includes("component")) {
    return writeError(
      WRITE_ERROR_CODES.MUTATION_REFUSED,
      WRITE_ERROR_REASONS.COMPONENT_CYCLE,
      "The editor refused the move because it would create a component cycle",
      { operation, retryable: true },
    );
  }
  if (lowered.includes("descendant") || lowered.includes("into itself")) {
    return writeError(
      WRITE_ERROR_CODES.MUTATION_REFUSED,
      WRITE_ERROR_REASONS.PARENTING_CYCLE,
      "The editor refused the move because it would create a cycle",
      { operation, retryable: true },
    );
  }
  if (lowered.includes("read-only") || lowered.includes("readonly") || lowered.includes("locked")) {
    return writeError(
      WRITE_ERROR_CODES.MUTATION_REFUSED,
      WRITE_ERROR_REASONS.READONLY_NODE,
      "The editor refused the change because the node is read only",
      { operation, retryable: true },
    );
  }
  if (lowered.includes("instance")) {
    return writeError(
      WRITE_ERROR_CODES.MUTATION_REFUSED,
      WRITE_ERROR_REASONS.INSTANCE_CHILD,
      "The editor refused the change because the node lives inside an instance",
      { operation, retryable: true },
    );
  }
  if (lowered.includes("index") || lowered.includes("out of range")) {
    return writeError(
      WRITE_ERROR_CODES.MUTATION_REFUSED,
      WRITE_ERROR_REASONS.INDEX_OUT_OF_RANGE,
      "The editor refused the change because the index is outside the parent",
      { operation, retryable: true },
    );
  }
  return writeError(
    WRITE_ERROR_CODES.MUTATION_REFUSED,
    WRITE_ERROR_REASONS.EDITOR_REFUSED,
    "The editor refused the change",
    { operation, retryable: true },
  );
}

/**
 * Narrow a thrown value to the structured detail of a failure.
 *
 * A failure the plugin raised carries its detail. A failure the editor
 * raised is mapped to the reason of its message.
 */
export function toWriteErrorDetail(error: unknown, operation: number | null): WriteErrorDetail {
  if (error instanceof Error && "detail" in error) {
    const detail = (error as { detail?: unknown }).detail;
    if (typeof detail === "object" && detail !== null) {
      return detail as WriteErrorDetail;
    }
  }
  return mapEditorError(error, operation);
}
