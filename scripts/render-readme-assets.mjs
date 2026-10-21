// Original, deterministic technical figures. See assets/DESIGN.md for the design system.
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const check = process.argv.includes("--check");
const C = {
  ink: "#111111",
  muted: "#525252",
  rule: "#d4d4d4",
  light: "#f1f1f1",
  paper: "#ffffff",
  warning: "#9a4b00",
};
const esc = (v) =>
  String(v).replace(
    /[&<>"']/g,
    (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&apos;" })[c],
  );
const t = (x, y, value, cls = "body", extra = "") =>
  `<text x="${x}" y="${y}" class="${cls}" ${extra}>${esc(value)}</text>`;
const line = (x1, y1, x2, y2, color = C.rule, extra = "") =>
  `<path d="M ${x1} ${y1} L ${x2} ${y2}" stroke="${color}" ${extra}/>`;
const rule = (y) => line(40, y, 1060, y);
const box = (x, y, w, h, fill = C.paper) =>
  `<rect x="${x}" y="${y}" width="${w}" height="${h}" fill="${fill}" stroke="${C.ink}"/>`;
const arrow = (x1, y, x2) =>
  `<path d="M ${x1} ${y} H ${x2}" fill="none" stroke="${C.ink}" stroke-width="1.5" marker-end="url(#arrow)"/>`;
const header = (number, section, title, subtitle) =>
  `${t(40, 40, `${number} / FIGTETHER - ${section}`, "meta")}${rule(58)}${t(40, 106, title, "heading")}${t(40, 138, subtitle, "small")}`;
function svg(height, title, desc, body) {
  return `<svg xmlns="http://www.w3.org/2000/svg" width="1100" height="${height}" viewBox="0 0 1100 ${height}" role="img" aria-labelledby="title desc">
<title id="title">${esc(title)}</title><desc id="desc">${esc(desc)}</desc>
<style>text{fill:${C.ink};font-family:Arial,Helvetica,sans-serif;font-size:16px}.heading{font-family:Gelasio,Georgia,serif;font-size:36px;font-weight:400;letter-spacing:-.6px}.subhead{font-family:Gelasio,Georgia,serif;font-size:24px}.label{font-size:18px;font-weight:600}.body{font-size:16px}.small{font-size:14px;fill:${C.muted}}.meta{font-family:"Ubuntu Mono",Consolas,monospace;font-size:13px;letter-spacing:.5px}.code{font-family:"Ubuntu Mono",Consolas,monospace;font-size:19px}.value{font-family:Consolas,monospace;font-size:16px}.warn{fill:${C.warning};font-size:14px}</style>
<rect width="1100" height="${height}" fill="${C.paper}"/>
<defs><marker id="arrow" markerWidth="7" markerHeight="7" refX="6" refY="3.5" orient="auto"><path d="M 0 0 L 7 3.5 L 0 7" fill="${C.ink}"/></marker></defs>
${body}
</svg>\n`;
}

function architecture() {
  const xs = [40, 310, 580, 850];
  const nodes = [
    ["Coding agent", "Your MCP client", "Reads + design plans"],
    ["Rust adapter", "figtether mcp --stdio", "Typed tool requests"],
    ["Local daemon", "Scope · queue · cache", "127.0.0.1:38947"],
    ["Figma plugin", "UI ↔ sandbox", "Native Plugin API"],
  ]
    .map(
      ([name, detail, caption], i) =>
        `${box(xs[i], 198, 210, 124)}${t(xs[i] + 16, 232, name, "label")}${t(xs[i] + 16, 267, detail, "small")}${t(xs[i] + 16, 299, caption, "small")}`,
    )
    .join("\n");
  return svg(
    560,
    "FigTether system architecture",
    "The coding agent calls FigTether's own MCP server over stdio. The Rust adapter reaches the daemon through an authenticated loopback WebSocket; the daemon reaches the Figma plugin through another local WebSocket. The sandbox reads and mutates native nodes. Writes require a user-granted root, a separate write grant, preview and approval for the exact plan.",
    `
${header("01", "SYSTEM", "Read, create, and edit in Figma.", "One local bridge. Native nodes. Explicit access and approval.")}
${nodes}
${t(280, 181, "stdio", "meta", 'text-anchor="middle"')}
${t(550, 181, "loopback WS", "meta", 'text-anchor="middle"')}
${t(820, 181, "local WS", "meta", 'text-anchor="middle"')}
${arrow(256, 260, 301)}${arrow(526, 260, 571)}${arrow(796, 260, 841)}
<path d="M 955 330 V 359 H 145 V 331" stroke="${C.ink}" fill="none" stroke-width="1.5" marker-end="url(#arrow)"/>
<rect x="330" y="347" width="440" height="26" fill="white"/>
${t(550, 365, "Node data, asset metadata, and write results", "small", 'text-anchor="middle"')}
${rule(399)}
${t(40, 433, "WRITE PROTOCOL", "meta")}
${t(40, 470, "preview_write", "code")}${arrow(222, 464, 294)}
${t(310, 470, "User approval", "label")}${arrow(480, 464, 564)}
${t(580, 470, "get_write_approval", "code")}${arrow(797, 464, 834)}
${t(850, 470, "apply_write", "code")}
${t(40, 512, "Requires a granted root + write permission. Rust checks the exact plan before the plugin mutates the document.", "small")}
${t(40, 540, "The agent chooses the design; the plugin executes supported operations. The client controls model inference.", "small")}
`,
  );
}

function bridgeGuide() {
  return svg(
    680,
    "FigTether Bridge pairing guide",
    "Start figtether in terminal one. Run figtether pair in terminal two. Copy its one-time, five-minute secret into the plugin and press Pair. Pairing is separate from granting design access. Select and refresh a frame, check the root, and allow a read session. Enable a write session only when desired; approve each exact plan.",
    `
${header("03", "BRIDGE GUIDE", "The pairing secret comes from your terminal.", "Run the daemon locally, connect the plugin, then choose what the agent can access.")}
${t(40, 195, "ON YOUR COMPUTER", "meta")}${t(640, 195, "IN THE FIGMA PLUGIN", "meta")}
${t(40, 235, "Terminal 1 - keep it running", "small")}
${t(40, 270, "figtether", "code")}
${t(40, 321, "Terminal 2 - request a secret", "small")}
${t(40, 356, "figtether pair", "code")}
${t(40, 402, "[one-time secret printed here]", "value")}
${t(640, 235, "Pairing secret", "label")}
${box(640, 254, 420, 46)}${t(658, 285, "••••••••••••••••••••••••", "value")}
${box(640, 318, 76, 42, C.light)}${t(678, 345, "Pair", "body", 'text-anchor="middle"')}
${t(740, 344, "Paste the secret, then press Pair.", "small")}
<path d="M 430 397 H 563 V 277 H 630" fill="none" stroke="${C.ink}" stroke-width="1.5" marker-end="url(#arrow)"/>
${t(521, 425, "copy", "meta", 'text-anchor="middle"')}
${t(640, 401, "One use. Expires after 5 minutes.", "small")}
${t(640, 430, "A new secret replaces the previous pending secret.", "small")}
${rule(466)}
${t(40, 504, "NEXT: GRANT DESIGN ACCESS", "meta")}
${t(40, 544, "Select a frame", "label")}${arrow(189, 538, 236)}
${t(254, 544, "Refresh + check", "label")}${arrow(422, 538, 474)}
${t(492, 544, "Allow read session", "label")}${arrow(692, 538, 744)}
${t(762, 544, "Agent can read", "label")}
${t(40, 583, "To create or edit: enable Allow write session, review the preview, and approve each plan.", "body")}
${rule(611)}
${t(40, 643, "Pairing connects the bridge; it does not grant design access. Never paste the secret into chat or MCP config.", "small")}
${t(40, 667, "Illustrated walkthrough; controls simplified. See the Bridge UI guide for the actual screenshot and recovery steps.", "small")}
`,
  );
}

function desktopBenchmark() {
  const source = "benchmarks/published/desktop-2026-10-20.json";
  const data = JSON.parse(readFileSync(path.join(root, source), "utf8"));
  const build = data.builds.filter((b) => b.profile === "release").at(-1);
  if (!build) throw new Error("Desktop figure needs a measured release build");
  const specs = [
    ["W1-cached-root", "Cached-root request", 1, 1.2, "1 node / response"],
    ["W2-fresh-72-nodes", "Fresh design read", 72, 200, "72 nodes / response"],
    ["W3-website-png-1440x1640", "Website PNG export", 1, 800, "1440 × 1640 px"],
  ];
  const lanes = specs.map(([name, title, nodes, max, scope]) => {
    const lane = data.lanes.find((l) => l.build === build.id && l.lane === name);
    if (
      !lane ||
      lane.returned_nodes_min !== nodes ||
      lane.returned_nodes_max !== nodes ||
      lane.success !== lane.samples ||
      lane.failed !== 0 ||
      ![lane.p50_ms, lane.p95_ms, lane.p99_ms].every(Number.isFinite) ||
      lane.p50_ms < 0 ||
      lane.p50_ms > lane.p95_ms ||
      lane.p95_ms > lane.p99_ms ||
      lane.p99_ms > max
    ) {
      throw new Error(`Desktop figure scope or percentile mismatch: ${name}`);
    }
    return { ...lane, title, max, scope };
  });
  const count = (n) => n.toLocaleString("en-US");
  const rows = lanes
    .map((lane, i) => {
      const y = 195 + i * 137,
        x = 366,
        plot = 420;
      const bars = [50, 95, 99]
        .map((p, j) => {
          const val = lane[`p${p}_ms`],
            yy = y + j * 22;
          const w = (plot * val) / lane.max;
          return p === 99
            ? `<path d="M ${x} ${yy + 6} H ${x + w}" stroke="${C.ink}" stroke-width="2" stroke-dasharray="4 3"/>`
            : `<rect x="${x}" y="${yy}" width="${w}" height="12" fill="${p === 50 ? "#8a8a8a" : C.ink}"/>`;
        })
        .join("\n");
      return `
${t(40, y + 20, lane.title, "subhead")}
${t(40, y + 50, `${count(lane.samples)} requests`, "small")}
${t(40, y + 76, lane.scope, "small")}
${bars}
${line(x, y + 72, x + plot, y + 72)}
${[0, 0.5, 1].map((f) => t(x + plot * f, y + 96, String(lane.max * f), "meta", 'text-anchor="middle"')).join("")}
${[50, 95, 99].map((p, j) => t([870, 960, 1050][j], y + 42, lane[`p${p}_ms`].toFixed(2), "value", 'text-anchor="end"')).join("")}
${rule(y + 116)}`;
    })
    .join("\n");
  const cache = lanes[0];
  return svg(
    850,
    "FigTether desktop request latency",
    "Real Figma Desktop measurements on one Windows machine. The cached-root lane includes 20,000 confirmed hits and 10 fresh fallbacks. Fresh reads return 72 nodes. PNG render/export metadata uses a 1440 by 1640 fixture; client download delivery was not validated. Independent linear axes start at zero. Model cost and tokens were not measured. Known get_selection timeout and inspect_design protocol mismatch remain.",
    `
${header("02", "MEASUREMENTS", "Latency, with the workload attached.", `Figma ${build.figma_desktop} / Windows x64 / Ryzen 5 4500 / 20–21 Oct 2026 / one machine`)}
${t(40, 174, "WORKLOAD / SCOPE", "meta")}
<rect x="366" y="161" width="20" height="10" fill="#8a8a8a"/>${t(394, 171, "p50", "small")}
<rect x="454" y="161" width="20" height="10" fill="${C.ink}"/>${t(482, 171, "p95", "small")}
<path d="M 542 166 H 562" stroke="${C.ink}" stroke-width="2" stroke-dasharray="4 3"/>${t(570, 171, "p99", "small")}
${t(870, 174, "p50 ms", "meta", 'text-anchor="end"')}${t(960, 174, "p95 ms", "meta", 'text-anchor="end"')}${t(1050, 174, "p99 ms", "meta", 'text-anchor="end"')}
${rows}
${t(366, 620, "Milliseconds · each row has its own zero-based linear scale", "small")}
${t(40, 657, `Cache: ${count(cache.freshness_counts.cache_hit)} confirmed hits + ${cache.freshness_counts.fresh_read} fresh fallbacks. All plotted requests succeeded.`, "small")}
${t(40, 684, "PNG: render / transport / metadata; completed client download was not validated.", "small")}
${rule(707)}
${t(40, 739, `Daemon peak working set: ${(data.resources.peak_working_set_bytes / 2 ** 20).toFixed(2)} MiB (${data.resources.samples} samples across builds).`, "body")}
${t(40, 767, `Model cost + tokens: ${data.costs.status.replaceAll("_", " ").toLowerCase()}.`, "warn")}
${t(600, 739, "Known failures: get_selection timeout;", "warn")}
${t(600, 767, "inspect_design protocol mismatch.", "warn")}
${t(40, 806, `v${build.version} / dirty source build / binary ${build.binary_sha256.slice(0, 12)} / plugin ${build.plugin_sha256.slice(0, 12)}`, "meta")}
${t(40, 832, "Source: desktop-2026-10-20.json. Nearest-rank percentiles. Limited compatibility evidence; no release certification.", "small")}
`,
  );
}

function offlineBenchmark() {
  const d = JSON.parse(
    readFileSync(path.join(root, "benchmarks/published/offline-stages.json"), "utf8"),
  );
  if (d.kind !== "offline-diagnostic" || d.unit !== "ms" || d.stages.length !== 4)
    throw new Error("Unexpected offline source");
  for (const s of d.stages)
    if (
      ![s.p50, s.p95, s.p99].every((v) => Number.isFinite(v) && v >= 0) ||
      s.p50 > s.p95 ||
      s.p95 > s.p99
    )
      throw new Error(`Invalid stage: ${s.key}`);
  const max = Math.max(200, Math.ceil(Math.max(...d.stages.map((s) => s.p95 * 1000)) / 50) * 50);
  const x = 330,
    plot = 590;
  const grid = [0, 0.25, 0.5, 0.75, 1]
    .map(
      (f) =>
        `<path d="M ${x + plot * f} 184 V 478" stroke="${C.rule}"/>${t(x + plot * f, 501, (max * f).toFixed(0), "meta", 'text-anchor="middle"')}`,
    )
    .join("");
  const rows = d.stages
    .map((s, i) => {
      const y = 206 + i * 71;
      return `${t(40, y + 19, s.label, "label")}${[50, 95].map((p, j) => `<rect x="${x}" y="${y + j * 21}" width="${((s[`p${p}`] * 1000) / max) * plot}" height="13" fill="${p === 50 ? "#8a8a8a" : C.ink}"/>${t(x + ((s[`p${p}`] * 1000) / max) * plot + 8, y + j * 21 + 12, (s[`p${p}`] * 1000).toFixed(1), "small")}`).join("")}`;
    })
    .join("");
  return svg(
    600,
    "Offline Rust stage diagnostic",
    "A separate offline synthetic Rust diagnostic. The zero-based linear axis is in microseconds. Bars show p50 and p95; Figma, network, and model inference are not measured. One run, no confidence interval.",
    `
${header("04", "OFFLINE DIAGNOSTIC", "Where the Rust core spends time.", `${d.samples_per_stage.toLocaleString("en-US")} samples / stage · ${d.environment.cpu} · commit ${d.source_commit.slice(0, 7)}`)}
<rect x="780" y="159" width="20" height="12" fill="#8a8a8a"/>${t(810, 170, "p50", "small")}
<rect x="880" y="159" width="20" height="12" fill="${C.ink}"/>${t(910, 170, "p95", "small")}
${grid}${rows}
${t(625, 531, "Microseconds · linear scale · lower is less time", "small", 'text-anchor="middle"')}
${rule(550)}${t(40, 576, "Independent stage operations, not an end-to-end call. One run; no confidence interval.", "small")}
${t(40, 596, "Figma, network, and model inference: not measured. Do not compare directly with desktop latency.", "small")}
`,
  );
}
const assets = {
  "architecture.svg": architecture(),
  "bridge-setup.svg": bridgeGuide(),
  "benchmark-desktop.svg": desktopBenchmark(),
  "benchmark-stages.svg": offlineBenchmark(),
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
