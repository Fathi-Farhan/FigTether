# ADR-008: Resource budgets adopted from PRD 8.5.5 as code constants

| Field                | Value      |
| -------------------- | ---------- |
| Status               | Accepted   |
| Created              | 2026-10-20 |
| Related decision IDs | D01, D12   |
| Supersedes           | -          |

## Context

The PRD freezes an initial profile of resource budgets in 8.5.5 with an explicit escalation rule:
limits can be lowered by configuration; a value raised above the tested profile is labeled
`custom`/`unvalidated` in metrics and diagnostics; the cache budget is not an RSS guarantee
because parsing, allocator, and transport buffers add overhead (`N04`). `N05` demands no crash
and no OOM under mixed load. The frozen numeric reference is
`docs/04-data-contracts/06-resource-limits.md`; this ADR records the adoption decision and the
governance rules, not new numbers. Scheduler priority is also fixed there: revoke, cancel, and
heartbeat first, then small reads, then export, with aging against starvation, and a request that
exhausts its deadline while queued is not executed at all.

## Decision

Adopt the PRD 8.5.5 defaults verbatim as Rust constants in one core module
(`crates/core/resource_limits`), mirrored in the frozen contract doc:

| Budget                  | Default                                   | Over limit                                |
| ----------------------- | ----------------------------------------- | ----------------------------------------- |
| Plugin sessions         | 4 per daemon                              | `SESSION_LIMIT`                           |
| Active editor ops       | 1 per session; 1 export globally          | fair queue; no parallel export            |
| Queue                   | 64 requests total; 16 per client          | `BUSY` plus `retry_after_ms`              |
| Node batch              | 50 IDs; depth max 32                      | `INVALID_LIMIT` or batch split            |
| Traversal               | max 10,000 visited nodes per op           | partial plus cursor, or `SCOPE_TOO_LARGE` |
| MCP response            | 64 KiB encoded UTF-8                      | projection and pagination (ADR-006)       |
| MCP inline image        | 1 MiB raw; max 2 MiB encoded per response | downscale only on request, or resource    |
| Bridge frame / transfer | 1 MiB per frame; 32 MiB per transfer      | `PAYLOAD_TOO_LARGE` plus cleanup          |
| Design plus asset cache | 128 MiB total LRU; single asset 16 MiB    | eviction (ADR-007)                        |
| Export image            | max 16 megapixels; scale 0.25 to 2        | `INVALID_LIMIT`                           |
| Request timeout         | read 20 s; export 60 s; queue time counts | `TIMEOUT`, cancel, late result discarded  |
| Traversal slice         | target 8 ms then yield                    | adaptive smaller batch, progress shown    |
| Fresh read retry        | max 1; transient and idempotent only      | structured error (ADR-011)                |

Rules:

1. The constant module owns the numbers; every other crate reads from it. Duplicated literals in
   other crates are a review blocker to prevent drift.
2. Configuration may only lower: a lowered value needs no new evidence; any value above the
   default immediately labels the running profile `custom`/`unvalidated` in diagnostics and
   doctor output (8.5.5 closing note, `FR18`).
3. Over-limit behavior always uses the structured taxonomy (ADR-011) or marked partial coverage
   (ADR-006); silent truncation with unmarked side effects is forbidden (`FR12`).
4. Scheduler policy follows the priority classes above with aging; queue deadlines are checked
   before any editor operation starts (`FR13`, `FR14`).
5. Monitoring covers the 8.7 metrics row (requests per operation and source, cache hits, queue
   wait and depth, p95 latency, bytes, cancel and revoke counts) plus RSS per `N04` and error
   rates per `N05` at 1 second sampling.

## Rationale

Constants in one module make the tested profile reviewable and benchmarkable as a unit, and
publishable under `D12` (performance beyond the profile is only claimed through reproducible
benchmarks). The lower-only rule keeps support interactions answerable: every shipped behavior
matches the tested envelope, and raised profiles are self-declared as unvalidated, which prevents
silent claim drift (`P14`, `AT16`).

## Consequences

- Positive: budget behavior is fuzzable and soak-testable as one table; diagnostics print profile
  identity (`FR18`); timeout and retry policies stay consistent with `FR14`.
- Negative: hardware that genuinely needs more must run under an explicitly labeled profile; some
  legitimate configurations will refuse work by design.
- Neutral: revising defaults is a contract change: PRD or contract doc and code change in the same
  PR, per the README change management rule.

## Alternatives considered

- Runtime discovery of machine capacity with self-tuning budgets: rejected for v0.1; results are
  non-reproducible and untunable against `N01` through `N05`.
- Soft limits that warn instead of reject: rejected; backpressure must be enforced for `P05` and
  `FR13`, and `AT08` expects denial behavior under overload.
- Per-client budgets at v0.1: rejected; per-client isolation is `FR22` (P1, v0.2); the global and
  per-session table ships first, with per-client 16-request queue bound already present.

## Verification

Evidence: unit tests deriving behavior from the constant module with no duplicated values across
crates; `AT08` overload, cancel, and slow request behaviors; memory profiling per `N04` including
documented allocator overhead; `N05` soak with error rate below 0.1 percent; benchmark logs label
the profile as default or custom/unvalidated.
AT refs: AT08, AT13, AT11 (export limits), AT04 (cache budget interplay); benchmarks `N01`,
`N02`, `N03`, `N04`, `N05`.
Gate: G1 (constant module plus tests), G2 (benchmarks and soak evidence at exact commit).

## References

- PRD 4.3 (`FR13`, `FR14`, `FR18`, `FR22`), 2.1 (`N01..N05`), 8.5.5, 8.7 (metrics and doctor),
  Appendix A, Appendix C (evidence template).
- `D01`, `D12`; committed interpretation 5 (bounded everything) in
  `docs/00-overview/01-prd-analysis.md` section 10.
- `docs/04-data-contracts/06-resource-limits.md` (frozen numbers), ADR-006, ADR-007, ADR-011.
