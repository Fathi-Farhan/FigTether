/**
 * Base64 encoding for byte payloads.
 *
 * Mirror of the `base64_bytes` module in `crates/protocol/src/lib.rs`: the
 * standard alphabet with padding. The implementation is dependency free so it
 * runs in the Figma main sandbox, which has neither `btoa` nor `atob`.
 */
import { utf8Decode, utf8Encode } from "./utf8";

const ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

const LOOKUP: ReadonlyMap<string, number> = new Map(
  ALPHABET.split("").map((character, index) => [character, index] as const),
);

const VALID_INPUT = /^[A-Za-z0-9+/]*={0,2}$/;

/** Encode bytes as a padded base64 string. */
export function base64Encode(bytes: Uint8Array): string {
  let out = "";
  let index = 0;
  for (; index + 2 < bytes.length; index += 3) {
    const first = bytes[index] ?? 0;
    const second = bytes[index + 1] ?? 0;
    const third = bytes[index + 2] ?? 0;
    const triplet = (first << 16) | (second << 8) | third;
    out += ALPHABET.charAt((triplet >> 18) & 0x3f);
    out += ALPHABET.charAt((triplet >> 12) & 0x3f);
    out += ALPHABET.charAt((triplet >> 6) & 0x3f);
    out += ALPHABET.charAt(triplet & 0x3f);
  }
  const remaining = bytes.length - index;
  if (remaining === 1) {
    const triplet = (bytes[index] ?? 0) << 16;
    out += ALPHABET.charAt((triplet >> 18) & 0x3f);
    out += ALPHABET.charAt((triplet >> 12) & 0x3f);
    out += "==";
  } else if (remaining === 2) {
    const triplet = ((bytes[index] ?? 0) << 16) | ((bytes[index + 1] ?? 0) << 8);
    out += ALPHABET.charAt((triplet >> 18) & 0x3f);
    out += ALPHABET.charAt((triplet >> 12) & 0x3f);
    out += ALPHABET.charAt((triplet >> 6) & 0x3f);
    out += "=";
  }
  return out;
}

/** Decode a padded base64 string into bytes. */
export function base64Decode(text: string): Uint8Array {
  if (text.length % 4 !== 0) {
    throw new Error("base64Decode: input length must be a multiple of 4");
  }
  if (!VALID_INPUT.test(text)) {
    throw new Error("base64Decode: input is not a valid base64 string");
  }
  const padding = text.endsWith("==") ? 2 : text.endsWith("=") ? 1 : 0;
  const bytes = new Uint8Array((text.length / 4) * 3 - padding);
  let byteIndex = 0;
  for (let index = 0; index < text.length; index += 4) {
    const chunk = text.slice(index, index + 4);
    let accumulator = 0;
    let paddingInChunk = 0;
    for (let position = 0; position < 4; position += 1) {
      const character = chunk.charAt(position);
      if (character === "=") {
        paddingInChunk += 1;
        accumulator = accumulator << 6;
        continue;
      }
      const value = LOOKUP.get(character);
      if (value === undefined) {
        throw new Error(`base64Decode: invalid base64 character "${character}"`);
      }
      accumulator = (accumulator << 6) | value;
    }
    const group = [(accumulator >> 16) & 0xff, (accumulator >> 8) & 0xff, accumulator & 0xff];
    for (let position = 0; position < 3 - paddingInChunk; position += 1) {
      bytes[byteIndex] = group[position] ?? 0;
      byteIndex += 1;
    }
  }
  return bytes;
}

/** Encode a string as base64 using its UTF-8 representation. */
export function base64EncodeText(text: string): string {
  return base64Encode(utf8Encode(text));
}

/** Decode a base64 string into text using its UTF-8 representation. */
export function base64DecodeText(text: string): string {
  return utf8Decode(base64Decode(text));
}
