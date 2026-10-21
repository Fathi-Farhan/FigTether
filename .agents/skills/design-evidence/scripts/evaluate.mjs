import { readFileSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { fileURLToPath } from "node:url";
import path from "node:path";
export const VERSION = "1.0.0";
const cmp = (a, b) => (a < b ? -1 : a > b ? 1 : 0);
function canonical(v) {
  if (v === null || typeof v === "string" || typeof v === "boolean") return v;
  if (typeof v === "number" && Number.isFinite(v)) return v;
  if (Array.isArray(v)) return v.map(canonical);
  if (v && Object.getPrototypeOf(v) === Object.prototype)
    return Object.fromEntries(
      Object.keys(v)
        .sort(cmp)
        .map((k) => [k, canonical(v[k])]),
    );
  throw new Error("Inputs must contain finite JSON values only");
}
export const stable = (v) => JSON.stringify(canonical(v));
const hash = (v) =>
  createHash("sha256")
    .update(typeof v === "string" ? v : stable(v))
    .digest("hex");
const engineHash = hash(readFileSync(fileURLToPath(import.meta.url), "utf8"));
const finite = (v) => typeof v === "number" && Number.isFinite(v);
export function rgb(v) {
  if (typeof v !== "string") return null;
  if (/^#[0-9a-f]{6}$/i.test(v)) return [1, 3, 5].map((i) => parseInt(v.slice(i, i + 2), 16) / 255);
  if (/^#[0-9a-f]{3}$/i.test(v)) return [...v.slice(1)].map((c) => parseInt(c + c, 16) / 255);
  const m = v.match(
    /^rgba?\(\s*([\d.]+)[,\s]+([\d.]+)[,\s]+([\d.]+)(?:\s*[,/]\s*([\d.]+))?\s*\)$/i,
  );
  if (!m || (m[4] !== undefined && Number(m[4]) !== 1)) return null;
  const values = m.slice(1, 4).map(Number);
  return values.every((n) => finite(n) && n >= 0 && n <= 255) ? values.map((n) => n / 255) : null;
}
export function contrast(fg, bg) {
  const a = rgb(fg),
    b = rgb(bg);
  if (!a || !b) return null;
  const lum = (c) =>
    c
      .map((v) => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4))
      .reduce((s, v, i) => s + v * [0.2126, 0.7152, 0.0722][i], 0);
  const x = lum(a),
    y = lum(b);
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}
const rectValid = (r) =>
  r && ["x", "y", "width", "height"].every((k) => finite(r[k])) && r.width >= 0 && r.height >= 0;
function validate(s, p) {
  canonical(s);
  canonical(p);
  if (
    s.schema_version !== 1 ||
    !Array.isArray(s.nodes) ||
    s.nodes.length > 10000 ||
    !["dom", "figma", "manual"].includes(s.source?.kind) ||
    typeof s.capture?.complete !== "boolean"
  )
    throw new Error("Invalid snapshot schema, source, or capture coverage");
  const ids = new Set();
  for (const n of s.nodes) {
    if (
      !n ||
      typeof n.id !== "string" ||
      !n.id ||
      ids.has(n.id) ||
      !["text", "control", "graphic", "surface"].includes(n.role) ||
      typeof n.visible !== "boolean"
    )
      throw new Error("Invalid or duplicate node identity/role/visibility");
    ids.add(n.id);
    if (n.rect !== undefined && n.rect !== null && !rectValid(n.rect))
      throw new Error("Invalid node rectangle");
    for (const k of ["font_size", "font_weight", "border_radius"])
      if (n[k] !== undefined && n[k] !== null && (!finite(n[k]) || n[k] < 0))
        throw new Error("Invalid numeric observation: " + k);
    for (const k of [
      "text",
      "name",
      "name_source",
      "foreground",
      "background",
      "adjacent_color",
      "contrast_role",
      "background_image",
      "box_shadow",
    ])
      if (n[k] !== undefined && n[k] !== null && typeof n[k] !== "string")
        throw new Error("Invalid string observation: " + k);
    if (n.disabled !== undefined && typeof n.disabled !== "boolean")
      throw new Error("Invalid disabled state");
    if (
      n.target_exception !== undefined &&
      (!["inline", "equivalent", "user-agent", "essential"].includes(n.target_exception.kind) ||
        typeof n.target_exception.evidence !== "string" ||
        !n.target_exception.evidence.trim())
    )
      throw new Error("Target exception needs kind and evidence");
  }
  if (s.viewport) {
    for (const k of ["width", "height", "scroll_width"])
      if (!finite(s.viewport[k]) || s.viewport[k] <= 0) throw new Error("Invalid viewport");
    if (
      s.viewport.reflow_exception !== undefined &&
      typeof s.viewport.reflow_exception !== "boolean"
    )
      throw new Error("Invalid reflow exception");
  }
  if (p.schema_version !== 1 || typeof p.id !== "string" || !p.id)
    throw new Error("Invalid profile identity");
  for (const k of ["font_sizes", "radii"]) {
    const a = p.tokens?.[k];
    if (a !== undefined && (!Array.isArray(a) || !a.length || !a.every((v) => finite(v) && v >= 0)))
      throw new Error("Invalid token list: " + k);
  }
  for (const k of ["max_gradients", "max_shadows"]) {
    const v = p.decoration?.[k];
    if (v !== undefined && (!Number.isInteger(v) || v < 0))
      throw new Error("Invalid decoration limit");
  }
  for (const k of ["repeated_surface_min", "pill_min"]) {
    const v = p.review?.[k];
    if (v !== undefined && (!Number.isInteger(v) || v < 2))
      throw new Error("Invalid pattern sample minimum");
  }
  for (const k of ["repeated_surface_ratio", "pill_ratio"]) {
    const v = p.review?.[k];
    if (v !== undefined && (!finite(v) || v < 0 || v > 1)) throw new Error("Invalid pattern ratio");
  }
  if (
    p.review?.generic_phrases !== undefined &&
    (!Array.isArray(p.review.generic_phrases) ||
      !p.review.generic_phrases.every((v) => typeof v === "string" && v.trim()))
  )
    throw new Error("Invalid phrase list");
}
export function evaluate(s, p = { schema_version: 1, id: "unconfigured" }) {
  validate(s, p);
  const nodes = [...s.nodes].sort((a, b) => cmp(a.id, b.id)),
    visible = nodes.filter((n) => n.visible),
    findings = [];
  const add = (rule, node, outcome, category, message, observed = null) =>
    findings.push({ rule, node, outcome, category, message, observed });
  add(
    "Q001",
    "document",
    s.capture.complete ? "passed" : "cantTell",
    "coverage",
    s.capture.complete
      ? "Capture declares its scoped node set complete."
      : "Capture is incomplete; uncaptured nodes and states are not passes.",
  );
  add(
    "Q002",
    "document",
    "untested",
    "manual",
    "Task completion, recovery, hierarchy, originality, keyboard focus, and uncaptured states require separate review.",
  );
  const texts = visible.filter((n) => typeof n.text === "string" && n.text.trim());
  if (!texts.length)
    add("A001", "document", "inapplicable", "standard-derived", "No visible text target supplied.");
  for (const n of texts) {
    if (n.disabled || ["decorative", "logotype"].includes(n.contrast_role)) {
      add(
        "A001",
        n.id,
        "inapplicable",
        "standard-derived",
        "Inactive or explicitly excepted text.",
      );
      continue;
    }
    const value = contrast(n.foreground, n.background);
    if (value === null || !finite(n.font_size) || !finite(n.font_weight)) {
      add(
        "A001",
        n.id,
        "cantTell",
        "standard-derived",
        "Need resolved opaque colors and font metrics; effects and unknown backgrounds are not inferred.",
      );
      continue;
    }
    const minimum =
      n.font_size >= 24 || (n.font_size >= (14 * 96) / 72 && n.font_weight >= 700) ? 3 : 4.5;
    add(
      "A001",
      n.id,
      value >= minimum ? "passed" : "failed",
      "standard-derived",
      "Captured solid-color text contrast; ratio is not rounded before comparison.",
      { ratio: value, minimum },
    );
  }
  const controls = visible.filter((n) => n.role === "control" && !n.disabled);
  if (!controls.length) {
    add("A002", "document", "inapplicable", "standard-derived", "No visible enabled control.");
    add(
      "A003",
      "document",
      "inapplicable",
      "standard-derived",
      "No visible enabled pointer target.",
    );
  }
  for (const n of controls) {
    if (!["browser-ax", "verified-manual"].includes(n.name_source) || typeof n.name !== "string")
      add(
        "A002",
        n.id,
        "cantTell",
        "standard-derived",
        "A computed or manually verified accessible name is required.",
      );
    else
      add(
        "A002",
        n.id,
        n.name.trim() ? "passed" : "failed",
        "standard-derived",
        "Checks the supplied accessible name only, not full role/state semantics.",
      );
    const r = n.rect;
    if (!rectValid(r) || r.width === 0 || r.height === 0) {
      add(
        "A003",
        n.id,
        "cantTell",
        "standard-derived",
        "Need the actual captured pointer-target rectangle.",
      );
      continue;
    }
    if (r.width >= 24 && r.height >= 24) {
      add(
        "A003",
        n.id,
        "passed",
        "standard-derived",
        "Target meets captured 24 by 24 CSS-pixel size.",
        r,
      );
      continue;
    }
    if (n.target_exception) {
      add(
        "A003",
        n.id,
        "inapplicable",
        "standard-derived",
        "Explicit exception; its evidence needs reviewer verification.",
        n.target_exception,
      );
      continue;
    }
    const center = { x: r.x + r.width / 2, y: r.y + r.height / 2 };
    let conflict = null,
      unknown = !s.capture.complete;
    for (const bnode of controls) {
      if (bnode === n) continue;
      const b = bnode.rect;
      if (!rectValid(b) || b.width === 0 || b.height === 0) {
        unknown = true;
        continue;
      }
      const small = b.width < 24 || b.height < 24;
      const d = small
        ? Math.hypot(center.x - (b.x + b.width / 2), center.y - (b.y + b.height / 2))
        : Math.hypot(
            center.x - Math.max(b.x, Math.min(center.x, b.x + b.width)),
            center.y - Math.max(b.y, Math.min(center.y, b.y + b.height)),
          );
      if (d < (small ? 24 : 12)) {
        conflict = bnode.id;
        break;
      }
    }
    add(
      "A003",
      n.id,
      conflict ? "failed" : unknown ? "cantTell" : "passed",
      "standard-derived",
      conflict
        ? "Undersized target spacing intersects another supplied target."
        : unknown
          ? "Spacing cannot be established for incomplete geometry."
          : "Undersized target meets captured spacing.",
      { rect: r, conflicting_target: conflict },
    );
  }
  const graphics = visible.filter((n) => n.role === "graphic");
  if (!graphics.length)
    add("A004", "document", "inapplicable", "standard-derived", "No graphic target supplied.");
  for (const n of graphics) {
    if (["decorative", "logotype"].includes(n.contrast_role)) {
      add(
        "A004",
        n.id,
        "inapplicable",
        "standard-derived",
        "Explicit decorative or logotype exception.",
      );
      continue;
    }
    const v = contrast(n.foreground, n.adjacent_color);
    if (n.contrast_role !== "essential" || v === null)
      add(
        "A004",
        n.id,
        "cantTell",
        "standard-derived",
        "Need meaningful-cue classification and opaque adjacent colors.",
      );
    else
      add(
        "A004",
        n.id,
        v >= 3 ? "passed" : "failed",
        "standard-derived",
        "Captured essential non-text cue contrast.",
        { ratio: v, minimum: 3 },
      );
  }
  const vp = s.viewport;
  if (!vp || s.source.kind === "figma")
    add(
      "L001",
      "document",
      "untested",
      "layout",
      "No browser viewport observation; a Figma frame is not a reflow test.",
    );
  else {
    const overflow = vp.scroll_width > vp.width + 1,
      outcome = !overflow
        ? "passed"
        : vp.reflow_exception === true
          ? "inapplicable"
          : vp.reflow_exception === false
            ? "failed"
            : "review";
    add(
      "L001",
      "document",
      outcome,
      "layout",
      "Horizontal overflow at this captured width only; 1 CSS-pixel tolerance. Not a complete WCAG reflow audit.",
      { width: vp.width, scroll_width: vp.scroll_width },
    );
  }
  for (const [rule, key, field] of [
    ["D001", "font_sizes", "font_size"],
    ["D002", "radii", "border_radius"],
  ]) {
    const allowed = p.tokens?.[key];
    if (!allowed) {
      add(
        rule,
        "document",
        "untested",
        "project-contract",
        "No project token contract configured.",
      );
      continue;
    }
    const targets = visible.filter(
      (n) => field !== "font_size" || n.role === "text" || n.role === "control",
    );
    if (!targets.length)
      add(rule, "document", "inapplicable", "project-contract", "No applicable token target.");
    for (const n of targets)
      add(
        rule,
        n.id,
        !finite(n[field])
          ? "cantTell"
          : allowed.some((v) => Math.abs(v - n[field]) <= 0.01)
            ? "passed"
            : "failed",
        "project-contract",
        "Project token membership with 0.01 CSS-pixel tolerance.",
        { value: n[field] ?? null, allowed },
      );
  }
  for (const [rule, key, field, match] of [
    ["D003", "max_gradients", "background_image", (v) => /gradient\(/i.test(v)],
    ["D004", "max_shadows", "box_shadow", (v) => v !== "none" && v !== ""],
  ]) {
    const limit = p.decoration?.[key];
    if (limit === undefined) {
      add(rule, "document", "untested", "project-contract", "No decoration budget configured.");
      continue;
    }
    const measured = visible.filter((n) => typeof n[field] === "string"),
      matched = measured.filter((n) => match(n[field])).map((n) => n.id),
      unknown = measured.length !== visible.length || !s.capture.complete;
    add(
      rule,
      "document",
      matched.length > limit ? "failed" : unknown ? "cantTell" : "passed",
      "project-contract",
      "Configured decoration count, not an AI-authorship test.",
      { count: matched.length, limit, nodes: matched },
    );
  }
  const review = p.review;
  if (!review)
    for (const id of ["P001", "P002", "P003"])
      add(id, "document", "untested", "pattern-review", "No pattern-review profile configured.");
  else {
    const surfaces = visible.filter(
        (n) =>
          n.role === "surface" &&
          rectValid(n.rect) &&
          finite(n.border_radius) &&
          n.border_radius >= 8,
      ),
      groups = new Map();
    for (const n of surfaces) {
      const signature = [n.rect.width, n.rect.height, n.border_radius]
          .map((v) => Math.round(v / 4) * 4)
          .join("/"),
        ids = groups.get(signature) ?? [];
      ids.push(n.id);
      groups.set(signature, ids);
    }
    const largest =
        [...groups.values()].sort((a, b) => b.length - a.length || cmp(a[0], b[0]))[0] ?? [],
      min = review.repeated_surface_min ?? 6,
      ratio = review.repeated_surface_ratio ?? 0.6;
    const surfaceUnknown =
      !s.capture.complete ||
      visible.some((n) => n.role === "surface" && (!rectValid(n.rect) || !finite(n.border_radius)));
    add(
      "P001",
      "document",
      surfaces.length >= min && largest.length / surfaces.length >= ratio
        ? "review"
        : surfaceUnknown
          ? "cantTell"
          : "inapplicable",
      "pattern-review",
      "Repeated rounded-surface geometry, quantized to 4 CSS pixels. Deliberate repetition may justify it.",
      { surfaces: surfaces.length, largest_group: largest, minimum: min, ratio_threshold: ratio },
    );
    const pills = controls.filter(
        (n) =>
          rectValid(n.rect) &&
          finite(n.border_radius) &&
          n.rect.height > 0 &&
          n.rect.width >= 2 * n.rect.height &&
          n.border_radius >= n.rect.height / 2,
      ),
      pm = review.pill_min ?? 5,
      pr = review.pill_ratio ?? 0.75;
    const pillUnknown =
      !s.capture.complete || controls.some((n) => !rectValid(n.rect) || !finite(n.border_radius));
    add(
      "P002",
      "document",
      controls.length >= pm && pills.length / controls.length >= pr
        ? "review"
        : pillUnknown
          ? "cantTell"
          : "inapplicable",
      "pattern-review",
      "Pill-heavy control treatment; not a defect or origin verdict.",
      {
        controls: controls.length,
        pills: pills.map((n) => n.id),
        minimum: pm,
        ratio_threshold: pr,
      },
    );
    const phrases = review.generic_phrases;
    if (!phrases)
      add(
        "P003",
        "document",
        "untested",
        "pattern-review",
        "No local copy-review vocabulary configured.",
      );
    else {
      const matches = [];
      for (const n of texts)
        for (const phrase of phrases) {
          const escaped = phrase.replace(/[.*+?^$(){}|[\]\\]/g, "\\$&");
          if (new RegExp("(?<![\\p{L}\\p{N}])" + escaped + "(?![\\p{L}\\p{N}])", "iu").test(n.text))
            matches.push({ node: n.id, phrase });
        }
      add(
        "P003",
        "document",
        matches.length ? "review" : "inapplicable",
        "pattern-review",
        "Local generic-copy vocabulary; context determines whether a phrase should change.",
        { matches },
      );
    }
  }
  findings.sort((a, b) => cmp(a.rule, b.rule) || cmp(a.node, b.node));
  const counts = Object.fromEntries(
    ["passed", "failed", "inapplicable", "cantTell", "untested", "review"].map((outcome) => [
      outcome,
      findings.filter((f) => f.outcome === outcome).length,
    ]),
  );
  return {
    schema_version: 1,
    kind: "design-quality-observations",
    ruleset_version: VERSION,
    input_sha256: hash({ ...s, nodes }),
    profile_sha256: hash(p),
    ruleset_sha256: engineHash,
    coverage: {
      nodes: nodes.length,
      visible_nodes: visible.length,
      complete: s.capture.complete,
      source_kind: s.source.kind,
    },
    summary: {
      counts,
      decision: counts.failed ? "issues_found" : "review_required",
      authorship: "not_assessed",
      wcag_conformance: "not_assessed",
    },
    findings,
  };
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const args = process.argv.slice(2),
      input = args[0];
    if (!input || input.startsWith("--"))
      throw new Error(
        "Usage: evaluate.mjs snapshot.json [--profile profile.json] [--out report.json]",
      );
    const seen = new Set();
    for (let i = 1; i < args.length; i += 2) {
      if (
        !["--profile", "--out"].includes(args[i]) ||
        seen.has(args[i]) ||
        !args[i + 1] ||
        args[i + 1].startsWith("--")
      )
        throw new Error("Unknown, repeated, or incomplete option: " + args[i]);
      seen.add(args[i]);
    }
    const option = (name) => {
      const i = args.indexOf(name);
      if (i < 0) return null;
      if (!args[i + 1] || args[i + 1].startsWith("--"))
        throw new Error("Missing value for " + name);
      return args[i + 1];
    };
    const read = (file) => JSON.parse(readFileSync(file, "utf8").replace(/^\uFEFF/, ""));
    const report = evaluate(
        read(input),
        option("--profile") ? read(option("--profile")) : undefined,
      ),
      output = stable(report) + "\n",
      out = option("--out");
    if (out) writeFileSync(out, output);
    else process.stdout.write(output);
    process.exitCode = report.summary.counts.failed ? 1 : 0;
  } catch (error) {
    process.stderr.write(error.message + "\n");
    process.exitCode = 2;
  }
}
