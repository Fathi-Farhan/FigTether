/**
 * The write path of the user interface.
 *
 * The module is pure: it parses the preview and the result of a write plan,
 * counts the operations of a plan by kind and builds the decision of the user.
 * It never touches the editor and it never renders markup, so the whole write
 * path can be tested without a browser.
 *
 * The parser is tolerant on purpose. The preview travels from the sandbox
 * inside an envelope that may wrap it, and a field that is missing is shown
 * as missing instead of failing the whole preview. What the parser never does
 * is invent a count: a group that carries neither a list nor a number stays
 * empty.
 */

import type { WriteOperationKind } from "@figtether/shared";

/**
 * The kinds of operation a plan may carry.
 *
 * The list is the order the panel shows the operations in. The shared package
 * carries the type of the kinds but not the list itself.
 */
export const WRITE_OPERATION_KINDS: readonly WriteOperationKind[] = [
  "create_node",
  "insert_child",
  "set_properties",
  "set_text",
  "set_fills",
  "delete_node",
  "move_node",
];

/**
 * How deep the readers of a plan look into a payload.
 *
 * The payload of the daemon is untrusted data, so the readers stop after a
 * few levels instead of following a nesting of any depth.
 */
export const MAX_PLAN_DEPTH = 4;

/** The four groups of nodes a plan touches. */
export interface WriteCounts {
  /** Nodes the plan would create. */
  readonly create: number;
  /** Nodes whose properties the plan would change. */
  readonly change: number;
  /** Nodes the plan would reparent or reposition. */
  readonly move: number;
  /** Nodes the plan would remove. */
  readonly remove: number;
}

/** One kind of operation of a plan with the number of operations of that kind. */
export interface WriteOperationCount {
  /** The kind of the operation. */
  readonly kind: WriteOperationKind;
  /** How many operations of that kind the plan holds. */
  readonly count: number;
}

/** The preview of a plan, as the user interface shows it. */
export interface WritePreviewView {
  /** Id of the plan, minted by the agent. */
  readonly planId: string;
  /** Hash of the plan, the token of the approval. */
  readonly planHash: string;
  /** A short fingerprint of the hash, so the user can compare two plans. */
  readonly fingerprint: string;
  /** The approval the daemon issued for the plan, when it issued one. */
  readonly approvalId: string | null;
  /** The intent the agent supplied, when it supplied one. */
  readonly intent: string | null;
  /** Whether the plan can be applied as it is written. */
  readonly blocked: boolean;
  /** The reasons the plan is blocked. */
  readonly blockedReasons: readonly string[];
  /** The warnings the plugin reported for the plan. */
  readonly warnings: readonly string[];
  /** The number of nodes per group. */
  readonly counts: WriteCounts;
  /** The number of operations of the plan. */
  readonly operationsCount: number;
  /** The operations of the plan, counted by kind. */
  readonly operationCounts: readonly WriteOperationCount[];
}

/** The result of an applied plan, as the user interface shows it. */
export interface WriteResultView {
  /** Id of the plan that was applied. */
  readonly planId: string;
  /** Hash of the plan that was applied. */
  readonly planHash: string;
  /** Whether every operation of the plan ran. */
  readonly partial: boolean;
  /** The number of nodes per group. */
  readonly counts: WriteCounts;
  /** Whether the plugin committed the plan as one undo step. */
  readonly undoCommitted: boolean;
  /** The id of the version checkpoint, when one was written. */
  readonly versionSaved: string | null;
  /** The warnings the plugin reported for the apply. */
  readonly warnings: readonly string[];
}

/** The decision the user takes about a plan. */
export interface ApprovalDecision {
  /** The approval the decision answers. */
  readonly approval_id: string;
  /** The hash of the plan the user was shown. */
  readonly plan_hash: string;
  /** Whether the user approved the plan. */
  readonly approved: boolean;
  /** Why the user rejected the plan, when they said why. */
  readonly reason?: string;
}

/**
 * What the interface knows about a plan besides its preview.
 *
 * The preview of the plugin does not carry the intent of the plan and not the
 * kinds of its operations, so the interface reads them from the request of the
 * daemon that caused the preview.
 */
