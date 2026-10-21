#!/usr/bin/env node
/**
 * Build the FigTether Figma plugin.
 *
 * esbuild bundles the two plugin entries into `plugins/figma/dist`, the UI
 * document is copied next to the bundle and the development manifest is
 * written with paths that resolve inside `dist`.
 *
 * Usage:
 *   node build.mjs            one build
 *   node build.mjs --watch    rebuild on every change
 */
import { build, context } from "esbuild";
import { existsSync } from "node:fs";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const distDir = path.join(here, "dist");
const manifestSource = path.join(here, "manifest.json");
const watch = process.argv.includes("--watch");
const localManifestPath = path.join(here, "manifest.local.json");
const localManifest = existsSync(localManifestPath)
  ? JSON.parse(await readFile(localManifestPath, "utf8"))
  : null;
const localPort = localManifest?.networkAccess?.devAllowedDomains
  ?.map((domain) => /^ws:\/\/localhost:(\d+)$/.exec(domain)?.[1])
  .find(Boolean);
const selectedPort = process.env.FIGTETHER_PORT ?? localPort ?? "38947";
if (!/^\d{1,5}$/.test(selectedPort) || Number(selectedPort) < 1 || Number(selectedPort) > 65535)
  throw Error("FIGTETHER_PORT must be an integer from 1 to 65535");
const bridgePort = Number(selectedPort);

/**
 * Entry points of the plugin.
 *
 * The first candidate of every list is the canonical location, the others are
 * accepted alternatives so a renamed entry does not silently break the build.
 */
const ENTRIES = [
  {
    name: "main",
    label: "main sandbox",
    candidates: ["main/src/main.ts", "main/src/index.ts"],
    outfile: path.join(distDir, "main.js"),
  },
  {
    name: "ui",
    label: "UI iframe",
    candidates: ["ui/src/ui.ts", "ui/src/main.ts", "ui/src/index.ts"],
    outfile: path.join(distDir, "ui.js"),
  },
];

/** Candidate locations of the UI document. */
const HTML_CANDIDATES = ["ui/src/index.html", "ui/index.html"];

/** Script sources that are rewritten to the bundle that this build produces. */
const SCRIPT_SOURCES = ["./dist/ui.js", "dist/ui.js", "./ui.js", "ui.js"];

function relative(absolute) {
  return path.relative(here, absolute).split(path.sep).join("/");
}

function firstExisting(candidates) {
  for (const candidate of candidates) {
    const absolute = path.join(here, candidate);
    if (existsSync(absolute)) {
      return { candidate, absolute };
    }
  }
  return null;
}

/**
 * Resolve every entry point.
 *
 * @throws {Error} when a source entry is missing, with the searched locations.
 */
function resolveEntries() {
  const resolved = [];
  const missing = [];
  for (const entry of ENTRIES) {
    const found = firstExisting(entry.candidates);
    if (found === null) {
      missing.push({ entry, searched: entry.candidates.map(relative) });
      continue;
    }
    resolved.push({ ...entry, entryPoint: found.absolute });
  }
  if (missing.length > 0) {
    const details = missing
      .map((item) => `  ${item.entry.label}: no file at ${item.searched.join(", ")}`)
      .join("\n");
    throw new Error(
      [
        `build failed: missing source ${missing.length === 1 ? "entry" : "entries"}.`,
        details,
        "  create the file, or add the new location to the candidate list in plugins/figma/build.mjs.",
      ].join("\n"),
    );
  }
  return resolved;
}

/** Build the esbuild options for one entry. */
function buildOptions(entry) {
  return {
    entryPoints: [entry.entryPoint],
    outfile: entry.outfile,
    bundle: true,
    format: "iife",
    platform: "browser",
    target: "es2020",
    sourcemap: true,
    logLevel: "warning",
    define: { __FIGTETHER_PORT__: String(bridgePort) },
    alias: {
      "@figtether/shared": path.join(here, "shared", "src", "index.ts"),
    },
  };
}

