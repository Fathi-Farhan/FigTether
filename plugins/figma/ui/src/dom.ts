/**
 * DOM implementation of the user interface port.
 *
 * The controller never touches the document. It renders a model into the
 * elements collected here. Every control is a native element with a label,
 * the status areas are live regions, and the focus outline is never removed.
 */

import type { ActionName, CapabilityView, UiModel, UiPort } from "./controller.js";
import type { CandidateRoot } from "./protocol.js";

/** The elements the user interface renders into. */
export interface UiElements {
  readonly connectionSummary: HTMLElement;
  readonly connectionState: HTMLElement;
  readonly sessionName: HTMLElement;
  readonly sessionFingerprint: HTMLElement;
  readonly pairForm: HTMLFormElement;
  readonly secretInput: HTMLInputElement;
  readonly pairButton: HTMLButtonElement;
  readonly pairError: HTMLElement;
  readonly pairFeedback: HTMLElement;
  readonly grantEmpty: HTMLElement;
  readonly candidateList: HTMLElement;
  readonly refreshButton: HTMLButtonElement;
  readonly allowButton: HTMLButtonElement;
  readonly grantFeedback: HTMLElement;
  readonly writeGrantButton: HTMLButtonElement;
  readonly writeGrantState: HTMLElement;
  readonly writeGrantFeedback: HTMLElement;
  readonly statusConnection: HTMLElement;
  readonly statusRoots: HTMLElement;
  readonly statusMode: HTMLElement;
  readonly statusLastRead: HTMLElement;
  readonly statusQueue: HTMLElement;
  readonly capabilityList: HTMLElement;
  readonly writeResultCreated: HTMLElement;
  readonly writeResultChanged: HTMLElement;
  readonly writeResultMoved: HTMLElement;
  readonly writeResultRemoved: HTMLElement;
  readonly writeResultUndo: HTMLElement;
  readonly writeResultVersion: HTMLElement;
  readonly writeResultFeedback: HTMLElement;
  readonly writeApprovalPanel: HTMLElement;
  readonly writeApprovalState: HTMLElement;
  readonly writeApprovalPlan: HTMLElement;
  readonly writeCountCreate: HTMLElement;
  readonly writeCountChange: HTMLElement;
  readonly writeCountMove: HTMLElement;
  readonly writeCountRemove: HTMLElement;
  readonly writePlanIntent: HTMLElement;
  readonly writeWarningList: HTMLElement;
  readonly writeWarningEmpty: HTMLElement;
  readonly writeBlocked: HTMLElement;
  readonly writeBlockedList: HTMLElement;
  readonly writeApproveButton: HTMLButtonElement;
  readonly writeRejectButton: HTMLButtonElement;
  readonly writeRejectReason: HTMLInputElement;
  readonly writeApprovalFeedback: HTMLElement;
  readonly pauseButton: HTMLButtonElement;
  readonly resumeButton: HTMLButtonElement;
  readonly captureButton: HTMLButtonElement;
  readonly revokeButton: HTMLButtonElement;
  readonly revokeConfirm: HTMLElement;
  readonly revokeConfirmButton: HTMLButtonElement;
  readonly revokeCancelButton: HTMLButtonElement;
  readonly controlFeedback: HTMLElement;
  readonly logList: HTMLElement;
}

/** Callbacks the user interface triggers. */
export interface UiCallbacks {
  onPair(secret: string): void;
  onSelectionChanged(rootIds: readonly string[]): void;
  onSelectionRefresh(): void;
  onAllow(): void;
  onWriteGrant(): void;
  onPause(): void;
  onResume(): void;
  onCapture(): void;
  onRevoke(): void;
  onRevokeConfirm(): void;
  onRevokeCancel(): void;
  onApproveWrite(): void;
  onRejectWrite(reason: string): void;
}

function requireElement<T extends HTMLElement>(doc: Document, id: string): T {
  const element = doc.getElementById(id);
  if (element === null) {
    throw new Error(`The FigTether UI is missing the element ${id}.`);
  }
  return element as T;
}

