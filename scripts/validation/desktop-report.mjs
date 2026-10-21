import {
  readFileSync,
  writeFileSync,
  existsSync,
  mkdirSync,
  copyFileSync,
  statSync,
} from "node:fs";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import path from "node:path";
const dirs = (process.argv[2] ?? "benchmarks/results/desktop-2026-10-20").split(",");
const dir = dirs.at(-1);
const stem = process.argv[3] ?? "desktop-2026-10-20";
const hash = (value) => createHash("sha256").update(value).digest("hex");
mkdirSync(`${dir}/evidence`, { recursive: true });
// Capture the changed sources and runner alongside measurements, including for dirty builds.
const diff = execFileSync(
  "git",
  ["-c", `safe.directory=${process.cwd().replaceAll("\\", "/")}`, "diff"],
  { encoding: "utf8" },
).trim();
const patchFile = `${dir}/evidence/${hash(diff)}.patch`;
if (!existsSync(patchFile)) writeFileSync(patchFile, diff + "\n");
for (const source of [
  "scripts/validation/mcp-run.mjs",
  "scripts/validation/desktop-website.mjs",
  "tmp/desktop-validation/website-fixture.json",
  "tmp/desktop-validation/benchmark.json",
]) {
  if (existsSync(source)) {
    const content = readFileSync(source);
    const target = `${dir}/evidence/${hash(content)}-${path.basename(source)}`;
    if (!existsSync(target)) copyFileSync(source, target);
  }
}
const records = dirs.flatMap((directory) =>
  readFileSync(`${directory}/calls.raw.jsonl`, "utf8")
    .trim()
    .split(/\r?\n/)
    .map((line) => JSON.parse(line)),
);
const semanticOk = (r) =>
  r.ok &&
  r.data?.blocked !== true &&
  r.data?.status !== "partial" &&
  !(r.tool === "get_nodes" && r.args.node_ids?.length > 0 && r.data?.data?.nodes?.length === 0);
const buildKey = (r) =>
  `${r.build.profile}-${r.build.binary_sha256.slice(0, 12)}-${r.build.plugin_sha256.slice(0, 12)}-${r.build.ui_sha256.slice(0, 12)}`;
const measuredLane = (r) =>
  r.lane.startsWith("W2-fresh-") && Number.isInteger(r.data?.coverage?.returned_nodes)
    ? `W2-fresh-${r.data.coverage.returned_nodes}-nodes`
    : r.lane;
