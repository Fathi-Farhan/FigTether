import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

const css = readFileSync(fileURLToPath(new URL("../styles.css", import.meta.url)), "utf8");

interface Palette {
  readonly name: string;
  readonly colors: Map<string, string>;
}

function parsePalette(name: string, block: string): Palette {
  const colors = new Map<string, string>();
  const pattern = /--figtether-([a-z-]+):\s*(#[0-9a-fA-F]{6})\s*;/g;
  let match = pattern.exec(block);
  while (match !== null) {
    const key = match[1] as string;
    const value = match[2] as string;
    colors.set(key, value);
    match = pattern.exec(block);
  }
  return { name, colors };
}

function palettes(): Palette[] {
  const parts = css.split(/\/\* palette: (light|dark) \*\//);
  const result: Palette[] = [];
  for (let index = 1; index + 1 < parts.length; index += 2) {
    const name = parts[index] as string;
    const block = parts[index + 1] as string;
    result.push(parsePalette(name, block));
  }
  return result;
}

function luminance(hex: string): number {
  const channel = (offset: number): number => {
    const value = parseInt(hex.slice(offset, offset + 2), 16) / 255;
    return value <= 0.03928 ? value / 12.92 : Math.pow((value + 0.055) / 1.055, 2.4);
  };
  return 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5);
}

function contrast(first: string, second: string): number {
  const a = luminance(first);
  const b = luminance(second);
  const lighter = Math.max(a, b);
  const darker = Math.min(a, b);
  return (lighter + 0.05) / (darker + 0.05);
}

/** Pairs that carry text and the threshold of the WCAG AA target. */
const TEXT_PAIRS: readonly [string, string, number][] = [
  ["fg", "bg", 4.5],
  ["fg", "bg-raised", 4.5],
  ["fg-muted", "bg", 4.5],
  ["fg-muted", "bg-raised", 4.5],
  ["accent", "bg", 4.5],
  ["accent", "bg-raised", 4.5],
  ["accent-fg", "accent", 4.5],
  ["danger", "bg", 4.5],
  ["danger", "bg-raised", 4.5],
  ["danger", "danger-bg", 4.5],
  ["ok", "bg", 4.5],
  ["ok", "bg-raised", 4.5],
  ["ok", "ok-bg", 4.5],
];

/** Pairs that carry a boundary or a focus ring. */
const UI_PAIRS: readonly [string, string, number][] = [
  ["border", "bg", 3],
  ["border", "bg-raised", 3],
  ["focus", "bg", 3],
  ["focus", "bg-raised", 3],
];

describe("stylesheet palettes", () => {
  const all = palettes();

  it("declares a light and a dark palette", () => {
    expect(all.map((palette) => palette.name)).toEqual(["light", "dark"]);
  });

  it("declares the same color tokens in both palettes", () => {
    const light = all[0] as Palette;
    const dark = all[1] as Palette;
    expect([...light.colors.keys()].sort()).toEqual([...dark.colors.keys()].sort());
  });

  for (const palette of all) {
    it(`keeps the text contrast of the ${palette.name} palette at the AA target`, () => {
      for (const [foreground, background, threshold] of TEXT_PAIRS) {
        const first = palette.colors.get(foreground);
        const second = palette.colors.get(background);
        expect(first, `${palette.name}: ${foreground} is missing`).toBeDefined();
        expect(second, `${palette.name}: ${background} is missing`).toBeDefined();
        const ratio = contrast(first as string, second as string);
        expect(
          ratio,
          `${palette.name}: ${foreground} on ${background} has a contrast of ${ratio.toFixed(2)}`,
        ).toBeGreaterThanOrEqual(threshold);
      }
    });

    it(`keeps the control contrast of the ${palette.name} palette at the AA target`, () => {
      for (const [foreground, background, threshold] of UI_PAIRS) {
        const first = palette.colors.get(foreground);
        const second = palette.colors.get(background);
        expect(first).toBeDefined();
        expect(second).toBeDefined();
        const ratio = contrast(first as string, second as string);
        expect(
          ratio,
          `${palette.name}: ${foreground} on ${background} has a contrast of ${ratio.toFixed(2)}`,
        ).toBeGreaterThanOrEqual(threshold);
      }
    });
  }

  it("never removes the focus outline", () => {
    expect(css).toContain(":focus-visible");
    expect(css).not.toMatch(/outline:\s*(none|0)\b/);
  });

  it("does not load an external asset or a remote font", () => {
    expect(css).not.toMatch(/@import/);
    expect(css).not.toMatch(/url\(\s*['"]?https?:/);
    expect(css).not.toMatch(/https?:\/\/[^)]*\.(woff2?|ttf|otf|eot)/);
  });
});
