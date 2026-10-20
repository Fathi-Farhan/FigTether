/**
 * Application controller.
 *
 * The controller owns the flows of the UI and is independent of the DOM. It
 * receives commands from the user interface, drives the bridge socket and the
 * sandbox channel, and renders a plain model back to the interface. Every
 * side effect goes through a port, so the whole controller runs in a test
 * without a browser.
 */

import { base64Decode, base64Encode, createFrame } from "@figtether/shared";
import type { BridgeFrame, JsonValue } from "@figtether/shared";

import { BridgeClient } from "./bridge.js";
import type { BridgeHandlers } from "./bridge.js";
import { formatLogLine } from "./log.js";
import type { LogEntry } from "./log.js";
import type {
  AuthAcceptedPayload,
  UiError,
  CandidateRoot,
  CapabilityMap,
  ReadMode,
  SessionMode,
  UiEnvelope,
  UiMessageType,
  WriteGrantMode,
} from "./protocol.js";
import { createSecretStore } from "./secret.js";
import type { SecretStore } from "./secret.js";
import {
  connectionLabel,
  connectionMarker,
  derivePhase,
  fingerprintSessionId,
  formatTimestamp,
  modeLabel,
  phaseLabel,
} from "./status.js";
import type { ConnectionState, SessionPhase } from "./status.js";
import { splitIntoChunks, TransferAssembler } from "./transfer.js";
import type { SandboxChannel } from "./sandbox.js";
import {
  affectedNodes,
  buildApprovalDecision,
  countOperations,
  previewHeadline,
  readApprovalId,
  readPlanIntent,
  readPlanOperations,
  readWritePreview,
  readWriteResult,
  writeResultSummary,
} from "./write.js";
import type {
  ApprovalDecision,
  WritePlanContext,
  WritePreviewView,
  WriteResultView,
} from "./write.js";

/** Actions that report a result to the user. */
export type ActionName =
  | "pair"
  | "grant"
  | "write_grant"
  | "pause"
  | "resume"
  | "capture"
  | "revoke"
  | "preview"
  | "approve"
  | "reject"
  | "write_result";

/** The decision the user took about a proposed plan. */
export type WriteDecision = "none" | "approved" | "rejected";

/** One capability of the runtime as the user interface shows it. */
export interface CapabilityView {
  readonly name: string;
  readonly available: boolean;
  readonly reason: string | null;
}

/** The model the interface renders. */
export interface UiModel {
  readonly connection: ConnectionState;
  readonly connectionText: string;
  readonly connectionMarker: string;
  readonly phase: SessionPhase;
  readonly phaseText: string;
  readonly sessionName: string;
  readonly sessionFingerprint: string;
  readonly grantedRootCount: number;
  readonly modeText: string;
  readonly lastReadText: string;
  readonly queueDepthText: string;
  readonly capabilities: readonly CapabilityView[];
  readonly candidates: readonly CandidateRoot[];
  readonly selectedRootIds: readonly string[];
  readonly canPair: boolean;
  readonly canGrant: boolean;
  readonly canPause: boolean;
  readonly canResume: boolean;
  readonly canCapture: boolean;
  readonly canRevoke: boolean;
  readonly revokeConfirmVisible: boolean;
  readonly grantPending: boolean;
  /** Whether the user allowed editing inside the granted roots. */
  readonly writeGrantEnabled: boolean;
  /** Whether the user may allow editing inside the granted roots now. */
  readonly canGrantWrite: boolean;
  /** The plan the agent proposed, when one is waiting for a decision. */
  readonly writePreview: WritePreviewView | null;
  /** The decision the user took about the proposed plan. */
  readonly writeDecision: WriteDecision;
  /** Whether the user may approve the proposed plan. */
  readonly canApproveWrite: boolean;
  /** Whether the user may reject the proposed plan. */
  readonly canRejectWrite: boolean;
  /** The plan that was applied last, when one was. */
  readonly writeResult: WriteResultView | null;
}

/** The interface the controller renders into. */
export interface UiPort {
  render(model: UiModel): void;
  appendLog(line: string): void;
  setPairError(message: string | null): void;
  setActionResult(action: ActionName, ok: boolean, message: string): void;
  setRevokeConfirmVisible(visible: boolean): void;
  focusPairInput(): void;
}

/** The socket surface the controller uses. */
export interface BridgePort {
  pair(secret: string): void;
  sendGrantUpdate(
    rootIds: readonly string[],
    grantEpoch: number,
    sessionMode: SessionMode,
    writeMode?: WriteGrantMode,
  ): boolean;
  revoke(): boolean;
  sendFrame(messageType: UiMessageType, payload: JsonValue, requestId?: string): boolean;
  sendApproval(decision: ApprovalDecision): boolean;
  reportPolicyViolation?(): void;
  readonly sessionId: string | null;
  readonly connectionEpoch: number;
  readonly grantEpoch: number;
}

/** Options of the controller. */
export interface ControllerOptions {
  readonly sandbox: SandboxChannel;
  readonly ui: UiPort;
  readonly now?: () => number;
  readonly secret?: SecretStore;
  /** Factory for the bridge. Tests replace it with a fake. */
  readonly bridgeFactory?: (handlers: BridgeHandlers) => BridgePort;
  /** Options of the default bridge factory. */
  readonly bridgeOptions?: Omit<BridgeOptions, "handlers">;
}

/** Options of the default bridge factory. */
export interface BridgeOptions {
  readonly clientId: string;
  readonly url?: string;
}

