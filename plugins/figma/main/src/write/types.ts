/**
 * Contract of the write path of the plugin main sandbox.
 *
 * The wire shapes live in the shared package `@figtether/shared`, which
 * mirrors the Rust contract of the write path. This module builds on them:
 * the operations carry the fields the plugin needs, the actions are the
 * concrete mutations of a plan, and the preview and the result add the
 * fields the plugin reports to the daemon.
 *
 * The rule of the shared package is followed here as well: an optional field
 * is absent rather than null, so the plan hash of the two sides covers the
 * same bytes.
 */

import type {
  WriteOperation as SharedWriteOperation,
  WritePlan as SharedWritePlan,
  WritePreview as SharedWritePreview,
  WriteResult as SharedWriteResult,
} from "@figtether/shared";
import { WRITE_NODE_TYPES } from "@figtether/shared";
import type { JsonValue } from "@figtether/shared";

import type { FigmaFontName, FigmaPaint } from "../figma-api.js";

/**
 * The node types a plan may create.
 *
 * The list is the one of the shared contract. A group is part of it under a
 * creation call of its own: the editor builds a group from the nodes it takes
 * over, because it does not support an empty group.
 */
export const CREATE_NODE_TYPES = WRITE_NODE_TYPES;

/** One node type a plan may create. */
export type CreateNodeType = (typeof CREATE_NODE_TYPES)[number];

/** The operations a plan may contain, in wire order. */
export const WRITE_OPERATION_KINDS = [
  "create_node",
  "insert_child",
  "set_properties",
  "set_text",
  "set_fills",
  "delete_node",
  "move_node",
] as const;

/** One operation kind of a plan. */
export type WriteOperationKind = (typeof WRITE_OPERATION_KINDS)[number];

/**
 * The bridge operations of the write path.
 *
 * The read operations live in the shared contract. The write operations are
 * added by the write path, because the shared contract does not name them
 * yet.
 */
export const WRITE_OPS = ["preview_write", "apply_write"] as const;

/** One bridge operation of the write path. */
export type WriteOp = (typeof WRITE_OPS)[number];

/**
 * A reference to a node of the document.
 *
 * A reference that starts with a hash names a node the same plan creates
 * earlier, for example `#2` for the node the third operation creates. The
 * reference of a node that does not exist yet cannot be a document id, so the
 * plan uses the alias and the apply reports the real id.
 */
export type NodeRef = string;

/** Create a node of a type under a parent. */
export interface CreateNodeOperation extends SharedWriteOperation {
  /** The kind of the operation. */
  kind: "create_node";
  /** The parent the node is appended to. */
  parent_id: string;
  /** The type of the node. */
  node_type: CreateNodeType;
  /** The allowlisted properties of the new node. */
  properties: Record<string, JsonValue>;
  /** The nodes a group takes over, empty for every other type. */
  children_ids: string[];
}

/** Move an existing node under another parent. */
export interface InsertChildOperation extends SharedWriteOperation {
  /** The kind of the operation. */
  kind: "insert_child";
  /** The parent the node is appended to. */
  parent_id: string;
  /** The node that moves. */
  node_id: string;
  /** The index inside the parent, appended at the end when it is absent. */
  index?: number;
}

/** Set allowlisted properties on an existing node. */
export interface SetPropertiesOperation extends SharedWriteOperation {
  /** The kind of the operation. */
  kind: "set_properties";
  /** The node that changes. */
  node_id: string;
  /** The allowlisted properties and their values. */
  properties: Record<string, JsonValue>;
}

/** Replace the characters of a text node. */
export interface SetTextOperation extends SharedWriteOperation {
  /** The kind of the operation. */
  kind: "set_text";
  /** The text node that changes. */
  node_id: string;
  /** The new characters. */
  text: string;
  /** The family of the font the characters are written in. */
  font_family?: string;
  /** The style of the font the characters are written in. */
  font_style?: string;
}

