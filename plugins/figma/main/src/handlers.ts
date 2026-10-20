/**
 * Operation handlers of the plugin main sandbox.
 *
 * Every handler is read only. A handler never creates, modifies, deletes,
 * moves or resizes anything, and never changes the current selection or the
 * current page as a side effect of a read.
 */

import {
  exportPixelCount,
  MAX_EXPORT_SCALE,
  MIN_EXPORT_SCALE,
  withinExportPixelBudget,
  withinExportScaleRange,
} from "./bytes.js";
import { base64Encode } from "./encoding.js";
import { contentHash } from "./canonical.js";
import {
  capabilityUnavailable,
  cursorExpired,
  exportFailed,
  fontUnavailable,
  invalidLimit,
  outOfScope,
  payloadTooLarge,
  PluginError,
  toPluginError,
  ERROR_CODES,
} from "./errors.js";
import type { CapabilityReport } from "./capabilities.js";
import type { FigmaApi, FigmaExportSettings, FigmaNode } from "./figma-api.js";
import { asNumber, asString, asArray } from "./figma-api.js";
import type { FontCatalog } from "./normalize.js";
import { normalizeNode } from "./normalize.js";
import {
  DEFAULT_BATCH_SIZE,
  DEFAULT_MAX_DEPTH,
  DEFAULT_MAX_NODES,
  traverse,
  type TraversalEntry,
  type TraversalProgress,
} from "./traversal.js";
import type { ScopeGuard } from "./scope.js";
import type { TokenResolver } from "./tokens.js";
import { decodeCursor, encodeCursor, type CursorFrame } from "./cursor.js";
import {
  normalizeFieldList,
  projectNode,
  DEFAULT_SUMMARY_FIELDS,
  type NodeField,
} from "./projection.js";
import { assertJsonValue, sha256Hex } from "@figtether/shared";
import type {
  BridgeOp,
  CapabilityState,
  PluginDocument,
  PluginExport,
  PluginNode,
  PluginVariable,
} from "./types.js";

/** Coverage summary attached to every read. */
export interface Coverage {
  /** Union of the unavailable fields of the returned nodes. */
  unavailable_fields: string[];
  /** Verbatim node types the plugin does not know. */
  unsupported_types: string[];
  /** Nodes visited by the operation. */
  visited: number;
}

/** Everything a handler needs besides its parameters. */
export interface HandlerContext {
  /** Correlation id of the request. */
  requestId: string;
  /** Session the request belongs to. */
  sessionId?: string | undefined;
  /** Grant epoch the request was issued under. */
  grantEpoch?: number | undefined;
  /** Deadline of the request, in milliseconds since the epoch. */
  deadlineMs?: number | undefined;
  /** The editor API. */
  api: FigmaApi;
  /** Grant enforcement. */
  guard: ScopeGuard;
  /** Variable and style resolution. */
  resolver: TokenResolver;
  /** Font availability lookup. */
  fontCatalog: FontCatalog;
  /** Capabilities probed for this session. */
  capabilities: CapabilityReport;
  /** Cancellation check. */
  isCancelled: () => boolean;
  /** Progress reporter. */
  onProgress: (progress: TraversalProgress) => void;
}

/** One operation handler. */
export type Handler = (params: unknown, context: HandlerContext) => Promise<unknown>;

/** Registry of the allowlisted operations. */
export class HandlerRegistry {
  private readonly handlers = new Map<BridgeOp, Handler>();

  /** Register one operation. */
  register(op: BridgeOp, handler: Handler): void {
    this.handlers.set(op, handler);
  }

  /** Look up one operation. */
  get(op: BridgeOp): Handler | undefined {
    return this.handlers.get(op);
  }

  /** Names of the registered operations. */
  ops(): BridgeOp[] {
    return [...this.handlers.keys()];
  }
}

const KNOWN_NODE_TYPES = new Set([
  "DOCUMENT",
  "PAGE",
  "CANVAS",
  "FRAME",
  "GROUP",
  "SECTION",
  "COMPONENT",
  "COMPONENT_SET",
  "INSTANCE",
  "TEXT",
  "RECTANGLE",
  "ELLIPSE",
  "LINE",
  "POLYGON",
  "STAR",
  "VECTOR",
  "BOOLEAN_OPERATION",
  "SLICE",
  "FRAME_NODE",
  "ROUNDED_RECTANGLE",
  "STICKY",
  "SHAPE_WITH_TEXT",
  "CONNECTOR",
  "HIGHLIGHT",
  "CODE_BLOCK",
  "WASHI_TAPE",
  "EMBEDDED_PROTO",
  "TABLE",
  "SLIDE",
  "SLIDE_ROW",
  "SLIDE_GRID",
  "MEDIA",
  "WIDGET",
]);