const NO_SESSION = "Not paired";
const NO_FINGERPRINT = "Not available";
const NOT_REPORTED = "Not reported by the daemon";

/** Whether a value is a plain record. */
function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** Controller of the plugin UI. */
export class AppController {
  private readonly sandbox: SandboxChannel;
  private readonly ui: UiPort;
  private readonly bridge: BridgePort;
  private readonly secret: SecretStore;
  private readonly now: () => number;

  private connection: ConnectionState = "unpaired";
  private paused = false;
  private sessionName: string | null = null;
  private sessionFingerprint: string | null = null;
  private grantedRootIds: string[] = [];
  private mode: ReadMode = "unknown";
  private lastReadAt: number | null = null;
  private queueDepth: number | null = null;
  private capabilities: CapabilityView[] = [];
  private candidates: CandidateRoot[] = [];
  private selectedRootIds: string[] = [];
  private grantPending = false;
  private revokeConfirmVisible = false;
  private localGrantEpoch = 0;
  /** Whether the user allowed editing inside the granted roots. */
  private writeGrantEnabled = false;
  /** The plan the agent proposed, waiting for a decision of the user. */
  private writePreview: WritePreviewView | null = null;
  /** The approval the daemon issued for the proposed plan. */
  private writeApprovalId: string | null = null;
  /** Whether the user already decided about the proposed plan. */
  private writeDecision: WriteDecision = "none";
  /** The plan that was applied last. */
  private writeResult: WriteResultView | null = null;
  /**
   * The requests that the sandbox still has to answer.
   *
   * Only one request is in flight at a time, so the answer of the sandbox can
   * be matched with the request that caused it. The sandbox does not carry
   * the request id in its result payload.
   */
  private readonly waiting: string[] = [];
  private inFlight: string | null = null;
  /** The frames of the daemon that wait for an answer of the sandbox. */
  private readonly daemonFrames = new Map<string, BridgeFrame>();
  private readonly assembler = new TransferAssembler();
  /** The request id of the chunked transfer that is being reassembled. */
  private chunkRequestId: string | null = null;
  private requestCounter = 0;

  constructor(options: ControllerOptions) {
    this.sandbox = options.sandbox;
    this.ui = options.ui;
    this.secret = options.secret ?? createSecretStore();
    this.now = options.now ?? ((): number => Date.now());

    const handlers: BridgeHandlers = {
      onConnection: (state, detail): void => this.handleConnection(state, detail),
      onAuthAccepted: (payload): void => this.handleAuthAccepted(payload),
      onAuthRejected: (code, message): void => this.handleAuthRejected(code, message),
      onFrame: (frame): void => this.handleBridgeFrame(frame),
      onError: (error): void => this.handleBridgeError(error),
      onLog: (entry): void => this.log(entry),
    };

    if (options.bridgeFactory !== undefined) {
      this.bridge = options.bridgeFactory(handlers);
    } else {
      this.bridge = new BridgeClient({
        ...(options.bridgeOptions ?? { clientId: "figtether-ui" }),
        handlers,
      });
    }
  }

  /** Announce the UI to the sandbox and request the initial state. */
  start(): void {
    this.sandbox.post("capabilities", {});
    this.requestSelection();
    this.render();
  }

  /** The user pasted a pairing secret and pressed Pair. */
  onPairRequested(secret: string): void {
    const trimmed = secret.trim();
    if (trimmed.length === 0) {
      this.ui.setPairError("Paste the pairing secret shown by the CLI, then press Pair.");
      this.ui.focusPairInput();
      return;
    }
    this.ui.setPairError(null);
    this.secret.write(trimmed);
    this.bridge.pair(trimmed);
    this.secret.clear();
    this.ui.setActionResult("pair", true, "Connecting to the local daemon.");
    this.render();
  }

  /** The user asked the editor for the candidate roots again. */
  onSelectionRefreshRequested(): void {
    this.requestSelection();
  }

  /** The user changed the checkboxes of the candidate roots. */
  onRootSelectionChanged(rootIds: readonly string[]): void {
    const known = new Set(this.candidates.map((root) => root.node_id));
    this.selectedRootIds = rootIds.filter((id) => known.has(id));
    this.render();
  }

  /** The user pressed Allow read session. */
  onAllowRequested(): void {
    if (this.connection !== "connected") {
      this.ui.setActionResult(
        "grant",
        false,
        "Pair with the local daemon before allowing a read session.",
      );
      return;
    }
    if (this.selectedRootIds.length === 0) {
      this.ui.setActionResult(
        "grant",
        false,
        "Check at least one root before allowing a read session.",
      );
      return;
    }
    const roots = [...this.selectedRootIds];
    const epoch = this.nextGrantEpoch();
    this.grantPending = true;
    this.ui.setActionResult("grant", true, "Registering the selected roots with the local daemon.");
    // The read grant is a read grant: it never carries the write grant with
    // it, so a new read grant always returns the session to read only.
    this.writeGrantEnabled = false;
    this.sandbox.post("grant_update", {
      root_ids: roots,
      grant_epoch: epoch,
      mode: "read",
      session_mode: "live",
    });
    const sent = this.bridge.sendGrantUpdate(roots, epoch, "live", "read");
    if (!sent) {
      this.grantPending = false;
      this.ui.setActionResult(
        "grant",
        false,
        "The daemon is not reachable. Reconnect and allow again.",
      );
      this.render();
      return;
    }
    this.grantedRootIds = roots;
    this.localGrantEpoch = epoch;
    this.paused = false;
    this.grantPending = false;
    this.mode = "live";
    this.ui.setActionResult(
      "grant",
      true,
      "Read session allowed for the selected roots. A later selection change never widens the grant.",
    );
    this.log({ operation: "grant_update", count: roots.length });
    this.render();
  }

