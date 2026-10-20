import { describe, expect, it } from "vitest";

import { mapEditorError } from "../../src/write/errors.js";
import { handleApplyWrite, handlePreviewWrite } from "../../src/write/handlers.js";
import { createWriteContext, planPayload, sampleDocument } from "./write-fixture.js";

describe("refusals of the write path", () => {
  it("refuses a move that would create a cycle", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const preview = await handlePreviewWrite(
      planPayload(
        [{ kind: "insert_child", parent_id: "1:4", node_id: "1:3" }],
        { destructive: true },
      ),
      context,
    );
    expect(preview.blocked).toBe(true);
    expect(preview.reasons[0]).toContain("cycle");
  });

  it("refuses the move of a read only node", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const preview = await handlePreviewWrite(
      planPayload(
        [{ kind: "insert_child", parent_id: "1:3", node_id: "1:5" }],
        { destructive: true },
      ),
      context,
    );
    expect(preview.blocked).toBe(true);
    expect(preview.reasons[0]).toContain("read only");
  });

  it("refuses the move of a node that lives inside an instance", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const preview = await handlePreviewWrite(
      planPayload(
        [{ kind: "insert_child", parent_id: "1:3", node_id: "1:7" }],
        { destructive: true },
      ),
      context,
    );
    expect(preview.blocked).toBe(true);
    expect(preview.reasons[0]).toContain("instance");
  });

  it("refuses a move that would put a component inside itself", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const preview = await handlePreviewWrite(
      planPayload(
        [{ kind: "insert_child", parent_id: "1:10", node_id: "1:9" }],
        { destructive: true },
      ),
      context,
    );
    expect(preview.blocked).toBe(true);
    expect(preview.reasons[0]).toContain("component");
  });

  it("maps a refusal of the editor to a structured error", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const result = await handleApplyWrite(
      planPayload(
        [{ kind: "insert_child", parent_id: "1:3", node_id: "1:2", index: 9 }],
        { destructive: true },
      ),
      context,
    );
    expect(result.status).toBe("partial");
    expect(result.error?.code).toBe("MUTATION_REFUSED");
    expect(result.error?.reason).toBe("index_out_of_range");
  });

  it("reads the reason of a refusal of the editor out of its message", () => {
    const cases: Array<[string, string]> = [
      ["Cannot move a node into its own descendant", "parenting_cycle"],
      ["The node is read-only", "readonly_node"],
      ["Cannot move a child of an instance", "instance_child"],
      ["Cannot move a component into itself", "component_cycle"],
      ["insertChild: the index is outside the parent", "index_out_of_range"],
      ["something else entirely", "editor_refused"],
    ];
    for (const [message, reason] of cases) {
      const detail = mapEditorError(new Error(message), 3);
      expect(detail.reason).toBe(reason);
      expect(detail.operation).toBe(3);
    }
  });

  it("refuses a delete that would take a granted root with it", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const preview = await handlePreviewWrite(
      planPayload([{ kind: "delete_node", node_id: "1:1" }], { destructive: true }),
      context,
    );
    expect(preview.blocked).toBe(true);
    expect(preview.reasons[0]).toContain("granted root");
  });

  it("refuses a parent that cannot hold children", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const preview = await handlePreviewWrite(
      planPayload([{ kind: "create_node", parent_id: "1:2", node_type: "FRAME" }]),
      context,
    );
    expect(preview.blocked).toBe(true);
    expect(preview.reasons[0]).toContain("cannot hold children");
  });

  it("refuses a group without a node to take over", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const preview = await handlePreviewWrite(
      planPayload([{ kind: "create_node", parent_id: "1:1", node_type: "GROUP" }]),
      context,
    );
    expect(preview.blocked).toBe(true);
    expect(preview.reasons[0]).toContain("group");
  });

  it("refuses a plan that targets another session", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const payload = planPayload([
      { kind: "set_properties", node_id: "1:2", properties: { x: 1 } },
    ]);
    const plan = payload["plan"] as Record<string, unknown>;
    plan["session_id"] = "session_2";
    const error = await handleApplyWrite(payload, context).catch((reason: unknown) => reason);
    expect((error as { detail?: { reason?: string } }).detail?.reason).toBe("invalid_plan");
    expect(document.mutationLog).toEqual([]);
  });

  it("refuses a plan without an id", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const error = await handlePreviewWrite({ plan: { operations: [] } }, context).catch(
      (reason: unknown) => reason,
    );
    expect((error as { code?: string }).code).toBe("PLAN_INVALID");
  });

  it("applies a plan that names no session under the session that asked for it", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const payload = planPayload([
      { kind: "set_properties", node_id: "1:2", properties: { x: 1 } },
    ]);
    const plan = payload["plan"] as Record<string, unknown>;
    delete plan["session_id"];
    const result = await handleApplyWrite(payload, context);
    expect(result.status).toBe("applied");
  });
});
