/**
 * Drift guard of the contract of the postMessage channel.
 *
 * The two sides of the channel used to carry their own copy of the contract:
 * the sandbox declared the channel, the envelope and the vocabulary of its
 * answers, the user interface declared the channel, the envelope and the
 * vocabulary of the answers it accepts. The copies drifted apart twice, so
 * the test pins the properties that broke the plugin:
 *
 * - the channel has exactly one value in the TypeScript sources of the three
 *   packages, and the documentation of the two packages names the same one;
 * - the vocabulary of the daemon and the vocabulary of the plugin are
 *   separate lists, and the list of the answers and the list of the accepted
 *   answers agree with each other;
 * - both packages derive their lists from the shared source instead of
 *   repeating them.
 */
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

import {
  BRIDGE_CHANNEL,
  DAEMON_MESSAGE_KINDS,
  INBOUND_MESSAGE_KINDS,
  LOCAL_MESSAGE_KINDS,
  OUTBOUND_MESSAGE_KINDS,
  unwrapPluginMessage,
} from "../src/index";

/** The folder of the shared package. */
const SHARED_PACKAGE = new URL("../", import.meta.url);

/** The folder of the main sandbox. */
const MAIN_PACKAGE = new URL("../../main/", import.meta.url);

/** The folder of the user interface. */
const UI_PACKAGE = new URL("../../ui/", import.meta.url);

/** The folders that hold build output and dependencies, not sources. */
const IGNORED_FOLDERS = new Set(["dist", "node_modules", "types-dist"]);

/** The module of the shared package that holds the contract. */
const SHARED_CONTRACT = fileURLToPath(new URL("../src/bridge.ts", import.meta.url));

/** The module of the main sandbox that holds the bridge. */
const MAIN_BRIDGE = fileURLToPath(new URL("../../main/src/bridge.ts", import.meta.url));

/** The module of the user interface that holds the protocol. */
const UI_PROTOCOL = fileURLToPath(new URL("../../ui/src/protocol.ts", import.meta.url));

/** The documentation of the main sandbox. */
const MAIN_README = fileURLToPath(new URL("../../main/README.md", import.meta.url));

/** The documentation of the user interface. */
const UI_README = fileURLToPath(new URL("../../ui/README.md", import.meta.url));

/** Every TypeScript file of a folder, build output excluded. */
function sourceFiles(directory: string): string[] {
  const files: string[] = [];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      if (!IGNORED_FOLDERS.has(entry.name)) {
        files.push(...sourceFiles(join(directory, entry.name)));
      }
      continue;
    }
    if (entry.isFile() && entry.name.endsWith(".ts")) {
      files.push(join(directory, entry.name));
    }
  }
  return files;
}

/** Number of times a text appears in a source. */
function countOccurrences(content: string, needle: string): number {
  let count = 0;
  let index = content.indexOf(needle);
  while (index !== -1) {
    count += 1;
    index = content.indexOf(needle, index + needle.length);
  }
  return count;
}

/** The literal of the channel, as it appears in a source file. */
const CHANNEL_LITERAL = `"${BRIDGE_CHANNEL}"`;

describe("the contract of the postMessage channel", () => {
  it("carries the channel value in exactly one place of the tree", () => {
    const holders: string[] = [];
    let occurrences = 0;
    for (const root of [SHARED_PACKAGE, MAIN_PACKAGE, UI_PACKAGE]) {
      for (const file of sourceFiles(fileURLToPath(root))) {
        const count = countOccurrences(readFileSync(file, "utf8"), CHANNEL_LITERAL);
        if (count > 0) {
          holders.push(file);
          occurrences += count;
        }
      }
    }
    expect(holders).toEqual([SHARED_CONTRACT]);
    expect(occurrences).toBe(1);
  });

  it("documents the same channel as the code", () => {
    for (const readme of [MAIN_README, UI_README]) {
      expect(readFileSync(readme, "utf8"), readme).toContain(BRIDGE_CHANNEL);
    }
  });

  it("keeps the vocabulary of the daemon and of the plugin apart", () => {
    expect(DAEMON_MESSAGE_KINDS).toEqual([
      "result",
      "error",
      "progress",
      "chunk",
      "capabilities",
      "selection",
    ]);
    expect(LOCAL_MESSAGE_KINDS).toEqual(["grant_verified", "status", "capture_chunk"]);
    for (const kind of DAEMON_MESSAGE_KINDS) {
      expect(LOCAL_MESSAGE_KINDS).not.toContain(kind);
    }
  });

  it("derives the list of the answers and the list of the accepted answers from one source", () => {
    expect(OUTBOUND_MESSAGE_KINDS).toEqual([...DAEMON_MESSAGE_KINDS, ...LOCAL_MESSAGE_KINDS]);
    expect(INBOUND_MESSAGE_KINDS).toEqual(DAEMON_MESSAGE_KINDS);
    expect(OUTBOUND_MESSAGE_KINDS.length).toBe(
      DAEMON_MESSAGE_KINDS.length + LOCAL_MESSAGE_KINDS.length,
    );
  });

  it("keeps the lists of the packages derived from the shared source", () => {
    const main = readFileSync(MAIN_BRIDGE, "utf8");
    const ui = readFileSync(UI_PROTOCOL, "utf8");

    // Both packages name the shared constants.
    expect(main).toContain("OUTBOUND_MESSAGE_KINDS");
    expect(ui).toContain("INBOUND_MESSAGE_KINDS");

    // Neither package repeats the vocabulary of the shared lists.
    for (const kind of LOCAL_MESSAGE_KINDS) {
      expect(main.includes(`"${kind}"`), `the sandbox repeats ${kind}`).toBe(false);
    }
    for (const kind of ["result", "error", "progress", "chunk", "selection"]) {
      expect(ui.includes(`"${kind}"`), `the interface repeats ${kind}`).toBe(false);
    }
  });
});

describe("unwrapPluginMessage", () => {
  it("removes the wrapper of the editor", () => {
    expect(unwrapPluginMessage({ pluginMessage: { channel: BRIDGE_CHANNEL } })).toEqual({
      channel: BRIDGE_CHANNEL,
    });
  });

  it("accepts a message that arrives raw", () => {
    const raw = { channel: BRIDGE_CHANNEL, kind: "result" };
    expect(unwrapPluginMessage(raw)).toBe(raw);
  });

  it("accepts a value that is not a message at all", () => {
    expect(unwrapPluginMessage(null)).toBeNull();
    expect(unwrapPluginMessage(undefined)).toBeUndefined();
    expect(unwrapPluginMessage(42)).toBe(42);
    expect(unwrapPluginMessage("text")).toBe("text");
    expect(unwrapPluginMessage([1, 2, 3])).toEqual([1, 2, 3]);
  });
});
