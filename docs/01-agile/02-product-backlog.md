# FigTether Product Backlog

Status: `Proposed`

Purpose. This document turns the PRD 4.2 delivery structure into a working backlog: the epic
table `E01`..`E10` with deliverables, dependencies, and priorities from PRD 4.2; the six persona
stories expanded as `US-01`..`US-06` with Given/When/Then acceptance criteria and FR/AT links;
per-epic item tables; a totals comparison against the PRD 12 to 18 engineer-week envelope
(PRD 8.10); and the decision spikes for `D06`, `D08`, `D09`. Process rules (DoR, DoD, estimation,
gross capacity) are in `docs/01-agile/01-sdlc-process.md`. Priority classes follow PRD 4.2: P0 is
mandatory for v0.1, P1 targets v0.2 after P0 stabilization, P2 is exploration without a schedule
promise. Note from PRD 4.2: P0 items may produce explicit `unsupported` results for platform
capabilities that are genuinely unavailable; basic node reads and PNG fixture export must succeed
at `G0`.

## 1. Epic table (PRD 4.2, verbatim structure)

| Epic                     | Deliverable                                                               | Depends on                         | Priority |
| ------------------------ | ------------------------------------------------------------------------- | ---------------------------------- | -------- |
| E01 Feasibility          | Starter probe, export, transport, Rust MCP handshake, evidence            | none                               | P0 / G0  |
| E02 Contracts            | Schemas, DTO, ID and epoch rules, errors, cross-language tests            | E01                                | P0       |
| E03 Session/access       | Pairing, ACL, grant UI, scope enforcement, revoke                         | E02                                | P0       |
| E04 Acquisition          | Live read, snapshot capture, tree pagination, capability probes           | E03                                | P0       |
| E05 Context/assets       | Normalizer, styled text, layout, paints, tokens, PNG/SVG                  | E04                                | P0       |
| E06 Reliability          | Scheduler, memory and bytes limits, cancel, reconnect, diagnostics        | E03 plus E05                       | P0       |
| E07 MCP/package          | Tool and resource adapter, config print, doctor, binary and plugin bundle | E02/E04; final packaging after E06 | P0       |
| E08 Verification/release | Fixtures, soak, security tests, pilot, docs, provenance                   | E05 plus E07                       | P0       |
| E09 Watch/offline        | Scoped invalidation, resync, journal, bundles                             | G2 stable                          | P1       |
| E10 Ecosystem            | Templates, package managers, Community publication                        | Need evidence and G3               | P2       |

Critical path per PRD 4.2: E01 -> E02 -> E03 -> E04 -> E05/E06 -> E07 -> E08. Parallel work is
allowed only after contracts are agreed (PRD 4.2 closing note) and final packaging waits for E06
(PRD 4.2 E07 row).

## 2. Persona stories US-01..US-06

Estimates are provisional SP on the calibration defined in
`docs/01-agile/01-sdlc-process.md` section 8; story SP overlap epic items and are not additive.
IDs are new but never renumber PRD ids.

### US-01 Starter solo developer: frame-scoped iterative reads (P0)

| Aspect                        | Detail                                                                              |
| ----------------------------- | ----------------------------------------------------------------------------------- |
| Story (PRD 4.2 persona row 1) | I grant one frame, then the agent reads its layers and assets all session           |
| FR/AT links                   | FR03, FR04, FR06, FR07, FR11, FR12, FR17, FR19 / AT01, AT03, AT05, AT07, AT11, AT14 |
| Gate, estimate                | G1 core flow; G2 onboarding (`N10`) / 8 SP                                          |

| Given                                                                             | When                                                   | Then                                                                                                                           |
| --------------------------------------------------------------------------------- | ------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------ |
| Plugin session is paired on a Starter-eligible account and a grant covers frame F | agent requests a fresh read of F with projected fields | read reaches the plugin, carries `source`, `observed_at`, `coverage`, and a resumable cursor when truncated (FR06, FR07, FR12) |
| Layer L is inside F                                                               | agent requests PNG asset for L                         | asset is served through an opaque `asset_id` inside size and bytes limits (FR11; AT11)                                         |
| MCP client connected over stdio, no official Figma MCP configured                 | any tool call runs                                     | handshake is clean on stdout and no official MCP/REST request leaves the core (FR17, FR19; AT01, AT02, AT14)                   |

### US-02 Companion designer: coverage, pause, revoke (P0)

