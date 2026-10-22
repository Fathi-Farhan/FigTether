// Reproducible failure/recovery probes against the real CLI, isolated from Figma.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdir, mkdtemp, writeFile } from "node:fs/promises";
import net from "node:net";
import os from "node:os";
import path from "node:path";
const flag = process.argv.indexOf("--binary");
assert(flag >= 0 && process.argv[flag + 1], "pass --binary PATH_TO_FIGTETHER");
const binary = path.resolve(process.argv[flag + 1]);
const root = await mkdtemp(path.join(os.tmpdir(), "figtether-user-errors-"));
const runtime = path.join(root, "runtime");
const rows = [];
const fixture = async (name, body) => {
  const file = path.join(root, name);
  await writeFile(file, body);
  return file;
};
function probe(id, args, status, expected, json = false) {
  const start = performance.now();
  const result = spawnSync(binary, [...args, "--runtime-dir", runtime], {
    cwd: root,
    encoding: "utf8",
    windowsHide: true,
    timeout: 8000,
  });
  let passed =
    !result.error && result.status === status && expected.test(result.stdout + result.stderr);
  if (json) {
    try {
      const error = JSON.parse(result.stderr);
      passed &&=
        typeof error.code === "string" &&
        typeof error.retryable === "boolean" &&
        error.action.length > 15;
      passed &&=
        args[0] === "doctor" ? JSON.parse(result.stdout).ok === false : result.stdout.trim() === "";
    } catch {
      passed = false;
    }
  }
  if (status === 1 && !json) passed &&= result.stderr.includes("Next:");
  rows.push({
    id,
    passed,
    exit_code: result.status,
    elapsed_ms: performance.now() - start,
    stdout: result.stdout,
    stderr: result.stderr,
  });
}
for (const name of ["status", "pair"])
  for (const json of [false, true])
    probe(
      `${name}-daemon-missing-${json ? "json" : "text"}`,
      [name, ...(json ? ["--json"] : [])],
      1,
      /figtether start/,
      json,
    );
probe("revoke-daemon-missing", ["revoke", "--all"], 1, /figtether start/);
probe("stop-already-stopped", ["stop", "--json"], 0, /not_running/);
for (const value of ["0", "65536", "-1", "not-a-port"])
  probe(`invalid-port-${value}`, ["daemon", "--port", value], 2, /help/);
probe("invalid-session", ["revoke", "--session", "not-a-session"], 2, /help/);
probe("missing-revoke-target", ["revoke"], 2, /help/);
probe(
  "source-folder-missing",
  ["start", "--plugin-dir", path.join(root, "not-there")],
  1,
  /plugin source folder not found/,
);
probe("source-package-missing", ["start", "--plugin-dir", root], 1, /package.json is missing/);
await writeFile(path.join(root, "package.json"), "{broken");
probe(
  "source-package-json-invalid",
  ["start", "--plugin-dir", root],
  1,
  /package.json is not valid JSON/,
);
await writeFile(path.join(root, "package.json"), JSON.stringify({ name: "unrelated-package" }));
probe(
  "source-package-wrong",
  ["start", "--plugin-dir", root],
  1,
  /choose the FigTether plugin source folder/,
);
const bom = await fixture("bom.json", "\uFEFF{}");
const broken = await fixture("broken.json", "{broken");
const missing = path.join(root, "missing.json");
for (const [name, file, expected] of [
  ["bom", bom, /byte order mark/],
  ["json", broken, /not valid JSON/],
  ["missing", missing, /does not exist|cannot|could not|No manifest|no manifest/],
])
  probe(`doctor-manifest-${name}`, ["doctor", "--manifest", file, "--json"], 1, expected, true);
// An explicit output is also the manifest input: malformed existing files must not be overwritten.
probe("manifest-bom-repaired", ["manifest", "--out", bom, "--port", "auto"], 0, /Manifest written/);
probe(
  "manifest-json-preserved",
  ["manifest", "--out", broken, "--port", "auto"],
  1,
  /invalid JSON|key must be a string/,
);
const reservation = net.createServer();
await new Promise((resolve) => reservation.listen(0, "127.0.0.1", resolve));
const port = reservation.address().port;
try {
  probe("occupied-port", ["daemon", "--port", String(port)], 1, /port|bind/);
} finally {
  await new Promise((resolve) => reservation.close(resolve));
}
const outputFlag = process.argv.indexOf("--out");
const out = path.resolve(
  outputFlag < 0 ? "artifacts/end-user-auto/cli-errors.json" : process.argv[outputFlag + 1],
);
await mkdir(path.dirname(out), { recursive: true });
await writeFile(
  out,
  JSON.stringify(
    { scope: "isolated real CLI; no production daemon, secrets, or design data", rows },
    null,
    2,
  ),
);
const failures = rows.filter((row) => !row.passed);
console.log(
  JSON.stringify({
    cases: rows.length,
    passed: rows.length - failures.length,
    failed: failures.map((row) => row.id),
    evidence: out,
  }),
);
if (failures.length) process.exitCode = 1;