  /**
   * The user pressed the write grant.
   *
   * The write grant is a separate action: the read grant never implies it.
   * It allows editing inside the roots of the read grant and it says so, and
   * it never approves a plan on its own.
   */
  onWriteGrantRequested(): void {
    if (this.connection !== "connected") {
      this.ui.setActionResult(
        "write_grant",
        false,
        "Pair with the local daemon before allowing a write session.",
      );
      return;
    }
    if (this.grantedRootIds.length === 0) {
      this.ui.setActionResult(
        "write_grant",
        false,
        "Allow a read session first. The write session uses the roots of the read session.",
      );
      return;
    }
    const enabled = !this.writeGrantEnabled;
    const mode: WriteGrantMode = enabled ? "write" : "read";
    const epoch = this.nextGrantEpoch();
    const sent = this.bridge.sendGrantUpdate(
      this.grantedRootIds,
      epoch,
      this.paused ? "paused" : "live",
      mode,
    );
    if (!sent) {
      this.ui.setActionResult(
        "write_grant",
        false,
        "The daemon is not reachable. Reconnect and allow again.",
      );
      this.log({ operation: "write_grant", errorCode: "PLUGIN_OFFLINE" });
      return;
    }
    this.sandbox.post("grant_update", {
      root_ids: this.grantedRootIds,
      grant_epoch: epoch,
      mode,
      session_mode: this.paused ? "paused" : "live",
    });
    this.localGrantEpoch = epoch;
    this.writeGrantEnabled = enabled;
    this.ui.setActionResult(
      "write_grant",
      true,
      enabled
        ? "Write session allowed for the granted roots. Every plan still needs your approval before anything changes."
        : "The session is back to read only. Plans are refused now.",
    );
    this.log({ operation: "write_grant", count: this.grantedRootIds.length });
    this.render();
  }

  /**
   * The user approved the proposed plan.
   *
   * The decision goes to the daemon with the hash of the plan the user was
   * shown. The plugin never applies a plan on its own.
   */
  onApproveWriteRequested(): void {
    const preview = this.writePreview;
    if (preview === null) {
      this.ui.setActionResult("approve", false, "There is no proposed change to approve.");
      return;
    }
    if (preview.blocked) {
      this.ui.setActionResult(
        "approve",
        false,
        "The proposed change is blocked. It cannot be approved as it is written.",
      );
      return;
    }
    if (this.writeDecision !== "none") {
      this.ui.setActionResult(
        "approve",
        false,
        "This plan was already decided. Wait for the next proposal.",
      );
      return;
    }
    const approvalId = this.writeApprovalId;
    if (approvalId === null) {
      this.ui.setActionResult(
        "approve",
        false,
        "The daemon issued no approval for this plan. Ask the agent to preview the plan again.",
      );
      return;
    }
    const decision = buildApprovalDecision(approvalId, preview.planHash, true, null);
    if (!this.bridge.sendApproval(decision)) {
      this.ui.setActionResult(
        "approve",
        false,
        "The daemon is not reachable. The plan stays unapproved.",
      );
      this.log({ operation: "approve", errorCode: "PLUGIN_OFFLINE" });
      return;
    }
    this.writeDecision = "approved";
    this.ui.setActionResult(
      "approve",
      true,
      "Approved. The daemon applies the plan, and the change lands as one step in the undo history of Figma.",
    );
    this.log({ operation: "approve", count: preview.operationsCount });
    this.render();
  }

  /**
   * The user rejected the proposed plan.
   *
   * The reason is optional. It travels with the decision when the user wrote
   * one, and it never travels to the log.
   */
  onRejectWriteRequested(reason: string): void {
    const preview = this.writePreview;
    if (preview === null) {
      this.ui.setActionResult("reject", false, "There is no proposed change to reject.");
      return;
    }
    if (this.writeDecision !== "none") {
      this.ui.setActionResult(
        "reject",
        false,
        "This plan was already decided. Wait for the next proposal.",
      );
      return;
    }
    const approvalId = this.writeApprovalId;
    if (approvalId === null) {
      this.ui.setActionResult(
        "reject",
        false,
        "The daemon issued no approval for this plan. Ask the agent to preview the plan again.",
      );
      return;
    }
    const decision = buildApprovalDecision(approvalId, preview.planHash, false, reason);
    if (!this.bridge.sendApproval(decision)) {
      this.ui.setActionResult(
        "reject",
        false,
        "The daemon is not reachable. The plan stays undecided.",
      );
      this.log({ operation: "reject", errorCode: "PLUGIN_OFFLINE" });
      return;
    }
    this.writeDecision = "rejected";
    this.ui.setActionResult("reject", true, "Rejected. The daemon will not apply this plan.");
    this.log({ operation: "reject", count: preview.operationsCount });
    this.render();
  }

