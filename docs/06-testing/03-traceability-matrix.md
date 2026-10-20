# Traceability Matrix

Status: `Proposed`

Purpose. This file is the single traceability surface for the FigTether documentation set: it maps
every functional requirement (FR01..FR24), nonfunctional target (N01..N10), problem statement
(P01..P14) and product decision (D01..D12) to acceptance tests (AT01..AT20), delivery epics
(E01..E10), release gates (G0..G4), design documents, data contracts and backlog stories. It
implements the traceability rules in docs/00-overview/01-prd-analysis.md (section 11) and
Appendix A of FigTether_PRD_Confluence_v1.1 (`FGT-PRD-001`). Scenario detail lives in
docs/06-testing/02-acceptance-catalog.md; benchmark procedures live in
docs/06-testing/04-benchmark-plan.md. All IDs are verbatim from the PRD: nothing is renumbered or
reassigned, and every value in a Target column is a target until a published evidence report (slots
in docs/07-ops/04-release-process.md) records measured results.

Extension for the write path. Section 3.1 adds the write path requirements `WR01..WR09`, which
come from `ADR-013` and not from the PRD, and maps them to the write path scenarios
`AT21..AT29` and to the write path design documents. The PRD `FR`, `N`, `P`, and `D` sets keep
their identity and their row counts; `D13` is a proposed decision from `ADR-013` and is listed in
matrix (d) with the same convention as the PRD decisions.

## 1. Reading guide and conventions

| Rule             | Meaning                                                                                                                                                                                                                                                                                                                                                                             |
| ---------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| ID authority     | PRD Appendix A stays the authority for AT-to-FR association; where a row lists several FRs, all of them propagate into every matrix below.                                                                                                                                                                                                                                          |
| Multi-gate tests | Appendix A may list two gates for one test (for example AT06 shows G1/G2). Both are kept verbatim; the earlier gate introduces the test, later gates re-verify it on release candidates.                                                                                                                                                                                            |
| US slugs         | Story slugs (US-...) name the PRD 4.2 user stories; docs/01-agile/02-product-backlog.md binds each slug to a backlog row and sprint items. US slugs never replace FR IDs.                                                                                                                                                                                                           |
| Folder shorthand | In matrices (a), design cells resolve under docs/02-architecture/, contract cells under docs/04-data-contracts/, security cells under docs/05-security/, ops cells under docs/07-ops/.                                                                                                                                                                                              |
| Pinned slugs     | Only these paths are pinned cross-document today: docs/04-data-contracts/06-resource-limits.md, docs/07-ops/03-runbook.md, docs/07-ops/04-release-process.md, docs/02-architecture/adr/, docs/06-testing/02-acceptance-catalog.md. Other numbered slugs (for example 02-tool-catalog.md) resolve against each folder index when created; IDs never change because of a file rename. |
| Targets          | PRD 2.1 numbers (N01..N10) are targets, never results; see docs/00-overview/01-prd-analysis.md section 6 and the "What is deliberately missing" note in docs/README.md.                                                                                                                                                                                                             |

| Matrix         | Primary consumer                 | Feeds which decision                                        |
| -------------- | -------------------------------- | ----------------------------------------------------------- |
| (a) FR01..FR24 | engineering, QA, sprint lead     | sprint scoping and Definition of Done per requirement       |
| (b) N01..N10   | performance owner, release owner | which benchmark runs block which gate                       |
| (c) P01..P14   | product maintainer, QA           | proof that each claimed problem response is actually tested |
| (d) D01..D12   | architects, maintainers          | which ADRs and evidence close a gate                        |

## 2. Gate map

| Gate           | Blocking acceptance                                                      | Scope notes                                                                                                                                                                                    |
| -------------- | ------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| G0 feasibility | AT01, AT02, AT12, AT18                                                   | pairing transport, eligible Starter read, JSON + PNG export, capability probe, no-mutation check on real Windows + macOS desktops; G0 evidence is later reproduced at the exact release commit |
| G1 alpha       | AT04, AT05, AT06, AT07, AT08, AT09, AT10, AT13, AT17                     | all P0 functions working internally: fresh/snapshot/cached semantics, double scope enforcement, pagination, cancellation, diagnostics review (FR18 closes by review here)                      |
| G2 public v0.1 | AT03, AT08, AT09, AT11, AT14, AT15, AT16 plus all P0 requirements        | release-candidate artifacts from the exact commit; N01..N09 measured and republished in release evidence slots                                                                                 |
| G3 v0.2        | AT19, AT20 plus P1 set (FR20, FR21, full FR22 multi-client, FR23 review) | watch/resync, offline bundle, fairness; a feature is not published as stable when its gate fails                                                                                               |
| G4 v0.3+       | E10 items; D10/D11 closure windows                                       | Community publication decisions; tracked in docs/01-agile/ roadmap, not in this matrix                                                                                                         |

Note: tests listed at two gates (AT06, AT09, AT10, AT11, AT13, AT15 among others) must keep passing
at every gate where they are listed; a later gate never lowers an earlier one.

