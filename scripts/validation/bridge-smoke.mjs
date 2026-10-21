import { spawn, spawnSync } from "node:child_process";
import { createServer } from "node:net";
import { createInterface } from "node:readline";
import { existsSync, mkdirSync, writeFileSync, readFileSync } from "node:fs";
import { createHash } from "node:crypto";
import os from "node:os";
import { resolve } from "node:path";
// A real binary/stdio/socket smoke test with a simulated plugin, never Figma.
// Pairing material stays local. Every run requires a fresh output directory.
const args = process.argv.slice(2);
const argument = (name) => {
  const index = args.indexOf(name);
  return index < 0 ? undefined : args[index + 1];
};
const selectedBinary = argument("--binary");
if (!selectedBinary)
  throw new Error(
    "Usage: node scripts/validation/bridge-smoke.mjs --binary PATH [--out NEW_DIRECTORY]",
  );
const dir = resolve(
  argument("--out") ??
    `benchmarks/results/bridge-smoke-${new Date().toISOString().replace(/[:.]/g, "-")}`,
);
if (existsSync(dir))
  throw new Error(
    "Output directory already exists; choose a new directory to preserve previous evidence.",
  );
const runtime = dir + "/isolated-runtime";
mkdirSync(dir, { recursive: true });
const binary = resolve(selectedBinary);
if (!existsSync(binary)) throw new Error("Build the CLI first or select an existing --binary.");
const hash = (file) => createHash("sha256").update(readFileSync(file)).digest("hex");
const build = {
  started_at: new Date().toISOString(),
  node: process.version,
  os: `${os.platform()} ${os.release()} ${os.arch()}`,
  binary_sha256: hash(binary),
  harness_sha256: hash(new URL(import.meta.url)),
  plugin_sha256: existsSync("plugins/figma/dist/main.js")
    ? hash("plugins/figma/dist/main.js")
    : null,
  ui_sha256: existsSync("plugins/figma/dist/ui.html") ? hash("plugins/figma/dist/ui.html") : null,
};
const server = createServer();
await new Promise((r) => server.listen(0, "127.0.0.1", r));
const port = server.address().port;
await new Promise((r) => server.close(r));
const daemon = spawn(binary, ["daemon", "--port", String(port), "--runtime-dir", runtime], {
  windowsHide: true,
  stdio: ["ignore", "pipe", "pipe"],
});
let daemonLog = "";
let daemonError = null;
daemon.on("error", (error) => {
  daemonError = error;
});
daemon.stdout.on("data", (c) => (daemonLog += c));
daemon.stderr.on("data", (c) => (daemonLog += c));
let mcp, plugin;
const calls = [];
const pending = new Map();
let id = 0;
const redact = (v) =>
  Array.isArray(v)
    ? v.map(redact)
    : v && typeof v === "object"
      ? Object.fromEntries(
          Object.entries(v).map(([k, val]) => [
            k,
            /secret|credential|token/i.test(k) ? "[REDACTED]" : redact(val),
          ]),
        )
      : v;
const rpc = (method, params) =>
  new Promise((resolve, reject) => {
    const reqId = ++id;
    const start = performance.now();
    const timer = setTimeout(() => {
      pending.delete(reqId);
      reject(new Error(method + " timeout"));
    }, 10000);
    pending.set(reqId, {
      resolve: (answer) => {
        clearTimeout(timer);
        calls.push({ method, elapsed_ms: performance.now() - start, answer: redact(answer) });
        resolve(answer);
      },
    });
    mcp.stdin.write(JSON.stringify({ jsonrpc: "2.0", id: reqId, method, params }) + "\n");
  });
