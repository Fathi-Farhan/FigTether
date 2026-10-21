import { AppController, type UiModel } from "../../../plugins/figma/ui/src/controller.js";
import { DomUi } from "../../../plugins/figma/ui/src/dom.js";
import {
  FakeBridge,
  FakeSandbox,
  acceptHandshake,
  bridgeFrame,
  sandboxEnvelope,
} from "../../../plugins/figma/ui/test/fakes.js";
let controller: AppController;
let bridge: FakeBridge;
let current: UiModel;
let number = 0;
const dom = new DomUi(document, {
  onPair: (s) => controller.onPairRequested(s),
  onSelectionChanged: (ids) => controller.onRootSelectionChanged(ids),
  onSelectionRefresh: () => undefined,
  onAllow: () => controller.onAllowRequested(),
  onWriteGrant: () => controller.onWriteGrantRequested(),
  onAutoMode: (x) => controller.onAutoModeChanged(x),
  onExternalPanel: () => controller.onExternalPanelRequested(),
  onPause: () => controller.onPauseRequested(),
  onResume: () => controller.onResumeRequested(),
  onCapture: () => undefined,
  onRevoke: () => controller.onRevokeRequested(),
  onRevokeConfirm: () => controller.onRevokeConfirmed(),
  onRevokeCancel: () => controller.onRevokeCancelled(),
  onApproveWrite: () => controller.onApproveWriteRequested(),
  onRejectWrite: (x) => controller.onRejectWriteRequested(x),
});
controller = new AppController({
  sandbox: new FakeSandbox(),
  ui: {
    render: (m) => {
      current = m;
      dom.render(m);
    },
    appendLog: (l) => dom.appendLog(l),
    setActionResult: (...args) => dom.setActionResult(...args),
    setPairError: (x) => dom.setPairError(x),
    setRevokeConfirmVisible: (x) => dom.setRevokeConfirmVisible(x),
    focusPairInput: () => dom.focusPairInput(),
  },
  bridgeFactory: (handlers) => {
    bridge = new FakeBridge(handlers);
    return bridge;
  },
});
controller.start();
const fixture = {
  connect() {
    acceptHandshake(bridge);
    controller.onSandboxEnvelope(
      sandboxEnvelope("selection", {
        roots: [{ node_id: "1:2", name: "Website Test", node_type: "FRAME", child_count: 71 }],
      }),
    );
  },
  preview(warnings: string[] = []) {
    const id = `fixture-${++number}`;
    bridge.handlers.onFrame(
      bridgeFrame(
        "request",
        {
          op: "preview_write",
          params: {
            approval_id: id,
            plan: {
              plan_id: id,
              intent: "Refine the hero heading and spacing",
              operations: [{ kind: "set_properties", node_id: "1:2", properties: { width: 1440 } }],
            },
          },
        },
        id,
      ),
    );
    controller.onSandboxEnvelope(
      sandboxEnvelope(
        "preview",
        {
          preview: {
            plan_id: id,
            plan_hash: `sha256:fixture-${number}`,
            blocked: false,
            warnings,
            removed: [],
            created: [],
            changed: ["1:2"],
            moved: [],
            operations_count: 1,
          },
        },
        id,
      ),
    );
  },
  logs() {
    for (let i = 0; i < 205; i++)
      bridge.handlers.onLog({ operation: "list_nodes", count: 50, durationMs: 62 });
  },
  model: () => current,
  approvals: () => bridge.approvals(),
};
(window as unknown as { fixture: typeof fixture }).fixture = fixture;