Note: the write path scenarios `AT21..AT29` are assigned to no gate. `ADR-013` is pending product
approval (`docs/00-overview/01-prd-analysis.md` section 12), and the PRD lists write-back as not
available until a separate RFC. Adding a gate for them is a product decision, not a doc edit.

## 3. Matrix (a): FR01..FR24

| ID   | Pri | Requirement (short)                                                                                  | Acceptance tests                         | Epics    | Design refs (docs/02-architecture/)                                         | Contract refs (docs/04-data-contracts/)                                        | Gate (Appendix A)    | Primary gate | Backlog US                |
| ---- | --- | ---------------------------------------------------------------------------------------------------- | ---------------------------------------- | -------- | --------------------------------------------------------------------------- | ------------------------------------------------------------------------------ | -------------------- | ------------ | ------------------------- |
| FR01 | P0  | Prebuilt Rust core + plugin bundle; no Rust/Node install for end users                               | AT03                                     | E07, E08 | 01-system-design.md (packaging)                                             | none; artifact set + checksums in docs/07-ops/04-release-process.md            | G2                   | G2           | US-install-first-run      |
| FR02 | P0  | Authenticated, temporary pairing; 256-bit single-use secret, 5 min TTL, never in URL/log/tool schema | AT03, AT06                               | E03      | 01-system-design.md (pairing), adr/ (D06), threat model (docs/05-security/) | 03-bridge-protocol.md (auth frame)                                             | G1/G2                | G1           | US-pair-plugin            |
| FR03 | P0  | Grant selection-root with double enforcement in Rust and plugin                                      | AT05, AT06, AT17                         | E03      | 01-system-design.md (grant model), threat model                             | 02-tool-catalog.md, 03-bridge-protocol.md (epoch fields)                       | G1/G2                | G1           | US-grant-root             |
| FR04 | P0  | Session discovery restricted to authorized clients; reads require session_id or snapshot_id          | AT05                                     | E03, E07 | 01-system-design.md (session registry)                                      | 02-tool-catalog.md (list_sessions)                                             | G1                   | G1           | US-pick-session           |
| FR05 | P0  | Immutable snapshots; snapshot_id and content hash stable                                             | AT04                                     | E04      | 01-system-design.md (snapshot store)                                        | 01-mcp-envelope.md (source.kind), 04-normalized-model.md                       | G1                   | G1           | US-capture-snapshot       |
| FR06 | P0  | Live node read reaches the plugin; no stale-cache answer; observation window recorded                | AT02, AT04, AT17; benchmark N02          | E04      | 01-system-design.md (live read path)                                        | 01-mcp-envelope.md (observation block)                                         | G0, G1               | G1           | US-fresh-read             |
| FR07 | P0  | Tree pagination with projected fields; resumable cursor; explicit depth/bytes limits                 | AT07, AT13                               | E04      | 01-system-design.md (cursors)                                               | 02-tool-catalog.md, 06-resource-limits.md                                      | G1, G1/G3            | G1           | US-paginate-tree          |
| FR08 | P0  | Normalized implementation context surfaced wherever the API exposes it                               | AT10                                     | E05      | 01-system-design.md (normalizer)                                            | 04-normalized-model.md                                                         | G1/G2                | G1           | US-implementation-context |
| FR09 | P0  | Capability discovery with explicit unsupported; seat never inferred from names                       | AT10, AT12                               | E04      | 01-system-design.md (probes), adr/ (D09)                                    | 02-tool-catalog.md (capabilities), 05-error-catalog.md                         | G0/G2                | G0           | US-capability-check       |
| FR10 | P0  | Scoped style/variable reads; unresolved alias/mode reported                                          | AT10                                     | E05      | 01-system-design.md (tokens)                                                | 04-normalized-model.md, 02-tool-catalog.md (get_design_tokens)                 | G1/G2                | G1           | US-tokens-in-scope        |
| FR11 | P0  | Asset export PNG/SVG; opaque asset_id; size/bytes limits; grant-bound URIs                           | AT11                                     | E05      | 01-system-design.md (asset pipeline)                                        | 02-tool-catalog.md (export_assets), 06-resource-limits.md, 05-error-catalog.md | G1/G2                | G1           | US-export-asset           |
| FR12 | P0  | Freshness contract on every result: source, observed_at, cache_age_ms, coverage, consistency         | AT04, AT07, AT10                         | E02, E05 | 01-system-design.md (freshness metadata)                                    | 01-mcp-envelope.md, 05-error-catalog.md                                        | G1/G2                | G1           | US-freshness-labels       |
| FR13 | P0  | Scheduler and backpressure; one active editor op per session; BUSY carries retry_after_ms            | AT08, AT13                               | E06      | 01-system-design.md (scheduler)                                             | 06-resource-limits.md, 05-error-catalog.md                                     | G2, G1/G3            | G2           | US-queue-fairness         |
| FR14 | P0  | Deadline, cancellation, bounded retry (max 1, idempotent reads only)                                 | AT08, AT15                               | E06      | 01-system-design.md (cancellation)                                          | 06-resource-limits.md, 05-error-catalog.md                                     | G2, G1/G2            | G2           | US-cancel-read            |
| FR15 | P0  | Pause rejects new reads; revoke drops grant, clears cache and assets                                 | AT06, AT15                               | E03, E06 | 01-system-design.md (grant lifecycle)                                       | 03-bridge-protocol.md (revoke), 01-mcp-envelope.md                             | G1/G2                | G1           | US-pause-revoke           |
| FR16 | P0  | Disturbance-tolerant connection: heartbeat, offline, reconnect, new epoch, no blind replay           | AT09, AT15                               | E06      | 01-system-design.md (state machine)                                         | 03-bridge-protocol.md (heartbeat, epochs)                                      | G1/G2                | G1           | US-reconnect              |
| FR17 | P0  | MCP stdio + CLI; stdout carries protocol only; two-client handshake                                  | AT03, AT14                               | E07      | 01-system-design.md (adapter), adr/ (D08)                                   | 02-tool-catalog.md                                                             | G2                   | G2           | US-mcp-config             |
| FR18 | P0  | Local diagnostics: queue/cache/latency metrics, error IDs; no content or secrets                     | none; closes by review at G1             | E06      | 01-system-design.md (observability), docs/07-ops/03-runbook.md              | 05-error-catalog.md                                                            | G1 review            | G1           | US-diagnostics            |
| FR19 | P0  | No official MCP/REST on the core path; no adapter in core                                            | AT01, AT02                               | E01, E08 | 01-system-design.md (transport boundaries), threat model                    | none; absence enforced by egress audit and N09 checklist                       | G0/G2                | G0           | US-no-official-path       |
| FR20 | P1  | Watch with resync: gap detection, tombstones, stale cursor demands resync                            | AT19                                     | E09      | 01-system-design.md (watch), adr/ (D07)                                     | 02-tool-catalog.md (watch_scope, get_changes), 05-error-catalog.md             | G3                   | G3           | US-watch                  |
| FR21 | P1  | Versioned offline bundle; hash + manifest + schema; import cannot pose as live                       | AT20                                     | E09      | 01-system-design.md (bundle)                                                | bundle manifest schema under schemas/ at implementation                        | G3                   | G3           | US-offline-bundle         |
| FR22 | P1  | Multi-client fairness: per-client grants, fair queues, per-client limits                             | AT08 (soak extension), AT14              | E06      | 01-system-design.md (fair scheduling)                                       | 06-resource-limits.md (per-client caps)                                        | G2 soak, full at G3  | G2           | US-multi-client           |
| FR23 | P1  | Optional starter code separate from acquisition; no ephemeral asset URLs                             | none; closes by review and docs evidence | E10, E07 | 01-system-design.md (starter separation)                                    | none                                                                           | review; window G3/G4 | G3           | US-starter-code           |
| FR24 | P0  | No document mutation from reads/exports; selection and page untouched                                | AT17, AT18                               | E08      | threat model (read-only path)                                               | none                                                                           | G0/G2                | G0           | US-read-only-guarantee    |

