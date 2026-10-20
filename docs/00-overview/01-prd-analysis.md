# PRD Deep Analysis: FigTether v1.1 (FGT-PRD-001)

Status: `Proposed` (analysis of record). Source: `FigTether_PRD_Confluence_v1.1.md` / `.docx`,
v1.1, 2026-10-20. This document is the canonical interpretation of the PRD. Where the PRD is open
or ambiguous, this document records a **commitment**; commitments must never contradict PRD IDs.

## 1. Executive summary

FigTether gives coding agents a **local, quota-independent path** to read Figma design data. A
Rust daemon runs on the developer's machine; a TypeScript plugin runs inside Figma Desktop and
exposes the documented Plugin API (tree, layout, text, styles, variables, PNG/SVG export); the two
talk over an authenticated loopback transport; an MCP server process exposes a small, read-only
tool surface to agents. Two data modes exist from day one: **live session** reads (fresh on
request) and **immutable snapshots** (explicit capture). Watch/invalidation (E09), multi-client
scheduling (FR22), and offline bundles (FR21) are explicitly deferred.

The product exists because the official Figma MCP server has seat/package-gated access and REST
alternatives consume REST quota and require tokens, both of which break intense agent workflows
(`P01`, `P02`). FigTether's core never calls official MCP or REST endpoints (`FR19`, `AT01`,
`AT02`).

## 2. Product facts (fixed interpretation)

| Aspect            | Interpretation                                                                                                                    |
| ----------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| Core language     | **Rust** for daemon, MCP adapter, CLI, contract model, scheduler, cache, asset store (`D01`)                                      |
| Adapter language  | TypeScript only inside the Figma plugin (main sandbox + UI iframe), enforced by scope review                                      |
| Primary target    | Figma Design desktop on Windows and macOS; Linux gets core/CLI/fixtures only until an editor environment is tested                |
| Read path         | Plugin API on the live path; `JSON_REST_V1` (`exportAsync`) is an optional adapter input, never the whole model (`FR08`)          |
| Write path        | None. v0.1/v0.2 are read-only (`D03`, `FR24`). Write-back requires a separate RFC                                                 |
| Modes             | Snapshot (immutable capture) and Live session (request-based). Watch is P1 and is invalidation-only, not a truth source (`D07`)   |
| Freshness         | `consistency=fresh` is the live default; cached reads require explicit `consistency=cached` plus `max_age_ms` (`D04`)             |
| Session identity  | UUID per plugin run/tab; `fileKey` is never the routing key (`D05`, `FR04`)                                                       |
| Default transport | Local WebSocket on loopback, plugin dials daemon; finalized by G0 evidence (`D06`)                                                |
| Default port      | `38947`, loopback only; collision fails loudly with `--port` guidance                                                             |
| Pricing/claims    | Free publication; claims limited to "core path does not consume official MCP/REST quota"; **no unlimited claims** (`P14`, `AT16`) |

## 3. Problem decomposition (P01-P14)

| ID  | Observed failure                        | FigTether response                                                               | Provenance artifact                  |
| --- | --------------------------------------- | -------------------------------------------------------------------------------- | ------------------------------------ |
| P01 | Official MCP quota halts agent sessions | Plugin-API path; core has no official MCP/REST adapter                           | `AT01`, egress audit at exact commit |
| P02 | REST MCPs shift quota and need PATs     | No Figma REST client, no PAT, no silent fallback                                 | `AT02`                               |
| P03 | Snapshot goes stale silently            | Live mode re-reads on demand; every payload carries freshness metadata           | `AT04`                               |
| P04 | Giant JSON floods agent context         | Summaries by default, field projection, pagination, staged resources             | `AT07`                               |
| P05 | Mass reads freeze the editor            | Bounded queues, one active editor op per session, sliced traversal, cancellation | `AT08`                               |
| P06 | Wrong-tab reads                         | Explicit `session_id`, grants bound to session, no auto tab selection            | `AT05`                               |
| P07 | JSON cannot represent visuals           | PNG/SVG export via plugin; unsupported-feature reporting                         | `AT11`                               |
| P08 | Agents invent missing information       | Field availability, warnings, provenance, partial results                        | `AT10`                               |
| P09 | Plugin dev setup pain                   | `figtether doctor`, manifest generator, guided pairing, prebuilt binaries        | `AT03`                               |
| P10 | Localhost is still attackable           | Auth, scope, Host/Origin validation, resource limits                             | `AT06`                               |
| P11 | Plugin close/sleep/Local network flake  | State machine, heartbeat, offline, reconnect-without-blind-replay                | `AT09`, `AT15`                       |
| P12 | Design-to-code marker panic claims      | Accurate data + assets first; starter code is optional later                     | Documentation, UI wording            |
| P13 | Seat/plan variance changes behavior     | Runtime capability probes; per-seat support matrix from real tests               | `AT12`                               |
| P14 | "Unlimited" marketing is unfalsifiable  | Measured claims only; benchmark tables published with commits                    | `AT16`                               |

