/**
 * Random identifiers.
 *
 * The client identifier of a plugin run is a random value that never leaves
 * the iframe except inside the authentication frame. It is not a credential
 * and it is not persisted.
 */

const HEX = "0123456789abcdef";

function randomBytes(length: number): Uint8Array {
  const bytes = new Uint8Array(length);
  const source = globalThis.crypto;
  if (source && typeof source.getRandomValues === "function") {
    source.getRandomValues(bytes);
    return bytes;
  }
  for (let index = 0; index < length; index += 1) {
    bytes[index] = Math.floor(Math.random() * 256);
  }
  return bytes;
}

/** Random lowercase hexadecimal token of the requested length. */
export function randomHex(length: number): string {
  const bytes = randomBytes(Math.max(1, Math.trunc(length)));
  let out = "";
  for (const byte of bytes) {
    out += HEX[(byte >> 4) & 0x0f];
    out += HEX[byte & 0x0f];
  }
  return out.slice(0, Math.max(1, Math.trunc(length)));
}

/** Random identifier in the shape of a version 4 UUID. */
export function randomUuid(): string {
  const bytes = randomBytes(16);
  bytes[6] = ((bytes[6] as number) & 0x0f) | 0x40;
  bytes[8] = ((bytes[8] as number) & 0x3f) | 0x80;
  const hex: string[] = [];
  for (const byte of bytes) {
    hex.push(byte.toString(16).padStart(2, "0"));
  }
  return [
    hex.slice(0, 4).join(""),
    hex.slice(4, 6).join(""),
    hex.slice(6, 8).join(""),
    hex.slice(8, 10).join(""),
    hex.slice(10, 16).join(""),
  ].join("-");
}
