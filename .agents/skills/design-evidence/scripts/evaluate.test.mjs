import { test } from "node:test";
import assert from "node:assert/strict";
import { evaluate, contrast, rgb, stable } from "./evaluate.mjs";
const base = (nodes = [], more = {}) => ({
  schema_version: 1,
  source: { kind: "dom" },
  capture: { complete: true },
  nodes,
  ...more,
});
const text = (more = {}) => ({
  id: "t",
  role: "text",
  visible: true,
  text: "Example",
  font_size: 16,
  font_weight: 400,
  foreground: "#111111",
  background: "#ffffff",
  ...more,
});
const control = (id, x, y, w, h, more = {}) => ({
  id,
  role: "control",
  visible: true,
  disabled: false,
  rect: { x, y, width: w, height: h },
  name: "Action",
  name_source: "browser-ax",
  ...more,
});
const outcome = (r, id, node) =>
  r.findings.find((f) => f.rule === id && (node === undefined || f.node === node))?.outcome;
const profile = (more) => ({ schema_version: 1, id: "test", ...more });
test("black and white contrast is 21, opaque rgba is accepted", () => {
  assert.equal(contrast("#000", "#fff"), 21);
  assert.deepEqual(rgb("rgba(0, 0, 0, 1)"), [0, 0, 0]);
});
test("transparent and unsupported colors are unknown", () => {
  assert.equal(contrast("rgba(0,0,0,.5)", "#fff"), null);
  assert.equal(rgb("color(display-p3 0 0 0)"), null);
  assert.equal(rgb("rgb(999,0,0)"), null);
});
test("ordinary text uses 4.5 without rounding a near miss", () => {
  const r = evaluate(base([text({ foreground: "#777" })]));
  assert.equal(outcome(r, "A001"), "failed");
  assert.ok(r.findings.find((f) => f.rule === "A001").observed.ratio < 4.5);
});
test("24 CSS px ordinary text qualifies as large", () => {
  assert.equal(
    outcome(evaluate(base([text({ foreground: "#777", font_size: 24 })])), "A001"),
    "passed",
  );
});
test("14 CSS px bold is not 14 points", () => {
  assert.equal(
    outcome(
      evaluate(base([text({ foreground: "#777", font_size: 14, font_weight: 700 })])),
      "A001",
    ),
    "failed",
  );
});
test("14 points bold qualifies at its exact CSS conversion", () => {
  assert.equal(
    outcome(
      evaluate(base([text({ foreground: "#777", font_size: (14 * 96) / 72, font_weight: 700 })])),
      "A001",
    ),
    "passed",
  );
});
test("below the large bold threshold remains normal text", () => {
  assert.equal(
    outcome(
      evaluate(base([text({ foreground: "#777", font_size: 18.66, font_weight: 700 })])),
      "A001",
    ),
    "failed",
  );
});
test("unresolved background abstains", () =>
  assert.equal(outcome(evaluate(base([text({ background: null })])), "A001"), "cantTell"));
test("inactive text and hidden nodes are excluded", () => {
  assert.equal(outcome(evaluate(base([text({ disabled: true })])), "A001"), "inapplicable");
  assert.equal(outcome(evaluate(base([text({ visible: false })])), "A001"), "inapplicable");
});
test("heuristic names do not pass as computed names", () =>
  assert.equal(
    outcome(evaluate(base([control("b", 0, 0, 40, 30, { name_source: "heuristic" })])), "A002"),
    "cantTell",
  ));
test("empty computed names fail", () =>
  assert.equal(
    outcome(evaluate(base([control("b", 0, 0, 40, 30, { name: "" })])), "A002"),
    "failed",
  ));
test("24 by 24 passes target size", () =>
  assert.equal(outcome(evaluate(base([control("b", 0, 0, 24, 24)])), "A003"), "passed"));