/** Replace the fills of a node. */
export interface SetFillsOperation extends Omit<SharedWriteOperation, "fills"> {
  /** The kind of the operation. */
  kind: "set_fills";
  /** The node that changes. */
  node_id: string;
  /**
   * The paints of the plan, as the plan carried them.
   *
   * The dedicated field of the operation is the only place a paint travels:
   * fills are not a property, so they are not on the property allowlist. The
   * values stay raw here, because the plan hash covers exactly these bytes,
   * and the resolver normalizes them before they reach the editor.
   */
  fills: JsonValue[];
}

/** Remove a node and its subtree. */
export interface DeleteNodeOperation extends SharedWriteOperation {
  /** The kind of the operation. */
  kind: "delete_node";
  /** The node that is removed. */
  node_id: string;
}

/** Change the position of a node. */
export interface MoveNodeOperation extends SharedWriteOperation {
  /** The kind of the operation. */
  kind: "move_node";
  /** The node that moves. */
  node_id: string;
  /** The new position on the horizontal axis. */
  x?: number;
  /** The new position on the vertical axis. */
  y?: number;
}

/** One operation of a plan. */
export type WriteOperation =
  | CreateNodeOperation
  | InsertChildOperation
  | SetPropertiesOperation
  | SetTextOperation
  | SetFillsOperation
  | DeleteNodeOperation
  | MoveNodeOperation;

/** A plan, as the daemon submits it. */
export interface WritePlan extends SharedWritePlan {
  /** The operations of the plan, in the order they run. */
  operations: WriteOperation[];
}

/**
 * A plan as it arrived, with the approval and the hash that travel with it.
 *
 * The approval and the hash are not part of the plan: they bind the
 * approval to exactly the plan the user was shown.
 */
export interface ParsedWritePlan {
  /** The plan itself. */
  plan: WritePlan;
  /** The approval the user has to grant, when the daemon minted one. */
  approvalId: string | null;
  /** The hash the daemon computed, when it sent one. */
  planHash: string | null;
}

/**
 * The error codes of the write path.
 *
 * They extend the taxonomy of the plugin with the codes of the write path
 * design, section 2.6.
 */
export const WRITE_ERROR_CODES = {
  /** The session has no write grant. */
  WRITE_NOT_GRANTED: "WRITE_NOT_GRANTED",
  /** The plan is structurally invalid. */
  PLAN_INVALID: "PLAN_INVALID",
  /** The plan differs from the plan the user approved. */
  PLAN_HASH_MISMATCH: "PLAN_HASH_MISMATCH",
  /** The plan exceeds one of the budgets. */
  BUDGET_EXCEEDED: "BUDGET_EXCEEDED",
  /** A destructive operation without the destructive flag. */
  DESTRUCTIVE_NOT_ALLOWED: "DESTRUCTIVE_NOT_ALLOWED",
  /** The editor refused the mutation. */
  MUTATION_REFUSED: "MUTATION_REFUSED",
  /** A text change needs a font the editor cannot load. */
  FONT_UNAVAILABLE: "FONT_UNAVAILABLE",
  /** A node of the plan is outside the granted roots. */
  OUT_OF_SCOPE: "OUT_OF_SCOPE",
  /** A node of the plan does not exist. */
  NODE_NOT_FOUND: "NODE_NOT_FOUND",
  /** The plan was not approved. */
  APPROVAL_REQUIRED: "APPROVAL_REQUIRED",
} as const;

/** One error code of the write path. */
export type WriteErrorCode = (typeof WRITE_ERROR_CODES)[keyof typeof WRITE_ERROR_CODES];

/**
 * The reasons a mutation can be refused.
 *
 * The vocabulary is closed, so a caller can react to a reason without
 * parsing a message.
 */
