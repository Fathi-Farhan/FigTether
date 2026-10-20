import { describe, expect, it } from "vitest";

import { DEFAULT_SUMMARY_FIELDS, normalizeFieldList, projectNode } from "../src/projection.js";
import { UNAVAILABLE_REASON, type PluginNode } from "../src/types.js";

const node: PluginNode = {
  id: "1:2",
  parent_id: "1:1",
  node_type: "FRAME",
  name: "Card",
  visible: true,
  locked: false,
  children: ["1:3"],
  geometry: {
    local_transform: [1, 0, 0, 1],
    absolute_transform: [1, 0, 0, 1],
    bounding_box: [0, 0, 100, 40],
    rotation: 0,
    constraints_horizontal: "LEFT",
    constraints_vertical: "TOP",
    coordinate_space: "page",
  },
  layout: {
    auto_layout: true,
    axis: "HORIZONTAL",
    gap: 8,
    padding: [4, 4, 4, 4],
    primary_axis_align: "MIN",
    counter_axis_align: "MIN",
    sizing: "FIXED",
    wraps: false,
  },
  fills: [
    {
      kind: "SOLID",
      color: [1, 1, 1, 1],
      gradient_stops: [],
      opacity: 1,
      visible: true,
    },
  ],
  strokes: [],
  effects: [],
  variables: [],
  styles: [],
  unavailable_fields: [],
};

describe("field projection", () => {
  it("keeps every field when no projection is given", () => {
    const projected = projectNode(node, null);
    expect(projected).toBe(node);
  });

  it("keeps the identity fields even when they are not requested", () => {
    const projected = projectNode(node, ["fills"]);
    expect(projected.id).toBe("1:2");
    expect(projected.node_type).toBe("FRAME");
  });

  it("drops the fields that were not requested", () => {
    const projected = projectNode(node, ["id", "name"]);
    expect(projected.name).toBe("Card");
    expect(projected.geometry).toBeUndefined();
    expect(projected.layout).toBeUndefined();
    expect(projected.fills).toEqual([]);
  });

  it("reports every omitted field with the projection reason", () => {
    const projected = projectNode(node, ["id", "name"]);
    expect(projected.unavailable_fields).toContain(
      `geometry:${UNAVAILABLE_REASON.PROJECTION_OMITTED}`,
    );
    expect(projected.unavailable_fields).toContain(
      `fills:${UNAVAILABLE_REASON.PROJECTION_OMITTED}`,
    );
  });

  it("keeps the unavailable fields the normalizer already reported", () => {
    const withGap: PluginNode = {
      ...node,
      unavailable_fields: ["typography.font_size:capability_unavailable"],
    };
    const projected = projectNode(withGap, ["id"]);
    expect(projected.unavailable_fields).toContain("typography.font_size:capability_unavailable");
  });

  it("maps the aliases of the catalog on the canonical names", () => {
    expect(normalizeFieldList(["type", "parent", "typography"])).toEqual([
      "node_type",
      "parent_id",
      "text",
    ]);
  });

  it("drops unknown field names", () => {
    expect(normalizeFieldList(["not_a_field"])).toBeNull();
  });

  it("treats a missing projection as no projection", () => {
    expect(normalizeFieldList(undefined)).toBeNull();
    expect(normalizeFieldList(null)).toBeNull();
  });

  it("exposes the default summary projection", () => {
    expect(DEFAULT_SUMMARY_FIELDS).toContain("node_type");
    expect(DEFAULT_SUMMARY_FIELDS).not.toContain("geometry");
  });
});