const assert = (v, message) => {
  if (!v) throw new Error(message);
};
let summary;
try {
  const deadline = Date.now() + 10000;
  while (!existsSync(runtime + "/daemon.json")) {
    if (daemonError) throw daemonError;
    if (Date.now() > deadline) throw new Error("isolated daemon did not start");
    await new Promise((r) => setTimeout(r, 50));
  }
  const paired = spawnSync(binary, ["pair", "--json", "--runtime-dir", runtime], {
    encoding: "utf8",
    windowsHide: true,
  });
  assert(paired.status === 0, "pair failed");
  const pair = JSON.parse(paired.stdout);
  assert(
    JSON.stringify(Object.keys(pair).sort()) === JSON.stringify(["expires_in_seconds", "secret"]),
    "pair output allowlist",
  );
  assert(
    typeof pair.secret === "string" && pair.secret.length > 0 && pair.expires_in_seconds === 300,
    "pair value types/expiry",
  );
  plugin = new WebSocket(`ws://127.0.0.1:${port}/bridge`);
  const connected = new Promise((resolve, reject) => {
    plugin.onopen = resolve;
    plugin.onerror = reject;
  });
  let auth;
  let grant;
  plugin.onmessage = (e) => {
    const f = JSON.parse(e.data);
    if (f.request_id === "auth" && f.message_type === "result") auth = f.payload;
    if (f.message_type === "grant_update") grant = f.payload;
    if (f.message_type === "heartbeat")
      plugin.send(
        JSON.stringify({
          protocol_version: "1.0",
          message_type: "heartbeat",
          request_id: "heartbeat",
          payload: { sequence: 1 },
          connection_epoch: auth?.connection_epoch,
        }),
      );
  };
  await connected;
  plugin.send(
    JSON.stringify({
      protocol_version: "1.0",
      message_type: "hello_auth",
      request_id: "auth",
      payload: {
        secret: pair.secret,
        client_id: "isolated-integration-plugin",
        protocol_version: "1.0",
      },
    }),
  );
  pair.secret = "";
  const wait = async (fn) => {
    const until = Date.now() + 5000;
    while (!fn()) {
      if (Date.now() > until) throw new Error("plugin state timeout");
      await new Promise((r) => setTimeout(r, 20));
    }
  };
  await wait(() => auth);
  plugin.send(
    JSON.stringify({
      protocol_version: "1.0",
      message_type: "grant_update",
      request_id: "grant",
      connection_epoch: auth.connection_epoch,
      payload: {
        root_ids: ["synthetic:root"],
        grant_epoch: 0,
        mode: "write",
        session_mode: "live",
      },
    }),
  );
  await wait(() => grant);
  mcp = spawn(binary, ["mcp", "--stdio", "--runtime-dir", runtime], {
    windowsHide: true,
    stdio: ["pipe", "pipe", "pipe"],
  });
  mcp.stderr.resume();
  mcp.on("error", () => {}); // Missing replies are bounded by the RPC deadline.
  const lines = createInterface({ input: mcp.stdout });
  lines.on("line", (l) => {
    try {
      const a = JSON.parse(l);
      const p = pending.get(a.id);
      if (p) {
        pending.delete(a.id);
        p.resolve(a);
      }
    } catch {}
  });
  const init = await rpc("initialize", {
    protocolVersion: "2024-11-05",
    capabilities: {},
    clientInfo: { name: "isolated-integration-probe", version: "1" },
  });
  assert(
    init.result?.instructions.includes("Figma tools first"),
    "missing canvas-first instructions",
  );
  assert(init.result.instructions.includes("granted_root_ids"), "missing discovery instructions");
  assert(init.result.instructions.includes("font_family"), "missing inline styling instructions");
  mcp.stdin.write(JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" }) + "\n");
  const listed = await rpc("tools/list", {});
  const names = listed.result.tools.map((t) => t.name);
  for (const name of ["preview_write", "get_write_approval", "apply_write"])
    assert(names.includes(name), "missing " + name);
  const sessions = await rpc("tools/call", { name: "list_sessions", arguments: {} });
  const envelope =
    sessions.result.structuredContent ??
    JSON.parse(sessions.result.content.find((c) => c.type === "text").text);
  const session = (envelope.data ?? envelope.envelope?.data).sessions.find(
    (s) => s.session_id === auth.session_id,
  );
  assert(
    JSON.stringify(session.granted_root_ids) === JSON.stringify(["synthetic:root"]),
    "granted roots not discoverable",
  );
  assert(session.grant_mode === "write", "grant mode not discoverable");
  summary = {
    ok: true,
    build,
    kind: "isolated-real-binary-and-MCP-with-simulated-plugin",
    pair_json: {
      keys: ["secret", "expires_in_seconds"],
      expires_in_seconds: 300,
      secret_retained: false,
    },
    automatic_instructions: true,
    write_tools: names.filter((n) => n.includes("write")),
    discoverable_granted_roots: session.granted_root_ids,
    grant_mode: session.grant_mode,
    calls,
    cost: { tokens: null, billed_cost: null, status: "unmeasured" },
    native_figma: false,
  };
  console.log(
    JSON.stringify({
      ok: true,
      automatic_instructions: true,
      pair_json: true,
      discovery: true,
      tools: names.length,
      calls: calls.map((c) => ({ method: c.method, elapsed_ms: c.elapsed_ms })),
    }),
  );
} catch (error) {
  summary = { ok: false, build, error: String(error), calls, native_figma: false };
  throw error;
} finally {
  writeFileSync(dir + "/isolated-mcp.json", JSON.stringify(summary, null, 2) + "\n");
  plugin?.close();
  mcp?.kill();
  daemon.kill();
  writeFileSync(dir + "/isolated-daemon.log", daemonLog);
}
