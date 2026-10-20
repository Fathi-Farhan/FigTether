/**
 * Runtime of the plugin main sandbox.
 *
 * The runtime owns the grant, the capability report, the request registry and
 * the bridge counters. It dispatches validated messages to the allowlisted
 * handlers and sends the answers back to the UI.
 *
 * The channel carries two kinds of traffic:
 * - the bridge frames of the daemon, relayed by the UI as `frame` or as the
 *   alias `bridge_frame`;
 * - the local requests of the UI, for example `selection_request`.
 *
 * The answers travel back with the names of the bridge contract, so the user
 * interface can forward every answer to the daemon without a rename. The
 * request id of the answer is the request id of the request it belongs to.
 */

import type { BridgeEnvelope } from "@figtether/shared";

import {
  buildEnvelope,
  createCounters,
  sendChunked,
  validateInbound,
  type BridgeCounters,
  type InboundMessage,
} from "./bridge.js";
import { probeCapabilities, type CapabilityReport } from "./capabilities.js";
import { capabilityUnavailable, outOfScope, paused, toPluginError } from "./errors.js";
import type { FigmaApi } from "./figma-api.js";
import { createHandlerRegistry, handleCaptureSnapshot, type HandlerContext } from "./handlers.js";
import { FontCatalog } from "./normalize.js";
import { ScopeGuard } from "./scope.js";
import { TokenResolver } from "./tokens.js";
import type { BridgeFrame, BridgeOp, ErrorPayload } from "./types.js";
import { utf8Encode } from "./encoding.js";

/** Transport between the sandbox and the UI iframe. */
export interface SandboxTransport {
  /** Register the handler for inbound messages. */
  onMessage(handler: (raw: unknown) => void): void;
  /** Send one message to the UI. */
  post(envelope: BridgeEnvelope): void;
}

/** Options of the runtime. */
export interface RuntimeOptions {
  /** The editor API. */
  api: FigmaApi;
  /** The transport towards the UI. */
  transport: SandboxTransport;
}

/** Runtime of the main sandbox. */
export class PluginRuntime {
  /** Counters of the bridge, exposed for diagnostics. */
  readonly counters: BridgeCounters = createCounters();

  private readonly api: FigmaApi;
  private readonly transport: SandboxTransport;
  private readonly guard: ScopeGuard;
  private readonly registry = createHandlerRegistry();
  private readonly resolver: TokenResolver;
  private readonly fontCatalog: FontCatalog;
  private readonly cancelled = new Set<string>();
  private readonly inFlight = new Set<string>();
  private capabilities: CapabilityReport = { capabilities: {}, notes: {} };
  private sessionId: string | undefined;
  private transferCounter = 0;
  private paused = false;
  private lastReadAt: number | null = null;

  constructor(options: RuntimeOptions) {
    this.api = options.api;
    this.transport = options.transport;
    this.guard = new ScopeGuard(options.api);
    this.resolver = new TokenResolver(options.api);
    this.fontCatalog = new FontCatalog(options.api);
  }

  /** The grant in force. */
  get grant(): { rootIds: string[]; grantEpoch: number } | null {
    return this.guard.current;
  }

  /** Start the runtime: listen for messages and probe the capabilities. */
  async start(): Promise<void> {
    this.transport.onMessage((raw) => {
      void this.handleRaw(raw).catch(() => {
        // A failure of the message pump never breaks the plugin.
      });
    });
    await this.refreshCapabilities();
  }

  /** Recompute the capability report of the running editor. */
  async refreshCapabilities(): Promise<CapabilityReport> {
    try {
      this.capabilities = await probeCapabilities(this.api);
    } catch {
      this.capabilities = {
        capabilities: {
          document_access: {
            status: "unavailable",
            reason: "the document could not be inspected",
          },
        },
        notes: {},
      };
    }
    return this.capabilities;
  }