| Aspect                        | Detail                                                                                 |
| ----------------------------- | -------------------------------------------------------------------------------------- |
| Story (PRD 4.2 persona row 2) | I see access coverage, pause the session, and revoke access without closing the design |
| FR/AT links                   | FR02, FR03, FR12, FR14, FR15 / AT06, AT15, plus revoke timing evidence for `N07`       |
| Gate, estimate                | G1; re-test at G2 / 5 SP                                                               |

| Given                                  | When                                            | Then                                                                                                                                |
| -------------------------------------- | ----------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| Grant is active                        | designer opens the grant panel in the plugin UI | roots, connected client, freshness basis, and session state are shown (FR12; state machine per PRD 8.4)                             |
| Session READY                          | designer selects Pause                          | new reads are rejected with a structured error; resume revalidates the grant (FR15; AT06)                                           |
| Active grant with a transfer in flight | designer selects Revoke                         | grant drops, cache and asset entries are invalidated, in-scope replies stop per revoke rules, epoch rises (FR14, FR15; AT15; `N07`) |

### US-03 Developer on client files: clear permission status first (P0)

| Aspect                        | Detail                                                          |
| ----------------------------- | --------------------------------------------------------------- |
| Story (PRD 4.2 persona row 3) | I get a clear permission status before the agent reads the file |
| FR/AT links                   | FR03, FR04, FR09, FR12, FR19 / AT05, AT06, AT12                 |
| Gate, estimate                | G1; G2 confidence probe call / 5 SP                             |

| Given                                                | When                  | Then                                                                                                                                                                                                                             |
| ---------------------------------------------------- | --------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Session registers in an unknown seat or plan context | capability probes run | per-session capability results are exposed, seat is never inferred from account names, and unreachable features return explicit `unsupported` (FR09; AT12; interpretation in `docs/00-overview/01-prd-analysis.md` section 10.2) |
| Read targets a node outside the grant subtree        | request is processed  | structured scope error; no sibling or ancestor data leaks (FR03, FR04; AT07 isolation row)                                                                                                                                       |
| Platform or permission context denies plugin use     | agent calls a tool    | fail-closed denial with reason and remediation hint; no silent switch to snapshot posing as live or to REST (FR19; fail-closed rule 10.2)                                                                                        |

### US-04 Multi-agent user: two clients, fair queues (P1)

| Aspect                        | Detail                                                                        |
| ----------------------------- | ----------------------------------------------------------------------------- |
| Story (PRD 4.2 persona row 4) | I connect two clients to the same session with a fair queue and no tab mixups |
| FR/AT links                   | FR03, FR04, FR13, FR22 / AT05, AT08 soak extension                            |
| Gate, estimate                | G3 (E09) / 5 SP                                                               |

| Given                                         | When                   | Then                                                                                                                             |
| --------------------------------------------- | ---------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| Clients A and B are granted on session S      | both read concurrently | grant and data isolation hold per client; queue interleaves fairly; `BUSY` carries `retry_after_ms` (FR22, FR13; AT08 extension) |
| One client issues an oversized or slow stream | other client reads     | per-client limits bound queue, memory, and bytes; the unaffected client stays within latency targets (FR22, FR13; `N01`, `N05`)  |
| Same node read by both clients                | results return         | neither client receives data, cursors, or assets belonging to the other grant (FR03, FR04; AT05)                                 |

### US-05 OSS maintainer: sanitized diagnostics and public fixtures (P0)

| Aspect                        | Detail                                                                                              |
| ----------------------------- | --------------------------------------------------------------------------------------------------- |
| Story (PRD 4.2 persona row 5) | I receive sanitized diagnostics and public fixtures and can reproduce failures without private data |
| FR/AT links                   | FR18, FR13, FR16 / AT08, AT09 evidence reuse                                                        |
| Gate, estimate                | G1 review of FR18; G2 full suites / 5 SP                                                            |

| Given                                             | When                      | Then                                                                                                                                                       |
| ------------------------------------------------- | ------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| A failure report arrives                          | diagnostics are collected | versions, queue and cache and latency metrics, error ids, and command context are included; no design content or secrets by default (FR18; R13 mitigation) |
| Maintainer has public fixtures and sanitized logs | reproduction runs         | failure reproduces on fixtures from the licensed corpus (PRD 2.1 fixture rules; OR2)                                                                       |
| Schema or protocol mismatch                       | contract tests run        | mismatch is identified cross-language through `schemas/` artifacts; optional unknown fields tolerated per the PRD 8.11 versioning row                      |

### US-06 Offline user: labeled local snapshots (P1)

