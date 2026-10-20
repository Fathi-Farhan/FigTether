import { describe, expect, it } from "vitest";

import { PluginError } from "../src/errors.js";
import {
  handleCaptureSnapshot,
  handleExportAssets,
  handleGetDesignTokens,
  handleGetSelection,
  handleInspectDesign,
  handleListNodes,
  handleReadNodes,
  handleWatchScope,
  type HandlerContext,
} from "../src/handlers.js";
import { FontCatalog } from "../src/normalize.js";
import { ScopeGuard } from "../src/scope.js";
import { TokenResolver } from "../src/tokens.js";
import { createFakeDocument, type FakeDocument } from "./fake-figma.js";

function createContext(
  document: FakeDocument,
  rootIds: string[],
): { context: HandlerContext; guard: ScopeGuard } {
  const guard = new ScopeGuard(document.api);
  guard.setGrant({ rootIds, grantEpoch: 1 });
  const context: HandlerContext = {
    requestId: "req_1",
    sessionId: "session_1",
    grantEpoch: 1,
    api: document.api,
    guard,
    resolver: new TokenResolver(document.api),
    fontCatalog: new FontCatalog(document.api),
    capabilities: { capabilities: {}, notes: {} },
    isCancelled: () => false,
    onProgress: () => undefined,
  };
  return { context, guard };
}

function sampleDocument(): FakeDocument {
  return createFakeDocument(
    [
      {
        id: "1:1",
        type: "FRAME",
        name: "Root",
        width: 200,
        height: 100,
        children: [
          {
            id: "1:2",
            type: "TEXT",
            name: "Label",
            characters: "Pay now",
            fontName: { family: "Inter", style: "Semi Bold" },
            fontSize: 14,
            lineHeight: 20,
            letterSpacing: 0.2,
            textAlignHorizontal: "LEFT",
          },
          {
            id: "1:3",
            type: "FRAME",
            name: "Nested",
            children: [{ id: "1:4", type: "ELLIPSE", name: "Dot" }],
          },
        ],
      },
      {
        id: "2:1",
        type: "FRAME",
        name: "Other root",
        children: [{ id: "2:2", type: "ELLIPSE", name: "Other dot" }],
      },
    ],
    {
      selection: ["1:2"],
      fonts: [
        { family: "Inter", style: "Regular" },
        { family: "Inter", style: "Semi Bold" },
      ],
    },
  );
}

describe("list_nodes", () => {
  it("lists the nodes under the granted root", async () => {
    const document = sampleDocument();
    const { context } = createContext(document, ["1:1"]);
    const result = await handleListNodes({ root_id: "1:1" }, context);
    expect(result.root_id).toBe("1:1");
    expect(result.nodes.map((node) => node.id)).toEqual(["1:1", "1:2", "1:3", "1:4"]);
    expect(result.truncated).toBe(false);
    expect(result.next_cursor).toBeNull();
  });

  it("refuses a root outside the grant", async () => {
    const document = sampleDocument();
    const { context } = createContext(document, ["1:1"]);
    const error = await handleListNodes({ root_id: "2:1" }, context).catch(
      (reason: unknown) => reason,
    );
    expect(error).toBeInstanceOf(PluginError);
    expect((error as PluginError).code).toBe("OUT_OF_SCOPE");
  });

  it("refuses a root that does not exist", async () => {
    const document = sampleDocument();
    const { context } = createContext(document, ["1:1"]);
    const error = await handleListNodes({ root_id: "9:9" }, context).catch(
      (reason: unknown) => reason,
    );
    expect((error as PluginError).code).toBe("NODE_NOT_FOUND");
  });

  it("applies the field projection", async () => {
    const document = sampleDocument();
    const { context } = createContext(document, ["1:1"]);
    const result = await handleListNodes({ root_id: "1:1", fields: ["id", "name"] }, context);
    const first = result.nodes[0];
    expect(first?.geometry).toBeUndefined();
    expect(first?.unavailable_fields).toContain("geometry:projection_omitted");
  });

  it("filters the node types", async () => {
    const document = sampleDocument();
    const { context } = createContext(document, ["1:1"]);
    const result = await handleListNodes({ root_id: "1:1", filters: { types: ["TEXT"] } }, context);
    expect(result.nodes.map((node) => node.node_type)).toEqual(["TEXT"]);
  });
});

