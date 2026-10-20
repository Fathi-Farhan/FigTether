import { describe, expect, it } from "vitest";

import {
  REDACTED,
  createSecretStore,
  looksLikeSecret,
  redact,
  redactKnownSecrets,
  redactSensitiveAssignments,
} from "../src/secret.js";

describe("createSecretStore", () => {
  it("holds a value only until it is cleared", () => {
    const store = createSecretStore();
    expect(store.has()).toBe(false);
    expect(store.read()).toBeNull();

    store.write("s3cret-value");
    expect(store.has()).toBe(true);
    expect(store.read()).toBe("s3cret-value");

    store.clear();
    expect(store.has()).toBe(false);
    expect(store.read()).toBeNull();
  });

  it("treats an empty value as no value", () => {
    const store = createSecretStore();
    store.write("");
    expect(store.has()).toBe(false);
  });
});

describe("redactKnownSecrets", () => {
  it("removes every occurrence of a known secret", () => {
    const text = "failed with token abcdef123456 and abcdef123456 again";
    const result = redactKnownSecrets(text, ["abcdef123456"]);
    expect(result).not.toContain("abcdef123456");
    expect(result).toBe(`failed with token ${REDACTED} and ${REDACTED} again`);
  });

  it("ignores empty and short values", () => {
    expect(redactKnownSecrets("abc", [""])).toBe("abc");
    expect(redactKnownSecrets("abc", [null])).toBe("abc");
    expect(redactKnownSecrets("abc", [undefined])).toBe("abc");
  });
});

describe("redactSensitiveAssignments", () => {
  it("removes the value of a credential shaped key", () => {
    const result = redactSensitiveAssignments('secret="hunter2" client_id="abc"');
    expect(result).not.toContain("hunter2");
    expect(result).toContain("client_id");
  });

  it("keeps a message without a credential shaped key", () => {
    const message = "The daemon reported BUSY. Retry in 750 ms.";
    expect(redactSensitiveAssignments(message)).toBe(message);
  });
});

describe("redact", () => {
  it("applies both rules", () => {
    const text = 'session_credential="abcdef1234567890"';
    const result = redact(text, ["abcdef1234567890"]);
    expect(result).not.toContain("abcdef1234567890");
  });
});

describe("looksLikeSecret", () => {
  it("accepts a long opaque token", () => {
    expect(looksLikeSecret("A1b2C3d4E5f6G7h8I9j0")).toBe(true);
  });

  it("rejects short or structured values", () => {
    expect(looksLikeSecret("short")).toBe(false);
    expect(looksLikeSecret("has spaces in it at all")).toBe(false);
  });
});