  /** The user pressed Pause. */
  onPauseRequested(): void {
    if (this.connection !== "connected" || this.grantedRootIds.length === 0) {
      this.ui.setActionResult("pause", false, "There is no active read session to pause.");
      return;
    }
    const epoch = this.nextGrantEpoch();
    const sent = this.bridge.sendGrantUpdate(
      this.grantedRootIds,
      epoch,
      "paused",
      this.writeGrantEnabled ? "write" : "read",
    );
    if (!sent) {
      this.ui.setActionResult(
        "pause",
        false,
        "The daemon is not reachable. Reconnect before pausing.",
      );
      return;
    }
    this.sandbox.post("grant_update", {
      root_ids: this.grantedRootIds,
      grant_epoch: epoch,
      mode: this.writeGrantEnabled ? "write" : "read",
      session_mode: "paused",
    });
    this.localGrantEpoch = epoch;
    this.paused = true;
    this.ui.setActionResult("pause", true, "Paused. New reads are refused until you resume.");
    this.log({ operation: "pause", count: 1 });
    this.render();
  }

  /** The user pressed Resume. */
  onResumeRequested(): void {
    if (this.connection !== "connected") {
      this.ui.setActionResult("resume", false, "The session is not connected. Pair again first.");
      return;
    }
    const epoch = this.nextGrantEpoch();
    const sent = this.bridge.sendGrantUpdate(
      this.grantedRootIds,
      epoch,
      "live",
      this.writeGrantEnabled ? "write" : "read",
    );
    if (!sent) {
      this.ui.setActionResult(
        "resume",
        false,
        "The daemon is not reachable. Reconnect before resuming.",
      );
      return;
    }
    this.sandbox.post("grant_update", {
      root_ids: this.grantedRootIds,
      grant_epoch: epoch,
      mode: this.writeGrantEnabled ? "write" : "read",
      session_mode: "live",
    });
    this.localGrantEpoch = epoch;
    this.paused = false;
    this.mode = "live";
    this.ui.setActionResult("resume", true, "Resumed. Reads are allowed again.");
    this.log({ operation: "resume", count: 1 });
    this.render();
  }

  /** The user pressed Capture snapshot. */
  onCaptureRequested(): void {
    if (this.connection !== "connected" || this.grantedRootIds.length === 0) {
      this.ui.setActionResult(
        "capture",
        false,
        "Allow a read session before capturing a snapshot.",
      );
      return;
    }
    this.assembler.reset();
    this.waiting.push(this.nextRequestId("cap"));
    this.forwardNext();
    this.ui.setActionResult("capture", true, "Capturing a snapshot of the granted roots.");
    this.log({ operation: "capture", count: 1 });
  }

  /** The user pressed Revoke. The action waits for a confirmation. */
  onRevokeRequested(): void {
    this.revokeConfirmVisible = true;
    this.ui.setRevokeConfirmVisible(true);
    this.render();
  }

  /** The user cancelled the revoke confirmation. */
  onRevokeCancelled(): void {
    this.revokeConfirmVisible = false;
    this.ui.setRevokeConfirmVisible(false);
    this.render();
  }

  /** The user confirmed the revoke. */
  onRevokeConfirmed(): void {
    this.revokeConfirmVisible = false;
    this.ui.setRevokeConfirmVisible(false);
    this.bridge.revoke();
    this.sandbox.post("revoke", { reason: "user_revoked" });
    this.clearLocalState();
    this.connection = "revoked";
    this.ui.setActionResult(
      "revoke",
      true,
      "Revoked. Later reads are refused. Data the client already received cannot be recalled.",
    );
    this.log({ operation: "revoke", count: 1 });
    this.render();
  }

  /** A message arrived from the main sandbox. */
  onSandboxEnvelope(envelope: UiEnvelope): void {
    const requestId = envelope.request_id ?? null;
    switch (envelope.type) {
      case "selection":
        this.handleSelection(envelope.payload);
        return;
      case "capabilities":
        this.handleCapabilities(envelope.payload);
        return;
      case "result":
        this.handleSandboxResult(envelope.payload, requestId);
        return;
      case "preview":
        this.handleSandboxPreview(envelope.payload, requestId);
        return;
      case "write_result":
        this.handleSandboxWriteResult(envelope.payload, requestId);
        return;
      case "error":
        this.handleSandboxError(envelope.payload, requestId);
        return;
      case "chunk":
        this.handleSandboxChunk(envelope.payload, requestId);
        return;
      case "progress":
        this.handleSandboxProgress(envelope.payload, requestId);
        return;
      default:
        return;
    }
  }

  /** Release every local resource. */
  dispose(): void {
    this.secret.clear();
    this.assembler.reset();
    this.sandbox.dispose();
  }

  /**
   * The editor reported that it blocked the socket by its content security
   * policy. The bridge turns this into an actionable message.
   */
  reportPolicyViolation(): void {
    this.bridge.reportPolicyViolation?.();
  }

  private requestSelection(): void {
    this.sandbox.post("selection_request", {});
  }

  private nextGrantEpoch(): number {
    this.localGrantEpoch += 1;
    return this.localGrantEpoch;
  }

  private nextRequestId(prefix: string): string {
    this.requestCounter += 1;
    return `ui_${prefix}_${this.requestCounter}`;
  }

  private buildRequestFrame(requestId: string, op: string): BridgeFrame {
    return createFrame(
      "request",
      requestId,
      { op, params: {} },
      {
        ...(this.bridge.sessionId === null ? {} : { session_id: this.bridge.sessionId }),
        connection_epoch: this.bridge.connectionEpoch,
        grant_epoch: this.localGrantEpoch,
      },
    );
  }

