/**
 * Credential memory and redaction helpers.
 *
 * The pairing secret and the session credential live in closure variables of
 * this module only. They are never written to localStorage, sessionStorage,
 * cookies or a URL, they are never part of a log line, and they are dropped
 * as soon as the handshake consumed them.
 */

/** Placeholder that replaces every credential shaped value. */
export const REDACTED = "[redacted]";

/** In memory holder for one credential. */
export interface SecretStore {
  /** Return the current value, or null when the slot is empty. */
  read(): string | null;
  /** Store a value, replacing any previous one. */
  write(value: string): void;
  /** Drop the value. The reference is released immediately. */
  clear(): void;
  /** True when a value is currently held. */
  has(): boolean;
}

/**
 * Create a secret store whose value is only reachable through the returned
 * closures. The value cannot be observed from the outside, which keeps it out
 * of diagnostics and out of any accidental serialization of the UI state.
 */
export function createSecretStore(): SecretStore {
  let value: string | null = null;
  return {
    read: (): string | null => value,
    write: (next: string): void => {
      value = next.length > 0 ? next : null;
    },
    clear: (): void => {
      value = null;
    },
    has: (): boolean => value !== null,
  };
}

/**
 * Replace every occurrence of the given secrets in a text.
 *
 * Empty values are ignored so that a cleared store cannot blank the whole
 * string.
 */
export function redactKnownSecrets(
  text: string,
  secrets: readonly (string | null | undefined)[],
): string {
  let result = text;
  for (const secret of secrets) {
    if (typeof secret !== "string" || secret.length < 4) {
      continue;
    }
    result = result.split(secret).join(REDACTED);
  }
  return result;
}

const SENSITIVE_KEY_PATTERN =
  /(secret|credential|token|authorization|password|passphrase|apikey|api_key|signature)(["'\s]*[:=]["'\s]*)([^\s"'&,;)}\]]+)/gi;

/**
 * Redact values that follow a credential shaped key in free text.
 *
 * This is a safety net for text that is rendered from outside the UI, such as
 * a daemon supplied error message. It never replaces a key name, only the
 * value that follows it.
 */
export function redactSensitiveAssignments(text: string): string {
  return text.replace(SENSITIVE_KEY_PATTERN, (_match, key: string, separator: string) => {
    return `${key}${separator}${REDACTED}`;
  });
}

/** Redact both known secrets and credential shaped assignments. */
export function redact(text: string, secrets: readonly (string | null | undefined)[] = []): string {
  return redactSensitiveAssignments(redactKnownSecrets(text, secrets));
}

/**
 * True when a value looks like a credential rather than ordinary text.
 *
 * The check is intentionally conservative and only used to decide whether a
 * value must be dropped before it reaches a log line.
 */
export function looksLikeSecret(value: string): boolean {
  if (value.length < 16 || value.length > 512) {
    return false;
  }
  return /^[A-Za-z0-9_\-+/=.]{16,512}$/.test(value);
}
