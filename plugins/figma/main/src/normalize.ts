/**
 * Normalization of Figma nodes into the plugin wire shape.
 *
 * The rules come from `docs/02-architecture/04-plugin-design.md` section 6
 * and `docs/04-data-contracts/04-normalized-model.md`:
 * - a field the platform did not expose becomes an unavailable field with a
 *   reason, never a silent zero value;
 * - mixed typography becomes styled runs;
 * - unknown node types keep their verbatim type string;
 * - unresolved style and variable references stay unresolved.
 */

import {
  asArray,
  asNumber,
  asRecord,
  asString,
  isMixed,
  type FigmaApi,
  type FigmaEffect,
  type FigmaFontName,
  type FigmaNode,
  type FigmaPaint,
  type FigmaStyledTextSegment,
} from "./figma-api.js";
import type { ReferenceResolver } from "./tokens.js";
import { boundVariableId } from "./tokens.js";
import {
  UNAVAILABLE_REASON,
  unavailable,
  type PluginComponent,
  type PluginEffect,
  type PluginGeometry,
  type PluginLayout,
  type PluginNode,
  type PluginPaint,
  type PluginText,
  type PluginTextRun,
  type PluginVariable,
  type PluginVariantProperty,
} from "./types.js";

/** Everything the normalizer needs besides the node itself. */
export interface NormalizeContext {
  /** The editor API, used to detect mixed values. */
  api: FigmaApi;
  /** Resolves variable and style references. */
  resolver: ReferenceResolver;
  /** Parent id when the parent is inside the traversed scope. */
  parentId?: string | undefined;
  /** Ids of the children inside the traversed scope. */
  childIds?: string[] | undefined;
  /** Whether variable and style references should be resolved. */
  includeReferences?: boolean | undefined;
  /** Font availability lookup, used to flag unusable fonts. */
  fontCatalog?: FontCatalog | undefined;
}

/** Cache of the fonts the editor can render. */
export class FontCatalog {
  private fonts: Promise<Set<string> | null> | null = null;
  private probed = false;

  constructor(private readonly api: FigmaApi) {}

  /** Whether the font list could be probed at all. */
  get wasProbed(): boolean {
    return this.probed;
  }

  /** Whether a font can be rendered by the editor. */
  async isAvailable(family: string, style: string): Promise<boolean> {
    const set = await this.load();
    if (set === null) {
      return true;
    }
    return set.has(fontKey(family, style));
  }

  private async load(): Promise<Set<string> | null> {
    if (this.fonts !== null) {
      return await this.fonts;
    }
    this.probed = true;
    if (typeof this.api.listAvailableFontsAsync !== "function") {
      this.fonts = Promise.resolve(null);
      return null;
    }
    this.fonts = this.api.listAvailableFontsAsync().then((fonts) => {
      const set = new Set<string>();
      for (const entry of fonts) {
        if (entry?.fontName !== undefined) {
          set.add(fontKey(entry.fontName.family, entry.fontName.style));
        }
      }
      return set;
    });
    return await this.fonts;
  }
}

function fontKey(family: string, style: string): string {
  return `${family.toLowerCase()}::${style.toLowerCase()}`;
}

const FONT_WEIGHTS: Readonly<Record<string, number>> = {
  thin: 100,
  "ultra light": 200,
  "extra light": 200,
  light: 300,
  regular: 400,
  normal: 400,
  book: 400,
  medium: 500,
  "semi bold": 600,
  "demi bold": 600,
  bold: 700,
  "extra bold": 800,
  "ultra bold": 800,
  black: 900,
  heavy: 900,
};

/**
 * Derive a numeric weight from a font style name.
 *
 * The Plugin API exposes the style name only, never a numeric weight, so the
 * weight is derived from the well known style names.
 */
export function fontWeightOf(style: string): number {
  const normalized = style
    .trim()
    .toLowerCase()
    .replace(/\s+italic$/, "");
  const weight = FONT_WEIGHTS[normalized];
  return weight ?? 400;
}

