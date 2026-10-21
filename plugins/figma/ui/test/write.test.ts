/**
 * The write path of the user interface.
 *
 * The tests drive the controller with the doubles of the other tests, so the
 * whole path runs without a browser: the daemon asks for a preview, the
 * plugin answers, the user decides, and the decision travels to the daemon.
 *
 * The tests of the markup read the document of the interface, because the
 * browser is not part of the test environment.
 */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

import { BridgeClient } from "../src/bridge.js";
import type { BridgeHandlers, SocketLike } from "../src/bridge.js";
import { AppController } from "../src/controller.js";
import type { BridgePort, UiModel } from "../src/controller.js";
import { DomUi } from "../src/dom.js";
import type { UiCallbacks } from "../src/dom.js";
import { FakeDocument, idsOf } from "./dom-double.js";
import type { FakeElement } from "./dom-double.js";
import {
  acceptHandshake,
  bridgeFrame,
  FakeBridge,
  FakeSandbox,
  harness,
  sandboxEnvelope,
} from "./fakes.js";
import type { Harness } from "./fakes.js";

const markup = readFileSync(fileURLToPath(new URL("../index.html", import.meta.url)), "utf8");

/** The plan the agent proposes in the tests. */
const PLAN = {
  plan_id: "plan_01",
  intent: "Restyle the hero banner of the landing page",
  destructive: false,
  operations: [
    { kind: "create_node", parent_id: "1:2", node_type: "FRAME" },
    { kind: "create_node", parent_id: "1:2", node_type: "TEXT" },
    { kind: "set_text", node_id: "1:9", characters: "Welcome to the show" },
    { kind: "set_properties", node_id: "1:9", properties: { width: 320 } },
    { kind: "delete_node", node_id: "1:44" },
  ],
};

/** The preview the plugin resolves the plan into. */
const PREVIEW = {
  plan_id: "plan_01",
  plan_hash: "sha256:0123456789abcdef",
  created: ["1:100", "1:101"],
  changed: ["1:9"],
  removed: ["1:44"],
  moved: [],
  operations_count: 5,
  warnings: [],
  blocked: false,
};

/** The request the daemon sends to preview a plan. */
function previewRequest(approvalId: string | null, plan: unknown = PLAN): unknown {
  return {
    op: "preview_write",
    params: {
      plan,
      ...(approvalId === null ? {} : { approval_id: approvalId }),
    },
  };
}

/** Let the daemon ask for a preview and let the plugin answer it. */
function sendPreview(
  parts: Harness,
  options: { readonly approvalId?: string | null; readonly preview?: unknown } = {},
): void {
  const approvalId = options.approvalId === undefined ? "apr_01" : options.approvalId;
  const request = previewRequest(approvalId);
  parts.bridge.handlers.onFrame(bridgeFrame("request", request, "req-write"));
  const preview = options.preview ?? { preview: PREVIEW };
  parts.controller.onSandboxEnvelope(sandboxEnvelope("preview", preview, "req-write"));
}

/** Pair the controller and grant the roots it needs. */
function connected(): Harness {
  const parts = harness();
  parts.controller.onPairRequested("pairing-secret-value");
  acceptHandshake(parts.bridge);
  parts.controller.onSandboxEnvelope(
    sandboxEnvelope("selection", {
      roots: [{ node_id: "1:2", node_type: "FRAME", child_count: 0 }],
    }),
  );
  parts.controller.onRootSelectionChanged(["1:2"]);
  parts.controller.onAllowRequested();
  return parts;
}

/** The model of the last render. */
function model(parts: Harness): UiModel {
  return parts.ui.model as UiModel;
}

