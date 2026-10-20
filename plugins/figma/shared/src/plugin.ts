/**
 * Plugin payload contract.
 *
 * Mirror of `crates/protocol/src/plugin.rs`. The plugin is the only producer of
 * these types. The shape is intentionally permissive: fields the platform did
 * not expose are absent, and unknown node types are preserved verbatim instead
 * of being dropped.
 */
import type { CapabilityMap } from "./capabilities";
import type { JsonValue } from "./json";

/** A four component tuple, used for transforms, boxes and colours. */
export type Vec4 = [number, number, number, number];

/** A node as reported by the plugin. */
export interface PluginNode {
  /** Figma node id. */
  id: string;
  /** Parent node id, when the parent is inside the traversed scope. */
  parent_id?: string;
  /** Node type, for example `FRAME`. Unknown types are preserved. */
  node_type: string;
  /** Display name. */
  name: string;
  /** Whether the node is visible. */
  visible: boolean;
  /** Whether the node is locked. */
  locked: boolean;
  /** Node ids of the children inside the traversed scope. */
  children: string[];
  /** Geometry fields, when available. */
  geometry?: PluginGeometry;
  /** Auto layout fields, when available. */
  layout?: PluginLayout;
  /** Text fields, when the node is a text node. */
  text?: PluginText;
  /** Fills. */
  fills: PluginPaint[];
  /** Strokes. */
  strokes: PluginPaint[];
  /** Effects. */
  effects: PluginEffect[];
  /** Component reference, when the node is an instance. */
  component?: PluginComponent;
  /** Variable references used by the node. */
  variables: PluginVariable[];
  /** Style references used by the node. */
  styles: PluginVariable[];
  /** Fields the plugin could not obtain, with the reason. */
  unavailable_fields: string[];
}

/** Geometry fields of a node. */
export interface PluginGeometry {
  /** Transform relative to the parent. */
  local_transform: Vec4;
  /** Transform relative to the canvas. */
  absolute_transform: Vec4;
  /** Bounding box. */
  bounding_box: Vec4;
  /** Rotation in degrees. */
  rotation: number;
  /** Horizontal constraint. */
  constraints_horizontal: string;
  /** Vertical constraint. */
  constraints_vertical: string;
  /** Coordinate space the numbers are expressed in. */
  coordinate_space: string;
}

/** Auto layout fields of a node. */
export interface PluginLayout {
  /** Whether auto layout is enabled. */
  auto_layout: boolean;
  /** Primary axis. */
  axis: string;
  /** Gap between children. */
  gap: number;
  /** Padding tuple: top, right, bottom, left. */
  padding: Vec4;
  /** Primary axis alignment. */
  primary_axis_align: string;
  /** Counter axis alignment. */
  counter_axis_align: string;
  /** Sizing mode: `FIXED`, `HUG`, or `FILL`. */
  sizing: string;
  /** Whether text wraps. */
  wraps: boolean;
}

/** Text fields of a text node. */
export interface PluginText {
  /** Full text. */
  text: string;
  /** Styled runs. */
  runs: PluginTextRun[];
  /** Text alignment. */
  text_align: string;
  /** Whether the text is truncated. */
  truncated: boolean;
}

/** One styled run. */
export interface PluginTextRun {
  /** Start character offset. */
  start: number;
  /** End character offset. */
  end: number;
  /** Font family. */
  font_family: string;
  /** Font style. */
  font_style: string;
  /** Font weight. */
  font_weight: number;
  /** Font size. */
  font_size: number;
  /** Line height. */
  line_height: number;
  /** Letter spacing. */
  letter_spacing: number;
  /** Whether the font is available in the editor runtime. */
  font_available: boolean;
}

/** A fill or stroke. */
export interface PluginPaint {
  /** Kind: `SOLID`, `GRADIENT`, `IMAGE`, or `UNKNOWN`. */
  kind: string;
  /** Colour channels, when the paint is solid. */
  color?: Vec4;
  /** Gradient stops. */
  gradient_stops: PluginGradientStop[];
  /** Opacity. */
  opacity: number;
  /** Whether the paint is visible. */
  visible: boolean;
}

/** One gradient stop. */
export interface PluginGradientStop {
  /** Position of the stop. */
  position: number;
  /** Colour at the stop. */
  color: Vec4;
}

/** One effect. */
export interface PluginEffect {
  /** Kind of the effect. */
  kind: string;
  /** Whether the effect is visible. */
  visible: boolean;
  /** Radius of the blur or shadow. */
  radius: number;
  /** Shadow colour. */
  color?: Vec4;
}

/** Component reference of an instance node. */
export interface PluginComponent {
  /** Instance node id. */
  instance_id: string;
  /** Main component id, when resolvable. */
  main_component_id?: string;
  /** Variant properties. */
  variant_properties: PluginVariantProperty[];
}

/** One variant property. */
export interface PluginVariantProperty {
  /** Property name. */
  name: string;
  /** Property value. */
  value: string;
}

/** A variable or style reference. */
export interface PluginVariable {
  /** Id of the variable or style. */
  id: string;
  /** Display name. */
  name: string;
  /** Collection, when known. */
  collection?: string;
  /** Mode, when known. */
  mode?: string;
  /** Value, when resolvable. */
  value?: JsonValue;
  /** Alias target, when the variable aliases another variable. */
  alias_target?: string;
  /** Whether the reference resolved. */
  resolved: boolean;
  /** Reason for an unresolved reference. */
  unresolved_reason?: string;
}

/** A document as reported by the plugin. */
export interface PluginDocument {
  /** Human visible document label. Never an authorization key. */
  document_label: string;
  /** Name of the page that was read. */
  page_name: string;
  /** Nodes in the traversed scope. */
  nodes: PluginNode[];
  /** Capabilities detected in the plugin runtime. */
  capabilities: CapabilityMap;
}

/** The result of an asset export. Bytes travel base64 encoded. */
export interface PluginExport {
  /** Node that was exported. */
  node_id: string;
  /** MIME type of the export. */
  mime: string;
  /** Pixel width. */
  width: number;
  /** Pixel height. */
  height: number;
  /** Raw bytes of the export, base64 encoded. */
  bytes: string;
  /** Hash of the export bytes. */
  hash: string;
}

/** Capabilities reported by the plugin runtime. */
export interface PluginCapabilities {
  /** Probed capabilities. */
  capabilities: CapabilityMap;
}
