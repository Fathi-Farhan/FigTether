import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

const markup = readFileSync(fileURLToPath(new URL("../index.html", import.meta.url)), "utf8");
const uiSource = readFileSync(fileURLToPath(new URL("../src/dom.ts", import.meta.url)), "utf8");

function idsOf(html: string): string[] {
  const ids: string[] = [];
  const pattern = /\sid="([^"]+)"/g;
  let match = pattern.exec(html);
  while (match !== null) {
    ids.push(match[1] as string);
    match = pattern.exec(html);
  }
  return ids;
}

function requiredIds(): string[] {
  const ids: string[] = [];
  const pattern = /requireElement(?:<[^>]+>)?\(doc,\s*"([^"]+)"\)/g;
  let match = pattern.exec(uiSource);
  while (match !== null) {
    ids.push(match[1] as string);
    match = pattern.exec(uiSource);
  }
  return ids;
}

describe("markup", () => {
  it("declares every element that the interface renders into", () => {
    const present = new Set(idsOf(markup));
    for (const id of requiredIds()) {
      expect(present.has(id), `the element ${id} is missing in index.html`).toBe(true);
    }
  });

  it("uses every element identifier only once", () => {
    const ids = idsOf(markup);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("binds every label to an existing control", () => {
    const present = new Set(idsOf(markup));
    const pattern = /<label[^>]*\sfor="([^"]+)"/g;
    let match = pattern.exec(markup);
    let labels = 0;
    while (match !== null) {
      labels += 1;
      expect(present.has(match[1] as string), `the label target ${match[1]} is missing`).toBe(true);
      match = pattern.exec(markup);
    }
    expect(labels).toBeGreaterThan(0);
  });

  it("marks the status areas as live regions", () => {
    expect(markup).toContain('role="status"');
    expect(markup).toContain('aria-live="polite"');
    expect(markup).toContain('role="alert"');
    expect(markup).toContain('role="log"');
  });

  it("names every section with a heading", () => {
    const sections = markup.match(/<section[^>]*>/g) ?? [];
    expect(sections.length).toBeGreaterThan(0);
    for (const section of sections) {
      expect(section).toContain("aria-labelledby");
    }
  });

  it("keeps the title and the language of the document", () => {
    expect(markup).toContain('<html lang="en">');
    expect(markup).toContain("<title>FigTether</title>");
  });

  it("loads no external asset", () => {
    expect(markup).not.toMatch(/<script[^>]+src="https?:/);
    expect(markup).not.toMatch(/<link[^>]+href="https?:/);
  });
});