describe("the preview of a write plan", () => {
  it("groups the nodes of the plan by kind", () => {
    const parts = connected();
    sendPreview(parts);
    const preview = model(parts).writePreview;
    expect(preview).not.toBeNull();
    expect(preview?.counts.create).toBe(2);
    expect(preview?.counts.change).toBe(1);
    expect(preview?.counts.move).toBe(0);
    expect(preview?.counts.remove).toBe(1);
  });

  it("counts the operations of the plan by kind", () => {
    const parts = connected();
    sendPreview(parts);
    const kinds = (model(parts).writePreview?.operationCounts ?? []).map(
      (entry) => `${entry.kind}:${entry.count}`,
    );
    expect(kinds).toEqual(["create_node:2", "set_properties:1", "set_text:1", "delete_node:1"]);
  });

  it("shows the intent the agent supplied", () => {
    const parts = connected();
    sendPreview(parts);
    expect(model(parts).writePreview?.intent).toBe("Restyle the hero banner of the landing page");
  });

  it("offers the approval while the plan is not decided", () => {
    const parts = connected();
    sendPreview(parts);
    expect(model(parts).canApproveWrite).toBe(true);
    expect(model(parts).canRejectWrite).toBe(true);
  });

  it("shows a blocked plan with its reasons and offers no approval", () => {
    const parts = connected();
    sendPreview(parts, {
      preview: {
        preview: {
          ...PREVIEW,
          blocked: true,
          warnings: [],
          reasons: [{ code: "NODE_NOT_FOUND", message: "The node 1:44 does not exist." }],
        },
      },
    });
    const preview = model(parts).writePreview;
    expect(preview?.blocked).toBe(true);
    expect(preview?.blockedReasons).toEqual(["The node 1:44 does not exist."]);
    expect(model(parts).canApproveWrite).toBe(false);
  });

  it("refuses to approve a blocked plan", () => {
    const parts = connected();
    sendPreview(parts, { preview: { preview: { ...PREVIEW, blocked: true } } });
    parts.controller.onApproveWriteRequested();
    expect(parts.bridge.approvals()).toHaveLength(0);
    const failure = parts.ui.results.find((entry) => entry.action === "approve");
    expect(failure?.ok).toBe(false);
    expect(failure?.message).toContain("blocked");
  });

  it("forwards the preview to the daemon with the request id", () => {
    const parts = connected();
    sendPreview(parts);
    const answer = parts.bridge.frames.find((entry) => entry.type === "result");
    expect(answer).toBeDefined();
    expect(answer?.requestId).toBe("req-write");
    const payload = answer?.payload as { data?: { preview?: unknown } };
    expect(payload.data?.preview).toBeDefined();
  });

  it("refuses to approve a plan the daemon issued no approval for", () => {
    const parts = connected();
    sendPreview(parts, { approvalId: null });
    expect(model(parts).canApproveWrite).toBe(false);
    parts.controller.onApproveWriteRequested();
    expect(parts.bridge.approvals()).toHaveLength(0);
    const failure = parts.ui.results.find((entry) => entry.action === "approve");
    expect(failure?.message).toContain("no approval");
  });

  it("shows a preview that the plugin answered with a plain result", () => {
    const parts = connected();
    parts.bridge.handlers.onFrame(bridgeFrame("request", previewRequest("apr_01"), "req-write"));
    parts.controller.onSandboxEnvelope(
      sandboxEnvelope("result", { data: { preview: PREVIEW } }, "req-write"),
    );
    expect(model(parts).writePreview?.counts.create).toBe(2);
    expect(model(parts).canApproveWrite).toBe(true);
  });

  it("reads the count of a preview that carries no ids yet", () => {
    const parts = connected();
    sendPreview(parts, {
      preview: {
        preview: {
          plan_id: "plan_01",
          plan_hash: "sha256:0123456789abcdef",
          created: [],
          created_count: 2,
          created_plan: [{ ref: "#1", parent_id: "1:2", node_type: "FRAME" }],
          changed: ["1:9"],
          removed: ["1:44"],
          moved: [],
          operations_count: 5,
          warnings: [],
          blocked: false,
        },
      },
    });
    expect(model(parts).writePreview?.counts.create).toBe(2);
    expect(model(parts).writePreview?.counts.remove).toBe(1);
  });

  it("never mistakes the result of an applied plan for a preview", () => {
    const parts = connected();
    parts.controller.onSandboxEnvelope(
      sandboxEnvelope(
        "result",
        {
          data: {
            plan_id: "plan_01",
            plan_hash: "sha256:0123456789abcdef",
            status: "applied",
            created: ["1:100"],
            changed: [],
            removed: [],
            moved: [],
            undo_committed: true,
            version_saved: null,
          },
        },
        "req-write",
      ),
    );
    expect(model(parts).writePreview).toBeNull();
    expect(model(parts).writeResult?.counts.create).toBe(1);
  });
});

