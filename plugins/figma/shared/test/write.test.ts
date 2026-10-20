/**
 * Tests of the contract of the write path.
 *
 * The rules checked here are the ones the daemon and the plugin both apply
 * to a plan: a plan has operations, an operation names the node it applies
 * to, a create names its parent and its type, and every property is on the
 * allowlist. The canonical encoding is checked as well, because the plan
 * hash of the two sides has to cover the same bytes.
 */
import { describe, expect, it } from "vitest";

import {
  ALLOWED_WRITE_PROPERTIES,
  WRITE_APPROVAL_TTL_MS,
  WRITE_NODE_TYPES,
  canonicalPlanJson,
  isDestructivePlan,
  planOperationCount,
  planTextLength,
  sha256HexOfText,
  validatePlanShape,
} from "../src/index";
import type { WriteOperation, WritePlan } from "../src/index";

/** Build a plan with the given operations. */
function plan(operations: WriteOperation[], overrides: Partial<WritePlan> = {}): WritePlan {
  return {
    plan_id: "plan_1",
    operations,
    destructive: false,
    ...overrides,
  };
}

/** A valid plan the tests mutate to break one rule at a time. */
function validPlan(): WritePlan {
  return plan([
    {
      kind: "create_node",
      parent_id: "1:2",
      node_type: "FRAME",
      properties: { width: 120, height: 40 },
    },
    { kind: "set_text", node_id: "1:3", text: "Hello" },
  ]);
}

describe("the allowlists of the write path", () => {
  it("names the node types a plan may create", () => {
    expect(WRITE_NODE_TYPES).toEqual([
      "FRAME",
      "TEXT",
      "RECTANGLE",
      "COMPONENT",
      "VECTOR",
      "SECTION",
      "GROUP",
      "LINE",
      "ELLIPSE",
    ]);
  });

  it("names the properties a plan may set", () => {
    expect(ALLOWED_WRITE_PROPERTIES).toContain("width");
    expect(ALLOWED_WRITE_PROPERTIES).toContain("layoutMode");
    expect(ALLOWED_WRITE_PROPERTIES).toContain("cornerRadius");
    expect(ALLOWED_WRITE_PROPERTIES).not.toContain("characters");
    expect(ALLOWED_WRITE_PROPERTIES).not.toContain("fills");
    expect(ALLOWED_WRITE_PROPERTIES).not.toContain("effects");
    expect(new Set(ALLOWED_WRITE_PROPERTIES).size).toBe(ALLOWED_WRITE_PROPERTIES.length);
  });

  it("carries the same property list as the Rust allowlist", () => {
    // The Rust list `ALLOWED_WRITE_PROPERTIES` in
    // `crates/model/src/write.rs` is the source of truth. The names, their
    // order and their number are the same on both sides, so a change on one
    // side has to change the other one.
    expect([...ALLOWED_WRITE_PROPERTIES]).toEqual([
      "x",
      "y",
      "width",
      "height",
      "rotation",
      "opacity",
      "visible",
      "locked",
      "name",
      "fontSize",
      "cornerRadius",
      "topLeftRadius",
      "topRightRadius",
      "bottomLeftRadius",
      "bottomRightRadius",
      "layoutMode",
      "primaryAxisAlignItems",
      "counterAxisAlignItems",
      "layoutSizingHorizontal",
      "layoutSizingVertical",
      "itemSpacing",
      "paddingLeft",
      "paddingRight",
      "paddingTop",
      "paddingBottom",
      "clipsContent",
      "blendMode",
      "description",
    ]);
    expect(ALLOWED_WRITE_PROPERTIES.length).toBe(28);
  });

  it("keeps the lifetime of an approval at ten minutes", () => {
    expect(WRITE_APPROVAL_TTL_MS).toBe(600_000);
  });
});

