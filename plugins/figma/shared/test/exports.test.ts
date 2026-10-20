import { describe, expect, it } from "vitest";
import * as shared from "../src/index";
import type {
  AcceptedBridgeOp,
  AuthAccepted,
  AuthRejected,
  AuthRequest,
  BridgeEnvelope,
  BridgeFrame,
  BridgeOp,
  CapabilitiesPayload,
  ChunkPayload,
  DaemonMessageKind,
  ErrorPayload,
  GrantMode,
  GrantUpdatePayload,
  HeartbeatPayload,
  InboundMessageKind,
  LocalMessageKind,
  MessageType,
  OutboundMessageKind,
  PluginComponent,
  PluginDocument,
  PluginEffect,
  PluginExport,
  PluginGeometry,
  PluginGradientStop,
  PluginLayout,
  PluginNode,
  PluginPaint,
  PluginText,
  PluginTextRun,
  PluginVariable,
  PluginVariantProperty,
  RequestPayload,
  ResultPayload,
  RevokePayload,
  UiToDaemonMessageKind,
  WriteApproval,
  WriteBridgeOp,
  WriteOperation,
  WriteOperationKind,
  WritePlan,
  WritePreview,
  WriteResult,
} from "../src/index";

/**
 * The names the other two work streams import. The list is a contract: a
 * rename in the shared package has to be a deliberate change.
 */
const REQUIRED_TYPES = [
  "BridgeFrame",
  "MessageType",
  "BridgeOp",
  "AuthRequest",
  "AuthAccepted",
  "AuthRejected",
  "RequestPayload",
  "ResultPayload",
  "ErrorPayload",
  "HeartbeatPayload",
  "GrantUpdatePayload",
  "RevokePayload",
  "ChunkPayload",
  "CapabilitiesPayload",
  "BridgeEnvelope",
  "DaemonMessageKind",
  "LocalMessageKind",
  "OutboundMessageKind",
  "InboundMessageKind",
  "PluginNode",
  "PluginGeometry",
  "PluginLayout",
  "PluginText",
  "PluginTextRun",
  "PluginPaint",
  "PluginGradientStop",
  "PluginEffect",
  "PluginComponent",
  "PluginVariantProperty",
  "PluginVariable",
  "PluginDocument",
  "PluginExport",
  "PluginCapabilities",
  "GrantMode",
  "WriteOperationKind",
  "WriteOperation",
  "WritePlan",
  "WritePreview",
  "WriteApproval",
  "WriteResult",
  "UiToDaemonMessageKind",
  "WriteBridgeOp",
  "AcceptedBridgeOp",
] as const;

/** Compile time use of every exported type, so a rename breaks the build. */
type ExportedTypes = [
  BridgeFrame,
  MessageType,
  BridgeOp,
  AuthRequest,
  AuthAccepted,
  AuthRejected,
  RequestPayload,
  ResultPayload,
  ErrorPayload,
  HeartbeatPayload,
  GrantUpdatePayload,
  RevokePayload,
  ChunkPayload,
  CapabilitiesPayload,
  BridgeEnvelope,
  DaemonMessageKind,
  LocalMessageKind,
  OutboundMessageKind,
  InboundMessageKind,
  PluginNode,
  PluginGeometry,
  PluginLayout,
  PluginText,
  PluginTextRun,
  PluginPaint,
  PluginGradientStop,
  PluginEffect,
  PluginComponent,
  PluginVariantProperty,
  PluginVariable,
  PluginDocument,
  PluginExport,
  shared.PluginCapabilities,
  GrantMode,
  WriteOperationKind,
  WriteOperation,
  WritePlan,
  WritePreview,
  WriteApproval,
  WriteResult,
  UiToDaemonMessageKind,
  WriteBridgeOp,
  AcceptedBridgeOp,
];