Notes: FR02 and FR03 gain install-path coverage from AT03 in addition to their adversarial AT06
evidence, exactly as Appendix A assigns. FR06 carries benchmark evidence in N02 (fresh reaches the
plugin). FR18 and FR23 have no Appendix A row; their closure is a named review record filed in the
G1 and G3/G4 evidence slots respectively. FR22 closes P1 enforcement in the G3 window while soak
evidence already lands at G2.

## 3.1 Matrix (a.1): write path requirements (WR01..WR09)

These requirements come from `ADR-013` and the write path design, not from the PRD. They are
proposed: `ADR-013` is pending product approval, and no gate is assigned to the scenarios they
map to.

| ID   | Requirement (short)                                                                                                            | Source                | Acceptance tests | Design refs                            | Contract refs                                                                 | Status   |
| ---- | ------------------------------------------------------------------------------------------------------------------------------ | --------------------- | ---------------- | -------------------------------------- | ----------------------------------------------------------------------------- | -------- |
| WR01 | A write needs a write grant; a read grant never authorizes a mutation, and no tool parameter can widen a grant                 | ADR-013 rule 1        | AT21             | write path design section 4            | 02-access-control.md section 3.1; 05-error-catalog.md section 2.1             | proposed |
| WR02 | The agent submits declarative plans; no tool mutates the document directly                                                     | ADR-013 rule 2        | AT21, AT22       | write path design sections 2.1 and 2.2 | 02-tool-catalog.md sections 3.13 to 3.15                                      | proposed |
| WR03 | A preview resolves the plan against the live document without mutating it, and a blocked plan names its reason                 | ADR-013 rule 3        | AT22, AT26       | write path design section 2.3          | 02-tool-catalog.md section 3.13                                               | proposed |
| WR04 | A write needs a per plan user approval bound to the plan hash, the session, and the grant epoch, with an expiry                | ADR-013 rule 4        | AT23, AT24, AT28 | write path design section 2.4          | 02-access-control.md section 3.1; 05-error-catalog.md section 2.1             | proposed |
| WR05 | An approved plan is applied as one editor undo step, and the commit and the version checkpoint are reported                    | ADR-013 rule 5        | AT28, AT29       | write path design section 5            | 02-tool-catalog.md section 3.15; 01-threat-model.md section 4.1 (TH18)        | proposed |
| WR06 | Write budgets bound operations, created nodes, text length, property changes, subtree size, plan validity, and write rate      | ADR-013 rule 6        | AT25             | write path design section 3            | adr/ADR-008-resource-budgets.md amendment; 05-error-catalog.md section 2.1    | proposed |
| WR07 | A write stays inside the granted roots; a delete or an out-of-scope move needs the destructive flag and a separate user toggle | ADR-013 rules 3 and 6 | AT26, AT27       | write path design section 6            | 01-threat-model.md section 4.1 (TH14, TH15); 02-access-control.md section 3.1 | proposed |
| WR08 | Every applied plan leaves a content free audit record                                                                          | ADR-013 rule 7        | AT27, AT28       | write path design section 5            | 01-threat-model.md section 6; 05-error-catalog.md section 8                   | proposed |
| WR09 | No self invented revert: the editor undo and the version history are the recovery path                                         | ADR-013 rule 8        | AT29             | write path design section 5            | adr/ADR-013-bidirectional-write-path.md alternatives considered               | proposed |