export interface WritePlanContext {
  /** The intent the agent supplied with the plan. */
  readonly intent: string | null;
  /** The operations of the plan, counted by kind. */
  readonly operationCounts: readonly WriteOperationCount[];
  /** The approval the daemon issued for the plan. */
  readonly approvalId: string | null;
}

/** An empty context, used when the interface knows nothing about the plan. */
export const EMPTY_PLAN_CONTEXT: WritePlanContext = {
  intent: null,
  operationCounts: [],
  approvalId: null,
};

/**
 * Parse the preview of a plan.
 *
 * The payload may be the preview itself, the preview under `preview` or under
 * `data`, so every shape the sandbox and the daemon use is accepted. The
 * function returns null when the payload carries no preview at all.
 */
export function readWritePreview(
  payload: unknown,
  context: WritePlanContext = EMPTY_PLAN_CONTEXT,
): WritePreviewView | null {
  const record = asRecord(payload);
  if (record === null) {
    return null;
  }
  const source = previewSource(record);
  if (source === null) {
    return null;
  }
  const warnings = readReasons(source["warnings"]);
  const reasons = readReasons(source["reasons"]);
  const planHash = readString(source["plan_hash"]) ?? "";
  const intent = context.intent ?? readString(source["intent"]);
  return {
    planId: readString(source["plan_id"]) ?? "",
    planHash,
    fingerprint: shortFingerprint(planHash),
    approvalId: readString(source["approval_id"]),
    intent,
    blocked: source["blocked"] === true,
    blockedReasons: reasons.length > 0 ? reasons : warnings,
    warnings,
    counts: {
      create: readNodeCount(source, "created"),
      change: readNodeCount(source, "changed"),
      move: readNodeCount(source, "moved"),
      remove: readNodeCount(source, "removed"),
    },
    operationsCount: readNumber(source["operations_count"]),
    operationCounts: context.operationCounts,
  };
}

/**
 * Parse the result of an applied plan.
 *
 * The payload may be the result itself, the result under `result` or under
 * `data`. The function returns null when the payload carries no result.
 */
export function readWriteResult(payload: unknown): WriteResultView | null {
  const record = asRecord(payload);
  if (record === null) {
    return null;
  }
  const source = resultSource(record);
  if (source === null) {
    return null;
  }
  const versionSaved = readString(source["version_saved"]);
  return {
    planId: readString(source["plan_id"]) ?? "",
    planHash: readString(source["plan_hash"]) ?? "",
    partial: source["status"] === "partial",
    counts: {
      create: readNodeCount(source, "created"),
      change: readNodeCount(source, "changed"),
      move: readNodeCount(source, "moved"),
      remove: readNodeCount(source, "removed"),
    },
    undoCommitted: source["undo_committed"] === true,
    versionSaved,
    warnings: readReasons(source["warnings"]),
  };
}

/**
 * Count the operations of a plan by kind.
 *
 * The kinds the contract does not know are ignored, so an operation of an
 * unknown kind never reaches the log and never reaches the panel.
 */
export function countOperations(operations: readonly unknown[]): readonly WriteOperationCount[] {
  const counts = new Map<WriteOperationKind, number>();
  for (const operation of operations) {
    const record = asRecord(operation);
    if (record === null) {
      continue;
    }
    const kind = readOperationKind(record["kind"] ?? record["op"] ?? record["type"]);
    if (kind === null) {
      continue;
    }
    counts.set(kind, (counts.get(kind) ?? 0) + 1);
  }
  const result: WriteOperationCount[] = [];
  for (const kind of WRITE_OPERATION_KINDS) {
    const count = counts.get(kind);
    if (count !== undefined) {
      result.push({ kind, count });
    }
  }
  return result;
}

/**
 * Build the decision of the user.
 *
 * The reason travels only when the user wrote one, so a rejection without a
 * reason carries no empty field.
 */
export function buildApprovalDecision(
  approvalId: string,
  planHash: string,
  approved: boolean,
  reason: string | null,
): ApprovalDecision {
  const trimmed = reason === null ? "" : reason.trim();
  return {
    approval_id: approvalId,
    plan_hash: planHash,
    approved,
    ...(trimmed.length === 0 ? {} : { reason: trimmed }),
  };
}

