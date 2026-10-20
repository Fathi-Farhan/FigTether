/**
 * SHA-256 (FIPS 180-4) without any dependency.
 *
 * The Figma main sandbox has no Node.js and no WebCrypto, so the shared
 * package carries a plain TypeScript implementation. It is verified against the
 * published test vectors in `shared/test/sha256.test.ts`.
 */
import { utf8Encode } from "./utf8";

const ROUND_CONSTANTS: readonly number[] = [
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
  0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
  0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
  0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
  0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
  0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

function rotateRight(value: number, bits: number): number {
  return ((value >>> bits) | (value << (32 - bits))) >>> 0;
}

/** Compute the SHA-256 digest of a byte slice. */
export function sha256(input: Uint8Array): Uint8Array {
  const state = new Uint32Array([
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
  ]);

  const blockCount = Math.floor((input.length + 8) / 64) + 1;
  const padded = new Uint8Array(blockCount * 64);
  padded.set(input);
  padded[input.length] = 0x80;

  const bitLengthHigh = Math.floor((input.length / 0x20000000) >>> 0);
  const bitLengthLow = (input.length << 3) >>> 0;
  const tail = padded.length;
  padded[tail - 8] = (bitLengthHigh >>> 24) & 0xff;
  padded[tail - 7] = (bitLengthHigh >>> 16) & 0xff;
  padded[tail - 6] = (bitLengthHigh >>> 8) & 0xff;
  padded[tail - 5] = bitLengthHigh & 0xff;
  padded[tail - 4] = (bitLengthLow >>> 24) & 0xff;
  padded[tail - 3] = (bitLengthLow >>> 16) & 0xff;
  padded[tail - 2] = (bitLengthLow >>> 8) & 0xff;
  padded[tail - 1] = bitLengthLow & 0xff;

  const schedule = new Uint32Array(64);
  for (let offset = 0; offset < padded.length; offset += 64) {
    for (let index = 0; index < 16; index += 1) {
      const base = offset + index * 4;
      schedule[index] =
        (((padded[base] ?? 0) << 24) |
          ((padded[base + 1] ?? 0) << 16) |
          ((padded[base + 2] ?? 0) << 8) |
          (padded[base + 3] ?? 0)) >>>
        0;
    }
    for (let index = 16; index < 64; index += 1) {
      const previous = schedule[index - 15] ?? 0;
      const recent = schedule[index - 2] ?? 0;
      const s0 = rotateRight(previous, 7) ^ rotateRight(previous, 18) ^ (previous >>> 3);
      const s1 = rotateRight(recent, 17) ^ rotateRight(recent, 19) ^ (recent >>> 10);
      schedule[index] =
        (((schedule[index - 16] ?? 0) + s0 + (schedule[index - 7] ?? 0) + s1) >>> 0) >>> 0;
    }

    let a = state[0] ?? 0;
    let b = state[1] ?? 0;
    let c = state[2] ?? 0;
    let d = state[3] ?? 0;
    let e = state[4] ?? 0;
    let f = state[5] ?? 0;
    let g = state[6] ?? 0;
    let h = state[7] ?? 0;

    for (let index = 0; index < 64; index += 1) {
      const s1 = rotateRight(e, 6) ^ rotateRight(e, 11) ^ rotateRight(e, 25);
      const choice = (e & f) ^ (~e & g);
      const temp1 =
        (h + s1 + choice + (ROUND_CONSTANTS[index] ?? 0) + (schedule[index] ?? 0)) >>> 0;
      const s0 = rotateRight(a, 2) ^ rotateRight(a, 13) ^ rotateRight(a, 22);
      const majority = (a & b) ^ (a & c) ^ (b & c);
      const temp2 = (s0 + majority) >>> 0;

      h = g;
      g = f;
      f = e;
      e = (d + temp1) >>> 0;
      d = c;
      c = b;
      b = a;
      a = (temp1 + temp2) >>> 0;
    }

    state[0] = ((state[0] ?? 0) + a) >>> 0;
    state[1] = ((state[1] ?? 0) + b) >>> 0;
    state[2] = ((state[2] ?? 0) + c) >>> 0;
    state[3] = ((state[3] ?? 0) + d) >>> 0;
    state[4] = ((state[4] ?? 0) + e) >>> 0;
    state[5] = ((state[5] ?? 0) + f) >>> 0;
    state[6] = ((state[6] ?? 0) + g) >>> 0;
    state[7] = ((state[7] ?? 0) + h) >>> 0;
  }

  const digest = new Uint8Array(32);
  for (let index = 0; index < 8; index += 1) {
    const word = state[index] ?? 0;
    digest[index * 4] = (word >>> 24) & 0xff;
    digest[index * 4 + 1] = (word >>> 16) & 0xff;
    digest[index * 4 + 2] = (word >>> 8) & 0xff;
    digest[index * 4 + 3] = word & 0xff;
  }
  return digest;
}

const HEX_DIGITS = "0123456789abcdef";

/** Compute the hex encoded SHA-256 digest of a byte slice. */
export function sha256Hex(input: Uint8Array): string {
  const digest = sha256(input);
  let out = "";
  for (const byte of digest) {
    out += HEX_DIGITS.charAt((byte >> 4) & 0x0f);
    out += HEX_DIGITS.charAt(byte & 0x0f);
  }
  return out;
}

/** Compute the hex encoded SHA-256 digest of a UTF-8 string. */
export function sha256HexOfText(text: string): string {
  return sha256Hex(utf8Encode(text));
}