/** Collect the elements of the user interface from a document. */
export function collectElements(doc: Document): UiElements {
  return {
    connectionSummary: requireElement(doc, "connection-summary"),
    connectionState: requireElement(doc, "status-connection"),
    sessionName: requireElement(doc, "session-name"),
    sessionFingerprint: requireElement(doc, "session-fingerprint"),
    pairForm: requireElement<HTMLFormElement>(doc, "pair-form"),
    secretInput: requireElement<HTMLInputElement>(doc, "pairing-secret"),
    pairButton: requireElement<HTMLButtonElement>(doc, "pair-button"),
    pairError: requireElement(doc, "pair-error"),
    pairFeedback: requireElement(doc, "pair-feedback"),
    grantEmpty: requireElement(doc, "grant-empty"),
    candidateList: requireElement(doc, "candidate-roots"),
    refreshButton: requireElement<HTMLButtonElement>(doc, "refresh-button"),
    allowButton: requireElement<HTMLButtonElement>(doc, "allow-button"),
    grantFeedback: requireElement(doc, "grant-feedback"),
    writeGrantButton: requireElement<HTMLButtonElement>(doc, "write-grant-button"),
    writeGrantState: requireElement(doc, "write-grant-state"),
    writeGrantFeedback: requireElement(doc, "write-grant-feedback"),
    statusConnection: requireElement(doc, "status-connection"),
    statusRoots: requireElement(doc, "status-roots"),
    statusMode: requireElement(doc, "status-mode"),
    statusLastRead: requireElement(doc, "status-last-read"),
    statusQueue: requireElement(doc, "status-queue"),
    capabilityList: requireElement(doc, "capability-list"),
    writeResultCreated: requireElement(doc, "write-result-created"),
    writeResultChanged: requireElement(doc, "write-result-changed"),
    writeResultMoved: requireElement(doc, "write-result-moved"),
    writeResultRemoved: requireElement(doc, "write-result-removed"),
    writeResultUndo: requireElement(doc, "write-result-undo"),
    writeResultVersion: requireElement(doc, "write-result-version"),
    writeResultFeedback: requireElement(doc, "write-result-feedback"),
    writeApprovalPanel: requireElement(doc, "write-approval-panel"),
    writeApprovalState: requireElement(doc, "write-approval-state"),
    writeApprovalPlan: requireElement(doc, "write-approval-plan"),
    writeCountCreate: requireElement(doc, "write-count-create"),
    writeCountChange: requireElement(doc, "write-count-change"),
    writeCountMove: requireElement(doc, "write-count-move"),
    writeCountRemove: requireElement(doc, "write-count-remove"),
    writePlanIntent: requireElement(doc, "write-plan-intent"),
    writeWarningList: requireElement(doc, "write-warning-list"),
    writeWarningEmpty: requireElement(doc, "write-warning-empty"),
    writeBlocked: requireElement(doc, "write-blocked"),
    writeBlockedList: requireElement(doc, "write-blocked-list"),
    writeApproveButton: requireElement<HTMLButtonElement>(doc, "write-approve-button"),
    writeRejectButton: requireElement<HTMLButtonElement>(doc, "write-reject-button"),
    writeRejectReason: requireElement<HTMLInputElement>(doc, "write-reject-reason"),
    writeApprovalFeedback: requireElement(doc, "write-approval-feedback"),
    pauseButton: requireElement<HTMLButtonElement>(doc, "pause-button"),
    resumeButton: requireElement<HTMLButtonElement>(doc, "resume-button"),
    captureButton: requireElement<HTMLButtonElement>(doc, "capture-button"),
    revokeButton: requireElement<HTMLButtonElement>(doc, "revoke-button"),
    revokeConfirm: requireElement(doc, "revoke-confirm"),
    revokeConfirmButton: requireElement<HTMLButtonElement>(doc, "revoke-confirm-button"),
    revokeCancelButton: requireElement<HTMLButtonElement>(doc, "revoke-cancel-button"),
    controlFeedback: requireElement(doc, "control-feedback"),
    logList: requireElement(doc, "log-list"),
  };
}