  /**
   * Handle one raw inbound message.
   *
   * The message is validated first. A rejected message is counted and never
   * reaches a handler.
   */
  async handleRaw(raw: unknown): Promise<void> {
    this.counters.received += 1;
    const validation = validateInbound(raw);
    if (!validation.ok) {
      this.counters.rejected += 1;
      switch (validation.reason) {
        case "unknown_channel":
          this.counters.rejectedUnknownChannel += 1;
          break;
        case "unknown_kind":
          this.counters.rejectedUnknownKind += 1;
          break;
        case "oversized":
          this.counters.rejectedOversized += 1;
          break;
        case "protocol_mismatch":
          this.counters.rejectedProtocolMismatch += 1;
          break;
        default:
          this.counters.rejectedMalformed += 1;
          break;
      }
      return;
    }
    this.counters.accepted += 1;
    await this.dispatch(validation.message);
  }

  private async dispatch(message: InboundMessage): Promise<void> {
    switch (message.kind) {
      case "frame":
        await this.dispatchFrame(message.frame);
        return;
      case "grant_update":
        this.applyGrant(message.rootIds, message.grantEpoch, message.mode);
        await this.refreshCapabilities();
        return;
      case "grant_request":
        await this.verifyGrantRoots(message.rootIds);
        return;
      case "cancel":
        this.cancelled.add(message.requestId);
        return;
      case "revoke":
        this.guard.clearGrant();
        this.paused = false;
        for (const requestId of this.inFlight) {
          this.cancelled.add(requestId);
        }
        return;
      case "capabilities":
        this.sendCapabilities();
        return;
      case "selection_request":
        this.sendSelection();
        return;
      case "status_request":
        this.sendStatus();
        return;
      case "capture_request":
        await this.runCapture();
        return;
      case "ui_ready":
        this.sendStatus();
        return;
      default:
        return;
    }
  }

  private async dispatchFrame(frame: BridgeFrame): Promise<void> {
    switch (frame.message_type) {
      case "request":
        await this.handleRequest(frame);
        return;
      case "grant_update": {
        const payload = frame.payload as {
          root_ids?: unknown;
          grant_epoch?: unknown;
          mode?: unknown;
        };
        const rootIds = Array.isArray(payload?.root_ids)
          ? payload.root_ids.filter((entry): entry is string => typeof entry === "string")
          : [];
        const grantEpoch = typeof payload?.grant_epoch === "number" ? payload.grant_epoch : 0;
        const mode = typeof payload?.mode === "string" ? payload.mode : "live";
        this.applyGrant(rootIds, grantEpoch, mode);
        await this.refreshCapabilities();
        return;
      }
      case "revoke":
        this.guard.clearGrant();
        this.paused = false;
        return;
      case "cancel":
        this.cancelled.add(frame.request_id);
        return;
      case "capabilities":
        this.sendCapabilities();
        return;
      default:
        return;
    }
  }

  private applyGrant(rootIds: string[], grantEpoch: number, mode: string): void {
    this.guard.setGrant({ rootIds, grantEpoch });
    this.paused = mode === "paused";
  }

  private async handleRequest(frame: BridgeFrame): Promise<void> {
    const requestId = frame.request_id;
    if (frame.session_id !== undefined) {
      this.sessionId = frame.session_id;
    }

    const payload = frame.payload as { op?: unknown; params?: unknown } | null;
    const op = typeof payload?.op === "string" ? payload.op : "";
    const handler = op === "" ? undefined : this.registry.get(op as BridgeOp);
    if (handler === undefined) {
      this.sendError(
        requestId,
        capabilityUnavailable("The requested operation is not part of this plugin build"),
      );
      return;
    }

    if (this.paused) {
      this.sendError(requestId, paused("The session is paused; the user has to resume it first"));
      return;
    }

    const grant = this.guard.current;
    if (
      frame.grant_epoch !== undefined &&
      grant !== null &&
      frame.grant_epoch !== grant.grantEpoch
    ) {
      this.sendError(requestId, outOfScope("The request was issued under an older grant epoch"));
      return;
    }

    const context: HandlerContext = {
      requestId,
      sessionId: frame.session_id ?? this.sessionId,
      grantEpoch: frame.grant_epoch,
      deadlineMs: frame.deadline_ms,
      api: this.api,
      guard: this.guard,
      resolver: this.resolver,
      fontCatalog: this.fontCatalog,
      capabilities: this.capabilities,
      isCancelled: () => this.cancelled.has(requestId),
      onProgress: (progress) => {
        this.counters.progressSent += 1;
        this.transport.post(
          buildEnvelope(
            "progress",
            {
              request_id: requestId,
              visited: progress.visited,
              elapsed_ms: Math.round(progress.elapsedMs),
            },
            requestId,
          ),
        );
      },
    };

    try {
      this.inFlight.add(requestId);
      const data = await handler(payload?.params, context);
      this.lastReadAt = Date.now();
      this.counters.resultsSent += 1;
      const result = { request_id: requestId, ok: true, data };
      this.transport.post(buildEnvelope("result", result, requestId));
    } catch (error) {
      this.sendError(requestId, toPluginError(error));
    } finally {
      this.inFlight.delete(requestId);
      this.cancelled.delete(requestId);
    }
  }