/** Normalize a node into the plugin wire shape. */
export async function normalizeNode(
  node: FigmaNode,
  context: NormalizeContext,
): Promise<PluginNode> {
  const unavailableFields: string[] = [];
  const result: PluginNode = {
    id: node.id,
    node_type: node.type,
    name: node.name,
    visible: node.visible,
    locked: node.locked,
    children: [...(context.childIds ?? [])],
    fills: [],
    strokes: [],
    effects: [],
    variables: [],
    styles: [],
    unavailable_fields: unavailableFields,
  };

  if (context.parentId !== undefined) {
    result.parent_id = context.parentId;
  }

  const geometry = buildGeometry(node, unavailableFields);
  if (geometry !== undefined) {
    result.geometry = geometry;
  }
  const layout = buildLayout(node, unavailableFields);
  if (layout !== undefined) {
    result.layout = layout;
  }
  const text = await buildText(node, context, unavailableFields);
  if (text !== undefined) {
    result.text = text;
  }
  result.fills = buildPaints(node.fills, context.api, "fills", unavailableFields);
  result.strokes = buildPaints(node.strokes, context.api, "strokes", unavailableFields);
  result.effects = buildEffects(node.effects, context.api, unavailableFields);

  const component = await buildComponent(node, unavailableFields);
  if (component !== undefined) {
    result.component = component;
  }

  if (context.includeReferences === true) {
    const references = await buildReferences(node, context, unavailableFields);
    result.variables = references.variables;
    result.styles = references.styles;
  }

  return result;
}

/** Build the geometry block of a node. */
export function buildGeometry(
  node: FigmaNode,
  unavailableFields: string[],
): PluginGeometry | undefined {
  const local = linearPart(node.relativeTransform);
  const absolute = linearPart(node.absoluteTransform);
  if (local === undefined && absolute === undefined) {
    unavailableFields.push(
      unavailable("geometry.local_transform", UNAVAILABLE_REASON.CAPABILITY_UNAVAILABLE),
    );
  }

  const bounds = node.absoluteBoundingBox;
  let boundingBox: [number, number, number, number];
  if (bounds !== undefined && bounds !== null) {
    boundingBox = [bounds.x, bounds.y, bounds.width, bounds.height];
  } else {
    boundingBox = [node.x ?? 0, node.y ?? 0, node.width ?? 0, node.height ?? 0];
    unavailableFields.push(
      unavailable("geometry.bounding_box", UNAVAILABLE_REASON.CAPABILITY_UNAVAILABLE),
    );
  }

  const constraints = node.constraints;
  const horizontal = constraints?.horizontal ?? "";
  const vertical = constraints?.vertical ?? "";
  if (constraints === undefined) {
    unavailableFields.push(
      unavailable("geometry.constraints", UNAVAILABLE_REASON.CAPABILITY_UNAVAILABLE),
    );
  }

  return {
    local_transform: local ?? [1, 0, 0, 1],
    absolute_transform: absolute ?? local ?? [1, 0, 0, 1],
    bounding_box: boundingBox,
    rotation: node.rotation ?? 0,
    constraints_horizontal: horizontal,
    constraints_vertical: vertical,
    coordinate_space: "page",
  };
}

function linearPart(
  transform: number[][] | undefined,
): [number, number, number, number] | undefined {
  if (transform === undefined || transform.length < 2) {
    return undefined;
  }
  const first = transform[0];
  const second = transform[1];
  if (first === undefined || second === undefined) {
    return undefined;
  }
  const a = asNumber(first[0]);
  const c = asNumber(first[1]);
  const b = asNumber(second[0]);
  const d = asNumber(second[1]);
  if (a === undefined || b === undefined || c === undefined || d === undefined) {
    return undefined;
  }
  return [a, b, c, d];
}

