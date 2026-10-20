# ADR-004: Grant scope model with double enforcement

| Field                | Value              |
| -------------------- | ------------------ |
| Status               | Accepted           |
| Created              | 2026-10-20         |
| Related decision IDs | D05, D06           |
| Supersedes           | -                  |
| Amended by           | ADR-013 (Proposed) |

## Context

Users decide what an agent may read, and a wrong tab or a stale scope is a real, observed failure
(`P06`, `P10`, risk `R05`). PRD 8.4 defines the entities: `session_id` per plugin run or tab,
`grant_id` and `grant_epoch` as the authorization version that rises on scope change or revoke,
and scope roots as "union subtree, dedup descendants". It also fixes the behavior rules: moving
page or selection never changes a grant; nodes that leave the subtree stop resolving;
ancestor and sibling metadata outside scope is not included; style and component references
outside scope surface as unresolved IDs, with content resolution gated on an approved dependency
or an additional grant. `FR03` demands double enforcement: Rust and plugin check session, epoch,
and root membership on every request. `FR04` limits discovery to granted sessions only. Committed
interpretation 4 (in `docs/00-overview/01-prd-analysis.md` section 10) makes scope monotonic
within a grant until the epoch changes.

## Decision

1. Scope set semantics: for each granted root, the root node and its full subtree are members.
   Overlapping roots produce one union; a descendant shared by several roots is the same single
   member and appears once in listings ("union subtree, dedup descendants", PRD 8.4).
2. Every read carries `session_id` or `snapshot_id`, mutually exclusive per 8.5.1, and resolves
   the grant bound to that source: session plus `grant_epoch` for live reads, the originating
   binding for snapshots. Cursors and asset resource URIs carry the same session and
   `grant_epoch` binding (8.6.1).
3. Double enforcement on every request (`FR03`): the plugin re-verifies session, `grant_epoch`,
   and membership for every allowlisted handler call in the main sandbox, and the daemon
   re-verifies the same triple before scheduling and again before answering. Either side failing
   denies; uncertainty denies (fail-closed, 8.6.1).
4. Fail-closed error mapping: node outside scope or stale epoch answers `OUT_OF_SCOPE`; missing
   platform permission or grant answers `PERMISSION_DENIED`; revoked answers `REVOKED` (code list
   in PRD 8.7, ADR-011).
5. Selection never widens scope: selection, page, zoom, and layer names do not change membership;
   out-of-subtree nodes stop resolving; scope stays monotonic until the epoch changes. Presenting
   selection summaries (`get_selection`) only reports the intersection with scope (8.5.1).
6. Grant lifecycle is user-driven: create, expand, pause, and revoke happen in plugin UI or CLI
   (8.5.1); an agent cannot escalate through read tools. Any scope or revoke change increments
   `grant_epoch`, invalidates dependent cursors and asset URIs, pauses responses for stale work,
   and requires regrant for further access (`FR15`).

## Rationale

Deduplication keeps pagination counts and `truncated` coverage honest when roots overlap;
otherwise `FR07` reports would invent phantom nodes. Binding epochs everywhere removes "the
answer was right two minutes ago" routing (`P06`, `N07`). Double enforcement defends in depth: a
bridge input bug or plugin confusion cannot become a scope leak because the second side
independently refuses (`R05`, `AT06`). Making the epoch check the single mechanism for revoke,
reconnect, and change gives one observable, testable rule instead of several.

## Consequences

- Positive: one membership algorithm runs on both sides; grants and their effects are visible in
  diagnostics correlation data (`FR18`); regrant gives users a legible path after any change.
- Negative: epoch invalidations can force re-listing mid-walk; the plugin re-verification adds a
  membership check inside hot traversal paths.
- Neutral: scope is a set, not a tree; presentations may nest the answer, but membership tests
  are always set-based against the union.

## Alternatives considered

- Enforcement only in the plugin: rejected; a plugin bug becomes a direct leak, contradicting the
  8.6.1 mitigation that membership verification exists on both sides.
- Enforcement only in the daemon: rejected; the plugin is the editor-side trusted surface and must
  refuse out-of-scope handler calls it receives, whatever the daemon thinks.
- Scope anchored to the current selection at request time: rejected; violates committed
  interpretation 4 and the PRD 8.4 rule that selection never widens a grant (`P06`).
- Per-node allowlist maintained in the plugin: rejected; it drifts from daemon truth and explodes
  combinatorially for large scopes.

## Verification

Evidence: scope isolation suite with two sessions sharing node IDs under disjoint grants (no
crosstalk, no shared cursors or assets); epoch bump mid-pagination tests; dependency grant tests
for style and component resolution; revocation mid-transfer tests.
AT refs: AT05 (two sessions, same IDs, no data exchange), AT06 (wrong token, replay, out-of-scope
node, revoke during transfer), AT17 (scope moved, deleted, or renamed during read), AT10
(unresolved references surfaced).
Gate: G0 (E01 grant and epoch baseline probes), G1 (AT05, AT06, AT17), G2 (release regression on
exact commit).

## Amendment: write grant mode

`ADR-013` is the separate RFC the PRD requires before write-back may exist. It proposes to give
the grant a mode (`read` or `write`). This ADR is not rewritten by it:

- The scope semantics, the double enforcement, and the epoch rules of this ADR are unchanged. A
  write is checked with the same session, epoch, and membership triple as a read, on both sides.
- The write grant is a separate user action in the plugin UI, and it can be narrowed to fewer
  roots than the read grant. It never widens the read scope of a session.
- A mode change increments `grant_epoch`, so every outstanding approval, cursor, and asset bound
  to the previous epoch stops resolving. A grant change invalidates outstanding write approvals.
- No tool parameter can widen a grant in either mode.

The write mode is specified in `docs/11-write-path/01-write-path-design.md` section 4 and in
`docs/05-security/02-access-control.md` section 3.1. `ADR-013` is pending product approval, so
this amendment records a proposed extension and not a shipped behavior.

## References

- PRD 4.3 (`FR03`, `FR04`, `FR10`, `FR15`, `FR17`), 8.4 (grant entities and closing rules),
  8.5.1, 8.6.1, Appendix A.
- `D05`, `D06`; committed interpretations 2 and 4 in `docs/00-overview/01-prd-analysis.md`
  section 10.
- ADR-010 (session identity), ADR-006 (cursor epoch binding), ADR-007 (invalidation on grant
  change), ADR-011 (structured denial codes).
- ADR-013 (bidirectional write path, Proposed) and `docs/11-write-path/01-write-path-design.md`
  for the write grant mode.