describe("shared package surface", () => {
  it("exports the protocol constants", () => {
    expect(shared.PROTOCOL_VERSION).toBe("1.0");
    expect(shared.MAX_FRAME_BYTES).toBe(1024 * 1024);
    expect(shared.MAX_TRANSFER_BYTES).toBe(32 * 1024 * 1024);
    expect(shared.AUTH_WINDOW_MS).toBe(3000);
    expect(shared.HEARTBEAT_INTERVAL_MS).toBe(5000);
    expect(shared.MISSED_HEARTBEATS_BEFORE_OFFLINE).toBe(3);
  });

  it("exports every message type and operation of the Rust contract", () => {
    expect(shared.MESSAGE_TYPES).toEqual([
      "hello_auth",
      "capabilities",
      "grant_update",
      "request",
      "progress",
      "chunk",
      "result",
      "error",
      "cancel",
      "heartbeat",
      "revoke",
      "approval",
    ]);
    expect(shared.BRIDGE_OPS).toEqual([
      "list_nodes",
      "read_nodes",
      "inspect_design",
      "get_design_tokens",
      "export_assets",
      "get_selection",
      "capture_snapshot",
      "list_sessions",
      "watch_scope",
      "get_changes",
    ]);
  });

  it("exports the helpers", () => {
    for (const name of [
      "base64Encode",
      "base64Decode",
      "base64EncodeText",
      "base64DecodeText",
      "canonicalStringify",
      "parseJsonValue",
      "assertJsonValue",
      "sha256",
      "sha256Hex",
      "sha256HexOfText",
      "utf8Encode",
      "utf8Decode",
      "createFrame",
      "serializeFrame",
      "parseFrame",
      "readPayload",
      "createChunk",
      "isMessageType",
      "isBridgeOp",
      "isCapabilityState",
      "isCapabilityAvailable",
      "capabilityReason",
      "capabilityAvailable",
      "capabilityUnavailable",
      "capabilityUnsupported",
      "createPluginNode",
      "createPluginGeometry",
      "createPluginLayout",
      "createPluginText",
      "createPluginPaint",
      "createPluginEffect",
      "createPluginVariable",
      "createPluginDocument",
      "unwrapPluginMessage",
      "isAcceptedBridgeOp",
      "isDestructivePlan",
      "planOperationCount",
      "planTextLength",
      "validatePlanShape",
      "canonicalPlanJson",
    ]) {
      expect(typeof shared[name as keyof typeof shared], name).toBe("function");
    }
  });

  it("exports the contract of the postMessage channel", () => {
    // The value of the channel is pinned by the drift guard of the shared
    // package, so the literal does not appear a second time in the tree.
    expect(typeof shared.BRIDGE_CHANNEL).toBe("string");
    expect(shared.BRIDGE_CHANNEL.length).toBeGreaterThan(0);
    expect(shared.BRIDGE_CHANNEL).toBe(shared.BRIDGE_CHANNEL.trim());
    expect(shared.DAEMON_MESSAGE_KINDS).toEqual([
      "result",
      "error",
      "progress",
      "chunk",
      "capabilities",
      "selection",
      "preview",
      "write_result",
    ]);
    expect(shared.UI_TO_DAEMON_MESSAGE_KINDS).toEqual(["approval"]);
    expect(shared.LOCAL_MESSAGE_KINDS).toEqual(["grant_verified", "status", "capture_chunk"]);
    expect(shared.OUTBOUND_MESSAGE_KINDS).toEqual([
      ...shared.DAEMON_MESSAGE_KINDS,
      ...shared.LOCAL_MESSAGE_KINDS,
    ]);
    expect(shared.INBOUND_MESSAGE_KINDS).toEqual(shared.DAEMON_MESSAGE_KINDS);
  });

  it("exports the contract of the write path", () => {
    expect(shared.WRITE_BRIDGE_OPS).toEqual(["preview_write", "apply_write"]);
    expect(shared.ACCEPTED_BRIDGE_OPS).toEqual([...shared.BRIDGE_OPS, ...shared.WRITE_BRIDGE_OPS]);
    expect(shared.isAcceptedBridgeOp("preview_write")).toBe(true);
    expect(shared.isAcceptedBridgeOp("apply_write")).toBe(true);
    expect(shared.isAcceptedBridgeOp("read_nodes")).toBe(true);
    expect(shared.isAcceptedBridgeOp("drop_table")).toBe(false);
    expect(shared.WRITE_APPROVAL_TTL_MS).toBe(600_000);
    expect(shared.WRITE_NODE_TYPES.length).toBeGreaterThan(0);
    expect(shared.ALLOWED_WRITE_PROPERTIES.length).toBeGreaterThan(0);
  });

  it("keeps the exported type names in the contract", () => {
    expect(REQUIRED_TYPES.length).toBe(43);
    // The type only exists so the compiler checks the list above.
    const marker: ExportedTypes | undefined = undefined;
    expect(marker).toBeUndefined();
  });
});
