import { spawn, spawnSync } from "node:child_process";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { createHash } from "node:crypto";
import net from "node:net";
import path from "node:path";
import { browser } from "./browser.mjs";
const binary = process.env.FIGTETHER_TEST_BINARY || "target/release/figtether.exe";
const directory = "benchmarks/results/bridge-ux-2026-10-21";
mkdirSync(directory, { recursive: true });
const runtime = path.resolve(directory, "runtime");
const listener = net.createServer();
await new Promise((r) => listener.listen(0, "127.0.0.1", r));
const port = listener.address().port;
await new Promise((r) => listener.close(r));
const base = `http://127.0.0.1:${port}`;
const daemon = spawn(binary, ["daemon", "--port", String(port), "--runtime-dir", runtime], {
  windowsHide: true,
});
let log = "";
daemon.stdout.on("data", (x) => (log += x));
daemon.stderr.on("data", (x) => (log += x));
const rows = [];
let auth, url, plugin, b;
const wait = async (predicate) => {
  for (let i = 0; i < 120; i++) {
    if (await predicate()) return;
    await new Promise((r) => setTimeout(r, 40));
  }
  throw Error("condition timed out");
};
const check = (name, ok, extra = {}) => {
  rows.push({ name, ok, ...extra });
  if (!ok) throw Error(name);
};
const model = {
  connection: "connected",
  connectionText: "Connected",
  grantedRootCount: 1,
  grantEpoch: 0,
  sharedFrameNames: ["Website Test"],
  writeGrantEnabled: true,
  canGrant: true,
  canGrantWrite: true,
  canRevoke: true,
  canUseAutoMode: true,
  autoMode: false,
  canApproveWrite: false,
  canRejectWrite: false,
  nextStepText: "Ready to design. Your shared frame stays editable.",
  selectedRootIds: ["1:2"],
  candidates: [{ node_id: "1:2", name: "Website Test", node_type: "FRAME", child_count: 71 }],
  writePreview: null,
  writeDecision: "none",
  writeApprovalId: null,
  revokeConfirmVisible: false,
};
const messages = [];
const send = (type, payload, id = "fixture") =>
  plugin.send(
    JSON.stringify({
      protocol_version: "1.0",
      message_type: type,
      request_id: id,
      session_id: auth?.session_id,
      connection_epoch: auth?.connection_epoch,
      payload,
    }),
  );
const publish = () =>
  send("ui_control", {
    action: "state",
    model,
    logs: ["12:30:01 op=list_nodes count=50 duration=62ms", "12:30:02 op=preview count=1"],
  });
