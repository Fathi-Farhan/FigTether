import { describe, expect, it } from "vitest";

import {
  base64EncodedLength,
  exportPixelCount,
  MAX_EXPORT_PIXELS,
  withinExportPixelBudget,
  withinExportScaleRange,
  withinFrameBudget,
  withinFrameByteBudget,
  withinTransferBudget,
} from "../src/bytes.js";
import { MAX_FRAME_BYTES, MAX_TRANSFER_BYTES } from "../src/types.js";

describe("byte budgets", () => {
  it("accepts a payload inside the frame budget", () => {
    expect(withinFrameBudget("x".repeat(1024))).toBe(true);
  });

  it("refuses a payload above the frame budget", () => {
    expect(withinFrameBudget("x".repeat(MAX_FRAME_BYTES + 1))).toBe(false);
  });

  it("measures the frame budget in bytes and not in characters", () => {
    expect(withinFrameBudget("é".repeat(MAX_FRAME_BYTES))).toBe(false);
  });

  it("checks the frame and the transfer budget from a byte count", () => {
    expect(withinFrameByteBudget(MAX_FRAME_BYTES)).toBe(true);
    expect(withinFrameByteBudget(MAX_FRAME_BYTES + 1)).toBe(false);
    expect(withinTransferBudget(MAX_TRANSFER_BYTES)).toBe(true);
    expect(withinTransferBudget(MAX_TRANSFER_BYTES + 1)).toBe(false);
  });

  it("computes the length of a base64 payload", () => {
    expect(base64EncodedLength(0)).toBe(0);
    expect(base64EncodedLength(3)).toBe(4);
    expect(base64EncodedLength(4)).toBe(8);
  });

  it("counts the pixels of an export", () => {
    expect(exportPixelCount(100, 100, 1)).toBe(10000);
    expect(exportPixelCount(100, 100, 2)).toBe(40000);
    expect(exportPixelCount(100.5, 100.5, 1)).toBe(10201);
  });

  it("refuses an export above sixteen megapixels", () => {
    expect(withinExportPixelBudget(5000, 5000, 1)).toBe(false);
    expect(withinExportPixelBudget(5000, 5000, 0.25)).toBe(true);
    expect(MAX_EXPORT_PIXELS).toBe(16 * 1024 * 1024);
  });

  it("accepts a scale inside the range and refuses the rest", () => {
    expect(withinExportScaleRange(0.25)).toBe(true);
    expect(withinExportScaleRange(2)).toBe(true);
    expect(withinExportScaleRange(0.24)).toBe(false);
    expect(withinExportScaleRange(2.01)).toBe(false);
    expect(withinExportScaleRange(Number.NaN)).toBe(false);
  });
});