describe("validatePlanShape", () => {
  it("accepts a well formed plan", () => {
    expect(validatePlanShape(validPlan())).toEqual([]);
  });

  it("reports a plan without operations", () => {
    expect(validatePlanShape(plan([]))).toEqual(["the plan has no operations"]);
  });

  it("reports a missing node id where the operation needs one", () => {
    const problems = validatePlanShape(plan([{ kind: "delete_node" }]));
    expect(problems).toEqual(["operation 0: missing node id"]);
  });

  it("reports a missing parent id for a create and for an insert", () => {
    expect(validatePlanShape(plan([{ kind: "create_node", node_type: "FRAME" }]))).toEqual([
      "operation 0: missing parent id",
    ]);
    expect(validatePlanShape(plan([{ kind: "insert_child", node_id: "1:3" }]))).toEqual([
      "operation 0: missing parent id",
    ]);
  });

  it("reports a missing node type for a create", () => {
    expect(validatePlanShape(plan([{ kind: "create_node", parent_id: "1:2" }]))).toEqual([
      "operation 0: missing node type",
    ]);
  });

  it("reports an unknown node type", () => {
    expect(
      validatePlanShape(plan([{ kind: "create_node", parent_id: "1:2", node_type: "SLICE" }])),
    ).toEqual(['operation 0: unknown node type "SLICE"']);
  });

  it("reports an unknown property", () => {
    const problems = validatePlanShape(
      plan([{ kind: "set_properties", node_id: "1:3", properties: { characters: "no" } }]),
    );
    expect(problems).toEqual(['operation 0: unknown property "characters"']);
  });

  it("reports every broken rule of a plan", () => {
    const problems = validatePlanShape(
      plan([
        { kind: "set_properties", properties: { opacity: 0.5, strokes: [] } },
        { kind: "create_node", node_type: "GROUP" },
      ]),
    );
    expect(problems).toEqual([
      "operation 0: missing node id",
      'operation 0: unknown property "strokes"',
      "operation 1: missing parent id",
    ]);
  });
});

describe("isDestructivePlan", () => {
  it("is false for a plan without a destructive operation", () => {
    expect(isDestructivePlan(validPlan())).toBe(false);
  });

  it("is true when the plan sets the flag", () => {
    expect(isDestructivePlan(plan([], { destructive: true }))).toBe(true);
  });

  it("is true when the plan carries a delete, an insert or a move", () => {
    expect(isDestructivePlan(plan([{ kind: "delete_node", node_id: "1:3" }]))).toBe(true);
    expect(isDestructivePlan(plan([{ kind: "insert_child", node_id: "1:3" }]))).toBe(true);
    expect(isDestructivePlan(plan([{ kind: "move_node", node_id: "1:3" }]))).toBe(true);
  });

  it("is false for the operations that only change the content", () => {
    expect(
      isDestructivePlan(
        plan([
          { kind: "set_properties", node_id: "1:3" },
          { kind: "set_text", node_id: "1:4" },
          { kind: "set_fills", node_id: "1:5" },
        ]),
      ),
    ).toBe(false);
  });
});

describe("the counters of a plan", () => {
  it("counts the operations", () => {
    expect(planOperationCount(validPlan())).toBe(2);
    expect(planOperationCount(plan([]))).toBe(0);
  });

  it("counts the characters a plan writes", () => {
    expect(planTextLength(validPlan())).toBe(5);
    expect(planTextLength(plan([{ kind: "set_text", node_id: "1:3", text: "ab" }]))).toBe(2);
    expect(planTextLength(plan([{ kind: "set_text", node_id: "1:3" }]))).toBe(0);
  });
});