try {
  await wait(() => log.includes("DAEMON_STARTED"));
  const pair = spawnSync(binary, ["pair", "--json", "--runtime-dir", runtime], {
    encoding: "utf8",
    windowsHide: true,
  });
  if (pair.status !== 0) throw Error("local fixture pairing failed");
  const secret = JSON.parse(pair.stdout);
  plugin = new WebSocket(`ws://127.0.0.1:${port}/bridge`);
  plugin.addEventListener("message", (event) => {
    const frame = JSON.parse(event.data);
    if (frame.request_id === "auth" && frame.message_type === "result") auth = frame.payload;
    else if (frame.message_type === "heartbeat" && auth) send("heartbeat", { sequence: 1 });
    else if (frame.message_type === "ui_control" && frame.payload.action === "opened") {
      url = frame.payload.url;
      publish();
    } else if (frame.message_type === "ui_control" && frame.payload.action === "command") {
      messages.push(frame.payload);
      if (frame.payload.command === "auto") model.autoMode = frame.payload.value;
      if (frame.payload.command === "stop") model.revokeConfirmVisible = true;
      if (frame.payload.command === "stop_cancel") model.revokeConfirmVisible = false;
      if (frame.payload.command === "approve") model.writeDecision = "approved";
      publish();
    }
  });
  await new Promise((resolve, reject) => {
    plugin.onopen = resolve;
    plugin.onerror = reject;
  });
  plugin.send(
    JSON.stringify({
      protocol_version: "1.0",
      message_type: "hello_auth",
      request_id: "auth",
      payload: { secret: secret.secret, client_id: "companion-test", protocol_version: "1.0" },
    }),
  );
  secret.secret = "";
  await wait(() => auth);
  check(
    "daemon advertises supported UI features",
    auth.ui_features.includes("companion_window") &&
      auth.ui_features.includes("warning_gated_auto"),
  );
  send("ui_control", { action: "open" });
  await wait(() => url);
  const capability = url.split("#")[1],
    headers = { Authorization: `Bearer ${capability}`, Origin: base };
  check(
    "state rejects unauthenticated request",
    (await fetch(base + "/panel/state")).status === 401,
  );
  check(
    "cross-origin companion request rejected",
    (await fetch(base + "/panel/state", { headers: { ...headers, Origin: "https://example.com" } }))
      .status === 403,
  );
  check(
    "native control remains inaccessible to browser",
    (await fetch(base + "/control/pair", { method: "POST", headers })).status === 403,
  );
  let snapshot = await (await fetch(base + "/panel/state", { headers })).json();
  check(
    "only the opened session snapshot is exposed",
    snapshot.model.sharedFrameNames[0] === "Website Test" &&
      !JSON.stringify(snapshot).includes(capability),
  );
  const samples = [];
  for (let i = 0; i < 10; i++) {
    const start = performance.now();
    const r = await fetch(base + "/panel/state", { headers });
    await r.text();
    samples.push(performance.now() - start);
    check(`protected snapshot sample ${i + 1}`, r.status === 200);
  }
  b = await browser();
  await b.send("Page.navigate", { url });
  await wait(() =>
    b.eval('document.getElementById("connection").textContent.includes("Ready to design")'),
  );
  check("fragment capability removed from browser address", await b.eval('location.hash === ""'));
  check(
    "named frame rendered in companion",
    await b.eval('document.getElementById("shared").textContent === "Website Test"'),
  );
  await b.eval('document.getElementById("auto").click()');
  await wait(() => messages.some((x) => x.command === "auto"));
  check(
    "browser auto toggle relayed to plugin with epoch",
    messages.find((x) => x.command === "auto").value === true &&
      messages.find((x) => x.command === "auto").grant_epoch === 0,
  );
  await new Promise((r) => setTimeout(r, 250));
  await b.capture("tmp/bridge-ux/09-companion.png", 740, 1050);
  model.autoMode = false;
  model.autoModeReason = "Paused: review the warnings in this plan.";
  model.writeApprovalId = "fixture-approval";
  model.writeDecision = "none";
  model.canApproveWrite = true;
  model.canRejectWrite = true;
  model.writePreview = {
    planId: "fixture",
    planHash: "sha256:fixture",
    intent: "Adjust the website heading",
    warnings: ["Font unavailable. Choose an installed font."],
    blocked: false,
    counts: { create: 0, change: 1, move: 0, remove: 0 },
  };
  publish();
  await wait(() =>
    b.eval('document.getElementById("warnings").textContent.includes("Font unavailable")'),
  );
  check(
    "warning and paused mode visible in browser",
    await b.eval(
      '!document.getElementById("auto").checked && document.getElementById("auto-state").textContent.includes("Paused")',
    ),
  );
  await b.capture("tmp/bridge-ux/10-companion-warning.png", 740, 1050);
  await b.eval('document.getElementById("approve").click()');
  await wait(() => messages.some((x) => x.command === "approve"));
  const decision = messages.find((x) => x.command === "approve");
  check(
    "manual decision is bound to the displayed plan",
    decision.value.planHash === "sha256:fixture" &&
      decision.value.approvalId === "fixture-approval",
  );
  await new Promise((r) => setTimeout(r, 250));
  await b.eval('document.getElementById("stop").click()');
  await wait(() => b.eval('document.getElementById("stop-confirmation").hidden===false'));
  check(
    "stop sharing requires separate confirmation",
    !messages.some((x) => x.command === "stop_confirm"),
  );
  await new Promise((r) => setTimeout(r, 250));
  await b.eval('document.getElementById("stop_cancel").click()');
  await wait(() => b.eval('document.getElementById("stop-confirmation").hidden===true'));
  const old = url;
  send("ui_control", { action: "open" });
  await wait(() => url !== old);
  check(
    "reopening invalidates old capability",
    (await fetch(base + "/panel/state", { headers })).status === 401,
  );
  await wait(() => b.eval('document.getElementById("auto").disabled===true'));
  check(
    "expired companion controls disabled",
    await b.eval('document.getElementById("approve").disabled===true'),
  );
  samples.sort((a, b) => a - b);
  writeFileSync(
    `${directory}/native-companion.json`,
    JSON.stringify(
      {
        kind: "isolated-daemon-and-browser-with-synthetic-plugin",
        created_at: new Date().toISOString(),
        binary_sha256: createHash("sha256").update(readFileSync(binary)).digest("hex"),
        rows,
        latency: {
          workload: "authorized companion snapshot, synthetic state, local HTTP",
          samples: 10,
          p50_ms: samples[4],
          p95_ms: samples[9],
        },
        cost: { tokens: null, billed_cost_usd: null, status: "unmeasured" },
        limits: [
          "Synthetic plugin on a real authenticated WebSocket; no actual Figma editor mutations",
          "Shared daemon/session not restarted or revoked",
          "Browser controls and HTTP security tested separately from native Figma openExternal click",
        ],
      },
      null,
      2,
    ),
  );
  console.log(
    JSON.stringify({
      checks: rows.length,
      passed: rows.filter((x) => x.ok).length,
      p50_ms: samples[4],
      p95_ms: samples[9],
    }),
  );
} finally {
  writeFileSync(`${directory}/last-observations.json`, JSON.stringify(rows, null, 2));
  writeFileSync(`${directory}/daemon.log`, log);
  if (b) await b.close();
  plugin?.close();
  daemon.kill();
}