describe("the decision of the user", () => {
  it("sends the approval with the hash of the plan and the approval id", () => {
    const parts = connected();
    sendPreview(parts);
    parts.controller.onApproveWriteRequested();
    const decisions = parts.bridge.approvals();
    expect(decisions).toHaveLength(1);
    expect(decisions[0]?.approved).toBe(true);
    expect(decisions[0]?.plan_hash).toBe("sha256:0123456789abcdef");
    expect(decisions[0]?.approval_id).toBe("apr_01");
    expect(decisions[0]?.reason).toBeUndefined();
  });

  it("sends the rejection with approved false", () => {
    const parts = connected();
    sendPreview(parts);
    parts.controller.onRejectWriteRequested("");
    const decisions = parts.bridge.approvals();
    expect(decisions).toHaveLength(1);
    expect(decisions[0]?.approved).toBe(false);
    expect(decisions[0]?.plan_hash).toBe("sha256:0123456789abcdef");
    expect(decisions[0]?.reason).toBeUndefined();
  });

  it("carries the reason of a rejection", () => {
    const parts = connected();
    sendPreview(parts);
    parts.controller.onRejectWriteRequested("  the wrong parent  ");
    const decisions = parts.bridge.approvals();
    expect(decisions[0]?.approved).toBe(false);
    expect(decisions[0]?.reason).toBe("the wrong parent");
  });

  it("reports the outcome of the decision in a live region", () => {
    const parts = connected();
    sendPreview(parts);
    parts.controller.onApproveWriteRequested();
    const outcome = parts.ui.results.find((entry) => entry.action === "approve");
    expect(outcome?.ok).toBe(true);
    expect(outcome?.message).toContain("undo history");
  });

  it("decides a plan only once", () => {
    const parts = connected();
    sendPreview(parts);
    parts.controller.onApproveWriteRequested();
    parts.controller.onApproveWriteRequested();
    expect(parts.bridge.approvals()).toHaveLength(1);
    expect(model(parts).canApproveWrite).toBe(false);
  });

  it("sends the decision as a frame of the type approval", () => {
    const socket = new RecordingSocket("ws://127.0.0.1:38947/bridge");
    const handlers: BridgeHandlers = {
      onConnection: (): void => undefined,
      onAuthAccepted: (): void => undefined,
      onAuthRejected: (): void => undefined,
      onFrame: (): void => undefined,
      onError: (): void => undefined,
      onLog: (): void => undefined,
    };
    const bridge = new BridgeClient({
      clientId: "client-01",
      handlers,
      socketFactory: (): SocketLike => socket,
    });
    bridge.pair("pairing-secret");
    socket.open();
    bridge.sendApproval({
      approval_id: "apr_01",
      plan_hash: "sha256:0123456789abcdef",
      approved: true,
    });
    const frames = socket.frames();
    const frame = frames[frames.length - 1];
    expect(frame?.message_type).toBe("approval");
    expect(frame?.payload["approval_id"]).toBe("apr_01");
    expect(frame?.payload["plan_hash"]).toBe("sha256:0123456789abcdef");
    expect(frame?.payload["approved"]).toBe(true);
    expect(frame?.payload["reason"]).toBeUndefined();
  });

  it("sends the approval decision with the canonical encoding of the contract", () => {
    const socket = new RecordingSocket("ws://127.0.0.1:38947/bridge");
    const handlers: BridgeHandlers = {
      onConnection: (): void => undefined,
      onAuthAccepted: (): void => undefined,
      onAuthRejected: (): void => undefined,
      onFrame: (): void => undefined,
      onError: (): void => undefined,
      onLog: (): void => undefined,
    };
    const bridge = new BridgeClient({
      clientId: "client-01",
      handlers,
      socketFactory: (): SocketLike => socket,
    });
    bridge.pair("pairing-secret");
    socket.open();
    bridge.sendApproval({
      approval_id: "apr_01",
      plan_hash: "sha256:0123456789abcdef",
      approved: true,
    });
    const raw = socket.sent[socket.sent.length - 1] ?? "";
    expect(raw).toContain('"message_type":"approval"');
    expect(raw).toContain(
      '"payload":{"approval_id":"apr_01","approved":true,"plan_hash":"sha256:0123456789abcdef"}',
    );
    // The keys of the frame are sorted, the way the canonical encoding of
    // the shared contract writes them.
    expect(raw.indexOf('"connection_epoch"')).toBeLessThan(raw.indexOf('"grant_epoch"'));
    expect(raw.indexOf('"grant_epoch"')).toBeLessThan(raw.indexOf('"message_type"'));
    expect(raw.indexOf('"message_type"')).toBeLessThan(raw.indexOf('"payload"'));
  });

  it("echoes the approval the plugin reported in the preview", () => {
    const parts = connected();
    sendPreview(parts, {
      preview: { preview: { ...PREVIEW, approval_id: "apr_02" } },
    });
    parts.controller.onApproveWriteRequested();
    const decisions = parts.bridge.approvals();
    expect(decisions).toHaveLength(1);
    expect(decisions[0]?.approval_id).toBe("apr_02");
    expect(decisions[0]?.plan_hash).toBe("sha256:0123456789abcdef");
  });
});

