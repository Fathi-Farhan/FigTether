/**
 * Thin, injectable view of the Figma Plugin API.
 *
 * The main sandbox never touches the global `figma` object directly. Every
 * handler receives a `FigmaApi`, so the whole plugin can be exercised in a
 * unit test without an editor. `resolveFigmaApi` performs the single runtime
 * narrowing step that turns the editor global into this interface.
 *
 * The interfaces below are structural subsets of the documented Plugin API.
 * They intentionally use `unknown` for the fields that the API can return as
 * `figma.mixed`, and every read narrows the value before it is used.
 */

/** A colour with an alpha channel, channels in `0.0 ..= 1.0`. */
export interface FigmaColor {
  r: number;
  g: number;
  b: number;
  a: number;
}

/** An axis aligned rectangle. */
export interface FigmaBounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** A font reference. */
export interface FigmaFontName {
  family: string;
  style: string;
}

/** One gradient stop of a gradient paint. */
export interface FigmaGradientStop {
  position: number;
  color: FigmaColor;
}

/** A fill or a stroke. */
export interface FigmaPaint {
  type: string;
  visible?: boolean;
  opacity?: number;
  color?: FigmaColor;
  gradientStops?: FigmaGradientStop[];
  imageHash?: string;
  scaleMode?: string;
  boundVariables?: Record<string, FigmaBoundVariable>;
}

/** A visual effect. */
export interface FigmaEffect {
  type: string;
  visible?: boolean;
  radius?: number;
  color?: FigmaColor;
  spread?: number;
}

/** One styled segment of a text node. */
export interface FigmaStyledTextSegment {
  characters: string;
  start: number;
  end: number;
  fontName: FigmaFontName;
  fontSize: number;
  lineHeight: number | { unit: string; value: number };
  letterSpacing: number | { unit: string; value: number };
  textCase?: string;
}

/** A variant or component property of an instance. */
export interface FigmaComponentProperty {
  value: string;
  type: string;
}

/** A variable binding of a node property. */
export type FigmaBoundVariable =
  { type: "VARIABLE_ALIAS"; id: string } | { type: "MIXED" } | { type: "BOOLEAN" };

/** A node of the document. */
export interface FigmaNode {
  id: string;
  type: string;
  name: string;
  visible: boolean;
  locked: boolean;
  parent: FigmaNode | null;
  children?: FigmaNode[];
  selection?: FigmaNode[];
  width?: number;
  height?: number;
  x?: number;
  y?: number;
  rotation?: number;
  relativeTransform?: number[][];
  absoluteTransform?: number[][];
  absoluteBoundingBox?: FigmaBounds | null;
  constraints?: { horizontal: string; vertical: string };
  layoutMode?: string;
  itemSpacing?: number;
  paddingTop?: number;
  paddingRight?: number;
  paddingBottom?: number;
  paddingLeft?: number;
  primaryAxisAlignItems?: string;
  counterAxisAlignItems?: string;
  primaryAxisSizingMode?: string;
  counterAxisSizingMode?: string;
  layoutWrap?: string;
  clipsContent?: boolean;
  blendMode?: string;
  description?: string;
  fills?: unknown;
  strokes?: unknown;
  effects?: unknown;
  opacity?: number;
  cornerRadius?: unknown;
  characters?: string;
  fontSize?: unknown;
  fontName?: unknown;
  lineHeight?: unknown;
  letterSpacing?: unknown;
  textAlignHorizontal?: string;
  textAlignVertical?: string;
  textTruncation?: string;
  getStyledTextSegments?: (fields: readonly string[]) => FigmaStyledTextSegment[];
  getMainComponentAsync?: () => Promise<FigmaNode | null>;
  mainComponent?: FigmaNode | null;
  componentProperties?: Record<string, FigmaComponentProperty>;
  boundVariables?: Record<string, FigmaBoundVariable>;
  fillStyleId?: unknown;
  strokeStyleId?: unknown;
  effectStyleId?: unknown;
  gridStyleId?: unknown;
  textStyleId?: unknown;
  exportAsync?: (settings: FigmaExportSettings) => Promise<Uint8Array>;
  /**
   * Append a child, when the node accepts children.
   *
   * The method is optional because not every node of the document can hold
   * children, and because a readonly node refuses the call.
   */
  appendChild?: ((child: FigmaNode) => void) | undefined;
  /** Insert a child at an index, when the node accepts children. */
  insertChild?: ((index: number, child: FigmaNode) => void) | undefined;
  /** Remove the node from its parent. */
  remove?: (() => void) | undefined;
  /** Resize the node, when the node has a size. */
  resize?: ((width: number, height: number) => void) | undefined;
}

/** Export settings accepted by `node.exportAsync`. */
export interface FigmaExportSettings {
  format: "PNG" | "JPG" | "SVG" | "PDF";
  constraint?: { type: "SCALE" | "WIDTH" | "HEIGHT"; value: number };
  svgOutlineText?: boolean;
  svgIncludeId?: boolean;
  useAbsoluteBounds?: boolean;
}

/** A local style. */
export interface FigmaStyle {
  id: string;
  name: string;
  type: string;
  remote: boolean;
  key?: string;
}