/** Build the registry with every allowlisted operation. */
export function createHandlerRegistry(): HandlerRegistry {
  const registry = new HandlerRegistry();
  registry.register("list_nodes", handleListNodes);
  registry.register("read_nodes", handleReadNodes);
  registry.register("inspect_design", handleInspectDesign);
  registry.register("get_design_tokens", handleGetDesignTokens);
  registry.register("export_assets", handleExportAssets);
  registry.register("get_selection", handleGetSelection);
  registry.register("capture_snapshot", handleCaptureSnapshot);
  registry.register("list_sessions", handleListSessions);
  registry.register("watch_scope", handleWatchScope);
  registry.register("get_changes", handleGetChanges);
  return registry;
}

export interface ListNodesResult {
  root_id: string;
  nodes: PluginNode[];
  next_cursor: string | null;
  truncated: boolean;
  visited: number;
  coverage: Coverage;
}

export interface ReadNodesResult {
  batch_status: "complete" | "partial" | "failed";
  nodes: PluginNode[];
  items: Array<
    | { node_id: string; status: "ok"; node: PluginNode }
    | {
        node_id: string;
        status: "error";
        error: { code: string; message: string; retryable: boolean; action: string };
      }
  >;
  summary: { total: number; ok: number; error: number };
  next_cursor: string | null;
  truncated: boolean;
  coverage: Coverage;
}

export interface InspectDesignResult {
  root_id: string;
  detail: string;
  nodes: PluginNode[];
  unsupported: Array<{ node_id: string; node_type: string }>;
  next_cursor: string | null;
  truncated: boolean;
  visited: number;
  coverage: Coverage;
}

export interface DesignTokensResult {
  root_id: string;
  variables: PluginVariable[];
  styles: PluginVariable[];
  collections: Array<{ id: string; name: string; modes: string[] }>;
  unresolved: Array<{ id: string; reason: string }>;
  coverage: Coverage;
}

export interface ExportAssetsResult {
  batch_status: "complete" | "partial" | "failed";
  items: Array<
    | {
        node_id: string;
        status: "ok";
        export: PluginExport;
      }
    | {
        node_id: string;
        status: "error";
        error: { code: string; message: string; retryable: boolean; action: string };
      }
  >;
  summary: { total: number; ok: number; error: number };
}

export interface SelectionResult {
  nodes: PluginNode[];
  selection_count: number;
  coverage: Coverage;
}

export interface SnapshotResult {
  document: PluginDocument;
  content_hash: string;
  consistency: "best_effort";
  started_at: string;
  observed_at: string;
  truncated: boolean;
  next_cursor: string | null;
  visited: number;
  coverage: Coverage;
}

export interface SessionsResult {
  sessions: Array<{
    session_id: string;
    grant_epoch: number;
    root_ids: string[];
    status: string;
    capabilities: Record<string, CapabilityState>;
  }>;
}

/** Handle `list_nodes`: the children summary of a granted root. */
export async function handleListNodes(
  params: unknown,
  context: HandlerContext,
): Promise<ListNodesResult> {
  const record = asParams(params);
  const rootId = requiredString(record, "root_id");
  const filters = asRecordValue(record["filters"]);
  const maxDepth = readDepth(filters?.["max_depth"]);
  const types = readStringArray(filters?.["types"]);
  const projection = normalizeFieldList(record["fields"]) ?? [...DEFAULT_SUMMARY_FIELDS];
  const cursorToken = optionalString(record, "cursor");

  const grant = context.guard.requireGrant();
  const root = await context.guard.requireInScope(rootId);

  let startStack;
  let visitedBefore = 0;
  if (cursorToken !== undefined) {
    const cursor = decodeCursor(cursorToken, grant.grantEpoch);
    if (cursor.root_id !== root.id) {
      throw cursorExpired("The pagination cursor belongs to another root of the grant");
    }
    startStack = cursor.stack;
    visitedBefore = cursor.visited;
  }

  const result = await traverse({
    guard: context.guard,
    rootIds: [root.id],
    maxDepth,
    maxNodes: DEFAULT_MAX_NODES,
    startStack,
    visitedBefore,
    onProgress: context.onProgress,
    isCancelled: context.isCancelled,
  });
  assertTraversalUsable(result.cancelled, result.scopeLost);

  const filtered =
    types === undefined
      ? result.entries
      : result.entries.filter((entry) => types.includes(entry.node.type));
  const nodes = await normalizeEntries(filtered, context, projection);
  const coverage = coverageOf(nodes, result.visited);

  return {
    root_id: root.id,
    nodes,
    next_cursor: nextCursorFor(result, grant.grantEpoch, root.id),
    truncated: result.truncated,
    visited: result.visited,
    coverage,
  };
}

