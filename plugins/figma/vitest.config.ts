import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

const here = path.dirname(fileURLToPath(import.meta.url));

/**
 * Test configuration for the whole plugin package.
 *
 * The UI folder keeps its own configuration because it runs in a browser like
 * environment, everything else runs in node. The shared package is imported
 * through the same path mapping that the compiler and esbuild use.
 */
export default defineConfig({
  resolve: {
    alias: {
      "@figtether/shared": path.join(here, "shared", "src", "index.ts"),
    },
  },
  test: {
    environment: "node",
    include: ["shared/**/*.test.ts", "main/**/*.test.ts", "ui/**/*.test.ts"],
    exclude: ["**/node_modules/**", "**/dist/**"],
    passWithNoTests: false,
    reporters: ["default"],
  },
});
