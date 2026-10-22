// Original, deterministic figures. Measured data stays in benchmarks/published/.
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const check = process.argv.includes("--check");
const C = { ink: "#153f2e", paper: "#f7f6ee", lime: "#d7f3ac", rule: "#748375" };
const esc = (v) =>
  String(v).replace(
    /[&<>"']/g,
    (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&apos;" })[c],
  );
const t = (x, y, value, cls = "body", extra = "") =>
  `<text x="${x}" y="${y}" class="${cls}" ${extra}>${esc(value)}</text>`;
const line = (x1, y1, x2, y2, extra = "") =>
  `<path d="M ${x1} ${y1} L ${x2} ${y2}" fill="none" stroke="${C.rule}" ${extra}/>`;
const rule = (y) => line(40, y, 760, y);
const arrow = (x, y1, y2) =>
  `<path d="M ${x} ${y1} V ${y2}" fill="none" stroke="${C.ink}" stroke-width="3" marker-end="url(#arrow)"/>`;
const heading = (section, title, subtitle) =>
  `${t(40, 48, `FIGTETHER / ${section}`, "meta")}${t(40, 104, title, "heading")}${t(40, 144, subtitle, "body")}`;
const lines = (x, y, values, cls = "body", step = 32) =>
  values.map((v, i) => t(x, y + i * step, v, cls)).join("\n");
function svg(height, title, desc, body) {
  return `<svg xmlns="http://www.w3.org/2000/svg" width="800" height="${height}" viewBox="0 0 800 ${height}" role="img" aria-labelledby="title desc">
<title id="title">${esc(title)}</title><desc id="desc">${esc(desc)}</desc>
<style>text{fill:${C.ink};font-family:Arial,Helvetica,sans-serif;font-size:24px}.heading{font-size:40px;font-weight:800;letter-spacing:-1px}.label{font-size:28px;font-weight:700}.body{font-size:24px}.small{font-size:22px}.meta{font-size:20px;font-weight:700;letter-spacing:1px}.code{font-family:Consolas,monospace;font-size:26px;font-weight:700}.value{font-size:32px;font-weight:700;font-variant-numeric:tabular-nums}.tick{font-size:20px}.step{font-size:26px;font-weight:800}</style>
<rect width="800" height="${height}" fill="${C.paper}"/>
<defs><marker id="arrow" markerWidth="7" markerHeight="7" refX="6" refY="3.5" orient="auto"><path d="M 0 0 L 7 3.5 L 0 7" fill="${C.ink}"/></marker></defs>
${body}
</svg>\n`;
}
function step(number, y, title, detail) {
  return `<rect x="40" y="${y - 26}" width="40" height="40" rx="6" fill="${C.lime}" stroke="${C.ink}" stroke-width="2"/>${t(60, y + 3, number, "step", 'text-anchor="middle"')}${t(100, y + 3, title, "label")}${lines(100, y + 45, detail, "body")}`;
}
function architecture() {
  const nodes = [
    ["Coding agent", "Your MCP client", "stdio"],
    ["Rust MCP adapter", "figtether mcp --stdio", "Local WebSocket"],
    ["Local daemon", "Scope, approval, queue, cache", "Local WebSocket"],
    ["Figma plugin", "Reads and edits native nodes", null],
  ]
    .map(([title, detail, transport], i) => {
      const y = 192 + i * 156;
      const handles =
        i === 3
          ? [
              [44, y - 6],
              [744, y - 6],
              [44, y + 102],
              [744, y + 102],
            ]
              .map(
                ([x, yy]) =>
                  `<rect x="${x}" y="${yy}" width="12" height="12" fill="${C.lime}" stroke="${C.ink}" stroke-width="2"/>`,
              )
              .join("")
          : "";
      return `<rect x="50" y="${y}" width="700" height="108" rx="${i === 3 ? 0 : 8}" fill="${i === 2 ? C.lime : C.paper}" stroke="${C.ink}" stroke-width="2"/>${t(74, y + 40, title, "label")}${t(74, y + 78, detail, "small")}${handles}${transport ? `${arrow(400, y + 115, y + 145)}${t(432, y + 137, transport, "small")}` : ""}`;
    })
    .join("\n");
  return svg(
    1170,
    "How FigTether connects AI agents to Figma",
    "A coding agent calls the Rust MCP adapter over stdio. The adapter and daemon use an authenticated loopback WebSocket; the daemon connects to the Figma plugin over another local WebSocket. The manifest fixes the shared port. Native writes require a shared root, Allow editing, preview, exact-plan approval, then apply. Auto is optional and user-enabled for eligible plans.",
    `${heading("HOW IT WORKS", "One bridge. Native designs.", "Your agent proposes. Figma creates the real nodes.")}${nodes}
${lines(40, 824, ["Local WebSockets use the port in your manifest.", "The daemon validates scope before editor work."], "small")}
${rule(882)}${t(40, 928, "Every edit has a plan.", "label")}
${lines(40, 970, ["1. preview_write - inspect the proposed changes.", "2. get_write_approval - check the exact plan.", "3. apply_write - create or edit native nodes."], "small", 36)}
${lines(40, 1102, ["Share a frame and enable Allow editing first.", "Review each plan, or enable Auto for eligible plans."], "small")}`,
  );
}
function bridgeGuide() {
  return svg(
    1230,
    "Connect FigTether to Figma",
    "Run figtether start from the source checkout. It builds the plugin, prepares the manifest, starts the daemon and prints a one-time pairing secret. Import the printed dist/manifest.json in Figma, open Bridge and pair. Share selected frames, enable Allow editing if desired, and review plans or explicitly enable eligible Auto mode. A fresh secret is available with figtether pair; it lasts five minutes and works once.",
    `${heading("GET CONNECTED", "From terminal to canvas.", "Start once. Pair the plugin. Choose what to share.")}
${step("1", 210, "Start on your computer.", ["Run this from the source checkout:"])}
<rect x="100" y="282" width="660" height="64" rx="8" fill="${C.ink}"/>
${t(124, 324, "figtether start", "code", `style="fill:${C.paper}"`)}
${lines(100, 384, ["Builds the plugin and starts the daemon.", "Keep the terminal open for its pairing secret."], "small")}
${step("2", 492, "Open the plugin in Figma.", ["Import the printed dist/manifest.json.", "Open Bridge, paste the secret, then Pair."])}
${step("3", 662, "Share a frame.", ["Select a frame, then Update selection.", "Choose the frames and share them in Bridge."])}
${step("4", 832, "Choose how editing works.", ["Enable Allow editing when you want changes.", "Review plans, or explicitly turn on Auto."])}
${rule(954)}${t(40, 1004, "Already running? Need a fresh secret?", "label")}
${t(40, 1050, "figtether pair", "code")}
${lines(40, 1094, ["The secret works once and expires after 5 minutes.", "Pairing connects; sharing grants design access.", "Keep the secret out of chat and MCP configuration."], "small")}`,
  );
}

function validatePercentiles(lane, keys, max, name) {
  const values = keys.map((key) => lane[key]);
  if (
    !values.every((v) => Number.isFinite(v) && v >= 0) ||
    values.some((v, i) => i > 0 && v < values[i - 1]) ||
    values.at(-1) > max
  )
    throw new Error(`Figure percentile or axis mismatch: ${name}`);
}
function latencyFigure({ section, title, subtitle, description, lanes, percentiles, unit, notes }) {
  const rowHeight = percentiles.length === 3 ? 288 : 248;
  const bottom = 188 + lanes.length * rowHeight;
  const bars = lanes
    .map((lane, i) => {
      const y = 194 + i * rowHeight,
        x = 112,
        plot = 440;
      const end = y + 76 + (percentiles.length - 1) * 44;
      return `${t(40, y, lane.title, "label")}${t(40, y + 36, lane.scope, "small")}
${percentiles
  .map((p, j) => {
    const yy = y + 64 + j * 44,
      value = lane.values[p],
      w = (plot * value) / lane.max;
    const bar =
      p === 99
        ? `<path d="M ${x} ${yy + 10} H ${x + w}" stroke="${C.ink}" stroke-width="4" stroke-dasharray="8 4"/>`
        : `<rect x="${x}" y="${yy}" width="${w}" height="20" fill="${p === 95 ? C.ink : C.lime}" stroke="${C.ink}" stroke-width="1.5"/>`;
    return `${t(40, yy + 18, `p${p}`, "small")}${bar}${t(760, yy + 18, `${value.toFixed(unit === "ms" ? 2 : 1)} ${unit}`, "value", 'text-anchor="end"')}`;
  })
  .join("\n")}
${line(x, end + 28, x + plot, end + 28)}
${[0, 0.5, 1].map((f) => `${line(x + plot * f, end + 28, x + plot * f, end + 34)}${t(x + plot * f, end + 58, `${lane.max * f}`, "tick", 'text-anchor="middle"')}`).join("")}
${i < lanes.length - 1 ? rule(y + rowHeight - 38) : ""}`;
    })
    .join("\n");
  return svg(
    bottom + 80 + notes.length * 32,
    title,
    description,
    `${heading(section, title, subtitle)}${bars}${rule(bottom)}${lines(40, bottom + 46, notes, "small")}`,
  );
}
function desktopBenchmark() {
  const d = JSON.parse(
    readFileSync(path.join(root, "benchmarks/published/desktop-2026-10-20.json"), "utf8"),
  );
  const build = d.builds.filter((b) => b.profile === "release").at(-1);
  if (!build) throw new Error("Desktop figure needs a measured release build");
  const specs = [
    ["W1-cached-root", "Cached root", 1, 1.2, "1 node"],
    ["W2-fresh-72-nodes", "Fresh design read", 72, 200, "72 nodes"],
    ["W3-website-png-1440x1640", "Website PNG metadata", 1, 800, "1440 × 1640 px"],
  ];
  const measured = specs.map(([name, title, nodes, max, scope]) => {
    const lane = d.lanes.find((l) => l.build === build.id && l.lane === name);
    if (
      !lane ||
      lane.returned_nodes_min !== nodes ||
      lane.returned_nodes_max !== nodes ||
      lane.success !== lane.samples ||
      lane.failed !== 0
    )
      throw new Error(`Desktop figure scope mismatch: ${name}`);
    validatePercentiles(lane, ["p50_ms", "p95_ms", "p99_ms"], max, name);
    return { ...lane, title, max, scope };
  });
  return latencyFigure({
    section: "DESKTOP / 20–21 OCT 2026",
    title: "Real Figma. Measured latency.",
    subtitle: "Release build · Windows · One machine · Lower is faster",
    description:
      "Historical Figma Desktop 126.10.10 measurements. Independent zero-based millisecond axes. Cached-root p50/p95/p99: 0.65/0.90/1.08 ms, 20,010 requests including 20,000 confirmed cache hits and 10 fresh fallbacks. Fresh 72-node read: 113.53/127.75/169.79 ms, 1,000 requests. PNG render/export metadata: 566.82/613.76/640.86 ms, 100 requests. Client download not validated. Model cost unmeasured. This historical run also recorded get_selection timeout and inspect_design protocol mismatch.",
    lanes: measured.map((l) => ({
      title: l.title,
      scope: `${l.scope} · ${l.samples.toLocaleString("en-US")} requests`,
      max: l.max,
      values: { 50: l.p50_ms, 95: l.p95_ms, 99: l.p99_ms },
    })),
    percentiles: [50, 95, 99],
    unit: "ms",
    notes: [
      "Each workload has its own zero-based scale (ms).",
      "Cache: 20,000 hits + 10 fresh fallbacks.",
      "PNG: metadata timing; client download not validated.",
      `Daemon peak: ${(d.resources.peak_working_set_bytes / 2 ** 20).toFixed(2)} MiB across ${d.resources.samples} samples.`,
      "Historical: get_selection timed out;",
      "inspect_design had a protocol mismatch.",
      "Model cost / tokens: not measured. Full report linked below.",
    ],
  });
}
function offlineBenchmark() {
  const d = JSON.parse(
    readFileSync(path.join(root, "benchmarks/published/offline-stages.json"), "utf8"),
  );
  if (
    d.kind !== "offline-diagnostic" ||
    d.unit !== "ms" ||
    d.stages.length !== 4 ||
    !(d.samples_per_stage > 0)
  )
    throw new Error("Unexpected offline source");
  for (const stage of d.stages)
    validatePercentiles(stage, ["p50", "p95", "p99"], Infinity, stage.key);
  const max = Math.max(200, Math.ceil(Math.max(...d.stages.map((s) => s.p95 * 1000)) / 50) * 50);
  return latencyFigure({
    section: "OFFLINE / RUST DIAGNOSTIC",
    title: "Inside the Rust core.",
    subtitle: `${d.samples_per_stage.toLocaleString("en-US")} samples per stage · Synthetic 100-node fixture`,
    description:
      "One offline synthetic Rust diagnostic on AMD Ryzen 5 4500. Directly labeled p50 and p95 bars in microseconds, all using the same zero-based linear scale. Independent operations, not an end-to-end pipeline. Figma, network and model inference not measured. One run without confidence intervals. Original source and hashes are linked in the accompanying Markdown.",
    lanes: d.stages.map((s) => ({
      title: s.label,
      scope: "Offline operation · microseconds",
      max,
      values: { 50: s.p50 * 1000, 95: s.p95 * 1000 },
    })),
    percentiles: [50, 95],
    unit: "µs",
    notes: [
      "All stages use the same zero-based scale (µs).",
      "Independent operations; not an end-to-end request.",
      "Figma, network and model inference: not measured.",
      "One run, one machine. No confidence interval.",
      "Source data and reproduction notes are linked below.",
    ],
  });
}
function websiteBenchmark() {
  const d = JSON.parse(
    readFileSync(path.join(root, "benchmarks/published/website-2026-10-21.json"), "utf8"),
  );
  if (
    d.kind !== "native-editable-website-diagnostic" ||
    d.build.profile !== "debug" ||
    d.design.verified_unique_nodes !== 123 ||
    d.design.successful_applies !== 6
  )
    throw new Error("Unexpected website diagnostic source");
  const specs = [
    ["fresh-default-38-nodes", "Fresh 38-node read", 10, 150],
    ["fresh-first-page-50-nodes", "First page, 50 nodes", 10, 150],
    ["full-website-png-metadata", "1440 × 4440 PNG metadata", 3, 2000],
  ];
  const lanes = specs.map(([name, title, count, max]) => {
    const lane = d.workloads.find((l) => l.lane === name);
    if (!lane || lane.count !== count || lane.successes !== count || lane.failures !== 0)
      throw new Error(`Website figure scope mismatch: ${name}`);
    validatePercentiles(lane, ["p50_ms", "p95_ms", "p99_ms"], max, name);
    return {
      title,
      scope: `${lane.count} requests · ${lane.successes}/${lane.count} succeeded`,
      max,
      values: { 50: lane.p50_ms, 95: lane.p95_ms },
    };
  });
  return latencyFigure({
    section: "DESKTOP / 21 OCT 2026",
    title: "123 native, editable nodes.",
    subtitle: "Six approved writes · Debug build · One Windows machine",
    description:
      "Recorded website diagnostic in Figma Desktop 126.10.11. Independent zero-based millisecond axes: fresh 38-node read, 10 requests, p50 108.29 ms and p95 120.64 ms; first 50-node page, 10 requests, p50 62.47 ms and p95 114.35 ms; 1440 by 4440 PNG export metadata, 3 requests, p50 1558.44 ms and p95 1581.64 ms. Two rich reads timed out and a pagination cursor repeated a page. All sections verified separately. Model costs unmeasured; not a responsive or interaction test.",
    lanes,
    percentiles: [50, 95],
    unit: "ms",
    notes: [
      "Each workload has its own zero-based scale (ms).",
      "Historical failures: two rich reads timed out at 20 s;",
      "a continuation cursor repeated a page.",
      "All five sections were verified separately.",
      "Static design; no responsive or interaction test.",
      "Model cost / tokens and client download: not measured.",
    ],
  });
}
const assets = {
  "architecture.svg": architecture(),
  "bridge-setup.svg": bridgeGuide(),
  "benchmark-desktop.svg": desktopBenchmark(),
  "benchmark-stages.svg": offlineBenchmark(),
  "benchmark-website-2026-10-21.svg": websiteBenchmark(),
};
mkdirSync(path.join(root, "assets"), { recursive: true });
for (const [name, content] of Object.entries(assets)) {
  const file = path.join(root, "assets", name);
  if (check) {
    if (readFileSync(file, "utf8") !== content) throw new Error(`Stale asset: assets/${name}`);
    console.log(`current: assets/${name}`);
  } else {
    writeFileSync(file, content);
    console.log(`wrote: assets/${name}`);
  }
}