const group = Object.groupBy(records, (r) => `${buildKey(r)}/${measuredLane(r)}`);
const quantile = (values, p) => values[Math.max(0, Math.ceil(values.length * p) - 1)] ?? null;
const lanes = Object.entries(group).map(([key, rows]) => {
  const values = rows.map((r) => r.elapsed_ms).sort((a, b) => a - b);
  const successes = rows.filter(semanticOk);
  const nodeCounts = rows.map((r) => r.data?.coverage?.returned_nodes).filter(Number.isFinite);
  const segments = Object.values(
    Object.groupBy(rows, (r) => `${r.label}/${r.args.session_id ?? "no-session"}`),
  );
  const wall_ms = segments.reduce(
    (total, part) =>
      total +
      Date.parse(part.at(-1).started) +
      part.at(-1).elapsed_ms -
      Date.parse(part[0].started),
    0,
  );
  const hitValues = rows
    .filter((r) => r.data?.observation?.freshness === "cache_hit")
    .map((r) => r.elapsed_ms)
    .sort((a, b) => a - b);
  return {
    build: key.split("/")[0],
    lane: measuredLane(rows[0]),
    requested_lane: rows[0].lane,
    samples: rows.length,
    success: successes.length,
    failed: rows.length - successes.length,
    p50_ms: quantile(values, 0.5),
    p95_ms: quantile(values, 0.95),
    p99_ms: quantile(values, 0.99),
    max_ms: values.at(-1),
    logical_json_max_bytes: Math.max(...rows.map((r) => Buffer.byteLength(JSON.stringify(r.data)))),
    wire_json_max_bytes: Math.max(...rows.map((r) => r.response_bytes)),
    returned_nodes_min: nodeCounts.length ? Math.min(...nodeCounts) : null,
    returned_nodes_max: nodeCounts.length ? Math.max(...nodeCounts) : null,
    observed_freshness: [
      ...new Set(rows.map((r) => r.data?.observation?.freshness).filter(Boolean)),
    ],
    freshness_counts: Object.fromEntries(
      Object.entries(Object.groupBy(rows, (r) => r.data?.observation?.freshness ?? "unknown")).map(
        ([k, v]) => [k, v.length],
      ),
    ),
    confirmed_cache_hit_p95_ms: quantile(hitValues, 0.95),
    run_segments: segments.length,
    elapsed_wall_ms: wall_ms,
    measured_calls_per_second: (1000 * rows.length) / wall_ms,
  };
});
const resources = dirs.flatMap((directory) =>
  existsSync(`${directory}/resources.raw.jsonl`)
    ? readFileSync(`${directory}/resources.raw.jsonl`, "utf8")
        .trim()
        .split(/\r?\n/)
        .filter(Boolean)
        .map(JSON.parse)
    : [],
);
const idle = resources.filter((r) => r.phase === "idle");
const builds = [
  ...new Map(records.map((r) => [buildKey(r), { id: buildKey(r), ...r.build }])).values(),
];
const failures = records
  .filter((r) => !semanticOk(r))
  .map((r) => ({
    time: r.started,
    build: buildKey(r),
    label: r.label,
    tool: r.tool,
    expected: r.lane.startsWith("expected-"),
    elapsed_ms: r.elapsed_ms,
    code:
      r.data?.code ??
      (r.data?.blocked
        ? "BLOCKED_PREVIEW"
        : r.tool === "get_nodes"
          ? "EMPTY_NODE_RESULT"
          : "FUNCTIONAL_FAILURE"),
    message: r.data?.message ?? r.data?.reasons ?? r.protocol_error,
  }));
const engineering_checks = lanes
  .filter((l) => l.lane === "W1-cached-root" || l.lane.startsWith("W2-fresh-"))
  .map((l) => ({
    build: l.build,
    lane: l.lane,
    latency_pass: l.p95_ms <= (l.lane.startsWith("W1") ? 100 : 2000),
    logical_size_pass: l.logical_json_max_bytes <= (l.lane.startsWith("W1") ? 8192 : 65536),
    all_calls_in_expected_lane:
      l.freshness_counts[l.lane.startsWith("W1") ? "cache_hit" : "fresh_read"] === l.samples,
    root_only_pass: l.lane.startsWith("W1") ? l.returned_nodes_max === 1 : null,
    fixture_scope_pass: l.lane.startsWith("W2")
      ? l.returned_nodes_min === 72 && l.returned_nodes_max === 72
      : true,
    note: "A local diagnostic check, not certification of all acceptance requirements.",
  }));
const setup_findings = [
  [
    "bridge-imported.png",
    "MANIFEST_REJECTED",
    "Unexpected networkAccess.reason property; changed to reasoning.",
  ],
  [
    "clean-import.png",
    "LOOPBACK_PATTERN_REJECTED",
    "Numeric loopback pattern rejected; localhost development patterns accepted.",
  ],
  [
    "boot-error.png",
    "OPTIONAL_API_PERMISSION",
    "teamLibrary getter required a permission; absent capability now remains optional.",
  ],
  [
    "source-debug.png",
    "EDITOR_RELAY_SOURCE",
    "Figma Desktop delivered sandbox responses from the top-level editor window.",
  ],
  [
    "root-frame.png",
    "NO_SELECTION_RESPONSE",
    "UI-to-sandbox channel did not reach Figma; corrected parent/pluginMessage routing.",
  ],
  [
    "reconnect-ready.png",
    "PAIRING_FAILED",
    "New pairing was overridden by the previous daemon credential; explicit pair now replaces it.",
  ],
]
  .filter(([file]) => existsSync(`tmp/desktop-validation/${file}`))
  .map(([file, code, description]) => ({
    observed_at: statSync(`tmp/desktop-validation/${file}`).mtime.toISOString(),
    code,
    description,
    screenshot: file,
    elapsed_ms: null,
    cost_usd: null,
  }));
