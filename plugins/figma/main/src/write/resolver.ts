/**
 * The resolver of the write path.
 *
 * The resolver turns a plan into the concrete mutations it would perform. It
 * is the only place that decides what a plan means, and the same code serves
 * the preview and the apply: the preview is therefore exactly what the apply
 * would do.
 *
 * The resolver never mutates the document. It resolves the identifiers of the
 * plan against the live document, it checks the scope, the destructive
 * policy and the budgets, and it produces the actions the applier runs.
 */

import type { JsonValue } from "@figtether/shared";

import type { FigmaApi, FigmaFontName, FigmaNode, FigmaPaint } from "../figma-api.js";
import { checkPropertyValue, findProperty, normalizeFills, supportsNodeType } from "./allowlist.js";
import { BudgetLedger, DEFAULT_WRITE_BUDGETS, type WriteBudgets } from "./budgets.js";
import { toWriteErrorDetail, writeError } from "./errors.js";
import { planHash } from "./plan.js";
import {
  WRITE_ERROR_CODES,
  WRITE_ERROR_REASONS,
  type CreateNodeAction,
  type CreateNodeType,
  type DeleteNodeAction,
  type InsertChildAction,
  type MoveNodeAction,
  type NodeRef,
  type PlannedCreation,
  type PropertyChange,
  type ResolvedPlan,
  type SetFillsAction,
  type SetPropertiesAction,
  type SetTextAction,
  type WriteAction,
  type WriteErrorDetail,
  type WriteOperation,
  type WritePlan,
  type WritePreview,
} from "./types.js";

/** The font the plugin falls back to when a text node names none. */
export const FALLBACK_FONT: FigmaFontName = { family: "Inter", style: "Regular" };

/** The granted scope the resolver enforces. */
export interface WriteScope {
  /** The ids of the granted roots. */
  rootIds: readonly string[];
  /** The epoch of the grant. */
  grantEpoch: number;
  /** Look up a node of the document. */
  resolveNode(nodeId: string): Promise<FigmaNode>;
  /** Whether a node lives inside the granted roots. */
  isInScope(node: FigmaNode): boolean;
}

/** Options of one resolution. */
export interface ResolveOptions {
  /** Which flow asked for the resolution. */
  mode: "preview" | "apply";
  /** The limits the plan has to stay inside. */
  budgets?: WriteBudgets | undefined;
  /** The approval the user has to grant, when the daemon minted one. */
  approvalId?: string | null;
}

/** A node the plan creates, as the resolver knows it. */
interface PendingNode {
  /** The alias of the node. */
  ref: NodeRef;
  /** The parent the node is created under. */
  parentId: string;
  /** The type of the node. */
  nodeType: CreateNodeType;
  /** The name of the node, when the plan names one. */
  name: string | null;
}

/**
 * Resolve a plan against the live document.
 *
 * The result carries the preview of the plan and the actions the applier
 * runs. A plan that cannot be applied as written produces a blocked preview
 * with the reasons, and no action that would change the document.
 */
export async function resolvePlan(
  plan: WritePlan,
  api: FigmaApi,
  scope: WriteScope,
  options: ResolveOptions,
): Promise<ResolvedPlan> {
  if (options.mode !== "preview" && options.mode !== "apply") {
    throw new Error("resolvePlan: the mode has to be preview or apply");
  }
  const resolver = new PlanResolver(plan, api, scope, options.budgets ?? DEFAULT_WRITE_BUDGETS);
  if (options.approvalId !== undefined && options.approvalId !== null) {
    resolver.approvalId = options.approvalId;
  }
  return await resolver.resolve();
}

/** The resolver of one plan. */
class PlanResolver {
  private readonly ledger: BudgetLedger;
  private readonly warnings: string[] = [];
  private readonly reasons: WriteErrorDetail[] = [];
  private readonly actions: WriteAction[] = [];
  private readonly createdPlan: PlannedCreation[] = [];
  private readonly changed = new Set<string>();
  private readonly removed = new Set<string>();
  private readonly moved = new Set<string>();
  private readonly pending = new Map<string, PendingNode>();
  /** The approval the user has to grant, when the daemon minted one. */
  approvalId: string | null = null;

  constructor(
    private readonly plan: WritePlan,
    private readonly api: FigmaApi,
    private readonly scope: WriteScope,
    budgets: WriteBudgets,
  ) {
    this.ledger = new BudgetLedger(budgets);
  }

