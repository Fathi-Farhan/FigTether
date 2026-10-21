# Data Contracts: Resource Limits and Backpressure

> **Design reference.** Original requirements and approval labels below are historical design
> records, not a feature-availability list. See [current documentation status](../documentation-status.md)
> for implemented reads/writes, observed desktop evidence, and remaining gaps.

Status: `Proposed`

Purpose: this document fixes the default resource limits from PRD 8.5.5, the on-exceed behavior
and code for every limit, the metrics to watch for each, the configuration policy (lowering only),
the cache-versus-RSS caveat, and the scheduler priority rules that keep the editor responsive
while large requests age fairly. Gate G0 has not run and these numbers are unmeasured, so every
value is a proposed default pending benchmark evidence (PRD 2.1, N01 to N10).

## 1. Status of these numbers

- Every limit below reproduces PRD 8.5.5 verbatim. None has been measured on the reference
  profile (4 logical cores, 8 GB RAM, release build, recorded Figma Desktop version).
- Gates G1 and G2 must deliver the measured tables (N01 to N06); if an NFR is missed, the profile
  and claims are revised from results, never silently (PRD 2.3, R12).
- Until then, this document is the single source for what "bounded" means in the commitment list
  (`docs/00-overview/01-prd-analysis.md` section 10, item 5).

## 2. Defaults table (PRD 8.5.5)

Codes are the literal strings from `docs/04-data-contracts/05-error-catalog.md`. "On exceed"
describes what the system does the moment the limit trips, before any retry logic.

| Limit                   | Default (PRD 8.5.5)                                | On exceed: behavior and code                                                                          | Metrics to watch                                             |
| ----------------------- | -------------------------------------------------- | ----------------------------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| Plugin session          | 4 per daemon; explicit grant per session           | `SESSION_LIMIT`; the caller picks or closes a session, no eviction by the agent                       | `sessions_active`, `session_limit_rejections`                |
| Active editor operation | 1 per session; 1 global asset export               | Later work queues fairly; never parallel exports against the editor                                   | `ops_active`, `ops_queue_wait_ms p95`                        |
| Request queue           | 64 requests total, 16 per client                   | New enqueue rejected with `BUSY` plus `retry_after_ms` (FR13)                                         | `queue_depth`, `busy_rejections`, `queue_wait_ms p95`        |
| Node batch              | 50 IDs; maximum depth 32                           | `INVALID_LIMIT`, or the daemon splits the batch with a notice                                         | `batch_size p95`, `max_depth_used`                           |
| Traversal               | Maximum 10,000 nodes visited per operation         | Partial result with cursor when resumable; otherwise `SCOPE_TOO_LARGE` (FR07)                         | `nodes_visited`, `partial_rate`, `scope_too_large_count`     |
| MCP response JSON       | 64 KiB encoded UTF-8                               | Projection and pagination; respond with metadata and resource references instead of payloads (P04)    | `response_bytes p95`, `projection_use`                       |
| Inline image in MCP     | 1 MiB raw; maximum encoded 2 MiB per response      | Lower scale only when requested; otherwise serve as a resource or chunk                               | `image_bytes_raw`, `image_bytes_encoded`                     |
| Bridge frame / transfer | 1 MiB per frame; 32 MiB per transfer               | `PAYLOAD_TOO_LARGE` plus cleanup of the abandoned transfer (PRD 8.5.4)                                | `frame_bytes`, `transfer_bytes`, `transfer_incomplete_count` |
| Design + asset cache    | 128 MiB total; LRU; single asset 16 MiB            | Eviction of the least recent item; an item still above the asset cap is rejected with `INVALID_LIMIT` | `cache_bytes`, `evictions`, `cache_hit_rate`                 |
| Export image            | Maximum 16 megapixels; scale 0.25 to 2             | `INVALID_LIMIT`; the user may pick a smaller frame instead                                            | `export_megapixels`, `export_scale_histogram`                |
| Request timeout         | 20 s read; 60 s export; queue wait counted in both | `TIMEOUT`, cancel, and the late result is discarded (FR14)                                            | `deadline_missed`, `timeout_cancelled`                       |
| Traversal slice         | Target 8 ms, then yield when the runtime allows    | Adaptive smaller batches and visible progress (N06)                                                   | `slice_ms p95`, `yields_per_op`                              |
| Fresh read retry        | Maximum 1; transient and idempotent only           | Structured error; no unbounded retries                                                                | `retry_count`, `retry_success_rate`                          |