function setText(element: HTMLElement, text: string): void {
  element.textContent = text;
}

function describeRoot(root: CandidateRoot, index: number): string {
  const type = root.node_type.length > 0 ? root.node_type : "node";
  const children = root.child_count === null ? "" : `, ${root.child_count} children`;
  return `Root ${index + 1}, ${type}${children}, id ${root.node_id}`;
}

function describeCapability(capability: CapabilityView): string {
  if (capability.available) {
    return `${capability.name}: available`;
  }
  const reason = capability.reason ?? "unavailable without a reported reason";
  return `${capability.name}: unavailable, ${reason}`;
}

/**
 * The sentence that names the state of the proposed plan.
 *
 * The state is always named in words, so it never depends on the color of
 * the panel.
 */
function describeDecision(model: UiModel): string {
  const preview = model.writePreview;
  if (preview === null) {
    return "There is no proposed change.";
  }
  if (preview.blocked) {
    return "This plan is blocked. It cannot be applied as it is written.";
  }
  switch (model.writeDecision) {
    case "approved":
      return "You approved this plan. The daemon applies it, and the change lands as one step in the undo history of Figma.";
    case "rejected":
      return "You rejected this plan. The daemon will not apply it.";
    default:
      return "This plan is a proposal. Nothing has changed yet, and nothing changes before you approve it here.";
  }
}

/** The DOM backed user interface. */
export class DomUi implements UiPort {
  private readonly elements: UiElements;
  private readonly callbacks: UiCallbacks;
  private readonly doc: Document;

  constructor(doc: Document, callbacks: UiCallbacks) {
    this.doc = doc;
    this.elements = collectElements(doc);
    this.callbacks = callbacks;
    this.wire();
  }

  private wire(): void {
    const elements = this.elements;
    elements.pairForm.addEventListener("submit", (event: Event): void => {
      event.preventDefault();
      const secret = elements.secretInput.value;
      elements.secretInput.value = "";
      this.callbacks.onPair(secret);
    });
    elements.refreshButton.addEventListener("click", (): void =>
      this.callbacks.onSelectionRefresh(),
    );
    elements.allowButton.addEventListener("click", (): void => this.callbacks.onAllow());
    elements.writeGrantButton.addEventListener("click", (): void => this.callbacks.onWriteGrant());
    elements.writeApproveButton.addEventListener("click", (): void =>
      this.callbacks.onApproveWrite(),
    );
    elements.writeRejectButton.addEventListener("click", (): void => {
      const reason = elements.writeRejectReason.value;
      elements.writeRejectReason.value = "";
      this.callbacks.onRejectWrite(reason);
    });
    elements.pauseButton.addEventListener("click", (): void => this.callbacks.onPause());
    elements.resumeButton.addEventListener("click", (): void => this.callbacks.onResume());
    elements.captureButton.addEventListener("click", (): void => this.callbacks.onCapture());
    elements.revokeButton.addEventListener("click", (): void => this.callbacks.onRevoke());
    elements.revokeConfirmButton.addEventListener("click", (): void =>
      this.callbacks.onRevokeConfirm(),
    );
    elements.revokeCancelButton.addEventListener("click", (): void =>
      this.callbacks.onRevokeCancel(),
    );
  }

