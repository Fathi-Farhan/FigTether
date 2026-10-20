# ADR-005: Consistency model with fresh default, explicit cached, and best_effort captures

| Field                | Value      |
| -------------------- | ---------- |
| Status               | Accepted   |
| Created              | 2026-10-20 |
| Related decision IDs | D04, D07   |
| Supersedes           | -          |

## Context

Snapshots go stale silently, and agents then build decisions on stale data (`P03`, risk `R03` and
`R14`). PRD 8.3 fixes the evidence semantics: `consistency=fresh` is the default live mode where
the cache never answers; `consistency=cached` with `max_age_ms` is explicit opt-in carrying
`observed_at` and `cache_age_ms`; an over-age cached request must do a fresh read or fail, never
degrade silently; plugin offline means fresh fails with no fallback; a change detected during a
read labels the result `best_effort` with start and end times and allows at most one retry.
`FR06` requires fresh reads to reach the plugin, and `FR12` makes the freshness contract mandatory
on every result. `D04` decides fresh-first; `D07` decides that watch, even in v0.2, is
invalidation only, never a truth source. Committed interpretation 3 (in
`docs/00-overview/01-prd-analysis.md` section 10) freezes snapshot values and marks every capture
`best_effort`.

## Decision

1. Request parameter `consistency` accepts `fresh` (default) or `cached`. Unknown values fail
   structured per ADR-011; there is no silent reinterpretation.
2. `fresh`: the request must reach the plugin and answer from a live read; the cache can never
   satisfy it (`FR06`). The envelope observation block carries `started_at`, `observed_at`,
   `freshness` (for example `fresh_read`), and `cache_age_ms` when applicable (8.5.2).
3. Observation windows: a read is one window with a start time and an end time. Identical
   concurrent requests (same source, scope, projection, export settings) may coalesce inside one
   window and share one `observation_id`; coalescing is allowed, not required, and the shared
   window is stated by identical timestamps (8.3).
4. `cached`: valid only when explicitly requested with `max_age_ms`; the answer includes
   `observed_at` and `cache_age_ms`; when age exceeds `max_age_ms`, the daemon must perform a
   fresh read or fail; legality of the cache entry is re-checked against session and epoch.
5. `best_effort` labeling: any result where a change was detected during the observation window,
   including every snapshot capture and every mid-edit traversal, labels
   `consistency=best_effort`, carries window timestamps, and may carry a `DOCUMENT_CHANGED`
   warning with at most one automatic retry (`FR14`, 8.3).
6. Snapshots: reads reference the immutable capture; they never claim freshness against the live
   document and never pose as live sessions (`source.kind=plugin_snapshot`, `FR05`, committed
   interpretation 3).
7. Plugin offline: fresh fails with `PLUGIN_OFFLINE`; snapshot data is reachable only through an
   explicit snapshot source (8.3); there is no REST fallback (`FR19`, `AT02`).
8. There is no atomic whole-file read guarantee (8.3 closing paragraph, `R14`); traversals are
   windows, and snapshots are the only stability offer (see ADR-006 and ADR-003 on watch being
   invalidation-only per `D07`).
9. Unsupported is not null: null means genuinely empty only when the field availability statement
   says available; otherwise the result reports an unavailable reason (`FR09`, 8.5.2, error list
   in 8.7).

## Rationale

Fresh-first removes the stale-by-default failure (`P03`) without removing fast repeated reads,
because the explicitly cached tier keeps a measured fast path (`N01`). Coalescing inside one
window is honest: one read observed one editor state once; pretending it was two observations
would invent coverage. Refusing whole-file atomicity is a stated limit rather than a hidden one,
which is what `R14` mitigation asks for: best_effort labels plus bounded retries instead of false
transaction guarantees.

## Consequences

- Positive: freshness is machine-checkable on every payload (`FR12`); agents can branch on
  consistency values instead of guessing age.
- Negative: cached mode needs age bookkeeping and epoch-aware invalidation (ADR-007);
  coalescing couples queue logic to the scheduler.
- Neutral: watch events (v0.2, `D07`) may trigger invalidation and resync suggestions but never
  fill evidence fields of fresh reads.

## Alternatives considered

- Watch events as the truth source for freshness: rejected by `D07` and `R03` (event coverage has
  known gaps, `S6`).
- Always-fresh with no cache tier: rejected for v0.1 because `N01` defines cached reads as a
  supported, measured, explicit path.
- Cache on by default with an opt-out: rejected; it recreates the stale-data failure and breaks
  committed interpretation 1.
- ETag-style whole-document version checks against Figma: rejected; the Plugin API documents no
  stable whole-document version to validate against (`S4`, `S7`).

## Verification

Evidence: freshness contract fixture (edit text or layout, fresh read returns the new value, an
old snapshot stays unchanged, a cached request reports positive `cache_age_ms` and refuses when
too old); soak captures labeling mid-edit windows as best_effort with timestamps.
AT refs: AT04 (fresh, snapshot permanence, cache age), AT10 (normalized context and labeling),
AT05 (no cross-session pollution of observation windows).
Gate: G1 (contract fixtures) and G2 (release evidence with split benchmarks `N01`, `N02`).

## References

- PRD 4.3 (`FR05`, `FR06`, `FR09`, `FR12`, `FR14`), 8.3, 8.5.2, 8.5.5, 8.9 (`R03`, `R14`),
  Appendix A.
- `D04`, `D07`; committed interpretations 1 and 3 in `docs/00-overview/01-prd-analysis.md`
  section 10.
- ADR-006 (pagination inside one window), ADR-007 (cache ages and invalidation), ADR-011
  (`DOCUMENT_CHANGED`, `PLUGIN_OFFLINE`, unavailable reasons).