/** Build the auto layout block of a node. */
export function buildLayout(
  node: FigmaNode,
  unavailableFields: string[],
): PluginLayout | undefined {
  const axis = node.layoutMode;
  if (axis === undefined) {
    return undefined;
  }
  const autoLayout = axis !== "NONE";
  const primary = node.primaryAxisSizingMode;
  const counter = node.counterAxisSizingMode;
  let sizing = "FIXED";
  if (primary === "AUTO" || counter === "AUTO") {
    sizing = "HUG";
  }
  if (primary === undefined || counter === undefined) {
    unavailableFields.push(unavailable("layout.sizing", UNAVAILABLE_REASON.CAPABILITY_UNAVAILABLE));
  }
  return {
    auto_layout: autoLayout,
    axis,
    gap: node.itemSpacing ?? 0,
    padding: [
      node.paddingTop ?? 0,
      node.paddingRight ?? 0,
      node.paddingBottom ?? 0,
      node.paddingLeft ?? 0,
    ],
    primary_axis_align: node.primaryAxisAlignItems ?? "",
    counter_axis_align: node.counterAxisAlignItems ?? "",
    sizing,
    wraps: node.layoutWrap === "WRAP",
  };
}

/** Build the typography block of a text node. */
export async function buildText(
  node: FigmaNode,
  context: NormalizeContext,
  unavailableFields: string[],
): Promise<PluginText | undefined> {
  if (node.type !== "TEXT") {
    return undefined;
  }
  const characters = node.characters ?? "";
  const runs = await buildRuns(node, characters, context, unavailableFields);
  const truncated = node.textTruncation === "ENDING";
  return {
    text: characters,
    runs,
    text_align: node.textAlignHorizontal ?? "",
    truncated,
  };
}

async function buildRuns(
  node: FigmaNode,
  characters: string,
  context: NormalizeContext,
  unavailableFields: string[],
): Promise<PluginTextRun[]> {
  const segments = readSegments(node);
  if (segments !== undefined && segments.length > 0) {
    const runs: PluginTextRun[] = [];
    for (const segment of segments) {
      const fontName = asFontName(segment.fontName);
      const fontSize = asNumber(segment.fontSize) ?? 0;
      const fontAvailable =
        fontName === undefined ? false : await isFontAvailable(context, fontName);
      const run: PluginTextRun = {
        start: segment.start,
        end: segment.end,
        font_family: fontName?.family ?? "",
        font_style: fontName?.style ?? "",
        font_weight: fontWeightOf(fontName?.style ?? ""),
        font_size: fontSize,
        line_height: normalizeLineHeight(segment.lineHeight, fontSize),
        letter_spacing: normalizeLetterSpacing(segment.letterSpacing, fontSize),
        font_available: fontAvailable,
      };
      if (fontName === undefined) {
        unavailableFields.push(
          unavailable(
            `text.runs[${runs.length}].font_family`,
            UNAVAILABLE_REASON.CAPABILITY_UNAVAILABLE,
          ),
        );
      }
      runs.push(run);
    }
    return runs;
  }

  const fontName = asFontName(node.fontName);
  const fontSize = asNumber(node.fontSize) ?? 0;
  const run: PluginTextRun = {
    start: 0,
    end: charactersLength(characters),
    font_family: fontName?.family ?? "",
    font_style: fontName?.style ?? "",
    font_weight: fontWeightOf(fontName?.style ?? ""),
    font_size: fontSize,
    line_height: normalizeLineHeight(node.lineHeight, fontSize),
    letter_spacing: normalizeLetterSpacing(node.letterSpacing, fontSize),
    font_available: fontName === undefined ? false : await isFontAvailable(context, fontName),
  };
  if (fontName === undefined) {
    unavailableFields.push(
      unavailable("text.runs[0].font_family", UNAVAILABLE_REASON.CAPABILITY_UNAVAILABLE),
    );
  }
  return [run];
}

function charactersLength(characters: string): number {
  return Array.from(characters).length;
}

async function isFontAvailable(
  context: NormalizeContext,
  fontName: FigmaFontName,
): Promise<boolean> {
  const catalog = context.fontCatalog;
  if (catalog === undefined) {
    return true;
  }
  return await catalog.isAvailable(fontName.family, fontName.style);
}