| Aspect                        | Detail                                                                   |
| ----------------------------- | ------------------------------------------------------------------------ |
| Story (PRD 4.2 persona row 6) | I open a local snapshot with an offline label and a clear schema version |
| FR/AT links                   | FR21 / AT20, plus decompression bounds via AT13                          |
| Gate, estimate                | G3 (E09) / 3 SP                                                          |

| Given                                                      | When          | Then                                                                                                                    |
| ---------------------------------------------------------- | ------------- | ----------------------------------------------------------------------------------------------------------------------- |
| Bundle with hash, manifest, and schema version is exported | user opens it | reads are labeled offline with schema version; the import can never pose as a live session (FR21; AT20)                 |
| Bundle from an older schema                                | opened        | version check reports the migration needs; decompression stays bounded (FR21; PRD 8.11 versioning; AT13 zip bomb class) |
| Bundle hash is invalid                                     | opened        | structured refusal instead of partial trust (FR21; fail-closed interpretation 10.2)                                     |

## 3. Per-epic item backlogs

Items are sized 1-3 SP each and pass DoR (PRD 8.10.1) before sprint entry. Ranges: 5-8 items per
epic. Priority column is the PRD class; every P0 item carries an AT anchor (PRD 8.8).

### E01 Feasibility (G0) - 10 SP

| Item                                                                      | FR/AT refs                            | Priority | SP  |
| ------------------------------------------------------------------------- | ------------------------------------- | -------- | --- |
| Small fixture plus before/after change-detection harness                  | AT18, PRD 2.1                         | P0       | 1   |
| Loopback transport and pairing handshake, minimal end-to-end              | FR02, FR16; AT02 loopback requirement | P0       | 2   |
| Starter read probe on eligible seat plus permission-context refusal probe | AT12, FR09 basis                      | P0       | 1   |
| `exportAsync` JSON node proof plus single PNG export                      | FR06, FR11                            | P0       | 2   |
| rmcp stdio handshake plus one read tool through a first MCP client        | FR17; AT14 basis, D08                 | P0       | 2   |
| Egress evidence pack: deny-egress mock and process connection capture     | AT01, AT02, `N09` method, P01/P02     | P0       | 2   |

### E02 Contracts - 10 SP

| Item                                                                          | FR/AT refs                                                 | Priority | SP  |
| ----------------------------------------------------------------------------- | ---------------------------------------------------------- | -------- | --- |
| Tool surface and envelope schemas v1, shared cross-language                   | FR12, FR17; PRD 8.5                                        | P0       | 2   |
| Structured error taxonomy including `BUSY` with `retry_after_ms`              | FR13                                                       | P0       | 1   |
| ID model: `session_id`, grants, epoch, `snapshot_id`, `observation_id`        | FR03, FR04, FR05; PRD 8.4                                  | P0       | 2   |
| Pagination cursor and field projection contract                               | FR07                                                       | P0       | 1   |
| Freshness and coverage fields plus cap table types                            | FR12, FR07; `docs/04-data-contracts/06-resource-limits.md` | P0       | 1   |
| Cross-language contract test harness (Rust and TypeScript against `schemas/`) | FR02..FR12 basis; PRD 8.11                                 | P0       | 2   |
| Resource bound enforcement types (depth, bytes, chunk shapes)                 | FR07, FR13                                                 | P0       | 1   |

### E03 Session/access - 10 SP

| Item                                                                             | FR/AT refs              | Priority | SP  |
| -------------------------------------------------------------------------------- | ----------------------- | -------- | --- |
| Pairing service: 256-bit secret, single use, 5 minute TTL, never in URL or logs  | FR02                    | P0       | 2   |
| Grant registry with epochs and scope-root union, dedup descendants               | FR03; PRD 8.4 scope row | P0       | 2   |
| Grant, pause, and revoke UI in `plugins/figma/ui/`                               | FR15                    | P0       | 2   |
| Double enforcement membership checks in `crates/daemon` and `plugins/figma/main` | FR03; AT17              | P0       | 2   |
| Restricted session discovery (`list_sessions` exposes granted only)              | FR04; AT05              | P0       | 1   |
| Cache and asset invalidation on revoke, stale frame rejection                    | FR15; AT15, AT17        | P0       | 1   |

### E04 Acquisition - 12 SP

