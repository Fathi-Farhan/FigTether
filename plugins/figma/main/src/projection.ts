/**
 * Field projection.
 *
 * The daemon may ask for a subset of the node fields so a response stays
 * inside the response budget. Omitted fields are never silently missing:
 * every omitted field is listed in `unavailable_fields` with the reason
 * `projection_omitted`, exactly like `docs/04-data-contracts/04-normalized-model.md`
 * section 4 requires.
 */

import { UNAVAILABLE_REASON, type PluginNode } from "./types.js";

/** Canonical name of every projectable field. */
export const NODE_FIELDS = [
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
] as const;

/** One projectable field name. */
export type NodeField = (typeof NODE_FIELDS)[number];

/** Fields that are always present, whatever the caller asked for. */
export const ALWAYS_PRESENT_FIELDS: readonly NodeField[] = ["id", "node_type"];

/** Default summary projection used by `list_nodes`. */
export const DEFAULT_SUMMARY_FIELDS: readonly NodeField[] = [
  "id",
  "node_type",
  "name",
  "visible",
  "locked",
  "children",
];

const ALIASES: Readonly<Record<string, NodeField>> = {
  id: "id",
  parent: "parent_id",
  parent_id: "parent_id",
  type: "node_type",
  node_type: "node_type",
  name: "name",
  visible: "visible",
  locked: "locked",
  children: "children",
  geometry: "geometry",
  layout: "layout",
  text: "text",
  typography: "text",
  fills: "fills",
  strokes: "strokes",
  effects: "effects",
  component: "component",
  variables: "variables",
  styles: "styles",
};

/**
 * Normalize a requested field list.
 *
 * Returns `null` when the caller did not ask for a projection, which means
 * every field is included. Unknown field names are dropped.
 */
export function normalizeFieldList(fields: unknown): NodeField[] | null {
  if (fields === undefined || fields === null) {
    return null;
  }
  if (!Array.isArray(fields)) {
    return null;
  }
  const selected: NodeField[] = [];
  for (const entry of fields) {
    if (typeof entry !== "string") {
      continue;
    }
    const canonical = ALIASES[entry];
    if (canonical === undefined) {
      continue;
    }
    if (!selected.includes(canonical)) {
      selected.push(canonical);
    }
  }
  if (selected.length === 0) {
    return null;
  }
  return selected;
}

/** Whether a field is part of the projection. */
export function isProjected(field: NodeField, projection: readonly NodeField[] | null): boolean {
  if (projection === null) {
    return true;
  }
  if (ALWAYS_PRESENT_FIELDS.includes(field)) {
    return true;
  }
  return projection.includes(field);
}

/**
 * Project a node onto the requested fields.
 *
 * The identity fields `id` and `node_type` always survive so the daemon can
 * still correlate the node.
 */
export function projectNode(node: PluginNode, projection: readonly NodeField[] | null): PluginNode {
  if (projection === null) {
    return node;
  }
  const omitted: string[] = [];
  const result: PluginNode = {
    id: node.id,
    node_type: node.node_type,
    name: "",
    visible: true,
    locked: false,
    children: [],
    fills: [],
    strokes: [],
    effects: [],
    variables: [],
    styles: [],
    unavailable_fields: [...node.unavailable_fields],
  };

  for (const field of NODE_FIELDS) {
    if (isProjected(field, projection)) {
      continue;
    }
    omitted.push(`${field}:${UNAVAILABLE_REASON.PROJECTION_OMITTED}`);
  }

  if (isProjected("parent_id", projection) && node.parent_id !== undefined) {
    result.parent_id = node.parent_id;
  }
  if (isProjected("name", projection)) {
    result.name = node.name;
  }
  if (isProjected("visible", projection)) {
    result.visible = node.visible;
  }
  if (isProjected("locked", projection)) {
    result.locked = node.locked;
  }
  if (isProjected("children", projection)) {
    result.children = [...node.children];
  }
  if (isProjected("geometry", projection) && node.geometry !== undefined) {
    result.geometry = node.geometry;
  }
  if (isProjected("layout", projection) && node.layout !== undefined) {
    result.layout = node.layout;
  }
  if (isProjected("text", projection) && node.text !== undefined) {
    result.text = node.text;
  }
  if (isProjected("fills", projection)) {
    result.fills = [...node.fills];
  }
  if (isProjected("strokes", projection)) {
    result.strokes = [...node.strokes];
  }
  if (isProjected("effects", projection)) {
    result.effects = [...node.effects];
  }
  if (isProjected("component", projection) && node.component !== undefined) {
    result.component = node.component;
  }
  if (isProjected("variables", projection)) {
    result.variables = [...node.variables];
  }
  if (isProjected("styles", projection)) {
    result.styles = [...node.styles];
  }

  result.unavailable_fields = [...result.unavailable_fields, ...omitted];
  return result;
}