describe("the write grant", () => {
  it("is a separate control from the read grant", () => {
    const parts = connected();
    const updates = parts.bridge.grantUpdates();
    expect(updates[0]?.mode).toBe("read");
    expect(updates[0]?.session_mode).toBe("live");
    parts.controller.onWriteGrantRequested();
    const after = parts.bridge.grantUpdates();
    expect(after).toHaveLength(2);
    expect(after[1]?.mode).toBe("write");
    expect(after[1]?.root_ids).toEqual(["1:2"]);
  });

  it("is never implied by the read grant", () => {
    const parts = harness();
    parts.controller.onPairRequested("pairing-secret-value");
    acceptHandshake(parts.bridge);
    parts.controller.onSandboxEnvelope(
      sandboxEnvelope("selection", {
        roots: [{ node_id: "1:2", node_type: "FRAME", child_count: 0 }],
      }),
    );
    parts.controller.onRootSelectionChanged(["1:2"]);
    parts.controller.onAllowRequested();
    expect(model(parts).writeGrantEnabled).toBe(false);
    expect(model(parts).canGrantWrite).toBe(true);
  });

  it("states that a plan still needs an approval", () => {
    const parts = connected();
    parts.controller.onWriteGrantRequested();
    const outcome = parts.ui.results.find((entry) => entry.action === "write_grant");
    expect(outcome?.message).toContain("approval");
    expect(model(parts).writeGrantEnabled).toBe(true);
  });

  it("needs a read session before it can be allowed", () => {
    const parts = harness();
    parts.controller.onPairRequested("pairing-secret-value");
    acceptHandshake(parts.bridge);
    parts.controller.onWriteGrantRequested();
    const failure = parts.ui.results.find((entry) => entry.action === "write_grant");
    expect(failure?.ok).toBe(false);
    expect(failure?.message).toContain("read session");
    expect(parts.bridge.grantUpdates()).toHaveLength(0);
  });

  it("raises the grant epoch when the mode changes", () => {
    const parts = connected();
    const before = parts.bridge.grantUpdates()[0]?.grant_epoch ?? 0;
    parts.controller.onWriteGrantRequested();
    const after = parts.bridge.grantUpdates()[1]?.grant_epoch ?? 0;
    expect(after).toBeGreaterThan(before);
  });
});