## 4. Goals to measurement bridge

| Goal | Statement                                 | How it is evidenced                                                                  |
| ---- | ----------------------------------------- | ------------------------------------------------------------------------------------ |
| G01  | Zero core-path calls to official MCP/REST | `AT01`, `AT02`, source review at exact release commit                                |
| G02  | Intense local reads survive               | Workloads in `N01`, `N02`; workload report separates cached/fresh/snapshot/export    |
| G03  | No decisions from unlabeled stale data    | Envelope contract (`FR12`), `AT04`, `AT10`                                           |
| G04  | Installable without Rust/Node toolchain   | Prebuilt binary + plugin bundle; onboarding `p90 <= 15 min` (`N10`) with pilot users |
| G05  | Maintainable by community                 | Reproducible build from lockfiles, published schemas, contribution flow              |

## 5. Functional requirements inventory (FR01-FR24)

P0 requirements are mandatory for v0.1; P1 target v0.2; "Interpretation" is the committed reading
in case of PRD ambiguity.

| ID   | Pri | Requirement (condensed)                                                | Acceptance anchor         | Interpretation                                                                                                |
| ---- | --- | ---------------------------------------------------------------------- | ------------------------- | ------------------------------------------------------------------------------------------------------------- |
| FR01 | P0  | Prebuilt Rust core + plugin bundle; no Rust/Node install for end users | `AT03`                    | Distribution artifacts only; source build stays available for contributors                                    |
| FR02 | P0  | Authenticated, temporary pairing                                       | embedded in `AT06`        | 256-bit secret, single use, 5 min TTL, never in URL/log/tool schema                                           |
| FR03 | P0  | Grant selection-root with double enforcement                           | content in `AT06`, `AT17` | Both Rust daemon and plugin re-verify session + grant epoch + membership per request                          |
| FR04 | P0  | Restricted session discovery                                           | `AT05`                    | `list_sessions` exposes only sessions granted to this client; all reads require `session_id` or `snapshot_id` |
| FR05 | P0  | Immutable snapshots                                                    | `AT04`                    | `snapshot_id` and content hash are stable once captured                                                       |
| FR06 | P0  | Live node read                                                         | benchmark in `N02`        | Fresh must reach the plugin; observation start/end timestamps mandatory                                       |
| FR07 | P0  | Tree pagination with projected fields                                  | `AT07`                    | Resumable cursor; explicit depth/bytes limits; `truncated` never hidden                                       |
| FR08 | P0  | Normalized implementation context                                      | `AT10`                    | Geometry, layout, text runs, paints/effects, component refs surfaced wherever the API exposes them            |
| FR09 | P0  | Capability discovery with unsupported                                  | `AT12`                    | Runtime probes per session; seat never inferred from names; `null` vs missing distinguished                   |
| FR10 | P0  | Scoped style/variable reads                                            | `AT10`                    | Only in-scope references or approved dependencies; unresolved alias/mode reported                             |
| FR11 | P0  | Asset export PNG/SVG (fixture-level)                                   | `AT11`                    | Opaque `asset_id`, size/bytes limits, grant-bound resource URIs                                               |
| FR12 | P0  | Freshness contract on every result                                     | contract in `AT04`/`AT10` | `source`, `observed_at`, `cache_age_ms` (when cached), coverage, consistency                                  |
| FR13 | P0  | Scheduler with backpressure                                            | `AT08`                    | One active editor op per session; bounded queue; `BUSY` carries `retry_after_ms`                              |
| FR14 | P0  | Deadline, cancellation, bounded retry                                  | `AT08`, `AT15`            | Late results discarded; revoked grants never revived; retries max 1 and only for idempotent reads             |
| FR15 | P0  | Pause and revoke                                                       | `AT06`, `AT15`            | Pause gates new reads; revoke drops grant, invalidates cache and assets                                       |
| FR16 | P0  | Disturbance-tolerant connection                                        | `AT09`                    | Heartbeat, offline detection, reconnect with new epoch; no blind replay                                       |
| FR17 | P0  | MCP stdio + CLI                                                        | `AT14`                    | Handshake validated against at least two MCP clients; stdout carries protocol only                            |
| FR18 | P0  | Local diagnostics                                                      | review at `G1`            | Queue/cache/latency metrics, error IDs, versions; no design content or secrets by default                     |
| FR19 | P0  | No official MCP/REST on core path                                      | `AT01`                    | No adapter in core; egress audit + review at release commit                                                   |
| FR20 | P1  | Watch with resync                                                      | `AT19`                    | Gap detection; deleted IDs tombstoned; stale cursor demands resync                                            |
| FR21 | P1  | Versioned offline bundle                                               | `AT20`                    | Hash + manifest + schema; import cannot pose as live                                                          |
| FR22 | P1  | Multi-client fairness                                                  | soak extension of `AT08`  | Per-client grant isolation, fair queues, per-client limits                                                    |
| FR23 | P1  | Optional starter code                                                  | review                    | Separate from acquisition; warnings for unsupported features; no ephemeral asset URLs                         |
| FR24 | P0  | No document mutation                                                   | `AT18`                    | Fixture hash/props unchanged before/after; selection/page untouched                                           |