const report = {
  schema_version: 1,
  created_at: new Date().toISOString(),
  fixture: {
    name: "FigTether website landing page",
    native_scene_nodes: 72,
    editable_children: 71,
    width: 1440,
    height: 1640,
    manifest: "plugins/figma/manifest.json",
    fixture_sha256: hash(readFileSync("tmp/desktop-validation/website-fixture.json")),
    native_png_sha256: hash(readFileSync("tmp/desktop-validation/figtether-website.png")),
  },
  builds,
  lanes,
  engineering_checks,
  failures,
  setup_findings,
  raw_run_directories: dirs,
  resources: {
    samples: resources.length,
    interval_seconds: 1,
    idle_samples: idle.length,
    idle_working_set_max_bytes: idle.length
      ? Math.max(...idle.map((r) => r.daemon_working_set_bytes))
      : null,
    peak_working_set_bytes: resources.length
      ? Math.max(...resources.map((r) => r.daemon_working_set_bytes))
      : null,
    peak_private_bytes: resources.length
      ? Math.max(...resources.map((r) => r.daemon_private_bytes))
      : null,
    note: "Windows WorkingSet64 approximates RSS. Figma process sums include shared pages and must not be treated as unique physical RAM.",
  },
  costs: {
    model_tokens: null,
    model_cost_usd: null,
    status: "NOT_MEASURED",
    reason:
      "Chat/API billing and token counters are not exposed in this session. The runner invokes no model or paid remote API.",
  },
  limits: [
    "One Windows machine and Figma Desktop 126.10.10; no macOS/seat/client matrix.",
    "get_selection timed out and inspect_design returned a protocol mismatch in the final functional controls. These tools remain unverified/broken in this editor workflow.",
    "An out-of-scope read was correctly refused; that expected refusal is retained alongside unexpected errors.",
    "The initially mislabeled fresh-72-node run on the corrected plugin returned one node; it is reported as W2-fresh-1-nodes. The subsequent 1000-call run asserted exactly 72 returned nodes on every call.",
    "Website PNG is 1440x1640, not the planned N03 1024x1024 fixture.",
    "No 60-minute soak, editor freeze profiling, CPU isolation, or multi-client contention test.",
    "Call elapsed includes MCP, daemon queue, bridge, editor and normalization; these phases are not individually instrumented.",
    "The PNG file was obtained with Figma native Copy as PNG; MCP export measurements validate render/transport/asset metadata, not client download delivery.",
    "Writes used UI automation to approve each exact preview under the user’s explicit desktop testing authorization.",
    "Synthetic preview content and diagram status labels are artwork, not benchmark observations.",
  ],
};
mkdirSync("benchmarks/published", { recursive: true });
writeFileSync(`benchmarks/published/${stem}.json`, JSON.stringify(report, null, 2) + "\n");
const benchmarkLanes = lanes.filter((l) => /^W[123]-/.test(l.lane));
const rows = benchmarkLanes
  .map(
    (l) =>
      `| ${l.build.split("-").slice(-2).join("/")} | ${l.lane} | ${l.samples} | ${l.success}/${l.samples} | ${l.p50_ms.toFixed(2)} | ${l.p95_ms.toFixed(2)} | ${l.p99_ms.toFixed(2)} |`,
  )
  .join("\n");