test("isolated undersized control can pass through spacing", () =>
  assert.equal(outcome(evaluate(base([control("b", 0, 0, 16, 16)])), "A003"), "passed"));
test("neighboring undersized target spacing fails", () => {
  const r = evaluate(base([control("a", 0, 0, 16, 16), control("b", 20, 0, 16, 16)]));
  assert.equal(outcome(r, "A003", "a"), "failed");
  assert.equal(outcome(r, "A003", "b"), "failed");
});
test("spacing at 24 pixel center distance passes", () =>
  assert.equal(
    outcome(evaluate(base([control("a", 0, 0, 16, 16), control("b", 24, 0, 16, 16)])), "A003", "a"),
    "passed",
  ));
test("small-circle intersection with a full-sized target fails", () =>
  assert.equal(
    outcome(evaluate(base([control("a", 0, 0, 16, 16), control("b", 18, 0, 30, 30)])), "A003", "a"),
    "failed",
  ));
test("partial capture cannot establish isolated-target spacing", () =>
  assert.equal(
    outcome(evaluate(base([control("a", 0, 0, 16, 16)], { capture: { complete: false } })), "A003"),
    "cantTell",
  ));
test("explicit target exception needs evidence and is distinct from a pass", () => {
  const a = control("a", 0, 0, 16, 16, {
    target_exception: { kind: "inline", evidence: "Reviewer confirmed link within sentence." },
  });
  assert.equal(outcome(evaluate(base([a])), "A003"), "inapplicable");
  assert.throws(() =>
    evaluate(base([{ ...a, target_exception: { kind: "inline", evidence: "" } }])),
  );
});
test("Figma layer names do not establish accessibility names", () =>
  assert.equal(
    outcome(
      evaluate(
        base([control("a", 0, 0, 30, 30, { name_source: "figma-layer" })], {
          source: { kind: "figma" },
        }),
      ),
      "A002",
    ),
    "cantTell",
  ));
test("essential cue contrast and decorative exceptions", () => {
  const g = {
    id: "g",
    role: "graphic",
    visible: true,
    contrast_role: "essential",
    foreground: "#fff",
    adjacent_color: "#fff",
  };
  assert.equal(outcome(evaluate(base([g])), "A004"), "failed");
  assert.equal(
    outcome(evaluate(base([{ ...g, contrast_role: "decorative" }])), "A004"),
    "inapplicable",
  );
});
test("reflow exception and missing applicability stay explicit", () => {
  const vp = { width: 320, height: 600, scroll_width: 500 };
  assert.equal(outcome(evaluate(base([], { viewport: vp })), "L001"), "review");
  assert.equal(
    outcome(evaluate(base([], { viewport: { ...vp, reflow_exception: false } })), "L001"),
    "failed",
  );
  assert.equal(
    outcome(evaluate(base([], { viewport: { ...vp, reflow_exception: true } })), "L001"),
    "inapplicable",
  );
});
test("Figma frame is not a browser reflow test", () =>
  assert.equal(
    outcome(
      evaluate(
        base([], {
          source: { kind: "figma" },
          viewport: { width: 320, height: 600, scroll_width: 320 },
        }),
      ),
      "L001",
    ),
    "untested",
  ));
test("style checks require an explicit profile", () =>
  assert.equal(outcome(evaluate(base([text()])), "D001"), "untested"));
test("font token drift is a project violation", () =>
  assert.equal(
    outcome(
      evaluate(base([text({ font_size: 17 })]), profile({ tokens: { font_sizes: [14, 16, 18] } })),
      "D001",
    ),
    "failed",
  ));
test("gradient budget is scoped to the configured policy", () => {
  const n = {
    id: "s",
    role: "surface",
    visible: true,
    background_image: "linear-gradient(red,blue)",
  };
  assert.equal(outcome(evaluate(base([n])), "D003"), "untested");
  assert.equal(
    outcome(evaluate(base([n]), profile({ decoration: { max_gradients: 0 } })), "D003"),
    "failed",
  );
});
test("missing decoration observations abstain", () =>
  assert.equal(
    outcome(evaluate(base([text()]), profile({ decoration: { max_shadows: 0 } })), "D004"),
    "cantTell",
  ));