describe("canonicalPlanJson", () => {
  it("encodes the same plan with the same bytes", () => {
    expect(canonicalPlanJson(validPlan())).toBe(canonicalPlanJson(validPlan()));
  });

  it("encodes a changed plan with other bytes", () => {
    const changed = validPlan();
    changed.operations[1] = { kind: "set_text", node_id: "1:3", text: "Hello!" };
    expect(canonicalPlanJson(changed)).not.toBe(canonicalPlanJson(validPlan()));
  });

  it("sorts the keys and leaves an absent optional field absent", () => {
    const encoded = canonicalPlanJson(validPlan());
    expect(encoded).toBe(
      '{"destructive":false,"operations":[{"kind":"create_node","node_type":"FRAME",' +
        '"parent_id":"1:2","properties":{"height":40,"width":120}},' +
        '{"kind":"set_text","node_id":"1:3","text":"Hello"}],"plan_id":"plan_1"}',
    );
    expect(encoded).not.toContain("session_id");
    expect(encoded).not.toContain("intent");
  });

  it("encodes the optional fields of a plan when they are present", () => {
    const encoded = canonicalPlanJson(
      plan([{ kind: "move_node", node_id: "1:3", x: 10, y: 20, index: 0 }], {
        session_id: "sess_1",
        intent: "move the label",
        destructive: true,
      }),
    );
    expect(encoded).toBe(
      '{"destructive":true,"intent":"move the label","operations":[{"index":0,"kind":"move_node",' +
        '"node_id":"1:3","x":10.0,"y":20.0}],"plan_id":"plan_1","session_id":"sess_1"}',
    );
  });

  it("writes the position of a node the way the Rust serializer writes it", () => {
    // The position is a float in the Rust contract, so the canonical form
    // carries a decimal point. The hash of the two sides only matches when
    // both sides write the number the same way.
    const encoded = canonicalPlanJson(plan([{ kind: "move_node", node_id: "1:3", x: 12, y: 0.5 }]));
    expect(encoded).toContain('"x":12.0');
    expect(encoded).toContain('"y":0.5');
  });

  it("writes a very small position the way the Rust serializer writes it", () => {
    const encoded = canonicalPlanJson(plan([{ kind: "move_node", node_id: "1:3", x: 1e-7 }]));
    expect(encoded).toContain('"x":1e-7');
  });

  it("leaves an empty property map out", () => {
    const encoded = canonicalPlanJson(plan([{ kind: "set_properties", node_id: "1:3" }]));
    expect(encoded).not.toContain('"properties":');
  });

  it("covers the paints of a plan with the hash", () => {
    const first = canonicalPlanJson(
      plan([{ kind: "set_fills", node_id: "1:3", fills: [{ type: "SOLID", color: { r: 1 } }] }]),
    );
    const second = canonicalPlanJson(
      plan([{ kind: "set_fills", node_id: "1:3", fills: [{ type: "SOLID", color: { r: 0 } }] }]),
    );
    expect(first).toContain('"fills":[{"color":{"r":1},"type":"SOLID"}]');
    expect(first).not.toBe(second);
  });

  it("writes the same bytes as the Rust serializer of the plan", () => {
    // The bytes and the hash below are the ones the Rust side produces for
    // the same plan: `canonical_plan_hash` in `crates/model/src/write.rs`
    // hashes the compact JSON of the typed plan, with every key in
    // lexicographic order, the position of a node written as a float and an
    // empty property map left out. The two sides only agree when the
    // encoding follows the same rules, so the values are pinned here.
    const encoded = canonicalPlanJson(
      plan(
        [
          {
            kind: "create_node",
            parent_id: "1:2",
            node_type: "FRAME",
            properties: { width: 120, x: 12 },
            x: 12,
            y: 0.5,
          },
          {
            kind: "set_fills",
            node_id: "1:3",
            fills: [{ type: "SOLID", color: { r: 1, g: 0, b: 0 } }],
            index: 0,
          },
        ],
        { intent: "make it blue" },
      ),
    );
    expect(encoded).toBe(
      '{"destructive":false,"intent":"make it blue","operations":[{"kind":"create_node",' +
        '"node_type":"FRAME","parent_id":"1:2","properties":{"width":120,"x":12},"x":12.0,' +
        '"y":0.5},{"fills":[{"color":{"b":0,"g":0,"r":1},"type":"SOLID"}],"index":0,' +
        '"kind":"set_fills","node_id":"1:3"}],"plan_id":"plan_1"}',
    );
    expect(`sha256:${sha256HexOfText(encoded)}`).toBe(
      "sha256:4f44e6283bd85fb77d1cd6d9cdddcc2e265af5f9be27432608c6a5a42c9524a1",
    );
  });
});