  /**
   * Forward the next request to the sandbox.
   *
   * Only one request is in flight, so that the answer of the sandbox can be
   * matched with the request that caused it. The envelope carries the request
   * id, so the sandbox can echo it in its answer.
   */
  private forwardNext(): void {
    if (this.inFlight !== null) {
      return;
    }
    const next = this.waiting.shift();
    if (next === undefined) {
      return;
    }
    this.inFlight = next;
    if (next.startsWith("ui_cap_")) {
      this.sandbox.post("frame", this.buildRequestFrame(next, "capture_snapshot"), next);
      return;
    }
    const frame = this.daemonFrames.get(next);
    if (frame === undefined) {
      this.inFlight = null;
      this.forwardNext();
      return;
    }
    this.sandbox.post("frame", frame, next);
  }

  private handleConnection(state: ConnectionState, detail: string | null): void {
    const previous = this.connection;
    if (previous === "revoked" && state !== "connected" && state !== "connecting") {
      // A revoked session is terminal. The close of the socket must not turn
      // it back into an offline session that the user would have to repair.
      this.render();
      return;
    }
    this.connection = state;
    if (state === "offline" && previous === "connected") {
      this.ui.setActionResult(
        "pair",
        false,
        detail ?? "The connection to the local daemon was lost. Reconnecting with a bounded retry.",
      );
    }
    if (state === "revoked") {
      this.clearLocalState();
    }
    if (detail !== null) {
      this.ui.setPairError(detail);
    }
    this.render();
  }

  private handleAuthAccepted(payload: AuthAcceptedPayload): void {
    this.secret.clear();
    if (this.connection !== "revoked") {
      this.connection = "connected";
    }
    this.sessionName = payload.session_name;
    this.sessionFingerprint = fingerprintSessionId(payload.session_id);
    this.capabilities = this.toCapabilityViews(payload.capabilities);
    this.paused = false;
    this.ui.setPairError(null);
    this.ui.setActionResult("pair", true, "Paired with the local daemon.");
    this.log({ operation: "pair", count: 1 });
    this.requestSelection();
    this.render();
  }

  private handleAuthRejected(code: string, message: string): void {
    this.secret.clear();
    this.ui.setPairError(
      `${code}. ${message} Run 'figtether pair' for a new secret, then pair again.`,
    );
    this.ui.setActionResult("pair", false, "Pairing failed.");
    this.log({ operation: "pair", errorCode: code });
    this.render();
  }

  private handleBridgeFrame(frame: BridgeFrame): void {
    switch (frame.message_type) {
      case "grant_update": {
        const payload = frame.payload as { root_ids?: unknown; grant_epoch?: unknown };
        if (Array.isArray(payload.root_ids)) {
          this.grantedRootIds = payload.root_ids.filter(
            (entry): entry is string => typeof entry === "string",
          );
        }
        if (typeof payload.grant_epoch === "number") {
          this.localGrantEpoch = payload.grant_epoch;
        }
        this.grantPending = false;
        break;
      }
      case "capabilities": {
        const payload = frame.payload as { capabilities?: unknown };
        if (payload.capabilities !== undefined) {
          this.capabilities = this.toCapabilityViews(payload.capabilities as CapabilityMap);
        }
        break;
      }
      case "revoke": {
        this.clearLocalState();
        this.connection = "revoked";
        this.ui.setActionResult(
          "revoke",
          true,
          "The daemon revoked the session. Later reads are refused.",
        );
        break;
      }
      case "request": {
        // The daemon asked the editor for something. The request travels to
        // the sandbox and the answer travels back on the same request id.
        this.daemonFrames.set(frame.request_id, frame);
        this.waiting.push(frame.request_id);
        this.forwardNext();
        break;
      }
      case "cancel": {
        // The daemon gave up on a request. The sandbox stops the work and
        // answers with a cancellation error.
        this.sandbox.post("cancel", { request_id: frame.request_id }, frame.request_id);
        break;
      }
      default:
        break;
    }
    this.render();
  }

  private handleBridgeError(error: UiError): void {
    this.ui.setActionResult("pair", false, this.describeError(error));
    this.log({ operation: "other", errorCode: error.code });
    this.render();
  }

  private handleSelection(payload: unknown): void {
    this.candidates = this.readRoots(payload);
    const known = new Set(this.candidates.map((root) => root.node_id));
    this.selectedRootIds = this.selectedRootIds.filter((id) => known.has(id));
    this.render();
  }

  private handleCapabilities(payload: unknown): void {
    if (payload === null || typeof payload !== "object") {
      return;
    }
    const record = payload as { capabilities?: unknown };
    if (record.capabilities !== undefined) {
      this.capabilities = this.toCapabilityViews(record.capabilities as CapabilityMap);
    }
    this.render();
  }

  /**
   * Answer of the sandbox to a request.
   *
   * The answer travels to the daemon with the request id of the request it
   * belongs to, so the daemon completes the pending request. The local state
   * of the interface is updated as well.
   *
   * A write request may also be answered with a plain result. The interface
   * reads the preview out of it, so the plan reaches the user either way.
   */
  private handleSandboxResult(payload: unknown, requestId: string | null): void {
    const context = this.readPlanContext(requestId ?? this.inFlight);
    const data =
      payload !== null && typeof payload === "object"
        ? (payload as { data?: unknown }).data
        : undefined;
    const preview = data === undefined ? null : readWritePreview(data, context);
    const applied = data === undefined ? null : readWriteResult(data);
    const answerId = this.completeRequest(requestId);
    if (answerId === null) {
      if (preview !== null) {
        this.showPreview(preview, context);
      } else if (applied !== null) {
        this.showWriteResult(applied);
      }
      return;
    }
    if (data === undefined) {
      this.bridge.sendFrame("result", { data: null }, answerId);
    } else {
      this.bridge.sendFrame("result", { data: data as JsonValue }, answerId);
    }
    if (preview !== null) {
      this.showPreview(preview, context);
      return;
    }
    if (applied !== null) {
      this.showWriteResult(applied);
      return;
    }
    this.lastReadAt = this.now();
    this.render();
  }

