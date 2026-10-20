/**
 * Parsing of a write plan.
 *
 * The plan arrives from the daemon as untrusted JSON, so it is parsed into
 * the shape of the shared contract before anything else happens. A plan that
 * does not fit the contract is refused as a whole: the plugin never applies
 * half of a plan it did not understand.
 *
 * The parse is strict about the structure and tolerant about the spelling:
 * an operation names its kind in `kind`, `op` or `type`, and a text
 * operation accepts its characters in `text` or in `characters`.
 *
 * The tolerance never widens what the plugin applies without a word from
 * the daemon: the daemon hashes the plan it read, and the plugin hashes the
 * plan it parsed. A plan that used another spelling hashes differently, so
 * the comparison of the two hashes refuses it.
 */

import { assertJsonValue, canonicalPlanJson, sha256HexOfText } from "@figtether/shared";
import type { JsonValue } from "@figtether/shared";

import { fail } from "./errors.js";
import {
  CREATE_NODE_TYPES,
  WRITE_ERROR_CODES,
  WRITE_ERROR_REASONS,
  WRITE_OPERATION_KINDS,
  type CreateNodeOperation,
  type CreateNodeType,
  type DeleteNodeOperation,
  type InsertChildOperation,
  type MoveNodeOperation,
  type ParsedWritePlan,
  type SetFillsOperation,
  type SetPropertiesOperation,
  type SetTextOperation,
  type WriteOperation,
  type WriteOperationKind,
  type WritePlan,
} from "./types.js";

/**
 * Parse the payload of a write request into a plan.
 *
 * The payload is either the plan itself or an object that carries the plan
 * under `plan`. The approval and the hash of the plan travel next to the
 * plan, so they are read from the same place.
 */
export function parseWritePlan(payload: unknown): ParsedWritePlan {
  const record = asRecord(payload);
  if (record === null) {
    invalid("The write request does not carry a plan");
  }
  const source = asRecord(record["plan"]) ?? record;
  const planId = readString(source["plan_id"]);
  if (planId === undefined || planId.length === 0) {
    invalid("The plan has no id");
  }
  const operations = readOperations(source["operations"]);
  const sessionId = readString(source["session_id"]);
  const intent = readString(source["intent"]);
  const destructive = source["destructive"];
  if (destructive !== undefined && typeof destructive !== "boolean") {
    invalid("The destructive flag of the plan is not a boolean");
  }
  const plan: WritePlan = {
    plan_id: planId,
    destructive: destructive === true,
    operations,
  };
  if (sessionId !== undefined) {
    plan.session_id = sessionId;
  }
  if (intent !== undefined) {
    plan.intent = intent;
  }
  return {
    plan,
    approvalId: readString(record["approval_id"]) ?? null,
    planHash: readString(record["plan_hash"]) ?? null,
  };
}

/** Parse the operations of a plan. */
function readOperations(value: unknown): WriteOperation[] {
  if (!Array.isArray(value)) {
    invalid("The plan holds no operations");
  }
  const operations: WriteOperation[] = [];
  for (const entry of value) {
    operations.push(readOperation(entry));
  }
  return operations;
}

/** Parse one operation of a plan. */
function readOperation(value: unknown): WriteOperation {
  const record = asRecord(value);
  if (record === null) {
    invalid("One operation of the plan is not an object");
  }
  const kind = readOperationKind(record);
  switch (kind) {
    case "create_node":
      return readCreateNode(record);
    case "insert_child":
      return readInsertChild(record);
    case "set_properties":
      return readSetProperties(record);
    case "set_text":
      return readSetText(record);
    case "set_fills":
      return readSetFills(record);
    case "delete_node":
      return readDeleteNode(record);
    case "move_node":
      return readMoveNode(record);
    default:
      invalid("One operation of the plan has an unknown kind");
  }
}

/** Read the kind of an operation. */
function readOperationKind(record: Record<string, unknown>): WriteOperationKind {
  const raw = record["kind"] ?? record["op"] ?? record["type"];
  if (typeof raw !== "string") {
    invalid("One operation of the plan has no kind");
  }
  const kind = raw.trim().toLowerCase();
  if (!(WRITE_OPERATION_KINDS as readonly string[]).includes(kind)) {
    invalid("One operation of the plan has an unknown kind");
  }
  return kind as WriteOperationKind;
}

/** Read a create operation. */
function readCreateNode(record: Record<string, unknown>): CreateNodeOperation {
  const parentId = readRequiredString(record, "parent_id");
  const nodeType = readNodeType(record["node_type"]);
  const properties = readProperties(record);
  const childrenIds = readStringArray(record["children_ids"]) ?? [];
  const operation: CreateNodeOperation = {
    kind: "create_node",
    parent_id: parentId,
    node_type: nodeType,
    properties,
    children_ids: childrenIds,
  };
  const x = readNumber(record["x"]);
  if (x !== undefined) {
    operation.x = x;
  }
  const y = readNumber(record["y"]);
  if (y !== undefined) {
    operation.y = y;
  }
  return operation;
}

/** Read the type of a created node. */
function readNodeType(value: unknown): CreateNodeType {
  if (typeof value !== "string") {
    invalid("One create operation of the plan has no node type");
  }
  const normalized = value.trim().toUpperCase();
  if (!(CREATE_NODE_TYPES as readonly string[]).includes(normalized)) {
    invalid("One create operation of the plan uses a node type the plugin cannot create");
  }
  return normalized as CreateNodeType;
}

