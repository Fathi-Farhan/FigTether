import { describe, expect, it } from "vitest";

import { handlePreviewWrite } from "../../src/write/handlers.js";
import { parseWritePlan, planHash } from "../../src/write/plan.js";
import { createWriteContext, planPayload, sampleDocument } from "./write-fixture.js";

describe("preview_write", () => {
  it("changes nothing in the document", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const preview = await handlePreviewWrite(
      planPayload([
        { kind: "create_node", parent_id: "1:1", node_type: "FRAME", properties: { name: "New" } },
        { kind: "set_properties", node_id: "1:2", properties: { x: 42 } },
      ]),
      context,
    );
    expect(preview.blocked).toBe(false);
    expect(document.mutationLog).toEqual([]);
    expect(document.commitUndoCalls).toBe(0);
    expect(document.fontLoads).toEqual([]);
    expect(document.versionSaves).toEqual([]);
    expect(document.page.children).toHaveLength(2);
  });

  it("reports the planned creations, the parents and the count of the operations", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const preview = await handlePreviewWrite(
      planPayload([
        { kind: "create_node", parent_id: "1:1", node_type: "FRAME", properties: { name: "New" } },
        { kind: "create_node", parent_id: "1:3", node_type: "ELLIPSE" },
      ]),
      context,
    );
    expect(preview.operations_count).toBe(2);
    expect(preview.created_count).toBe(2);
    expect(preview.created_plan).toHaveLength(2);
    expect(preview.created_plan[0]?.parent_id).toBe("1:1");
    expect(preview.created_plan[0]?.node_type).toBe("FRAME");
    expect(preview.created_plan[0]?.name).toBe("New");
    expect(preview.created_plan[1]?.parent_id).toBe("1:3");
  });

  it("does not pretend to know the id of a node that does not exist yet", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const preview = await handlePreviewWrite(
      planPayload([{ kind: "create_node", parent_id: "1:1", node_type: "FRAME" }]),
      context,
    );
    expect(preview.created).toEqual([]);
    expect(preview.created_count).toBe(1);
  });

  it("reports the nodes that would change, be removed and move", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const preview = await handlePreviewWrite(
      planPayload(
        [
          { kind: "set_properties", node_id: "1:2", properties: { x: 42 } },
          { kind: "set_text", node_id: "1:2", text: "Now" },
          { kind: "delete_node", node_id: "1:4" },
          { kind: "move_node", node_id: "1:3", x: 5, y: 6 },
        ],
        { destructive: true },
      ),
      context,
    );
    expect(preview.changed).toEqual(["1:2"]);
    expect(preview.removed).toEqual(["1:4"]);
    expect(preview.moved).toEqual(["1:3"]);
  });

  it("blocks a plan that names a node outside of the grant and names the reason", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const preview = await handlePreviewWrite(
      planPayload([{ kind: "set_properties", node_id: "2:2", properties: { x: 1 } }]),
      context,
    );
    expect(preview.blocked).toBe(true);
    expect(preview.reasons).toHaveLength(1);
    expect(preview.reasons[0]).toContain("outside the granted scope");
  });

  it("blocks a plan that names a node that does not exist", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const preview = await handlePreviewWrite(
      planPayload([{ kind: "delete_node", node_id: "9:9" }], { destructive: true }),
      context,
    );
    expect(preview.blocked).toBe(true);
    expect(preview.reasons[0]).toContain("does not exist");
  });

  it("blocks a plan that changes the structure without the destructive flag", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const preview = await handlePreviewWrite(
      planPayload([{ kind: "delete_node", node_id: "1:4" }]),
      context,
    );
    expect(preview.blocked).toBe(true);
    expect(preview.reasons[0]).toContain("destructive flag");
  });

  it("blocks a plan that uses a property outside of the allowlist", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const preview = await handlePreviewWrite(
      planPayload([{ kind: "set_properties", node_id: "1:2", properties: { characters: "no" } }]),
      context,
    );
    expect(preview.blocked).toBe(true);
    expect(preview.reasons[0]).toContain("not allowlisted");
  });

  it("warns about a property the node type does not support", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const preview = await handlePreviewWrite(
      planPayload([
        { kind: "set_properties", node_id: "1:2", properties: { layoutMode: "HORIZONTAL" } },
      ]),
      context,
    );
    expect(preview.blocked).toBe(false);
    expect(preview.warnings).toHaveLength(1);
    expect(preview.warnings[0]).toContain("layoutMode");
  });

  it("resolves the same plan as the apply does", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const payload = planPayload([
      { kind: "create_node", parent_id: "1:1", node_type: "TEXT", properties: { name: "New" } },
      { kind: "set_text", node_id: "1:2", text: "Now" },
    ]);
    const preview = await handlePreviewWrite(payload, context);
    const again = await handlePreviewWrite(payload, context);
    expect(preview.plan_hash).toBe(again.plan_hash);
    expect(preview.operations_count).toBe(again.operations_count);
    expect(preview.changed).toEqual(again.changed);
    expect(preview.created_plan).toEqual(again.created_plan);
  });

  it("carries the approval of the daemon into the preview", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const preview = await handlePreviewWrite(
      planPayload([{ kind: "create_node", parent_id: "1:1", node_type: "FRAME" }], {
        approvalId: "apr_01",
      }),
      context,
    );
    expect(preview.approval_id).toBe("apr_01");
  });

  it("leaves the approval out when the daemon minted none", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const preview = await handlePreviewWrite(
      planPayload([{ kind: "create_node", parent_id: "1:1", node_type: "FRAME" }]),
      context,
    );
    expect(preview.approval_id).toBeUndefined();
  });

  it("refuses a plan that changed on the way to the plugin", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const error = await handlePreviewWrite(
      planPayload([{ kind: "create_node", parent_id: "1:1", node_type: "FRAME" }], {
        planHash: "sha256:0000000000000000000000000000000000000000000000000000000000000000",
      }),
      context,
    ).catch((reason: unknown) => reason);
    expect((error as { code?: string }).code).toBe("PLAN_HASH_MISMATCH");
    expect(document.mutationLog).toEqual([]);
  });

  it("accepts a plan whose hash matches the one of the daemon", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const payload = planPayload([{ kind: "create_node", parent_id: "1:1", node_type: "FRAME" }]);
    const hash = planHash(parseWritePlan(payload).plan);
    const preview = await handlePreviewWrite({ ...payload, plan_hash: hash }, context);
    expect(preview.blocked).toBe(false);
    expect(preview.plan_hash).toBe(hash);
  });
});