  /**
   * The preview of a write plan.
   *
   * The plugin resolved the plan against the live document without changing
   * it. The preview travels to the daemon with the request id of the request
   * it belongs to, so the daemon can answer the agent, and the interface
   * shows the plan to the user.
   */
  private handleSandboxPreview(payload: unknown, requestId: string | null): void {
    const context = this.readPlanContext(requestId ?? this.inFlight);
    const preview = readWritePreview(payload, context);
    const answerId = this.completeRequest(requestId);
    this.forwardToDaemon(this.previewAnswer(payload), answerId);
    if (preview === null) {
      this.ui.setActionResult(
        "preview",
        false,
        "The editor sent a preview the interface could not read.",
      );
      this.log({ operation: "preview", errorCode: "INTERNAL_ERROR" });
      this.render();
      return;
    }
    this.showPreview(preview, context);
  }

  /**
   * The result of an applied write plan.
   *
   * The result travels to the daemon with the request id of the request it
   * belongs to, and the status area of the interface reports what the plan
   * did.
   */
  private handleSandboxWriteResult(payload: unknown, requestId: string | null): void {
    const answerId = this.completeRequest(requestId);
    this.forwardToDaemon(this.resultAnswer(payload), answerId);
    const result = readWriteResult(payload);
    if (result === null) {
      this.ui.setActionResult(
        "write_result",
        false,
        "The editor sent a result the interface could not read.",
      );
      this.log({ operation: "write_result", errorCode: "INTERNAL_ERROR" });
      this.render();
      return;
    }
    this.showWriteResult(result);
  }

  /**
   * Report the result of an applied plan.
   *
   * The status area names the counts of the nodes, the undo history and the
   * version checkpoint. It never promises a revert that FigTether does not
   * provide: the recovery path is the undo command of the editor.
   */
  private showWriteResult(result: WriteResultView): void {
    this.writeResult = result;
    this.ui.setActionResult("write_result", true, writeResultSummary(result));
    this.log({
      operation: "write_result",
      count: affectedNodes(result.counts),
    });
    this.render();
  }

  /**
   * Show the plan the agent proposed.
   *
   * The plan waits for a decision of the user. The interface never moves the
   * focus, so a key press that belongs to another control cannot approve the
   * plan.
   */
  private showPreview(preview: WritePreviewView, context: WritePlanContext): void {
    this.writePreview = preview;
    this.writeApprovalId = preview.approvalId ?? context.approvalId;
    this.writeDecision = "none";
    this.ui.setActionResult("preview", true, previewHeadline(preview));
    this.logPreview(preview);
    this.render();
  }

  /**
   * Read what the interface knows about the plan behind a preview.
   *
   * The preview of the plugin does not carry the intent of the plan and not
   * the kinds of its operations, and the approval id is minted by the
   * daemon. All of them travel with the request of the daemon that caused
   * the preview, so the interface reads them there.
   */
  private readPlanContext(requestId: string | null): WritePlanContext {
    const frame = requestId === null ? undefined : this.daemonFrames.get(requestId);
    const payload = frame === undefined ? null : frame.payload;
    const operations = countOperations(readPlanOperations(payload));
    return {
      intent: readPlanIntent(payload),
      operationCounts: operations,
      approvalId: readApprovalId(payload),
    };
  }

  /**
   * Forward the answer of the sandbox to the daemon.
   *
   * The answer keeps its shape, so the daemon reads the preview and the
   * result exactly as the plugin produced them.
   */
  private forwardToDaemon(payload: unknown, answerId: string | null): void {
    if (answerId === null) {
      return;
    }
    this.bridge.sendFrame("result", { data: payload as JsonValue }, answerId);
  }

  /**
   * The data of an answer of the sandbox.
   *
   * The sandbox wraps the answer of a request in an envelope that carries the
   * answer under `data`. The data is what the daemon reads.
   */
  private answerData(payload: unknown): unknown {
    if (payload !== null && typeof payload === "object") {
      const record = payload as { data?: unknown };
      if (record.data !== undefined) {
        return record.data;
      }
    }
    return payload;
  }

  /**
   * The answer that carries a preview to the daemon.
   *
   * The daemon reads the preview under `preview`, so an answer that holds the
   * preview on its own is wrapped.
   */
  private previewAnswer(payload: unknown): unknown {
    const data = this.answerData(payload);
    if (isRecord(data) && "preview" in data) {
      return data;
    }
    return { preview: data };
  }

  /**
   * The answer that carries a result to the daemon.
   *
   * The daemon reads the result under `result`, so an answer that holds the
   * result on its own is wrapped.
   */
  private resultAnswer(payload: unknown): unknown {
    const data = this.answerData(payload);
    if (isRecord(data) && "result" in data) {
      return data;
    }
    return { result: data };
  }

  /**
   * Write the preview of a plan into the log.
   *
   * The log carries the kinds of the operations and their counts. It never
   * carries the plan itself, the intent of the plan or any text of the
   * document.
   */
  private logPreview(preview: WritePreviewView): void {
    if (preview.operationCounts.length === 0) {
      this.log({ operation: "preview", count: preview.operationsCount });
      return;
    }
    for (const entry of preview.operationCounts) {
      this.log({ operation: entry.kind, count: entry.count });
    }
  }