/** Handle `read_nodes`: node data with depth and field projection. */
export async function handleReadNodes(
  params: unknown,
  context: HandlerContext,
): Promise<ReadNodesResult> {
  const record = asParams(params);
  const nodeIds = requiredStringArray(record, "node_ids");
  if (nodeIds.length > DEFAULT_BATCH_SIZE) {
    throw invalidLimit(`A batch accepts at most ${DEFAULT_BATCH_SIZE} node ids`);
  }
  const depth = readDepth(record["depth"]) ?? 1;
  const projection = normalizeFieldList(record["fields"]);

  const grant = context.guard.requireGrant();
  const items: ReadNodesResult["items"] = [];
  const allNodes: PluginNode[] = [];
  let visited = 0;
  let truncated = false;
  let nextCursor: string | null = null;

  for (const nodeId of nodeIds) {
    let node: FigmaNode;
    try {
      node = await context.guard.requireInScope(nodeId);
    } catch (error) {
      const pluginError = toPluginError(error);
      items.push({
        node_id: nodeId,
        status: "error",
        error: {
          code: pluginError.code,
          message: pluginError.message,
          retryable: pluginError.retryable,
          action: pluginError.action,
        },
      });
      continue;
    }

    const result = await traverse({
      guard: context.guard,
      rootIds: [node.id],
      maxDepth: depth,
      maxNodes: DEFAULT_MAX_NODES,
      onProgress: context.onProgress,
      isCancelled: context.isCancelled,
    });
    assertTraversalUsable(result.cancelled, result.scopeLost);
    visited += result.visited;
    truncated = truncated || result.truncated;
    if (result.truncated) {
      nextCursor = nextCursorFor(result, grant.grantEpoch, node.id);
    }
    const normalized = await normalizeEntries(result.entries, context, projection);
    allNodes.push(...normalized);
    items.push({
      node_id: nodeId,
      status: "ok",
      node: normalized[0] ?? emptyNode(nodeId),
    });
  }

  const errorCount = items.filter((item) => item.status === "error").length;
  const okCount = items.length - errorCount;
  const batchStatus: ReadNodesResult["batch_status"] =
    errorCount === 0 ? "complete" : okCount === 0 ? "failed" : "partial";

  return {
    batch_status: batchStatus,
    nodes: allNodes,
    items,
    summary: { total: items.length, ok: okCount, error: errorCount },
    next_cursor: nextCursor,
    truncated,
    coverage: coverageOf(allNodes, visited),
  };
}

/** Handle `inspect_design`: the full implementation context of a subtree. */
export async function handleInspectDesign(
  params: unknown,
  context: HandlerContext,
): Promise<InspectDesignResult> {
  const record = asParams(params);
  const rootId = requiredString(record, "root_id");
  const detail = optionalString(record, "detail") ?? "full";
  const projection = normalizeFieldList(record["fields"]) ?? projectionForDetail(detail);

  const grant = context.guard.requireGrant();
  const root = await context.guard.requireInScope(rootId);

  const result = await traverse({
    guard: context.guard,
    rootIds: [root.id],
    maxDepth: readDepth(record["depth"]) ?? DEFAULT_MAX_DEPTH,
    maxNodes: DEFAULT_MAX_NODES,
    onProgress: context.onProgress,
    isCancelled: context.isCancelled,
  });
  assertTraversalUsable(result.cancelled, result.scopeLost);

  const nodes = await normalizeEntries(result.entries, context, projection);
  const unsupported = result.entries
    .filter((entry) => !KNOWN_NODE_TYPES.has(entry.node.type))
    .map((entry) => ({ node_id: entry.node.id, node_type: entry.node.type }));

  return {
    root_id: root.id,
    detail,
    nodes,
    unsupported,
    next_cursor: nextCursorFor(result, grant.grantEpoch, root.id),
    truncated: result.truncated,
    visited: result.visited,
    coverage: coverageOf(nodes, result.visited),
  };
}

