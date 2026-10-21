#!/usr/bin/env node
/**
 * Typecheck driver for the FigTether Figma plugin.
 *
 * The plugin is a solution with three projects (shared, main, ui). The root
 * `tsconfig.json` carries the shared compiler options and the project
 * references, but a project reference requires every referenced project to
 * exist. This driver therefore typechecks each project that is present, with
 * the settings of that project, and reports one line per project.
 *
 * Usage:
 *   node typecheck.mjs
 */
import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const tsc = path.join(here, "node_modules", "typescript", "bin", "tsc");

/** Projects that are typechecked, in dependency order. */
const PROJECTS = [
  { name: "shared", config: path.join(here, "shared", "tsconfig.json") },
  { name: "shared (tests)", config: path.join(here, "shared", "test", "tsconfig.json") },
  { name: "main", config: path.join(here, "main", "tsconfig.json") },
  { name: "ui", config: path.join(here, "ui", "tsconfig.json") },
  { name: "tooling", config: path.join(here, "tsconfig.tools.json") },
];

function runTsc(project) {
  const arguments_ =
    project.name === "shared"
      ? [tsc, "--build", "--force", project.config]
      : [tsc, "--noEmit", "-p", project.config];
  const result = spawnSync(process.execPath, arguments_, {
    cwd: here,
    encoding: "utf8",
  });
  const output = `${result.stdout ?? ""}${result.stderr ?? ""}`.trim();
  if (output.length > 0) {
    process.stdout.write(`${output}\n`);
  }
  if (result.status === 0) {
    console.log(`  ok    ${project.name}`);
    return true;
  }
  console.log(`  FAIL  ${project.name}`);
  if (output.includes("TS6307")) {
    console.log("        hint: a referenced project must list the shared sources, or import them");
    console.log("        through the @figtether/shared path mapping of the root tsconfig.");
  }
  if (output.includes("TS6305")) {
    console.log("        hint: run `npm run build:types` so the shared declarations exist");
  }
  return false;
}

let failures = 0;
let checked = 0;

console.log("typecheck: FigTether Figma plugin");
for (const project of PROJECTS) {
  if (!existsSync(project.config)) {
    console.log(`  skip  ${project.name} (no ${path.relative(here, project.config)})`);
    continue;
  }
  checked += 1;
  if (!runTsc(project)) {
    failures += 1;
  }
}

if (checked === 0) {
  console.error("typecheck: no project configuration was found");
  process.exit(1);
}

if (failures > 0) {
  console.error(`typecheck: ${failures} of ${checked} projects failed`);
  process.exit(1);
}

console.log(`typecheck: ${checked} projects ok`);
