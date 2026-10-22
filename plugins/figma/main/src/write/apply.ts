/**
 * The applier of the write path.
 *
 * The applier runs the actions the resolver produced, in the order of the
 * plan, and it commits the undo history of the editor exactly once, so the
 * whole plan is a single step of the undo history of the user.
 *
 * A failure halfway through the plan does not hide what happened: the
 * applier reports the operations that ran, the failure that stopped the plan
 * and the fact that the undo history was committed.
 */

import type { FigmaApi, FigmaFontName, FigmaNode } from "../figma-api.js";
import type { JsonValue } from "@figtether/shared";

import { toWriteErrorDetail, writeError, WritePlanError } from "./errors.js";
import type { WriteScope } from "./resolver.js";
import {
  WRITE_ERROR_CODES,
  WRITE_ERROR_REASONS,
  type ApplyOutcome,
  type CreateNodeAction,
  type CreateNodeType,
  type DeleteNodeAction,
  type InsertChildAction,
  type MoveNodeAction,
  type NodeRef,
  type PropertyChange,
  type ResolvedPlan,
  type SetFillsAction,
  type SetPropertiesAction,
  type SetTextAction,
  type WriteAction,
  type WriteErrorDetail,
} from "./types.js";

/** Options of one apply. */
export interface ApplyOptions {
  /** Whether a version checkpoint is written before the apply. */
  saveVersion: boolean;
  /** The title of the version checkpoint. */
  versionTitle: string;
}

/**
 * Apply the actions of a resolved plan.
 *
 * The function never throws: a failure is reported in the outcome, so the
 * caller can tell what happened to the document.
 */
export async function applyResolvedPlan(
  resolved: ResolvedPlan,
  api: FigmaApi,
  scope: WriteScope,
  options: ApplyOptions,
): Promise<ApplyOutcome> {
  const applier = new PlanApplier(api, scope);
  return await applier.apply(resolved, options);
}

/** The applier of one plan. */
class PlanApplier {
  private readonly loadedFonts = new Set<string>();
  private readonly createdNodes = new Map<string, FigmaNode>();
  private readonly created: string[] = [];
  private readonly changed: string[] = [];
  private readonly removed: string[] = [];
  private readonly moved: string[] = [];
  private readonly warnings: string[] = [];

  constructor(
    private readonly api: FigmaApi,
    private readonly scope: WriteScope,
  ) {}

  /** Apply the actions of a plan. */
  async apply(resolved: ResolvedPlan, options: ApplyOptions): Promise<ApplyOutcome> {
    let versionSaved: string | null = null;
    if (options.saveVersion) {
      versionSaved = await this.saveVersion(options.versionTitle);
    }

    let applied = 0;
    let error: WriteErrorDetail | null = null;
    for (const action of resolved.actions) {
      try {
        await this.applyAction(action);
        applied += 1;
      } catch (reason) {
        error = toWriteErrorDetail(reason, action.operation);
        break;
      }
    }

    let undoCommitted = false;
    if (applied > 0 && this.api.commitUndo !== undefined) {
      this.api.commitUndo();
      undoCommitted = true;
    }

    return {
      status: error === null ? "applied" : "partial",
      created: this.created,
      changed: this.changed,
      removed: this.removed,
      moved: this.moved,
      applied_operations: applied,
      error,
      undo_committed: undoCommitted,
      version_saved: versionSaved,
      warnings: this.warnings,
    };
  }

  /**
   * Write a version checkpoint of the file.
   *
   * The call is best effort: the seat may not expose it and the file may not
   * have a version history. The absence is reported, never hidden.
   */
  private async saveVersion(title: string): Promise<string | null> {
    if (this.api.saveVersionHistoryAsync === undefined) {
      this.warnings.push("The editor does not expose the version history of this file");
      return null;
    }
    try {
      const result = await this.api.saveVersionHistoryAsync(title);
      if (result === null || result === undefined) {
        this.warnings.push("The editor wrote no version checkpoint");
        return null;
      }
      return result.id;
    } catch {
      this.warnings.push("The editor could not write a version checkpoint");
      return null;
    }
  }

  /** Apply one action of a plan. */
  private async applyAction(action: WriteAction): Promise<void> {
    switch (action.kind) {
      case "create_node":
        await this.applyCreateNode(action);
        return;
      case "insert_child":
        await this.applyInsertChild(action);
        return;
      case "set_properties":
        await this.applySetProperties(action);
        return;
      case "set_text":
        await this.applySetText(action);
        return;
      case "set_fills":
        await this.applySetFills(action);
        return;
      case "delete_node":
        await this.applyDeleteNode(action);
        return;
      case "move_node":
        await this.applyMoveNode(action);
        return;
      default:
        return;
    }
  }

