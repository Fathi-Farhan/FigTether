import { spawn } from "node:child_process";
import {
  readFileSync,
  writeFileSync,
  mkdirSync,
  mkdtempSync,
  existsSync,
  readdirSync,
} from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { createHash } from "node:crypto";
import { performance } from "node:perf_hooks";
import { collectBrowser } from "../../.agents/skills/design-evidence/scripts/collect-browser.mjs";
import { evaluate, stable } from "../../.agents/skills/design-evidence/scripts/evaluate.mjs";
const runId = new Date().toISOString().replaceAll(":", "-").replaceAll(".", "-");
const raw = path.resolve(process.argv[2] ?? "benchmarks/results/design-evidence-" + runId);
const out = process.argv[3] ?? path.join(raw, "run-summary.json");
if ((existsSync(raw) && readdirSync(raw).length) || existsSync(out)) {
  throw new Error(
    "Choose a new results directory and output file; existing evidence is never overwritten.",
  );
}
const executable =
  process.env.DESIGN_REVIEW_BROWSER ?? "C:/Program Files/Google/Chrome/Application/chrome.exe";
const profile = JSON.parse(
  readFileSync(".agents/skills/design-evidence/references/figtether-bridge-profile.json", "utf8"),
);
const artifact = path.resolve("plugins/figma/dist/ui.html");
const hash = (file) => createHash("sha256").update(readFileSync(file)).digest("hex");
const pause = (ms) => new Promise((r) => setTimeout(r, ms));
mkdirSync(raw, { recursive: true });
const userData = mkdtempSync(path.join(raw, "browser-"));
const browser = spawn(
  executable,
  [
    "--headless",
    "--disable-gpu",
    "--no-sandbox",
    "--no-first-run",
    "--remote-debugging-address=127.0.0.1",
    "--remote-debugging-port=0",
    "--user-data-dir=" + userData,
    "about:blank",
  ],
  { stdio: "ignore", windowsHide: true },
);
let socket,
  id = 0;
const pending = new Map();
const call = (method, params = {}) =>
  new Promise((resolve, reject) => {
    const seq = ++id,
      timer = setTimeout(() => {
        pending.delete(seq);
        reject(new Error("CDP timeout: " + method));
      }, 10000);
    pending.set(seq, { resolve, reject, timer });
    socket.send(JSON.stringify({ id: seq, method, params }));
  });
