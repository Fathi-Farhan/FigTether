// Run the real CLI and plugin build in an isolated source copy. Pairing secrets
// stay in memory and are never written into evidence or printed by this harness.
import assert from "node:assert/strict";
import { spawn, execFileSync } from "node:child_process";
import { cp, mkdir, mkdtemp, readFile, symlink, unlink, writeFile } from "node:fs/promises";
import net from "node:net";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repo = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const binaryFlag = process.argv.indexOf("--binary");
assert(binaryFlag !== -1 && process.argv[binaryFlag + 1], "pass --binary PATH_TO_FIGTETHER");
const binary = path.resolve(process.argv[binaryFlag + 1]);
const comparablePath = (value) => value.replaceAll("\\\\?\\", "").replaceAll("\\", "/");
const root = await mkdtemp(path.join(os.tmpdir(), "figtether-start-smoke-"));
const plugin = path.join(root, "plugin source");
const runtime = path.join(root, "runtime");
const unrelated = path.join(root, "unrelated");
await mkdir(plugin);
await mkdir(unrelated);
for (const name of [
  "build.mjs",
  "manifest-config.mjs",
  "package.json",
  "manifest.json",
  "main/src",
  "shared/src",
  "ui/src",
  "ui/assets",
  "ui/index.html",
  "ui/styles.css",
])
  await cp(path.join(repo, "plugins/figma", name), path.join(plugin, name), { recursive: true });
await symlink(
  path.join(repo, "plugins/figma/node_modules"),
  path.join(plugin, "node_modules"),
  process.platform === "win32" ? "junction" : "dir",
);

const reservation = net.createServer();
await new Promise((resolve) => reservation.listen(0, "127.0.0.1", resolve));
const port = reservation.address().port;
await new Promise((resolve) => reservation.close(resolve));
const invoke = (...args) =>
  execFileSync(binary, [...args, "--runtime-dir", runtime], {
    cwd: unrelated,
    encoding: "utf8",
    timeout: 15000,
    stdio: ["ignore", "pipe", "pipe"],
  });
const child = spawn(
  binary,
  [
    "start",
    "--plugin-dir",
    plugin,
    "--plugin-id",
    "synthetic-startup-id",
    "--port",
    String(port),
    "--runtime-dir",
    runtime,
  ],
  { cwd: unrelated, stdio: ["ignore", "pipe", "pipe"], windowsHide: true },
);
let output = "";
let errors = "";
child.stdout.setEncoding("utf8").on("data", (chunk) => {
  output += chunk;
});
child.stderr.setEncoding("utf8").on("data", (chunk) => {
  errors += chunk;
});
const closed = new Promise((resolve) => child.once("close", resolve));
try {
  await new Promise((resolve, reject) => {
    const deadline = setTimeout(() => {
      clearInterval(poll);
      reject(Error("startup did not display a pairing secret within 30 seconds"));
    }, 30000);
    const poll = setInterval(() => {
      if (child.exitCode !== null) {
        clearInterval(poll);
        clearTimeout(deadline);
        reject(Error(`startup exited with ${child.exitCode}: ${errors}`));
      }
      if (output.includes("Pairing secret (single use")) {
        clearInterval(poll);
        clearTimeout(deadline);
        resolve();
      }
    }, 50);
  });
  assert(
    comparablePath(output).includes(
      `Select ${comparablePath(path.join(plugin, "dist/manifest.json"))}`,
    ),
  );
  output = "";
  const selected = JSON.parse(await readFile(path.join(plugin, "manifest.local.json"), "utf8"));
  const built = JSON.parse(await readFile(path.join(plugin, "dist/manifest.json"), "utf8"));
  assert.equal(selected.id, "synthetic-startup-id");
  assert.equal(built.id, selected.id);
  assert.deepEqual(built.networkAccess, selected.networkAccess);
  const report = JSON.parse(invoke("doctor", "--json"));
  assert.equal(report.port, port);
  assert.equal(report.checks.find((check) => check.name === "plugin bundle").status, "ok");
  const before = JSON.parse(await readFile(path.join(runtime, "daemon.json"), "utf8")).instance_id;
  // An already-running daemon can predate manifest registration.
  await unlink(path.join(runtime, "plugin.json"));
  const reused = invoke("start", "--plugin-dir", plugin);
  assert(reused.includes("already running"));
  assert(
    comparablePath(reused).includes(
      `Figma import: ${comparablePath(path.join(plugin, "dist/manifest.json"))}`,
    ),
  );
  assert(reused.includes("Pairing secret (single use"));
  const infoPath = path.join(plugin, "dist/build-info.json");
  const info = await readFile(infoPath, "utf8");
  await writeFile(infoPath, JSON.stringify({ ...JSON.parse(info), port: 0 }));
  assert.throws(
    () => invoke("start", "--plugin-dir", plugin),
    (error) => {
      assert(!String(error.stdout).includes("Pairing secret (single use"));
      return String(error.stderr).includes("before pairing");
    },
  );
  await writeFile(infoPath, info);
  const inputPath = path.join(plugin, "manifest.local.json");
  const originalInput = await readFile(inputPath, "utf8");
  const movedPort = port === 65535 ? port - 1 : port + 1;
  await writeFile(
    inputPath,
    JSON.stringify({
      ...selected,
      networkAccess: {
        ...selected.networkAccess,
        devAllowedDomains: [`http://localhost:${movedPort}`, `ws://localhost:${movedPort}`],
      },
    }),
  );
  assert.throws(
    () => invoke("start", "--plugin-dir", plugin, "--port", String(port)),
    (error) => {
      assert(!String(error.stdout).includes("Pairing secret (single use"));
      return String(error.stderr).includes("source manifest does not yet agree");
    },
  );
  await writeFile(inputPath, originalInput);
  const remembered = JSON.parse(await readFile(path.join(runtime, "plugin.json"), "utf8"));
  assert.equal(path.basename(remembered.manifest), "manifest.local.json");
  assert.equal(
    JSON.parse(await readFile(path.join(runtime, "daemon.json"), "utf8")).instance_id,
    before,
  );
  assert.equal(JSON.parse(invoke("stop", "--json")).outcome, "stopped");
  assert.equal(await closed, 0);
  // A stale descriptor cannot pull the manifest generator onto an unrelated listener.
  const generated = JSON.parse(await readFile(path.join(plugin, "manifest.local.json"), "utf8"));
  invoke("manifest", "--port", "auto");
  assert.deepEqual(
    JSON.parse(await readFile(path.join(plugin, "manifest.local.json"), "utf8")),
    generated,
  );
  console.log(
    "Startup smoke passed: real plugin build, one-terminal pairing, saved manifest discovery, daemon reuse, stable regeneration, graceful stop.",
  );
} finally {
  output = "";
  if (child.exitCode === null) {
    try {
      invoke("stop", "--json");
    } catch {
      child.kill();
    }
    await closed;
  }
}
