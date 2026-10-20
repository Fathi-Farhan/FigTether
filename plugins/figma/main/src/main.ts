/**
 * Entry point of the plugin main sandbox.
 *
 * The sandbox never opens a socket and never stores a secret. It listens for
 * the messages of the UI iframe, validates them and answers the allowlisted
 * read operations.
 */

import { resolveFigmaApi } from "./figma-api.js";
import { PluginRuntime } from "./runtime.js";
import { createSandboxTransport } from "./transport.js";

/** Boot the sandbox inside the editor. */
export async function boot(scope: unknown = globalThis): Promise<void> {
  const api = resolveFigmaApi(scope);
  const transport = createSandboxTransport(api, scope);
  const runtime = new PluginRuntime({ api, transport });
  await runtime.start();
}

if (typeof globalThis !== "undefined") {
  void boot().catch(() => {
    // A boot failure never breaks the editor and never leaks details.
  });
}
