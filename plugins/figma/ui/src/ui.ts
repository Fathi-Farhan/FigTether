/**
 * Entry point of the plugin user interface.
 *
 * The module wires the DOM, the sandbox channel and the bridge socket
 * together. It is the only file with side effects; every other module is
 * pure enough to be tested without a browser.
 *
 * The build of the plugin bundle uses this file as the entry of the UI.
 */

import "../styles.css";

import { AppController } from "./controller.js";
import { DomUi } from "./dom.js";
import { randomUuid } from "./random.js";
import { createSandboxChannel } from "./sandbox.js";
import type { SandboxChannel, WindowLike } from "./sandbox.js";

interface PolicyViolationEvent extends Event {
  readonly effectiveDirective?: string;
  readonly blockedURI?: string;
}

function bootstrap(): void {
  let controller: AppController | null = null;

  const channel: SandboxChannel = createSandboxChannel({
    window: window as unknown as WindowLike,
    handlers: {
      onEnvelope: (envelope): void => {
        controller?.onSandboxEnvelope(envelope);
      },
      onRejected: (): void => {
        // A message that is not a valid envelope is dropped without a reply.
      },
    },
  });

  const ui = new DomUi(document, {
    onPair: (secret): void => controller?.onPairRequested(secret),
    onSelectionChanged: (rootIds): void => controller?.onRootSelectionChanged(rootIds),
    onSelectionRefresh: (): void => controller?.onSelectionRefreshRequested(),
    onAllow: (): void => controller?.onAllowRequested(),
    onWriteGrant: (): void => controller?.onWriteGrantRequested(),
    onPause: (): void => controller?.onPauseRequested(),
    onResume: (): void => controller?.onResumeRequested(),
    onCapture: (): void => controller?.onCaptureRequested(),
    onRevoke: (): void => controller?.onRevokeRequested(),
    onRevokeConfirm: (): void => controller?.onRevokeConfirmed(),
    onRevokeCancel: (): void => controller?.onRevokeCancelled(),
    onApproveWrite: (): void => controller?.onApproveWriteRequested(),
    onRejectWrite: (reason): void => controller?.onRejectWriteRequested(reason),
  });

  controller = new AppController({
    sandbox: channel,
    ui,
    bridgeOptions: {
      clientId: randomUuid(),
    },
  });

  document.addEventListener("securitypolicyviolation", (event: Event): void => {
    const violation = event as PolicyViolationEvent;
    if (typeof violation.blockedURI === "string" && violation.blockedURI.includes("127.0.0.1")) {
      controller?.reportPolicyViolation();
    }
  });

  window.addEventListener("unload", (): void => {
    controller?.dispose();
  });

  controller.start();
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", bootstrap);
} else {
  bootstrap();
}
