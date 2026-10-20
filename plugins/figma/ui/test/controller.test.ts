import { base64Encode, sha256Hex } from "@figtether/shared";
import { describe, expect, it } from "vitest";

import type { CapabilityView, UiModel } from "../src/controller.js";
import type { BridgeFrame } from "../src/protocol.js";
import { acceptHandshake, bridgeFrame, harness, sandboxEnvelope } from "./fakes.js";

describe("AppController", () => {
  it("announces itself and asks for the candidate roots", () => {
    const { controller, sandbox } = harness();
    controller.start();
    const types = sandbox.posted.map((entry) => entry.type);
    expect(types).toContain("capabilities");
    expect(types).toContain("selection_request");
  });

  it("refuses an empty pairing secret without touching the socket", () => {
    const { controller, ui, bridge } = harness();
    controller.onPairRequested("   ");
    expect(ui.pairError).not.toBeNull();
    expect(ui.focused).toBe(true);
    expect(bridge.pairedWith).toBeNull();
  });

  it("passes the secret to the bridge and clears it from memory", () => {
    const { controller, bridge, secret } = harness();
    controller.onPairRequested("  pairing-secret-value  ");
    expect(bridge.pairedWith).toBe("pairing-secret-value");
    expect(secret.read()).toBeNull();
  });

  it("shows the session and the fingerprint after the handshake", () => {
    const { controller, ui, bridge } = harness();
    controller.onPairRequested("pairing-secret-value");
    acceptHandshake(bridge);
    const model = ui.model as UiModel;
    expect(model.sessionName).toBe("Design review");
    expect(model.sessionFingerprint).not.toBe("Not available");
    expect(model.sessionFingerprint).not.toContain("session-01");
    expect(model.phase).toBe("awaiting_grant");
    expect(model.capabilities).toHaveLength(1);
  });

  it("names the recovery action when the handshake is refused", () => {
    const { controller, ui, bridge } = harness();
    controller.onPairRequested("pairing-secret-value");
    bridge.handlers.onAuthRejected("AUTH_EXPIRED", "The secret expired.");
    expect(ui.pairError).toContain("AUTH_EXPIRED");
    expect(ui.pairError).toContain("figtether pair");
  });

  it("grants the roots that the user checked", () => {
    const { controller, ui, sandbox, bridge } = harness();
    controller.onPairRequested("pairing-secret-value");
    acceptHandshake(bridge);
    controller.onSandboxEnvelope(
      sandboxEnvelope("selection", {
        roots: [
          { node_id: "1:2", node_type: "FRAME", child_count: 3 },
          { node_id: "1:9", node_type: "GROUP", child_count: 0 },
        ],
      }),
    );
    controller.onRootSelectionChanged(["1:2"]);
    expect((ui.model as UiModel).canGrant).toBe(true);

    controller.onAllowRequested();
    const grant = sandbox.lastOf("grant_update");
    expect(grant).toBeDefined();
    expect((grant as { payload: { root_ids: string[] } }).payload.root_ids).toEqual(["1:2"]);

    const updates = bridge.grantUpdates();
    expect(updates).toHaveLength(1);
    expect(updates[0]?.root_ids).toEqual(["1:2"]);
    expect(updates[0]?.mode).toBe("live");
    expect((ui.model as UiModel).grantedRootCount).toBe(1);
  });

  it("never widens the grant when the selection changes later", () => {
    const { controller, ui, sandbox, bridge } = harness();
    controller.onPairRequested("pairing-secret-value");
    acceptHandshake(bridge);
    controller.onSandboxEnvelope(
      sandboxEnvelope("selection", { roots: [{ node_id: "1:2", node_type: "FRAME", child_count: 0 }] }),
    );
    controller.onRootSelectionChanged(["1:2"]);
    controller.onAllowRequested();
    expect((ui.model as UiModel).grantedRootCount).toBe(1);

    controller.onSandboxEnvelope(
      sandboxEnvelope("selection", {
        roots: [
          { node_id: "1:2", node_type: "FRAME", child_count: 0 },
          { node_id: "9:9", node_type: "FRAME", child_count: 0 },
        ],
      }),
    );
    controller.onRootSelectionChanged(["1:2", "9:9"]);
    expect((ui.model as UiModel).grantedRootCount).toBe(1);
    expect(bridge.grantUpdates()).toHaveLength(1);
    expect(sandbox.posted.filter((entry) => entry.type === "grant_update")).toHaveLength(1);
  });

  it("pauses and resumes the session", () => {
    const { controller, ui, bridge } = harness();
    controller.onPairRequested("pairing-secret-value");
    acceptHandshake(bridge);
    controller.onSandboxEnvelope(
      sandboxEnvelope("selection", { roots: [{ node_id: "1:2", node_type: "FRAME", child_count: 0 }] }),
    );
    controller.onRootSelectionChanged(["1:2"]);
    controller.onAllowRequested();
    expect((ui.model as UiModel).canPause).toBe(true);

    controller.onPauseRequested();
    expect(bridge.grantUpdates()[1]?.mode).toBe("paused");
    expect((ui.model as UiModel).canResume).toBe(true);
    expect((ui.model as UiModel).canPause).toBe(false);

    controller.onResumeRequested();
    expect(bridge.grantUpdates()[2]?.mode).toBe("live");
    expect((ui.model as UiModel).canPause).toBe(true);
  });

  it("raises the grant epoch on every scope change", () => {
    const { controller, bridge } = harness();
    controller.onPairRequested("pairing-secret-value");
    acceptHandshake(bridge);
    controller.onSandboxEnvelope(
      sandboxEnvelope("selection", { roots: [{ node_id: "1:2", node_type: "FRAME", child_count: 0 }] }),
    );
    controller.onRootSelectionChanged(["1:2"]);
    controller.onAllowRequested();
    controller.onPauseRequested();
    controller.onResumeRequested();
    const updates = bridge.grantUpdates();
    expect(updates[0]?.grant_epoch).toBe(1);
    expect(updates[1]?.grant_epoch).toBe(2);
    expect(updates[2]?.grant_epoch).toBe(3);
  });

  it("asks for a confirmation before it revokes and clears the local state", () => {
    const { controller, ui, sandbox, bridge } = harness();
    controller.onPairRequested("pairing-secret-value");
    acceptHandshake(bridge);
    controller.onSandboxEnvelope(
      sandboxEnvelope("selection", { roots: [{ node_id: "1:2", node_type: "FRAME", child_count: 0 }] }),
    );
    controller.onRootSelectionChanged(["1:2"]);
    controller.onAllowRequested();

    controller.onRevokeRequested();
    expect(ui.revokeVisible).toBe(true);
    expect(bridge.frames.filter((entry) => entry.type === "revoke")).toHaveLength(0);

    controller.onRevokeConfirmed();
    expect(bridge.frames.filter((entry) => entry.type === "revoke")).toHaveLength(1);
    expect(sandbox.lastOf("revoke")).toBeDefined();
    expect(ui.revokeVisible).toBe(false);
    const model = ui.model as UiModel;
    expect(model.connection).toBe("revoked");
    expect(model.grantedRootCount).toBe(0);
    expect(model.sessionName).toBe("Not paired");
  });

  it("states that delivered data cannot be recalled", () => {
    const { controller, ui, bridge } = harness();
    controller.onPairRequested("pairing-secret-value");
    acceptHandshake(bridge);
    controller.onSandboxEnvelope(
      sandboxEnvelope("selection", { roots: [{ node_id: "1:2", node_type: "FRAME", child_count: 0 }] }),
    );
    controller.onRootSelectionChanged(["1:2"]);
    controller.onAllowRequested();
    controller.onRevokeRequested();
    controller.onRevokeConfirmed();
    const result = ui.results.find((entry) => entry.action === "revoke");
    expect(result?.message).toContain("cannot be recalled");
  });

  it("reports the retry delay of a busy daemon", () => {
    const { controller, ui, bridge } = harness();
    controller.onPairRequested("pairing-secret-value");
    bridge.handlers.onError({
      code: "BUSY",
      message: "The request queue is full.",
      retryable: true,
      action: "wait_then_retry",
      request_id: "req-01",
      retry_after_ms: 750,
    });
    const failure = ui.results.find((entry) => !entry.ok);
    expect(failure?.message).toContain("BUSY");
    expect(failure?.message).toContain("750");
  });

  it("reports an offline plugin with a recovery action", () => {
    const { controller, ui, bridge } = harness();
    controller.onPairRequested("pairing-secret-value");
    bridge.handlers.onError({
      code: "PLUGIN_OFFLINE",
      message: "The plugin is not reachable.",
      retryable: false,
      action: "resume_by_user",
      request_id: null,
      retry_after_ms: null,
    });
    const failure = ui.results.find((entry) => !entry.ok);
    expect(failure?.message).toContain("PLUGIN_OFFLINE");
    expect(failure?.message).toContain("pair again");
  });

  it("relays a request of the daemon to the sandbox", () => {
    const { controller, sandbox, bridge } = harness();
    controller.onPairRequested("pairing-secret-value");
    acceptHandshake(bridge);
    bridge.handlers.onFrame(bridgeFrame("request", { op: "list_nodes", params: {} }));
    const relayed = sandbox.lastOf("frame");
    expect(relayed).toBeDefined();
    expect((relayed as { payload: BridgeFrame }).payload.message_type).toBe("request");
    expect(relayed?.requestId).toBe("req-01");
  });

  it("relays a cancel of the daemon to the sandbox", () => {
    const { controller, sandbox, bridge } = harness();
    controller.onPairRequested("pairing-secret-value");
    acceptHandshake(bridge);
    bridge.handlers.onFrame(bridgeFrame("cancel", {}, "req-04"));
    const cancel = sandbox.lastOf("cancel");
    expect(cancel).toBeDefined();
    expect(cancel?.requestId).toBe("req-04");
    expect((cancel?.payload as { request_id?: unknown }).request_id).toBe("req-04");
  });

  it("answers a request of the daemon with the result of the sandbox", () => {
    const { controller, sandbox, bridge } = harness();
    controller.onPairRequested("pairing-secret-value");
    acceptHandshake(bridge);
    bridge.handlers.onFrame(bridgeFrame("request", { op: "list_nodes", params: {} }, "req-07"));
    controller.onSandboxEnvelope(sandboxEnvelope("result", { data: { nodes: [] } }, "req-07"));
    const answer = bridge.frames.find((entry) => entry.type === "result");
    expect(answer).toBeDefined();
    expect(answer?.requestId).toBe("req-07");
    expect((answer?.payload as { data?: unknown }).data).toEqual({ nodes: [] });
    expect(sandbox.lastOf("frame")).toBeDefined();
  });

  it("answers with the request id of the envelope of the sandbox", () => {
    const { controller, bridge } = harness();
    controller.onPairRequested("pairing-secret-value");
    acceptHandshake(bridge);
    // The sandbox echoes the request id of the request it answered.
    controller.onSandboxEnvelope(sandboxEnvelope("result", { data: { nodes: [] } }, "req-11"));
    const answer = bridge.frames.find((entry) => entry.type === "result");
    expect(answer).toBeDefined();
    expect(answer?.requestId).toBe("req-11");
  });

  it("forwards the error of the sandbox to the daemon", () => {
    const { controller, bridge } = harness();
    controller.onPairRequested("pairing-secret-value");
    acceptHandshake(bridge);
    controller.onSandboxEnvelope(
      sandboxEnvelope(
        "error",
        {
          code: "OUT_OF_SCOPE",
          message: "The node is outside the granted scope.",
          retryable: false,
          action: "widen_grant",
          request_id: "req-12",
        },
        "req-12",
      ),
    );
    const answer = bridge.frames.find((entry) => entry.type === "error");
    expect(answer).toBeDefined();
    expect(answer?.requestId).toBe("req-12");
    const payload = answer?.payload as { code?: unknown; request_id?: unknown };
    expect(payload.code).toBe("OUT_OF_SCOPE");
    expect(payload.request_id).toBe("req-12");
  });

  it("forwards the progress of the sandbox to the daemon", () => {
    const { controller, bridge } = harness();
    controller.onPairRequested("pairing-secret-value");
    acceptHandshake(bridge);
    controller.onSandboxEnvelope(
      sandboxEnvelope("progress", { visited: 12, elapsed_ms: 4 }, "req-13"),
    );
    const answer = bridge.frames.find((entry) => entry.type === "progress");
    expect(answer).toBeDefined();
    expect(answer?.requestId).toBe("req-13");
    expect((answer?.payload as { visited?: unknown }).visited).toBe(12);
  });

  it("forwards one request to the sandbox at a time", () => {
    const { controller, sandbox, bridge } = harness();
    controller.onPairRequested("pairing-secret-value");
    acceptHandshake(bridge);
    bridge.handlers.onFrame(bridgeFrame("request", { op: "list_nodes", params: {} }, "req-01"));
    bridge.handlers.onFrame(bridgeFrame("request", { op: "list_nodes", params: {} }, "req-02"));
    const forwarded = sandbox.posted.filter((entry) => entry.type === "frame");
    expect(forwarded).toHaveLength(1);
    expect((forwarded[0] as { payload: BridgeFrame }).payload.request_id).toBe("req-01");

    controller.onSandboxEnvelope(sandboxEnvelope("result", { data: { nodes: [] } }));
    const after = sandbox.posted.filter((entry) => entry.type === "frame");
    expect(after).toHaveLength(2);
    expect((after[1] as { payload: BridgeFrame }).payload.request_id).toBe("req-02");
  });

  it("captures a snapshot and streams it to the daemon", () => {
    const { controller, sandbox, bridge } = harness();
    controller.onPairRequested("pairing-secret-value");
    acceptHandshake(bridge);
    controller.onSandboxEnvelope(
      sandboxEnvelope("selection", { roots: [{ node_id: "1:2", node_type: "FRAME", child_count: 0 }] }),
    );
    controller.onRootSelectionChanged(["1:2"]);
    controller.onAllowRequested();
    controller.onCaptureRequested();

    const capture = sandbox.posted.filter((entry) => entry.type === "frame").pop();
    expect(capture).toBeDefined();
    const captureFrame = (capture as { payload: BridgeFrame }).payload;
    expect(captureFrame.message_type).toBe("request");
    expect((captureFrame.payload as { op: string }).op).toBe("capture_snapshot");

    const payload = new TextEncoder().encode('{"nodes":[]}');
    controller.onSandboxEnvelope(
      sandboxEnvelope("chunk", {
        transfer_id: "tr-01",
        sequence: 0,
        bytes: base64Encode(payload),
        total_bytes: payload.length,
        checksum: sha256Hex(payload),
      }),
    );
    const chunk = bridge.frames.find((entry) => entry.type === "chunk");
    expect(chunk).toBeDefined();
    expect((chunk?.payload as { transfer_id: string }).transfer_id).toBe("tr-01");
    const result = bridge.frames.find((entry) => entry.type === "result");
    expect(result).toBeDefined();
  });

  it("shows the capability report of the sandbox", () => {
    const { controller, ui } = harness();
    controller.onSandboxEnvelope(
      sandboxEnvelope("capabilities", {
        capabilities: {
          list_nodes: { status: "available" },
          export_assets: { status: "unavailable", reason: "the seat does not allow it" },
        },
      }),
    );
    const capabilities = (ui.model as UiModel).capabilities as readonly CapabilityView[];
    expect(capabilities).toHaveLength(2);
    expect(capabilities[0]?.available).toBe(true);
    expect(capabilities[1]?.available).toBe(false);
    expect(capabilities[1]?.reason).toBe("the seat does not allow it");
  });

  it("writes only sanitized lines into the log", () => {
    const { controller, ui, bridge } = harness();
    controller.onPairRequested("pairing-secret-value");
    acceptHandshake(bridge);
    bridge.handlers.onError({
      code: "BUSY",
      message: "The request queue is full.",
      retryable: true,
      action: "wait_then_retry",
      request_id: "req-01",
      retry_after_ms: 750,
    });
    expect(ui.logs.length).toBeGreaterThan(0);
    for (const line of ui.logs) {
      expect(line).toMatch(
        /^\d{2}:\d{2}:\d{2} op=[a-z_]+( count=\d+)?( duration=\d+ms)?( error=[A-Z_]+)?$/,
      );
      expect(line).not.toContain("session-01");
      expect(line).not.toContain("session-credential");
    }
  });
});