/**
 * The sentence that tells the user what the preview is.
 *
 * The sentence names the state in words, so the state never depends on the
 * color of the panel.
 */
export function previewHeadline(view: WritePreviewView): string {
  if (view.blocked) {
    return "The proposed change is blocked. It cannot be applied as it is written.";
  }
  return "A proposed change is waiting for your approval. Nothing has changed yet.";
}

/**
 * The sentences that report the result of an applied plan.
 *
 * The wording never promises a revert that FigTether does not provide: the
 * recovery path is the undo command of the editor.
 */
export function writeResultSummary(view: WriteResultView): string {
  const parts: string[] = [];
  parts.push(
    view.partial
      ? `Partially applied: ${view.counts.create} nodes created, ${view.counts.change} changed, ${view.counts.move} moved, ${view.counts.remove} removed.`
      : `Applied: ${view.counts.create} nodes created, ${view.counts.change} changed, ${view.counts.move} moved, ${view.counts.remove} removed.`,
  );
  parts.push(
    view.undoCommitted
      ? "The change is committed to the undo history of Figma as a single step."
      : "The change was not committed to the undo history of Figma.",
  );
  if (view.versionSaved !== null) {
    parts.push(`A version checkpoint was written: ${view.versionSaved}.`);
  }
  parts.push("If the change is not what you wanted, use the undo command of the editor.");
  return parts.join(" ");
}

/** The number of nodes a result touched. */
export function affectedNodes(counts: WriteCounts): number {
  return counts.create + counts.change + counts.move + counts.remove;
}

/** A short fingerprint of a plan hash. */
export function shortFingerprint(hash: string): string {
  const trimmed = hash.trim();
  if (trimmed.length === 0) {
    return "not available";
  }
  return trimmed.slice(0, 12);
}

/**
 * Read the approval id of a plan.
 *
 * The daemon issues the approval id when it previews a plan, so the id
 * travels with the request of the daemon. The reader accepts the shapes the
 * daemon may use: the id next to the plan, the id under `approval` and the id
 * under the parameters of the request.
 */
export function readApprovalId(payload: unknown, depth = 0): string | null {
  const record = asRecord(payload);
  if (record === null || depth > MAX_PLAN_DEPTH) {
    return null;
  }
  const direct = readString(record["approval_id"]);
  if (direct !== null) {
    return direct;
  }
  const approval = asRecord(record["approval"]);
  const nested = approval === null ? null : readString(approval["approval_id"]);
  if (nested !== null) {
    return nested;
  }
  const params = asRecord(record["params"]);
  if (params !== null) {
    return readApprovalId(params, depth + 1);
  }
  return null;
}

/**
 * Read the intent of a plan.
 *
 * The intent is free text the agent supplied. It is shown to the user and it
 * is never executed, so it never reaches the log.
 */
export function readPlanIntent(payload: unknown, depth = 0): string | null {
  const record = asRecord(payload);
  if (record === null || depth > MAX_PLAN_DEPTH) {
    return null;
  }
  const plan = asRecord(record["plan"]);
  if (plan !== null) {
    const intent = readString(plan["intent"]);
    if (intent !== null) {
      return intent;
    }
  }
  const params = asRecord(record["params"]);
  if (params !== null) {
    return readPlanIntent(params, depth + 1);
  }
  return readString(record["intent"]);
}

/**
 * Read the operations of a plan.
 *
 * The operations are only counted, never rendered, so the panel shows how many
 * operations of a kind the plan holds and not what they change.
 */
export function readPlanOperations(payload: unknown, depth = 0): readonly unknown[] {
  const record = asRecord(payload);
  if (record === null || depth > MAX_PLAN_DEPTH) {
    return [];
  }
  const plan = asRecord(record["plan"]);
  if (plan !== null && Array.isArray(plan["operations"])) {
    return plan["operations"];
  }
  const params = asRecord(record["params"]);
  if (params !== null) {
    return readPlanOperations(params, depth + 1);
  }
  if (Array.isArray(record["operations"])) {
    return record["operations"];
  }
  return [];
}

