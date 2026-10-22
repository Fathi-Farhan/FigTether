/**
 * The closed property allowlist of the write path.
 *
 * The list is closed on purpose: a property outside the list is a plan error
 * and never a best effort, because a design system needs the geometry, the
 * size, the layout, the appearance and the naming of a node, and nothing
 * that changes the structure of the document implicitly.
 *
 * The names are the names of the shared contract, so the daemon and the
 * plugin refuse the same properties. The plugin adds the properties of the
 * text, because the font of a text node is written through them.
 *
 * The paint payloads are normalized here as well. A raw payload of the agent
 * never reaches the editor: it is parsed, clamped and rebuilt first.
 */

import type { JsonValue } from "@figtether/shared";

import type { FigmaColor, FigmaGradientStop, FigmaPaint } from "../figma-api.js";
import { fail } from "./errors.js";
import { WRITE_ERROR_CODES, WRITE_ERROR_REASONS } from "./types.js";

/** The kind of value a property accepts. */
export type PropertyKind = "number" | "string" | "boolean" | "size";

/** One entry of the property allowlist. */
export interface PropertySpec {
  /** The allowlisted name of the property. */
  name: string;
  /** The property of the editor the name maps to. */
  path: string;
  /** The kind of value the property accepts. */
  kind: PropertyKind;
  /** The node types that support the property. */
  nodeTypes: readonly string[];
  /** The accepted values, when the property is an enumeration. */
  values?: readonly string[];
  /** Whether the font of the node has to be loaded before the write. */
  requiresFont: boolean;
}

/** The node types that carry an auto layout. */
const FRAME_LIKE: readonly string[] = ["FRAME", "COMPONENT", "COMPONENT_SET", "INSTANCE"];

/** The node types that carry a corner radius. */
const ROUNDED: readonly string[] = [
  "FRAME",
  "COMPONENT",
  "COMPONENT_SET",
  "INSTANCE",
  "RECTANGLE",
  "ELLIPSE",
  "VECTOR",
];

/** The node types that carry text. */
const TEXT_LIKE: readonly string[] = ["TEXT"];

/** The node types that carry every property. */
const EVERY: readonly string[] = ["*"];

/** The blend modes of the editor. */
const BLEND_MODES: readonly string[] = [
  "PASS_THROUGH",
  "NORMAL",
  "DARKEN",
  "MULTIPLY",
  "LINEAR_BURN",
  "COLOR_BURN",
  "LIGHTEN",
  "SCREEN",
  "LINEAR_DODGE",
  "COLOR_DODGE",
  "OVERLAY",
  "SOFT_LIGHT",
  "HARD_LIGHT",
  "DIFFERENCE",
  "EXCLUSION",
  "HUE",
  "SATURATION",
  "COLOR",
  "LUMINOSITY",
];

/**
 * Every property a plan may write.
 *
 * The list is the whole contract: the resolver refuses a plan that names
 * another property, and it skips a property the node type does not support.
 *
 * The names are the names of the Rust allowlist in
 * `crates/model/src/write.rs`, in the same order, and the list carries no
 * other name: the Rust list is the source of truth, so a property that is
 * not on it is not writable, not even here.
 */