describe("the result of an applied plan", () => {
  it("reports the counts of the nodes and the undo commit", () => {
    const parts = connected();
    parts.controller.onSandboxEnvelope(
      sandboxEnvelope(
        "write_result",
        {
          result: {
            plan_id: "plan_01",
            plan_hash: "sha256:0123456789abcdef",
            status: "applied",
            created: ["1:100", "1:101"],
            changed: ["1:9"],
            removed: ["1:44"],
            moved: [],
            undo_committed: true,
            version_saved: "v_77",
            warnings: [],
          },
        },
        "req-write",
      ),
    );
    const result = model(parts).writeResult;
    expect(result?.counts.create).toBe(2);
    expect(result?.counts.change).toBe(1);
    expect(result?.counts.move).toBe(0);
    expect(result?.counts.remove).toBe(1);
    expect(result?.undoCommitted).toBe(true);
    expect(result?.versionSaved).toBe("v_77");
  });

  it("names the undo history in the status area", () => {
    const parts = connected();
    parts.controller.onSandboxEnvelope(
      sandboxEnvelope(
        "write_result",
        { result: { plan_id: "plan_01", undo_committed: true, version_saved: null } },
        "req-write",
      ),
    );
    const outcome = parts.ui.results.find((entry) => entry.action === "write_result");
    expect(outcome?.message).toContain("undo history");
    expect(outcome?.message).toContain("undo command of the editor");
  });

  it("does not promise a revert that FigTether does not provide", () => {
    const parts = connected();
    parts.controller.onSandboxEnvelope(
      sandboxEnvelope(
        "write_result",
        { result: { plan_id: "plan_01", undo_committed: true, version_saved: null } },
        "req-write",
      ),
    );
    const outcome = parts.ui.results.find((entry) => entry.action === "write_result");
    expect(outcome?.message).not.toContain("revert");
    expect(outcome?.message).not.toContain("undo the change");
  });

  it("names the version id when a checkpoint was written", () => {
    const parts = connected();
    parts.controller.onSandboxEnvelope(
      sandboxEnvelope(
        "write_result",
        { result: { plan_id: "plan_01", undo_committed: true, version_saved: "v_77" } },
        "req-write",
      ),
    );
    const outcome = parts.ui.results.find((entry) => entry.action === "write_result");
    expect(outcome?.message).toContain("v_77");
  });

  it("forwards the result to the daemon", () => {
    const parts = connected();
    parts.controller.onSandboxEnvelope(
      sandboxEnvelope(
        "write_result",
        { result: { plan_id: "plan_01", undo_committed: true } },
        "req-write",
      ),
    );
    const answer = parts.bridge.frames.find((entry) => entry.type === "result");
    expect(answer).toBeDefined();
    expect(answer?.requestId).toBe("req-write");
  });
});

describe("the hygiene of the log", () => {
  it("never carries the plan, the intent or the text of the document", () => {
    const parts = connected();
    sendPreview(parts);
    parts.controller.onApproveWriteRequested();
    expect(parts.ui.logs.length).toBeGreaterThan(0);
    for (const line of parts.ui.logs) {
      expect(line).not.toContain("Restyle");
      expect(line).not.toContain("hero");
      expect(line).not.toContain("Welcome");
      expect(line).not.toContain("1:44");
      expect(line).not.toContain("sha256");
    }
  });

  it("carries the kinds of the operations and their counts", () => {
    const parts = connected();
    sendPreview(parts);
    const lines = parts.ui.logs.join("\n");
    expect(lines).toContain("op=create_node count=2");
    expect(lines).toContain("op=set_text count=1");
    expect(lines).toContain("op=delete_node count=1");
  });
});

