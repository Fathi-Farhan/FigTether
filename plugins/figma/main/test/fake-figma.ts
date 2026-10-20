/**
 * A fake Figma API for the unit tests.
 *
 * The fake implements the injectable `FigmaApi` interface with a small
 * document tree, so the handlers can be exercised without an editor.
 */

import type {
  FigmaApi,
  FigmaFontName,
  FigmaNode,
  FigmaStyle,
  FigmaVariable,
  FigmaVariableCollection,
} from "../src/figma-api.js";

/** The mixed sentinel of the fake API. */
export const MIXED = { type: "MIXED" };

/** Description of one node of the fake document. */
export interface FakeNodeSpec {
  id: string;
  type?: string;
  name?: string;
  visible?: boolean;
  locked?: boolean;
  width?: number;
  height?: number;
  x?: number;
  y?: number;
  rotation?: number;
  layoutMode?: string;
  fills?: unknown;
  strokes?: unknown;
  effects?: unknown;
  characters?: string;
  fontName?: { family: string; style: string };
  fontSize?: number;
  lineHeight?: number | { unit: string; value: number };
  letterSpacing?: number | { unit: string; value: number };
  textAlignHorizontal?: string;
  textTruncation?: string;
  segments?: Array<{
    characters: string;
    start: number;
    end: number;
    fontName: { family: string; style: string };
    fontSize: number;
    lineHeight: number;
    letterSpacing: number;
  }>;
  boundVariables?: Record<string, { type: "VARIABLE_ALIAS"; id: string }>;
  fillStyleId?: string;
  strokeStyleId?: string;
  effectStyleId?: string;
  textStyleId?: string;
  componentProperties?: Record<string, { value: string; type: string }>;
  mainComponentId?: string;
  exportBytes?: Uint8Array;
  exportError?: string;
  children?: FakeNodeSpec[];
}

/** The fake document and its API. */
export interface FakeDocument {
  /** The API the handlers receive. */
  api: FigmaApi;
  /** Every node of the document by id. */
  nodes: Map<string, FigmaNode>;
  /** The page of the document. */
  page: FigmaNode;
  /** The root of the document. */
  root: FigmaNode;
  /** The nodes the editor reports as selected. */
  selection: FigmaNode[];
  /** The styles the fake API exposes. */
  styles: FigmaStyle[];
  /** The variables the fake API exposes. */
  variables: FigmaVariable[];
  /** The variable collections the fake API exposes. */
  collections: FigmaVariableCollection[];
  /** Whether the variables API is exposed. */
  variablesAvailable: boolean;
  /** Whether the team library API is exposed. */
  teamLibraryAvailable: boolean;
  /** Number of export calls. */
  exportCalls: number;
  /** Every mutation the fake applied, in the order it happened. */
  mutationLog: string[];
  /** Number of calls to the undo commit. */
  commitUndoCalls: number;
  /** The fonts the fake loaded. */
  fontLoads: Array<{ family: string; style: string }>;
  /** The version checkpoints the fake wrote. */
  versionSaves: Array<{ title: string; description: string | undefined }>;
}