export const WRITE_ERROR_REASONS = {
  /** The plan is not a valid plan. */
  INVALID_PLAN: "invalid_plan",
  /** The plan differs from the plan the user approved. */
  HASH_MISMATCH: "plan_hash_mismatch",
  /** The plan names a property that is not allowlisted. */
  UNKNOWN_PROPERTY: "unknown_property",
  /** The plan names a node that does not exist. */
  UNKNOWN_NODE: "unknown_node",
  /** The plan names a node outside the granted roots. */
  NODE_OUT_OF_SCOPE: "node_out_of_scope",
  /** The plan exceeds a budget. */
  BUDGET_EXCEEDED: "budget_exceeded",
  /** The plan holds a delete or an insert without the destructive flag. */
  DESTRUCTIVE_NOT_ALLOWED: "destructive_not_allowed",
  /** The parent is a descendant of the child, so the move is a cycle. */
  PARENTING_CYCLE: "parenting_cycle",
  /** The node is read only. */
  READONLY_NODE: "readonly_node",
  /** The node lives inside an instance, so it cannot move. */
  INSTANCE_CHILD: "instance_child",
  /** The move would put a component inside itself. */
  COMPONENT_CYCLE: "component_cycle",
  /** The parent cannot hold children. */
  PARENT_CANT_HOLD_CHILDREN: "parent_cant_hold_children",
  /** The index of an insert is outside the parent. */
  INDEX_OUT_OF_RANGE: "index_out_of_range",
  /** The editor does not expose the call the operation needs. */
  CAPABILITY_UNAVAILABLE: "capability_unavailable",
  /** The plan tries to delete a granted root. */
  GRANTED_ROOT_DELETE: "granted_root_delete",
  /** The font of a text change cannot be loaded. */
  FONT_UNAVAILABLE: "font_unavailable",
  /** The paint payload cannot be normalized. */
  INVALID_PAINT: "invalid_paint",
  /** The editor refused the operation for a reason the plugin does not know. */
  EDITOR_REFUSED: "editor_refused",
} as const;

/** One reason of the write path. */
export type WriteErrorReason = (typeof WRITE_ERROR_REASONS)[keyof typeof WRITE_ERROR_REASONS];

/** One structured failure of the write path. */
export interface WriteErrorDetail {
  /** The error code. */
  code: WriteErrorCode;
  /** The machine readable reason. */
  reason: WriteErrorReason;
  /** A message without design content. */
  message: string;
  /** Whether the caller may retry the same plan. */
  retryable: boolean;
  /** The recovery hint. */
  action: string;
  /** The operation of the plan that failed, when one is known. */
  operation: number | null;
}

/** One planned creation of a preview. */
export interface PlannedCreation {
  /** The alias the created node can be referenced by. */
  ref: NodeRef;
  /** The parent the node is created under. */
  parent_id: string;
  /** The type of the node. */
  node_type: CreateNodeType;
  /** The name of the node, when the plan names one. */
  name: string | null;
}

/** What a plan would do, resolved against the live document. */
export interface WritePreview extends SharedWritePreview {
  /**
   * The number of nodes the plan would create.
   *
   * A node that does not exist yet has no id, so the preview leaves `created`
   * empty and reports the planned creations in `created_plan` instead. The
   * apply reports the real ids.
   */
  created_count: number;
  /** The creations of the plan, with the parent they land in. */
  created_plan: PlannedCreation[];
  /**
   * The reasons the plan is blocked.
   *
   * The reasons are plain sentences, so the interface can show them without
   * knowing the structure of a failure. The structured failures of the
   * resolver carry the same reasons with their codes.
   */
  reasons: string[];
}

/** One property assignment of an action. */
export interface PropertyChange {
  /** The allowlisted name of the property. */
  name: string;
  /** The property of the editor the name maps to. */
  path: string;
  /** The value that is assigned. */
  value: JsonValue;
  /** Whether the font of the node has to be loaded first. */
  requiresFont: boolean;
  /** The font that has to be loaded, when one is needed. */
  font: FigmaFontName | null;
}

/** Create a node under a parent. */
export interface CreateNodeAction {
  /** Validated initial styling, applied before reporting the created id. */
  initialFills?: FigmaPaint[];
  initialText?: string;
  font?: FigmaFontName;
  /** The kind of the action. */
  kind: "create_node";
  /** The index of the operation in the plan. */
  operation: number;
  /** The alias of the created node. */
  ref: NodeRef;
  /** The parent of the created node. */
  parentRef: NodeRef;
  /** The type of the created node. */
  nodeType: CreateNodeType;
  /** The properties of the created node. */
  changes: PropertyChange[];
  /** The nodes a group takes over. */
  childRefs: NodeRef[];
}