/** Handle `get_design_tokens`: variables and styles in scope. */
export async function handleGetDesignTokens(
  params: unknown,
  context: HandlerContext,
): Promise<DesignTokensResult> {
  const record = asParams(params);
  const rootId = requiredString(record, "root_id");
  const kinds = readStringArray(record["requested_kinds"]) ?? [
    "color_styles",
    "text_styles",
    "effect_styles",
    "variables",
  ];

  const root = await context.guard.requireInScope(rootId);
  const result = await traverse({
    guard: context.guard,
    rootIds: [root.id],
    maxDepth: readDepth(record["depth"]) ?? DEFAULT_MAX_DEPTH,
    maxNodes: DEFAULT_MAX_NODES,
    onProgress: context.onProgress,
    isCancelled: context.isCancelled,
  });
  assertTraversalUsable(result.cancelled, result.scopeLost);

  const variableIds = new Set<string>();
  const styleIds = new Set<string>();
  for (const entry of result.entries) {
    collectBoundIds(entry.node, variableIds, styleIds);
  }

  const wantsVariables = kinds.includes("variables");
  const wantsStyles =
    kinds.includes("color_styles") ||
    kinds.includes("text_styles") ||
    kinds.includes("effect_styles") ||
    kinds.includes("grid_styles");

  const variablesAvailable = context.resolver.variablesAvailable;
  const stylesAvailable = context.resolver.stylesAvailable;

  const unavailableFields: string[] = [];
  if (wantsVariables && !variablesAvailable) {
    unavailableFields.push("variables:capability_unavailable");
  }
  if (wantsStyles && !stylesAvailable) {
    unavailableFields.push("styles:capability_unavailable");
  }
  if (wantsVariables && !variablesAvailable && wantsStyles && !stylesAvailable) {
    throw capabilityUnavailable(
      "Neither the variables API nor the local style API is exposed by this editor or seat",
    );
  }

  const variables: PluginVariable[] = [];
  if (wantsVariables && variablesAvailable) {
    for (const id of variableIds) {
      variables.push(await context.resolver.resolveVariable(id));
    }
  }
  const styles: PluginVariable[] = [];
  if (wantsStyles && stylesAvailable) {
    for (const id of styleIds) {
      styles.push(await context.resolver.resolveStyle(id));
    }
  }

  const collections = (await context.resolver.listCollections()).map((collection) => ({
    id: collection.id,
    name: collection.name,
    modes: collection.modes.map((mode) => mode.name),
  }));

  const unresolved = [...variables, ...styles]
    .filter((reference) => !reference.resolved)
    .map((reference) => ({
      id: reference.id,
      reason: reference.unresolved_reason ?? "the reference could not be resolved",
    }));

  return {
    root_id: root.id,
    variables,
    styles,
    collections,
    unresolved,
    coverage: {
      unavailable_fields: unavailableFields,
      unsupported_types: [],
      visited: result.visited,
    },
  };
}

