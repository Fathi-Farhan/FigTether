/**
 * UTF-8 encoding without platform APIs.
 *
 * The Figma main sandbox has no `TextEncoder`, so the shared package carries
 * its own encoder. Only the code point range of the Basic Multilingual Plane
 * plus astral planes is supported, which is what the plugin payloads need.
 */

/** Encode a string as UTF-8 bytes. */
export function utf8Encode(text: string): Uint8Array {
  const bytes: number[] = [];
  for (let index = 0; index < text.length; index += 1) {
    let codePoint = text.charCodeAt(index);
    if (codePoint >= 0xd800 && codePoint <= 0xdbff && index + 1 < text.length) {
      const low = text.charCodeAt(index + 1);
      if (low >= 0xdc00 && low <= 0xdfff) {
        codePoint = (codePoint - 0xd800) * 0x400 + (low - 0xdc00) + 0x10000;
        index += 1;
      }
    }
    if (codePoint < 0x80) {
      bytes.push(codePoint);
    } else if (codePoint < 0x800) {
      bytes.push(0xc0 | (codePoint >> 6), 0x80 | (codePoint & 0x3f));
    } else if (codePoint < 0x10000) {
      bytes.push(
        0xe0 | (codePoint >> 12),
        0x80 | ((codePoint >> 6) & 0x3f),
        0x80 | (codePoint & 0x3f),
      );
    } else {
      bytes.push(
        0xf0 | (codePoint >> 18),
        0x80 | ((codePoint >> 12) & 0x3f),
        0x80 | ((codePoint >> 6) & 0x3f),
        0x80 | (codePoint & 0x3f),
      );
    }
  }
  return Uint8Array.from(bytes);
}

/** Decode UTF-8 bytes into a string. */
export function utf8Decode(bytes: Uint8Array): string {
  let out = "";
  let index = 0;
  while (index < bytes.length) {
    const first = bytes[index] ?? 0;
    let codePoint: number;
    let size: number;
    if (first < 0x80) {
      codePoint = first;
      size = 1;
    } else if ((first & 0xe0) === 0xc0) {
      codePoint = first & 0x1f;
      size = 2;
    } else if ((first & 0xf0) === 0xe0) {
      codePoint = first & 0x0f;
      size = 3;
    } else if ((first & 0xf8) === 0xf0) {
      codePoint = first & 0x07;
      size = 4;
    } else {
      throw new Error("utf8Decode: invalid UTF-8 lead byte");
    }
    if (index + size > bytes.length) {
      throw new Error("utf8Decode: truncated UTF-8 sequence");
    }
    for (let offset = 1; offset < size; offset += 1) {
      const next = bytes[index + offset] ?? 0;
      if ((next & 0xc0) !== 0x80) {
        throw new Error("utf8Decode: invalid UTF-8 continuation byte");
      }
      codePoint = (codePoint << 6) | (next & 0x3f);
    }
    index += size;
    if (codePoint > 0xffff) {
      const adjusted = codePoint - 0x10000;
      out += String.fromCharCode(0xd800 + (adjusted >> 10), 0xdc00 + (adjusted & 0x3ff));
    } else {
      out += String.fromCharCode(codePoint);
    }
  }
  return out;
}