## 4. Reverse acceptance index (AT01..AT20)

| ID   | Scenario (condensed)                                                                                   | FRs covered           | Problems | Gate  | Benchmark link                     |
| ---- | ------------------------------------------------------------------------------------------------------ | --------------------- | -------- | ----- | ---------------------------------- |
| AT01 | Plugin tools work without any official MCP connection; evidence path shows no official endpoints       | FR19                  | P01, P14 | G0/G2 | N09                                |
| AT02 | FIGMA_TOKEN unset; core still reads a node; egress blocked except loopback                             | FR06, FR19            | P02      | G0    | N09                                |
| AT03 | New user installs binary + plugin, pairs, grants, reads on two OS                                      | FR01, FR02, FR17      | P09      | G2    | N10                                |
| AT04 | Change text/layout; fresh reads reflect it; old snapshot stays; cached answers carry age               | FR05, FR06, FR12      | P03      | G1    | N01, N02                           |
| AT05 | Two sessions reuse the same node IDs; no data, cursor or asset exchange                                | FR03, FR04            | P06      | G1    | isolation only                     |
| AT06 | Wrong/expired/replayed token, wrong Host/Origin, out-of-scope node, revoke mid-transfer                | FR02, FR03, FR15      | P10      | G1/G2 | N07                                |
| AT07 | Large fixture exceeds bytes/depth; partial + cursor correct; no sibling/ancestor leak                  | FR07, FR12            | P04      | G1    | N01 limits, N02 cursor             |
| AT08 | Queue overload, slow request, memory limits, mixed soak, cancel                                        | FR13, FR14            | P05      | G2    | N04, N05, N06, N07                 |
| AT09 | Close/reopen plugin, sleep/wake, daemon restart, stale epoch frames rejected                           | FR16                  | P11      | G1/G2 | N08                                |
| AT10 | Normalized JSON matches curated fixture; mixed text, gradient, unresolved alias preserved              | FR08, FR09, FR10      | P08, P12 | G1/G2 | accuracy sample of N02             |
| AT11 | PNG/SVG match fixture; MIME/hash correct; oversize asset and active SVG handled                        | FR11                  | P07      | G1/G2 | N03                                |
| AT12 | Eligible Starter Design profile, plus a seat/permission context that rejects the plugin                | FR09                  | P13      | G0/G2 | eligibility probes only            |
| AT13 | Fuzz/property: recursion, numeric extremes, invalid IDs, unknown enums, malformed chunks, P1 zip bombs | FR07, FR13            | P04, P10 | G1/G3 | controls in plan section 1.3       |
| AT14 | MCP initialize/tools/resources/cancellation; clean stdout on two clients                               | FR17                  | none     | G2    | none                               |
| AT15 | Revoke wipes cache/assets and blocks retries; no job replay after reconnect                            | FR14, FR15, FR16      | P10, P11 | G1/G2 | N07                                |
| AT16 | README/website: no unlimited or all-features-free claims; measured limits included                     | none (goals G01, G02) | P14      | G2    | publication policy, plan section 4 |
| AT17 | Scope moved/deleted/renamed during read; fresh membership check; stale result rejected                 | FR03, FR06, FR24      | none     | G1    | N02 (fresh)                        |
| AT18 | Before/after fixture unchanged after reads/exports; selection/page untouched                           | FR24                  | none     | G0/G2 | hash check in plan section 1.2     |
| AT19 | Watch event gap, component/style dependency, journal overflow, resync                                  | FR20                  | none     | G3    | none (P1)                          |
| AT20 | Bundle hash/schema/size validation; offline source correct; no implicit live routing                   | FR21                  | none     | G3    | none (P1)                          |

Scenario text is condensed; the binding definitions live in docs/06-testing/02-acceptance-catalog.md.

