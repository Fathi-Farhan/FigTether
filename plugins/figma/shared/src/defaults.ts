/**
 * Factories that apply the Rust side defaults.
 *
 * Every field the Rust structs declare with `#[serde(default)]` is written on
 * the wire, so the plugin must produce the full shape. These helpers fill the
 * defaulted fields so a producer never has to repeat them.
 */
import type { CapabilityMap } from "./capabilities";
import type {
  PluginDocument,
  PluginEffect,
  PluginGeometry,
  PluginLayout,
  PluginNode,
  PluginPaint,
  PluginText,
  PluginVariable,
  Vec4,
} from "./plugin";

/** The identity transform used when a node has no transform. */
export const IDENTITY_TRANSFORM: Vec4 = [1, 0, 0, 1];

/** The empty bounding box. */
export const EMPTY_BOX: Vec4 = [0, 0, 0, 0];

/** The zero padding tuple. */
export const ZERO_PADDING: Vec4 = [0, 0, 0, 0];

/** Input for {@link createPluginNode}. */
export interface PluginNodeInit {
  /** Figma node id. */
  id: string;
  /** Node type, for example `FRAME`. */
  node_type: string;
  /** Display name. */
  name?: string;
  /** Parent node id, when the parent is inside the traversed scope. */
  parent_id?: string;
  /** Whether the node is visible. */
  visible?: boolean;
  /** Whether the node is locked. */
  locked?: boolean;
  /** Node ids of the children inside the traversed scope. */
  children?: string[];
  /** Geometry fields, when available. */
  geometry?: PluginGeometry;
  /** Auto layout fields, when available. */
  layout?: PluginLayout;
  /** Text fields, when the node is a text node. */
  text?: PluginText;
  /** Fills. */
  fills?: PluginPaint[];
  /** Strokes. */
  strokes?: PluginPaint[];
  /** Effects. */
  effects?: PluginEffect[];
  /** Component reference, when the node is an instance. */
  component?: PluginNode["component"];
  /** Variable references used by the node. */
  variables?: PluginVariable[];
  /** Style references used by the node. */
  styles?: PluginVariable[];
  /** Fields the plugin could not obtain, with the reason. */
  unavailable_fields?: string[];
}

/** Build a complete node payload with the Rust defaults applied. */
export function createPluginNode(init: PluginNodeInit): PluginNode {
  const node: PluginNode = {
    id: init.id,
    node_type: init.node_type,
    name: init.name ?? "",
    visible: init.visible ?? true,
    locked: init.locked ?? false,
    children: init.children ?? [],
    fills: init.fills ?? [],
    strokes: init.strokes ?? [],
    effects: init.effects ?? [],
    variables: init.variables ?? [],
    styles: init.styles ?? [],
    unavailable_fields: init.unavailable_fields ?? [],
  };
  if (init.parent_id !== undefined) {
    node.parent_id = init.parent_id;
  }
  if (init.geometry !== undefined) {
    node.geometry = init.geometry;
  }
  if (init.layout !== undefined) {
    node.layout = init.layout;
  }
  if (init.text !== undefined) {
    node.text = init.text;
  }
  if (init.component !== undefined) {
    node.component = init.component;
  }
  return node;
}

/** Input for {@link createPluginGeometry}. */
export interface PluginGeometryInit {
  /** Transform relative to the parent. */
  local_transform?: Vec4;
  /** Transform relative to the canvas. */
  absolute_transform?: Vec4;
  /** Bounding box. */
  bounding_box?: Vec4;
  /** Rotation in degrees. */
  rotation?: number;
  /** Horizontal constraint. */
  constraints_horizontal?: string;
  /** Vertical constraint. */
  constraints_vertical?: string;
  /** Coordinate space the numbers are expressed in. */
  coordinate_space?: string;
}

/** Build a complete geometry payload with the Rust defaults applied. */
export function createPluginGeometry(init: PluginGeometryInit = {}): PluginGeometry {
  return {
    local_transform: init.local_transform ?? IDENTITY_TRANSFORM,
    absolute_transform: init.absolute_transform ?? IDENTITY_TRANSFORM,
    bounding_box: init.bounding_box ?? EMPTY_BOX,
    rotation: init.rotation ?? 0,
    constraints_horizontal: init.constraints_horizontal ?? "",
    constraints_vertical: init.constraints_vertical ?? "",
    coordinate_space: init.coordinate_space ?? "page",
  };
}

/** Input for {@link createPluginLayout}. */
export interface PluginLayoutInit {
  /** Whether auto layout is enabled. */
  auto_layout?: boolean;
  /** Primary axis. */
  axis?: string;
  /** Gap between children. */
  gap?: number;
  /** Padding tuple: top, right, bottom, left. */
  padding?: Vec4;
  /** Primary axis alignment. */
  primary_axis_align?: string;
  /** Counter axis alignment. */
  counter_axis_align?: string;
  /** Sizing mode: `FIXED`, `HUG`, or `FILL`. */
  sizing?: string;
  /** Whether text wraps. */
  wraps?: boolean;
}