describe("the markup of the write panels", () => {
  it("gives every control an accessible name", () => {
    const controls = elementsOf(markup).filter(
      (element) =>
        element.name === "input" || element.name === "select" || element.name === "textarea",
    );
    expect(controls.length).toBeGreaterThan(0);
    const labels = new Set(labelTargets(markup));
    for (const control of controls) {
      const named =
        labels.has(control.attributes["id"] ?? "") ||
        (control.attributes["aria-label"] ?? "").length > 0;
      expect(named, `the control ${control.attributes["id"] ?? ""} has no label`).toBe(true);
    }
    const buttons = elementsOf(markup).filter((element) => element.name === "button");
    expect(buttons.length).toBeGreaterThan(0);
    for (const button of buttons) {
      const named =
        button.text.trim().length > 0 || (button.attributes["aria-label"] ?? "").length > 0;
      expect(named, `the button ${button.attributes["id"] ?? ""} has no name`).toBe(true);
    }
  });

  it("keeps the write grant a control of its own", () => {
    const buttons = elementsOf(markup).filter((element) => element.name === "button");
    const ids = buttons.map((button) => button.attributes["id"]);
    expect(ids).toContain("allow-button");
    expect(ids).toContain("write-grant-button");
  });

  it("states that the plan is a proposal and that it becomes one undo step", () => {
    const panel = sectionOf(markup, "write-approval-panel");
    expect(panel).toContain("proposal");
    expect(panel).toContain("approve");
    expect(panel).toContain("undo history");
    expect(panel).toContain("Figma");
  });

  it("reaches the approve button with the keyboard alone", () => {
    const approve = elementsOf(markup).find(
      (element) => element.name === "button" && element.attributes["id"] === "write-approve-button",
    );
    expect(approve).toBeDefined();
    expect(approve?.attributes["type"]).toBe("button");
    expect(approve?.attributes["tabindex"]).toBeUndefined();
    const panel = sectionOf(markup, "write-approval-panel");
    expect(panel).not.toContain("<form");
    expect(panel).not.toContain("autofocus");
  });

  it("never focuses the approve button by itself", () => {
    const parts = connected();
    sendPreview(parts);
    expect(parts.ui.focused).toBe(false);
    expect(markup).not.toContain("autofocus");
  });

  it("names every section of the write panels", () => {
    const sections = elementsOf(markup).filter((element) => element.name === "section");
    for (const section of sections) {
      expect(section.attributes["aria-labelledby"]).toBeDefined();
    }
  });
});

/** One element of the markup. */
interface Element {
  readonly name: string;
  readonly attributes: Record<string, string>;
  readonly text: string;
}

/** Read every element of the markup with its attributes and its text. */
function elementsOf(html: string): Element[] {
  const elements: Element[] = [];
  const pattern = /<([a-zA-Z][a-zA-Z0-9-]*)((?:\s+[a-zA-Z-]+(?:\s*=\s*"[^"]*")?)*)\s*(\/?)>/g;
  let match = pattern.exec(html);
  while (match !== null) {
    const name = (match[1] as string).toLowerCase();
    const attributes = readAttributes(match[2] ?? "");
    const closing = match[3] === "/";
    const text = closing ? "" : textOf(html, pattern.lastIndex, name);
    elements.push({ name, attributes, text });
    match = pattern.exec(html);
  }
  return elements;
}

/** Read the attributes of a tag. */
function readAttributes(source: string): Record<string, string> {
  const attributes: Record<string, string> = {};
  const pattern = /([a-zA-Z-]+)(?:\s*=\s*"([^"]*)")?/g;
  let match = pattern.exec(source);
  while (match !== null) {
    attributes[match[1] as string] = match[2] ?? "";
    match = pattern.exec(source);
  }
  return attributes;
}

/** Read the text of an element that is not closed by a slash. */
function textOf(html: string, start: number, name: string): string {
  const closing = `</${name}>`;
  const end = html.indexOf(closing, start);
  if (end < 0) {
    return "";
  }
  return html.slice(start, end);
}

/** The targets of the labels of the markup. */
function labelTargets(html: string): string[] {
  const targets: string[] = [];
  const pattern = /<label[^>]*\sfor="([^"]+)"/g;
  let match = pattern.exec(html);
  while (match !== null) {
    targets.push(match[1] as string);
    match = pattern.exec(html);
  }
  return targets;
}

