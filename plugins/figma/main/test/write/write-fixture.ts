/**
 * The fixture of the write path tests.
 *
 * The fixture builds a fake document, a handler context and the plans of the
 * tests, so every test file reads the same way.
 */

import type { HandlerContext } from "../../src/handlers.js";
import { FontCatalog } from "../../src/normalize.js";
import { ScopeGuard } from "../../src/scope.js";
import { TokenResolver } from "../../src/tokens.js";
import { createFakeDocument, type FakeDocument } from "../fake-figma.js";

/** Build a handler context with a grant over the given roots. */
export function createWriteContext(
  document: FakeDocument,
  rootIds: string[],
): HandlerContext {
  const guard = new ScopeGuard(document.api);
  guard.setGrant({ rootIds, grantEpoch: 1 });
  return {
    requestId: "req_write",
    sessionId: "session_1",
    grantEpoch: 1,
    api: document.api,
    guard,
    resolver: new TokenResolver(document.api),
    fontCatalog: new FontCatalog(document.api),
    capabilities: { capabilities: {}, notes: {} },
    isCancelled: () => false,
    onProgress: () => undefined,
  };
}

/**
 * The document the tests work on.
 *
 * The grant covers the frame `1:1`. The frame `2:1` is outside of it, the
 * frame `1:5` is read only, the instance `1:6` holds a child that cannot
 * move, and the component `1:9` holds a frame a component cannot move into.
 */
export function sampleDocument(options?: {
  versionHistory?: boolean;
}): FakeDocument {
  return createFakeDocument(
    [
      {
        id: "1:1",
        type: "FRAME",
        name: "Root",
        width: 400,
        height: 300,
        x: 0,
        y: 0,
        children: [
          {
            id: "1:2",
            type: "TEXT",
            name: "Label",
            characters: "Pay now",
            fontName: { family: "Inter", style: "Regular" },
            fontSize: 14,
            x: 10,
            y: 20,
          },
          {
            id: "1:3",
            type: "FRAME",
            name: "Nested",
            width: 100,
            height: 50,
            children: [{ id: "1:4", type: "ELLIPSE", name: "Dot" }],
          },
          {
            id: "1:5",
            type: "FRAME",
            name: "Read only",
            locked: true,
            children: [],
          },
          {
            id: "1:6",
            type: "INSTANCE",
            name: "Instance",
            children: [{ id: "1:7", type: "TEXT", name: "Inside" }],
          },
          {
            id: "1:8",
            type: "FRAME",
            name: "Component host",
            children: [
              {
                id: "1:9",
                type: "COMPONENT",
                name: "Component",
                children: [{ id: "1:10", type: "FRAME", name: "Inside component" }],
              },
            ],
          },
        ],
      },
      {
        id: "2:1",
        type: "FRAME",
        name: "Other root",
        children: [{ id: "2:2", type: "ELLIPSE", name: "Other dot" }],
      },
    ],
    {
      fonts: [
        { family: "Inter", style: "Regular" },
        { family: "Inter", style: "Semi Bold" },
      ],
      ...(options?.versionHistory === undefined ? {} : { versionHistory: options.versionHistory }),
    },
  );
}

/** One operation of a plan, as the daemon sends it. */
export type Operation = Record<string, unknown>;

/** Build the payload of a write request. */
export function planPayload(
  operations: Operation[],
  options?: {
    planId?: string;
    destructive?: boolean;
    saveVersion?: boolean;
    approvalId?: string;
    planHash?: string;
  },
): Record<string, unknown> {
  const payload: Record<string, unknown> = {
    plan: {
      plan_id: options?.planId ?? "plan_1",
      session_id: "session_1",
      intent: "a change of the design system",
      destructive: options?.destructive ?? false,
      operations,
    },
  };
  if (options?.saveVersion === true) {
    payload["save_version"] = true;
  }
  if (options?.approvalId !== undefined) {
    payload["approval_id"] = options.approvalId;
  }
  if (options?.planHash !== undefined) {
    payload["plan_hash"] = options.planHash;
  }
  return payload;
}
