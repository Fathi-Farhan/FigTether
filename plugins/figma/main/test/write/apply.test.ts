import { describe, expect, it } from "vitest";

import { handleApplyWrite } from "../../src/write/handlers.js";
import { parseWritePlan, planHash } from "../../src/write/plan.js";
import { createWriteContext, planPayload, sampleDocument } from "./write-fixture.js";

describe("apply_write", () => {
  it("creates the node of the plan and reports its real id", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const result = await handleApplyWrite(
      planPayload([
        { kind: "create_node", parent_id: "1:1", node_type: "FRAME", properties: { name: "New" } },
      ]),
      context,
    );
    expect(result.status).toBe("applied");
    expect(result.created).toHaveLength(1);
    expect(result.created[0]).toBe("new:1");
    expect(result.applied_operations).toBe(1);
    const created = document.nodes.get("new:1");
    expect(created?.name).toBe("New");
    expect(created?.parent?.id).toBe("1:1");
  });

  it("mutates the nodes of the plan as the plan describes", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const result = await handleApplyWrite(
      planPayload([
        { kind: "set_properties", node_id: "1:2", properties: { x: 42, name: "Renamed" } },
        { kind: "set_fills", node_id: "1:4", fills: [{ type: "SOLID", color: "#ff0000" }] },
      ]),
      context,
    );
    expect(result.status).toBe("applied");
    expect(result.changed).toEqual(["1:2", "1:4"]);
    const label = document.nodes.get("1:2");
    expect(label?.x).toBe(42);
    expect(label?.name).toBe("Renamed");
    const dot = document.nodes.get("1:4");
    expect(dot?.fills).toEqual([
      { type: "SOLID", color: { r: 1, g: 0, b: 0, a: 1 }, opacity: 1, visible: true },
    ]);
  });

  it("commits the undo history exactly once", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const result = await handleApplyWrite(
      planPayload([
        { kind: "create_node", parent_id: "1:1", node_type: "FRAME" },
        { kind: "set_properties", node_id: "1:2", properties: { x: 1 } },
      ]),
      context,
    );
    expect(result.undo_committed).toBe(true);
    expect(document.commitUndoCalls).toBe(1);
  });

  it("never commits the undo history in the preview", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const { handlePreviewWrite } = await import("../../src/write/handlers.js");
    await handlePreviewWrite(
      planPayload([{ kind: "create_node", parent_id: "1:1", node_type: "FRAME" }]),
      context,
    );
    expect(document.commitUndoCalls).toBe(0);
  });

  it("loads the font before it writes the characters", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    await handleApplyWrite(
      planPayload([
        {
          kind: "set_text",
          node_id: "1:2",
          text: "Now",
          font_family: "Inter",
          font_style: "Semi Bold",
        },
      ]),
      context,
    );
    expect(document.fontLoads).toEqual([{ family: "Inter", style: "Semi Bold" }]);
    const fontIndex = document.mutationLog.indexOf("loadFont:Inter:Semi Bold");
    const textIndex = document.mutationLog.indexOf("characters:1:2:Now");
    expect(fontIndex).toBeGreaterThanOrEqual(0);
    expect(textIndex).toBeGreaterThan(fontIndex);
  });

  it("reports a font the editor cannot load", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const result = await handleApplyWrite(
      planPayload([
        {
          kind: "set_text",
          node_id: "1:2",
          text: "Now",
          font_family: "Ghost",
          font_style: "Regular",
        },
      ]),
      context,
    );
    expect(result.status).toBe("partial");
    expect(result.error?.code).toBe("FONT_UNAVAILABLE");
    expect(result.error?.reason).toBe("font_unavailable");
    expect(document.nodes.get("1:2")?.characters).toBe("Pay now");
  });

  it("reports the partial state of a plan that fails halfway", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const result = await handleApplyWrite(
      planPayload([
        { kind: "create_node", parent_id: "1:1", node_type: "FRAME", properties: { name: "New" } },
        {
          kind: "set_text",
          node_id: "1:2",
          text: "Now",
          font_family: "Ghost",
          font_style: "Regular",
        },
      ]),
      context,
    );
    expect(result.status).toBe("partial");
    expect(result.applied_operations).toBe(1);
    expect(result.created).toHaveLength(1);
    expect(result.error?.code).toBe("FONT_UNAVAILABLE");
    expect(result.undo_committed).toBe(true);
    expect(document.commitUndoCalls).toBe(1);
  });

  it("removes a node and reports it", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const result = await handleApplyWrite(
      planPayload([{ kind: "delete_node", node_id: "1:4" }], { destructive: true }),
      context,
    );
    expect(result.status).toBe("applied");
    expect(result.removed).toEqual(["1:4"]);
    expect(document.nodes.get("1:3")?.children).toEqual([]);
  });

  it("moves a node under another parent and reports it", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const result = await handleApplyWrite(
      planPayload([
        { kind: "insert_child", parent_id: "1:3", node_id: "1:2" },
      ], { destructive: true }),
      context,
    );
    expect(result.status).toBe("applied");
    expect(result.moved).toEqual(["1:2"]);
    expect(document.nodes.get("1:3")?.children?.map((child) => child.id)).toEqual(["1:4", "1:2"]);
  });

  it("changes the position of a node", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const result = await handleApplyWrite(
      planPayload([{ kind: "move_node", node_id: "1:2", x: 120, y: 240 }]),
      context,
    );
    expect(result.status).toBe("applied");
    expect(result.moved).toEqual(["1:2"]);
    const label = document.nodes.get("1:2");
    expect(label?.x).toBe(120);
    expect(label?.y).toBe(240);
  });

  it("loads the font before it writes the font size", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const result = await handleApplyWrite(
      planPayload([
        { kind: "set_properties", node_id: "1:2", properties: { fontSize: 22 } },
      ]),
      context,
    );
    expect(result.status).toBe("applied");
    expect(document.fontLoads).toEqual([{ family: "Inter", style: "Regular" }]);
    expect(document.nodes.get("1:2")?.fontSize).toBe(22);
    const fontIndex = document.mutationLog.indexOf("loadFont:Inter:Regular");
    expect(fontIndex).toBeGreaterThanOrEqual(0);
  });

  it("groups the nodes of the plan under a group", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const result = await handleApplyWrite(
      planPayload([
        {
          kind: "create_node",
          parent_id: "1:1",
          node_type: "GROUP",
          children_ids: ["1:4"],
        },
      ]),
      context,
    );
    expect(result.status).toBe("applied");
    expect(result.created).toHaveLength(1);
    const group = document.nodes.get(result.created[0] ?? "");
    expect(group?.type).toBe("GROUP");
    expect(group?.children?.map((child) => child.id)).toEqual(["1:4"]);
  });

  it("writes a version checkpoint when the plan asks for one", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const result = await handleApplyWrite(
      planPayload([{ kind: "set_properties", node_id: "1:2", properties: { x: 1 } }], {
        saveVersion: true,
      }),
      context,
    );
    expect(result.version_requested).toBe(true);
    expect(result.version_available).toBe(true);
    expect(result.version_saved).toBe("version:1");
    expect(document.versionSaves).toHaveLength(1);
    expect(document.versionSaves[0]?.title).toContain("plan_1");
  });

  it("reports the absence of a version checkpoint instead of hiding it", async () => {
    const document = sampleDocument({ versionHistory: false });
    const context = createWriteContext(document, ["1:1"]);
    const result = await handleApplyWrite(
      planPayload([{ kind: "set_properties", node_id: "1:2", properties: { x: 1 } }], {
        saveVersion: true,
      }),
      context,
    );
    expect(result.version_requested).toBe(true);
    expect(result.version_available).toBe(false);
    expect(result.version_saved).toBeUndefined();
    expect(result.version_note).not.toBeNull();
    expect(result.warnings.length).toBeGreaterThan(0);
  });

  it("echoes the approval of the daemon", async () => {    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const result = await handleApplyWrite(
      planPayload([{ kind: "set_properties", node_id: "1:2", properties: { x: 1 } }], {
        approvalId: "approval_1",
      }),
      context,
    );
    expect(result.approval_id).toBe("approval_1");
    expect(result.plan_id).toBe("plan_1");
    expect(result.plan_hash.startsWith("sha256:")).toBe(true);
  });

  it("refuses a plan that names a node outside of the grant", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const error = await handleApplyWrite(
      planPayload([{ kind: "set_properties", node_id: "2:2", properties: { x: 1 } }]),
      context,
    ).catch((reason: unknown) => reason);
    expect((error as { code?: string }).code).toBe("OUT_OF_SCOPE");
    expect(document.mutationLog).toEqual([]);
  });

  it("refuses a destructive plan without the flag", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const error = await handleApplyWrite(
      planPayload([{ kind: "delete_node", node_id: "1:4" }]),
      context,
    ).catch((reason: unknown) => reason);
    expect((error as { code?: string }).code).toBe("DESTRUCTIVE_NOT_ALLOWED");
    expect((error as { detail?: { reason?: string } }).detail?.reason).toBe(
      "destructive_not_allowed",
    );
    expect(document.nodes.get("1:4")).toBeDefined();
  });

  it("refuses a plan with a property outside of the allowlist", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const error = await handleApplyWrite(
      planPayload([
        { kind: "set_properties", node_id: "1:2", properties: { fontName: "Comic Sans" } },
      ]),
      context,
    ).catch((reason: unknown) => reason);
    expect((error as { detail?: { reason?: string } }).detail?.reason).toBe("unknown_property");
    expect(document.mutationLog).toEqual([]);
  });

  it("refuses an apply whose approval token names another plan", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const payload = planPayload([
      { kind: "set_properties", node_id: "1:2", properties: { x: 1 } },
    ]);
    payload["approval_token"] =
      "w:apr_01:0000000000000000000000000000000000000000000000000000000000000000" +
      ":session_1:1:2026-10-20T12:34:56.789+00:00";
    const error = await handleApplyWrite(payload, context).catch((reason: unknown) => reason);
    expect((error as { code?: string }).code).toBe("PLAN_HASH_MISMATCH");
    expect(document.mutationLog).toEqual([]);
  });

  it("takes the approval of the token into the result", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const payload = planPayload([
      { kind: "set_properties", node_id: "1:2", properties: { x: 1 } },
    ]);
    const hash = planHash(parseWritePlan(payload).plan).replace("sha256:", "");
    payload["approval_token"] = `w:apr_01:${hash}:session_1:1:2026-10-20T12:34:56.789+00:00`;
    const result = await handleApplyWrite(payload, context);
    expect(result.status).toBe("applied");
    expect(result.approval_id).toBe("apr_01");
  });
});