## 6. Nonfunctional targets (N01-N10)

All values are **targets, not results**. Reference profile: 4 logical cores, 8 GB RAM, release
build, Figma Desktop version recorded, fixtures non-private.

| ID  | Target (condensed)                                                            | Method                                                                          |
| --- | ----------------------------------------------------------------------------- | ------------------------------------------------------------------------------- |
| N01 | Cached metadata `p95 <= 100 ms`, response `<= 8 KiB`, sustained `>= 20 req/s` | 10,000 local reads                                                              |
| N02 | Fresh read `p95 <= 2 s` for `<= 100` simple nodes / `<= 64 KiB`               | 1,000 sequential fresh reads within 60 min, all reaching the plugin             |
| N03 | PNG export 1024x1024 `p95 <= 5 s` on simple fixture                           | 100 exports, isolated queue, render cost logged                                 |
| N04 | Idle RSS `<= 50 MiB`; mixed-load peak `<= 256 MiB`                            | 1 s sampling; documented allocator overhead                                     |
| N05 | No crash/OOM; unexpected errors `< 0.1%`                                      | 60 min mixed soak, 2 clients, 4 mock sessions, at least one real-editor session |
| N06 | No editor freeze `> 1 s` from our traversal                                   | Manual interaction + profiling on reference fixture                             |
| N07 | Logical cancel/revoke `<= 1 s`; no post-revoke payload                        | Timestamps from daemon + plugin                                                 |
| N08 | Offline detection `<= 15 s`; reconnect readiness `<= 10 s`                    | Sleep/wake, plugin kill, restart, port collision                                |
| N09 | Zero official MCP/REST requests from core                                     | Source review, deny-egress mock, process-connection capture                     |
| N10 | First onboarding `p90 <= 15 min`; repeat `<= 2 min`                           | Pilot with `>= 10` users, per-step failure logging                              |

## 7. Assumption audit (section 3 of the PRD)

Documentation facts (treat as design inputs, re-verify at G0 on the exact Figma release under
test): official MCP limits are plan/seat-based (`S1`); `exportAsync` supports `JSON_REST_V1`
(`S4`); plugin access is seat-dependent with Starter supporting Design plugins (`S2`); new plugins
use `dynamic-page` and manifest-controlled network access (`S5`); global `documentchange` needs
`loadAllPagesAsync` and has known non-delivery cases (`S6`); `fileKey` is not generally available
to public plugins (`S7`).

Hypotheses that G0 must convert into evidence:

1. _Plugin-API reads do not consume official MCP quota._ Path-separation proof: capture process
   connections on the exact commit; mock deny-egress; account-side check where possible.
2. _Users accept keeping the plugin open during agent sessions._ Validated in a 30-minute session
   study plus the G2 pilot.
3. _4 cores / 8 GB windows the reference workloads._ Measured in G1/G2; adjust published profile,
   never fake the numbers.
