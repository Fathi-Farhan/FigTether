/**
 * Canonical JSON encoding.
 *
 * Object keys are sorted by their UTF-16 code unit order, which matches the
 * byte order that the Rust side uses for ASCII keys. The output is stable for
 * a given value, so both sides of the bridge can compare hashes of the same
 * payload.
 */
import type { JsonValue } from "./json";

/** Encode a JSON value with sorted object keys. */
export function canonicalStringify(value: JsonValue): string {
  if (value === null) {
    return "null";
  }
  if (typeof value === "boolean") {
    return value ? "true" : "false";
  }
  if (typeof value === "number") {
    if (!Number.isFinite(value)) {
      throw new Error("canonicalStringify: numbers must be finite");
    }
    return JSON.stringify(value);
  }
  if (typeof value === "string") {
    return JSON.stringify(value);
  }
  if (Array.isArray(value)) {
    return `[${value.map((entry) => canonicalStringify(entry)).join(",")}]`;
  }
  const entries = Object.keys(value)
    .sort()
    .map((key) => {
      const child = value[key];
      if (child === undefined) {
        throw new Error(`canonicalStringify: key "${key}" has no value`);
      }
      return `${JSON.stringify(key)}:${canonicalStringify(child)}`;
    });
  return `{${entries.join(",")}}`;
}

/** Parse JSON text into a JSON value. */
export function parseJsonValue(text: string): JsonValue {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch (error) {
    const reason = error instanceof Error ? error.message : String(error);
    throw new Error(`parseJsonValue: invalid JSON (${reason})`);
  }
  return assertJsonValue(parsed);
}

/** Validate that an unknown value is a JSON value. */
export function assertJsonValue(value: unknown): JsonValue {
  if (value === null) {
    return null;
  }
  if (typeof value === "string" || typeof value === "boolean") {
    return value;
  }
  if (typeof value === "number") {
    if (!Number.isFinite(value)) {
      throw new Error("assertJsonValue: numbers must be finite");
    }
    return value;
  }
  if (Array.isArray(value)) {
    return value.map((entry) => assertJsonValue(entry));
  }
  if (typeof value === "object") {
    const source = value as Record<string, unknown>;
    const result: Record<string, JsonValue> = {};
    for (const key of Object.keys(source)) {
      const child = source[key];
      if (child === undefined) {
        continue;
      }
      result[key] = assertJsonValue(child);
    }
    return result;
  }
  throw new Error(`assertJsonValue: unsupported value of type ${typeof value}`);
}
