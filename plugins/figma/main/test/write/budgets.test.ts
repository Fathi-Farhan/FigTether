import { describe, expect, it } from "vitest";

import { DEFAULT_WRITE_BUDGETS, type WriteBudgets } from "../../src/write/budgets.js";
import { handleApplyWrite, handlePreviewWrite, scopeOf } from "../../src/write/handlers.js";
import { parseWritePlan } from "../../src/write/plan.js";
import { resolvePlan } from "../../src/write/resolver.js";
import { createFakeDocument } from "../fake-figma.js";
import { createWriteContext, planPayload, sampleDocument } from "./write-fixture.js";

/** Resolve a plan with custom budgets and report whether it is blocked. */
async function blockedWith(
  operations: Record<string, unknown>[],
  budgets: WriteBudgets,
): Promise<{ blocked: boolean; reason: string | undefined; code: string | undefined }> {
  const document = sampleDocument();
  const context = createWriteContext(document, ["1:1"]);
  const plan = parseWritePlan(planPayload(operations)).plan;
  const resolved = await resolvePlan(plan, document.api, scopeOf(context), {
    mode: "preview",
    budgets,
  });
  const first = resolved.reasons[0];
  return {
    blocked: resolved.preview.blocked,
    reason: first?.reason,
    code: first?.code,
  };
}

describe("budgets of the write path", () => {
  it("uses the defaults of the design", () => {
    expect(DEFAULT_WRITE_BUDGETS.maxOperations).toBe(50);
    expect(DEFAULT_WRITE_BUDGETS.maxCreatedNodes).toBe(100);
    expect(DEFAULT_WRITE_BUDGETS.maxTextCharacters).toBe(20000);
    expect(DEFAULT_WRITE_BUDGETS.maxPropertyChanges).toBe(200);
    expect(DEFAULT_WRITE_BUDGETS.maxDeleteSubtree).toBe(50);
  });

  it("refuses a plan with too many operations", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const operations = Array.from({ length: 51 }, () => ({
      kind: "set_properties",
      node_id: "1:2",
      properties: { x: 1 },
    }));
    const preview = await handlePreviewWrite(planPayload(operations), context);
    expect(preview.blocked).toBe(true);
    expect(preview.reasons[0]).toContain("operations");
  });

  it("refuses a plan that creates too many nodes", async () => {
    const operations = Array.from({ length: 4 }, () => ({
      kind: "create_node",
      parent_id: "1:1",
      node_type: "FRAME",
    }));
    const outcome = await blockedWith(operations, {
      ...DEFAULT_WRITE_BUDGETS,
      maxOperations: 10,
      maxCreatedNodes: 3,
    });
    expect(outcome.blocked).toBe(true);
    expect(outcome.reason).toBe("budget_exceeded");
  });

  it("refuses a plan that writes too many characters", async () => {
    const outcome = await blockedWith(
      [
        {
          kind: "set_text",
          node_id: "1:2",
          text: "x".repeat(120),
        },
      ],
      { ...DEFAULT_WRITE_BUDGETS, maxTextCharacters: 100 },
    );
    expect(outcome.blocked).toBe(true);
    expect(outcome.reason).toBe("budget_exceeded");
  });

  it("refuses a plan that changes too many properties", async () => {
    const outcome = await blockedWith(
      [
        {
          kind: "set_properties",
          node_id: "1:2",
          properties: { x: 1, y: 2, rotation: 3 },
        },
      ],
      { ...DEFAULT_WRITE_BUDGETS, maxPropertyChanges: 2 },
    );
    expect(outcome.blocked).toBe(true);
    expect(outcome.reason).toBe("budget_exceeded");
  });

  it("refuses a delete that takes more nodes with it than the budget allows", async () => {
    const children = Array.from({ length: 60 }, (_entry, index) => ({
      id: `5:${index + 1}`,
      type: "ELLIPSE",
      name: `Dot ${index + 1}`,
    }));
    const document = createFakeDocument([
      {
        id: "1:1",
        type: "FRAME",
        name: "Root",
        children: [{ id: "1:2", type: "FRAME", name: "Wide", children }],
      },
    ]);
    const context = createWriteContext(document, ["1:1"]);
    const preview = await handlePreviewWrite(
      planPayload([{ kind: "delete_node", node_id: "1:2" }], { destructive: true }),
      context,
    );
    expect(preview.blocked).toBe(true);
    expect(preview.reasons[0]).toContain("nodes");
    expect(document.nodes.get("1:2")).toBeDefined();
  });

  it("applies a plan that stays inside every budget", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const result = await handleApplyWrite(
      planPayload([{ kind: "set_properties", node_id: "1:2", properties: { x: 1 } }]),
      context,
    );
    expect(result.status).toBe("applied");
    expect(result.applied_operations).toBe(1);
  });

  it("refuses the plan before the first mutation", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const operations = [
      { kind: "create_node", parent_id: "1:1", node_type: "FRAME" },
      ...Array.from({ length: 51 }, () => ({
        kind: "set_properties",
        node_id: "1:2",
        properties: { x: 1 },
      })),
    ];
    const error = await handleApplyWrite(planPayload(operations), context).catch(
      (reason: unknown) => reason,
    );
    expect((error as { detail?: { code?: string } }).detail?.code).toBe("BUDGET_EXCEEDED");
    expect(document.mutationLog).toEqual([]);
  });
});