const js = async (expression) => {
  const response = await call("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  if (response.exceptionDetails)
    throw new Error("Browser evaluation failed: " + response.exceptionDetails.text);
  return response.result.value;
};
const runs = [];
const started = performance.now();
try {
  const active = path.join(userData, "DevToolsActivePort");
  for (let n = 0; n < 100 && !existsSync(active); n++) await pause(100);
  if (!existsSync(active)) throw new Error("Isolated browser did not start");
  const port = readFileSync(active, "utf8").split(/\r?\n/)[0];
  const targets = await (await fetch("http://127.0.0.1:" + port + "/json/list")).json();
  const target = targets.find((t) => t.type === "page");
  socket = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });
  socket.addEventListener("message", (event) => {
    const message = JSON.parse(event.data),
      p = pending.get(message.id);
    if (!p) return;
    clearTimeout(p.timer);
    pending.delete(message.id);
    if (message.error) p.reject(new Error(message.error.message));
    else p.resolve(message.result);
  });
  socket.addEventListener("close", () => {
    for (const p of pending.values()) {
      clearTimeout(p.timer);
      p.reject(new Error("Browser closed"));
    }
    pending.clear();
  });
  await call("Page.enable");
  await call("Runtime.enable");
  await call("DOM.enable");
  await call("Accessibility.enable");
  await call("Emulation.setFocusEmulationEnabled", { enabled: true });
  const version = await call("Browser.getVersion");
  for (const width of [320, 420, 960])
    for (const theme of ["light", "dark"]) {
      await call("Emulation.setDeviceMetricsOverride", {
        width,
        height: 900,
        deviceScaleFactor: 1,
        mobile: false,
      });
      await call("Emulation.setEmulatedMedia", {
        features: [{ name: "prefers-color-scheme", value: theme }],
      });
      await call("Page.navigate", { url: pathToFileURL(artifact).href });
      for (let i = 0; i < 100; i++) {
        if (await js('document.readyState==="complete"')) break;
        await pause(30);
      }
      await js(
        "document.fonts.ready.then(()=>new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(()=>resolve(true)))))",
      );
      const states = width === 420 ? ["unpaired", "help-expanded"] : ["unpaired"];
      for (const state of states) {
        const captureStart = performance.now();
        let helpKeyboard = null;
        if (state === "help-expanded") {
          await js('document.querySelector(".setup-help summary").focus();true');
          await call("Input.dispatchKeyEvent", {
            type: "keyDown",
            key: "Enter",
            code: "Enter",
            windowsVirtualKeyCode: 13,
            nativeVirtualKeyCode: 13,
            text: "\r",
            unmodifiedText: "\r",
          });
          await call("Input.dispatchKeyEvent", {
            type: "keyUp",
            key: "Enter",
            code: "Enter",
            windowsVirtualKeyCode: 13,
          });
          await js("new Promise(resolve=>requestAnimationFrame(()=>resolve(true)))");
          helpKeyboard = await js('document.querySelector(".setup-help").open');
          if (!helpKeyboard) throw new Error("Focused help disclosure did not open with Enter");
        }
        const snapshot = await js(
          "(" +
            collectBrowser.toString() +
            ")(" +
            JSON.stringify({ limit: 5000, reflowException: false }) +
            ")",
        );
        snapshot.source.browser = version.product;
        snapshot.source.javascript = version.jsVersion;
        snapshot.source.artifact_sha256 = hash(artifact);
        snapshot.source.collector_sha256 = hash(
          ".agents/skills/design-evidence/scripts/collect-browser.mjs",
        );
        snapshot.source.theme = theme;
        snapshot.source.state = state;
        snapshot.source.requested_width = width;
        const doc = await call("DOM.getDocument");
        for (const n of snapshot.nodes.filter((n) => n.role === "control")) {
          const found = await call("DOM.querySelector", {
            nodeId: doc.root.nodeId,
            selector: n.id,
          });
          if (!found.nodeId) continue;
          const tree = await call("Accessibility.getPartialAXTree", {
            nodeId: found.nodeId,
            fetchRelatives: false,
          });
          const ax = tree.nodes.find((a) => !a.ignored);
          if (typeof ax?.name?.value === "string") {
            n.name = ax.name.value;
            n.name_source = "browser-ax";
          }
        }
        const name = width + "-" + theme + "-" + state;
        writeFileSync(
          path.join(raw, name + ".snapshot.json"),
          JSON.stringify(snapshot, null, 2) + "\n",
        );
        const report = evaluate(snapshot, profile);
        writeFileSync(path.join(raw, name + ".report.json"), stable(report) + "\n");
        const screenshot = await call("Page.captureScreenshot", {
          format: "png",
          captureBeyondViewport: false,
        });
        writeFileSync(path.join(raw, name + ".png"), Buffer.from(screenshot.data, "base64"));
        const timings = [],
          expected = stable(report);
        for (let i = 0; i < 100; i++) {
          const begin = performance.now(),
            again = evaluate(snapshot, profile);
          timings.push(performance.now() - begin);
          if (stable(again) !== expected) throw new Error("Determinism mismatch");
        }
        timings.sort((a, b) => a - b);
        runs.push({
          width_css_px: snapshot.viewport.width,
          requested_width: width,
          theme,
          state,
          visible_nodes: report.coverage.visible_nodes,
          capture_complete: report.coverage.complete,
          counts: report.summary.counts,
          decision: report.summary.decision,
          findings: report.findings.filter((f) =>
            ["failed", "review", "cantTell"].includes(f.outcome),
          ),
          input_sha256: report.input_sha256,
          profile_sha256: report.profile_sha256,
          ruleset_sha256: report.ruleset_sha256,
          evaluator_runs: 100,
          byte_identical: true,
          evaluator_p50_ms: timings[49],
          evaluator_p95_ms: timings[94],
          capture_and_review_ms: performance.now() - captureStart,
          focused_help_enter_opens: helpKeyboard,
          raw_snapshot: path
            .relative(process.cwd(), path.join(raw, name + ".snapshot.json"))
            .replaceAll("\\", "/"),
          raw_screenshot: path
            .relative(process.cwd(), path.join(raw, name + ".png"))
            .replaceAll("\\", "/"),
        });
        console.log(name + ": " + JSON.stringify(report.summary.counts));
      }
    }
  const result = {
    schema_version: 1,
    kind: "design-evidence-browser-smoke-test",
    created_at: new Date().toISOString(),
    artifact: { file: "plugins/figma/dist/ui.html", sha256: hash(artifact) },
    browser: version.product,
    javascript: version.jsVersion,
    node: process.version,
    platform: process.platform,
    ruleset_sha256: runs[0].ruleset_sha256,
    collector_sha256: hash(".agents/skills/design-evidence/scripts/collect-browser.mjs"),
    profile,
    run_count: runs.length,
    elapsed_ms: performance.now() - started,
    runs,
    limits: [
      "Isolated local browser render of built plugin HTML, not a fresh native Figma or MCP benchmark.",
      "Only unpaired and focused-help-expanded states; no pairing secret entered and no design mutation.",
      "Standard-derived checks are partial; no WCAG conformance or AI-authorship accuracy is assessed.",
      "P50/p95 timings measure the local evaluator only; they do not measure Figma, model inference, or user task time.",
      "No representative human/AI labeled corpus or user study was run.",
    ],
    costs: {
      model_tokens: null,
      model_cost_usd: null,
      status: "NOT_MEASURED",
      note: "The collector/evaluator call no model or paid remote API. Chat/research billing is not exposed.",
    },
  };
  mkdirSync(path.dirname(out), { recursive: true });
  writeFileSync(out, JSON.stringify(result, null, 2) + "\n");
  console.log(out);
} finally {
  if (socket?.readyState === WebSocket.OPEN) {
    try {
      await call("Browser.close");
    } catch {}
    socket.close();
  }
  if (!browser.killed) browser.kill();
}