/** Handle `export_assets`: PNG and SVG exports of granted nodes. */
export async function handleExportAssets(
  params: unknown,
  context: HandlerContext,
): Promise<ExportAssetsResult> {
  const record = asParams(params);
  const nodeIds = requiredStringArray(record, "node_ids");
  const format = (optionalString(record, "format") ?? "png").toLowerCase();
  if (format !== "png" && format !== "svg") {
    throw invalidLimit("Only the PNG and SVG export formats are supported");
  }
  const scale = asNumber(record["scale"]) ?? 1;
  if (!withinExportScaleRange(scale)) {
    throw invalidLimit(
      `The export scale must be between ${MIN_EXPORT_SCALE} and ${MAX_EXPORT_SCALE}`,
    );
  }

  const items: ExportAssetsResult["items"] = [];
  for (const nodeId of nodeIds) {
    let node: FigmaNode;
    try {
      node = await context.guard.requireInScope(nodeId);
    } catch (error) {
      const pluginError = toPluginError(error);
      items.push({
        node_id: nodeId,
        status: "error",
        error: {
          code: pluginError.code,
          message: pluginError.message,
          retryable: pluginError.retryable,
          action: pluginError.action,
        },
      });
      continue;
    }

    const width = node.width ?? 0;
    const height = node.height ?? 0;
    if (!withinExportPixelBudget(width, height, scale)) {
      items.push({
        node_id: nodeId,
        status: "error",
        error: {
          code: payloadTooLarge("The export exceeds the 16 megapixel budget").code,
          message: "The export exceeds the 16 megapixel budget",
          retryable: false,
          action: "narrow_parameters",
        },
      });
      continue;
    }

    try {
      const exported = await runExport(node, format, scale);
      items.push({ node_id: nodeId, status: "ok", export: exported });
    } catch (error) {
      const pluginError = toPluginError(error);
      items.push({
        node_id: nodeId,
        status: "error",
        error: {
          code: pluginError.code,
          message: pluginError.message,
          retryable: pluginError.retryable,
          action: pluginError.action,
        },
      });
    }
  }

  const errorCount = items.filter((item) => item.status === "error").length;
  const okCount = items.length - errorCount;
  return {
    batch_status: errorCount === 0 ? "complete" : okCount === 0 ? "failed" : "partial",
    items,
    summary: { total: items.length, ok: okCount, error: errorCount },
  };
}

/** Handle `get_selection`: the selection bounded by the grant. */
export async function handleGetSelection(
  _params: unknown,
  context: HandlerContext,
): Promise<SelectionResult> {
  const grant = context.guard.requireGrant();
  const selection = context.api.currentPage.selection ?? [];
  const nodes: PluginNode[] = [];
  for (const node of selection) {
    if (!isInsideGrant(grant.rootIds, node)) {
      continue;
    }
    const normalized = await normalizeNode(node, {
      api: context.api,
      resolver: context.resolver,
      fontCatalog: context.fontCatalog,
      childIds: (node.children ?? []).map((child) => child.id),
    });
    nodes.push(normalized);
  }
  return {
    nodes,
    selection_count: nodes.length,
    coverage: coverageOf(nodes, nodes.length),
  };
}

/** Handle `capture_snapshot`: an immutable capture of the granted scope. */
export async function handleCaptureSnapshot(
  _params: unknown,
  context: HandlerContext,
): Promise<SnapshotResult> {
  const grant = context.guard.requireGrant();
  const startedAt = new Date().toISOString();

  const result = await traverse({
    guard: context.guard,
    rootIds: grant.rootIds,
    maxDepth: DEFAULT_MAX_DEPTH,
    maxNodes: DEFAULT_MAX_NODES,
    onProgress: context.onProgress,
    isCancelled: context.isCancelled,
  });
  assertTraversalUsable(result.cancelled, result.scopeLost);

  const nodes = await normalizeEntries(result.entries, context, null);
  const document: PluginDocument = {
    document_label: context.api.root.name ?? "",
    page_name: context.api.currentPage.name ?? "",
    nodes,
    capabilities: context.capabilities.capabilities,
  };
  const observedAt = new Date().toISOString();

  return {
    document,
    content_hash: contentHash(assertJsonValue(document)),
    consistency: "best_effort",
    started_at: startedAt,
    observed_at: observedAt,
    truncated: result.truncated,
    next_cursor: nextCursorFor(result, grant.grantEpoch, grant.rootIds[0] ?? ""),
    visited: result.visited,
    coverage: coverageOf(nodes, result.visited),
  };
}

/** Handle `list_sessions`: the sessions this plugin runtime serves. */
export async function handleListSessions(
  _params: unknown,
  context: HandlerContext,
): Promise<SessionsResult> {
  const grant = context.guard.current;
  if (grant === null || context.sessionId === undefined) {
    return { sessions: [] };
  }
  return {
    sessions: [
      {
        session_id: context.sessionId,
        grant_epoch: grant.grantEpoch,
        root_ids: [...grant.rootIds],
        status: "active",
        capabilities: context.capabilities.capabilities,
      },
    ],
  };
}

/**
 * Handle `watch_scope`.
 *
 * Watching is a phase P1 capability. The plugin answers with the honest
 * capability error instead of pretending that a watch exists.
 */
export async function handleWatchScope(_params: unknown, _context: HandlerContext): Promise<never> {
  throw capabilityUnavailable(
    "Watching a scope is not available in this plugin build; use a fresh read instead",
  );
}

/**
 * Handle `get_changes`.
 *
 * The change journal is a phase P1 capability and is answered honestly.
 */
