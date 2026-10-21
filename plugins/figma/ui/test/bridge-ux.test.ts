import { describe, expect, it } from "vitest";
import { acceptHandshake, bridgeFrame, harness, sandboxEnvelope } from "./fakes.js";
import { readWritePreview } from "../src/write.js";
import { readableActivity } from "../src/dom.js";

function ready() {
  const h = harness();
  acceptHandshake(h.bridge);
  h.controller.onSandboxEnvelope(
    sandboxEnvelope("selection", {
      roots: [{ node_id: "1:2", name: "Website Test", node_type: "FRAME", child_count: 2 }],
    }),
  );
  h.controller.onAllowRequested();
  h.controller.onWriteGrantRequested();
  return h;
}
const clean = {
  plan_id: "p",
  plan_hash: "sha256:abcdef",
  blocked: false,
  warnings: [],
  created: [],
  changed: [],
  moved: [],
  removed: [],
  operations_count: 1,
};
function preview(
  h: ReturnType<typeof ready>,
  overrides: Record<string, unknown> = {},
  id = "approval-one",
) {
  h.bridge.handlers.onFrame(
    bridgeFrame(
      "request",
      {
        op: "preview_write",
        params: {
          approval_id: id,
          plan: {
            plan_id: "p",
            operations: [{ kind: "create_node", parent_id: "1:2", node_type: "FRAME" }],
          },
        },
      },
      id,
    ),
  );
  h.controller.onSandboxEnvelope(
    sandboxEnvelope("preview", { preview: { ...clean, ...overrides } }, id),
  );
}
describe("user-facing Bridge flow", () => {
  it("keeps new controls disabled when the daemon has not advertised support", () => {
    const h = ready();
    h.bridge.handlers.onAuthAccepted({
      session_id: "session-01",
      session_credential: "test",
      connection_epoch: 2,
      grant_epoch: 2,
      capabilities: {},
      session_name: null,
    });
    h.controller.onAutoModeChanged(true);
    h.controller.onExternalPanelRequested();
    expect(h.ui.model?.autoMode).toBe(false);
    expect(h.ui.model?.canOpenPanel).toBe(false);
    expect(
      h.bridge.frames.some(
        (f) => f.type === "ui_control" && (f.payload as { action?: string }).action === "open",
      ),
    ).toBe(false);
  });
  it("shares the selected frame by name without requiring users to understand root IDs", () => {
    const h = ready();
    expect(h.ui.model?.sharedFrameNames).toEqual(["Website Test"]);
    expect(h.ui.model?.grantedRootCount).toBe(1);
  });
  it("leaves manual mode on by default and never auto-approves an existing proposal", () => {
    const h = ready();
    preview(h);
    expect(h.bridge.approvals()).toHaveLength(0);
    h.controller.onAutoModeChanged(true);
    expect(h.bridge.approvals()).toHaveLength(0);
    preview(h, {}, "approval-two");
    expect(h.bridge.approvals()).toHaveLength(1);
    expect(h.bridge.approvals()[0]?.automatic).toBe(true);
  });
  it.each([
    { warnings: ["Missing font"] },
    { blocked: true },
    { removed: ["1:8"] },
    { warnings: null },
    { blocked: undefined },
    { removed: undefined },
    { operations_count: 0 },
  ])("pauses auto mode and sends no approval for %j", (changes) => {
    const h = ready();
    h.controller.onAutoModeChanged(true);
    preview(h, changes);
    expect(h.bridge.approvals()).toHaveLength(0);
    expect(h.ui.model?.autoMode).toBe(false);
  });
  it("does not approve duplicate delivery of a plan twice", () => {
    const h = ready();
    h.controller.onAutoModeChanged(true);
    preview(h);
    preview(h);
    expect(h.bridge.approvals()).toHaveLength(1);
  });
  it.each(["offline", "revoked"] as const)("turns auto mode off after %s", (state) => {
    const h = ready();
    h.controller.onAutoModeChanged(true);
    h.bridge.handlers.onConnection(state, null);
    expect(h.ui.model?.autoMode).toBe(false);
  });
  it("turns auto mode off when editing is disabled or the shared frames change", () => {
    const h = ready();
    h.controller.onAutoModeChanged(true);
    h.controller.onWriteGrantRequested();
    expect(h.ui.model?.autoMode).toBe(false);
    h.controller.onWriteGrantRequested();
    h.controller.onAutoModeChanged(true);
    h.controller.onAllowRequested();
    expect(h.ui.model?.autoMode).toBe(false);
  });
  it("will not enable auto mode with read-only access", () => {
    const h = harness();
    acceptHandshake(h.bridge);
    h.controller.onAutoModeChanged(true);
    expect(h.ui.model?.autoMode).toBe(false);
  });
  it("rejects a stale companion approval after the proposal changes", () => {
    const h = ready();
    h.bridge.handlers.onFrame(
      bridgeFrame("ui_control", {
        action: "opened",
        url: `http://127.0.0.1:38947/panel#${"x".repeat(43)}`,
      }),
    );
    preview(h, {}, "old");
    preview(h, { plan_hash: "sha256:new" }, "new");
    h.bridge.handlers.onFrame(
      bridgeFrame("ui_control", {
        action: "command",
        command: "approve",
        grant_epoch: h.ui.model?.grantEpoch,
        value: { planHash: clean.plan_hash, approvalId: "old" },
      }),
    );
    expect(h.bridge.approvals()).toHaveLength(0);
  });
  it("does not grant access through a companion action from an obsolete grant epoch", () => {
    const h = ready();
    h.bridge.handlers.onFrame(
      bridgeFrame("ui_control", {
        action: "opened",
        url: `http://127.0.0.1:38947/panel#${"x".repeat(43)}`,
      }),
    );
    h.bridge.handlers.onFrame(
      bridgeFrame("ui_control", {
        action: "command",
        command: "auto",
        grant_epoch: 0,
        value: true,
      }),
    );
    expect(h.ui.model?.autoMode).toBe(false);
  });
  it("opens only a daemon-issued local companion URL", () => {
    const h = ready();
    h.bridge.handlers.onFrame(
      bridgeFrame("ui_control", { action: "opened", url: "https://example.com/panel#untrusted" }),
    );
    expect(h.sandbox.lastOf("open_external")).toBeUndefined();
  });
  it("keeps missing preview checks fail-closed", () => {
    expect(readWritePreview({ ...clean, warnings: undefined })?.autoApprovalSafe).toBe(false);
    expect(readWritePreview({ ...clean, reasons: ["Needs review"] })?.autoApprovalSafe).toBe(false);
  });
  it("formats activity without numbered technical rows or untrusted free text", () => {
    expect(readableActivity("12:30:00 op=list_nodes count=50 duration=62ms")).toBe(
      "12:30:00  Layers read · 50 items · 62 ms",
    );
    expect(readableActivity("a credential or document text")).toBe("Activity received");
  });
});
