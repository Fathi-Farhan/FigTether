// Authenticated synthetic editor, for protocol and real-host evaluations only.
// No native Figma connection or real design data. Every caller owns its isolated daemon.
import { spawn, spawnSync } from "node:child_process";
import { createServer } from "node:net";
import { createInterface } from "node:readline";
import { mkdirSync, existsSync, writeFileSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { createHash } from "node:crypto";
import { browser } from "./bridge-ux/browser.mjs";
const hash = (bytes) => "sha256:" + createHash("sha256").update(bytes).digest("hex");
export const delay = (ms) => new Promise((r) => setTimeout(r, ms));
export async function until(fn, timeout = 10000) {
  const end = Date.now() + timeout;
  while (!(await fn())) {
    if (Date.now() > end) throw Error("fixture condition timed out");
    await delay(25);
  }
}
const escape = (s) =>
  String(s).replace(
    /[&<>"']/g,
    (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c],
  );

export async function fixture({ binary, out, mode = "write", warnings = [], twoFrames = true }) {
  const directory = resolve(out);
  if (existsSync(directory)) throw Error("choose a new output directory");
  mkdirSync(directory, { recursive: true });
  const runtime = resolve(directory, "runtime");
  const listener = createServer();
  await new Promise((r) => listener.listen(0, "127.0.0.1", r));
  const port = listener.address().port;
  await new Promise((r) => listener.close(r));
  const daemon = spawn(binary, ["daemon", "--port", String(port), "--runtime-dir", runtime], {
    windowsHide: true,
  });
  let daemonLog = "";
  daemon.stdout.on("data", (c) => (daemonLog += c));
  daemon.stderr.on("data", (c) => (daemonLog += c));
  let plugin,
    auth,
    grant,
    b,
    seq = 10;
  const events = [],
    nodes = new Map();
  let nextReadFailure = null;
  const add = (id, name, parent_id = null, node_type = "FRAME", properties = {}) =>
    nodes.set(id, { id, name, parent_id, node_type, children: [], properties });
  add("1:2", "Website Test");
  if (twoFrames) add("1:3", "Other Agent");
  const wire = (node) => ({
    id: node.id,
    name: node.name,
    parent_id: node.parent_id,
    node_type: node.node_type,
    children: [...nodes.values()].filter((n) => n.parent_id === node.id).map((n) => n.id),
    geometry: {
      local_transform: [1, 0, 0, 1],
      absolute_transform: [1, 0, 0, 1],
      bounding_box: [
        node.properties.x ?? 0,
        node.properties.y ?? 0,
        node.properties.width ?? 720,
        node.properties.height ?? 480,
      ],
      rotation: 0,
    },
    ...(node.text ? { text: { text: node.text, runs: [] } } : {}),
    fills: [],
  });
  const send = (type, payload, id = "fixture") =>
    plugin.send(
      JSON.stringify({
        protocol_version: "1.0",
        message_type: type,
        request_id: id,
        session_id: auth?.session_id,
        connection_epoch: auth?.connection_epoch,
        payload: type === "result" ? { data: payload } : payload,
      }),
    );
  async function render(root) {
    b ??= await browser();
    const descendants = [...nodes.values()].filter((n) => n.id !== root && n.id !== "1:3");
    const html = `<html><body style="margin:0;background:#f7f5ef;font-family:Arial;color:#192c28"><main style="width:720px;height:480px;position:relative;overflow:hidden">${descendants.map((n) => `<div style="position:absolute;left:${Number(n.properties.x) || 0}px;top:${Number(n.properties.y) || 0}px;width:${Number(n.properties.width) || 300}px;height:${Number(n.properties.height) || 48}px;font-size:${Number(n.properties.fontSize) || 20}px;${n.node_type !== "TEXT" ? "background:#dce5df;" : ""}">${escape(n.text ?? n.name)}</div>`).join("")}</main></body></html>`;
    await b.send("Page.navigate", { url: "data:text/html," + encodeURIComponent(html) });
    const target = resolve(directory, "render.png");
    await b.capture(target, 720, 480);
    return readFileSync(target);
  }
  try {
    await until(() => existsSync(resolve(runtime, "daemon.json")));
    const pair = spawnSync(binary, ["pair", "--json", "--runtime-dir", runtime], {
      encoding: "utf8",
      windowsHide: true,
    });
    if (pair.status !== 0) throw Error("fixture pair failed");
    let secret = JSON.parse(pair.stdout).secret;
    plugin = new WebSocket(`ws://127.0.0.1:${port}/bridge`);
    await new Promise((r, j) => {
      plugin.onopen = r;
      plugin.onerror = j;
    });
    let chain = Promise.resolve();
    plugin.onmessage = (event) => {
      const f = JSON.parse(event.data);
      if (f.message_type === "result" && f.request_id === "auth") {
        auth = f.payload;
        return;
      }
      if (f.message_type === "grant_update") {
        grant = f.payload;
        return;
      }
      if (f.message_type === "heartbeat") {
        send("heartbeat", { sequence: 1 });
        return;
      }
      if (f.message_type !== "request") return;
      chain = chain
        .then(async () => {
          const { op, params: p = {} } = f.payload;
          events.push({ op, at: Date.now(), count: p.plan?.operations?.length ?? null });
          let result;
          if (op === "read_nodes" || op === "list_nodes") {
            if (nextReadFailure) {
              const code = nextReadFailure;
              nextReadFailure = null;
              send(
                "error",
                {
                  code,
                  message: "Synthetic read failure",
                  retryable: true,
                  action:
                    "Retry a smaller read; do not change sharing to repair a transport error.",
                },
                f.request_id,
              );
              return;
            }
            const ids =
              op === "read_nodes"
                ? p.node_ids
                : [...nodes.values()]
                    .filter((n) => n.id === p.root_id || n.parent_id === p.root_id)
                    .map((n) => n.id);
            const offset = p.cursor ? Number(p.cursor) : 0;
            const limit = p.limit ?? 100;
            const picked = ids
              .slice(offset, offset + limit)
              .map((id) => nodes.get(id))
              .filter(Boolean);
            const more = offset + limit < ids.length;
            result = {
              nodes: picked.map(wire),
              truncated: more,
              next_cursor: more ? String(offset + limit) : null,
            };
          } else if (op === "export_assets") {
            const bytes = await render(p.node_ids[0]);
            result = {
              node_id: p.node_ids[0],
              mime: "image/png",
              width: 720,
              height: 480,
              bytes: bytes.toString("base64"),
              hash: hash(bytes),
            };
          } else if (op === "preview_write") {
            const ops = p.plan.operations;
            result = {
              preview: {
                plan_id: p.plan.plan_id,
                plan_hash: p.plan_hash,
                blocked: false,
                warnings,
                removed: ops.filter((o) => o.kind === "delete_node").map((o) => o.node_id),
                created: [],
                changed: ops.filter((o) => o.node_id).map((o) => o.node_id),
                moved: [],
                operations_count: ops.length,
              },
            };
            send("result", result, f.request_id);
            // A simulated test user opted into clean-plan automation; never a real approval.
            if (!warnings.length && !result.preview.removed.length)
              setTimeout(
                () =>
                  send("approval", {
                    approval_id: p.approval_id,
                    plan_hash: p.plan_hash,
                    approved: true,
                    reason: null,
                    automatic: true,
                  }),
                20,
              );
            return;
          } else if (op === "apply_write") {
            const created = [],
              changed = [];
            for (const o of p.plan.operations) {
              if (o.kind === "create_node") {
                const id = `2:${++seq}`;
                add(
                  id,
                  o.properties?.name ?? o.node_type,
                  o.parent_id,
                  o.node_type,
                  o.properties ?? {},
                );
                const n = nodes.get(id);
                n.text = o.text;
                created.push(id);
              } else if (nodes.has(o.node_id)) {
                const n = nodes.get(o.node_id);
                if (o.kind === "set_text") n.text = o.text;
                if (o.kind === "set_properties") {
                  Object.assign(n.properties, o.properties);
                  if (o.properties.name) n.name = o.properties.name;
                }
                changed.push(o.node_id);
              }
            }
            result = {
              result: {
                plan_id: p.plan.plan_id,
                plan_hash: p.plan_hash,
                approval_id: p.approval_id,
                created,
                changed,
                removed: [],
                moved: [],
                applied_operations: p.plan.operations.length,
                total_operations: p.plan.operations.length,
                undo_committed: true,
                warnings: [],
              },
            };
          } else {
            send(
              "error",
              { code: "CAPABILITY_UNAVAILABLE", message: "fixture operation not supported" },
              f.request_id,
            );
            return;
          }
          send("result", result, f.request_id);
        })
        .catch(() =>
          send(
            "error",
            { code: "INTERNAL_ERROR", message: "synthetic editor failed" },
            f.request_id,
          ),
        );
    };
    plugin.send(
      JSON.stringify({
        protocol_version: "1.0",
        message_type: "hello_auth",
        request_id: "auth",
        payload: { secret, client_id: "agent-evaluation-fixture", protocol_version: "1.0" },
      }),
    );
    secret = "";
    await until(() => auth);
    send("grant_update", {
      root_ids: twoFrames ? ["1:2", "1:3"] : ["1:2"],
      grant_epoch: 0,
      mode,
      session_mode: "live",
    });
    await until(() => grant);
    return {
      runtime,
      port,
      events,
      nodes,
      failNextRead(code) {
        nextReadFailure = code;
      },
      get session() {
        return auth.session_id;
      },
      async changeGrant(nextMode = "read") {
        const epoch = grant.grant_epoch;
        send("grant_update", {
          root_ids: ["1:2"],
          grant_epoch: epoch,
          mode: nextMode,
          session_mode: "live",
        });
        await until(() => grant.grant_epoch !== epoch);
      },
      async close() {
        writeFileSync(
          resolve(directory, "fixture.json"),
          JSON.stringify(
            { kind: "synthetic-editor", events, nodes: [...nodes.values()], native_figma: false },
            null,
            2,
          ),
        );
        writeFileSync(resolve(directory, "daemon.log"), daemonLog);
        if (b) await b.close();
        plugin.close();
        daemon.kill();
      },
    };
  } catch (error) {
    if (b) await b.close();
    plugin?.close();
    daemon.kill();
    throw error;
  }
}

export async function mcpClient(binary, runtime) {
  const child = spawn(binary, ["mcp", "--stdio", "--runtime-dir", runtime], {
    windowsHide: true,
    stdio: ["pipe", "pipe", "pipe"],
  });
  child.stderr.resume();
  const pending = new Map(),
    calls = [];
  let id = 0;
  const lines = createInterface({ input: child.stdout });
  lines.on("line", (line) => {
    const v = JSON.parse(line);
    if (pending.has(v.id)) {
      pending.get(v.id)(v);
      pending.delete(v.id);
    }
  });
  const rpc = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const current = ++id,
        start = performance.now();
      const timer = setTimeout(() => {
        pending.delete(current);
        reject(Error(`${method} timed out`));
      }, 45000);
      pending.set(current, (value) => {
        clearTimeout(timer);
        calls.push({
          method,
          name: params.name,
          elapsed_ms: performance.now() - start,
          is_error: value.result?.isError === true,
        });
        resolve(value.result ?? value);
      });
      child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id: current, method, params }) + "\n");
    });
  const initialized = await rpc("initialize", {
    protocolVersion: "2025-11-25",
    capabilities: {},
    clientInfo: { name: "agent-integration-test", version: "1" },
  });
  child.stdin.write(JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" }) + "\n");
  return {
    rpc,
    initialized,
    calls,
    tool: (name, args) => rpc("tools/call", { name, arguments: args }),
    close() {
      child.kill();
    },
  };
}
