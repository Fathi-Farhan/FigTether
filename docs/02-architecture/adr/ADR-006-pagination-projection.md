# ADR-006: Cursor pagination and field projection under a response budget

| Field                | Value      |
| -------------------- | ---------- |
| Status               | Accepted   |
| Created              | 2026-10-20 |
| Related decision IDs | D04, D05   |
| Supersedes           | -          |

## Context

Giant JSON floods agent context and freezes tooling (`P04`), while hidden truncation causes agents
to invent missing pieces (`P08`). `FR07` requires resumable tree pagination with projected fields,
explicit depth and bytes limits, and visible truncation. PRD 8.3 binds live cursors to observation,
scope, and epoch: a detected change or an expiry yields `CURSOR_EXPIRED`, and snapshots are the
advised path for stable traversal. The envelope (8.5.2) carries coverage facts: `truncated`,
`next_cursor`, `unavailable_fields`. Budgets from 8.5.5: 64 KiB encoded UTF-8 per MCP response,
node batches of 50 IDs, traversal depth max 32, and at most 10,000 visited nodes per operation.

## Decision

1. Cursor binding: every live cursor encodes at least the `observation_id` (or the snapshot
   binding for snapshot listings), the session, `grant_epoch`, the scope fingerprint (root set),
   the field projection fingerprint, and the traversal position. Cursors are opaque strings to
   agents; there are no reconstructed arithmetic cursors and no secrets inside them.
2. Revalidation on continuation: a continue request re-checks session, `grant_epoch`, and scope
   membership (ADR-004 double enforcement). When the change affects the listing window, when scope
   or epoch changed, or when the cursor expired, the answer is `CURSOR_EXPIRED` with guidance to
   start a new listing or take a snapshot (8.3).
3. Snapshot listings: cursors over an immutable snapshot stay valid for the snapshot idle TTL
   (ADR-007), which is why snapshots are the advised path for stable traversal of large scopes
   (`FR07`).
4. Field projection: `fields` is an allowlist resolved against the frozen normalized model
   (8.5.3). Unprojected fields are never silently loaded; fields the model cannot supply for a
   node appear in `coverage.unavailable_fields` with a reason, not as null (`FR08`, `FR09`,
   ADR-005 item 9).
5. Response budget: 64 KiB encoded UTF-8 per MCP response. Responses are shaped first by
   projection, then by pagination; bulk content moves to resources instead of inline JSON
   (8.5.5). Parameters that cannot fit the budget return `INVALID_LIMIT` naming the offending
   dimension (depth beyond 32, batch beyond 50, or bytes over budget).
6. Truncation is always visible: `coverage.truncated` and `next_cursor` are set whenever a listing
   is partial, and partial results state the visited node count. Out-of-scope ancestors and
   siblings are truly excluded, never faked or leaked (8.4, `AT07`).
7. Node IDs stay strings: `node_id` is never parsed into numbers (8.4, `AT13` numeric extremes).

## Rationale

Binding cursors to observation, scope, and epoch closes the stale-cursor-into-stale-data path
(`P03`, `R05`); position-only cursors keep working silently after the subtree changed, which is
exactly the wrong-tab and stale-read family the PRD bans (`P06`). The hard byte budget enforces
the surface the PRD froze in `N01` and `N02`, and visible truncation is the direct countermeasure
to invented gaps (`P08`).

## Consequences

- Positive: predictable response sizes; resumable walks survive queue pauses and heartbeats;
  snapshot traversal is stable and cheap to continue.
- Negative: agents must handle `CURSOR_EXPIRED` and re-list; projection fingerprints add envelope
  fields.
- Neutral: cursor TTLs are configuration-lowered only (ADR-008) and are visible in diagnostics.

## Alternatives considered

- Offset or index-based cursors: rejected; tree mutations shift indices and produce silently wrong
  pages, the exact hidden-truncation failure `P08` punishes.
- Numeric ascending ID cursors: rejected; `node_id` is documented as a string token with no
  ordering guarantee (8.4; `AT13` numeric extremes fuzz).
- One large response that clients slice themselves: rejected; it violates the 64 KiB tool surface
  contract (8.5.5) and presses editor memory through whole-subtree allocations (8.5.3 closing
  note).
- Depth-unlimited traversal with automatic pagination: rejected; the explicit depth cap of 32 and
  the visited-node cap protect editor responsiveness (`N06`).

## Verification

Evidence: large fixture listing that crosses both depth and byte limits shows correct partial
results with no ancestor or sibling leakage; a continued walk after a scope change or epoch bump
returns `CURSOR_EXPIRED`; a projection fingerprint mismatch is rejected; snapshot walks continue
across TTL windows.
AT refs: AT07 (bytes and depth limits, partial plus cursor, no leak), AT13 (fuzz: recursion,
numeric extremes, invalid IDs), AT10 (projection and unavailable fields).
Gate: G1 (fixtures and fuzz), G2 (release evidence including `N01` and `N02` measurements).

## References

- PRD 4.3 (`FR07`, `FR08`, `FR09`, `FR12`), 8.3, 8.4, 8.5.2, 8.5.3, 8.5.5, Appendix A.
- `D04`, `D05`; problem table rows `P04`, `P08` in `docs/00-overview/01-prd-analysis.md`.
- ADR-004 (epoch and scope checks), ADR-005 (observation windows), ADR-007 (snapshot TTL),
  ADR-008 (budget constants), ADR-011 (`CURSOR_EXPIRED`, `INVALID_LIMIT`, `SCOPE_TOO_LARGE`).