Notes that belong to specific rows:

- Queue fairness: the 16-per-client sub-limit keeps one client from starving the shared 64 (FR22
  direction), and per-client accounting is required from P0 even while multi-client scheduling is
  full P1.
- Export timeout counts queue wait (PRD 8.5.5), so a 60 s export that waited 55 s has 5 s of
  editor time left; the scheduler borrows the deadline for the whole job including enqueue.
- The 1 MiB frame limit bounds every bridge message including `hello/auth` and `chunk` frames;
  nothing in the envelope is exempt (PRD 8.5.4).

## 2.1 Worked admission walkthrough

One `get_nodes` call with 120 node IDs against a large frame shows the order in which limits bite
(the exact tool inputs appear in `docs/04-data-contracts/01-mcp-envelope.md`):

1. **Input validation in the MCP adapter.** 120 IDs exceeds the batch limit of 50 and the request
   asks depth 40. The adapter rejects before enqueue with `INVALID_LIMIT` and `action:
narrow_parameters`, and suggests splitting into 3 batches of at most 50. No queue slot is ever
   consumed by a request that was invalid on arrival.
2. **Queue admission.** The corrected batches enqueue up to the 64-total and 16-per-client caps. A
   batch arriving while the queue is full gets `BUSY` with `retry_after_ms` and keeps its
   deadline; the caller retries without tight-looping (FR13).
3. **Scheduler admission.** The session already runs one active editor operation, so the batch
   waits in the fair lane. If its deadline expires while waiting it is never executed and the
   caller receives `TIMEOUT` (PRD 8.5.5).
4. **Traversal admission in the plugin.** The handler estimates subtree size first. At 12,000
   reachable nodes it stops at 10,000 visited and returns `truncated = true` with a cursor bound
   to the observation, epoch, and scope (FR07, PRD 8.3).
5. **Response shaping.** The first page is projected to under 64 KiB; deeper node payload becomes
   a staged resource reference, never an inline block.
6. **Transfer stage.** The pre-flight estimate of the full subtree export would exceed the 32 MiB
   transfer or editor memory comfort, so the plugin refuses with `SCOPE_TOO_LARGE` instead of
   starting an export it cannot finish cleanly (PRD 8.5.3, 8.5.4).
7. **Retry stage.** If a change is detected mid-traversal, the walk returns `best_effort` with a
   `DOCUMENT_CHANGED` warning, and the allowed single fresh retry is charged once, not per page
   (FR14).

## 2.2 Throughput targets that depend on these limits

The benchmark targets in PRD 2.1 are reachable only if the paired limits hold together:

| Target                                      | Governing limits                                 | Failure mode unguarded                     |
| ------------------------------------------- | ------------------------------------------------ | ------------------------------------------ |
| N01 cached `p95 <= 100 ms` at `20 req/s`    | Queue 64/16, JSON 64 KiB, cache 128 MiB          | Cache churn evicting hot nodes             |
| N02 fresh `p95 <= 2 s` for `100` nodes      | Batch 50, active op 1 per session, timeout 20 s  | Queue wait eating the whole deadline       |
| N03 export `p95 <= 5 s` for 1024x1024 PNG   | Global export slot 1, export 16 MP, timeout 60 s | Large exports starving small ones          |
| N04 RSS `<= 50 MiB` idle, `<= 256 MiB` peak | Cache 128 MiB, transfer 32 MiB, asset 16 MiB     | Reassembly buffers plus allocator overhead |
| N06 no editor freeze `> 1 s`                | Slice 8 ms, traversal 10,000, active op 1        | Slices never yielding in a soft runtime    |

