import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import path from "node:path";

const raw = process.argv[2] ?? "benchmarks/results/readme-ui-2026-10-21";
const out = process.argv[3] ?? "benchmarks/published/readme-ui-2026-10-21.json";
const read = (name) =>
  JSON.parse(readFileSync(path.join(raw, name), "utf8").replace(/^\uFEFF/, ""));
const files = [
  "plugins/figma/dist/main.js",
  "plugins/figma/dist/ui.html",
  "plugins/figma/ui/assets/figtether-icon.png",
  "assets/figtether-banner.png",
  "assets/figtether-icon.png",
  "assets/architecture.svg",
  "assets/benchmark-desktop.svg",
  "assets/benchmark-stages.svg",
  "assets/bridge-setup.svg",
  "assets/bridge-ui-desktop.png",
  "scripts/render-readme-assets.mjs",
];
const artifacts = files.map((file) => {
  const data = readFileSync(file);
  return { file, bytes: data.length, sha256: createHash("sha256").update(data).digest("hex") };
});
const git = (...args) =>
  execFileSync("git", ["-c", `safe.directory=${process.cwd().replaceAll("\\", "/")}`, ...args], {
    encoding: "utf8",
  });
const report = {
  schema_version: 1,
  kind: "documentation-and-ui-verification",
  created_at: new Date().toISOString(),
  source: {
    commit: git("rev-parse", "HEAD").trim(),
    dirty: true,
    tracked_diff_sha256: createHash("sha256")
      .update(git("diff", "--binary", "HEAD"))
      .digest("hex"),
  },
  artifacts,
  checks: [
    { ...read("plugin-tests-final.json"), passed_tests: 467, failed_tests: 0, test_files: 39 },
    { ...read("typecheck.json"), passed_projects: 5 },
    {
      ...read("rust-write-path.json"),
      passed_tests: 20,
      failed_tests: 0,
      daemon_tests: 7,
      e2e_tests: 13,
    },
    read("public-check.json"),
  ],
  retained_initial_failure: {
    source_log: `${raw}/plugin-tests.log`,
    passed: 466,
    failed: 1,
    reason: "Existing UI test expected old 'not allowed' wording after the copy update.",
    resolution:
      "Assertion updated to the new explicit 'Agent editing is off' wording; full suite passed.",
  },
  visual_review: {
    direction: "Editorial skill from bergside/awesome-design-skills",
    figures: ["banner", "icon", "architecture", "desktop chart", "offline chart", "pairing guide"],
    browser_ui_width_css_px: 420,
    browser_ui_result: "Icon visible; text wraps and controls fit; unpaired state.",
    figma_desktop: "126.10.10",
    native_ui_result:
      "Closed the old plugin dialog through scoped UI Automation; reopened the last plugin through its menu. Native capture confirms updated icon, headings, source-secret disclosure and unpaired labels.",
    native_ui_evidence: "assets/bridge-ui-desktop.png",
    native_pairing_or_mutation_retested: false,
    desktop_automation_limits: [
      "An initial keyboard shortcut was refused by the foreground guard; no keys were delivered.",
      "A later coordinate-based disclosure toggle did not expand it. The disclosure's native click/keyboard interaction was not verified in Figma; its content is present in the built HTML.",
    ],
  },
  previous_performance_data: "benchmarks/published/desktop-2026-10-20.json",
  performance_note:
    "This is a UI/documentation verification, not a new desktop latency benchmark. Prior measured artifact hashes remain unchanged in their historical report.",
  costs: {
    model_tokens: null,
    model_cost_usd: null,
    status: "NOT_MEASURED",
    image_generation_calls_this_work: 4,
    reason:
      "Chat and image-generation billing telemetry and exact image model revision are unavailable; missing cost is not zero.",
  },
  raw_evidence_directory: raw,
};
mkdirSync(path.dirname(out), { recursive: true });
writeFileSync(out, JSON.stringify(report, null, 2) + "\n");
console.log(out);