  /** Resolve the plan. */
  async resolve(): Promise<ResolvedPlan> {
    this.checkDestructivePolicy();
    for (const [index, operation] of this.plan.operations.entries()) {
      this.ledger.countOperation();
      const breach = this.ledger.breach(index);
      if (breach !== null) {
        this.reasons.push(breach);
        break;
      }
      await this.resolveOperation(index, operation);
      const operationBreach = this.ledger.breach(index);
      if (operationBreach !== null) {
        this.reasons.push(operationBreach);
        break;
      }
    }

    const preview: WritePreview = {
      plan_id: this.plan.plan_id,
      plan_hash: planHash(this.plan),
      created: [],
      created_count: this.createdPlan.length,
      created_plan: this.createdPlan,
      changed: [...this.changed],
      removed: [...this.removed],
      moved: [...this.moved],
      operations_count: this.plan.operations.length,
      warnings: this.warnings,
      blocked: this.reasons.length > 0,
      reasons: this.reasons.map((reason) => reason.message),
      ...(this.approvalId === null ? {} : { approval_id: this.approvalId }),
    };
    return { preview, actions: this.actions, reasons: [...this.reasons] };
  }

  /**
   * Refuse a plan that changes the structure without the destructive flag.
   *
   * A delete and an insert change the structure of the document, so both need
   * the explicit flag of the plan. A move is a structural change as well, but
   * the shared contract asks for the approval of a move on the side of the
   * daemon, so a move without the flag is reported and not forbidden.
   */
  private checkDestructivePolicy(): void {
    const structural = this.plan.operations.some(
      (operation) => operation.kind === "delete_node" || operation.kind === "insert_child",
    );
    if (structural && !this.plan.destructive) {
      this.reasons.push(
        writeError(
          WRITE_ERROR_CODES.DESTRUCTIVE_NOT_ALLOWED,
          WRITE_ERROR_REASONS.DESTRUCTIVE_NOT_ALLOWED,
          "The plan holds a delete or an insert but does not carry the destructive flag",
        ),
      );
      return;
    }
    const moves = this.plan.operations.some((operation) => operation.kind === "move_node");
    if (moves && !this.plan.destructive) {
      this.warnings.push(
        "The plan moves a node without carrying the destructive flag",
      );
    }
  }

  /** Resolve one operation of the plan. */
  private async resolveOperation(index: number, operation: WriteOperation): Promise<void> {
    switch (operation.kind) {
      case "create_node":
        await this.resolveCreateNode(index, operation);
        return;
      case "insert_child":
        await this.resolveInsertChild(index, operation);
        return;
      case "set_properties":
        await this.resolveSetProperties(index, operation);
        return;
      case "set_text":
        await this.resolveSetText(index, operation);
        return;
      case "set_fills":
        await this.resolveSetFills(index, operation);
        return;
      case "delete_node":
        await this.resolveDeleteNode(index, operation);
        return;
      case "move_node":
        await this.resolveMoveNode(index, operation);
        return;
      default:
        return;
    }
  }