export const WRITABLE_PROPERTIES: readonly PropertySpec[] = [
  { name: "x", path: "x", kind: "number", nodeTypes: EVERY, requiresFont: false },
  { name: "y", path: "y", kind: "number", nodeTypes: EVERY, requiresFont: false },
  { name: "width", path: "width", kind: "size", nodeTypes: EVERY, requiresFont: false },
  { name: "height", path: "height", kind: "size", nodeTypes: EVERY, requiresFont: false },
  { name: "rotation", path: "rotation", kind: "number", nodeTypes: EVERY, requiresFont: false },
  { name: "opacity", path: "opacity", kind: "number", nodeTypes: EVERY, requiresFont: false },
  { name: "visible", path: "visible", kind: "boolean", nodeTypes: EVERY, requiresFont: false },
  { name: "locked", path: "locked", kind: "boolean", nodeTypes: EVERY, requiresFont: false },
  { name: "name", path: "name", kind: "string", nodeTypes: EVERY, requiresFont: false },
  {
    name: "fontSize",
    path: "fontSize",
    kind: "number",
    nodeTypes: TEXT_LIKE,
    requiresFont: true,
  },
  {
    name: "cornerRadius",
    path: "cornerRadius",
    kind: "number",
    nodeTypes: ROUNDED,
    requiresFont: false,
  },
  {
    name: "topLeftRadius",
    path: "topLeftRadius",
    kind: "number",
    nodeTypes: ROUNDED,
    requiresFont: false,
  },
  {
    name: "topRightRadius",
    path: "topRightRadius",
    kind: "number",
    nodeTypes: ROUNDED,
    requiresFont: false,
  },
  {
    name: "bottomLeftRadius",
    path: "bottomLeftRadius",
    kind: "number",
    nodeTypes: ROUNDED,
    requiresFont: false,
  },
  {
    name: "bottomRightRadius",
    path: "bottomRightRadius",
    kind: "number",
    nodeTypes: ROUNDED,
    requiresFont: false,
  },
  {
    name: "layoutMode",
    path: "layoutMode",
    kind: "string",
    nodeTypes: FRAME_LIKE,
    values: ["NONE", "HORIZONTAL", "VERTICAL"],
    requiresFont: false,
  },
  {
    name: "primaryAxisAlignItems",
    path: "primaryAxisAlignItems",
    kind: "string",
    nodeTypes: FRAME_LIKE,
    values: ["MIN", "CENTER", "MAX", "SPACE_BETWEEN"],
    requiresFont: false,
  },
  {
    name: "counterAxisAlignItems",
    path: "counterAxisAlignItems",
    kind: "string",
    nodeTypes: FRAME_LIKE,
    values: ["MIN", "CENTER", "MAX", "BASELINE"],
    requiresFont: false,
  },
  {
    name: "layoutSizingHorizontal",
    path: "layoutSizingHorizontal",
    kind: "string",
    nodeTypes: FRAME_LIKE,
    values: ["FIXED", "HUG", "FILL"],
    requiresFont: false,
  },
  {
    name: "layoutSizingVertical",
    path: "layoutSizingVertical",
    kind: "string",
    nodeTypes: FRAME_LIKE,
    values: ["FIXED", "HUG", "FILL"],
    requiresFont: false,
  },
  {
    name: "itemSpacing",
    path: "itemSpacing",
    kind: "number",
    nodeTypes: FRAME_LIKE,
    requiresFont: false,
  },
  {
    name: "paddingLeft",
    path: "paddingLeft",
    kind: "number",
    nodeTypes: FRAME_LIKE,
    requiresFont: false,
  },
  {
    name: "paddingRight",
    path: "paddingRight",
    kind: "number",
    nodeTypes: FRAME_LIKE,
    requiresFont: false,
  },
  {
    name: "paddingTop",
    path: "paddingTop",
    kind: "number",
    nodeTypes: FRAME_LIKE,
    requiresFont: false,
  },
  {
    name: "paddingBottom",
    path: "paddingBottom",
    kind: "number",
    nodeTypes: FRAME_LIKE,
    requiresFont: false,
  },
  {
    name: "clipsContent",
    path: "clipsContent",
    kind: "boolean",
    nodeTypes: FRAME_LIKE,
    requiresFont: false,
  },
  {
    name: "blendMode",
    path: "blendMode",
    kind: "string",
    nodeTypes: EVERY,
    values: BLEND_MODES,
    requiresFont: false,
  },
  {
    name: "description",
    path: "description",
    kind: "string",
    nodeTypes: EVERY,
    requiresFont: false,
  },
];

