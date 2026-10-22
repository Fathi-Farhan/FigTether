// Publication hygiene, not a substitute for a history-aware secret scanner.
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export function riskyPath(file) {
  const name = path.posix.basename(file);
  if (
    (/^\.env(?:\.|$)/.test(name) && name !== ".env.example") ||
    /^(?:daemon|installation|credentials)\.json$/i.test(name) ||
    /\.(?:pem|key|p12|pfx|keystore|jks|log|tsbuildinfo)$/i.test(name) ||
    /^(?:id_rsa|id_ed25519)$/.test(name)
  )
    return true;
  return (
    /(?:^|\/)(?:node_modules|target|dist|coverage|tmp|artifacts|runs)(?:\/|$)/.test(file) ||
    /^(?:benchmarks\/results|security\/pentagi\/evidence)\//.test(file) ||
    /(?:^|\/)manifest\.local\.json$/.test(file)
  );
}

export function inspectText(file, text) {
  const problems = [];
  if (text.charCodeAt(0) === 0xfeff) problems.push("byte order mark at the start of the file");
  if (text.includes("\u2014")) problems.push("em dash; use an ASCII hyphen");
  if (/-----BEGIN (?:RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----/.test(text))
    problems.push("private key material");
  if (/\b(?:ghp_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{30,}|AKIA[A-Z0-9]{16})\b/.test(text))
    problems.push("credential-shaped value");
  if (
    /\.(?:md|ya?ml|json|toml)$/i.test(file) &&
    /https:\/\/github\.com\/<(?:owner|repo)>|security@<domain>/.test(text)
  )
    problems.push("placeholder public contact/link");
  if (
    /\.md$/i.test(file) &&
    /\b[A-Za-z]:[\\/](?:Users|laragon[^\\/]*)[\\/](?!example\b|USERNAME\b|your-user\b)/i.test(text)
  )
    problems.push("private workstation path");
  if (
    /\.svg$/i.test(file) &&
    /<script\b|<foreignObject\b|\bon\w+\s*=|(?:href|src)\s*=\s*["'](?:https?:|data:|javascript:)|url\(\s*["']?https?:/i.test(
      text,
    )
  )
    problems.push("active or external SVG content");
  return problems;
}

export function localLinks(markdown) {
  const prose = markdown.replace(/^```[^\n]*\n[\s\S]*?^```\s*$/gm, "");
  const links = [...prose.matchAll(/!?\[[^\]]*\]\(([^\s)]+)(?:\s+[^)]*)?\)/g)].map((m) => m[1]);
  links.push(
    ...[...prose.matchAll(/<(?:img|a)\b[^>]*\b(?:src|href)=["']([^"']+)["']/g)].map((m) => m[1]),
  );
  return links.filter((link) => !/^(?:[a-z][a-z\d+.-]*:|\/\/|#)/i.test(link));
}

export function checkPublicTree(root) {
  const output = execFileSync(
    "git",
    ["ls-files", "--cached", "--others", "--exclude-standard", "-z"],
    { cwd: root, encoding: "utf8", maxBuffer: 16 * 1024 * 1024 },
  );
  const files = [...new Set(output.split("\0").filter(Boolean))];
  const problems = [];
  for (const file of files) {
    if (riskyPath(file))
      problems.push(`${file}: local/generated/sensitive file in public file set`);
    const absolute = path.join(root, file);
    if (!existsSync(absolute)) continue; // A deletion in the working tree is not being published.
    if (!/\.(?:md|svg|mjs|js|ts|rs|json|toml|ya?ml|sh|ps1)$/.test(file)) continue;
    const text = readFileSync(absolute, "utf8");
    for (const issue of inspectText(file, text)) problems.push(`${file}: ${issue}`);
    if (!file.endsWith(".md")) continue;
    for (const link of localLinks(text)) {
      const pathname = decodeURIComponent(link.split(/[?#]/)[0]);
      if (!pathname) continue;
      const target = path.resolve(path.dirname(absolute), pathname);
      const relative = path.relative(root, target);
      if (relative.startsWith("..") || path.isAbsolute(relative) || !existsSync(target))
        problems.push(`${file}: unresolved local link: ${link}`);
    }
  }
  return { files: files.length, problems };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const result = checkPublicTree(root);
  for (const problem of result.problems) console.error(problem);
  console.log(`Public file check: ${result.files} files, ${result.problems.length} issue(s).`);
  console.log("This checks the proposed file set, not Git history or application security.");
  process.exitCode = result.problems.length ? 1 : 0;
}