export async function handleGetChanges(_params: unknown, _context: HandlerContext): Promise<never> {
  throw capabilityUnavailable(
    "The change journal is not available in this plugin build; use a fresh read instead",
  );
}

function projectionForDetail(detail: string): NodeField[] {
  if (detail === "summary") {
    return [...DEFAULT_SUMMARY_FIELDS];
  }
  if (detail === "layout") {
    return [
      "id",
      "node_type",
      "name",
      "visible",
      "locked",
      "children",
      "geometry",
      "layout",
      "fills",
      "strokes",
      "effects",
    ];
  }
  return [
    "id",
    "parent_id",
    "node_type",
    "name",
    "visible",
    "locked",
    "children",
    "geometry",
    "layout",
    "text",
    "fills",
    "strokes",
    "effects",
    "component",
    "variables",
    "styles",
  ];
}

async function normalizeEntries(
  entries: readonly TraversalEntry[],
  context: HandlerContext,
  projection: readonly NodeField[] | null,
): Promise<PluginNode[]> {
  const childrenByParent = new Map<string, string[]>();
  for (const entry of entries) {
    if (entry.parentId === undefined) {
      continue;
    }
    const siblings = childrenByParent.get(entry.parentId);
    if (siblings === undefined) {
      childrenByParent.set(entry.parentId, [entry.node.id]);
    } else {
      siblings.push(entry.node.id);
    }
  }

  const nodes: PluginNode[] = [];
  for (const entry of entries) {
    const normalized = await normalizeNode(entry.node, {
      api: context.api,
      resolver: context.resolver,
      fontCatalog: context.fontCatalog,
      parentId: entry.parentId,
      childIds: childrenByParent.get(entry.node.id) ?? [],
      includeReferences: true,
    });
    nodes.push(projectNode(normalized, projection));
  }
  return nodes;
}

function coverageOf(nodes: readonly PluginNode[], visited: number): Coverage {
  const unavailableFields = new Set<string>();
  const unsupportedTypes = new Set<string>();
  for (const node of nodes) {
    for (const field of node.unavailable_fields) {
      unavailableFields.add(field);
    }
    if (!KNOWN_NODE_TYPES.has(node.node_type)) {
      unsupportedTypes.add(node.node_type);
    }
  }
  return {
    unavailable_fields: [...unavailableFields].sort(),
    unsupported_types: [...unsupportedTypes].sort(),
    visited,
  };
}

function nextCursorFor(
  result: {
    truncated: boolean;
    stack: CursorFrame[];
    visited: number;
  },
  grantEpoch: number,
  rootId: string,
): string | null {
  if (!result.truncated) {
    return null;
  }
  return encodeCursor({
    v: 1,
    grant_epoch: grantEpoch,
    root_id: rootId,
    stack: result.stack,
    visited: result.visited,
  });
}

function assertTraversalUsable(cancelled: boolean, scopeLost: boolean): void {
  if (scopeLost) {
    throw outOfScope("The granted scope no longer covers the traversal");
  }
  if (cancelled) {
    throw new PluginError(ERROR_CODES.CANCELLED, "The operation was cancelled");
  }
}

function isInsideGrant(rootIds: readonly string[], node: FigmaNode): boolean {
  if (rootIds.length === 0) {
    return false;
  }
  if (rootIds.includes(node.id)) {
    return true;
  }
  let current = node.parent;
  let hops = 0;
  while (current !== null && current !== undefined && hops < 64) {
    if (rootIds.includes(current.id)) {
      return true;
    }
    current = current.parent;
    hops += 1;
  }
  return false;
}

function collectBoundIds(node: FigmaNode, variableIds: Set<string>, styleIds: Set<string>): void {
  const bound = node.boundVariables;
  if (bound !== undefined) {
    for (const binding of Object.values(bound)) {
      if (binding.type === "VARIABLE_ALIAS") {
        variableIds.add(binding.id);
      }
    }
  }
  for (const paints of [node.fills, node.strokes]) {
    const entries = asArray(paints);
    if (entries === undefined) {
      continue;
    }
    for (const entry of entries) {
      const record = asRecordValue(entry);
      if (record === null || record["boundVariables"] === undefined) {
        continue;
      }
      const paintBindings = asRecordValue(record["boundVariables"]);
      if (paintBindings === null) {
        continue;
      }
      for (const binding of Object.values(paintBindings)) {
        const value = asRecordValue(binding);
        if (value !== null && value["type"] === "VARIABLE_ALIAS") {
          const id = asString(value["id"]);
          if (id !== undefined) {
            variableIds.add(id);
          }
        }
      }
    }
  }
  for (const styleId of [
    node.fillStyleId,
    node.strokeStyleId,
    node.effectStyleId,
    node.textStyleId,
    node.gridStyleId,
  ]) {
    const id = asString(styleId);
    if (id !== undefined && id.length > 0) {
      styleIds.add(id);
    }
  }
}