  /** Resolve a create operation. */
  private async resolveCreateNode(
    index: number,
    operation: Extract<WriteOperation, { kind: "create_node" }>,
  ): Promise<void> {
    const parent = await this.resolveRef(operation.parent_id, index);
    if (parent === null) {
      return;
    }
    if (!canHoldChildren(parent.type)) {
      this.reasons.push(
        writeError(
          WRITE_ERROR_CODES.MUTATION_REFUSED,
          WRITE_ERROR_REASONS.PARENT_CANT_HOLD_CHILDREN,
          "The parent of the plan cannot hold children",
          { operation: index },
        ),
      );
      return;
    }
    const missing = missingCreationCall(this.api, operation.node_type);
    if (missing !== null) {
      this.reasons.push(
        writeError(
          WRITE_ERROR_CODES.MUTATION_REFUSED,
          WRITE_ERROR_REASONS.CAPABILITY_UNAVAILABLE,
          `The editor does not expose the call a ${operation.node_type} needs`,
          { operation: index },
        ),
      );
      return;
    }
    if (operation.node_type === "GROUP" && operation.children_ids.length === 0) {
      this.reasons.push(
        writeError(
          WRITE_ERROR_CODES.PLAN_INVALID,
          WRITE_ERROR_REASONS.INVALID_PLAN,
          "A group needs at least one node to take over",
          { operation: index },
        ),
      );
      return;
    }

    const ref = `#${index}`;
    const changes = this.resolveChanges(operation.properties, operation.node_type, null, index);
    if (operation.x !== undefined) {
      setChange(changes, "x", operation.x);
    }
    if (operation.y !== undefined) {
      setChange(changes, "y", operation.y);
    }
    const name = readName(changes);
    this.pending.set(ref, {
      ref,
      parentId: parent.id,
      nodeType: operation.node_type,
      name,
    });
    this.createdPlan.push({
      ref,
      parent_id: parent.id,
      node_type: operation.node_type,
      name,
    });
    this.ledger.countCreatedNode();
    this.ledger.countPropertyChanges(changes.length);

    const childRefs: NodeRef[] = [];
    for (const childId of operation.children_ids) {
      if (childId === ref) {
        this.reasons.push(
          writeError(
            WRITE_ERROR_CODES.PLAN_INVALID,
            WRITE_ERROR_REASONS.INVALID_PLAN,
            "A group cannot take over itself",
            { operation: index },
          ),
        );
        continue;
      }
      const child = await this.resolveRef(childId, index);
      if (child === null) {
        continue;
      }
      childRefs.push(childId);
    }

    const action: CreateNodeAction = {
      kind: "create_node",
      operation: index,
      ref,
      parentRef: operation.parent_id,
      nodeType: operation.node_type,
      changes,
      childRefs,
    };
    this.actions.push(action);
  }

  /** Resolve an insert operation. */
  private async resolveInsertChild(
    index: number,
    operation: Extract<WriteOperation, { kind: "insert_child" }>,
  ): Promise<void> {
    const parent = await this.resolveRef(operation.parent_id, index);
    const child = await this.resolveRef(operation.node_id, index);
    if (parent === null || child === null) {
      return;
    }
    if (!canHoldChildren(parent.type)) {
      this.reasons.push(
        writeError(
          WRITE_ERROR_CODES.MUTATION_REFUSED,
          WRITE_ERROR_REASONS.PARENT_CANT_HOLD_CHILDREN,
          "The parent of the plan cannot hold children",
          { operation: index },
        ),
      );
      return;
    }
    if (child.locked) {
      this.reasons.push(
        writeError(
          WRITE_ERROR_CODES.MUTATION_REFUSED,
          WRITE_ERROR_REASONS.READONLY_NODE,
          "The node of the plan is read only",
          { operation: index },
        ),
      );
      return;
    }
    if (isInsideInstance(child)) {
      this.reasons.push(
        writeError(
          WRITE_ERROR_CODES.MUTATION_REFUSED,
          WRITE_ERROR_REASONS.INSTANCE_CHILD,
          "The node of the plan lives inside an instance and cannot move",
          { operation: index },
        ),
      );
      return;
    }
    if (isDescendantOf(parent, child)) {
      const componentCycle = child.type === "COMPONENT" || child.type === "COMPONENT_SET";
      this.reasons.push(
        writeError(
          WRITE_ERROR_CODES.MUTATION_REFUSED,
          componentCycle
            ? WRITE_ERROR_REASONS.COMPONENT_CYCLE
            : WRITE_ERROR_REASONS.PARENTING_CYCLE,
          componentCycle
            ? "The move of the plan would put a component inside itself"
            : "The move of the plan would create a cycle",
          { operation: index },
        ),
      );
      return;
    }
    const action: InsertChildAction = {
      kind: "insert_child",
      operation: index,
      parentRef: operation.parent_id,
      childRef: operation.node_id,
      index: operation.index ?? null,
    };
    this.actions.push(action);
    this.moved.add(child.id);
  }

  /** Resolve a property operation. */
  private async resolveSetProperties(
    index: number,
    operation: Extract<WriteOperation, { kind: "set_properties" }>,
  ): Promise<void> {
    const node = await this.resolveRef(operation.node_id, index);
    if (node === null) {
      return;
    }
    const changes = this.resolveChanges(operation.properties, node.type, node, index);
    this.ledger.countPropertyChanges(changes.length);
    if (changes.length === 0) {
      return;
    }
    const action: SetPropertiesAction = {
      kind: "set_properties",
      operation: index,
      nodeRef: operation.node_id,
      changes,
    };
    this.actions.push(action);
    this.changed.add(node.id);
  }

