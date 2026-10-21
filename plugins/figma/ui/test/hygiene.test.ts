import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

const sourceRoot = fileURLToPath(new URL("../src", import.meta.url));

function sourceFiles(directory: string): string[] {
  const files: string[] = [];
  for (const entry of readdirSync(directory)) {
    const full = join(directory, entry);
    if (statSync(full).isDirectory()) {
      files.push(...sourceFiles(full));
    } else if (entry.endsWith(".ts")) {
      files.push(full);
    }
  }
  return files;
}

const sources = sourceFiles(sourceRoot).map((file) => ({
  file,
  content: readFileSync(file, "utf8"),
}));

/**
 * Remove the comments of a source file so that the scan looks at code only.
 *
 * String literals are preserved, so that an address such as ws://127.0.0.1
 * survives the scan.
 */
function stripComments(source: string): string {
  let out = "";
  let index = 0;
  let quote: string | null = null;
  while (index < source.length) {
    const char = source[index] as string;
    const next = index + 1 < source.length ? (source[index + 1] as string) : "";
    if (quote !== null) {
      out += char;
      if (char === "\\") {
        out += next;
        index += 2;
        continue;
      }
      if (char === quote) {
        quote = null;
      }
      index += 1;
      continue;
    }
    if (char === '"' || char === "'" || char === "`") {
      quote = char;
      out += char;
      index += 1;
      continue;
    }
    if (char === "/" && next === "*") {
      const end = source.indexOf("*/", index + 2);
      index = end < 0 ? source.length : end + 2;
      continue;
    }
    if (char === "/" && next === "/") {
      const end = source.indexOf("\n", index);
      index = end < 0 ? source.length : end;
      continue;
    }
    out += char;
    index += 1;
  }
  return out;
}

const code = sources.map((source) => ({
  file: source.file,
  content: stripComments(source.content),
}));

describe("credential and code hygiene", () => {
  it("never touches a persistent store", () => {
    for (const source of code) {
      expect(source.content, source.file).not.toMatch(/localStorage/);
      expect(source.content, source.file).not.toMatch(/sessionStorage/);
      expect(source.content, source.file).not.toMatch(/document\.cookie/);
      expect(source.content, source.file).not.toMatch(/indexedDB/);
    }
  });

  it("never evaluates code", () => {
    for (const source of code) {
      expect(source.content, source.file).not.toMatch(/\beval\s*\(/);
      expect(source.content, source.file).not.toMatch(/new\s+Function\s*\(/);
    }
  });

  it("never writes markup into the document", () => {
    for (const source of code) {
      expect(source.content, source.file).not.toMatch(/\.innerHTML\s*=/);
      expect(source.content, source.file).not.toMatch(/insertAdjacentHTML/);
      expect(source.content, source.file).not.toMatch(/document\.write/);
    }
  });

  it("keeps the loopback endpoint as the only network target", () => {
    let found = 0;
    for (const source of code) {
      const matches = source.content.match(/wss?:\/\/[^\s"'`]+/g) ?? [];
      found += matches.length;
      for (const match of matches) {
        expect(match, source.file).toBe("ws://localhost:${LOCAL_BRIDGE_PORT}/bridge");
      }
    }
    expect(found).toBeGreaterThan(0);
  });

  it("uses no external dependency in the sources", () => {
    for (const source of code) {
      const imports = source.content.match(/from\s+"([^"]+)"/g) ?? [];
      for (const statement of imports) {
        expect(statement, source.file).toMatch(/from\s+"(\.\/|@figtether\/shared)/);
      }
    }
  });
});
