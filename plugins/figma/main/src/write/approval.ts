/**
 * The approval token of the write path.
 *
 * The token is the proof that the user said yes to one plan. It is
 * deterministic and it is not a secret: the daemon and the plugin rebuild it
 * from the same inputs, so the plugin can read the approval id and the hash
 * of the plan out of it and compare them with the plan it received.
 *
 * The format is `w:<approval_id>:<plan_hash>:<session_id>:<grant_epoch>:<issued_at>`,
 * the same format the Rust side builds in `crates/protocol/src/write.rs`. The
 * hash travels without its `sha256:` prefix, because the prefix holds a colon
 * and the fields are separated by colons.
 */

/** The parts an approval token is built from. */
export interface ApprovalToken {
  /** Identity of the approval request. */
  approval_id: string;
  /** Hash of the approved plan, with its `sha256:` prefix. */
  plan_hash: string;
  /** Session the approval belongs to. */
  session_id: string;
  /** Grant epoch the approval is bound to. */
  grant_epoch: number;
  /** Moment the approval was issued, RFC 3339. */
  issued_at: string;
}

/** Prefix of every approval token. */
const TOKEN_PREFIX = "w:";

/**
 * Parse an approval token.
 *
 * A token that does not have the documented shape is refused, so a caller
 * never works with half a token.
 */
export function parseApprovalToken(token: string): ApprovalToken | null {
  if (!token.startsWith(TOKEN_PREFIX)) {
    return null;
  }
  const fields = token.slice(TOKEN_PREFIX.length).split(":");
  if (fields.length < 5) {
    return null;
  }
  const approvalId = fields[0] ?? "";
  const hash = fields[1] ?? "";
  const sessionId = fields[2] ?? "";
  const epoch = fields[3] ?? "";
  const issuedAt = fields.slice(4).join(":");
  if (
    approvalId.length === 0 ||
    hash.length === 0 ||
    sessionId.length === 0 ||
    epoch.length === 0 ||
    issuedAt.length === 0
  ) {
    return null;
  }
  const grantEpoch = Number.parseInt(epoch, 10);
  if (!Number.isInteger(grantEpoch) || grantEpoch < 0 || String(grantEpoch) !== epoch) {
    return null;
  }
  return {
    approval_id: approvalId,
    plan_hash: withHashPrefix(hash),
    session_id: sessionId,
    grant_epoch: grantEpoch,
    issued_at: issuedAt,
  };
}

/**
 * The hash as the rest of the code compares it.
 *
 * A hash that carries no prefix gets the prefix of the canonical form, so
 * the two forms compare equal.
 */
export function withHashPrefix(hash: string): string {
  return hash.startsWith("sha256:") ? hash : `sha256:${hash}`;
}

/**
 * Whether two hashes name the same plan.
 *
 * The hash of a token travels without its prefix, so the comparison puts the
 * prefix on both sides before it compares them.
 */
export function sameHash(left: string, right: string): boolean {
  return withHashPrefix(left) === withHashPrefix(right);
}
