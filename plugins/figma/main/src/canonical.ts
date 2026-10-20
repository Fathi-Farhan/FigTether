/**
 * Canonical JSON serialization and content hashing.
 *
 * The canonical form and the digest live in the shared package, so the daemon
 * and the plugin compute the same hash for the same payload. This module adds
 * the prefixed hash the snapshot contract uses.
 */

import { canonicalStringify, sha256HexOfText } from "@figtether/shared";
import type { JsonValue } from "@figtether/shared";

/**
 * Serialize a value into canonical JSON.
 *
 * The output is valid JSON with sorted object keys, so two structurally equal
 * values always produce the same string.
 */
export function canonicalJson(value: JsonValue): string {
  return canonicalStringify(value);
}

/**
 * Compute the content hash of a value.
 *
 * The hash is the SHA-256 of the canonical JSON encoding, prefixed with the
 * algorithm name so the daemon can tell which digest produced it.
 */
export function contentHash(value: JsonValue): string {
  return `sha256:${sha256HexOfText(canonicalStringify(value))}`;
}
