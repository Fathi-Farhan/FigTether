/**
 * Sliced traversal engine.
 *
 * The engine walks only under the granted roots, works in slices of about
 * eight milliseconds and yields to the editor between slices, so a large
 * document never freezes the editor. The traversal is budgeted at ten
 * thousand visited nodes and never truncates silently: an overrun returns a
 * partial result with a cursor that resumes the walk.
 */

import type { FigmaNode } from "./figma-api.js";
import type { ScopeGuard } from "./scope.js";
import { collectAncestorIds, containsNode } from "./scope.js";
import type { CursorFrame } from "./cursor.js";

/** Maximum number of nodes visited by one operation. */
export const DEFAULT_MAX_NODES = 10000;

/** Maximum depth of one traversal. */
export const DEFAULT_MAX_DEPTH = 32;

/** Target duration of one traversal slice, in milliseconds. */
export const DEFAULT_SLICE_BUDGET_MS = 8;

/** Maximum number of node ids accepted in one batch. */
export const DEFAULT_BATCH_SIZE = 50;

/** One visited node with its position in the traversed scope. */
export interface TraversalEntry {
  /** The node. */
  node: FigmaNode;
  /** Parent id, when the parent is inside the scope. */
  parentId?: string;
  /** Depth of the node, the roots are at depth one. */
  depth: number;
}

/** Progress report emitted at every slice boundary. */
export interface TraversalProgress {
  /** Nodes visited so far. */
  visited: number;
  /** Milliseconds spent in the traversal so far. */
  elapsedMs: number;
}

/** Options of one traversal. */
export interface TraversalOptions {
  /** The guard that resolves nodes and enforces the grant. */
  guard: ScopeGuard;
  /** Root ids to start from. */
  rootIds: readonly string[];
  /** Maximum depth of the walk. */
  maxDepth?: number | undefined;
  /** Maximum number of nodes to visit. */
  maxNodes?: number | undefined;
  /** Target duration of one slice, in milliseconds. */
  sliceBudgetMs?: number | undefined;
  /** Stack of a resumed traversal. */
  startStack?: readonly CursorFrame[] | undefined;
  /** Nodes already visited by an earlier page. */
  visitedBefore?: number | undefined;
  /** Progress callback, invoked at every slice boundary. */
  onProgress?: ((progress: TraversalProgress) => void) | undefined;
  /** Cancellation check, invoked at every slice boundary. */
  isCancelled?: (() => boolean) | undefined;
  /** Clock, overridable for tests. */
  clock?: (() => number) | undefined;
  /** Yield function, overridable for tests. */
  yieldFn?: (() => Promise<void>) | undefined;
}

/** Result of one traversal. */
export interface TraversalResult {
  /** The visited nodes, in document order. */
  entries: TraversalEntry[];
  /** Nodes visited in this call, including the resumed prefix. */
  visited: number;
  /** Whether the node budget stopped the walk. */
  truncated: boolean;
  /** Whether the walk was cancelled. */
  cancelled: boolean;
  /** Whether the grant no longer covers the current position. */
  scopeLost: boolean;
  /** Stack of the walk, for the next page. */
  stack: CursorFrame[];
  /** Greatest depth reached. */
  depthReached: number;
}

/** Default clock, in milliseconds. */
export function defaultClock(): number {
  const performanceApi = (globalThis as { performance?: { now?: () => number } }).performance;
  if (performanceApi !== undefined && typeof performanceApi.now === "function") {
    return performanceApi.now();
  }
  return Date.now();
}

/** Default yield, which hands control back to the editor runtime. */
export function defaultYield(): Promise<void> {
  return new Promise<void>((resolve) => {
    setTimeout(resolve, 0);
  });
}

/**
 * Walk the granted roots.
 *
 * The traversal never changes the selection, the current page or the
 * visibility of a node.
 */
export async function traverse(options: TraversalOptions): Promise<TraversalResult> {
  const clock = options.clock ?? defaultClock;
  const yieldFn = options.yieldFn ?? defaultYield;
  const maxDepth = options.maxDepth ?? DEFAULT_MAX_DEPTH;
  const maxNodes = options.maxNodes ?? DEFAULT_MAX_NODES;
  const sliceBudgetMs = options.sliceBudgetMs ?? DEFAULT_SLICE_BUDGET_MS;

  const entries: TraversalEntry[] = [];
  const stack: Frame[] = [];
  let visited = options.visitedBefore ?? 0;
  let truncated = false;
  let cancelled = false;
  let scopeLost = false;
  let depthReached = 0;

  const startFrames =
    options.startStack ??
    options.rootIds.map((id): CursorFrame => ({ id, index: 0, visited: false, depth: 1 }));
  for (const frame of startFrames) {
    stack.push({
      id: frame.id,
      index: frame.index,
      visited: frame.visited,
      depth: frame.depth,
      parentId: stack.length > 0 ? stack[stack.length - 1]?.id : undefined,
    });
  }

  const startedAt = clock();
  let sliceStartedAt = startedAt;

  while (stack.length > 0) {
    if (options.isCancelled !== undefined && options.isCancelled()) {
      cancelled = true;
      break;
    }

    const now = clock();
    if (now - sliceStartedAt >= sliceBudgetMs) {
      await yieldFn();
      sliceStartedAt = clock();
      options.onProgress?.({
        visited,
        elapsedMs: clock() - startedAt,
      });
      const top = stack[stack.length - 1];
      if (top !== undefined && top.node !== undefined) {
        const ancestors = collectAncestorIds(top.node);
        const grant = options.guard.current;
        if (grant !== null && !containsNode(grant.rootIds, top.node.id, ancestors)) {
          scopeLost = true;
          break;
        }
      }
    }

    const frame = stack[stack.length - 1];
    if (frame === undefined) {
      break;
    }

    if (frame.node === undefined) {
      if (!frame.visited && visited >= maxNodes) {
        truncated = true;
        break;
      }
      let node: FigmaNode;
      try {
        node = await options.guard.resolveNode(frame.id);
      } catch {
        stack.pop();
        continue;
      }
      frame.node = node;
      if (!frame.visited) {
        visited += 1;
        frame.visited = true;
        const parent = frame.parentId;
        entries.push(
          parent === undefined
            ? { node, depth: frame.depth }
            : { node, parentId: parent, depth: frame.depth },
        );
        depthReached = Math.max(depthReached, frame.depth);
      }
      continue;
    }

    const children = frame.node.children ?? [];
    if (frame.index >= children.length) {
      stack.pop();
      continue;
    }

    const child = children[frame.index];
    frame.index += 1;
    if (child === undefined) {
      continue;
    }
    const childDepth = frame.depth + 1;
    if (childDepth > maxDepth) {
      continue;
    }
    stack.push({
      id: child.id,
      index: 0,
      visited: false,
      depth: childDepth,
      parentId: frame.id,
    });
  }

  const stackState: CursorFrame[] = stack.map((frame) => ({
    id: frame.id,
    index: frame.index,
    visited: frame.visited,
    depth: frame.depth,
  }));

  return {
    entries,
    visited,
    truncated,
    cancelled,
    scopeLost,
    stack: stackState,
    depthReached,
  };
}

interface Frame {
  id: string;
  index: number;
  visited: boolean;
  depth: number;
  parentId?: string | undefined;
  node?: FigmaNode | undefined;
}