| Item                                                                       | FR/AT refs       | Priority | SP  |
| -------------------------------------------------------------------------- | ---------------- | -------- | --- |
| Live read pipeline request to plugin to result with observation timestamps | FR06; `N02`      | P0       | 2   |
| Snapshot capture with stable `snapshot_id` and content hash                | FR05; AT04       | P0       | 2   |
| Sliced tree walker with pagination and projection                          | FR07; AT07       | P0       | 3   |
| Capability probes per session                                              | FR09; AT12       | P0       | 2   |
| Freshness and coverage metadata on every result                            | FR12; AT04, AT10 | P0       | 1   |
| Read-only guard: no selection or page mutation during traversal            | FR24; AT18       | P0       | 2   |

### E05 Context/assets - 12 SP

| Item                                                             | FR/AT refs               | Priority | SP  |
| ---------------------------------------------------------------- | ------------------------ | -------- | --- |
| Normalizer skeleton and versioned model types in `crates/model`  | FR08                     | P0       | 2   |
| Text runs and typography mapping                                 | FR08; AT10               | P0       | 2   |
| Layout and geometry plus fills, strokes, effects                 | FR08; AT10               | P0       | 2   |
| Style and variable references with unresolved ref reporting      | FR10; AT10; D09 evidence | P0       | 2   |
| Asset store with opaque `asset_id` and grant-bound resource URIs | FR11                     | P0       | 2   |
| PNG/SVG export handling plus MIME and hash verification          | FR11; AT11               | P0       | 2   |

### E06 Reliability - 12 SP

| Item                                                                         | FR/AT refs        | Priority | SP  |
| ---------------------------------------------------------------------------- | ----------------- | -------- | --- |
| Scheduler: one active editor op per session, bounded queue, `BUSY` retry     | FR13; AT08        | P0       | 2   |
| Deadline, cancellation, bounded retry, late frame discard                    | FR14; AT08, AT15  | P0       | 2   |
| Heartbeat, offline detection, reconnect with epoch raising, no replay        | FR16; AT09        | P0       | 2   |
| Memory and bytes budgets per limit table                                     | FR07, FR13; `N04` | P0       | 2   |
| Local diagnostics: metrics, error ids, versions, sanitized output            | FR18              | P0       | 2   |
| Mixed soak harness: two clients, four mock sessions, one real editor session | FR13, FR16; `N05` | P0       | 2   |

### E07 MCP/package - 12 SP

| Item                                                            | FR/AT refs                  | Priority | SP  |
| --------------------------------------------------------------- | --------------------------- | -------- | --- |
| Tool and resource adapter on rmcp with cancellation passthrough | FR17; AT14; D08             | P0       | 3   |
| `figtether config` print plus port collision guidance           | FR17; PRD 8.4 default port  | P0       | 1   |
| `figtether doctor` with pairing repair guidance                 | FR18; P09                   | P0       | 2   |
| Plugin manifest generator for `plugins/figma`                   | FR01; P09                   | P0       | 2   |
| Binary and plugin bundle distribution checks                    | FR01; AT03                  | P0       | 2   |
| Cross-OS build matrix: Windows x64 and macOS arm64/x64          | FR01; PRD 8.11 platform row | P0       | 2   |

### E08 Verification/release - 14 SP

| Item                                                                                       | FR/AT refs                | Priority | SP  |
| ------------------------------------------------------------------------------------------ | ------------------------- | -------- | --- |
| Fixtures medium and large plus adversarial dataset                                         | PRD 2.1; AT07, AT13       | P0       | 3   |
| Full security tests: wrong token, expired, replay, Host/Origin, scope, revoke              | AT06; FR02/03/14/15       | P0       | 3   |
| Benchmark runs with cached, fresh, and export separated                                    | `N01`..`N03`; PRD 2.1/2.2 | P0       | 2   |
| Soak and stability runs plus freeze profiling                                              | `N04`..`N07`; FR13/16     | P0       | 1   |
| Pilot protocol and onboarding measurement                                                  | `N10`; PRD 2.3            | P0       | 2   |
| Provenance, SBOM, checksums, exact-release-commit artifact checks                          | PRD 8.11; DD-5            | P0       | 2   |
| Release docs: quickstart, support matrix, privacy, reporting, changelog, known limitations | FR01, AT16; DD-7          | P0       | 1   |

### E09 Watch/offline (P1, G3) - 12 SP

| Item                                                                           | FR/AT refs                            | Priority | SP  |
| ------------------------------------------------------------------------------ | ------------------------------------- | -------- | --- |
| Scoped invalidation watch with gap detection and resync trigger                | FR20; AT19; D07 page-scoped condition | P1       | 3   |
| Resync baseline rebuild and stale cursor rules                                 | FR20; AT19                            | P1       | 2   |
| Change journal with tombstones for deleted ids                                 | FR20; AT19                            | P1       | 2   |
| Offline bundle export, import, hash and manifest checks, bounded decompression | FR21; AT20                            | P1       | 3   |
| Multi-client fairness and per-client limits                                    | FR22; AT08 soak extension             | P1       | 2   |

