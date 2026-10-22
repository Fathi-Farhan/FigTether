import { ALLOWED_WRITE_PROPERTIES, canonicalPlanJson, sha256HexOfText } from "@figtether/shared";
import { describe, expect, it } from "vitest";

import {
  checkPropertyValue,
  findProperty,
  normalizeFills,
  WRITABLE_PROPERTIES,
} from "../../src/write/allowlist.js";
import { handlePreviewWrite } from "../../src/write/handlers.js";
import { parseWritePlan, planHash } from "../../src/write/plan.js";
import { createWriteContext, planPayload, sampleDocument } from "./write-fixture.js";

describe("the plan of the write path", () => {
  it("parses a plan into the closed shape of the contract", () => {
    const parsed = parseWritePlan(
      planPayload([
        {
          kind: "create_node",
          parent_id: "1:1",
          node_type: "frame",
          properties: { name: "New" },
          x: 4,
        },
      ]),
    );
    const plan = parsed.plan;
    expect(plan.plan_id).toBe("plan_1");
    expect(plan.session_id).toBe("session_1");
    expect(plan.destructive).toBe(false);
    expect(plan.operations).toHaveLength(1);
    const operation = plan.operations[0];
    expect(operation?.kind).toBe("create_node");
    if (operation?.kind === "create_node") {
      expect(operation.node_type).toBe("FRAME");
      expect(operation.parent_id).toBe("1:1");
      expect(operation.properties["name"]).toBe("New");
      expect(operation.x).toBe(4);
    }
  });

  it("reads the approval and the hash that travel with the plan", () => {
    const parsed = parseWritePlan({
      plan: {
        plan_id: "plan_1",
        destructive: false,
        operations: [{ kind: "set_text", node_id: "1:2", text: "Hello" }],
      },
      approval_id: "apr_01",
      plan_hash: "sha256:0123456789abcdef",
    });
    expect(parsed.approvalId).toBe("apr_01");
    expect(parsed.planHash).toBe("sha256:0123456789abcdef");
  });

  it("reads the paints of a fill operation from the dedicated field", () => {
    const parsed = parseWritePlan(
      planPayload([
        {
          kind: "set_fills",
          node_id: "1:2",
          fills: [{ type: "SOLID", color: { r: 1, g: 0, b: 0 } }],
        },
      ]),
    );
    const operation = parsed.plan.operations[0];
    expect(operation?.kind).toBe("set_fills");
    if (operation?.kind === "set_fills") {
      expect(operation.fills).toHaveLength(1);
    }
  });

  it("refuses a fill operation that carries no paints", () => {
    expect(() => parseWritePlan(planPayload([{ kind: "set_fills", node_id: "1:2" }]))).toThrow();
  });

  it("refuses an operation of an unknown kind", () => {
    const error = (() => {
      try {
        parseWritePlan(planPayload([{ kind: "drop_database", node_id: "1:1" }]));
        return null;
      } catch (reason) {
        return reason as { code?: string; detail?: { reason?: string } };
      }
    })();
    expect(error?.code).toBe("PLAN_INVALID");
    expect(error?.detail?.reason).toBe("invalid_plan");
  });

  it("refuses a node type the plugin cannot create", () => {
    expect(() =>
      parseWritePlan(planPayload([{ kind: "create_node", parent_id: "1:1", node_type: "STAR" }])),
    ).toThrow();
  });

  it("hashes the same plan the same way and a different plan differently", () => {
    const first = parseWritePlan(
      planPayload([{ kind: "set_properties", node_id: "1:2", properties: { x: 1, y: 2 } }]),
    ).plan;
    const second = parseWritePlan(
      planPayload([{ kind: "set_properties", node_id: "1:2", properties: { y: 2, x: 1 } }]),
    ).plan;
    const third = parseWritePlan(
      planPayload([{ kind: "set_properties", node_id: "1:2", properties: { x: 2, y: 2 } }]),
    ).plan;
    expect(planHash(first)).toBe(planHash(second));
    expect(planHash(first)).not.toBe(planHash(third));
    expect(planHash(first).startsWith("sha256:")).toBe(true);
  });

  it("covers the paints of a plan with the hash", () => {
    const first = parseWritePlan(
      planPayload([
        {
          kind: "set_fills",
          node_id: "1:2",
          fills: [{ type: "SOLID", color: { r: 1, g: 0, b: 0 } }],
        },
      ]),
    ).plan;
    const second = parseWritePlan(
      planPayload([
        {
          kind: "set_fills",
          node_id: "1:2",
          fills: [{ type: "SOLID", color: { r: 0, g: 0, b: 1 } }],
        },
      ]),
    ).plan;
    expect(planHash(first)).not.toBe(planHash(second));
  });

  it("normalizes the fills of a plan before they reach the editor", () => {
    const paints = normalizeFills([
      { type: "SOLID", color: "#3366ff", opacity: 2 },
      { type: "SOLID", color: { r: 1, g: 0.5, b: 0 } },
    ]);
    expect(paints).toEqual([
      { type: "SOLID", color: { r: 0.2, g: 0.4, b: 1 }, opacity: 1, visible: true },
      { type: "SOLID", color: { r: 1, g: 0.5, b: 0 }, opacity: 1, visible: true },
    ]);
  });

  it("maps solid alpha to native paint opacity instead of an unsupported RGB alpha", () => {
    const paint = normalizeFills([
      { type: "SOLID", color: { r: 1, g: 0, b: 0, a: 0.5 }, opacity: 0.6 },
    ])[0];
    expect(paint?.color).toEqual({ r: 1, g: 0, b: 0 });
    expect(paint?.opacity).toBeCloseTo(0.3);
  });

  it.each(["GRADIENT_LINEAR", "GRADIENT_RADIAL", "GRADIENT_ANGULAR", "GRADIENT_DIAMOND"])(
    "supplies a native transform for %s",
    (type) => {
      const paint = normalizeFills([
        {
          type,
          stops: [
            { position: 0, color: "#000000" },
            { position: 1, color: "#ffffff" },
          ],
        },
      ])[0];
      expect(paint?.type).toBe(type);
      expect(paint?.gradientTransform).toEqual([
        [1, 0, 0],
        [0, 1, 0],
      ]);
    },
  );

  it("refuses malformed gradient transforms before touching Figma", () => {
    expect(() =>
      normalizeFills([
        {
          type: "GRADIENT_LINEAR",
          stops: [{ position: 0, color: "#000000" }],
          gradientTransform: [[1, 0]],
        },
      ]),
    ).toThrow();
  });

  it.each(["layoutSizingHorizontal", "layoutSizingVertical"])(
    "uses Figma's FIXED/HUG/FILL values for %s",
    (property) => {
      const spec = findProperty(property)!;
      for (const value of ["FIXED", "HUG", "FILL"])
        expect(() => checkPropertyValue(spec, value)).not.toThrow();
      expect(() => checkPropertyValue(spec, "AUTO")).toThrow();
    },
  );

  it("normalizes the stops of a gradient", () => {
    const paints = normalizeFills([
      {
        type: "GRADIENT_LINEAR",
        stops: [
          { position: 1, color: "#ffffff" },
          { position: 0, color: "#000000" },
        ],
      },
    ]);
    expect(paints).toHaveLength(1);
    const paint = paints[0];
    expect(paint?.type).toBe("GRADIENT_LINEAR");
    expect(paint?.gradientStops?.[0]?.position).toBe(0);
    expect(paint?.gradientStops?.[1]?.position).toBe(1);
  });

  it("refuses a paint type the plugin does not know", () => {
    expect(() => normalizeFills([{ type: "IMAGE", imageHash: "abc" }])).toThrow();
  });

  it("refuses a colour that is not a colour", () => {
    expect(() => normalizeFills([{ type: "SOLID", color: "not a colour" }])).toThrow();
  });

  it("refuses a property value of the wrong kind", async () => {
    const document = sampleDocument();
    const context = createWriteContext(document, ["1:1"]);
    const preview = await handlePreviewWrite(
      planPayload([{ kind: "set_properties", node_id: "1:2", properties: { x: "four" } }]),
      context,
    );
    expect(preview.blocked).toBe(true);
    expect(preview.reasons[0]).toContain("finite number");
    expect(document.mutationLog).toEqual([]);
  });

  it("writes every property the shared contract allows", () => {
    for (const name of ALLOWED_WRITE_PROPERTIES) {
      expect(findProperty(name)).toBeDefined();
    }
  });

  it("names the same properties as the shared contract", () => {
    expect(WRITABLE_PROPERTIES.map((spec) => spec.name)).toEqual([...ALLOWED_WRITE_PROPERTIES]);
  });

  it("hashes the plan the way the shared contract does", () => {
    const plan = parseWritePlan(
      planPayload([{ kind: "set_properties", node_id: "1:2", properties: { x: 1 } }]),
    ).plan;
    const expected = `sha256:${sha256HexOfText(canonicalPlanJson(plan))}`;
    expect(planHash(plan)).toBe(expected);
  });
});