  /** Create a node and append it to its parent. */
  private async applyCreateNode(action: CreateNodeAction): Promise<void> {
    const parent = await this.resolveNode(action.parentRef, action.operation);
    if (action.font !== undefined) await this.loadFont(action.font, action.operation);
    let node: FigmaNode;
    if (action.nodeType === "GROUP") {
      const children: FigmaNode[] = [];
      for (const ref of action.childRefs) {
        children.push(await this.resolveNode(ref, action.operation));
      }
      const group = this.api.group?.(children, parent);
      if (group === undefined) {
        throw refusal(
          WRITE_ERROR_REASONS.CAPABILITY_UNAVAILABLE,
          "The editor does not expose the call a group needs",
          action.operation,
        );
      }
      node = group;
    } else {
      node = createNode(this.api, action.nodeType, action.operation);
      if (parent.appendChild === undefined) {
        throw refusal(
          WRITE_ERROR_REASONS.CAPABILITY_UNAVAILABLE,
          "The parent of the plan cannot hold children",
          action.operation,
        );
      }
      parent.appendChild(node);
    }
    if (action.font !== undefined) node.fontName = action.font;
    await this.applyChanges(node, action.changes, action.operation);
    if (action.initialFills !== undefined) node.fills = action.initialFills;
    if (action.initialText !== undefined) node.characters = action.initialText;
    this.createdNodes.set(action.ref, node);
    this.created.push(node.id);
  }

  /** Move an existing node under another parent. */
  private async applyInsertChild(action: InsertChildAction): Promise<void> {
    const parent = await this.resolveNode(action.parentRef, action.operation);
    const child = await this.resolveNode(action.childRef, action.operation);
    if (action.index === null) {
      if (parent.appendChild === undefined) {
        throw refusal(
          WRITE_ERROR_REASONS.CAPABILITY_UNAVAILABLE,
          "The parent of the plan cannot hold children",
          action.operation,
        );
      }
      parent.appendChild(child);
    } else {
      if (parent.insertChild === undefined) {
        throw refusal(
          WRITE_ERROR_REASONS.CAPABILITY_UNAVAILABLE,
          "The parent of the plan cannot hold children",
          action.operation,
        );
      }
      parent.insertChild(action.index, child);
    }
    this.moved.push(child.id);
  }

  /** Set the properties of an existing node. */
  private async applySetProperties(action: SetPropertiesAction): Promise<void> {
    const node = await this.resolveNode(action.nodeRef, action.operation);
    await this.applyChanges(node, action.changes, action.operation);
    this.changed.push(node.id);
  }

  /** Replace the characters of a text node. */
  private async applySetText(action: SetTextAction): Promise<void> {
    const node = await this.resolveNode(action.nodeRef, action.operation);
    await this.loadFont(action.font, action.operation);
    if (action.font !== null) {
      node.fontName = action.font;
    }
    node.characters = action.characters;
    if (action.fontSize !== null) {
      node.fontSize = action.fontSize;
    }
    this.changed.push(node.id);
  }

  /** Replace the fills of a node. */
  private async applySetFills(action: SetFillsAction): Promise<void> {
    const node = await this.resolveNode(action.nodeRef, action.operation);
    node.fills = action.fills;
    this.changed.push(node.id);
  }

  /** Remove a node and its subtree. */
  private async applyDeleteNode(action: DeleteNodeAction): Promise<void> {
    const node = await this.resolveNode(action.nodeRef, action.operation);
    if (node.remove === undefined) {
      throw refusal(
        WRITE_ERROR_REASONS.CAPABILITY_UNAVAILABLE,
        "The node of the plan cannot be removed",
        action.operation,
      );
    }
    const id = node.id;
    node.remove();
    this.removed.push(id);
  }

  /** Change the position of a node. */
  private async applyMoveNode(action: MoveNodeAction): Promise<void> {
    const node = await this.resolveNode(action.nodeRef, action.operation);
    if (action.x !== null) {
      node.x = action.x;
    }
    if (action.y !== null) {
      node.y = action.y;
    }
    this.moved.push(node.id);
  }

  /** Apply the property changes of an action. */
  private async applyChanges(
    node: FigmaNode,
    changes: readonly PropertyChange[],
    operation: number,
  ): Promise<void> {
    for (const change of changes) {
      if (change.requiresFont) await this.loadFont(change.font, operation);
    }
    // Figma resize switches the affected axis back to FIXED. Dimensions
    // therefore precede layout sizing, independent of canonical key order.
    const width = changes.find((change) => change.path === "width");
    const height = changes.find((change) => change.path === "height");
    if (width !== undefined || height !== undefined) {
      this.resize(
        node,
        width === undefined ? (node.width ?? 0) : numberOf(width.value),
        height === undefined ? (node.height ?? 0) : numberOf(height.value),
        operation,
      );
    }
    for (const change of changes) {
      if (change.path === "width" || change.path === "height") continue;
      assignProperty(node, change.path, change.value);
    }
  }