Numbers in this table are relations between proposed values, not claims (PRD 2.1: all targets are
unmeasured engineering goals).

## 3. Configuration policy

1. **Lowering only.** Operators may lower any limit through the daemon configuration. Values above
   the defaults from section 2 are technically accepted by the config format but are never part of
   the tested profile.
2. **Sanity minimums.** Lowering has floors so the system stays workable (proposed): at least 8
   total queue slots, at least 1 active op per session, batch at least 10 IDs, depth at least 4,
   JSON at least 4 KiB, timeouts at least 2 s. Zero or absent cache budgets mean "disabled cache"
   only where stated; a disabled cache is surfaced in capability output, not hidden.
3. **Custom label.** When any configured value exceeds the shipped default, the daemon reports
   `profile_status: custom_unvalidated` in `get_bridge_status` and in diagnostics, and release
   evidence must note the overridden limits (PRD 8.5.5: raising above the tested profile is
   marked custom/unvalidated). Published claims never rest on a custom profile.
4. **Fail-loud startup.** Invalid configuration (below floors, wrong types, unknown keys) refuses
   to start with a clear message naming the key; it does not silently clamp (PRD 8.5.5 spirit,
   doctor obligations in 8.7).
5. **Per-client caps.** Multi-client fairness belongs to P1 (FR22), but the 16-per-client queue
   cap and per-client cache accounting exist from P0 so a P1 rollout never widens limits.

## 4. Cache budget is not an RSS guarantee

The 128 MiB cache budget bounds cached design data and assets, not process memory. Parsing
buffers, allocator overhead, transport reassembly buffers, and transient payload copies all sit
outside the budget (PRD 8.5.5 explicitly: total core RSS stays monitored; the cache limit is not
the same as an RSS guarantee because of parsing, allocator, and transport buffers). Consequences:

- N04 targets idle RSS at or under 50 MiB and mixed-load peak RSS at or under 256 MiB; the
  overhead of the allocator must be documented per measurement (PRD 2.1, N04).
- The 32 MiB bridge transfer limit implies a reassembly buffer of up to 32 MiB per live transfer,
  in addition to the cache; the scheduler counts it in the memory admission sketch.
- Monitoring: `rss_bytes`, `cache_bytes`, `reassembly_bytes` are recorded separately in
  diagnostics (FR18), and benchmark reports must show all three (PRD 2.1 reporting rules).
- Eviction is the only remedy for cache pressure. Reaching RSS pressure triggers nothing hidden:
  either the operator lowers limits, or the measured profile says the budget is smaller.

## 5. Scheduler priorities and starvation control

From PRD 8.5.5, in priority order:

1. **Control traffic first.** `revoke`, `cancel`, and `heartbeat` handling beats any data
   operation, including exports. N07 requires logical cancel/revoke effect within about 1 s, so
   these paths cannot queue behind a running export.
2. **Small reads next.** Short projected reads (the N01/N02 workloads) run ahead of bulk jobs.
3. **Exports and traversals last.** They yield at the 8 ms slice target and show progress.