### E10 Ecosystem (P2, G4 window) - 6 SP

| Item                                                | FR/AT refs                          | Priority | SP  |
| --------------------------------------------------- | ----------------------------------- | -------- | --- |
| Starter templates for common agent harnesses        | E10 deliverable; PRD 8.10 G4        | P2       | 2   |
| Package manager distribution research need evidence | E10; PRD 8.10 G4 gate rule          | P2       | 2   |
| Community publication manifest preparation          | FR01; PRD 8.11; D11 scope-out check | P2       | 2   |

## 4. Totals versus the PRD envelope

| Slice                    | SP  | Notes                                                                     |
| ------------------------ | --- | ------------------------------------------------------------------------- |
| E01..E08 (v0.1 P0 scope) | 92  | Provisional; expected to compress 10-20 percent after the first 2 sprints |
| E09 (v0.2 P1 scope)      | 12  | G3 window per PRD 8.10                                                    |
| E10 (v0.3 P2 scope)      | 6   | Evidence-driven only (PRD 8.10 G4)                                        |
| US stories               | 31  | Overlap epic items; not additive to the totals above                      |

| Conversion and comparison | Value                                                             | Caveat                                                                                                                                                                                                           |
| ------------------------- | ----------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Provisional mapping       | 1 SP is about 1 engineer-day                                      | Reporting convention from `docs/01-agile/01-sdlc-process.md` section 8                                                                                                                                           |
| v0.1 scope rounded        | 92 engineer-days, about 18.4 engineer-weeks                       | Upper edge of the PRD 12 to 18 engineer-week envelope (PRD 8.10); the gap is closed by the gate trim rule (speed-ups when a gate or sprint exit criteria are met) and by expected sizing compression at Sprint 2 |
| Capacity check            | 20 SP per sprint x 5 sprints (Sprint 0..4) = 100 SP               | Leaves about 8 SP planning slack for gate ceremonies and part-time QA ramp-up (PRD 8.10)                                                                                                                         |
| Overflow response         | Cut P1 and P2 scope first, then B-tier P0 items (polish, tooling) | Never drop P0 acceptance tests or failed tasks from reports (PRD 2.3)                                                                                                                                            |

## 5. Decision spikes (PRD D06, D08, D09)

| Spike id | Decision and PRD anchor                                                   | Timebox and owner                      | Evidence to close                                                                                                                               | Lands in                                                         |
| -------- | ------------------------------------------------------------------------- | -------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------- |
| SPK-01   | Transport choice for D06 (local WebSocket candidate, plugin dials daemon) | 5 days, Sprint 0, core owner           | Per-OS Windows plus macOS loopback results incl. CSP/Origin/manifest behaviors recorded; failure capture protocol written (R02, analysis gap 2) | ADR-002                                                          |
| SPK-02   | rmcp version, MSRV, client support for D08                                | 5 days, Sprint 0, core owner           | Pinned version passes initialize/tools/resources/cancellation with two target clients (`AT14` basis); lockfile committed                        | ADR-003                                                          |
| SPK-03   | Variables and library resolution per seat for D09                         | 3 days inside Sprint 0-1, plugin owner | Probe matrix across Starter-eligible and restricted contexts; unavailable remains a valid answer (PRD 8.10 D09 line; AT12 context)              | Capability matrix under `docs/04-data-contracts/` plus FR10 item |
| SPK-04   | Timer and `JSON_REST_V1` drift open points                                | 3 days, Sprint 1, engineers pair       | Timer behavior in main sandbox and UI iframe recorded; per-version fixture diff procedure drafted (analysis 9.1, 9.5)                           | ADR follow-up where contracts are affected                       |

Spikes close only with recorded evidence (Appendix C fields); a spike that cannot close pushes
its decision into the next gate review with a fallback plan; it never suspends DoR rule DoR-3
(SDK pins).

## 6. Open questions

1. Whether `list_sessions` unfiltered output ever gets reused internally without the FR04 filter
   is a security-review question raised for Sprint 2.
2. SP totals are provisional until velocity exists after 2 sprints; recalibration may pull items
   between epics but never renames ids.
3. E07 final packaging order: whether the cross-OS build matrix precedes or follows the doctor
   CLI is decided at Sprint 3 planning together with CI runner availability (OR4).