  /** Resize a node, keeping the dimension the change does not name. */
  private resize(node: FigmaNode, width: number, height: number, operation: number): void {
    if (node.resize === undefined) {
      throw refusal(
        WRITE_ERROR_REASONS.CAPABILITY_UNAVAILABLE,
        "The node of the plan has no size",
        operation,
      );
    }
    node.resize(width, height);
  }

  /**
   * Load a font before the text of a node changes.
   *
   * The editor refuses to write the characters of a text node whose font is
   * not loaded, and it refuses to load a font it does not have.
   */
  private async loadFont(font: FigmaFontName | null, operation: number): Promise<void> {
    if (font === null) {
      return;
    }
    const key = JSON.stringify(font);
    if (this.loadedFonts.has(key)) return;
    if (this.api.loadFontAsync === undefined) {
      throw refusal(
        WRITE_ERROR_REASONS.CAPABILITY_UNAVAILABLE,
        "The editor does not expose the font loader",
        operation,
      );
    }
    try {
      await this.api.loadFontAsync(font);
      this.loadedFonts.add(key);
    } catch {
      throw new WritePlanError(
        writeError(
          WRITE_ERROR_CODES.FONT_UNAVAILABLE,
          WRITE_ERROR_REASONS.FONT_UNAVAILABLE,
          "The editor could not load the font the text change needs",
          { operation },
        ),
      );
    }
  }

  /**
   * Resolve a reference of an action.
   *
   * A reference that names a node the plan created resolves to that node, a
   * reference that names a node of the document resolves through the scope.
   */
  private async resolveNode(ref: NodeRef, operation: number): Promise<FigmaNode> {
    const created = this.createdNodes.get(ref);
    if (created !== undefined) {
      return created;
    }
    let node: FigmaNode;
    try {
      node = await this.scope.resolveNode(ref);
    } catch {
      throw new WritePlanError(
        writeError(
          WRITE_ERROR_CODES.NODE_NOT_FOUND,
          WRITE_ERROR_REASONS.UNKNOWN_NODE,
          "The plan names a node that does not exist in this document",
          { operation },
        ),
      );
    }
    return node;
  }
}

/**
 * Create a node of a type.
 *
 * The Plugin API has no `createGroup`: a group is built from the nodes it
 * takes over, so the group is created by the group call and not here.
 */
export function createNode(api: FigmaApi, nodeType: CreateNodeType, operation: number): FigmaNode {
  switch (nodeType) {
    case "FRAME":
      return requireCall(api.createFrame, "createFrame", operation);
    case "TEXT":
      return requireCall(api.createText, "createText", operation);
    case "RECTANGLE":
      return requireCall(api.createRectangle, "createRectangle", operation);
    case "COMPONENT":
      return requireCall(api.createComponent, "createComponent", operation);
    case "VECTOR":
      return requireCall(api.createVector, "createVector", operation);
    case "SECTION":
      return requireCall(api.createSection, "createSection", operation);
    case "LINE":
      return requireCall(api.createLine, "createLine", operation);
    case "ELLIPSE":
      return requireCall(api.createEllipse, "createEllipse", operation);
    default:
      throw refusal(
        WRITE_ERROR_REASONS.CAPABILITY_UNAVAILABLE,
        "The editor does not expose the call this node type needs",
        operation,
      );
  }
}

/** Call a creation function of the editor, or refuse the plan. */
function requireCall(
  call: (() => FigmaNode) | undefined,
  name: string,
  operation: number,
): FigmaNode {
  if (call === undefined) {
    throw refusal(
      WRITE_ERROR_REASONS.CAPABILITY_UNAVAILABLE,
      `The editor does not expose ${name}`,
      operation,
    );
  }
  return call();
}

/** Assign one property of a node. */
function assignProperty(node: FigmaNode, path: string, value: JsonValue): void {
  const target = node as unknown as Record<string, unknown>;
  target[path] = value;
}

/** Narrow a property value to a number. */
function numberOf(value: JsonValue): number {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

/** Build a refusal of the write path. */
function refusal(
  reason: (typeof WRITE_ERROR_REASONS)[keyof typeof WRITE_ERROR_REASONS],
  message: string,
  operation: number | null,
): WritePlanError {
  return new WritePlanError(
    writeError(WRITE_ERROR_CODES.MUTATION_REFUSED, reason, message, { operation }),
  );
}
