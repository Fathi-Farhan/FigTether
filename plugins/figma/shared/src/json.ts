/**
 * JSON value type used by the bridge payloads.
 *
 * The Rust side stores free form payloads as `serde_json::Value`, so the
 * TypeScript mirror uses the same closed set of JSON shapes. `unknown` is used
 * wherever the daemon may send a payload the plugin does not model yet.
 */
export type JsonValue = string | number | boolean | null | JsonValue[] | JsonObject;

/** A JSON object with string keys. */
export interface JsonObject {
  [key: string]: JsonValue;
}

/** Narrow a value to a plain JSON object. */
export function isJsonObject(value: unknown): value is JsonObject {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** Narrow a value to a JSON array. */
export function isJsonArray(value: unknown): value is JsonValue[] {
  return Array.isArray(value);
}