### 4.1 Write path scenarios (AT21..AT29)

These are not PRD Appendix A rows. They cover the write path proposed by `ADR-013`, they carry no
gate, and `AT29` needs a real Figma Desktop session.

| ID   | Scenario (condensed)                                                                                         | WRs covered      | Gate                                      |
| ---- | ------------------------------------------------------------------------------------------------------------ | ---------------- | ----------------------------------------- |
| AT21 | A write on a read only session is refused with `WRITE_NOT_GRANTED`; the write mode is a separate user action | WR01, WR02       | not assigned                              |
| AT22 | A preview resolves the plan without changing the document; a blocked preview names its reason                | WR02, WR03       | not assigned                              |
| AT23 | An apply without a valid approval is refused; a grant change invalidates outstanding approvals               | WR04             | not assigned                              |
| AT24 | A changed plan body does not match the approved plan hash and is refused                                     | WR04             | not assigned                              |
| AT25 | A plan over any write budget is refused with the limit named, and nothing is applied                         | WR06             | not assigned                              |
| AT26 | A write outside the granted roots is blocked and refused on both sides                                       | WR03, WR07       | not assigned                              |
| AT27 | A delete without the destructive flag and the user toggle is refused                                         | WR07, WR08       | not assigned                              |
| AT28 | An approved apply reports its result and leaves one content free audit record                                | WR04, WR05, WR08 | not assigned                              |
| AT29 | The applied plan is one step in the editor undo history; no revert tool is exposed                           | WR05, WR09       | not assigned; needs a real editor session |

## 5. Error-code traceability (PRD 8.7)

Every structured error code promised by the PRD must be reachable by a test or a benchmark control;
none is decorative.

| Code                                   | Trigger (condensed)                            | Surfaced by                         | ATs               | Benchmark or protocol link              |
| -------------------------------------- | ---------------------------------------------- | ----------------------------------- | ----------------- | --------------------------------------- |
| AUTH_REQUIRED / AUTH_EXPIRED           | credential missing or expired                  | pairing and reconnect flows         | AT06              | N08 reconnect re-auth step              |
| PERMISSION_DENIED / OUT_OF_SCOPE       | platform permission or grant misses the node   | double scope enforcement            | AT06, AT17        | out-of-scope control, plan section 1.3  |
| PLUGIN_OFFLINE / SESSION_NOT_FOUND     | plugin closed, asleep, or session stale        | connection state machine            | AT09              | N08 detect scenario                     |
| PAUSED / REVOKED                       | user decision via UI or CLI                    | grant lifecycle                     | AT06, AT15        | N07 revoke timing                       |
| NODE_NOT_FOUND                         | node deleted or invalid ID within the session  | per-item batch errors               | AT07, AT17        | invalid-ID controls in plan section 1.3 |
| CAPABILITY_UNAVAILABLE                 | API, editor or seat does not support a feature | capability fields                   | AT10, AT12        | probe matrix, D09                       |
| PAYLOAD_TOO_LARGE / SCOPE_TOO_LARGE    | resource limit exceeded                        | projection and pagination guidance  | AT07, AT13        | N01/N02 adversarial controls            |
| BUSY                                   | bounded queue full                             | scheduler backpressure              | AT08              | N01 sustained rate, N05 soak            |
| TIMEOUT / CANCELLED                    | operation too slow or cancelled                | deadline and cancellation           | AT08              | N02 deadline, N07 cancel                |
| DOCUMENT_CHANGED                       | document change detected during read           | observation window metadata         | AT04, AT17        | N02 bounded retry (max 1)               |
| CURSOR_EXPIRED / RESYNC_REQUIRED       | scope/epoch changed; cursor or journal stale   | pagination and watch                | AT07, AT19        | N02 cursor continuation                 |
| UNSUPPORTED_SCHEMA / PROTOCOL_MISMATCH | incompatible client, plugin or daemon versions | doctor and handshake                | AT14              | version pin evidence, D08               |
| EXPORT_FAILED / FONT_UNAVAILABLE       | node, format or render unsupported             | fallback guidance, no node mutation | AT11              | N03 failure denominators                |
| INTERNAL_ERROR                         | unexpected defect                              | sanitized correlation ID            | classifier of N05 | N05 unexpected error rate < 0.1%        |

Write path codes, proposed by `ADR-013` and defined in
`docs/04-data-contracts/05-error-catalog.md` section 2.1. They are listed here so that no code is
decorative; the scenarios carry no gate because `ADR-013` is pending product approval.