/** The markup of one section of the document. */
function sectionOf(html: string, id: string): string {
  const start = html.indexOf(`id="${id}"`);
  expect(start).toBeGreaterThan(-1);
  const open = html.lastIndexOf("<section", start);
  const close = html.indexOf("</section>", start);
  return html.slice(open, close);
}

describe("the panels of the interface", () => {
  it("renders the grouped counts of the preview", () => {
    const parts = domHarness();
    parts.controller.onPairRequested("pairing-secret-value");
    acceptHandshake(parts.bridge);
    previewThrough(parts);
    expect(parts.element("write-approval-panel")?.hidden).toBe(false);
    expect(parts.element("write-count-create")?.textContent).toBe("2");
    expect(parts.element("write-count-change")?.textContent).toBe("1");
    expect(parts.element("write-count-move")?.textContent).toBe("0");
    expect(parts.element("write-count-remove")?.textContent).toBe("1");
    expect(parts.element("write-plan-intent")?.textContent).toContain("hero banner");
  });

  it("offers no approve action for a blocked plan", () => {
    const parts = domHarness();
    parts.controller.onPairRequested("pairing-secret-value");
    acceptHandshake(parts.bridge);
    previewThrough(parts, {
      preview: {
        preview: {
          ...PREVIEW,
          blocked: true,
          reasons: [{ code: "NODE_NOT_FOUND", message: "The node 1:44 does not exist." }],
        },
      },
    });
    expect(parts.element("write-approve-button")?.hidden).toBe(true);
    expect(parts.element("write-approve-button")?.disabled).toBe(true);
    expect(parts.element("write-blocked")?.hidden).toBe(false);
    expect(parts.element("write-blocked-list")?.children[0]?.textContent).toContain(
      "does not exist",
    );
  });

  it("reports the undo commit and the version in the status area", () => {
    const parts = domHarness();
    parts.controller.onPairRequested("pairing-secret-value");
    acceptHandshake(parts.bridge);
    parts.controller.onSandboxEnvelope(
      sandboxEnvelope(
        "write_result",
        {
          result: {
            plan_id: "plan_01",
            status: "applied",
            created: ["1:100", "1:101"],
            changed: ["1:9"],
            removed: ["1:44"],
            moved: [],
            undo_committed: true,
            version_saved: "v_77",
          },
        },
        "req-write",
      ),
    );
    expect(parts.element("write-result-created")?.textContent).toBe("2");
    expect(parts.element("write-result-undo")?.textContent).toContain(
      "Committed to the undo history",
    );
    expect(parts.element("write-result-version")?.textContent).toBe("v_77");
  });

  it("keeps the write grant a control of its own", () => {
    const parts = domHarness();
    parts.element("write-grant-button")?.click();
    expect(parts.calls).toContain("write_grant");
    expect(parts.calls).not.toContain("allow");
  });

  it("sends the decision when the approve button is pressed", () => {
    const parts = domHarness();
    parts.controller.onPairRequested("pairing-secret-value");
    acceptHandshake(parts.bridge);
    previewThrough(parts);
    parts.element("write-approve-button")?.click();
    expect(parts.calls).toContain("approve");
    expect(parts.element("write-approve-button")?.focused).toBe(false);
  });

  it("sends the reason when the reject button is pressed", () => {
    const parts = domHarness();
    parts.controller.onPairRequested("pairing-secret-value");
    acceptHandshake(parts.bridge);
    previewThrough(parts);
    const reason = parts.element("write-reject-reason");
    if (reason !== undefined) {
      reason.value = "the wrong parent";
    }
    parts.element("write-reject-button")?.click();
    expect(parts.calls).toContain("reject:the wrong parent");
  });

  it("names the decision of the user in words", () => {
    const parts = domHarness();
    parts.controller.onPairRequested("pairing-secret-value");
    acceptHandshake(parts.bridge);
    previewThrough(parts);
    expect(parts.element("write-approval-state")?.textContent).toContain("proposal");
    parts.controller.onApproveWriteRequested();
    expect(parts.element("write-approval-state")?.textContent).toContain("approved");
  });

  it("states that the write grant still needs an approval", () => {
    const parts = domHarness();
    parts.controller.onPairRequested("pairing-secret-value");
    acceptHandshake(parts.bridge);
    parts.controller.onSandboxEnvelope(
      sandboxEnvelope("selection", {
        roots: [{ node_id: "1:2", node_type: "FRAME", child_count: 0 }],
      }),
    );
    parts.controller.onRootSelectionChanged(["1:2"]);
    parts.controller.onAllowRequested();
    expect(parts.element("write-grant-state")?.textContent).toContain("Agent editing is off");
    parts.controller.onWriteGrantRequested();
    expect(parts.element("write-grant-state")?.textContent).toContain("approval");
  });
});

