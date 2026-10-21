import { afterEach, describe, expect, it, vi } from "vitest";
afterEach(() => {
  vi.unstubAllGlobals();
  vi.resetModules();
});
describe("build-selected local port", () => {
  it("keeps the documented default for source imports", async () => {
    vi.resetModules();
    const shared = await import("../src/defaults");
    expect(shared.LOCAL_BRIDGE_PORT).toBe(38947);
    expect(shared.isCompanionUrl(`http://127.0.0.1:38947/panel#${"x".repeat(43)}`)).toBe(true);
  });
  it("opens only the configured loopback companion", async () => {
    vi.stubGlobal("__FIGTETHER_PORT__", 39147);
    vi.resetModules();
    const shared = await import("../src/defaults");
    expect(shared.LOCAL_BRIDGE_PORT).toBe(39147);
    expect(shared.isCompanionUrl(`http://127.0.0.1:39147/panel#${"x".repeat(43)}`)).toBe(true);
    for (const url of [
      `http://127.0.0.1:38947/panel#${"x".repeat(43)}`,
      `http://example.com:39147/panel#${"x".repeat(43)}`,
      "http://127.0.0.1:39147/panel#short",
    ]) {
      expect(shared.isCompanionUrl(url)).toBe(false);
    }
  });
});