let exportLane: Promise<unknown> = Promise.resolve();

/**
 * Run one export at a time.
 *
 * The editor allows a single global export lane, so every export queues
 * behind the running one.
 */
async function runExport(
  node: FigmaNode,
  format: "png" | "svg",
  scale: number,
): Promise<PluginExport> {
  const task = async (): Promise<PluginExport> => {
    const exportAsync = node.exportAsync;
    if (exportAsync === undefined) {
      throw exportFailed("The node does not support export");
    }
    const settings: FigmaExportSettings = {
      format: format === "svg" ? "SVG" : "PNG",
      constraint: { type: "SCALE", value: scale },
    };
    let bytes: Uint8Array;
    try {
      bytes = await exportAsync.call(node, settings);
    } catch (error) {
      throw mapExportError(error);
    }
    const width = Math.ceil((node.width ?? 0) * scale);
    const height = Math.ceil((node.height ?? 0) * scale);
    if (exportPixelCount(node.width ?? 0, node.height ?? 0, scale) > 16 * 1024 * 1024) {
      throw payloadTooLarge("The export exceeds the 16 megapixel budget");
    }
    return {
      node_id: node.id,
      mime: format === "svg" ? "image/svg+xml" : "image/png",
      width,
      height,
      bytes: base64Encode(bytes),
      hash: `sha256:${sha256Hex(bytes)}`,
    };
  };

  const started = exportLane.then(task, task);
  exportLane = started.then(
    () => undefined,
    () => undefined,
  );
  return await started;
}

function mapExportError(error: unknown): PluginError {
  const message = error instanceof Error ? error.message : "";
  const lowered = message.toLowerCase();
  if (lowered.includes("font") || lowered.includes("missing glyph")) {
    return fontUnavailable("A font required by the export is unavailable");
  }
  return exportFailed("The export failed for the requested node");
}

function emptyNode(nodeId: string): PluginNode {
  return {
    id: nodeId,
    node_type: "UNKNOWN",
    name: "",
    visible: true,
    locked: false,
    children: [],
    fills: [],
    strokes: [],
    effects: [],
    variables: [],
    styles: [],
    unavailable_fields: [],
  };
}

function asParams(value: unknown): Record<string, unknown> {
  if (value === undefined || value === null) {
    return {};
  }
  const record = asRecordValue(value);
  if (record === null) {
    throw invalidLimit("The request parameters are missing or malformed");
  }
  return record;
}

function asRecordValue(value: unknown): Record<string, unknown> | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return null;
  }
  return value as Record<string, unknown>;
}

function requiredString(record: Record<string, unknown>, key: string): string {
  const value = asString(record[key]);
  if (value === undefined || value.length === 0) {
    throw invalidLimit(`The request parameter ${key} is required`);
  }
  return value;
}

function optionalString(record: Record<string, unknown>, key: string): string | undefined {
  return asString(record[key]);
}

function requiredStringArray(record: Record<string, unknown>, key: string): string[] {
  const value = readStringArray(record[key]);
  if (value === undefined || value.length === 0) {
    throw invalidLimit(`The request parameter ${key} is required`);
  }
  return value;
}

function readStringArray(value: unknown): string[] | undefined {
  const entries = asArray(value);
  if (entries === undefined) {
    return undefined;
  }
  const result: string[] = [];
  for (const entry of entries) {
    const text = asString(entry);
    if (text !== undefined) {
      result.push(text);
    }
  }
  return result;
}

function readDepth(value: unknown): number | undefined {
  const depth = asNumber(value);
  if (depth === undefined) {
    return undefined;
  }
  if (depth < 1 || depth > DEFAULT_MAX_DEPTH) {
    throw invalidLimit(`The depth must be between 1 and ${DEFAULT_MAX_DEPTH}`);
  }
  return Math.floor(depth);
}
