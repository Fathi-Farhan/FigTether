// Original README artwork. No remote services, fonts, or third-party artwork.
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const check = process.argv.includes("--check");
const escape = (value) =>
  String(value).replace(
    /[&<>"']/g,
    (c) =>
      ({
        "&": "&amp;",
        "<": "&lt;",
        ">": "&gt;",
        '"': "&quot;",
        "'": "&apos;",
      })[c],
  );
const text = (x, y, value, extra = "") =>
  `<text x="${x}" y="${y}" ${extra}>${escape(value)}</text>`;
const wrap = (
  height,
  title,
  description,
  content,
) => `<svg xmlns="http://www.w3.org/2000/svg" width="1100" height="${height}" viewBox="0 0 1100 ${height}" role="img" aria-labelledby="title desc">
<title id="title">${escape(title)}</title>
<desc id="desc">${escape(description)}</desc>
<style>text{font-family:system-ui,-apple-system,Segoe UI,sans-serif;fill:#172b3a;font-size:16px}.small{font-size:13px;fill:#526575}.title{font-size:26px;font-weight:700}.label{font-size:17px;font-weight:600}</style>
<rect width="1100" height="${height}" rx="18" fill="#f5f8fb"/>
${content}
</svg>
`;

function architecture() {
  const boxes = [
    [32, "MCP client", "Your coding agent"],
    [248, "Rust adapter", "MCP over stdio"],
    [464, "Rust daemon", "Queue / cache / scope"],
    [680, "Plugin UI", "Local WebSocket"],
    [896, "Sandbox", "Figma Plugin API"],
  ];
  const nodes = boxes
    .map(
      ([
        x,
        title,
        caption,
      ]) => `<rect x="${x}" y="122" width="172" height="104" rx="12" fill="white" stroke="#bacbd9"/>
${text(x + 16, 158, title, 'class="label"')}
${text(x + 16, 186, caption, 'class="small"')}`,
    )
    .join("\n");
  const links = boxes
    .slice(0, -1)
    .map(
      ([x]) =>
        `<path d="M ${x + 178} 174 H ${x + 202}" stroke="#207f8c" stroke-width="2.5" marker-end="url(#arrow)"/>`,
    )
    .join("\n");
  return wrap(
    320,
    "FigTether: local acquisition, explicit access",
    "An MCP client reaches a Rust stdio adapter, loopback daemon, plugin UI, and Figma sandbox. The user grants selected roots. Only the sandbox reads the document; the AI client decides where results go.",
    `
<defs><marker id="arrow" markerWidth="6" markerHeight="6" refX="5" refY="3" orient="auto"><path d="M 0 0 L 6 3 L 0 6" fill="#207f8c"/></marker></defs>
${text(32, 48, "A local bridge, not a cloud relay", 'class="title"')}
${text(32, 79, "Read requests travel right. Design context returns left.", 'class="small"')}
<rect x="664" y="101" width="420" height="146" rx="16" fill="#e5f1ee"/>
${nodes}
${links}
${text(700, 270, "Figma Desktop: user-granted roots", 'class="small"')}
${text(32, 293, "Local acquisition does not imply local inference: your MCP client controls model access.", 'class="small"')}`,
  );
}

function benchmark() {
  const data = JSON.parse(
    readFileSync(path.join(root, "benchmarks/published/offline-stages.json"), "utf8"),
  );
  if (data.kind !== "offline-diagnostic" || data.unit !== "ms" || data.stages.length !== 4) {
    throw new Error("Unexpected chart data: expected four offline stages in milliseconds.");
  }
  for (const stage of data.stages) {
    if (
      ![stage.p50, stage.p95, stage.p99].every((v) => Number.isFinite(v) && v >= 0) ||
      stage.p95 < stage.p50 ||
      stage.p99 < stage.p95
    ) {
      throw new Error(`Invalid percentile data: ${stage.key}`);
    }
  }
  const left = 290;
  const plot = 640;
  const maxUs = Math.max(
    200,
    Math.ceil(Math.max(...data.stages.map((s) => s.p95 * 1000)) / 50) * 50,
  );
  const grid = Array.from({ length: 5 }, (_, i) => {
    const value = (maxUs * i) / 4;
    const x = left + (plot * i) / 4;
    return `<path d="M ${x} 112 V 435" stroke="#d6e0e8"/>${text(x, 460, value.toFixed(0), 'class="small" text-anchor="middle"')}`;
  }).join("\n");
  const bars = data.stages
    .map((stage, i) => {
      const y = 140 + i * 78;
      const p50 = stage.p50 * 1000;
      const p95 = stage.p95 * 1000;
      return `${text(32, y + 14, stage.label, 'class="label"')}
<rect x="${left}" y="${y - 8}" width="${((p50 / maxUs) * plot).toFixed(2)}" height="17" rx="3" fill="#207f8c"/>
<rect x="${left}" y="${y + 16}" width="${((p95 / maxUs) * plot).toFixed(2)}" height="17" rx="3" fill="#6475ce"/>
${text(left + (p50 / maxUs) * plot + 9, y + 6, p50.toFixed(1), 'class="small"')}
${text(left + (p95 / maxUs) * plot + 9, y + 30, p95.toFixed(1), 'class="small"')}`;
    })
    .join("\n");
  return wrap(
    555,
    "Offline Rust stage diagnostic",
    `One diagnostic run at commit ${data.source_commit.slice(0, 7)}, ${data.samples_per_stage} samples per stage. Bars show p50 and p95 in microseconds, on a linear zero-based axis. Figma work, network latency, and model inference are not measured.`,
    `
${text(32, 45, "Where the offline core spends time", 'class="title"')}
${text(32, 76, `${data.samples_per_stage.toLocaleString("en-US")} samples / stage | Windows x64 | ${data.environment.cpu} | ${data.source_commit.slice(0, 7)}`, 'class="small"')}
<rect x="790" y="29" width="14" height="14" rx="2" fill="#207f8c"/>${text(812, 41, "p50", 'class="small"')}
<rect x="880" y="29" width="14" height="14" rx="2" fill="#6475ce"/>${text(902, 41, "p95", 'class="small"')}
${grid}
${bars}
${text(610, 486, "Microseconds (lower is less time)", 'class="small" text-anchor="middle"')}
${text(32, 516, "Independent stage operations, not an end-to-end request. One run; no confidence interval.", 'class="small"')}
${text(32, 539, "Figma / network / model: NOT MEASURED. This is not a release target or a competitor comparison.", 'class="small"')}`,
  );
}

const assets = { "architecture.svg": architecture(), "benchmark-stages.svg": benchmark() };
mkdirSync(path.join(root, "assets"), { recursive: true });
for (const [name, svg] of Object.entries(assets)) {
  const file = path.join(root, "assets", name);
  if (check) {
    if (readFileSync(file, "utf8") !== svg) throw new Error(`Stale asset: assets/${name}`);
    console.log(`current: assets/${name}`);
  } else {
    writeFileSync(file, svg);
    console.log(`wrote: assets/${name}`);
  }
}