const text = `# Figma Desktop website test - 2026-10-20\n\nCreated a 1440×1640 landing page in the supplied Untitled Figma file using FigTether's local MCP stdio adapter, native daemon and development plugin. The frame contains 71 editable child layers (72 scene nodes including the frame). Its text and typography were read back through MCP. A full-resolution native Figma PNG was visually inspected.\n\n## Measured workloads\n\n| Lane | Samples | Successful | p50 ms | p95 ms | p99 ms |\n| --- | ---: | ---: | ---: | ---: | ---: |\n${rows}\n\nPercentiles use nearest rank and include failed calls. Cached/fresh/export lanes stay separate. Initial setup and earlier-build failures remain in the JSON report and raw log. The build records include exact commit, dirty state, source diff hash and binary/plugin/UI hashes.\n\n## Resource and cost data\n\n${resources.length} resource samples at one-second intervals. Daemon peak Windows working set: ${(report.resources.peak_working_set_bytes / 1048576).toFixed(2)} MiB; peak private bytes: ${(report.resources.peak_private_bytes / 1048576).toFixed(2)} MiB. Idle samples: ${idle.length}. This is a shared workstation measurement.\n\nModel token usage and USD cost: **NOT_MEASURED**. Billing information is not exposed here. The runner performs no model inference or paid API calls; this does not imply the chat itself is free.\n\n## Compatibility findings\n\nThe real editor exposed and prompted fixes for the invalid manifest reasoning key, numeric loopback domain rejection, missing showUI startup, non-inlined UI assets, a permission-protected teamLibrary getter, incorrect iframe routing, top-level editor message delivery, synchronous lookup under dynamic-page, requested font style not being assigned, batch export normalization, and old credentials overriding an explicit new pairing secret. Regression coverage was added for startup, routing, lookup, typography, pairing and export batches.\n\n## Scope and limits\n\n${report.limits.map((l) => "- " + l).join("\n")}\n\nThese results do not certify every N01–N10 requirement. In particular, the export fixture differs from N03 and this run does not establish N05/N06.\n\n## Reproduce and retain versions\n\nSee [the desktop testing guide](../desktop-testing.md). Raw calls and one-second resource samples stay under ignored \`benchmarks/results/desktop-2026-10-20/\`. The PNG, fixture, desktop screenshots, state mapping and live request specs stay under ignored \`tmp/desktop-validation/\`. Reviewed aggregates are in [the JSON report](${stem}.json). Never overwrite a prior run directory for a different version.\n`;
const cacheDetails = lanes
  .filter((l) => l.lane === "W1-cached-root")
  .map(
    (l) =>
      `- Build \`${l.build}\`: ${l.samples} cached requests; ${l.freshness_counts.cache_hit ?? 0} confirmed cache hits; ${l.freshness_counts.fresh_read ?? 0} fresh fallbacks; ${l.returned_nodes_max} nodes maximum; ${l.logical_json_max_bytes} bytes maximum logical response.`,
  )
  .join("\n");
const rendered = text
  .replace(
    "| Lane | Samples | Successful | p50 ms | p95 ms | p99 ms |",
    "| Build (plugin/UI hashes) | Lane | Samples | Successful | p50 ms | p95 ms | p99 ms |",
  )
  .replace(
    "| --- | ---: | ---: | ---: | ---: | ---: |",
    "| --- | --- | ---: | ---: | ---: | ---: | ---: |",
  )
  .replace(
    "## Resource and cost data",
    `The first build ignored the requested root-only depth and returned 72 nodes (over the 8 KiB metadata target). The corrected plugin honors the depth and limit. Both versions remain in this report. Counts below distinguish actual cache hits from fresh fallbacks:\n\n${cacheDetails}\n\n## Resource and cost data`,
  )
  .replace(
    "Raw calls and one-second resource samples stay under ignored `benchmarks/results/desktop-2026-10-20/`.",
    `Raw calls and one-second resource samples stay in these ignored run directories: ${dirs.map((d) => "\`" + d + "\`").join(", ")}.`,
  );
writeFileSync(`benchmarks/published/${stem}.md`, rendered);
console.log(
  JSON.stringify({
    report: `benchmarks/published/${stem}.md`,
    lanes: benchmarkLanes,
    failures: failures.length,
    resources: report.resources,
  }),
);
