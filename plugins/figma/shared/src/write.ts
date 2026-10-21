/**
 * Contract of the write path.
 *
 * Mirror of the Rust write contract in `crates/model/src/write.rs`. The agent
 * never mutates the document: it proposes a plan, the plugin resolves the
 * plan against the live document without mutating it, the user approves the
 * preview, and only then the plugin applies the plan. The types of this
 * module are the shapes that travel on that path.
 *
 * The field names are the wire names of the Rust structures, in snake case.
 * An optional field is absent rather than null, so the plan hash of the two
 * sides covers the same bytes.
 *
 * Nothing in this file uses a browser API or a Node.js API: the Figma main
 * sandbox has neither.
 */
import { canonicalStringify } from "./canonical";
import type { JsonValue } from "./json";

/**
 * Mode of a grant on the write path.
 *
 * A read grant never authorizes a mutation and a write grant never widens
 * the roots of the read grant. The session mode of a grant, which is `live`
 * or `paused`, is a different thing and lives in `protocol.ts`.
 */
export type GrantMode = "read" | "write";

/** Kinds of operation a write plan may carry. */
export type WriteOperationKind =
  | "create_node"
  | "insert_child"
  | "set_properties"
  | "set_text"
  | "set_fills"
  | "delete_node"
  | "move_node";

/** One operation of a write plan. */
export interface WriteOperation {
  /** What the operation does. */
  kind: WriteOperationKind;
  /** Parent the operation applies to, when it needs one. */
  parent_id?: string;
  /** Node the operation applies to, when it needs one. */
  node_id?: string;
  /** Type of the node the operation creates. */
  node_type?: string;
  /** Property values the operation sets. */
  properties?: Record<string, JsonValue>;
  /**
   * Paints the operation sets on the node.
   *
   * The `create_node` and `set_fills` operations read this field. It is the
   * place a paint travels: fills are not a property, so they are not on the
   * property allowlist. The values are the raw paints of the plan, so the
   * plan hash covers them.
   */
  fills?: JsonValue[];
  /** Characters the operation writes. */
  text?: string;
  /** Horizontal position, for a move. */
  x?: number;
  /** Vertical position, for a move. */
  y?: number;
  /** Index inside the parent, for a create or an insert. */
  index?: number;
  /** Font family of the written text. */
  font_family?: string;
  /** Font style of the written text. */
  font_style?: string;
}

/** A write plan, the only thing an agent may submit. */
export interface WritePlan {
  /** Opaque id of the plan, minted by the agent, echoed back in the preview. */
  plan_id: string;
  /** Session the plan targets. */
  session_id?: string;
  /** Ordered list of the operations of the plan. */
  operations: WriteOperation[];
  /** Short free text shown to the user, never executed. */
  intent?: string;
  /** Whether the plan asks for a destructive change. */
  destructive: boolean;
}

/**
 * A write plan on its way to the plugin.
 *
 * The approval and the hash travel next to the plan, not inside it: the plan
 * is the data the user approves, and the two fields bind the approval to
 * exactly that data.
 */
export interface WritePlanPayload {
  /** The plan, as the model layer serializes it. */
  plan: WritePlan;
  /** Approval the user has to grant for this plan. */
  approval_id?: string;
  /**
   * Hash the daemon computed for the plan.
   *
   * The plugin compares it with the hash it computes itself, so a plan that
   * changed on the way is refused instead of previewed.
   */
  plan_hash?: string;
}

/** The preview of a plan, the answer of the plugin before any mutation. */
export interface WritePreview {
  /** Id of the plan the preview belongs to. */
  plan_id: string;
  /** Hash of the plan, the binding token of the approval. */
  plan_hash: string;
  /**
   * Node ids the plan would create.
   *
   * A node that does not exist yet has no id, so the preview leaves the list
   * empty and reports the number of the planned creations in
   * `created_count`.
   */
  created: string[];
  /** Number of nodes the plan would create. */
  created_count: number;
  /** Approval the user has to grant for this plan. */
  approval_id?: string;
  /** Node ids the plan would change. */
  changed: string[];
  /** Node ids the plan would remove. */
  removed: string[];
  /** Node ids the plan would reparent or reposition. */
  moved: string[];
  /** Number of operations of the plan. */
  operations_count: number;
  /** Reasons a part of the plan could not be resolved. */
  warnings: string[];
  /** Whether the plan can be applied as it was written. */
  blocked: boolean;
}

/** The approval of a plan, minted by the daemon after the user agreed. */
export interface WriteApproval {
  /** Id of the approval. */
  approval_id: string;
  /** Hash of the plan the user approved. */
  plan_hash: string;
  /** Session the approval is bound to. */
  session_id: string;
  /** Grant epoch the approval is bound to. */
  grant_epoch: number;
  /** When the approval was minted, RFC 3339 in UTC. */
  issued_at: string;
  /** When the approval expires, RFC 3339 in UTC. */
  expires_at: string;
  /** Who approved the plan. */
  approved_by: string;
}