/** Move an existing node under another parent. */
export interface InsertChildAction {
  /** The kind of the action. */
  kind: "insert_child";
  /** The index of the operation in the plan. */
  operation: number;
  /** The parent of the node. */
  parentRef: NodeRef;
  /** The node that moves. */
  childRef: NodeRef;
  /** The index inside the parent. */
  index: number | null;
}

/** Set properties on a node. */
export interface SetPropertiesAction {
  /** The kind of the action. */
  kind: "set_properties";
  /** The index of the operation in the plan. */
  operation: number;
  /** The node that changes. */
  nodeRef: NodeRef;
  /** The properties that change. */
  changes: PropertyChange[];
}

/** Replace the characters of a text node. */
export interface SetTextAction {
  /** The kind of the action. */
  kind: "set_text";
  /** The index of the operation in the plan. */
  operation: number;
  /** The text node that changes. */
  nodeRef: NodeRef;
  /** The new characters. */
  characters: string;
  /** The font the characters are written in. */
  font: FigmaFontName;
  /** The font size, when the plan names one. */
  fontSize: number | null;
}

/** Replace the fills of a node. */
export interface SetFillsAction {
  /** The kind of the action. */
  kind: "set_fills";
  /** The index of the operation in the plan. */
  operation: number;
  /** The node that changes. */
  nodeRef: NodeRef;
  /** The normalized paints. */
  fills: FigmaPaint[];
}

/** Remove a node. */
export interface DeleteNodeAction {
  /** The kind of the action. */
  kind: "delete_node";
  /** The index of the operation in the plan. */
  operation: number;
  /** The node that is removed. */
  nodeRef: NodeRef;
  /** The number of nodes the removal takes with it. */
  subtreeSize: number;
}

/** Change the position of a node. */
export interface MoveNodeAction {
  /** The kind of the action. */
  kind: "move_node";
  /** The index of the operation in the plan. */
  operation: number;
  /** The node that moves. */
  nodeRef: NodeRef;
  /** The new position on the horizontal axis. */
  x: number | null;
  /** The new position on the vertical axis. */
  y: number | null;
}

/** One concrete mutation of a plan. */
export type WriteAction =
  | CreateNodeAction
  | InsertChildAction
  | SetPropertiesAction
  | SetTextAction
  | SetFillsAction
  | DeleteNodeAction
  | MoveNodeAction;

/** The resolved plan: what it would do and what it would change. */
export interface ResolvedPlan {
  /** The preview of the plan. */
  preview: WritePreview;
  /** The concrete mutations, in the order they run. */
  actions: WriteAction[];
  /**
   * The structured reasons the plan was blocked by.
   *
   * The preview carries the same reasons as plain sentences, and this list
   * carries them with their codes, so the apply can refuse a plan with the
   * code of its first reason.
   */
  reasons: WriteErrorDetail[];
}

/** The outcome of an apply. */
export interface ApplyOutcome {
  /** Whether every action ran, or only the first ones. */
  status: "applied" | "partial";
  /** The ids of the nodes that exist after the apply. */
  created: string[];
  /** The ids of the nodes that changed. */
  changed: string[];
  /** The ids of the nodes that were removed. */
  removed: string[];
  /** The ids of the nodes that moved. */
  moved: string[];
  /** The number of actions that ran. */
  applied_operations: number;
  /** The failure that stopped the plan, when one did. */
  error: WriteErrorDetail | null;
  /** Whether the undo history was committed. */
  undo_committed: boolean;
  /** The id of the version checkpoint, when one was written. */
  version_saved: string | null;
  /** The warnings of the apply. */
  warnings: string[];
}

/** What an apply reports to the daemon. */
export interface WriteResult extends SharedWriteResult {
  /** Whether every action ran, or only the first ones. */
  status: "applied" | "partial";
  /** The number of actions of the plan. */
  operations_count: number;
  /** The number of actions that ran. */
  applied_operations: number;
  /** Whether the plan asked for a version checkpoint. */
  version_requested: boolean;
  /** Whether the editor exposes the version checkpoint call. */
  version_available: boolean;
  /** Why there is no version id, when there is none. */
  version_note: string | null;
  /** The failure that stopped the plan, when one did. */
  error: WriteErrorDetail | null;
}