test("repeated cards produce review, never an authorship verdict", () => {
  const nodes = Array.from({ length: 6 }, (_, i) => ({
    id: "card" + i,
    role: "surface",
    visible: true,
    border_radius: 12,
    rect: { x: 0, y: i * 120, width: 200, height: 100 },
  }));
  const r = evaluate(base(nodes), profile({ review: {} }));
  assert.equal(outcome(r, "P001"), "review");
  assert.equal(r.summary.authorship, "not_assessed");
  assert.equal(r.summary.counts.failed, 0);
});
test("generic phrases use literal words, not regex payloads or substrings", () => {
  const p = profile({ review: { generic_phrases: ["seamless", "a+b"] } });
  assert.equal(outcome(evaluate(base([text({ text: "A seamless setup" })]), p), "P003"), "review");
  assert.equal(
    outcome(evaluate(base([text({ text: "unseamless aaab" })]), p), "P003"),
    "inapplicable",
  );
  assert.equal(outcome(evaluate(base([text({ text: "a+b" })]), p), "P003"), "review");
});
test("empty input never becomes a clean usability or conformance verdict", () => {
  const r = evaluate(base());
  assert.equal(r.summary.decision, "review_required");
  assert.equal(r.summary.wcag_conformance, "not_assessed");
  assert.equal(outcome(r, "Q002"), "untested");
});
test("byte-identical output across repeated evaluation and node order", () => {
  const a = text({ id: "a" }),
    b = text({ id: "b" });
  assert.equal(stable(evaluate(base([a, b]))), stable(evaluate(base([b, a]))));
  assert.equal(stable(evaluate(base([a, b]))), stable(evaluate(base([a, b]))));
});
test("authorship metadata changes neither findings nor quality counts", () => {
  const a = evaluate(base([text()], { provenance: { author: "human" } })),
    b = evaluate(base([text()], { provenance: { author: "ai" } }));
  assert.deepEqual(a.findings, b.findings);
  assert.deepEqual(a.summary, b.summary);
  assert.notEqual(a.input_sha256, b.input_sha256);
});
test("invalid numbers, duplicate IDs and malformed profiles are rejected", () => {
  assert.throws(() => evaluate(base([text({ font_size: NaN })])));
  assert.throws(() => evaluate(base([text(), text()])));
  assert.throws(() => evaluate(base(), profile({ review: { pill_ratio: 2 } })));
  assert.throws(() => evaluate({ ...base(), schema_version: 2 }));
  assert.throws(() => evaluate(base(), profile({ decoration: { max_gradients: -1 } })));
});
test("pattern checks abstain when applicable geometry is unknown", () => {
  const r = evaluate(
    base([
      { id: "s", role: "surface", visible: true },
      { id: "c", role: "control", visible: true },
    ]),
    profile({ review: {} }),
  );
  assert.equal(outcome(r, "P001"), "cantTell");
  assert.equal(outcome(r, "P002"), "cantTell");
});
test("pills remain a review signal with no hard failure", () => {
  const nodes = Array.from({ length: 5 }, (_, i) =>
    control("p" + i, i * 120, 0, 100, 30, { border_radius: 15 }),
  );
  const r = evaluate(base(nodes), profile({ review: {} }));
  assert.equal(outcome(r, "P002"), "review");
  assert.equal(r.summary.counts.failed, 0);
});
test("empty evidence does not silently pass an opted-in decoration rule", () => {
  const r = evaluate(
    base([], { capture: { complete: false } }),
    profile({ decoration: { max_gradients: 0 } }),
  );
  assert.equal(outcome(r, "D003"), "cantTell");
});