  /** Resolve a text operation. */
  private async resolveSetText(
    index: number,
    operation: Extract<WriteOperation, { kind: "set_text" }>,
  ): Promise<void> {
    const node = await this.resolveRef(operation.node_id, index);
    if (node === null) {
      return;
    }
    if (node.type !== "TEXT") {
      this.warnings.push(
        `The node ${node.id} is not a text node, the characters were not applied`,
      );
      return;
    }
    const font = fontOfOperation(operation) ?? fontOf(node) ?? FALLBACK_FONT;
    this.ledger.countTextCharacters(operation.text.length);
    const action: SetTextAction = {
      kind: "set_text",
      operation: index,
      nodeRef: operation.node_id,
      characters: operation.text,
      font,
      fontSize: null,
    };
    this.actions.push(action);
    this.changed.add(node.id);
  }

  /**
   * Resolve a fill operation.
   *
   * The paints travel in the dedicated field of the operation, and they are
   * normalized here: a raw payload of the agent never reaches the editor.
   */
  private async resolveSetFills(
    index: number,
    operation: Extract<WriteOperation, { kind: "set_fills" }>,
  ): Promise<void> {
    const node = await this.resolveRef(operation.node_id, index);
    if (node === null) {
      return;
    }
    if (!canHoldFills(node.type)) {
      this.warnings.push(`The node ${node.id} does not carry fills, the fills were not applied`);
      return;
    }
    let paints: FigmaPaint[];
    try {
      paints = normalizeFills(operation.fills);
    } catch (error) {
      const detail = toWriteErrorDetail(error, index);
      this.reasons.push({ ...detail, operation: index });
      return;
    }
    const action: SetFillsAction = {
      kind: "set_fills",
      operation: index,
      nodeRef: operation.node_id,
      fills: paints,
    };
    this.actions.push(action);
    this.changed.add(node.id);
  }

  /** Resolve a delete operation. */
  private async resolveDeleteNode(
    index: number,
    operation: Extract<WriteOperation, { kind: "delete_node" }>,
  ): Promise<void> {
    const node = await this.resolveRef(operation.node_id, index);
    if (node === null) {
      return;
    }
    if (this.scope.rootIds.includes(node.id)) {
      this.reasons.push(
        writeError(
          WRITE_ERROR_CODES.MUTATION_REFUSED,
          WRITE_ERROR_REASONS.GRANTED_ROOT_DELETE,
          "The plan deletes a granted root",
          { operation: index },
        ),
      );
      return;
    }
    const subtreeSize = countSubtree(node);
    const breach = this.ledger.checkDeleteSubtree(subtreeSize, index);
    if (breach !== null) {
      this.reasons.push(breach);
      return;
    }
    const action: DeleteNodeAction = {
      kind: "delete_node",
      operation: index,
      nodeRef: operation.node_id,
      subtreeSize,
    };
    this.actions.push(action);
    this.removed.add(node.id);
  }

  /** Resolve a move operation. */
  private async resolveMoveNode(
    index: number,
    operation: Extract<WriteOperation, { kind: "move_node" }>,
  ): Promise<void> {
    const node = await this.resolveRef(operation.node_id, index);
    if (node === null) {
      return;
    }
    if (await this.leavesScopeBounds(node, operation.x ?? null, operation.y ?? null)) {
      this.warnings.push(
        `The new position of ${node.id} leaves the bounds of the granted scope`,
      );
    }
    const action: MoveNodeAction = {
      kind: "move_node",
      operation: index,
      nodeRef: operation.node_id,
      x: operation.x ?? null,
      y: operation.y ?? null,
    };
    this.actions.push(action);
    this.moved.add(node.id);
  }

