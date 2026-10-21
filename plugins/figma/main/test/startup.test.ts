import { describe, expect, it, vi } from "vitest";
import { boot } from "../src/main.js";
import { resolveFigmaApi } from "../src/figma-api.js";

describe("desktop startup", () => {
  it("treats a permission-protected optional API as unavailable", () => {
    const api = resolveFigmaApi({
      figma: {
        root: { id: "0:0" },
        currentPage: { id: "0:1" },
        get teamLibrary() {
          throw new Error("teamlibrary permission not specified");
        },
      },
    });
    expect(api.teamLibrary).toBeUndefined();
  });
  it("opens the bundled UI and installs its message receiver", async () => {
    const showUI = vi.fn();
    const ui = { postMessage: vi.fn(), onmessage: undefined };
    await boot({
      figma: {
        root: { id: "0:0", type: "DOCUMENT", children: [] },
        currentPage: { id: "0:1", type: "PAGE", selection: [], children: [] },
        showUI,
        ui,
      },
      __html__: "<html>bridge</html>",
    });
    expect(showUI).toHaveBeenCalledWith("<html>bridge</html>", {
      width: 420,
      height: 700,
      themeColors: true,
    });
    expect(ui.onmessage).toBeTypeOf("function");
  });
});