/** A local variable. */
export interface FigmaVariable {
  id: string;
  name: string;
  resolvedType?: string;
  variableCollectionId?: string;
  valuesByMode?: Record<string, unknown>;
  scopes?: string[];
  remote?: boolean;
}

/** A local variable collection. */
export interface FigmaVariableCollection {
  id: string;
  name: string;
  modes: Array<{ modeId: string; name: string }>;
  defaultModeId?: string;
  remote?: boolean;
}

/** The variables API, present only on seats that expose it. */
export interface FigmaVariablesApi {
  getLocalVariablesAsync?: (type?: string) => Promise<FigmaVariable[]>;
  getLocalVariableCollectionsAsync?: () => Promise<FigmaVariableCollection[]>;
  getVariableByIdAsync?: (id: string) => Promise<FigmaVariable | null>;
}

/** The team library API, present only on seats that expose it. */
export interface FigmaTeamLibraryApi {
  getAvailableLibraryVariableCollectionsAsync?: () => Promise<unknown>;
  getAvailableLibraryStylesAsync?: () => Promise<unknown>;
}

/**
 * The result of a version checkpoint.
 *
 * The editor resolves the promise with `null` when it wrote no version, so
 * the absence of a version id is a normal outcome and not an error.
 */
export interface FigmaVersionResult {
  /** Id of the version that was written. */
  id: string;
}

/** The UI channel of the plugin. */
export interface FigmaUiApi {
  /** Send a message to the UI iframe. */
  postMessage?: ((message: unknown) => void) | undefined;
  /** Receive messages of the UI iframe. */
  onmessage?: ((message: unknown, options?: unknown) => void) | undefined;
}

/** The subset of the Plugin API the sandbox relies on. */
export interface FigmaApi {
  openExternal?: ((url: string) => void) | undefined;
  /** Document root. */
  readonly root: FigmaNode;
  /** Page the user is looking at. */
  readonly currentPage: FigmaNode;
  /** The mixed value sentinel of the API. */
  readonly mixed: unknown;
  /** Synchronous node lookup. */
  getNodeById?: ((id: string) => FigmaNode | null) | undefined;
  /** Asynchronous node lookup that can reach other loaded pages. */
  getNodeByIdAsync?: ((id: string) => Promise<FigmaNode | null>) | undefined;
  /** Style lookup. */
  getStyleById?: ((id: string) => FigmaStyle | null) | undefined;
  /** Asynchronous style lookup. */
  getStyleByIdAsync?: ((id: string) => Promise<FigmaStyle | null>) | undefined;
  /** Local paint styles. */
  getLocalPaintStylesAsync?: (() => Promise<FigmaStyle[]>) | undefined;
  /** Local text styles. */
  getLocalTextStylesAsync?: (() => Promise<FigmaStyle[]>) | undefined;
  /** Local effect styles. */
  getLocalEffectStylesAsync?: (() => Promise<FigmaStyle[]>) | undefined;
  /** Local grid styles. */
  getLocalGridStylesAsync?: (() => Promise<FigmaStyle[]>) | undefined;
  /** Fonts the editor can render. */
  listAvailableFontsAsync?: (() => Promise<Array<{ fontName: FigmaFontName }>>) | undefined;
  /** Create a frame. */
  createFrame?: (() => FigmaNode) | undefined;
  /** Create a text node. */
  createText?: (() => FigmaNode) | undefined;
  /** Create a rectangle. */
  createRectangle?: (() => FigmaNode) | undefined;
  /** Create a component. */
  createComponent?: (() => FigmaNode) | undefined;
  /** Create a vector network. */
  createVector?: (() => FigmaNode) | undefined;
  /** Create a section. */
  createSection?: (() => FigmaNode) | undefined;
  /** Create a line. */
  createLine?: (() => FigmaNode) | undefined;
  /** Create an ellipse. */
  createEllipse?: (() => FigmaNode) | undefined;
  /**
   * Group nodes under a parent.
   *
   * The Plugin API has no `createGroup`: a group always holds at least one
   * node, so the editor builds it from the nodes it takes over.
   */
  group?:
    ((nodes: readonly FigmaNode[], parent: FigmaNode, index?: number) => FigmaNode) | undefined;
  /** Load a font before its properties or its characters can be written. */
  loadFontAsync?: ((font: FigmaFontName) => Promise<void>) | undefined;
  /** Commit the changes of the plugin to the undo history of the editor. */
  commitUndo?: (() => void) | undefined;
  /** Write a version checkpoint of the file. */
  saveVersionHistoryAsync?:
    ((title: string, description?: string) => Promise<FigmaVersionResult | null>) | undefined;
  /** The variables API. */
  readonly variables?: FigmaVariablesApi | undefined;
  /** The team library API. */
  readonly teamLibrary?: FigmaTeamLibraryApi | undefined;
  /** The UI channel. */
  readonly ui?: FigmaUiApi | undefined;
}

/** Narrow an unknown value to a plain record. */
export function asRecord(value: unknown): Record<string, unknown> | null {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return null;
  }
  return value as Record<string, unknown>;
}