/** The allowlist, indexed by the name of the property. */
const BY_NAME: ReadonlyMap<string, PropertySpec> = new Map(
  WRITABLE_PROPERTIES.map((spec) => [spec.name, spec]),
);

/**
 * Look up one property of the allowlist.
 *
 * A property outside the list is refused: the caller reports the plan as
 * invalid and nothing is applied.
 */
export function findProperty(name: string): PropertySpec | undefined {
  return BY_NAME.get(name);
}

/** Whether a node type supports a property. */
export function supportsNodeType(spec: PropertySpec, nodeType: string): boolean {
  return spec.nodeTypes.includes("*") || spec.nodeTypes.includes(nodeType);
}

/**
 * Check the value of a property.
 *
 * A value of the wrong kind or a value outside an enumeration is a plan
 * error: the editor would throw on it, and a plan that throws is not a plan.
 */
export function checkPropertyValue(spec: PropertySpec, value: JsonValue): void {
  switch (spec.kind) {
    case "number":
      if (typeof value !== "number" || !Number.isFinite(value)) {
        fail(
          WRITE_ERROR_CODES.PLAN_INVALID,
          WRITE_ERROR_REASONS.INVALID_PLAN,
          `The property ${spec.name} needs a finite number`,
        );
      }
      return;
    case "size":
      if (typeof value !== "number" || !Number.isFinite(value) || value < 0) {
        fail(
          WRITE_ERROR_CODES.PLAN_INVALID,
          WRITE_ERROR_REASONS.INVALID_PLAN,
          `The property ${spec.name} needs a finite number that is not negative`,
        );
      }
      return;
    case "boolean":
      if (typeof value !== "boolean") {
        fail(
          WRITE_ERROR_CODES.PLAN_INVALID,
          WRITE_ERROR_REASONS.INVALID_PLAN,
          `The property ${spec.name} needs a boolean`,
        );
      }
      return;
    case "string":
      if (typeof value !== "string") {
        fail(
          WRITE_ERROR_CODES.PLAN_INVALID,
          WRITE_ERROR_REASONS.INVALID_PLAN,
          `The property ${spec.name} needs a string`,
        );
      }
      if (spec.values !== undefined && !spec.values.includes(value)) {
        fail(
          WRITE_ERROR_CODES.PLAN_INVALID,
          WRITE_ERROR_REASONS.INVALID_PLAN,
          `The property ${spec.name} does not accept the value ${value}`,
        );
      }
      return;
    default:
      return;
  }
}

/**
 * Normalize a colour of a paint payload.
 *
 * A colour is either a hexadecimal string or an object with the channels of
 * the editor. The channels are clamped, so a payload can never carry a value
 * the editor would reject.
 */
export function normalizeColor(value: unknown): FigmaColor {
  if (typeof value === "string") {
    return colorFromHex(value);
  }
  if (typeof value === "object" && value !== null && !Array.isArray(value)) {
    const record = value as Record<string, unknown>;
    const r = numberInUnitRange(record["r"]);
    const g = numberInUnitRange(record["g"]);
    const b = numberInUnitRange(record["b"]);
    const a = record["a"] === undefined ? 1 : numberInUnitRange(record["a"]);
    if (r === undefined || g === undefined || b === undefined || a === undefined) {
      invalidPaint("The colour of the paint payload is not a valid colour");
    }
    return { r, g, b, a };
  }
  invalidPaint("The colour of the paint payload is not a valid colour");
}