describe("read_nodes", () => {
  it("reports a per item error for a node outside the grant", async () => {
    const document = sampleDocument();
    const { context } = createContext(document, ["1:2"]);
    const result = await handleReadNodes({ node_ids: ["1:2", "1:3"] }, context);
    expect(result.batch_status).toBe("partial");
    expect(result.summary.ok).toBe(1);
    expect(result.summary.error).toBe(1);
    const failed = result.items[1];
    expect(failed?.status).toBe("error");
    if (failed?.status === "error") {
      expect(failed.error.code).toBe("OUT_OF_SCOPE");
    }
  });

  it("refuses a batch above the batch limit", async () => {
    const document = sampleDocument();
    const { context } = createContext(document, ["1:1"]);
    const nodeIds = Array.from({ length: 51 }, (_value, index) => `1:${index}`);
    const error = await handleReadNodes({ node_ids: nodeIds }, context).catch(
      (reason: unknown) => reason,
    );
    expect((error as PluginError).code).toBe("INVALID_LIMIT");
  });

  it("refuses a depth above the depth limit", async () => {
    const document = sampleDocument();
    const { context } = createContext(document, ["1:1"]);
    const error = await handleReadNodes({ node_ids: ["1:1"], depth: 40 }, context).catch(
      (reason: unknown) => reason,
    );
    expect((error as PluginError).code).toBe("INVALID_LIMIT");
  });
});

describe("inspect_design", () => {
  it("reports the typography of a text node", async () => {
    const document = sampleDocument();
    const { context } = createContext(document, ["1:1"]);
    const result = await handleInspectDesign({ root_id: "1:1" }, context);
    const text = result.nodes.find((node) => node.id === "1:2");
    expect(text?.text?.text).toBe("Pay now");
    expect(text?.text?.runs[0]?.font_family).toBe("Inter");
    expect(text?.text?.runs[0]?.font_weight).toBe(600);
    expect(text?.text?.runs[0]?.font_size).toBe(14);
    expect(text?.text?.runs[0]?.font_available).toBe(true);
  });

  it("keeps an unknown node type verbatim", async () => {
    const document = createFakeDocument([{ id: "1:1", type: "FUTURE_THING", name: "mystery" }]);
    const { context } = createContext(document, ["1:1"]);
    const result = await handleInspectDesign({ root_id: "1:1" }, context);
    expect(result.nodes[0]?.node_type).toBe("FUTURE_THING");
    expect(result.unsupported.map((entry) => entry.node_type)).toEqual(["FUTURE_THING"]);
    expect(result.coverage.unsupported_types).toEqual(["FUTURE_THING"]);
  });
});

describe("get_design_tokens", () => {
  it("reports the variables of the scope", async () => {
    const document = createFakeDocument(
      [
        {
          id: "1:1",
          boundVariables: {
            fills: { type: "VARIABLE_ALIAS", id: "VariableID:1:2" },
          },
        },
      ],
      {
        variables: [
          {
            id: "VariableID:1:2",
            name: "color/primary",
            variableCollectionId: "collection_1",
            valuesByMode: { mode_1: { r: 1, g: 0, b: 0, a: 1 } },
          },
        ],
        collections: [
          {
            id: "collection_1",
            name: "Theme",
            modes: [{ modeId: "mode_1", name: "Light" }],
            defaultModeId: "mode_1",
          },
        ],
      },
    );
    const { context } = createContext(document, ["1:1"]);
    const result = await handleGetDesignTokens({ root_id: "1:1" }, context);
    expect(result.variables).toHaveLength(1);
    expect(result.variables[0]?.name).toBe("color/primary");
    expect(result.variables[0]?.collection).toBe("Theme");
    expect(result.variables[0]?.mode).toBe("Light");
    expect(result.variables[0]?.resolved).toBe(true);
    expect(result.unresolved).toHaveLength(0);
  });

  it("reports an unresolved reference instead of dropping it", async () => {
    const document = createFakeDocument([
      {
        id: "1:1",
        boundVariables: {
          fills: { type: "VARIABLE_ALIAS", id: "VariableID:9:9" },
        },
      },
    ]);
    const { context } = createContext(document, ["1:1"]);
    const result = await handleGetDesignTokens({ root_id: "1:1" }, context);
    expect(result.variables).toHaveLength(1);
    expect(result.variables[0]?.resolved).toBe(false);
    expect(result.unresolved[0]?.id).toBe("VariableID:9:9");
  });

  it("reports the variables as unavailable when the variables API is missing", async () => {
    const document = createFakeDocument([{ id: "1:1" }], {
      variablesAvailable: false,
    });
    const { context } = createContext(document, ["1:1"]);
    const result = await handleGetDesignTokens({ root_id: "1:1" }, context);
    expect(result.coverage.unavailable_fields).toContain("variables:capability_unavailable");
    expect(result.variables).toEqual([]);
  });

  it("answers with the capability error when no token API is exposed", async () => {
    const document = createFakeDocument([{ id: "1:1" }], {
      variablesAvailable: false,
    });
    document.api.getLocalPaintStylesAsync = undefined;
    document.api.getStyleById = undefined;
    const { context } = createContext(document, ["1:1"]);
    const error = await handleGetDesignTokens({ root_id: "1:1" }, context).catch(
      (reason: unknown) => reason,
    );
    expect(error).toBeInstanceOf(PluginError);
    expect((error as PluginError).code).toBe("CAPABILITY_UNAVAILABLE");
  });
});

