/**
 * The contract of the channel between the two sides of the plugin.
 *
 * The channel, the envelope and the vocabulary of the answers live in the
 * shared package `@figtether/shared`. This test checks at runtime that the
 * main sandbox and the user interface really use the shared definitions and
 * not a copy of them, so a drift fails a test instead of the plugin.
 */
import {
  BRIDGE_CHANNEL,
  DAEMON_MESSAGE_KINDS,
  INBOUND_MESSAGE_KINDS,
  LOCAL_MESSAGE_KINDS,
  OUTBOUND_MESSAGE_KINDS,
} from "@figtether/shared";
import { describe, expect, it } from "vitest";

import { BRIDGE_CHANNEL as MAIN_BRIDGE_CHANNEL, OUTBOUND_KINDS } from "../../main/src/bridge.js";
import { UI_CHANNEL, UI_INBOUND_TYPES } from "../src/protocol.js";

describe("the contract of the channel between the two sides of the plugin", () => {
  it("uses one channel value on both sides", () => {
    expect(MAIN_BRIDGE_CHANNEL).toBe(BRIDGE_CHANNEL);
    expect(UI_CHANNEL).toBe(BRIDGE_CHANNEL);
  });

  it("derives the vocabulary of the answers from the shared source", () => {
    expect(OUTBOUND_KINDS).toEqual(OUTBOUND_MESSAGE_KINDS);
    expect(UI_INBOUND_TYPES).toEqual(INBOUND_MESSAGE_KINDS);
  });

  it("keeps the kinds of the daemon and the kinds of the plugin apart", () => {
    expect(UI_INBOUND_TYPES).toEqual(DAEMON_MESSAGE_KINDS);
    expect(OUTBOUND_MESSAGE_KINDS).toEqual([...DAEMON_MESSAGE_KINDS, ...LOCAL_MESSAGE_KINDS]);
    for (const kind of DAEMON_MESSAGE_KINDS) {
      expect(LOCAL_MESSAGE_KINDS).not.toContain(kind);
    }
  });
});