4. _Local WebSocket from the plugin UI iframe works on Windows and macOS._ CSP/Origin/manifest
   tested per OS (`R02`); fallback designs stay ADR-002 bound.

## 8. PRD decisions (D01-D12) and resolution path

| ID  | Topic                                            | Status              | Owner of closure                                       |
| --- | ------------------------------------------------ | ------------------- | ------------------------------------------------------ |
| D01 | Rust core + TS adapter                           | Decided in PRD      | `ADR-001`; build/size evidence at G0                   |
| D02 | Plugin path with no REST/PAT/official MCP        | Decided             | `AT01`/`AT02`                                          |
| D03 | Read-only v0.1/v0.2, no eval                     | Decided             | Handler audit + `AT18`                                 |
| D04 | Fresh default; cached explicit                   | Decided             | `AT04`, split benchmarks                               |
| D05 | Session identity independent of fileKey          | Decided             | `AT05`, probes                                         |
| D06 | Local WebSocket transport candidate              | Awaiting G0         | `ADR-002` + OS test matrix                             |
| D07 | Watch page-scoped (if any); global never default | Conditional         | nodechange coverage test; otherwise request-based only |
| D08 | rmcp version, MSRV, client support               | Awaiting kickoff G0 | `ADR-003`; pin + compatibility evidence                |
| D09 | Variables/library resolution per seat            | Awaiting G0         | Probe matrix; unavailable remains a valid answer       |
| D10 | Final name and dual license                      | Proposed            | `ADR-009`; trademark + dependency license review       |
| D11 | Community plugin publication                     | Out of v0.1         | Production manifest review at G4 window                |
| D12 | Performance beyond initial profile               | Not promised        | Only via reproducible benchmark publication            |

## 9. Gaps and open engineering questions

These do not block G0 but must be closed with evidence or explicit "unsupported" results:

1. `JSON_REST_V1` field deltas across Figma versions: needs a per-version fixture diff gate.
2. Plugin UI iframe -> loopback WS: CSP/Origin behavior per OS and Figma release; protocol for
   capturing failures (`R02`).
3. Rmcp SDK capability negotiation vs. client matrix with at least two target clients (`AT14`).
4. Variables API availability under Starter seats (`S10`, `D09`).
5. Timer/`setTimeout` behavior in the main sandbox and the UI iframe (affects slicing, debounce).
6. Asset URL origin (ephemeral image fills) constraining what the adapter may pass through
   (PRD FR23 wording).
7. Rapid re-pair after daemon crash: stale runtime file detection rules (`FR16`, ops runbook).
8. Mixed editable collaboration: best-effort labeling requirements in populated docs
   (`R14`, PRD 8.3).
9. Envelope capture identity: the PRD 8.5.2 envelope has no explicit `snapshot_id` (or
   `offline_bundle` id) member although FR04/FR05 require results to be bound to a capture; the
   contract marks this as a proposed extension in `docs/04-data-contracts/01-mcp-envelope.md`,
   pending G0 contract freeze.

## 10. Committed interpretations (binding on code and docs)

1. **Fresh-first.** `consistency=fresh` is the only live default; cached requires explicit opt-in.
2. **Fail-closed.** Any auth, scope, version, or capability uncertainty denies or reports
   `unsupported`; nothing silently falls back (no REST fallback, no snapshot posing as live).
3. **Freeze semantics.** `snapshot` values are immutable once written; snapshot capture is always
   `consistency=best_effort`.
4. **Scope is monotonic within a grant** until epoch changes; moving selection never widens scope;
   out-of-subtree nodes stop resolving.
5. **Bounded everything.** Every wire transfer and memory structure has an explicit cap from
   `04-data-contracts/06-resource-limits.md`; exceeding a cap is a structured error, not default.
6. **No eval, no shell, no arbitrary JS execution** anywhere in the tool chain (`D03`).
7. **Never auto-instrument official Figma endpoints**, even as diagnostics.

## 11. Traceability discipline

Each `FR*` maps to at least one `AT*` and at least one design section; see
`06-testing/03-traceability-matrix.md`. Each `N*` maps to a benchmark procedure in
`06-testing/04-benchmark-plan.md`. PRD problems `P*` map into the problem decomposition table
above and reporting templates. Any newly identified requirement takes a new ID with owner and
test before entering a sprint.
