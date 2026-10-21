// Real local MCP stdio client. Every tools/call, including failures, is retained.
import { spawn, execFileSync } from "node:child_process";
import { readFileSync, writeFileSync, appendFileSync, mkdirSync } from "node:fs";
import { createHash } from "node:crypto";
import { createInterface } from "node:readline";
import os from "node:os";
import path from "node:path";

const specPath = process.argv[2];
if (!specPath) throw new Error("Usage: node scripts/validation/mcp-run.mjs SPEC.json");
const spec = JSON.parse(readFileSync(specPath, "utf8").replace(/^\uFEFF/, ""));
const binary = spec.binary ?? "D:/rust/kiln-target/debug/figtether.exe";
const output = spec.output ?? "benchmarks/results/desktop-2026-10-20";
mkdirSync(output, { recursive: true });
const hash = (file) => createHash("sha256").update(readFileSync(file)).digest("hex");
const git = (...args) =>
  execFileSync("git", ["-c", `safe.directory=${process.cwd().replaceAll("\\", "/")}`, ...args], {
    encoding: "utf8",
    stdio: ["ignore", "pipe", "ignore"],
  }).trim();
const build = {
  commit: git("rev-parse", "HEAD"),
  dirty: git("status", "--porcelain").length > 0,
  diff_sha256: createHash("sha256").update(git("diff")).digest("hex"),
  binary_sha256: hash(binary),
  plugin_sha256: hash("plugins/figma/dist/main.js"),
  ui_sha256: hash("plugins/figma/dist/ui.html"),
  version: "0.1.0",
  profile: binary.includes("/debug/") ? "debug" : "release",
  node: process.version,
  os: `${os.platform()} ${os.release()} ${os.arch()}`,
  cpu: os.cpus()[0].model,
  logical_cpus: os.cpus().length,
  ram_bytes: os.totalmem(),
  figma_desktop: spec.figma_desktop ?? "126.10.10",
};
const child = spawn(binary, ["mcp", "--stdio"], {
  stdio: ["pipe", "pipe", "pipe"],
  windowsHide: true,
});
const pending = new Map();
let nextId = 1;
let stderr = "";
child.stderr.on("data", (chunk) => {
  stderr += chunk;
});
const lines = createInterface({ input: child.stdout });
lines.on("line", (line) => {
  try {
    const message = JSON.parse(line);
    const waiter = pending.get(message.id);
    if (waiter) {
      pending.delete(message.id);
      clearTimeout(waiter.timer);
      waiter.resolve(message);
    }
  } catch {
    /* stdout protocol parse failure will be retained as a timeout */
  }
});
child.on("exit", (code) => {
  for (const waiter of pending.values()) {
    clearTimeout(waiter.timer);
    waiter.reject(new Error(`MCP exited ${code}: ${stderr.slice(-1000)}`));
  }
  pending.clear();
});
function rpc(method, params) {
  const id = nextId++;
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`${method} timeout after 45 seconds`));
    }, 45_000);
    pending.set(id, { resolve, reject, timer });
    child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
  });
}
const redact = (value) => {
  if (Array.isArray(value)) return value.map(redact);
  if (value && typeof value === "object")
    return Object.fromEntries(
      Object.entries(value).map(([k, v]) => [
        k,
        /token|credential|secret/i.test(k) ? "[REDACTED]" : redact(v),
      ]),
    );
  return value;
};
const records = [];
async function call(name, args = {}, lane = name, expectedNodes) {
  const started = new Date().toISOString();
  const before = performance.now();
  let response;
  try {
    response = await rpc("tools/call", { name, arguments: args });
  } catch (error) {
    response = { error: { message: error.message } };
  }
  const elapsed_ms = performance.now() - before;
  const result = response.result;
  let data = result?.structuredContent;
  if (!data) {
    try {
      data = JSON.parse(result?.content?.find((c) => c.type === "text")?.text ?? "null");
    } catch {}
  }
  const protocol_ok = !response.error && !result?.isError && !data?.is_error;
  const semantic_failure =
    data?.blocked === true ||
    data?.status === "partial" ||
    (name === "get_nodes" && args.node_ids?.length > 0 && data?.data?.nodes?.length === 0);
  const node_count_ok =
    expectedNodes === undefined || data?.coverage?.returned_nodes === expectedNodes;
  const ok = protocol_ok && !semantic_failure && node_count_ok;
  const record = {
    started,
    label: spec.label ?? path.basename(specPath),
    tool: name,
    lane,
    elapsed_ms,
    ok,
    protocol_ok,
    node_count_ok,
    expected_nodes: expectedNodes,
    response_bytes: Buffer.byteLength(JSON.stringify(response)),
    args: redact(args),
    data: redact(data),
    protocol_error: response.error ?? null,
    build,
    cost: {
      model_input_tokens: null,
      model_output_tokens: null,
      model_cost_usd: null,
      external_paid_api_calls_by_runner: 0,
      note: "This harness performs no model inference. Chat usage/billing is not exposed; cost is NOT_MEASURED.",
    },
  };
  records.push(record);
  appendFileSync(path.join(output, "calls.raw.jsonl"), JSON.stringify(record) + "\n");
  if (!spec.quiet)
    console.log(
      JSON.stringify({ tool: name, ok, elapsed_ms, data: redact(data), error: response.error }),
    );
  return { data, ok };
}
try {
  const init = await rpc("initialize", {
    protocolVersion: "2024-11-05",
    capabilities: {},
    clientInfo: { name: "figtether-desktop-validation", version: "1.0.0" },
  });
  if (init.error) throw new Error(JSON.stringify(init.error));
  child.stdin.write(JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" }) + "\n");
  for (const entry of spec.calls ?? []) {
    let consecutiveFailures = 0;
    for (let i = 0; i < (entry.repeat ?? 1); i++) {
      const answer = await call(
        entry.name,
        entry.arguments ?? {},
        entry.lane ?? entry.name,
        entry.expected_nodes,
      );
      consecutiveFailures = answer.ok ? 0 : consecutiveFailures + 1;
      if (consecutiveFailures >= 3)
        throw new Error(
          `Stopped ${entry.name} after three consecutive failures; ${i + 1} attempts retained.`,
        );
    }
  }
  if (spec.apply) {
    const { approval_id, plan, session_id } = spec.apply;
    const approval = await call("get_write_approval", { approval_id });
    const token = approval.data?.approval_token ?? approval.data?.data?.approval_token;
    if (!token || !approval.ok) throw new Error("Plan has no approved token; no apply attempted.");
    await call("apply_write", { session_id, plan, approval_token: token });
  }
} finally {
  child.stdin.end();
  child.kill();
  lines.close();
  const groups = Object.groupBy(records, (r) => r.lane);
  const summaries = Object.entries(groups).map(([lane, rows]) => {
    const sorted = rows.map((r) => r.elapsed_ms).sort((a, b) => a - b);
    const percentile = (q) => sorted[Math.max(0, Math.ceil(sorted.length * q) - 1)];
    return {
      lane,
      count: rows.length,
      successes: rows.filter((r) => r.ok).length,
      failures: rows.filter((r) => !r.ok).length,
      p50_ms: percentile(0.5),
      p95_ms: percentile(0.95),
      p99_ms: percentile(0.99),
      max_ms: sorted.at(-1),
    };
  });
  writeFileSync(
    path.join(output, `${spec.label ?? "latest"}.summary.json`),
    JSON.stringify(
      {
        build,
        summaries,
        exclusions: [
          "debug build unless specified",
          "not a release benchmark",
          "model usage/cost not exposed",
          "call elapsed includes MCP/daemon/transport/editor, not human approval wait",
        ],
      },
      null,
      2,
    ),
  );
  writeFileSync(
    path.join(output, "last-response.json"),
    JSON.stringify(records.at(-1)?.data ?? {}, null, 2),
  );
  console.log(JSON.stringify({ output, summaries }));
}