| Code                    | Trigger (condensed)                                           | Surfaced by                     | ATs        | Benchmark or protocol link        |
| ----------------------- | ------------------------------------------------------------- | ------------------------------- | ---------- | --------------------------------- |
| WRITE_NOT_GRANTED       | the session holds a read grant only, or no grant              | write grant check in the daemon | AT21       | ADR-013 rule 1                    |
| APPROVAL_REQUIRED       | an apply arrives without a valid approval token               | approval verification           | AT23       | ADR-013 rule 4                    |
| APPROVAL_EXPIRED        | the approval expired or a grant change invalidated it         | approval verification           | AT23       | ADR-004 amendment, epoch rules    |
| PLAN_HASH_MISMATCH      | the plan differs from the approved plan                       | plan hash comparison            | AT24       | write path design section 2.4     |
| PLAN_INVALID            | the plan is invalid or names a closed contract violation      | plan validation in the daemon   | AT25       | write path design section 2.2     |
| BUDGET_EXCEEDED         | the plan exceeds a write budget                               | budget layer in the daemon      | AT25       | ADR-008 amendment                 |
| DESTRUCTIVE_NOT_ALLOWED | a delete or out-of-scope move without the flag and the toggle | destructive policy              | AT26, AT27 | ADR-013 rule 6                    |
| MUTATION_REFUSED        | the editor refused the mutation                               | the plugin                      | AT29       | write path design open question 1 |

## 6. Goal-to-evidence bridge (G01..G05) and assumption audit

Product goals may not float free of tests either.

| Goal | Statement (condensed)                                              | Evidence chain                                                                                                                     | Gate  |
| ---- | ------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------- | ----- |
| G01  | Zero product calls on the core path to official MCP/REST endpoints | FR19, AT01, AT02, N09 capture and review                                                                                           | G0/G2 |
| G02  | Intense local reads survive                                        | N01, N02 plus stability N04, N05; workload classes separated per plan section 1.2                                                  | G1/G2 |
| G03  | No agent decision on unlabeled stale data                          | FR12 envelope fields, AT04, AT10; class rules in plan section 1.2                                                                  | G1    |
| G04  | Installable without Rust/Node toolchain                            | FR01 artifacts, AT03 on two OS, pilot N10                                                                                          | G2    |
| G05  | Maintainable by the community                                      | reproducible lockfile build, published schemas (docs/04-data-contracts/), contribution flow; release review at G2 and continuously | G2+   |

Assumption audit (PRD section 3): documentation facts are design inputs re-verified at G0; the four
hypotheses must end as measured evidence, never as assumptions carried into release claims.

| Item                                                                                                   | Kind            | Closing evidence                                                                          |
| ------------------------------------------------------------------------------------------------------ | --------------- | ----------------------------------------------------------------------------------------- |
| Official MCP access is plan/seat gated [S1]                                                            | documented fact | design input; probed in AT12                                                              |
| exportAsync supports JSON_REST_V1 [S4]                                                                 | documented fact | reverified per Figma version under test; fixture diff gate feeding AT10                   |
| Plugin access is seat dependent; Starter runs Design plugins; user-started, no background service [S2] | documented fact | AT12 eligibility probes; N10 pilot behavior                                               |
| dynamic-page and manifest-controlled network access [S5]                                               | documented fact | manifest review matrix at G0 (D06, ADR-002)                                               |
| Global documentchange needs loadAllPagesAsync and has known non-delivery cases [S6]                    | documented fact | watch stays invalidation only; AT19 events at G3                                          |
| fileKey is not generally available to public plugins [S7]                                              | documented fact | session identity design verified by AT05                                                  |
| Plugin-API reads do not consume official MCP quota                                                     | hypothesis      | N09 path-separation proof on the exact commit                                             |
| Users accept keeping the plugin open during agent sessions                                             | hypothesis      | 30-minute session study plus pilot N10                                                    |
| 4 logical cores / 8 GB RAM windows the reference workloads                                             | hypothesis      | measured in N01..N04 at G1/G2; published profile adjusted only with honest re-measurement |
| Local loopback WebSocket from the plugin UI iframe works per OS                                        | hypothesis      | D06/ADR-002 OS test matrix at G0                                                          |

## 7. Matrix (b): N01..N10