  render(model: UiModel): void {
    const elements = this.elements;
    setText(
      elements.connectionSummary,
      `Connection: ${model.connectionText}. Session phase: ${model.phaseText}.`,
    );
    setText(elements.connectionState, `${model.connectionMarker} ${model.connectionText}`);
    setText(elements.sessionName, model.sessionName);
    setText(elements.sessionFingerprint, model.sessionFingerprint);
    setText(elements.statusRoots, String(model.grantedRootCount));
    setText(elements.statusMode, model.modeText);
    setText(elements.statusLastRead, model.lastReadText);
    setText(elements.statusQueue, model.queueDepthText);

    elements.pairButton.disabled = !model.canPair;
    elements.secretInput.disabled = !model.canPair;
    elements.allowButton.disabled = !model.canGrant;
    elements.writeGrantButton.disabled = !model.canGrantWrite;
    elements.pauseButton.disabled = !model.canPause;
    elements.resumeButton.disabled = !model.canResume;
    elements.captureButton.disabled = !model.canCapture;
    elements.revokeButton.disabled = !model.canRevoke;

    elements.revokeConfirm.hidden = !model.revokeConfirmVisible;

    this.renderCandidates(model);
    this.renderCapabilities(model.capabilities);
    this.renderWriteGrant(model);
    this.renderWritePreview(model);
    this.renderWriteResult(model);
  }

  /**
   * Render the write grant.
   *
   * The control states in words whether editing inside the granted roots is
   * allowed, so the state never depends on the color of the panel.
   */
  private renderWriteGrant(model: UiModel): void {
    const elements = this.elements;
    elements.writeGrantButton.textContent = model.writeGrantEnabled
      ? "Restrict the session to reading"
      : "Allow write session";
    setText(
      elements.writeGrantState,
      model.writeGrantEnabled
        ? "Editing inside the granted roots is allowed. Every plan still needs your approval before anything changes."
        : "Editing inside the granted roots is not allowed. The read grant never allows a change.",
    );
  }

  /**
   * Render the plan the agent proposed.
   *
   * A blocked plan shows its reasons and offers no approve action. The panel
   * never moves the focus, so an accidental key press cannot approve a plan.
   */
  private renderWritePreview(model: UiModel): void {
    const elements = this.elements;
    const preview = model.writePreview;
    if (preview === null) {
      elements.writeApprovalPanel.hidden = true;
      return;
    }
    elements.writeApprovalPanel.hidden = false;
    setText(elements.writeApprovalState, describeDecision(model));
    setText(
      elements.writeApprovalPlan,
      `Plan ${preview.planId === "" ? "without an id" : preview.planId}, fingerprint ${preview.fingerprint}.`,
    );
    setText(elements.writeCountCreate, String(preview.counts.create));
    setText(elements.writeCountChange, String(preview.counts.change));
    setText(elements.writeCountMove, String(preview.counts.move));
    setText(elements.writeCountRemove, String(preview.counts.remove));
    setText(elements.writePlanIntent, preview.intent ?? "The agent did not describe this plan.");
    this.renderReasons(elements.writeWarningList, preview.warnings);
    elements.writeWarningEmpty.hidden = preview.warnings.length > 0;
    this.renderReasons(elements.writeBlockedList, preview.blockedReasons);
    elements.writeBlocked.hidden = !preview.blocked;
    elements.writeApproveButton.hidden = !model.canApproveWrite;
    elements.writeApproveButton.disabled = !model.canApproveWrite;
    elements.writeRejectButton.hidden = !model.canRejectWrite;
    elements.writeRejectButton.disabled = !model.canRejectWrite;
    elements.writeRejectReason.disabled = !model.canRejectWrite;
  }

  /**
   * Render the result of the plan that was applied last.
   *
   * The area names the undo history in words and never promises a revert that
   * FigTether does not provide.
   */
  private renderWriteResult(model: UiModel): void {
    const elements = this.elements;
    const result = model.writeResult;
    if (result === null) {
      setText(elements.writeResultCreated, "0");
      setText(elements.writeResultChanged, "0");
      setText(elements.writeResultMoved, "0");
      setText(elements.writeResultRemoved, "0");
      setText(elements.writeResultUndo, "No write applied yet");
      setText(elements.writeResultVersion, "No version checkpoint");
      return;
    }
    setText(elements.writeResultCreated, String(result.counts.create));
    setText(elements.writeResultChanged, String(result.counts.change));
    setText(elements.writeResultMoved, String(result.counts.move));
    setText(elements.writeResultRemoved, String(result.counts.remove));
    setText(
      elements.writeResultUndo,
      result.undoCommitted
        ? "Committed to the undo history of Figma as one step"
        : "Not committed to the undo history of Figma",
    );
    setText(
      elements.writeResultVersion,
      result.versionSaved === null ? "No version checkpoint" : result.versionSaved,
    );
  }