  private sendError(requestId: string, error: ReturnType<typeof toPluginError>): void {
    this.counters.errorsSent += 1;
    const payload: ErrorPayload = error.toPayload(requestId);
    this.transport.post(buildEnvelope("error", payload, requestId));
  }

  private sendCapabilities(): void {
    this.transport.post(
      buildEnvelope("capabilities", {
        capabilities: this.capabilities.capabilities,
        notes: this.capabilities.notes,
      }),
    );
  }

  private sendStatus(): void {
    this.transport.post(
      buildEnvelope("status", {
        queue_depth: null,
        last_read_at: this.lastReadAt,
        mode: this.paused ? "paused" : "live",
        capabilities: this.capabilities.capabilities,
      }),
    );
  }

  /**
   * Answer the selection request of the UI.
   *
   * The answer carries the nodes the user selected in the editor. It is a
   * local affordance of the grant flow and never leaves the editor.
   */
  private sendSelection(): void {
    const selection = this.api.currentPage.selection ?? [];
    const roots = selection.map((node) => ({
      node_id: node.id,
      node_type: node.type,
      child_count: (node.children ?? []).length,
    }));
    this.transport.post(buildEnvelope("selection", { roots }));
  }

  /**
   * Verify the roots the user selected.
   *
   * A root that does not exist in the document is refused, so the daemon
   * never receives a grant that the editor cannot honour.
   */
  private async verifyGrantRoots(rootIds: string[]): Promise<void> {
    const verified: string[] = [];
    for (const rootId of rootIds) {
      try {
        const node = await this.guard.resolveNode(rootId);
        verified.push(node.id);
      } catch {
        // A root that does not exist is reported as not granted.
      }
    }
    if (verified.length !== rootIds.length) {
      const error = outOfScope(
        "One of the selected roots does not exist in this document",
      ).toPayload("");
      this.transport.post(
        buildEnvelope("grant_verified", {
          ok: false,
          root_ids: verified,
          error,
        }),
      );
      return;
    }
    this.transport.post(buildEnvelope("grant_verified", { ok: true, root_ids: verified }));
  }

  /**
   * Build a capture and stream it to the UI.
   *
   * The capture is a user action of the UI. The bytes travel as a chunked
   * transfer so the UI can forward them to the daemon.
   */
  private async runCapture(): Promise<void> {
    const context: HandlerContext = {
      requestId: "capture",
      sessionId: this.sessionId,
      api: this.api,
      guard: this.guard,
      resolver: this.resolver,
      fontCatalog: this.fontCatalog,
      capabilities: this.capabilities,
      isCancelled: () => false,
      onProgress: (progress) => {
        this.counters.progressSent += 1;
        this.transport.post(
          buildEnvelope("progress", {
            request_id: "capture",
            visited: progress.visited,
            elapsed_ms: Math.round(progress.elapsedMs),
          }),
        );
      },
    };
    try {
      const capture = await handleCaptureSnapshot({}, context);
      const raw = utf8Encode(JSON.stringify(capture));
      const transferId = this.nextTransferId("capture");
      this.counters.chunksSent += 1;
      sendChunked(
        (envelope) => this.transport.post(envelope),
        "capture_chunk",
        raw,
        transferId,
        "capture",
      );
    } catch (error) {
      this.sendError("capture", toPluginError(error));
    }
  }

  private nextTransferId(requestId: string): string {
    this.transferCounter += 1;
    return `tr_${requestId}_${this.transferCounter}`;
  }
}
