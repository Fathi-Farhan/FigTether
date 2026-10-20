import path from "node:path";
import { fileURLToPath } from "node:url";

import { defineConfig } from "vitest/config";

const here = path.dirname(fileURLToPath(import.meta.url));

/**
 * Test configuration of the user interface.
 *
 * The shared package is imported through the same path mapping that the
 * compiler and esbuild use, so every side of the plugin sees one contract.
 */
export default defineConfig({
  resolve: {
    alias: {
      "@figtether/shared": path.join(here, "..", "shared", "src", "index.ts"),
    },
  },
  test: {
    environment: "node",
    include: ["test/**/*.test.ts"],
    exclude: ["**/node_modules/**", "**/dist/**", "**/types-dist/**"],
    passWithNoTests: false,
    reporters: ["default"],
  },
});