| ID  | Target (verbatim intent)                                                                                                              | Measurement method                                                                    | Benchmark plan section                   | Gate                               | Fixture                                            | Report slot                                |
| --- | ------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- | ---------------------------------------- | ---------------------------------- | -------------------------------------------------- | ------------------------------------------ |
| N01 | Cached metadata response p95 <= 100 ms and <= 8 KiB across 10,000 reads; sustained >= 20 req/s                                        | 10,000 local cached reads; only cache hits count in this class                        | docs/06-testing/04-benchmark-plan.md 2.1 | G1 measured, republished at G2     | small (100 node) + medium (1,000 node)             | docs/07-ops/04-release-process.md slot N01 |
| N02 | Fresh read p95 <= 2 s for <= 100 simple nodes / <= 64 KiB; 1,000 sequential fresh reads within 60 min, every read reaching the plugin | sequential fresh workload against a real editor session                               | 04-benchmark-plan.md 2.2                 | G1 measured, republished at G2     | small (100 node)                                   | slot N02                                   |
| N03 | PNG 1024x1024 export p95 <= 5 s on the simple fixture                                                                                 | 100 exports with an isolated export queue; render cost logged                         | 04-benchmark-plan.md 2.3                 | G1 measured, republished at G2     | export fixture (image fills, gradients, SVG, text) | slot N03                                   |
| N04 | Daemon idle RSS <= 50 MiB; mixed-load peak RSS <= 256 MiB                                                                             | 1 s sampling across idle and soak phases                                              | 04-benchmark-plan.md 2.4                 | G1 (limits), G2 (soak peak)        | medium (1,000 node) corpus under workload W4       | slot N04                                   |
| N05 | No crash/OOM; unexpected errors < 0.1% on the valid workload                                                                          | 60 min mixed soak, 2 clients, 4 mock sessions, >= 1 real-editor session               | 04-benchmark-plan.md 2.5                 | G2                                 | medium corpus, all classes reported per class      | slot N05                                   |
| N06 | No editor freeze > 1 s from our traversal on the reference fixture                                                                    | manual interaction + profiling; editor internal export time reported separately       | 04-benchmark-plan.md 2.6                 | G1 measured, republished at G2     | reference (1,000 node) traversal fixture           | slot N06                                   |
| N07 | Logical cancel/revoke <= 1 s and no payload after revoke                                                                              | daemon + plugin timestamps for cancel and mid-transfer revoke                         | 04-benchmark-plan.md 2.7                 | G1 measured, republished at G2     | small fixture plus instrumented export             | slot N07                                   |
| N08 | Offline <= 15 s; reconnect readiness <= 10 s                                                                                          | sleep/wake, plugin kill, restart, port collision trials                               | 04-benchmark-plan.md 2.8                 | G1 measured, republished at G2     | none; small read as readiness probe                | slot N08                                   |
| N09 | Zero official MCP/REST from core                                                                                                      | source review + deny-egress mock + per-process connection capture on the exact commit | 04-benchmark-plan.md 2.9                 | G0, rerun per exact release commit | any; capture is fixture independent                | slot N09                                   |
| N10 | Onboarding first p90 <= 15 min; repeat <= 2 min                                                                                       | pilot with >= 10 users; per-step failure logging                                      | 04-benchmark-plan.md 2.10                | G2                                 | release bundle artifacts on Windows + macOS        | slot N10                                   |

## 8. Matrix (c): P01..P14

| ID  | Problem                                                      | Product response                                                                  | ATs                                    | Gate         |
| --- | ------------------------------------------------------------ | --------------------------------------------------------------------------------- | -------------------------------------- | ------------ |
| P01 | Agent sessions halt once the official MCP quota is exhausted | Direct plugin-API path; no proxy to the official MCP server                       | AT01                                   | G0/G2        |
| P02 | REST-based MCPs shift quota and add PAT credential burden    | No REST client, no PAT, no silent fallback in core                                | AT02                                   | G0           |
| P03 | Snapshots go stale without notice                            | Live mode re-reads on request; explicit freshness metadata                        | AT04                                   | G1           |
| P04 | Giant JSON floods agent context                              | Summaries by default, field projection, pagination, staged resources              | AT07, AT13                             | G1 (G3 fuzz) |
| P05 | Massive reads freeze the editor                              | Bounded queues, one active editor op per session, sliced traversal, cancellation  | AT08                                   | G2           |
| P06 | Many tabs cause wrong-file reads                             | Explicit session_id, grant bound to session, no auto tab selection                | AT05                                   | G1           |
| P07 | Images and effects are not representable in JSON             | PNG/SVG export via plugin; explicit unsupported reporting                         | AT11                                   | G1/G2        |
| P08 | Agents invent information that is actually unavailable       | Field availability, warnings, provenance, partial results                         | AT10                                   | G1/G2        |
| P09 | Plugin development setup is painful                          | CLI doctor, manifest generator, guided pairing, native binary                     | AT03                                   | G2           |
| P10 | Localhost is still exploitable by remote origin abuse        | Authentication, scope, Host/Origin validation, resource limits                    | AT06, AT15, AT13                       | G1/G2        |
| P11 | Plugin closed, sleep, or local network flake                 | State machine, heartbeat, offline status, reconnect without blind replay          | AT09, AT15                             | G1/G2        |
| P12 | Design-to-code output assumed production ready               | Accurate data + assets first; starter code optional later; no pixel-perfect claim | AT10; claim wording inside AT16 policy | G1/G2        |
| P13 | Seat/plan/permission variance changes behavior               | Runtime capability probes; support matrix from real tests                         | AT12                                   | G0/G2        |
| P14 | Unlimited claims cannot be proven                            | Claims limited to the tested plugin path; limits and benchmarks published         | AT16                                   | G2           |

## 9. Matrix (d): D01..D12