/** Narrow an unknown value to a finite number. */
export function asNumber(value: unknown): number | undefined {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return undefined;
  }
  return value;
}

/** Narrow an unknown value to a string. */
export function asString(value: unknown): string | undefined {
  if (typeof value !== "string") {
    return undefined;
  }
  return value;
}

/** Narrow an unknown value to a boolean. */
export function asBoolean(value: unknown): boolean | undefined {
  if (typeof value !== "boolean") {
    return undefined;
  }
  return value;
}

/** Narrow an unknown value to an array. */
export function asArray(value: unknown): unknown[] | undefined {
  if (!Array.isArray(value)) {
    return undefined;
  }
  return value;
}

/** Whether a value is the API mixed sentinel. */
export function isMixed(api: FigmaApi, value: unknown): boolean {
  return api.mixed !== undefined && value === api.mixed;
}

/**
 * Resolve the Figma API from a global scope.
 *
 * The runtime check keeps the plugin honest when the editor does not expose
 * a member: a missing member becomes `undefined` and the capability probe
 * reports it, instead of the plugin crashing on an optional API.
 */
export function resolveFigmaApi(scope: unknown): FigmaApi {
  const scopeRecord = asRecord(scope);
  if (scopeRecord === null) {
    throw new Error("the plugin sandbox does not expose a global scope");
  }
  const candidate = scopeRecord["figma"];
  const api = asRecord(candidate);
  if (api === null) {
    throw new Error("the plugin sandbox does not expose the figma API");
  }
  const root = asRecord(api["root"]);
  const currentPage = asRecord(api["currentPage"]);
  if (root === null || currentPage === null) {
    throw new Error("the figma API does not expose a document root");
  }
  return {
    root: root as unknown as FigmaNode,
    openExternal: asFunction<(url: string) => void>(api["openExternal"]),
    currentPage: currentPage as unknown as FigmaNode,
    mixed: api["mixed"],
    getNodeById: asFunction<(id: string) => FigmaNode | null>(api["getNodeById"]),
    getNodeByIdAsync: asFunction<(id: string) => Promise<FigmaNode | null>>(
      api["getNodeByIdAsync"],
    ),
    getStyleById: asFunction<(id: string) => FigmaStyle | null>(api["getStyleById"]),
    getStyleByIdAsync: asFunction<(id: string) => Promise<FigmaStyle | null>>(
      api["getStyleByIdAsync"],
    ),
    getLocalPaintStylesAsync: asFunction<() => Promise<FigmaStyle[]>>(
      api["getLocalPaintStylesAsync"],
    ),
    getLocalTextStylesAsync: asFunction<() => Promise<FigmaStyle[]>>(
      api["getLocalTextStylesAsync"],
    ),
    getLocalEffectStylesAsync: asFunction<() => Promise<FigmaStyle[]>>(
      api["getLocalEffectStylesAsync"],
    ),
    getLocalGridStylesAsync: asFunction<() => Promise<FigmaStyle[]>>(
      api["getLocalGridStylesAsync"],
    ),
    listAvailableFontsAsync: asFunction<() => Promise<Array<{ fontName: FigmaFontName }>>>(
      api["listAvailableFontsAsync"],
    ),
    createFrame: asFunction<() => FigmaNode>(api["createFrame"]),
    createText: asFunction<() => FigmaNode>(api["createText"]),
    createRectangle: asFunction<() => FigmaNode>(api["createRectangle"]),
    createComponent: asFunction<() => FigmaNode>(api["createComponent"]),
    createVector: asFunction<() => FigmaNode>(api["createVector"]),
    createSection: asFunction<() => FigmaNode>(api["createSection"]),
    createLine: asFunction<() => FigmaNode>(api["createLine"]),
    createEllipse: asFunction<() => FigmaNode>(api["createEllipse"]),
    group: asFunction<
      (nodes: readonly FigmaNode[], parent: FigmaNode, index?: number) => FigmaNode
    >(api["group"]),
    loadFontAsync: asFunction<(font: FigmaFontName) => Promise<void>>(api["loadFontAsync"]),
    commitUndo: asFunction<() => void>(api["commitUndo"]),
    saveVersionHistoryAsync: asFunction<
      (title: string, description?: string) => Promise<FigmaVersionResult | null>
    >(api["saveVersionHistoryAsync"]),
    variables: optionalRecord(api["variables"]) as FigmaVariablesApi | undefined,
    teamLibrary: optionalApiRecord(api, "teamLibrary") as FigmaTeamLibraryApi | undefined,
    ui: optionalRecord(api["ui"]) as FigmaUiApi | undefined,
  };
}

function asFunction<T>(value: unknown): T | undefined {
  if (typeof value !== "function") {
    return undefined;
  }
  return value as T;
}

function optionalRecord(value: unknown): Record<string, unknown> | undefined {
  return asRecord(value) ?? undefined;
}

/** Some optional Figma API getters throw when their manifest permission is absent. */
function optionalApiRecord(
  api: Record<string, unknown>,
  key: string,
): Record<string, unknown> | undefined {
  try {
    return optionalRecord(api[key]);
  } catch {
    return undefined;
  }
}