/** Build a fake document from a node description. */
export function createFakeDocument(
  specs: FakeNodeSpec[],
  options?: {
    selection?: string[];
    styles?: FigmaStyle[];
    variables?: FigmaVariable[];
    collections?: FigmaVariableCollection[];
    variablesAvailable?: boolean;
    teamLibraryAvailable?: boolean;
    fonts?: Array<{ family: string; style: string }>;
    /** Whether the editor exposes the version history of the file. */
    versionHistory?: boolean;
  },
): FakeDocument {
  const nodes = new Map<string, FigmaNode>();
  const selection: FigmaNode[] = [];
  const selectionIds = new Set(options?.selection ?? []);
  const mutationLog: string[] = [];
  const fontLoads: Array<{ family: string; style: string }> = [];
  const versionSaves: Array<{ title: string; description: string | undefined }> = [];
  const state = { commitUndoCalls: 0, created: 0 };
  const versionHistory = options?.versionHistory ?? true;

  const root: FigmaNode = {
    id: "0:0",
    type: "DOCUMENT",
    name: "Fake document",
    visible: true,
    locked: false,
    parent: null,
    children: [],
  };
  const page: FigmaNode = {
    id: "0:1",
    type: "PAGE",
    name: "Page 1",
    visible: true,
    locked: false,
    parent: root,
    children: [],
    selection: [],
  };
  root.children = [page];
  nodes.set(root.id, root);
  nodes.set(page.id, page);

  const build = (spec: FakeNodeSpec, parent: FigmaNode): FigmaNode => {
    const node: FigmaNode = {
      id: spec.id,
      type: spec.type ?? "FRAME",
      name: spec.name ?? spec.id,
      visible: spec.visible ?? true,
      locked: spec.locked ?? false,
      parent,
      children: [],
    };
    if (spec.width !== undefined) {
      node.width = spec.width;
    }
    if (spec.height !== undefined) {
      node.height = spec.height;
    }
    if (spec.x !== undefined) {
      node.x = spec.x;
    }
    if (spec.y !== undefined) {
      node.y = spec.y;
    }
    if (spec.rotation !== undefined) {
      node.rotation = spec.rotation;
    }
    if (spec.layoutMode !== undefined) {
      node.layoutMode = spec.layoutMode;
    }
    if (spec.fills !== undefined) {
      node.fills = spec.fills;
    }
    if (spec.strokes !== undefined) {
      node.strokes = spec.strokes;
    }
    if (spec.effects !== undefined) {
      node.effects = spec.effects;
    }
    if (spec.characters !== undefined) {
      node.characters = spec.characters;
    }
    if (spec.fontName !== undefined) {
      node.fontName = spec.fontName;
    }
    if (spec.fontSize !== undefined) {
      node.fontSize = spec.fontSize;
    }
    if (spec.lineHeight !== undefined) {
      node.lineHeight = spec.lineHeight;
    }
    if (spec.letterSpacing !== undefined) {
      node.letterSpacing = spec.letterSpacing;
    }
    if (spec.textAlignHorizontal !== undefined) {
      node.textAlignHorizontal = spec.textAlignHorizontal;
    }
    if (spec.textTruncation !== undefined) {
      node.textTruncation = spec.textTruncation;
    }
    if (spec.boundVariables !== undefined) {
      node.boundVariables = spec.boundVariables;
    }
    if (spec.fillStyleId !== undefined) {
      node.fillStyleId = spec.fillStyleId;
    }
    if (spec.strokeStyleId !== undefined) {
      node.strokeStyleId = spec.strokeStyleId;
    }
    if (spec.effectStyleId !== undefined) {
      node.effectStyleId = spec.effectStyleId;
    }
    if (spec.textStyleId !== undefined) {
      node.textStyleId = spec.textStyleId;
    }
    if (spec.componentProperties !== undefined) {
      node.componentProperties = spec.componentProperties;
    }
    if (spec.mainComponentId !== undefined) {
      node.mainComponent = {
        id: spec.mainComponentId,
        type: "COMPONENT",
        name: spec.mainComponentId,
        visible: true,
        locked: false,
        parent: null,
        children: [],
      };
    }
    if (spec.segments !== undefined) {
      node.getStyledTextSegments = () => spec.segments ?? [];
    }
    if (spec.exportBytes !== undefined || spec.exportError !== undefined) {
      node.exportAsync = async (): Promise<Uint8Array> => {
        document.exportCalls += 1;
        if (spec.exportError !== undefined) {
          throw new Error(spec.exportError);
        }
        return spec.exportBytes ?? new Uint8Array([1, 2, 3]);
      };
    }
    nodes.set(node.id, node);
    parent.children = [...(parent.children ?? []), node];
    attachMutations(node);
    for (const child of spec.children ?? []) {
      build(child, node);
    }
    if (selectionIds.has(node.id)) {
      selection.push(node);
    }
    return node;
  };

  for (const spec of specs) {
    build(spec, page);
  }
  page.selection = selection;

  /**
   * Create a node of a type, the way the editor does.
   *
   * A new node lands under the current page and is registered, so the
   * lookup by id finds it.
   */
  function createNodeOf(type: string, name: string): FigmaNode {
    state.created += 1;
    const id = `new:${state.created}`;
    const node: FigmaNode = {
      id,
      type,
      name,
      visible: true,
      locked: false,
      parent: page,
      children: [],
    };
    attachMutations(node);
    nodes.set(id, node);
    page.children = [...(page.children ?? []), node];
    mutationLog.push(`create:${type}`);
    return node;
  }

  /**
   * Give a node the mutations the editor exposes on it.
   *
   * The fake reproduces the refusal cases of a reparenting, so the plugin can
   * be tested against them.
   */
  function attachMutations(node: FigmaNode): void {
    if (node.type === "TEXT") {
      let characters = node.characters ?? "";
      Object.defineProperty(node, "characters", {
        get: (): string => characters,
        set: (value: string): void => {
          characters = value;
          mutationLog.push(`characters:${node.id}:${value}`);
        },
        enumerable: true,
        configurable: true,
      });
    }
    node.appendChild = (child: FigmaNode): void => {
      mutationLog.push(`appendChild:${node.id}:${child.id}`);
      assertReparentAllowed(node, child);
      detach(child);
      node.children = [...(node.children ?? []), child];
      child.parent = node;
    };
    node.insertChild = (index: number, child: FigmaNode): void => {
      mutationLog.push(`insertChild:${node.id}:${index}:${child.id}`);
      assertReparentAllowed(node, child);
      const children = [...(node.children ?? [])];
      if (index > children.length) {
        throw new Error("insertChild: the index is outside the parent");
      }
      detach(child);
      children.splice(index, 0, child);
      node.children = children;
      child.parent = node;
    };
    node.remove = (): void => {
      mutationLog.push(`remove:${node.id}`);
      detach(node);
    };
    node.resize = (width: number, height: number): void => {
      mutationLog.push(`resize:${node.id}:${width}x${height}`);
      node.width = width;
      node.height = height;
    };
  }

  /** Take a node out of the parent it is in. */
  function detach(node: FigmaNode): void {
    const parent = node.parent;
    if (parent === null || parent === undefined) {
      return;
    }
    parent.children = (parent.children ?? []).filter((entry) => entry.id !== node.id);
  }

  /**
   * Refuse a reparenting the editor refuses.
   *
   * The cases are the ones the Plugin API documents: a read only node, a
   * node inside an instance, a node into its own descendant and a component
   * into itself.
   */
  function assertReparentAllowed(parent: FigmaNode, child: FigmaNode): void {
    if (child.locked) {
      throw new Error("The node is read-only");
    }
    if (isInsideInstance(child)) {
      throw new Error("Cannot move a child of an instance");
    }
    if (isDescendantOf(parent, child)) {
      if (child.type === "COMPONENT" || child.type === "COMPONENT_SET") {
        throw new Error("Cannot move a component into itself");
      }
      throw new Error("Cannot move a node into its own descendant");
    }
  }

  /** Whether a node lives inside an instance. */
  function isInsideInstance(node: FigmaNode): boolean {
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

  /** Whether a node is a descendant of another node. */
  function isDescendantOf(node: FigmaNode, ancestor: FigmaNode): boolean {
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

  const styles = options?.styles ?? [];
  const variables = options?.variables ?? [];
  const collections = options?.collections ?? [];
  const variablesAvailable = options?.variablesAvailable ?? true;
  const teamLibraryAvailable = options?.teamLibraryAvailable ?? false;

  const variablesApi = variablesAvailable
    ? {
        getLocalVariablesAsync: async (): Promise<FigmaVariable[]> => variables,
        getLocalVariableCollectionsAsync: async (): Promise<FigmaVariableCollection[]> =>
          collections,
        getVariableByIdAsync: async (id: string): Promise<FigmaVariable | null> =>
          variables.find((variable) => variable.id === id) ?? null,
      }
    : undefined;

  const document: FakeDocument = {
    nodes,
    page,
    root,
    selection,
    styles,
    variables,
    collections,
    variablesAvailable,
    teamLibraryAvailable,
    exportCalls: 0,
    mutationLog,
    get commitUndoCalls(): number {
      return state.commitUndoCalls;
    },
    fontLoads,
    versionSaves,
    api: {
      root,
      currentPage: page,
      mixed: MIXED,
      getNodeById: (id: string): FigmaNode | null => nodes.get(id) ?? null,
      getNodeByIdAsync: async (id: string): Promise<FigmaNode | null> => nodes.get(id) ?? null,
      getStyleById: (id: string): FigmaStyle | null =>
        styles.find((style) => style.id === id) ?? null,
      getLocalPaintStylesAsync: async (): Promise<FigmaStyle[]> => styles,
      listAvailableFontsAsync: async (): Promise<
        Array<{ fontName: { family: string; style: string } }>
      > =>
        (options?.fonts ?? [{ family: "Inter", style: "Regular" }]).map((font) => ({
          fontName: font,
        })),
      variables: variablesApi,
      teamLibrary: teamLibraryAvailable ? {} : undefined,
      createFrame: (): FigmaNode => createNodeOf("FRAME", "Frame"),
      createText: (): FigmaNode => createNodeOf("TEXT", "Text"),
      createRectangle: (): FigmaNode => createNodeOf("RECTANGLE", "Rectangle"),
      createComponent: (): FigmaNode => createNodeOf("COMPONENT", "Component"),
      createVector: (): FigmaNode => createNodeOf("VECTOR", "Vector"),
      createSection: (): FigmaNode => createNodeOf("SECTION", "Section"),
      createLine: (): FigmaNode => createNodeOf("LINE", "Line"),
      createEllipse: (): FigmaNode => createNodeOf("ELLIPSE", "Ellipse"),
      group: (groupNodes: readonly FigmaNode[], parent: FigmaNode, index?: number): FigmaNode => {
        const group = createNodeOf("GROUP", "Group");
        for (const node of groupNodes) {
          group.appendChild?.(node);
        }
        if (index === undefined) {
          parent.appendChild?.(group);
        } else {
          parent.insertChild?.(index, group);
        }
        return group;
      },
      loadFontAsync: async (font: FigmaFontName): Promise<void> => {
        fontLoads.push({ family: font.family, style: font.style });
        mutationLog.push(`loadFont:${font.family}:${font.style}`);
        const available = options?.fonts ?? [{ family: "Inter", style: "Regular" }];
        const known = available.some(
          (entry) => entry.family === font.family && entry.style === font.style,
        );
        if (!known) {
          throw new Error(`Cannot load the font ${font.family} ${font.style}`);
        }
      },
      commitUndo: (): void => {
        state.commitUndoCalls += 1;
        mutationLog.push("commitUndo");
      },
      saveVersionHistoryAsync:
        versionHistory === false
          ? undefined
          : async (title: string, description?: string): Promise<{ id: string } | null> => {
              versionSaves.push({ title, description });
              mutationLog.push(`saveVersion:${title}`);
              return { id: `version:${versionSaves.length}` };
            },
    },
  };

  return document;
}
