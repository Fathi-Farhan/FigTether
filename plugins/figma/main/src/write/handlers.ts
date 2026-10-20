/**
 * The handlers of the write path.
 *
 * The two handlers are the only entry points of the write path: a preview and
 * an apply. Both of them resolve the plan against the live document, so the
 * preview is exactly what the apply would do.
 */

import type { FigmaNode } from "../figma-api.js";
import { collectAncestorIds, containsNode } from "../scope.js";
import type { HandlerContext } from "../handlers.js";
import { applyResolvedPlan } from "./apply.js";
import { parseApprovalToken, sameHash } from "./approval.js";
import { writeError, WritePlanError } from "./errors.js";
import { parseWritePlan, planHash } from "./plan.js";
import { resolvePlan, type WriteScope } from "./resolver.js";
import {
  WRITE_ERROR_CODES,
  WRITE_ERROR_REASONS,
  type ParsedWritePlan,
  type WritePlan,
  type WritePreview,
  type WriteResult,
} from "./types.js";

/**
 * Handle `preview_write`.
 *
 * The handler never mutates the document. It reports what the plan would do
 * and whether the plan can be applied as written.
 */
export async function handlePreviewWrite(
  params: unknown,
  context: HandlerContext,
): Promise<WritePreview> {
  const parsed = parseWritePlan(params);
  assertSession(parsed.plan, context);
  assertPlanHash(parsed, null);
  const scope = scopeOf(context);
  const resolved = await resolvePlan(parsed.plan, context.api, scope, {
    mode: "preview",
    approvalId: parsed.approvalId,
  });
  return resolved.preview;
}

/**
 * Handle `apply_write`.
 *
 * The handler applies the plan and reports what happened: the ids of the
 * created nodes, the nodes that changed, the nodes that were removed, the
 * nodes that moved, whether the undo history was committed and whether a
 * version checkpoint was written.
 */
export async function handleApplyWrite(
  params: unknown,
  context: HandlerContext,
): Promise<WriteResult> {
  const parsed = parseWritePlan(params);
  assertSession(parsed.plan, context);
  const token = parseApprovalToken(readApprovalToken(params));
  assertPlanHash(parsed, token === null ? null : token.plan_hash);
  const scope = scopeOf(context);
  const resolved = await resolvePlan(parsed.plan, context.api, scope, { mode: "apply" });
  if (resolved.preview.blocked) {
    const first = resolved.reasons[0];
    if (first !== undefined) {
      throw new WritePlanError(first, resolved.reasons);
    }
    throw new WritePlanError(
      writeError(
        WRITE_ERROR_CODES.PLAN_INVALID,
        WRITE_ERROR_REASONS.INVALID_PLAN,
        "The plan cannot be applied as written",
      ),
    );
  }

  const saveVersion = readSaveVersion(params);
  const outcome = await applyResolvedPlan(resolved, context.api, scope, {
    saveVersion,
    versionTitle: `FigTether: ${parsed.plan.plan_id}`,
  });

  const versionAvailable = context.api.saveVersionHistoryAsync !== undefined;
  const versionNote =
    outcome.version_saved !== null
      ? null
      : !saveVersion
        ? null
        : versionAvailable
          ? "the editor wrote no version checkpoint"
          : "the editor does not expose the version history of this file";

  const result: WriteResult = {
    plan_id: parsed.plan.plan_id,
    plan_hash: resolved.preview.plan_hash,
    approval_id: readApprovalId(params) ?? token?.approval_id ?? "",
    status: outcome.status,
    created: outcome.created,
    changed: outcome.changed,
    removed: outcome.removed,
    moved: outcome.moved,
    operations_count: resolved.preview.operations_count,
    applied_operations: outcome.applied_operations,
    undo_committed: outcome.undo_committed,
    version_requested: saveVersion,
    version_available: versionAvailable,
    version_note: versionNote,
    warnings: [...resolved.preview.warnings, ...outcome.warnings],
    error: outcome.error,
  };
  if (outcome.version_saved !== null) {
    result.version_saved = outcome.version_saved;
  }
  return result;
}

/**
 * Refuse a plan that changed on the way to the plugin.
 *
 * The daemon hashes the plan before it sends it, and the plugin hashes the
 * plan it received. A plan that hashes differently is not the plan the user
 * was shown, so it is refused instead of previewed or applied.
 *
 * The hash of the daemon travels in two places: next to the plan and inside
 * the approval token of an apply. Both of them are checked, so a plan that
 * changed on the way is refused in every flow.
 */
function assertPlanHash(parsed: ParsedWritePlan, tokenHash: string | null): void {
  const hashes = [parsed.planHash, tokenHash].filter(
    (hash): hash is string => hash !== null && hash.length > 0,
  );
  if (hashes.length === 0) {
    return;
  }
  const local = planHash(parsed.plan);
  for (const hash of hashes) {
    if (!sameHash(local, hash)) {
      throw new WritePlanError(
        writeError(
          WRITE_ERROR_CODES.PLAN_HASH_MISMATCH,
          WRITE_ERROR_REASONS.HASH_MISMATCH,
          "The plan changed on the way to the plugin, so it is not the plan the user approved",
        ),
      );
    }
  }
}

/**
 * Build the scope of the write path from the grant of the session.
 *
 * The scope is the same one the read path enforces, so a write can never
 * reach a node a read could not reach.
 */
export function scopeOf(context: HandlerContext): WriteScope {
  const grant = context.guard.requireGrant();
  return {
    rootIds: [...grant.rootIds],
    grantEpoch: grant.grantEpoch,
    resolveNode: async (nodeId: string): Promise<FigmaNode> =>
      await context.guard.resolveNode(nodeId),
    isInScope: (node: FigmaNode): boolean =>
      containsNode(grant.rootIds, node.id, collectAncestorIds(node)),
  };
}

/**
 * Refuse a plan that targets another session.
 *
 * A plan is bound to the session it was made for, so a plan of another
 * session never runs. A plan that names no session is not bound to one, so
 * it runs under the session that asked for it.
 */
function assertSession(plan: WritePlan, context: HandlerContext): void {
  if (plan.session_id === undefined || context.sessionId === undefined) {
    return;
  }
  if (plan.session_id !== context.sessionId) {
    throw new WritePlanError(
      writeError(
        WRITE_ERROR_CODES.PLAN_INVALID,
        WRITE_ERROR_REASONS.INVALID_PLAN,
        "The plan targets another session",
      ),
    );
  }
}

/** Read the version option of an apply request. */
function readSaveVersion(params: unknown): boolean {
  if (typeof params !== "object" || params === null || Array.isArray(params)) {
    return false;
  }
  const record = params as Record<string, unknown>;
  return record["save_version"] === true;
}

/** Read the approval id of an apply request. */
function readApprovalId(params: unknown): string | null {
  if (typeof params !== "object" || params === null || Array.isArray(params)) {
    return null;
  }
  const record = params as Record<string, unknown>;
  const approval = record["approval_id"];
  return typeof approval === "string" ? approval : null;
}

/** Read the approval token of an apply request. */
function readApprovalToken(params: unknown): string {
  if (typeof params !== "object" || params === null || Array.isArray(params)) {
    return "";
  }
  const record = params as Record<string, unknown>;
  const token = record["approval_token"];
  return typeof token === "string" ? token : "";
}