| ID  | Decision (short)                                                         | ADR association (docs/02-architecture/adr/)            | Evidence to close                                                                                                                      | Gate                                   |
| --- | ------------------------------------------------------------------------ | ------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------- |
| D01 | Rust core; TypeScript adapter required by the editor runtime             | ADR-001                                                | build and size verification of daemon, MCP adapter and plugin bundle on Windows and macOS; sizes in release evidence                   | G0                                     |
| D02 | Plugin path without REST/PAT/official MCP                                | none; PRD baseline decision, enforced by FR19          | AT01 and AT02 plus egress audit at the exact commit                                                                                    | G0, recheck G2                         |
| D03 | Read-only v0.1/v0.2; no eval or arbitrary execution                      | none; PRD decision                                     | handler audit plus AT18 before/after fixture hash; `ADR-013` would restate `FR24` for a future release and is pending product approval | G0/G2                                  |
| D13 | Write path: plan based, previewed, approved, budgeted, audited, undoable | ADR-013 (Proposed)                                     | AT21..AT29 once the product decision is made; the editor undo check needs a real Figma Desktop session                                 | not assigned; pending product approval |
| D04 | Live fresh default; cached only by explicit request                      | none; fixed by the envelope contract                   | AT04 fresh/snapshot/cached behavior; class split rules in 04-benchmark-plan.md section 1.2                                             | G1                                     |
| D05 | Session identity independent of fileKey                                  | none; PRD decision                                     | AT05 two-session fixture; public probes of identifyable session fields                                                                 | G1                                     |
| D06 | Local WebSocket transport candidate                                      | ADR-002                                                | CSP/Origin/manifest test per OS inside real Figma Desktop; on failure a new ADR, never a public relay fallback                         | G0                                     |
| D07 | Watch page-scoped if any; global watch never the default                 | ADR filed only if promoted                             | nodechange/style event coverage test; on failure watch stays request-based only                                                        | G3                                     |
| D08 | rmcp version, MSRV, client support                                       | ADR-003                                                | pinned version plus compatibility evidence with >= 2 MCP clients (AT14)                                                                | pin at G0 kickoff; evidence at G2      |
| D09 | Variables/library resolution per seat                                    | none; capability matrix docs                           | probe matrix per seat; unavailable stays a valid answer                                                                                | G0/G2                                  |
| D10 | Final name and dual license                                              | ADR-009                                                | name/marker check and dependency license review recorded by the maintainer                                                             | G2 wording; G4 window                  |
| D11 | Community plugin publication                                             | none; production release review                        | production manifest review and Community policy review                                                                                 | G4                                     |
| D12 | Performance beyond the initial profile not promised                      | none; governed by docs/06-testing/04-benchmark-plan.md | any raise needs a reproducible rerun and a labeled publication                                                                         | G2 baseline, then per claim            |

## 10. Coverage audit rules

| Check            | Expected state                                                                                                                                                                      |
| ---------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Row counts       | FR 24 rows (P0 = 20, P1 = FR20, FR21, FR22, FR23), AT 20 PRD rows plus 9 write path rows (AT21..AT29), N 10 rows, P 14 rows, D 12 PRD rows plus D13 (proposed write path decision). |
| FR closure       | every FR row names at least one AT, or a named review (FR18, FR23), soak extension (FR22), or benchmark (FR06 via N02).                                                             |
| FR completeness  | every FR row has at least one epic and one design reference; new engineering questions take new IDs only.                                                                           |
| AT reverse index | the FR lists in section 4 are identical to PRD Appendix A; drift is a spec change, not an edit here.                                                                                |
| N linkage        | every N row points to a protocol section in docs/06-testing/04-benchmark-plan.md and to one evidence slot in docs/07-ops/04-release-process.md.                                     |
| P linkage        | every P row maps to at least one AT; P12 additionally refers into the AT16 wording policy.                                                                                          |
| D linkage        | every D row has closure evidence and a gate; D06, D08, D09 stay open until G0 evidence exists.                                                                                      |
| Write path rows  | every WR row names at least one AT, a design ref, and a contract ref; the write scenarios carry no gate until the product decision is recorded.                                     |
| Error map        | every PRD 8.7 error code appears in section 5 with at least one AT or benchmark linkage.                                                                                            |
| Orphan rule      | an FR without an AT, design ref and backlog link may not enter a sprint (docs/00-overview/01-prd-analysis.md section 11).                                                           |

## 11. Maintenance rules

| Rule           | Detail                                                                                                                   |
| -------------- | ------------------------------------------------------------------------------------------------------------------------ |
| Same PR        | contract or AT changes update this file in the same PR (docs/README.md change management).                               |
| No renumbering | a new requirement gets a new ID; superseded IDs retire and are never reused.                                             |
| Gate drift     | moving a test between gates is a PRD-level change, never a doc-local edit.                                               |
| Results rule   | filling measured values happens only in the release evidence slots; targets in this file are never rewritten as results. |
| ADR slugs      | ADR numbers are normative; their actual file names resolve through the docs/02-architecture/adr/ index.                  |

Revision history: v0.1, 2026-10-20, initial four matrices proposed together with
docs/06-testing/02-acceptance-catalog.md and docs/06-testing/04-benchmark-plan.md. v0.2, 2026-10-20,
write path extension: section 3.1 adds the write path requirements `WR01..WR09` from `ADR-013`,
section 4.1 adds the scenarios `AT21..AT29`, section 5 adds the write error codes, matrix (d) adds
the proposed decision `D13`; the PRD `FR`, `N`, `P`, and `D` rows are unchanged and the write
scenarios carry no gate while `ADR-013` is pending product approval.