  /** Render a list of reasons of the plugin. */
  private renderReasons(list: HTMLElement, reasons: readonly string[]): void {
    while (list.firstChild !== null) {
      list.removeChild(list.firstChild);
    }
    for (const reason of reasons) {
      const item = this.doc.createElement("li");
      item.textContent = reason;
      list.appendChild(item);
    }
  }

  private renderCandidates(model: UiModel): void {
    const elements = this.elements;
    const list = elements.candidateList;
    while (list.firstChild !== null) {
      list.removeChild(list.firstChild);
    }
    const selected = new Set(model.selectedRootIds);
    if (model.candidates.length === 0) {
      elements.grantEmpty.hidden = false;
      return;
    }
    elements.grantEmpty.hidden = true;
    model.candidates.forEach((root, index) => {
      const item = this.doc.createElement("li");
      const label = this.doc.createElement("label");
      label.className = "candidate";
      const input = this.doc.createElement("input");
      input.type = "checkbox";
      input.value = root.node_id;
      input.checked = selected.has(root.node_id);
      input.dataset["rootId"] = root.node_id;
      input.addEventListener("change", (): void => {
        const checked: string[] = [];
        const inputs = list.querySelectorAll("input[type='checkbox']");
        inputs.forEach((node) => {
          const box = node as HTMLInputElement;
          if (box.checked) {
            checked.push(box.value);
          }
        });
        this.callbacks.onSelectionChanged(checked);
      });
      const text = this.doc.createElement("span");
      text.textContent = describeRoot(root, index);
      label.appendChild(input);
      label.appendChild(text);
      item.appendChild(label);
      list.appendChild(item);
    });
  }

  private renderCapabilities(capabilities: readonly CapabilityView[]): void {
    const list = this.elements.capabilityList;
    while (list.firstChild !== null) {
      list.removeChild(list.firstChild);
    }
    if (capabilities.length === 0) {
      const item = this.doc.createElement("li");
      item.textContent = "No capability report yet.";
      list.appendChild(item);
      return;
    }
    for (const capability of capabilities) {
      const item = this.doc.createElement("li");
      item.textContent = describeCapability(capability);
      list.appendChild(item);
    }
  }

  appendLog(line: string): void {
    const item = this.doc.createElement("li");
    item.textContent = line;
    this.elements.logList.appendChild(item);
    while (this.elements.logList.childElementCount > 200) {
      const first = this.elements.logList.firstChild;
      if (first === null) {
        break;
      }
      this.elements.logList.removeChild(first);
    }
  }

  setPairError(message: string | null): void {
    const element = this.elements.pairError;
    if (message === null || message.length === 0) {
      element.textContent = "";
      element.hidden = true;
      return;
    }
    element.textContent = message;
    element.hidden = false;
  }

  setActionResult(action: ActionName, ok: boolean, message: string): void {
    const element = this.feedbackFor(action);
    element.textContent = message;
    element.dataset["state"] = ok ? "ok" : "error";
  }

  /** The live region that carries the outcome of an action. */
  private feedbackFor(action: ActionName): HTMLElement {
    switch (action) {
      case "pair":
        return this.elements.pairFeedback;
      case "grant":
        return this.elements.grantFeedback;
      case "write_grant":
        return this.elements.writeGrantFeedback;
      case "preview":
      case "approve":
      case "reject":
        return this.elements.writeApprovalFeedback;
      case "write_result":
        return this.elements.writeResultFeedback;
      default:
        return this.elements.controlFeedback;
    }
  }

  setRevokeConfirmVisible(visible: boolean): void {
    this.elements.revokeConfirm.hidden = !visible;
    if (visible) {
      this.elements.revokeConfirmButton.focus();
    }
  }

  focusPairInput(): void {
    this.elements.secretInput.focus();
  }
}