function readSegments(node: FigmaNode): FigmaStyledTextSegment[] | undefined {
  const getter = node.getStyledTextSegments;
  if (getter === undefined) {
    return undefined;
  }
  try {
    const segments = getter.call(node, ["fontName", "fontSize", "lineHeight", "letterSpacing"]);
    if (!Array.isArray(segments)) {
      return undefined;
    }
    return segments;
  } catch {
    return undefined;
  }
}

function asFontName(value: unknown): FigmaFontName | undefined {
  const record = asRecord(value);
  if (record === null) {
    return undefined;
  }
  const family = asString(record["family"]);
  const style = asString(record["style"]);
  if (family === undefined || style === undefined) {
    return undefined;
  }
  return { family, style };
}

/**
 * Normalize a line height into a number.
 *
 * A percentage is resolved against the font size. The automatic line height
 * is not exposed by the API, so the editor default of 1.2 times the font
 * size is reported.
 */
export function normalizeLineHeight(value: unknown, fontSize: number): number {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  const record = asRecord(value);
  if (record === null) {
    return fontSize * 1.2;
  }
  const unit = asString(record["unit"]);
  const amount = asNumber(record["value"]);
  if (unit === "PERCENT" && amount !== undefined) {
    return (fontSize * amount) / 100;
  }
  if (unit === "PIXELS" && amount !== undefined) {
    return amount;
  }
  return fontSize * 1.2;
}

/** Normalize a letter spacing into a number. */
export function normalizeLetterSpacing(value: unknown, fontSize: number): number {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  const record = asRecord(value);
  if (record === null) {
    return 0;
  }
  const unit = asString(record["unit"]);
  const amount = asNumber(record["value"]);
  if (amount === undefined) {
    return 0;
  }
  if (unit === "PERCENT") {
    return (fontSize * amount) / 100;
  }
  return amount;
}

/** Build the fill or stroke list of a node. */
export function buildPaints(
  value: unknown,
  api: FigmaApi,
  path: string,
  unavailableFields: string[],
): PluginPaint[] {
  if (value === undefined) {
    return [];
  }
  if (isMixed(api, value)) {
    unavailableFields.push(unavailable(path, UNAVAILABLE_REASON.CAPABILITY_UNAVAILABLE));
    return [];
  }
  const entries = asArray(value);
  if (entries === undefined) {
    unavailableFields.push(unavailable(path, UNAVAILABLE_REASON.CAPABILITY_UNAVAILABLE));
    return [];
  }
  const paints: PluginPaint[] = [];
  for (const entry of entries) {
    const record = asRecord(entry) as FigmaPaint | null;
    if (record === null) {
      continue;
    }
    paints.push(buildPaint(record));
  }
  return paints;
}

function buildPaint(paint: FigmaPaint): PluginPaint {
  const type = paint.type;
  const kind =
    type === "SOLID"
      ? "SOLID"
      : type === "GRADIENT"
        ? "GRADIENT"
        : type === "IMAGE"
          ? "IMAGE"
          : "UNKNOWN";
  const result: PluginPaint = {
    kind,
    gradient_stops: [],
    opacity: paint.opacity ?? 1,
    visible: paint.visible ?? true,
  };
  if (kind === "SOLID" && paint.color !== undefined) {
    result.color = [paint.color.r, paint.color.g, paint.color.b, paint.color.a];
  }
  if (kind === "GRADIENT" && paint.gradientStops !== undefined) {
    result.gradient_stops = paint.gradientStops.map((stop) => ({
      position: stop.position,
      color: [stop.color.r, stop.color.g, stop.color.b, stop.color.a],
    }));
  }
  return result;
}

