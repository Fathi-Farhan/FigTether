import { spawn } from "node:child_process";
import { readFileSync, writeFileSync, mkdirSync, existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
export async function browser() {
  const profile = path.resolve(`tmp/bridge-ux/chrome-${Date.now()}`);
  mkdirSync(profile, { recursive: true });
  const child = spawn(
    process.env.CHROME_BINARY || "C:/Program Files/Google/Chrome/Application/chrome.exe",
    [
      "--headless=new",
      "--disable-gpu",
      "--no-first-run",
      "--no-default-browser-check",
      `--user-data-dir=${profile}`,
      "--remote-debugging-port=0",
      "about:blank",
    ],
    { windowsHide: true, stdio: "ignore" },
  );
  let file = path.join(profile, "DevToolsActivePort");
  for (let i = 0; i < 100 && !existsSync(file); i++) await new Promise((r) => setTimeout(r, 50));
  const port = readFileSync(file, "utf8").split("\n")[0];
  const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
  const socket = new WebSocket(targets.find((t) => t.type === "page").webSocketDebuggerUrl);
  await new Promise((r) => socket.addEventListener("open", r, { once: true }));
  let id = 0;
  const pending = new Map();
  socket.addEventListener("close", () => {
    for (const p of pending.values()) p.reject(Error("isolated browser closed"));
    pending.clear();
  });
  socket.addEventListener("message", (e) => {
    const d = JSON.parse(e.data);
    if (d.id) {
      const p = pending.get(d.id);
      pending.delete(d.id);
      d.error ? p?.reject(Error(d.error.message)) : p?.resolve(d.result);
    }
  });
  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      const i = ++id;
      const timer = setTimeout(() => {
        pending.delete(i);
        reject(Error(`browser ${method} timed out`));
      }, 5000);
      pending.set(i, {
        resolve: (v) => {
          clearTimeout(timer);
          resolve(v);
        },
        reject: (e) => {
          clearTimeout(timer);
          reject(e);
        },
      });
      socket.send(JSON.stringify({ id: i, method, params }));
    });
  await send("Page.enable");
  await send("Runtime.enable");
  return {
    send,
    async eval(expression) {
      const r = await send("Runtime.evaluate", {
        expression,
        returnByValue: true,
        awaitPromise: true,
      });
      if (r.exceptionDetails) throw Error(JSON.stringify(r.exceptionDetails));
      return r.result.value;
    },
    async capture(file, width = 440, height = 950) {
      await send("Emulation.setDeviceMetricsOverride", {
        width,
        height,
        deviceScaleFactor: 1,
        mobile: false,
      });
      await new Promise((r) => setTimeout(r, 150));
      const r = await send("Page.captureScreenshot", {
        format: "png",
        captureBeyondViewport: false,
      });
      writeFileSync(file, Buffer.from(r.data, "base64"));
    },
    async close() {
      try {
        await send("Browser.close");
      } catch {}
      socket.close();
      child.kill();
    },
  };
}
if (
  process.argv[1] &&
  path.resolve(process.argv[1]) === fileURLToPath(import.meta.url) &&
  process.argv[2]
) {
  const b = await browser();
  try {
    await b.send("Page.navigate", { url: process.argv[2] });
    await new Promise((r) => setTimeout(r, 350));
    await b.capture(process.argv[3]);
  } finally {
    await b.close();
  }
}
