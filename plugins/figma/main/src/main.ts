/**
 * Entry point of the plugin main sandbox.
 *
 * The sandbox never opens a socket and never stores a secret. It listens for
 * the messages of the UI iframe, validates them and answers the allowlisted
 * read operations.
 */

import { asRecord, resolveFigmaApi } from "./figma-api.js";
import { PluginRuntime } from "./runtime.js";
import { createSandboxTransport } from "./transport.js";

/** Boot the sandbox inside the editor. */
export async function boot(scope: unknown = globalThis): Promise<void> {
  const globals = asRecord(scope);
  const editor = asRecord(globals?.["figma"]);
  const html = globals?.["__html__"];
  if (editor !== null && typeof editor["showUI"] === "function" && typeof html === "string") {
    editor["showUI"].call(editor, html, { width: 420, height: 700, themeColors: true });
  }
  const api = resolveFigmaApi(scope);
  const transport = createSandboxTransport(api, scope);
  const runtime = new PluginRuntime({ api, transport });
  await runtime.start();
}

if (typeof figma !== "undefined") {
  figma.showUI(__html__, { width: 420, height: 700, themeColors: true });
  void boot({ figma }).catch((error: unknown) => {
    console.error(
      "FigTether startup failed:",
      error instanceof Error ? error.message : "unknown error",
    );
    figma.notify("FigTether could not start. Check the development console.", { error: true });
  });
}