/** The result of an applied plan. */
export interface WriteResult {
  /** Id of the plan that was applied. */
  plan_id: string;
  /** Hash of the plan that was applied. */
  plan_hash: string;
  /** Id of the approval that unlocked the plan. */
  approval_id: string;
  /** Node ids that exist after the apply. */
  created: string[];
  /** Node ids that were changed. */
  changed: string[];
  /** Node ids that were removed. */
  removed: string[];
  /** Node ids that were moved. */
  moved: string[];
  /** Whether the plugin committed the plan as one undo step. */
  undo_committed: boolean;
  /** Version id, when a version checkpoint was written. */
  version_saved?: string;
  /** Reasons a part of the plan could not be applied. */
  warnings: string[];
}

/** Node types a plan may create. */
export const WRITE_NODE_TYPES = [
  "FRAME",
  "TEXT",
  "RECTANGLE",
  "COMPONENT",
  "VECTOR",
  "SECTION",
  "GROUP",
  "LINE",
  "ELLIPSE",
] as const;

/**
 * Properties a plan may set.
 *
 * The list is closed. It holds the geometry, the size, the layout, the
 * appearance and the naming properties a design system needs, and nothing
 * that changes the structure of the document implicitly. A property outside
 * the list is a plan error, never a best effort.
 *
 * The Rust list `ALLOWED_WRITE_PROPERTIES` in `crates/model/src/write.rs` is
 * the source of truth. The names, their order and their number have to be
 * the same on both sides: a change on one side has to change the other one,
 * and the test of this package compares the list with the Rust one.
 */
export const ALLOWED_WRITE_PROPERTIES = [
  "x",
  "y",
  "width",
  "height",
  "rotation",
  "opacity",
  "visible",
  "locked",
  "name",
  "fontSize",
  "cornerRadius",
  "topLeftRadius",
  "topRightRadius",
  "bottomLeftRadius",
  "bottomRightRadius",
  "layoutMode",
  "primaryAxisAlignItems",
  "counterAxisAlignItems",
  "layoutSizingHorizontal",
  "layoutSizingVertical",
  "itemSpacing",
  "paddingLeft",
  "paddingRight",
  "paddingTop",
  "paddingBottom",
  "clipsContent",
  "blendMode",
  "description",
] as const;

/** Lifetime of an approval, in milliseconds. */
export const WRITE_APPROVAL_TTL_MS = 600_000;

/** Operations that change the structure of the document. */
const DESTRUCTIVE_OPERATION_KINDS: readonly WriteOperationKind[] = [
  "delete_node",
  "insert_child",
  "move_node",
];

/** Operations that name an existing node. */
const NODE_ID_REQUIRED_KINDS: readonly WriteOperationKind[] = [
  "insert_child",
  "set_properties",
  "set_text",
  "set_fills",
  "delete_node",
  "move_node",
];

/** Operations that name an existing parent. */
const PARENT_ID_REQUIRED_KINDS: readonly WriteOperationKind[] = ["create_node", "insert_child"];

/**
 * Whether a plan asks for a destructive change.
 *
 * A plan is destructive when it says so, and also when it carries a delete,
 * an insert or a move, because the flag of the plan is a claim of the agent
 * and the operation list is the truth.
 */
export function isDestructivePlan(plan: WritePlan): boolean {
  if (plan.destructive) {
    return true;
  }
  return plan.operations.some((operation) => DESTRUCTIVE_OPERATION_KINDS.includes(operation.kind));
}

/** Number of operations of a plan. */
export function planOperationCount(plan: WritePlan): number {
  return plan.operations.length;
}

/** Number of characters a plan writes. */
export function planTextLength(plan: WritePlan): number {
  let total = 0;
  for (const operation of plan.operations) {
    if (operation.text !== undefined) {
      total += operation.text.length;
    }
  }
  return total;
}

/**
 * Check the shape of a plan.
 *
 * The function is pure: it never touches the editor and it never decides
 * whether a write is allowed. It reports the structural problems of the
 * plan, so the daemon and the plugin can refuse a malformed plan with the
 * same reasons. The rules are the ones of the design: a plan has
 * operations, an operation names the node it applies to, a create names its
 * parent and its type, and every property is on the allowlist.
 *
 * The reasons are plain strings, so they travel inside an error payload.
 */
export function validatePlanShape(plan: WritePlan): string[] {
  const problems: string[] = [];
  if (plan.operations.length === 0) {
    problems.push("the plan has no operations");
  }
  plan.operations.forEach((operation, index) => {
    if (NODE_ID_REQUIRED_KINDS.includes(operation.kind) && !isFilled(operation.node_id)) {
      problems.push(`operation ${index}: missing node id`);
    }
    if (PARENT_ID_REQUIRED_KINDS.includes(operation.kind) && !isFilled(operation.parent_id)) {
      problems.push(`operation ${index}: missing parent id`);
    }
    if (operation.kind === "create_node" && !isFilled(operation.node_type)) {
      problems.push(`operation ${index}: missing node type`);
    }
    if (operation.node_type !== undefined && !isKnownNodeType(operation.node_type)) {
      problems.push(`operation ${index}: unknown node type "${operation.node_type}"`);
    }
    if (operation.properties !== undefined) {
      for (const name of Object.keys(operation.properties)) {
        if (!isAllowedProperty(name)) {
          problems.push(`operation ${index}: unknown property "${name}"`);
        }
      }
    }
  });
  return problems;
}