/** Copy the UI document and point it at the bundle of this build. */
async function copyUiDocument() {
  const found = firstExisting(HTML_CANDIDATES);
  if (found === null) {
    throw new Error(
      [
        "build failed: missing source entry for the UI document.",
        `  no file at ${HTML_CANDIDATES.map(relative).join(", ")}`,
        "  create the file, or add the new location to the candidate list in plugins/figma/build.mjs.",
      ].join("\n"),
    );
  }
  const original = await readFile(found.absolute, "utf8");
  let html = original;
  for (const source of SCRIPT_SOURCES) {
    html = html.split(`src="${source}"`).join('src="./ui.js"');
  }
  if (!html.includes('src="./ui.js"')) {
    console.warn(
      `warning: ${relative(found.absolute)} does not load ./ui.js, the UI bundle will not run`,
    );
  }
  // showUI receives an HTML string, not a document hosted beside the bundles.
  // Relative script/style URLs therefore cannot load inside the plugin iframe.
  const script = await readFile(path.join(distDir, "ui.js"), "utf8");
  const css = await readFile(path.join(distDir, "ui.css"), "utf8");
  html = html.replace(
    /<script\b[^>]*src="\.\/ui\.js"[^>]*><\/script>/g,
    () =>
      `<script>${script.replace(/\/\/# sourceMappingURL=.*$/gm, "").replace(/<\/script/gi, "<\\/script")}</script>`,
  );
  html = html.replace(
    /<link\b[^>]*href="\.\/ui\.css"[^>]*>/g,
    () => `<style>${css.replace(/\/\*# sourceMappingURL=.*?\*\//g, "")}</style>`,
  );
  // The Figma iframe has no relative asset host. Keep the small local icon
  // inside the HTML, just like the bundled stylesheet and script.
  const icon = await readFile(path.join(here, "ui/assets/figtether-icon.png"));
  html = html.replace(
    'src="./assets/figtether-icon.png"',
    `src="data:image/png;base64,${icon.toString("base64")}"`,
  );
  await writeFile(path.join(distDir, "ui.html"), html, "utf8");
  return relative(found.absolute);
}

/** Write the manifest of the build with paths that resolve inside `dist`. */
async function writeManifest() {
  if (!existsSync(manifestSource)) {
    throw new Error(`build failed: missing source entry ${relative(manifestSource)}`);
  }
  const manifest = JSON.parse(await readFile(manifestSource, "utf8"));
  const built = {
    ...manifest,
    main: "main.js",
    ui: "ui.html",
    networkAccess: {
      ...manifest.networkAccess,
      devAllowedDomains: [`http://localhost:${bridgePort}`, `ws://localhost:${bridgePort}`],
    },
  };
  await writeFile(
    path.join(distDir, "manifest.json"),
    `${JSON.stringify(built, null, 2)}\n`,
    "utf8",
  );
  return built;
}

async function runBuild() {
  await mkdir(distDir, { recursive: true });
  const entries = resolveEntries();
  for (const entry of entries) {
    await build(buildOptions(entry));
    console.log(`built ${relative(entry.outfile)} from ${relative(entry.entryPoint)}`);
  }
  const html = await copyUiDocument();
  console.log(`copied ${html} to dist/ui.html`);
  const manifest = await writeManifest();
  console.log(`wrote dist/manifest.json (plugin id "${manifest.id}")`);
}

async function runWatch() {
  await mkdir(distDir, { recursive: true });
  const entries = resolveEntries();
  const contexts = [];
  for (const entry of entries) {
    const ctx = await context(buildOptions(entry));
    await ctx.watch();
    contexts.push(ctx);
    console.log(`watching ${relative(entry.entryPoint)}`);
  }

  const copyAssets = async () => {
    try {
      const html = await copyUiDocument();
      await writeManifest();
      console.log(`updated dist/ui.html from ${html}`);
    } catch (error) {
      console.error(`error: ${error instanceof Error ? error.message : String(error)}`);
    }
  };

  for (const candidate of [...HTML_CANDIDATES, "manifest.json", "ui/assets/figtether-icon.png"]) {
    const absolute = path.join(here, candidate);
    if (!existsSync(absolute)) {
      continue;
    }
    const { watch: watchFile } = await import("node:fs");
    watchFile(absolute, () => {
      void copyAssets();
    });
  }

  await copyAssets();
  console.log("watching for changes, press Ctrl+C to stop");
}

try {
  if (watch) {
    await runWatch();
  } else {
    await runBuild();
  }
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
}