/** Find the preview inside a payload of the sandbox. */
function previewSource(record: Record<string, unknown>, depth = 0): Record<string, unknown> | null {
  if (depth > MAX_PLAN_DEPTH) {
    return null;
  }
  for (const key of ["data", "preview"]) {
    const nested = asRecord(record[key]);
    if (nested === null) {
      continue;
    }
    if (looksLikePreview(nested)) {
      return nested;
    }
    const deeper = previewSource(nested, depth + 1);
    if (deeper !== null) {
      return deeper;
    }
  }
  return looksLikePreview(record) ? record : null;
}

/** Find the result inside a payload of the sandbox. */
function resultSource(record: Record<string, unknown>, depth = 0): Record<string, unknown> | null {
  if (depth > MAX_PLAN_DEPTH) {
    return null;
  }
  for (const key of ["data", "result"]) {
    const nested = asRecord(record[key]);
    if (nested === null) {
      continue;
    }
    if (looksLikeResult(nested)) {
      return nested;
    }
    const deeper = resultSource(nested, depth + 1);
    if (deeper !== null) {
      return deeper;
    }
  }
  return looksLikeResult(record) ? record : null;
}

/**
 * Whether a record looks like the preview of a plan.
 *
 * A result of an applied plan carries the same plan id and the same hash, so
 * the undo commit is what tells the two apart.
 */
function looksLikePreview(record: Record<string, unknown>): boolean {
  if ("undo_committed" in record) {
    return false;
  }
  if (!("plan_id" in record || "plan_hash" in record)) {
    return false;
  }
  return (
    "blocked" in record ||
    "created" in record ||
    "operations_count" in record ||
    "warnings" in record
  );
}

/** Whether a record looks like the result of an applied plan. */
function looksLikeResult(record: Record<string, unknown>): boolean {
  return "undo_committed" in record || "version_saved" in record || "applied_operations" in record;
}

/**
 * Read a list of reasons.
 *
 * A reason may be a plain string or a structured failure of the plugin. The
 * reader keeps the message of the failure and never the whole object, so the
 * panel shows a sentence and not a blob of JSON.
 */
function readReasons(value: unknown): string[] {
  if (!Array.isArray(value)) {
    return [];
  }
  const reasons: string[] = [];
  for (const entry of value) {
    if (typeof entry === "string" && entry.trim().length > 0) {
      reasons.push(entry.trim());
      continue;
    }
    const record = asRecord(entry);
    if (record === null) {
      continue;
    }
    const message = readString(record["message"]);
    if (message !== null) {
      reasons.push(message);
      continue;
    }
    const reason = readString(record["reason"]);
    if (reason !== null) {
      reasons.push(reason);
    }
  }
  return reasons;
}

/**
 * Read the number of nodes of a group.
 *
 * The plugin reports the number of the nodes of a group. A preview that
 * reports the ids of the nodes instead is accepted as well.
 */
function readNodeCount(record: Record<string, unknown>, key: string): number {
  const count = record[`${key}_count`];
  if (typeof count === "number" && Number.isFinite(count)) {
    return Math.max(0, Math.trunc(count));
  }
  const list = record[key];
  if (Array.isArray(list)) {
    return list.length;
  }
  return 0;
}

/** Read a number, with a fallback of zero. */
function readNumber(value: unknown): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return 0;
  }
  return Math.max(0, Math.trunc(value));
}

/** Read a string, with a fallback of null. */
function readString(value: unknown): string | null {
  if (typeof value !== "string") {
    return null;
  }
  const trimmed = value.trim();
  return trimmed.length === 0 ? null : trimmed;
}

/** Read the kind of an operation, with a fallback of null. */
function readOperationKind(value: unknown): WriteOperationKind | null {
  if (typeof value !== "string") {
    return null;
  }
  const normalized = value.trim().toLowerCase();
  const known = WRITE_OPERATION_KINDS.find((kind): boolean => kind === normalized);
  return known ?? null;
}

/** Narrow a value to a plain record. */
function asRecord(value: unknown): Record<string, unknown> | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return null;
  }
  return value as Record<string, unknown>;
}