Starvation rules: big jobs age; aging prevents starvation of large reads and exports even while
small reads keep arriving. Deadline-expired requests are not executed: if a queued job's deadline
already expired while waiting, the scheduler discards it and reports `TIMEOUT` (PRD 8.5.5). One
active editor operation per session and one global export hold regardless of priority; the queue
between plugin and scheduler is the single bounded lane for editor work (PRD 8.5.5 row "Active
editor operation").

Aging mechanics (proposed implementation, behavior fixed here): each queued job carries a wait
credit that accrues for as long as it is not granted. When the credit of a big job reaches the
credit of a small read arriving now, the big job wins the next grant. Control traffic never takes
credits; it preempts. The walking pattern from the slice rule (8 ms then yield) applies inside the
plugin handler, while aging applies inside the daemon; both acknowledge that the runtime may not
guarantee timer precision (see `docs/00-overview/01-prd-analysis.md` section 9, item 5).

## 6. Enforcement points

| Stage            | Enforced by                    | Notes                                                                                                                  |
| ---------------- | ------------------------------ | ---------------------------------------------------------------------------------------------------------------------- |
| MCP tool input   | `crates/mcp`                   | Batch size, depth, scale, and projection validated before enqueue; rejects with `INVALID_LIMIT`                        |
| Scheduler        | `crates/core`                  | Queue totals, per-client caps, session count, deadlines, priority, aging                                               |
| Plugin handler   | `plugins/figma/main`           | Scope estimate before traversal (SCOPE_TOO_LARGE), export bounds, node/depth caps re-checked (FR03 double enforcement) |
| Bridge transport | daemon + `plugins/figma/ui`    | 1 MiB frames, 32 MiB transfers, incomplete transfer cleanup (PRD 8.5.4)                                                |
| Cache and assets | `crates/core`, `crates/assets` | LRU admission, 16 MiB asset cap, `cache_bytes` accounting                                                              |
| Response shaping | `crates/core`                  | 64 KiB JSON, 1/2 MiB image rules, resource references on overflow                                                      |

## 7. Interaction with the error catalog

Because the tables above reference error codes, `docs/04-data-contracts/05-error-catalog.md`
carries the canonical object shape; limit codes `SESSION_LIMIT` and `INVALID_LIMIT` are defined
there in section 3. `BUSY` always carries `retry_after_ms`; `SCOPE_TOO_LARGE` and
`PAYLOAD_TOO_LARGE` carry `action: narrow_parameters`; late `TIMEOUT` reports follow the batch
rules (a partially completed traversal returns `complete` or `partial` before any retry verdict).
Fresh retry (section 2, last row) is governed by the retry policy section of the catalog.

## 8. Benchmarks that close these numbers

| Gate | Evidence expected                                                                                  |
| ---- | -------------------------------------------------------------------------------------------------- |
| G0   | Feasibility numbers for pairing, a Starter read, and one export; not full profile proof (PRD 8.10) |
| G1   | Queue, cancellation, and pagination behavior under `AT07`, `AT08`; normalized reads within `AT10`  |
| G2   | Full N01 to N06 tables with p50/p95/p99, memory peaks, and mixture separation (PRD 2.1, 2.2)       |
| G3   | Multi-client scheduling evidence (FR22) and watch journal limits under `AT19`                      |

Reports must separate cached reads, fresh reads, snapshot reads, and asset exports, and must state
the exact commit, Figma version, and fixture hash (PRD 2.1, 2.2, Appendix C template).

## 9. Open questions

| ID    | Question                                                                                                                                         | Owner / closure                  |
| ----- | ------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------- |
| QRL01 | Do the sanity minimums (section 3) match the smallest viable profile, or should probes also do it                                                | Performance owner with G1 data   |
| QRL02 | Whether the global export slot should become one per session in a future version without breaking editor responsiveness                          | Plugin owner, needs N06 evidence |
| QRL03 | Whether `reassembly_bytes` needs a hard cap independent of the 32 MiB transfer limit, for multiple concurrent clients                            | Core owner (FR22 scheduling)     |
| QRL04 | Cache 128 MiB versus N04 256 MiB peak: measure real overlap before promising headroom                                                            | Performance owner, G2            |
| QRL05 | Aging weight: strict robbing of export slots when starved, or exponential aging curve                                                            | Scheduler owner, property tests  |
| QRL06 | Whether lowering-only extends to timeout floors on export: 60 s may be legally low on slow fixtures but shortening further risks flaky `TIMEOUT` | Performance owner                |

Final note: when any number in this document is superseded by Gate evidence, the table in section 2
keeps the PRD 8.5.5 wording and the measured value lands in the benchmark record with its exact
commit, so proposed defaults and measured facts never get merged silently (PRD 2.1, 2.3).
