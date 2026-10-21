import { readFileSync, writeFileSync, existsSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { riskyPath } from "./public-check.mjs";
const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const cmp = (a, b) => (a < b ? -1 : a > b ? 1 : 0);
const prose = (t) => t.replace(/^\x60{3}[^\n]*\n[\s\S]*?^\x60{3}\s*$/gm, "");
function anchors(text) {
  const found = new Set(),
    counts = new Map(),
    body = prose(text);
  for (const m of body.matchAll(/^ {0,3}#{1,6}\s+(.+?)\s*#*\s*$/gm)) {
    const base = m[1]
      .replace(/!?\[([^\]]*)\]\([^)]+\)/g, "$1")
      .replace(/<[^>]*>/g, "")
      .replace(/[\x60*_~]/g, "")
      .toLowerCase()
      .replace(/[^\p{L}\p{N}\p{M} _-]/gu, "")
      .replaceAll(" ", "-");
    const count = counts.get(base) ?? 0;
    found.add(base + (count ? "-" + count : ""));
    counts.set(base, count + 1);
  }
  for (const m of body.matchAll(/<(?:a|h[1-6])\b[^>]*\b(?:id|name)=["']([^"']+)["']/g))
    found.add(m[1]);
  return found;
}
const files = execFileSync(
  "git",
  [
    "-c",
    "safe.directory=" + root.replaceAll("\\", "/"),
    "ls-files",
    "--cached",
    "--others",
    "--exclude-standard",
    "-z",
  ],
  { cwd: root, encoding: "utf8" },
)
  .split("\0")
  .filter((f) => f.endsWith(".md") && !riskyPath(f) && existsSync(path.join(root, f)))
  .sort(cmp);
const unique = [...new Set(files)],
  texts = new Map(
    unique.map((file) => [
      file,
      readFileSync(path.join(root, file), "utf8").replace(/^\uFEFF/, ""),
    ]),
  );
const issues = [],
  inventory = [];
const language =
  /\b(?:Panduan FigTether|Jalankan|berikut menunjukkan|izin baca|sebelum pairing|pengujian desktop|tidak dikenali|Bahasa Indonesia)\b/iu;
const stale =
  /\b(?:no write has (?:been )?run in a Figma Desktop session|the sandbox never writes to the document|the plugin only reads; the tool surface has no write operation|Editor packaging is not yet validated)\b/i;
for (const [file, text] of texts) {
  const body = prose(text);
  if (language.test(body))
    issues.push({
      file,
      kind: "language",
      message: "Known non-English documentation phrase remains.",
    });
  if (stale.test(body.replace(/\s+/g, " ")))
    issues.push({
      file,
      kind: "stale-capability",
      message: "Known superseded capability claim remains.",
    });
  const links = [...body.matchAll(/!?\[[^\]]*\]\(([^\s)]+)(?:\s+[^)]*)?\)/g)].map((m) => m[1]);
  links.push(
    ...[...body.matchAll(/<(?:img|a)\b[^>]*\b(?:src|href)=["']([^"']+)["']/g)].map((m) => m[1]),
  );
  for (const link of links) {
    if (/^(?:[a-z][a-z\d+.-]*:|\/\/)/i.test(link)) continue;
    const [pathname, fragment] = link.split("#"),
      clean = pathname.split("?")[0];
    let target;
    try {
      target = clean
        ? path.resolve(root, path.dirname(file), decodeURIComponent(clean))
        : path.join(root, file);
    } catch {
      issues.push({ file, kind: "link", message: "Invalid encoded local link: " + link });
      continue;
    }
    const rel = path.relative(root, target).replaceAll("\\", "/");
    if (rel.startsWith("../") || path.isAbsolute(rel) || !existsSync(target)) {
      issues.push({ file, kind: "link", message: "Unresolved local target: " + link });
      continue;
    }
    if (fragment && target.endsWith(".md")) {
      let decoded;
      try {
        decoded = decodeURIComponent(fragment);
      } catch {
        decoded = fragment;
      }
      if (!anchors(texts.get(rel) ?? readFileSync(target, "utf8")).has(decoded))
        issues.push({ file, kind: "anchor", message: "Unresolved local heading anchor: " + link });
    }
  }
  const role = /^benchmarks\/published\/|publication-review\.md$/.test(file)
    ? "dated-evidence"
    : /^docs\/(?:0[0-9]-|11-|99-)/.test(file)
      ? "design-reference"
      : file.startsWith(".agents/")
        ? "skill"
        : file.startsWith("assets/")
          ? "visual-provenance"
          : "operational";
  inventory.push({
    file,
    role,
    bytes: Buffer.byteLength(text),
    sha256: createHash("sha256").update(text).digest("hex"),
  });
}
const report = {
  schema_version: 1,
  kind: "documentation-consistency-inventory",
  expected_language: "en",
  documents: inventory.length,
  checks: [
    "Known non-English phrase regression screen",
    "Known stale-capability phrase screen",
    "Local Markdown/HTML link targets",
    "Explicit Markdown heading anchors",
  ],
  limitations: [
    "The language screen is not a general language detector or grammar certification.",
    "This inventory does not establish every technical claim. The accompanying audit records the code/evidence passages reviewed.",
    "External URL availability, all inline-code paths, and unlinked prose references are not checked.",
    "Historical design-reference statuses are preserved; see docs/documentation-status.md.",
  ],
  issues,
  inventory,
};
const outIndex = process.argv.indexOf("--out");
if (outIndex >= 0) {
  if (!process.argv[outIndex + 1]) throw new Error("Missing --out path");
  writeFileSync(process.argv[outIndex + 1], JSON.stringify(report, null, 2) + "\n");
}
for (const i of issues) console.error(i.file + ": " + i.kind + ": " + i.message);
console.log(
  "Documentation audit: " + inventory.length + " Markdown files, " + issues.length + " issue(s).",
);
process.exitCode = issues.length ? 1 : 0;
