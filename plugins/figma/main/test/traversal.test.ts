import { describe, expect, it } from "vitest";

import { encodeCursor } from "../src/cursor.js";
import { ScopeGuard } from "../src/scope.js";
import { traverse } from "../src/traversal.js";
import { createFakeDocument } from "./fake-figma.js";

function guardFor(document: ReturnType<typeof createFakeDocument>): ScopeGuard {
  const guard = new ScopeGuard(document.api);
  guard.setGrant({ rootIds: ["1:1"], grantEpoch: 1 });
  return guard;
}

function wideDocument(children: number): ReturnType<typeof createFakeDocument> {
  return createFakeDocument([
    {
      id: "1:1",
      children: Array.from({ length: children }, (_value, index) => ({
        id: `1:${index + 2}`,
      })),
    },
  ]);
}

describe("sliced traversal", () => {
  it("walks the subtree of the granted root", async () => {
    const document = wideDocument(3);
    const result = await traverse({
      guard: guardFor(document),
      rootIds: ["1:1"],
    });
    expect(result.entries.map((entry) => entry.node.id)).toEqual(["1:1", "1:2", "1:3", "1:4"]);
    expect(result.truncated).toBe(false);
    expect(result.cancelled).toBe(false);
  });

  it("stops at the node budget and reports the truncation", async () => {
    const document = wideDocument(5);
    const result = await traverse({
      guard: guardFor(document),
      rootIds: ["1:1"],
      maxNodes: 2,
    });
    expect(result.visited).toBe(2);
    expect(result.truncated).toBe(true);
    expect(result.stack.length).toBeGreaterThan(0);
  });

  it("resumes a truncated traversal with the cursor", async () => {
    const document = wideDocument(3);
    const guard = guardFor(document);
    const first = await traverse({
      guard,
      rootIds: ["1:1"],
      maxNodes: 2,
    });
    const token = encodeCursor({
      v: 1,
      grant_epoch: 1,
      root_id: "1:1",
      stack: first.stack,
      visited: first.visited,
    });
    expect(token.length).toBeGreaterThan(0);

    const second = await traverse({
      guard,
      rootIds: ["1:1"],
      startStack: first.stack,
      visitedBefore: first.visited,
    });
    expect(second.entries.map((entry) => entry.node.id)).toEqual(["1:3", "1:4"]);
    expect(second.visited).toBe(4);
    expect(second.truncated).toBe(false);
  });

  it("yields to the editor between the slices", async () => {
    const document = wideDocument(40);
    let clock = 0;
    let yields = 0;
    const result = await traverse({
      guard: guardFor(document),
      rootIds: ["1:1"],
      sliceBudgetMs: 1,
      clock: () => {
        clock += 5;
        return clock;
      },
      yieldFn: async () => {
        yields += 1;
      },
    });
    expect(yields).toBeGreaterThan(0);
    expect(result.entries.length).toBe(41);
  });

  it("stops when the request was cancelled", async () => {
    const document = wideDocument(40);
    const result = await traverse({
      guard: guardFor(document),
      rootIds: ["1:1"],
      isCancelled: () => true,
    });
    expect(result.cancelled).toBe(true);
    expect(result.entries).toHaveLength(0);
  });

  it("respects the depth limit", async () => {
    const document = createFakeDocument([
      { id: "1:1", children: [{ id: "1:2", children: [{ id: "1:3" }] }] },
    ]);
    const result = await traverse({
      guard: guardFor(document),
      rootIds: ["1:1"],
      maxDepth: 2,
    });
    expect(result.entries.map((entry) => entry.node.id)).toEqual(["1:1", "1:2"]);
  });

  it("never leaves the granted subtree", async () => {
    const document = createFakeDocument([
      { id: "1:1", children: [{ id: "1:2" }] },
      { id: "2:1", children: [{ id: "2:2" }] },
    ]);
    const guard = new ScopeGuard(document.api);
    guard.setGrant({ rootIds: ["1:1"], grantEpoch: 1 });
    const result = await traverse({ guard, rootIds: ["1:1"] });
    expect(result.entries.map((entry) => entry.node.id)).toEqual(["1:1", "1:2"]);
  });
});