  /**
   * Resolve the property changes of an operation.
   *
   * A property outside the allowlist is a plan error. A property the node
   * type does not support is a warning, and the change is skipped.
   */
  private resolveChanges(
    properties: Record<string, JsonValue>,
    nodeType: string,
    node: FigmaNode | null,
    operation: number,
  ): PropertyChange[] {
    const changes: PropertyChange[] = [];
    for (const [name, value] of Object.entries(properties)) {
      const spec = findProperty(name);
      if (spec === undefined) {
        this.reasons.push(
          writeError(
            WRITE_ERROR_CODES.PLAN_INVALID,
            WRITE_ERROR_REASONS.UNKNOWN_PROPERTY,
            `The plan uses the property ${name}, which is not allowlisted`,
            { operation },
          ),
        );
        continue;
      }
      try {
        checkPropertyValue(spec, value);
      } catch (error) {
        const detail = toWriteErrorDetail(error, operation);
        this.reasons.push({ ...detail, operation });
        continue;
      }
      if (!supportsNodeType(spec, nodeType)) {
        this.warnings.push(
          `The property ${name} does not apply to a ${nodeType} and was not applied`,
        );
        continue;
      }
      changes.push({
        name,
        path: spec.path,
        value,
        requiresFont: spec.requiresFont,
        font: spec.requiresFont ? fontOf(node) ?? FALLBACK_FONT : null,
      });
    }
    return changes;
  }

  /**
   * Resolve a node reference of the plan.
   *
   * A reference that names a node of the document has to exist and has to
   * live inside the granted roots. A reference that names a node the plan
   * creates is resolved against the pending creations.
   */
  private async resolveRef(ref: NodeRef, operation: number): Promise<FigmaNode | null> {
    if (ref.startsWith("#")) {
      const pending = this.pending.get(ref);
      if (pending === undefined) {
        this.reasons.push(
          writeError(
            WRITE_ERROR_CODES.PLAN_INVALID,
            WRITE_ERROR_REASONS.INVALID_PLAN,
            "The plan refers to a node it never creates",
            { operation },
          ),
        );
        return null;
      }
      return syntheticNode(pending);
    }
    let node: FigmaNode;
    try {
      node = await this.scope.resolveNode(ref);
    } catch {
      this.reasons.push(
        writeError(
          WRITE_ERROR_CODES.NODE_NOT_FOUND,
          WRITE_ERROR_REASONS.UNKNOWN_NODE,
          "The plan names a node that does not exist in this document",
          { operation },
        ),
      );
      return null;
    }
    if (!this.scope.isInScope(node)) {
      this.reasons.push(
        writeError(
          WRITE_ERROR_CODES.OUT_OF_SCOPE,
          WRITE_ERROR_REASONS.NODE_OUT_OF_SCOPE,
          "The plan names a node outside the granted scope",
          { operation },
        ),
      );
      return null;
    }
    return node;
  }

  /**
   * Whether the new position of a node leaves the bounds of the granted
   * roots.
   *
   * The check is a warning and not a refusal: the plugin knows the bounds of
   * the root, but it does not always know the transform of every parent, so
   * a position outside the bounds is reported and not forbidden.
   */
  private async leavesScopeBounds(
    node: FigmaNode,
    x: number | null,
    y: number | null,
  ): Promise<boolean> {
    const box = node.absoluteBoundingBox;
    if (box === undefined || box === null) {
      return false;
    }
    const dx = x === null ? 0 : x - (node.x ?? 0);
    const dy = y === null ? 0 : y - (node.y ?? 0);
    const moved = {
      x: box.x + dx,
      y: box.y + dy,
      width: box.width,
      height: box.height,
    };
    for (const rootId of this.scope.rootIds) {
      let root: FigmaNode;
      try {
        root = await this.scope.resolveNode(rootId);
      } catch {
        return false;
      }
      const rootBox = root.absoluteBoundingBox;
      if (rootBox === undefined || rootBox === null) {
        return false;
      }
      if (boxesIntersect(moved, rootBox)) {
        return false;
      }
    }
    return true;
  }
}

/** Whether a node type can hold children. */
export function canHoldChildren(nodeType: string): boolean {
  return nodeType !== "TEXT" && nodeType !== "LINE" && nodeType !== "DOCUMENT" && nodeType !== "PAGE";
}

/** Whether a node type carries fills. */
export function canHoldFills(nodeType: string): boolean {
  return nodeType !== "LINE";
}

/**
 * The creation call a node type needs, when the editor does not expose it.
 *
 * The Plugin API has no `createGroup`: a group is built from the nodes it
 * takes over, so the group call is the one the resolver looks for.
 */