/**
 * Encode a plan with the canonical JSON encoding.
 *
 * Both sides of the bridge hash the plan before the approval, so the bytes
 * have to be the same on both sides. The encoding is the canonical one of
 * this package: the keys are sorted and an absent optional field stays
 * absent.
 *
 * The encoding follows the rules of the Rust serializer of the plan:
 * - an empty property map is left out, like an empty `BTreeMap` is;
 * - the position of a node is a float in the Rust contract, so `12` is
 *   written as `12.0`;
 * - the index of a node is a whole number, so it is written as `3`.
 *
 * The plan hash of the daemon covers the typed plan, and the plugin rebuilds
 * that typed plan out of the raw plan it receives, so the two hashes only
 * match when the encoding follows the same rules.
 */
export function canonicalPlanJson(plan: WritePlan): string {
  const entries: string[] = [];
  entries.push(`"destructive":${plan.destructive ? "true" : "false"}`);
  if (plan.intent !== undefined) {
    entries.push(`"intent":${canonicalStringify(plan.intent)}`);
  }
  entries.push(`"operations":[${plan.operations.map(canonicalOperationJson).join(",")}]`);
  entries.push(`"plan_id":${canonicalStringify(plan.plan_id)}`);
  if (plan.session_id !== undefined) {
    entries.push(`"session_id":${canonicalStringify(plan.session_id)}`);
  }
  return `{${entries.join(",")}}`;
}

/** Encode one operation with the canonical JSON encoding. */
function canonicalOperationJson(operation: WriteOperation): string {
  const entries: string[] = [];
  if (operation.fills !== undefined) {
    entries.push(`"fills":${canonicalStringify(operation.fills)}`);
  }
  if (operation.font_family !== undefined) {
    entries.push(`"font_family":${canonicalStringify(operation.font_family)}`);
  }
  if (operation.font_style !== undefined) {
    entries.push(`"font_style":${canonicalStringify(operation.font_style)}`);
  }
  if (operation.index !== undefined) {
    entries.push(`"index":${encodeWholeNumber(operation.index)}`);
  }
  entries.push(`"kind":${canonicalStringify(operation.kind)}`);
  if (operation.node_id !== undefined) {
    entries.push(`"node_id":${canonicalStringify(operation.node_id)}`);
  }
  if (operation.node_type !== undefined) {
    entries.push(`"node_type":${canonicalStringify(operation.node_type)}`);
  }
  if (operation.parent_id !== undefined) {
    entries.push(`"parent_id":${canonicalStringify(operation.parent_id)}`);
  }
  if (operation.properties !== undefined && Object.keys(operation.properties).length > 0) {
    entries.push(`"properties":${canonicalStringify(operation.properties)}`);
  }
  if (operation.text !== undefined) {
    entries.push(`"text":${canonicalStringify(operation.text)}`);
  }
  if (operation.x !== undefined) {
    entries.push(`"x":${encodeFloat(operation.x)}`);
  }
  if (operation.y !== undefined) {
    entries.push(`"y":${encodeFloat(operation.y)}`);
  }
  return `{${entries.join(",")}}`;
}

/**
 * Encode a number the way the Rust serializer encodes a float.
 *
 * A float always carries a decimal point, so a whole number is written with
 * a `.0` behind it. A number that the JSON encoding writes with an
 * exponent keeps that exponent, the way the Rust serializer writes it.
 */
function encodeFloat(value: number): string {
  if (!Number.isFinite(value)) {
    throw new Error("canonicalPlanJson: numbers must be finite");
  }
  if (Object.is(value, -0)) {
    return "-0.0";
  }
  const text = JSON.stringify(value);
  if (text.includes("e") || text.includes("E")) {
    return text;
  }
  return Number.isInteger(value) ? `${text}.0` : text;
}

/** Encode a number the way the Rust serializer encodes a whole number. */
function encodeWholeNumber(value: number): string {
  if (!Number.isFinite(value)) {
    throw new Error("canonicalPlanJson: numbers must be finite");
  }
  return JSON.stringify(value);
}

/** Whether an identifier carries a usable value. */
function isFilled(value: string | undefined): boolean {
  return value !== undefined && value.length > 0;
}

/** Whether a node type is one the write path may create. */
function isKnownNodeType(value: string): boolean {
  return (WRITE_NODE_TYPES as readonly string[]).includes(value);
}

/** Whether a property is on the allowlist. */
function isAllowedProperty(name: string): boolean {
  return (ALLOWED_WRITE_PROPERTIES as readonly string[]).includes(name);
}