  /**
   * Failure of the sandbox.
   *
   * The structured error travels to the daemon with the request id of the
   * request it belongs to, so the daemon completes the pending request.
   */
  private handleSandboxError(payload: unknown, requestId: string | null): void {
    const answerId = this.completeRequest(requestId);
    const error = this.readError(payload);
    if (answerId !== null) {
      this.bridge.sendFrame(
        "error",
        {
          code: error.code,
          message: error.message,
          retryable: error.retryable,
          action: error.action,
          request_id: answerId,
        },
        answerId,
      );
    }
    this.ui.setActionResult("capture", false, this.describeError(error));
  }

  /**
   * Progress report of the sandbox.
   *
   * The report travels to the daemon with the request id of the request it
   * belongs to. The request stays open, so the local queue is not advanced.
   */
  private handleSandboxProgress(payload: unknown, requestId: string | null): void {
    const answerId = requestId ?? this.inFlight;
    if (answerId === null) {
      return;
    }
    const record =
      payload !== null && typeof payload === "object"
        ? (payload as { visited?: unknown; elapsed_ms?: unknown })
        : {};
    const visited = typeof record.visited === "number" ? record.visited : 0;
    const elapsedMs = typeof record.elapsed_ms === "number" ? record.elapsed_ms : 0;
    this.bridge.sendFrame("progress", { visited, elapsed_ms: elapsedMs }, answerId);
  }

  private handleSandboxChunk(payload: unknown, requestId: string | null): void {
    if (requestId !== null) {
      this.chunkRequestId = requestId;
    }
    if (payload === null || typeof payload !== "object") {
      return;
    }
    const record = payload as {
      transfer_id?: unknown;
      sequence?: unknown;
      bytes?: unknown;
      total_bytes?: unknown;
      checksum?: unknown;
    };
    if (typeof record.transfer_id !== "string" || typeof record.sequence !== "number") {
      this.ui.setActionResult("capture", false, "The editor sent an unreadable capture chunk.");
      return;
    }
    const totalBytes = typeof record.total_bytes === "number" ? record.total_bytes : 0;
    if (totalBytes > 33_554_432) {
      this.assembler.reset();
      this.ui.setActionResult("capture", false, "The capture is larger than the transfer limit.");
      return;
    }
    if (!this.assembler.open) {
      if (!this.assembler.begin(record.transfer_id, totalBytes)) {
        this.ui.setActionResult("capture", false, "The capture exceeds the transfer limit.");
        return;
      }
    }
    let bytes: Uint8Array;
    try {
      bytes = base64Decode(typeof record.bytes === "string" ? record.bytes : "");
    } catch {
      this.assembler.reset();
      this.ui.setActionResult("capture", false, "The editor sent a malformed capture chunk.");
      return;
    }
    const accepted = this.assembler.accept({
      sequence: record.sequence,
      bytes,
      checksum: typeof record.checksum === "string" ? record.checksum : null,
    });
    if (accepted !== "accepted") {
      this.assembler.reset();
      this.ui.setActionResult("capture", false, "The capture chunks did not arrive in order.");
      return;
    }
    if (!this.assembler.complete) {
      return;
    }
    const payloadBytes = this.assembler.finish();
    if (payloadBytes === null) {
      this.ui.setActionResult("capture", false, "The capture did not complete and was discarded.");
      this.log({ operation: "capture", errorCode: "INTERNAL_ERROR" });
      return;
    }
    this.forwardCapture(record.transfer_id, payloadBytes);
  }

  private forwardCapture(transferId: string, bytes: Uint8Array): void {
    const answerId = this.completeRequest(this.chunkRequestId);
    this.chunkRequestId = null;
    const chunks = splitIntoChunks(bytes, transferId);
    for (const chunk of chunks) {
      this.bridge.sendFrame(
        "chunk",
        {
          transfer_id: chunk.transferId,
          sequence: chunk.sequence,
          bytes: base64Encode(chunk.bytes),
          total_bytes: chunk.totalBytes,
          ...(chunk.checksum === null ? {} : { checksum: chunk.checksum }),
        },
        answerId ?? undefined,
      );
    }
    this.bridge.sendFrame(
      "result",
      { data: { transfer_id: transferId, byte_count: bytes.length } },
      answerId ?? undefined,
    );
    this.mode = "snapshot";
    this.ui.setActionResult("capture", true, "Snapshot captured and sent to the local daemon.");
    this.log({ operation: "capture", count: bytes.length });
    this.render();
  }

  /**
   * Close the request that the sandbox answered and return its identifier.
   *
   * The identifier of the envelope wins, because the sandbox echoes the
   * request id of the request it answered. The local queue is the fallback for
   * a message that arrives without one. The next request of the queue is
   * forwarded afterwards.
   */
  private completeRequest(answerRequestId: string | null): string | null {
    const inFlight = this.inFlight;
    const requestId = answerRequestId ?? inFlight;
    if (inFlight !== null && requestId === inFlight) {
      this.inFlight = null;
      this.daemonFrames.delete(inFlight);
      this.forwardNext();
      return requestId;
    }
    if (requestId !== null) {
      this.daemonFrames.delete(requestId);
    }
    return requestId;
  }