/** The parts of a controller that renders into the double of the document. */
interface DomHarness {
  readonly controller: AppController;
  readonly bridge: FakeBridge;
  readonly calls: string[];
  /** The element with the given id. */
  element(id: string): FakeElement | undefined;
}

/** Build a controller that renders into the double of the document. */
function domHarness(): DomHarness {
  const doc = new FakeDocument(idsOf(markup));
  const calls: string[] = [];
  const callbacks: UiCallbacks = {
    onPair: (): void => void calls.push("pair"),
    onSelectionChanged: (): void => void calls.push("selection"),
    onSelectionRefresh: (): void => void calls.push("refresh"),
    onAllow: (): void => void calls.push("allow"),
    onWriteGrant: (): void => void calls.push("write_grant"),
    onPause: (): void => void calls.push("pause"),
    onResume: (): void => void calls.push("resume"),
    onCapture: (): void => void calls.push("capture"),
    onRevoke: (): void => void calls.push("revoke"),
    onRevokeConfirm: (): void => void calls.push("revoke_confirm"),
    onRevokeCancel: (): void => void calls.push("revoke_cancel"),
    onApproveWrite: (): void => void calls.push("approve"),
    onRejectWrite: (reason): void => void calls.push(`reject:${reason}`),
  };
  const ui = new DomUi(doc as unknown as Document, callbacks);
  const sandbox = new FakeSandbox();
  let bridge: FakeBridge | null = null;
  const controller = new AppController({
    sandbox,
    ui,
    now: (): number => 0,
    bridgeFactory: (handlers: BridgeHandlers): BridgePort => {
      bridge = new FakeBridge(handlers);
      return bridge;
    },
  });
  return {
    controller,
    bridge: bridge as unknown as FakeBridge,
    calls,
    element: (id: string): FakeElement | undefined => doc.getElementById(id) ?? undefined,
  };
}

/** Let the daemon ask for a preview and let the plugin answer it. */
function previewThrough(parts: DomHarness, options: { readonly preview?: unknown } = {}): void {
  parts.bridge.handlers.onFrame(bridgeFrame("request", previewRequest("apr_01"), "req-write"));
  const preview = options.preview ?? { preview: PREVIEW };
  parts.controller.onSandboxEnvelope(sandboxEnvelope("preview", preview, "req-write"));
}

/** A socket that records the frames it receives. */
class RecordingSocket implements SocketLike {
  onopen: ((event: unknown) => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: ((event: { code?: number; reason?: string }) => void) | null = null;
  onerror: ((event: unknown) => void) | null = null;
  readonly sent: string[] = [];

  constructor(readonly url: string) {}

  send(data: string): void {
    this.sent.push(data);
  }
  close(): void {
    // The socket of the test never closes on its own.
  }
  open(): void {
    this.onopen?.(null);
  }
  frames(): { message_type: string; request_id: string; payload: Record<string, unknown> }[] {
    return this.sent.map(
      (raw) =>
        JSON.parse(raw) as {
          message_type: string;
          request_id: string;
          payload: Record<string, unknown>;
        },
    );
  }
}