describe("export_assets", () => {
  it("exports a node as png", async () => {
    const document = createFakeDocument([
      { id: "1:1", width: 100, height: 50, exportBytes: new Uint8Array([1, 2, 3]) },
    ]);
    const { context } = createContext(document, ["1:1"]);
    const result = await handleExportAssets(
      { node_ids: ["1:1"], format: "png", scale: 1 },
      context,
    );
    expect(result.batch_status).toBe("complete");
    const item = result.items[0];
    if (item?.status !== "ok") {
      throw new Error("the export did not succeed");
    }
    expect(item.export.mime).toBe("image/png");
    expect(item.export.width).toBe(100);
    expect(item.export.height).toBe(50);
    expect(item.export.bytes).toBe("AQID");
    expect(item.export.hash).toMatch(/^sha256:[0-9a-f]{64}$/);
  });

  it("refuses an export above sixteen megapixels", async () => {
    const document = createFakeDocument([{ id: "1:1", width: 5000, height: 5000 }]);
    const { context } = createContext(document, ["1:1"]);
    const result = await handleExportAssets(
      { node_ids: ["1:1"], format: "png", scale: 1 },
      context,
    );
    const item = result.items[0];
    if (item?.status !== "error") {
      throw new Error("the export should have been refused");
    }
    expect(item.error.code).toBe("PAYLOAD_TOO_LARGE");
  });

  it("refuses a scale outside the range", async () => {
    const document = createFakeDocument([{ id: "1:1", width: 10, height: 10 }]);
    const { context } = createContext(document, ["1:1"]);
    const error = await handleExportAssets(
      { node_ids: ["1:1"], format: "png", scale: 4 },
      context,
    ).catch((reason: unknown) => reason);
    expect((error as PluginError).code).toBe("INVALID_LIMIT");
  });

  it("reports a font failure as FONT_UNAVAILABLE", async () => {
    const document = createFakeDocument([
      {
        id: "1:1",
        width: 10,
        height: 10,
        exportError: "font Inter Regular is missing",
      },
    ]);
    const { context } = createContext(document, ["1:1"]);
    const result = await handleExportAssets({ node_ids: ["1:1"], format: "png" }, context);
    const item = result.items[0];
    if (item?.status !== "error") {
      throw new Error("the export should have failed");
    }
    expect(item.error.code).toBe("FONT_UNAVAILABLE");
  });
});

describe("get_selection", () => {
  it("reports only the selection inside the grant", async () => {
    const document = sampleDocument();
    const { context } = createContext(document, ["1:3"]);
    const result = await handleGetSelection({}, context);
    expect(result.nodes).toHaveLength(0);
    expect(result.selection_count).toBe(0);
  });

  it("reports the selection inside the grant", async () => {
    const document = sampleDocument();
    const { context } = createContext(document, ["1:1"]);
    const result = await handleGetSelection({}, context);
    expect(result.nodes.map((node) => node.id)).toEqual(["1:2"]);
  });
});

describe("capture_snapshot", () => {
  it("returns the document and a stable content hash", async () => {
    const document = sampleDocument();
    const { context } = createContext(document, ["1:1"]);
    const first = await handleCaptureSnapshot({}, context);
    const second = await handleCaptureSnapshot({}, context);
    expect(first.document.nodes.map((node) => node.id)).toEqual(["1:1", "1:2", "1:3", "1:4"]);
    expect(first.content_hash).toMatch(/^sha256:[0-9a-f]{64}$/);
    expect(first.content_hash).toBe(second.content_hash);
    expect(first.consistency).toBe("best_effort");
    expect(first.document.page_name).toBe("Page 1");
  });

  it("changes the hash when the document changes", async () => {
    const document = sampleDocument();
    const { context } = createContext(document, ["1:1"]);
    const before = await handleCaptureSnapshot({}, context);
    const node = document.nodes.get("1:2");
    if (node !== undefined) {
      node.name = "Changed";
    }
    const after = await handleCaptureSnapshot({}, context);
    expect(after.content_hash).not.toBe(before.content_hash);
  });
});

describe("phase p1 operations", () => {
  it("answers watch_scope with the capability error", async () => {
    const document = sampleDocument();
    const { context } = createContext(document, ["1:1"]);
    const error = await handleWatchScope({ root_id: "1:1" }, context).catch(
      (reason: unknown) => reason,
    );
    expect(error).toBeInstanceOf(PluginError);
    expect((error as PluginError).code).toBe("CAPABILITY_UNAVAILABLE");
    expect((error as PluginError).message).not.toMatch(/watch_id/);
  });
});