  private describeError(error: UiError): string {
    if (error.code === "BUSY") {
      const wait = error.retry_after_ms;
      return typeof wait === "number"
        ? `BUSY. The daemon is busy. Retry in ${Math.max(0, Math.trunc(wait))} ms.`
        : "BUSY. The daemon is busy. Retry shortly.";
    }
    if (error.code === "PLUGIN_OFFLINE") {
      return "PLUGIN_OFFLINE. Reopen the plugin and pair again.";
    }
    if (error.code === "REVOKED") {
      return "REVOKED. The grant is gone. A new grant is a user action.";
    }
    if (error.code === "PAUSED") {
      return "PAUSED. The session is paused. Resume it to allow reads again.";
    }
    return `${error.code}. ${error.message}`;
  }

  private readRoots(payload: unknown): CandidateRoot[] {
    if (payload === null || typeof payload !== "object") {
      return [];
    }
    const record = payload as { roots?: unknown };
    if (!Array.isArray(record.roots)) {
      return [];
    }
    const roots: CandidateRoot[] = [];
    for (const item of record.roots) {
      if (item === null || typeof item !== "object") {
        continue;
      }
      const nodeId = (item as { node_id?: unknown }).node_id;
      if (typeof nodeId !== "string" || nodeId.length === 0) {
        continue;
      }
      const nodeType = (item as { node_type?: unknown }).node_type;
      const childCount = (item as { child_count?: unknown }).child_count;
      roots.push({
        node_id: nodeId,
        node_type: typeof nodeType === "string" ? nodeType : "node",
        child_count: typeof childCount === "number" ? childCount : null,
      });
    }
    return roots;
  }

  private readError(payload: unknown): UiError {
    if (payload === null || typeof payload !== "object") {
      return {
        code: "INTERNAL_ERROR",
        message: "The editor reported an error.",
        retryable: false,
        action: "contact_support",
        request_id: null,
        retry_after_ms: null,
      };
    }
    const record = payload as {
      code?: unknown;
      message?: unknown;
      retryable?: unknown;
      action?: unknown;
      request_id?: unknown;
      retry_after_ms?: unknown;
    };
    return {
      code: typeof record.code === "string" ? record.code : "INTERNAL_ERROR",
      message:
        typeof record.message === "string" ? record.message : "The editor reported an error.",
      retryable: record.retryable === true,
      action: typeof record.action === "string" ? record.action : "contact_support",
      request_id: typeof record.request_id === "string" ? record.request_id : null,
      retry_after_ms: typeof record.retry_after_ms === "number" ? record.retry_after_ms : null,
    };
  }

  private toCapabilityViews(capabilities: CapabilityMap): CapabilityView[] {
    const views: CapabilityView[] = [];
    for (const [name, state] of Object.entries(capabilities)) {
      if (state === null || typeof state !== "object") {
        continue;
      }
      const available = state.status === "available";
      const reason = "reason" in state && typeof state.reason === "string" ? state.reason : null;
      views.push({ name, available, reason });
    }
    return views;
  }

  private clearLocalState(): void {
    this.secret.clear();
    this.assembler.reset();
    this.sessionName = null;
    this.sessionFingerprint = null;
    this.grantedRootIds = [];
    this.selectedRootIds = [];
    this.candidates = [];
    this.capabilities = [];
    this.queueDepth = null;
    this.lastReadAt = null;
    this.mode = "unknown";
    this.paused = false;
    this.grantPending = false;
    this.writeGrantEnabled = false;
    this.writePreview = null;
    this.writeApprovalId = null;
    this.writeDecision = "none";
    this.writeResult = null;
    this.waiting.length = 0;
    this.inFlight = null;
    this.chunkRequestId = null;
    this.daemonFrames.clear();
  }

  private log(entry: LogEntry): void {
    this.ui.appendLog(formatLogLine(entry, this.now()));
  }

  private render(): void {
    const phase = derivePhase(this.connection, this.paused, this.grantedRootIds.length > 0);
    const connected = this.connection === "connected";
    const hasGrant = this.grantedRootIds.length > 0;
    const preview = this.writePreview;
    const canDecide = preview !== null && this.writeDecision === "none";
    this.ui.render({
      connection: this.connection,
      connectionText: connectionLabel(this.connection),
      connectionMarker: connectionMarker(this.connection),
      phase,
      phaseText: phaseLabel(phase),
      sessionName: this.sessionName ?? NO_SESSION,
      sessionFingerprint: this.sessionFingerprint ?? NO_FINGERPRINT,
      grantedRootCount: this.grantedRootIds.length,
      modeText: modeLabel(this.mode),
      lastReadText: this.lastReadAt === null ? "No read yet" : formatTimestamp(this.lastReadAt),
      queueDepthText: this.queueDepth === null ? NOT_REPORTED : String(this.queueDepth),
      capabilities: this.capabilities,
      candidates: this.candidates,
      selectedRootIds: this.selectedRootIds,
      canPair: this.connection !== "connected" && this.connection !== "connecting",
      canGrant: connected && !this.paused && this.selectedRootIds.length > 0 && !this.grantPending,
      canPause: connected && hasGrant && !this.paused,
      canResume: connected && hasGrant && this.paused,
      canCapture: connected && hasGrant && !this.paused,
      canRevoke: connected && hasGrant,
      revokeConfirmVisible: this.revokeConfirmVisible,
      grantPending: this.grantPending,
      writeGrantEnabled: this.writeGrantEnabled,
      canGrantWrite: connected && hasGrant && !this.paused,
      writePreview: preview,
      writeDecision: this.writeDecision,
      // A blocked plan is never approved, and a plan that was decided is
      // never decided twice.
      canApproveWrite: canDecide && preview.blocked === false && this.writeApprovalId !== null,
      canRejectWrite: canDecide && this.writeApprovalId !== null,
      writeResult: this.writeResult,
    });
  }
}