/** Read an insert operation. */
function readInsertChild(record: Record<string, unknown>): InsertChildOperation {
  const parentId = readRequiredString(record, "parent_id");
  const nodeId = readRequiredString(record, "node_id");
  const index = readNumber(record["index"]);
  if (index !== undefined && (!Number.isInteger(index) || index < 0)) {
    invalid("The index of an insert operation is not a positive whole number");
  }
  const operation: InsertChildOperation = {
    kind: "insert_child",
    parent_id: parentId,
    node_id: nodeId,
  };
  if (index !== undefined) {
    operation.index = index;
  }
  return operation;
}

/** Read a property operation. */
function readSetProperties(record: Record<string, unknown>): SetPropertiesOperation {
  const nodeId = readRequiredString(record, "node_id");
  const properties = readProperties(record);
  return { kind: "set_properties", node_id: nodeId, properties };
}

/** Read the properties of an operation. */
function readProperties(record: Record<string, unknown>): Record<string, JsonValue> {
  const declared = asRecord(record["properties"]);
  if (declared === null) {
    return {};
  }
  const properties: Record<string, JsonValue> = {};
  for (const [key, value] of Object.entries(declared)) {
    properties[key] = readJsonValue(value, key);
  }
  return properties;
}

/** Read a text operation. */
function readSetText(record: Record<string, unknown>): SetTextOperation {
  const nodeId = readRequiredString(record, "node_id");
  const text = record["text"] ?? record["characters"];
  if (typeof text !== "string") {
    invalid("One text operation of the plan carries no characters");
  }
  const operation: SetTextOperation = { kind: "set_text", node_id: nodeId, text };
  const family = readString(record["font_family"]);
  const style = readString(record["font_style"]);
  if (family !== undefined && style !== undefined) {
    operation.font_family = family;
    operation.font_style = style;
  }
  const font = asRecord(record["font"]);
  if (font !== null) {
    const fontFamily = readString(font["family"]);
    const fontStyle = readString(font["style"]);
    if (fontFamily === undefined || fontStyle === undefined) {
      invalid("The font of a text operation names no family and no style");
    }
    operation.font_family = fontFamily;
    operation.font_style = fontStyle;
  }
  return operation;
}

/**
 * Read a fill operation.
 *
 * The paints travel in the dedicated field of the operation. Fills are not a
 * property, so they are not on the property allowlist, and a plan that puts
 * them into the properties is refused as an unknown property.
 */
function readSetFills(record: Record<string, unknown>): SetFillsOperation {
  const nodeId = readRequiredString(record, "node_id");
  const raw = record["fills"];
  if (!Array.isArray(raw)) {
    invalid("One fill operation of the plan carries no paints");
  }
  const fills: JsonValue[] = [];
  for (const entry of raw) {
    fills.push(readJsonValue(entry, "fills"));
  }
  return { kind: "set_fills", node_id: nodeId, fills };
}

/** Read a delete operation. */
function readDeleteNode(record: Record<string, unknown>): DeleteNodeOperation {
  const nodeId = readRequiredString(record, "node_id");
  return { kind: "delete_node", node_id: nodeId };
}

/** Read a move operation. */
function readMoveNode(record: Record<string, unknown>): MoveNodeOperation {
  const nodeId = readRequiredString(record, "node_id");
  const x = readNumber(record["x"]);
  const y = readNumber(record["y"]);
  if (x === undefined && y === undefined) {
    invalid("One move operation of the plan names no position");
  }
  const operation: MoveNodeOperation = { kind: "move_node", node_id: nodeId };
  if (x !== undefined) {
    operation.x = x;
  }
  if (y !== undefined) {
    operation.y = y;
  }
  return operation;
}

/**
 * The hash of a plan.
 *
 * The hash is the one of the shared contract, so the daemon and the plugin
 * bind an approval to the same bytes.
 */
export function planHash(plan: WritePlan): string {
  return `sha256:${sha256HexOfText(canonicalPlanJson(plan))}`;
}

/** Refuse a plan that does not fit the contract. */
function invalid(message: string): never {
  fail(WRITE_ERROR_CODES.PLAN_INVALID, WRITE_ERROR_REASONS.INVALID_PLAN, message);
}

/** Narrow a value to a plain record. */
function asRecord(value: unknown): Record<string, unknown> | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return null;
  }
  return value as Record<string, unknown>;
}

/** Read a string of a record. */
function readString(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

/** Read a number of a record. */
function readNumber(value: unknown): number | undefined {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return undefined;
  }
  return value;
}

/** Read a required string of a record. */
function readRequiredString(record: Record<string, unknown>, key: string): string {
  const value = readString(record[key]);
  if (value === undefined || value.length === 0) {
    invalid(`The operation of the plan needs the parameter ${key}`);
  }
  return value;
}

/** Read a JSON value of a record, refusing a value that is not one. */
function readJsonValue(value: unknown, key: string): JsonValue {
  try {
    return assertJsonValue(value);
  } catch {
    invalid(`The value of the property ${key} is not a JSON value`);
  }
}

/** Read a list of strings of a record. */
function readStringArray(value: unknown): string[] | undefined {
  if (!Array.isArray(value)) {
    return undefined;
  }
  const result: string[] = [];
  for (const entry of value) {
    if (typeof entry !== "string") {
      return undefined;
    }
    result.push(entry);
  }
  return result;
}
