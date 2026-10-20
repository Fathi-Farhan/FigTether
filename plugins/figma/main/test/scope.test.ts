import { describe, expect, it } from "vitest";

import { containsNode, collectAncestorIds, ScopeGuard } from "../src/scope.js";
import { PluginError } from "../src/errors.js";
import { createFakeDocument } from "./fake-figma.js";

describe("scope membership", () => {
  it("accepts a granted root", () => {
    expect(containsNode(["1:2"], "1:2", [])).toBe(true);
  });

  it("accepts a descendant of a granted root", () => {
    expect(containsNode(["1:2"], "1:9", ["1:5", "1:2"])).toBe(true);
  });

  it("refuses a sibling of a granted root", () => {
    expect(containsNode(["1:2"], "1:9", ["1:5"])).toBe(false);
  });

  it("refuses every node when no root is granted", () => {
    expect(containsNode([], "1:2", [])).toBe(false);
    expect(containsNode([], "1:2", ["1:2"])).toBe(false);
  });

  it("collects the ancestors of a node, closest first", () => {
    const document = createFakeDocument([
      {
        id: "1:1",
        children: [{ id: "1:2", children: [{ id: "1:3" }] }],
      },
    ]);
    const node = document.nodes.get("1:3");
    expect(node).toBeDefined();
    expect(collectAncestorIds(node!)).toEqual(["1:2", "1:1", "0:1", "0:0"]);
  });

  it("bounds the ancestor walk", () => {
    const document = createFakeDocument([]);
    const root = document.root;
    let current = root;
    for (let index = 0; index < 100; index += 1) {
      const child = {
        id: `deep:${index}`,
        type: "FRAME",
        name: "deep",
        visible: true,
        locked: false,
        parent: current,
        children: [],
      };
      current.children = [...(current.children ?? []), child];
      current = child;
    }
    expect(collectAncestorIds(current).length).toBeLessThanOrEqual(64);
  });
});

describe("scope guard", () => {
  it("refuses a read without a grant", async () => {
    const document = createFakeDocument([{ id: "1:1" }]);
    const guard = new ScopeGuard(document.api);
    await expect(guard.requireInScope("1:1")).rejects.toThrowError(/no access grant/i);
  });

  it("refuses a node outside the grant with OUT_OF_SCOPE", async () => {
    const document = createFakeDocument([{ id: "1:1" }, { id: "2:1" }]);
    const guard = new ScopeGuard(document.api);
    guard.setGrant({ rootIds: ["1:1"], grantEpoch: 3 });
    const error = await guard.requireInScope("2:1").catch((reason: unknown) => reason);
    expect(error).toBeInstanceOf(PluginError);
    expect((error as PluginError).code).toBe("OUT_OF_SCOPE");
  });

  it("accepts a descendant of the granted root", async () => {
    const document = createFakeDocument([{ id: "1:1", children: [{ id: "1:2" }] }]);
    const guard = new ScopeGuard(document.api);
    guard.setGrant({ rootIds: ["1:1"], grantEpoch: 1 });
    const node = await guard.requireInScope("1:2");
    expect(node.id).toBe("1:2");
  });

  it("reports an unknown node as NODE_NOT_FOUND", async () => {
    const document = createFakeDocument([{ id: "1:1" }]);
    const guard = new ScopeGuard(document.api);
    guard.setGrant({ rootIds: ["1:1"], grantEpoch: 1 });
    const error = await guard.requireInScope("9:9").catch((reason: unknown) => reason);
    expect((error as PluginError).code).toBe("NODE_NOT_FOUND");
  });
});