export function missingCreationCall(api: FigmaApi, nodeType: CreateNodeType): string | null {
  switch (nodeType) {
    case "FRAME":
      return api.createFrame === undefined ? "createFrame" : null;
    case "TEXT":
      return api.createText === undefined ? "createText" : null;
    case "RECTANGLE":
      return api.createRectangle === undefined ? "createRectangle" : null;
    case "COMPONENT":
      return api.createComponent === undefined ? "createComponent" : null;
    case "VECTOR":
      return api.createVector === undefined ? "createVector" : null;
    case "SECTION":
      return api.createSection === undefined ? "createSection" : null;
    case "LINE":
      return api.createLine === undefined ? "createLine" : null;
    case "ELLIPSE":
      return api.createEllipse === undefined ? "createEllipse" : null;
    case "GROUP":
      return api.group === undefined ? "group" : null;
    default:
      return "createNode";
  }
}

/** Whether a node lives inside an instance. */
export function isInsideInstance(node: FigmaNode): boolean {
  let current = node.parent;
  let hops = 0;
  while (current !== null && current !== undefined && hops < 64) {
    if (current.type === "INSTANCE") {
      return true;
    }
    current = current.parent;
    hops += 1;
  }
  return false;
}

/**
 * Whether a node is a descendant of another node.
 *
 * The walk is bounded, so a malformed document cannot hang the sandbox.
 */
export function isDescendantOf(node: FigmaNode, ancestor: FigmaNode): boolean {
  let current = node.parent;
  let hops = 0;
  while (current !== null && current !== undefined && hops < 64) {
    if (current.id === ancestor.id) {
      return true;
    }
    current = current.parent;
    hops += 1;
  }
  return false;
}

/**
 * The number of nodes a removal takes with it.
 *
 * The walk is bounded by the delete budget times four, so a huge subtree
 * cannot hang the sandbox: a subtree that is too large is refused anyway.
 */
export function countSubtree(node: FigmaNode, limit = 10000): number {
  let count = 1;
  const stack: FigmaNode[] = [...(node.children ?? [])];
  while (stack.length > 0) {
    const child = stack.pop();
    if (child === undefined) {
      continue;
    }
    count += 1;
    if (count > limit) {
      return count;
    }
    for (const grand of child.children ?? []) {
      stack.push(grand);
    }
  }
  return count;
}

/** The font of a node, when the node names one. */
export function fontOf(node: FigmaNode | null): FigmaFontName | null {
  if (node === null) {
    return null;
  }
  const fontName = node.fontName;
  if (typeof fontName !== "object" || fontName === null || Array.isArray(fontName)) {
    return null;
  }
  const record = fontName as Record<string, unknown>;
  const family = record["family"];
  const style = record["style"];
  if (typeof family !== "string" || typeof style !== "string") {
    return null;
  }
  return { family, style };
}

/** The font a text operation names, when it names one. */
export function fontOfOperation(
  operation: Extract<WriteOperation, { kind: "set_text" }>,
): FigmaFontName | null {
  const family = operation.font_family;
  const style = operation.font_style;
  if (family === undefined || style === undefined) {
    return null;
  }
  return { family, style };
}

/** The name of a set of property changes, when the set names one. */
function readName(changes: readonly PropertyChange[]): string | null {
  for (const change of changes) {
    if (change.name === "name" && typeof change.value === "string") {
      return change.value;
    }
  }
  return null;
}

/**
 * Set a property change, replacing the change of the same name.
 *
 * The explicit fields of an operation win over the properties of the same
 * operation, so a plan that names a position twice does it once.
 */
function setChange(changes: PropertyChange[], name: string, value: number): void {
  const existing = changes.find((change) => change.name === name);
  if (existing !== undefined) {
    existing.value = value;
    return;
  }
  changes.push({
    name,
    path: name,
    value,
    requiresFont: false,
    font: null,
  });
}

/** The synthetic node of a pending creation. */
function syntheticNode(pending: PendingNode): FigmaNode {
  return {
    id: pending.ref,
    type: pending.nodeType,
    name: pending.name ?? pending.ref,
    visible: true,
    locked: false,
    parent: null,
    children: [],
  };
}

/** Whether two boxes overlap. */
function boxesIntersect(
  left: { x: number; y: number; width: number; height: number },
  right: { x: number; y: number; width: number; height: number },
): boolean {
  return (
    left.x < right.x + right.width &&
    right.x < left.x + left.width &&
    left.y < right.y + right.height &&
    right.y < left.y + left.height
  );
}