/** Build a colour from a hexadecimal string. */
function colorFromHex(text: string): FigmaColor {
  const hex = text.trim().replace(/^#/, "");
  const expanded =
    hex.length === 3 || hex.length === 4
      ? hex
          .split("")
          .map((digit) => `${digit}${digit}`)
          .join("")
      : hex;
  if (!/^[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$/.test(expanded)) {
    invalidPaint("The colour of the paint payload is not a valid colour");
  }
  const red = Number.parseInt(expanded.slice(0, 2), 16) / 255;
  const green = Number.parseInt(expanded.slice(2, 4), 16) / 255;
  const blue = Number.parseInt(expanded.slice(4, 6), 16) / 255;
  const alpha = expanded.length === 8 ? Number.parseInt(expanded.slice(6, 8), 16) / 255 : 1;
  return { r: red, g: green, b: blue, a: alpha };
}

/** Narrow a number to the unit range of a colour channel. */
function numberInUnitRange(value: unknown): number | undefined {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return undefined;
  }
  return Math.min(1, Math.max(0, value));
}

/** Refuse a paint payload that cannot be normalized. */
function invalidPaint(message: string): never {
  fail(WRITE_ERROR_CODES.PLAN_INVALID, WRITE_ERROR_REASONS.INVALID_PAINT, message);
}

/**
 * Normalize the fills of a plan.
 *
 * The result is the paint list of the editor: every colour is clamped, every
 * unknown field is dropped, and a paint type the plugin does not know is
 * refused instead of being passed through.
 */
export function normalizeFills(value: unknown): FigmaPaint[] {
  if (!Array.isArray(value)) {
    invalidPaint("The fills of the plan are not a list of paints");
  }
  const paints: FigmaPaint[] = [];
  for (const entry of value) {
    if (typeof entry !== "object" || entry === null || Array.isArray(entry)) {
      invalidPaint("One paint of the plan is not an object");
    }
    const record = entry as Record<string, unknown>;
    const type = record["type"];
    if (typeof type !== "string") {
      invalidPaint("One paint of the plan has no type");
    }
    if (type === "SOLID") {
      const color = normalizeColor(record["color"]);
      const opacity = record["opacity"];
      if (opacity !== undefined && (typeof opacity !== "number" || !Number.isFinite(opacity))) {
        invalidPaint("The opacity of a paint of the plan is not a number");
      }
      const alpha = color.a;
      paints.push({
        type: "SOLID",
        color: { r: color.r, g: color.g, b: color.b },
        opacity: alpha * (typeof opacity === "number" ? Math.min(1, Math.max(0, opacity)) : 1),
        visible: true,
      });
      continue;
    }
    if (
      ["GRADIENT_LINEAR", "GRADIENT_RADIAL", "GRADIENT_ANGULAR", "GRADIENT_DIAMOND"].includes(type)
    ) {
      const stops = normalizeStops(record["stops"] ?? record["gradientStops"]);
      const transform = record["gradientTransform"] ?? [
        [1, 0, 0],
        [0, 1, 0],
      ];
      if (
        !Array.isArray(transform) ||
        transform.length !== 2 ||
        !transform.every(
          (row) =>
            Array.isArray(row) &&
            row.length === 3 &&
            row.every((channel) => typeof channel === "number" && Number.isFinite(channel)),
        )
      )
        invalidPaint("Gradient transform must contain two rows of three finite numbers");
      paints.push({
        type,
        gradientStops: stops,
        gradientTransform: transform as number[][],
        visible: true,
      });
      continue;
    }
    invalidPaint("The plan uses a paint type the plugin does not support");
  }
  return paints;
}

/** Normalize the stops of a gradient paint. */
function normalizeStops(value: unknown): FigmaGradientStop[] {
  if (!Array.isArray(value) || value.length < 2) {
    invalidPaint("A gradient of the plan needs at least two stops");
  }
  const stops: FigmaGradientStop[] = [];
  for (const entry of value) {
    if (typeof entry !== "object" || entry === null || Array.isArray(entry)) {
      invalidPaint("One stop of a gradient of the plan is not an object");
    }
    const record = entry as Record<string, unknown>;
    const position = record["position"];
    if (typeof position !== "number" || !Number.isFinite(position)) {
      invalidPaint("One stop of a gradient of the plan has no valid position");
    }
    stops.push({
      position: Math.min(1, Math.max(0, position)),
      color: normalizeColor(record["color"]),
    });
  }
  stops.sort((left, right) => left.position - right.position);
  return stops;
}