/** Build a complete layout payload with the Rust defaults applied. */
export function createPluginLayout(init: PluginLayoutInit = {}): PluginLayout {
  return {
    auto_layout: init.auto_layout ?? false,
    axis: init.axis ?? "",
    gap: init.gap ?? 0,
    padding: init.padding ?? ZERO_PADDING,
    primary_axis_align: init.primary_axis_align ?? "",
    counter_axis_align: init.counter_axis_align ?? "",
    sizing: init.sizing ?? "",
    wraps: init.wraps ?? false,
  };
}

/** Input for {@link createPluginText}. */
export interface PluginTextInit {
  /** Full text. */
  text: string;
  /** Styled runs. */
  runs?: PluginText["runs"];
  /** Text alignment. */
  text_align?: string;
  /** Whether the text is truncated. */
  truncated?: boolean;
}

/** Build a complete text payload with the Rust defaults applied. */
export function createPluginText(init: PluginTextInit): PluginText {
  return {
    text: init.text,
    runs: init.runs ?? [],
    text_align: init.text_align ?? "",
    truncated: init.truncated ?? false,
  };
}

/** Input for {@link createPluginPaint}. */
export interface PluginPaintInit {
  /** Kind: `SOLID`, `GRADIENT`, `IMAGE`, or `UNKNOWN`. */
  kind: string;
  /** Colour channels, when the paint is solid. */
  color?: Vec4;
  /** Gradient stops. */
  gradient_stops?: PluginPaint["gradient_stops"];
  /** Opacity. */
  opacity?: number;
  /** Whether the paint is visible. */
  visible?: boolean;
}

/** Build a complete paint payload with the Rust defaults applied. */
export function createPluginPaint(init: PluginPaintInit): PluginPaint {
  const paint: PluginPaint = {
    kind: init.kind,
    gradient_stops: init.gradient_stops ?? [],
    opacity: init.opacity ?? 1,
    visible: init.visible ?? true,
  };
  if (init.color !== undefined) {
    paint.color = init.color;
  }
  return paint;
}

/** Input for {@link createPluginEffect}. */
export interface PluginEffectInit {
  /** Kind of the effect. */
  kind: string;
  /** Whether the effect is visible. */
  visible?: boolean;
  /** Radius of the blur or shadow. */
  radius?: number;
  /** Shadow colour. */
  color?: Vec4;
}

/** Build a complete effect payload with the Rust defaults applied. */
export function createPluginEffect(init: PluginEffectInit): PluginEffect {
  const effect: PluginEffect = {
    kind: init.kind,
    visible: init.visible ?? true,
    radius: init.radius ?? 0,
  };
  if (init.color !== undefined) {
    effect.color = init.color;
  }
  return effect;
}

/** Input for {@link createPluginVariable}. */
export interface PluginVariableInit {
  /** Id of the variable or style. */
  id: string;
  /** Display name. */
  name: string;
  /** Collection, when known. */
  collection?: string;
  /** Mode, when known. */
  mode?: string;
  /** Value, when resolvable. */
  value?: PluginVariable["value"];
  /** Alias target, when the variable aliases another variable. */
  alias_target?: string;
  /** Whether the reference resolved. */
  resolved?: boolean;
  /** Reason for an unresolved reference. */
  unresolved_reason?: string;
}

/** Build a complete variable payload with the Rust defaults applied. */
export function createPluginVariable(init: PluginVariableInit): PluginVariable {
  const variable: PluginVariable = {
    id: init.id,
    name: init.name,
    resolved: init.resolved ?? false,
  };
  if (init.collection !== undefined) {
    variable.collection = init.collection;
  }
  if (init.mode !== undefined) {
    variable.mode = init.mode;
  }
  if (init.value !== undefined) {
    variable.value = init.value;
  }
  if (init.alias_target !== undefined) {
    variable.alias_target = init.alias_target;
  }
  if (init.unresolved_reason !== undefined) {
    variable.unresolved_reason = init.unresolved_reason;
  }
  return variable;
}

/** Input for {@link createPluginDocument}. */
export interface PluginDocumentInit {
  /** Human visible document label. Never an authorization key. */
  document_label?: string;
  /** Name of the page that was read. */
  page_name?: string;
  /** Nodes in the traversed scope. */
  nodes?: PluginNode[];
  /** Capabilities detected in the plugin runtime. */
  capabilities?: CapabilityMap;
}

/** Build a complete document payload with the Rust defaults applied. */
export function createPluginDocument(init: PluginDocumentInit = {}): PluginDocument {
  return {
    document_label: init.document_label ?? "",
    page_name: init.page_name ?? "",
    nodes: init.nodes ?? [],
    capabilities: init.capabilities ?? {},
  };
}
/** Build-selected loopback port; tests and source imports retain the default. */
declare const __FIGTETHER_PORT__: number;
export const LOCAL_BRIDGE_PORT =
  typeof __FIGTETHER_PORT__ === "undefined" ? 38947 : __FIGTETHER_PORT__;

/** Only this build's private companion URL can be opened by the sandbox. */
export function isCompanionUrl(url: string): boolean {
  return new RegExp(`^http://127\\.0\\.0\\.1:${LOCAL_BRIDGE_PORT}/panel#[A-Za-z0-9_-]{43}$`).test(
    url,
  );
}
