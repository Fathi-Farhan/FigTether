/**
 * Byte level helpers of the main sandbox.
 *
 * The encoding primitives live in the shared package, because the UI iframe
 * and the daemon have to agree on them. This module adds the two helpers the
 * sandbox needs and the shared package does not ship: the byte length of a
 * string and the URL safe base64 used by the pagination cursors.
 */

import {
  base64Decode,
  base64Encode,
  base64DecodeText,
  base64EncodeText,
  utf8Decode,
  utf8Encode,
} from "@figtether/shared";

export { base64Decode, base64Encode, base64DecodeText, base64EncodeText, utf8Decode, utf8Encode };

/** Number of bytes a string occupies when encoded as UTF-8. */
export function utf8ByteLength(text: string): number {
  let length = 0;
  for (const character of text) {
    const codePoint = character.codePointAt(0);
    if (codePoint === undefined) {
      continue;
    }
    if (codePoint < 0x80) {
      length += 1;
    } else if (codePoint < 0x800) {
      length += 2;
    } else if (codePoint < 0x10000) {
      length += 3;
    } else {
      length += 4;
    }
  }
  return length;
}

/** Encode bytes as URL safe base64 without padding. */
export function base64UrlEncode(bytes: Uint8Array): string {
  return base64Encode(bytes).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

/** Decode URL safe base64 without padding. */
export function base64UrlDecode(text: string): Uint8Array {
  const normalized = text.replace(/-/g, "+").replace(/_/g, "/");
  const padded = normalized.padEnd(Math.ceil(normalized.length / 4) * 4, "=");
  return base64Decode(padded);
}
