/**
 * Byte budget helpers.
 *
 * The budgets come from `docs/04-data-contracts/06-resource-limits.md`:
 * one mebibyte per bridge frame, thirty two mebibytes per transfer, and at
 * most sixteen megapixels per exported image.
 */

import { utf8ByteLength } from "./encoding.js";
import { MAX_FRAME_BYTES, MAX_TRANSFER_BYTES } from "./types.js";

/** Maximum number of pixels of one exported image. */
export const MAX_EXPORT_PIXELS = 16 * 1024 * 1024;

/** Smallest accepted export scale. */
export const MIN_EXPORT_SCALE = 0.25;

/** Largest accepted export scale. */
export const MAX_EXPORT_SCALE = 2;

/** Whether a payload fits into one bridge frame. */
export function withinFrameBudget(text: string): boolean {
  return utf8ByteLength(text) <= MAX_FRAME_BYTES;
}

/** Whether a payload fits into one bridge frame, given its byte size. */
export function withinFrameByteBudget(byteLength: number): boolean {
  return byteLength <= MAX_FRAME_BYTES;
}

/** Whether a transfer fits into the transfer budget. */
export function withinTransferBudget(byteLength: number): boolean {
  return byteLength <= MAX_TRANSFER_BYTES;
}

/** Number of bytes a base64 payload occupies on the wire. */
export function base64EncodedLength(byteLength: number): number {
  if (byteLength <= 0) {
    return 0;
  }
  return 4 * Math.ceil(byteLength / 3);
}

/**
 * Number of pixels of an export at a given scale.
 *
 * The result is rounded up so a fractional pixel size never hides an
 * oversized export.
 */
export function exportPixelCount(width: number, height: number, scale: number): number {
  const scaledWidth = Math.ceil(width * scale);
  const scaledHeight = Math.ceil(height * scale);
  if (!Number.isFinite(scaledWidth) || !Number.isFinite(scaledHeight)) {
    return Number.POSITIVE_INFINITY;
  }
  if (scaledWidth < 0 || scaledHeight < 0) {
    return 0;
  }
  return scaledWidth * scaledHeight;
}

/** Whether an export at the given size and scale stays under the pixel cap. */
export function withinExportPixelBudget(width: number, height: number, scale: number): boolean {
  return exportPixelCount(width, height, scale) <= MAX_EXPORT_PIXELS;
}

/** Whether a scale value is inside the accepted range. */
export function withinExportScaleRange(scale: number): boolean {
  return Number.isFinite(scale) && scale >= MIN_EXPORT_SCALE && scale <= MAX_EXPORT_SCALE;
}
