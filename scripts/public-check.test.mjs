import { test } from "node:test";
import assert from "node:assert/strict";
import { inspectText, localLinks, riskyPath } from "./public-check.mjs";

test("docs and production definitions remain public", () => {
  for (const file of [
    "docs/README.md",
    ".github/workflows/release.yml",
    "security/pentagi/.env.example",
    "Cargo.lock",
    "plugins/figma/manifest.json",
    "benchmarks/published/offline-stages.json",
  ])
    assert.equal(riskyPath(file), false, file);
});
test("secrets, local state, and generated output are refused", () => {
  for (const file of [
    ".env",
    ".env.production",
    "runtime/daemon.json",
    "credentials.json",
    "signing.pfx",
    "target/debug/figtether.exe",
    "plugins/figma/dist/ui.js",
    "benchmarks/results/run.json",
    "security/pentagi/evidence/report.md",
    "plugins/figma/manifest.local.json",
  ])
    assert.equal(riskyPath(file), true, file);
});
test("links exclude external targets and fenced examples", () => {
  const text =
    '[Guide](docs/quickstart.md) [Web](https://example.com) [Here](#intro)\n```md\n[Example](missing.md)\n```\n<img src="assets/architecture.svg">';
  assert.deepEqual(localLinks(text), ["docs/quickstart.md", "assets/architecture.svg"]);
});
test("SVGs cannot carry script, embedded HTML, or remote assets", () => {
  for (const input of [
    "<svg><script>bad()</script></svg>",
    '<svg onload="bad()"/>',
    "<foreignObject/>",
    '<image href="https://example.com/a.png"/>',
  ])
    assert.equal(inspectText("assets/test.svg", input).length, 1);
  assert.deepEqual(inspectText("assets/test.svg", '<svg><path fill="url(#local)"/></svg>'), []);
});
test("placeholder contact checks do not expose credential values", () => {
  const placeholder = "https://github.com/" + "<owner>/<repo>";
  assert.deepEqual(inspectText("SECURITY.md", placeholder), ["placeholder public contact/link"]);
});
test("a byte order mark is refused, because Figma cannot parse it", () => {
  assert.deepEqual(inspectText("plugins/figma/manifest.json", "\u{feff}{}"), [
    "byte order mark at the start of the file",
  ]);
  assert.deepEqual(inspectText("plugins/figma/manifest.json", "{}"), []);
});
test("publication checks catch the same forbidden punctuation as hosted CI", () => {
  for (const file of ["docs/guide.md", "assets/architecture.svg", "scripts/render.mjs"]) {
    assert.deepEqual(inspectText(file, "preview\u2014apply"), ["em dash; use an ASCII hyphen"]);
    assert.deepEqual(inspectText(file, "preview - apply"), []);
  }
});