/** Build the effect list of a node. */
export function buildEffects(
  value: unknown,
  api: FigmaApi,
  unavailableFields: string[],
): PluginEffect[] {
  if (value === undefined) {
    return [];
  }
  if (isMixed(api, value)) {
    unavailableFields.push(unavailable("effects", UNAVAILABLE_REASON.CAPABILITY_UNAVAILABLE));
    return [];
  }
  const entries = asArray(value);
  if (entries === undefined) {
    unavailableFields.push(unavailable("effects", UNAVAILABLE_REASON.CAPABILITY_UNAVAILABLE));
    return [];
  }
  const effects: PluginEffect[] = [];
  for (const entry of entries) {
    const record = asRecord(entry) as FigmaEffect | null;
    if (record === null) {
      continue;
    }
    const effect: PluginEffect = {
      kind: record.type,
      visible: record.visible ?? true,
      radius: record.radius ?? 0,
    };
    if (record.color !== undefined) {
      effect.color = [record.color.r, record.color.g, record.color.b, record.color.a];
    }
    effects.push(effect);
  }
  return effects;
}

/** Build the component reference of an instance node. */
export async function buildComponent(
  node: FigmaNode,
  unavailableFields: string[],
): Promise<PluginComponent | undefined> {
  if (node.type !== "INSTANCE") {
    return undefined;
  }
  const component: PluginComponent = {
    instance_id: node.id,
    variant_properties: [],
  };
  let mainComponentId: string | undefined;
  const getter = node.getMainComponentAsync;
  if (getter !== undefined) {
    try {
      const main = await getter.call(node);
      mainComponentId = main?.id;
    } catch {
      mainComponentId = undefined;
    }
  } else if (node.mainComponent !== undefined && node.mainComponent !== null) {
    mainComponentId = node.mainComponent.id;
  }
  if (mainComponentId === undefined) {
    unavailableFields.push(
      unavailable("component.main_component_id", UNAVAILABLE_REASON.NOT_IN_GRANT),
    );
  } else {
    component.main_component_id = mainComponentId;
  }

  const properties = node.componentProperties;
  if (properties !== undefined) {
    const variantProperties: PluginVariantProperty[] = [];
    for (const [name, property] of Object.entries(properties)) {
      variantProperties.push({
        name,
        value: propertyValue(property?.value),
      });
    }
    component.variant_properties = variantProperties;
  }
  return component;
}

function propertyValue(value: unknown): string {
  if (typeof value === "string") {
    return value;
  }
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  return "";
}

/** Collect the variable and style references of a node. */
export async function buildReferences(
  node: FigmaNode,
  context: NormalizeContext,
  unavailableFields: string[],
): Promise<{ variables: PluginVariable[]; styles: PluginVariable[] }> {
  const variableIds = new Set<string>();
  const styleIds = new Set<string>();

  const bound = node.boundVariables;
  if (bound !== undefined) {
    for (const binding of Object.values(bound)) {
      const id = boundVariableId(binding);
      if (id !== undefined) {
        variableIds.add(id);
      }
    }
  }

  for (const paints of [node.fills, node.strokes]) {
    const entries = asArray(paints);
    if (entries === undefined) {
      continue;
    }
    for (const entry of entries) {
      const record = asRecord(entry) as FigmaPaint | null;
      if (record === null || record.boundVariables === undefined) {
        continue;
      }
      for (const binding of Object.values(record.boundVariables)) {
        const id = boundVariableId(binding);
        if (id !== undefined) {
          variableIds.add(id);
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

  const variables: PluginVariable[] = [];
  for (const id of variableIds) {
    const resolved = await context.resolver.resolveVariable(id);
    if (!resolved.resolved) {
      unavailableFields.push(
        unavailable(`variables.${id}`, UNAVAILABLE_REASON.CAPABILITY_UNAVAILABLE),
      );
    }
    variables.push(resolved);
  }

  const styles: PluginVariable[] = [];
  for (const id of styleIds) {
    const resolved = await context.resolver.resolveStyle(id);
    if (!resolved.resolved) {
      unavailableFields.push(unavailable(`styles.${id}`, UNAVAILABLE_REASON.NOT_IN_GRANT));
    }
    styles.push(resolved);
  }

  return { variables, styles };
}
