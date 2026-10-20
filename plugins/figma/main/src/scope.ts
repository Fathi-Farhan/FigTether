/**
 * Grant scope enforcement.
 *
 * The daemon checks the grant epoch, and the plugin checks it again. A node
 * may only be read when it is one of the granted roots or a descendant of
 * one of them. A node outside the grant is refused with `OUT_OF_SCOPE`
 * before any of its content is read.
 */

import { nodeNotFound, outOfScope, revoked } from "./errors.js";
import type { FigmaApi, FigmaNode } from "./figma-api.js";

/** The granted scope of the running session. */
export interface GrantState {
  /** Root node ids the user granted. */
  rootIds: string[];
  /** Epoch of the grant. */
  grantEpoch: number;
}

/** Maximum number of parent hops inspected while resolving membership. */
export const MAX_ANCESTOR_HOPS = 64;

/**
 * Whether a node is inside the granted scope.
 *
 * The ancestry is passed in so the decision itself stays a pure function.
 */
export function containsNode(
  rootIds: readonly string[],
  nodeId: string,
  ancestorIds: readonly string[],
): boolean {
  if (rootIds.length === 0) {
    return false;
  }
  if (rootIds.includes(nodeId)) {
    return true;
  }
  return ancestorIds.some((ancestorId) => rootIds.includes(ancestorId));
}

/**
 * Collect the ancestor ids of a node, closest ancestor first.
 *
 * Only the identity of the ancestors is read. The walk is bounded so a
 * malformed document cannot hang the sandbox.
 */
export function collectAncestorIds(node: FigmaNode, maxHops: number = MAX_ANCESTOR_HOPS): string[] {
  const ancestors: string[] = [];
  let current = node.parent;
  let hops = 0;
  while (current !== null && current !== undefined && hops < maxHops) {
    ancestors.push(current.id);
    current = current.parent;
    hops += 1;
  }
  return ancestors;
}

/**
 * Guards every read against the current grant.
 *
 * The guard owns the grant state, so a revoke or a grant update takes effect
 * immediately, including in the middle of a traversal.
 */
export class ScopeGuard {
  private grant: GrantState | null = null;

  constructor(private readonly api: FigmaApi) {}

  /** The grant in force, when the user granted one. */
  get current(): GrantState | null {
    return this.grant;
  }

  /** Install a new grant. */
  setGrant(grant: GrantState): void {
    this.grant = {
      rootIds: [...grant.rootIds],
      grantEpoch: grant.grantEpoch,
    };
  }

  /** Drop the grant, for example after a revoke. */
  clearGrant(): void {
    this.grant = null;
  }

  /**
   * The grant in force, or an error when no grant is active.
   *
   * A missing grant covers nothing, so every read is out of scope.
   */
  requireGrant(): GrantState {
    if (this.grant === null) {
      throw outOfScope("No access grant is active for this plugin session");
    }
    return this.grant;
  }

  /**
   * Look up a node by id.
   *
   * The lookup never changes the current page. The asynchronous lookup is
   * preferred because it can reach a node on another loaded page without
   * switching the editor view.
   */
  async resolveNode(nodeId: string): Promise<FigmaNode> {
    const direct = this.api.getNodeById?.(nodeId);
    if (direct !== undefined && direct !== null) {
      return direct;
    }
    if (this.api.getNodeByIdAsync !== undefined) {
      const loaded = await this.api.getNodeByIdAsync(nodeId);
      if (loaded !== null && loaded !== undefined) {
        return loaded;
      }
    }
    throw nodeNotFound("The requested node does not exist in this document");
  }

  /**
   * Resolve a node and prove that it is inside the grant.
   *
   * The membership check reads the ancestry only. The content of a node that
   * fails the check is never read.
   */
  async requireInScope(nodeId: string): Promise<FigmaNode> {
    const grant = this.requireGrant();
    const node = await this.resolveNode(nodeId);
    const ancestors = collectAncestorIds(node);
    if (!containsNode(grant.rootIds, node.id, ancestors)) {
      throw outOfScope("The requested node is outside the granted scope");
    }
    return node;
  }

  /** Whether a node id is one of the granted roots. */
  isRoot(nodeId: string): boolean {
    return this.grant !== null && this.grant.rootIds.includes(nodeId);
  }

  /** Refuse every read because the grant was revoked. */
  assertNotRevoked(): void {
    if (this.grant === null) {
      throw revoked("The grant was revoked; only the user can create a new one");
    }
  }
}
