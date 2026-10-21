# FigTether Sprint Roadmap

> **Design reference.** Original requirements and approval labels below are historical design
> records, not a feature-availability list. See [current documentation status](../documentation-status.md)
> for implemented reads/writes, observed desktop evidence, and remaining gaps.

Status: `Proposed`

Purpose. This document sequences the backlog from `docs/01-agile/02-product-backlog.md` into
Sprint 0 through Sprint 7 and maps each sprint to the PRD gates `G0`, `G1`, `G2` with exit
criteria from PRD 8.10. Sprints 5 to 7 cover the `G3` window (v0.2, PRD 8.10) and the first `G4`
evidence gathering; `G4` itself is not scheduled. Estimation, DoR, and gate rules live in
`docs/01-agile/01-sdlc-process.md`.

## 1. Relativity rule (binding)

Gate `G0` has not been run. All week numbers below count **from the kickoff date**, never from
calendar dates; no sprint has a committed date. The only committed structure is the gate order
and the exit criteria chain E01 -> E02 -> E03 -> E04 -> E05/E06 -> E07 -> E08 (PRD 4.2 critical
path). Two rules bend the timeline without breaking it:

| Rule             | Effect                                                                                                                                     |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| Sprint trim rule | A gate-closing sprint may shorten to 1-2 weeks when its gate exit criteria are demonstrably met; the full sprint is kept when they are not |
| Carryover rule   | Items not finished at sprint end are re-estimated and re-prioritized at planning; they never slip silently into the next sprint backlog    |

With the gates G0 (1-2 wks), G1 (3-4 wks), G2 (2-3 wks) from PRD 8.10, v0.1 lands in 6-9 calendar
weeks after kickoff with two engineers plus part-time QA (PRD 8.10).

## 2. Sprint sequence overview

| Sprint | Weeks after kickoff                | Gate window       | Goal                                                                                                    | Epics touched                 |
| ------ | ---------------------------------- | ----------------- | ------------------------------------------------------------------------------------------------------- | ----------------------------- |
| S0     | 1-2                                | G0                | Prove or kill feasibility; no advanced implementation starts before G0 core risks close (PRD 8.10 rule) | E01, E02 sketch, E08 scaffold |
| S1     | 3-4                                | G1 start          | Freeze contracts; pairing and grants core                                                               | E02, E03                      |
| S2     | 5-6                                | G1 close          | Alpha: all P0 functionality running behind security and limits                                          | E03, E04, E05, E06 start      |
| S3     | 7-8                                | G2 start          | Full acquisition and assets; reliability hardening; CLI surface                                         | E04, E05, E06, E07 start      |
| S4     | 9-10 (trim to 9 in the fast case)  | G2 close          | Pilot and release (public v0.1)                                                                         | E06, E07, E08                 |
| S5     | 11-12                              | G3 start          | v0.2 feature stream: watch with resync                                                                  | E09                           |
| S6     | 13-14                              | G3                | v0.2 feature stream: offline bundles, multi-client                                                      | E09                           |
| S7     | 15-16 (trim possible on gate exit) | G3 close, G4 prep | G3 gate proof and G4 evidence gathering                                                                 | E09, E10                      |

Trim rule note: leftover days of a shortened S0 go to contract sketching only; E03 and all later
implementation stay forbidden until G0 core risks are closed (PRD 8.10 closing rule).

## 3. Sprint details

### Sprint 0: feasibility (G0)

| Field              | Content                                                                                                                                                                                                                                                                                                                                                                                                                       |
| ------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Goal               | Convert PRD hypotheses 1, 3, 4 (PRD 3) and open points (analysis 9) into recorded evidence; prove or kill the plugin path on real desktop                                                                                                                                                                                                                                                                                     |
| Key items          | Repo and CI scaffold (CI-01..CI-05, CI-13 green); docs set skeleton with ADR stubs ADR-001..ADR-003; fixture scaffold: small 100-node fixture plus before/after harness (`AT18`); SPK-01 transport results; SPK-02 rmcp results; SPK-03 variable probe matrix; Starter read plus permission refusal probes (`AT12`); `exportAsync` JSON and PNG proof (FR06, FR11 minimal); egress evidence pack (`AT01`, `AT02`, N09 method) |
| Exit, mapped to G0 | `AT01`, `AT02`, `AT12`, `AT18` pass on real desktop Windows plus macOS; evidence fields recorded per PRD Appendix C. Failure decision from PRD 8.10: one OS failing restricts support to the tested OS; a failing Starter path repositions the product before MVP                                                                                                                                                             |
| Not started        | E03 and later implementation; any packaging beyond dev builds (PRD 8.10 closing rule)                                                                                                                                                                                                                                                                                                                                         |

### Sprint 1: contracts and pairing (G1 start)

| Field              | Content                                                                                                                                                                                                                                                                   |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Goal               | E02 done and frozen; E03 core running; SPK-04 timer and drift results                                                                                                                                                                                                     |
| Key items          | Tool surface and envelope schemas v1; error taxonomy; ID model; cursor and projection contract; freshness and cap types; cross-language harness green (CI-04); pairing service (FR02); grant registry with epochs (FR03); fixture license review for medium fixture (OR2) |
| Exit, mapped to G1 | Contracts published and pinned; pairing and grant core demonstrable locally; DoR rule DoR-2 satisfied for E03/E04 items through ADR-001..ADR-003 signatures                                                                                                               |

### Sprint 2: alpha (G1 close)

| Field              | Content                                                                                                                                                                                                                                                                             |
| ------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Goal               | All P0 requirements are functionally working: scoped live and snapshot reads, pagination, cancellation, diagnostics                                                                                                                                                                 |
| Key items          | Double enforcement (FR03, `AT17`); restricted `list_sessions` (FR04, `AT05`); pause and revoke flows (FR15); live read pipeline (FR06); snapshot capture (FR05); leaked tree walker with pagination (FR07, `AT07`); normalizer skeleton start (E05); scheduler skeleton start (E06) |
| Exit, mapped to G1 | Gate review G1: all P0 functional; integration and negative tests green; alpha is developer-only with limitations (PRD 8.10). Security reviewer sign-off on pairing, grants, revoke paths (FR02, FR03, FR15, `AT06` partial)                                                        |

### Sprint 3: full acquisition, assets, reliability (G2 start)

| Field              | Content                                                                                                                                                                                                                                                                                                              |
| ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Goal               | E04 complete with capability probes; E05 normalizer full; E06 scheduler, reconnect, diagnostics; E07 CLI starts                                                                                                                                                                                                      |
| Key items          | Capability probes (FR09, `AT12`); freshness on every result (FR12, `AT04`); text, layout, paints, styles and vars (FR08, FR10); asset store plus PNG/SVG bounds (FR11, `AT11`); deadline and retry (FR14); heartbeat and reconnect (FR16, `AT09`); `figtether config` and `figtether doctor` first pass (FR17, FR18) |
| Exit, mapped to G2 | E05/E06 items with their AT anchors passing locally; medium fixture read-only check (`AT18`) stable; CI-08 fuzz corpus switched on (`AT13` basis)                                                                                                                                                                    |

### Sprint 4: verification and release (G2 close, trim eligible)

| Field              | Content                                                                                                                                                                                                                                                                                                                                                                                                                    |
| ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Goal               | Public v0.1 gated by evidence; pilot run; artifacts from the exact release commit                                                                                                                                                                                                                                                                                                                                          |
| Key items          | Full security suite (`AT06`, `AT13`); soak (two clients, four mock sessions, one real editor session per N05); benchmarks N01-N03 with cached, fresh, export separated (PRD 2.1); N04-N07 recorded; bundle plus `AT03` install check from artifacts; pilot protocol with at least 10 users (N10); AT16 claim review of README and site; provenance, SBOM, checksums (CI-10, CI-11); release docs and support matrix (DD-7) |
| Exit, mapped to G2 | PRD 8.10 G2 exit: P0 AT pass with two clients; NFR targets recorded with configurations; zero critical data leak, crash, or misrouting bugs; artifacts from the exact release commit. DD-1..DD-8 from `docs/01-agile/01-sdlc-process.md` section 6 apply, plus the pilot decision rule of PRD 2.3                                                                                                                          |

### Sprint 5: v0.2 start (G3 window)

| Field              | Content                                                                                                                                                                                                                                      |
| ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Goal               | Scoped watch with resync as an invalidation source, never a truth source (D07)                                                                                                                                                               |
| Key items          | Gap detection and resync trigger; change journal with tombstones; page-scoped coverage test to decide whether the watch ships or stays request-based (D07, analysis 8.7); `AT19` test cases designed; schema migration plan for v0.2 changes |
| Exit, mapped to G3 | `AT19` cases drafted and reviewable; the D07 condition decided on nodechange evidence, not preference                                                                                                                                        |

### Sprint 6: v0.2 bundles and fairness (G3)

| Field              | Content                                                                                                                                                                                                                                                         |
| ------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Goal               | Offline bundles and multi-client scheduling on the stabilized core (E09 priority entry condition, PRD 8.10)                                                                                                                                                     |
| Key items          | Bundle export and import with hash, manifest, schema version, and bounded decompression (FR21, `AT20`, `AT13`); offline labels with no implicit live routing (fail-closed rule 10.2); multi-client fairness and per-client limits (FR22; `AT08` soak extension) |
| Exit, mapped to G3 | `AT20` passing on both OS targets; soak extension green with two clients on one session                                                                                                                                                                         |

### Sprint 7: G3 proof and G4 evidence (G3 close, trim eligible)

| Field                      | Content                                                                                                                                                                                                                                                                                      |
| -------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Goal                       | Gate proof for v0.2; collect the user evidence `G4` requires; no Community assumptions (D11)                                                                                                                                                                                                 |
| Key items                  | Full `AT19`/`AT20` runs with honest watch coverage documentation; schema migration executed and tested; E10 items: package manager need evidence, starter templates draft, publication manifest prep (E10 deliverables, PRD 8.10 G4 row); registry or name checks recorded for `D10`/ADR-009 |
| Exit, mapped to G3 then G4 | G3 per PRD 8.10: `AT19`, `AT20` pass; coverage is honest; migration done; a failing G3 keeps v0.2 features unpublished as stable. G4 entry work only proceeds when ADR plus user evidence exist; Community acceptance is never scheduled (PRD 8.10 G4)                                       |

## 4. Parallelization rules (PRD 4.2)

Parallel work needs frozen contracts (E02) plus approved ADRs, per PRD 4.2: activities may be
worked concurrently after contracts are agreed. Final packaging is never parallel to E06
hardening (E07 dependency row).

| Stream                                             | Owner (proposed) | Window | Start condition                                               |
| -------------------------------------------------- | ---------------- | ------ | ------------------------------------------------------------- |
| Core: E03 enforcement, E04 pipeline, E06 scheduler | Engineer 1       | S1-S3  | E02 schemas frozen, ADR-003 pinned                            |
| Adapter: E04 walker and slicing, E05 normalizer    | Engineer 2       | S2-S3  | Same contracts plus D06 transport evidence                    |
| Shared: E07 packaging and E08 evidence             | Both             | S3-S4  | E06 near-complete; QA part-time joins soak and benchmark runs |
| Evidence: SPK-03/04 probes and fixture work        | Engineer 2       | S0-S1  | Fixture scaffold from S0                                      |

## 5. Gate exit criteria and evidence register

Each gate closes only with the PRD Appendix C evidence fields (commit, artifact hash, editor
version, fixture hashes, test ids, outcomes, reviewer). Evidence slots are reserved in
`docs/07-ops/04-release-process.md` and every covered `AT*` gets its traceability row in
`docs/06-testing/03-traceability-matrix.md`.

| Gate | Exit criteria ids (PRD 8.10, Appendix A)                                               | Evidence artifacts                                                                               | Failure decision (PRD 8.10)                                                        |
| ---- | -------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------- |
| G0   | AT01, AT02, AT12, AT18 on Windows plus macOS desktop                                   | Egress capture pack, export proof, permission probe matrix, pairing transcript                   | One OS failing restricts support; Starter path failing repositions before MVP      |
| G1   | All P0 FR functional; AT04..AT10, AT17 functional and negative suites                  | Integration and negative test reports, security reviewer sign-off                                | Alpha stays developer-only; limitations published; re-review after fixes           |
| G2   | AT03, AT06..AT08, AT10, AT11, AT14, AT16, AT18 plus N01..N08 recorded with two clients | Benchmark tables (cached, fresh, export separated), pilot report, artifact and provenance checks | Pilot decision rule of PRD 2.3; revise profile and claims, never drop failed tasks |
| G3   | AT19, AT20 plus AT08 multi-client extension                                            | Watch coverage document, schema migration report, soak extension log                             | v0.2 features are not published as stable                                          |
| G4   | Need evidence plus an approved ADR (ADR-009 for D10)                                   | User evidence digest, manifest review record                                                     | Community acceptance is never assumed on schedule (D11)                            |

Decision owners: product maintainer plus QA/release owner decide; security reviewer holds a
blocking vote at G1 and G2 on R05 and R10 findings.

## 6. Dependency chain and slip handling

| Chain position         | Depends on                                                              | What feeds the next stage                                  | If upstream slips                                                                     |
| ---------------------- | ----------------------------------------------------------------------- | ---------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| E01 -> E02             | Transport and export evidence shapes schema decisions (D06, FR06, FR11) | Recorded probe outcomes become contract types              | Contracts sketch from hypotheses only where PRD 8.5 already fixes a shape; rest waits |
| E02 -> E03             | Cross-language schemas and ID rules (FR03, FR04)                        | Pairing, grants, epochs implemented against frozen schemas | E03 starts with registry internals that are contract-independent; UI waits            |
| E03 -> E04             | Enforcement and session identity                                        | Reads routed through grants                                | Walker development continues against mocks, but never merges ahead of enforcement     |
| E04 -> E05/E06         | Live and snapshot pipeline, probes                                      | Normalizer input shapes and scheduler load profile         | E06 scheduler skeleton may start on mocked queues; E05 waits for real payloads        |
| E06 -> E07 (packaging) | Limits, cancellation, diagnostics stable                                | Bundle contents and doctor output                          | Final packaging is serialized after E06 per the E07 dependency row (PRD 4.2)          |
| E05/E07 -> E08         | Tool surface plus assets complete                                       | Fixtures run against real outputs                          | Verification items re-prioritized; pilot never runs before P0 ATs pass                |
| G2 -> E09              | Public v0.1 stable (PRD 8.10)                                           | Core ready for feature streams                             | All G3 sprints shift by the fix window length                                         |

## 7. Sprint capacity and interrupt buffer

Planned scope per sprint comes from the epic item tables in `docs/01-agile/02-product-backlog.md`;
effective capacity is 2 engineers with the 20 percent interrupt buffer (R01/R04 handling), and
part-time QA joins for evidence-heavy sprints.

| Sprint | Planned SP | Effective capacity | Buffer handling                                                                  |
| ------ | ---------- | ------------------ | -------------------------------------------------------------------------------- |
| S0     | 14         | 16                 | Plenty of space for spike reruns; D06/D08 evidence may consume overtime          |
| S1     | 18         | 16                 | 2 SP deficit absorbed by the QA ramp or by moving fixture license work to S3     |
| S2     | 20         | 16                 | 4 SP deficit: allowed once, covered by the 30 percent contingency on G1 duration |
| S3     | 22         | 18 (QA joins)      | Deficit covered by contingency; B-tier polish items flagged as trim candidates   |
| S4     | 18         | 18 (QA full)       | Balanced; pilot and AT16 review must not be trimmed                              |

| Implicit rule       | Detail                                                                                                                                 |
| ------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| Overflow response   | Cut P1 and P2 scope, then B-tier P0 items; never cut P0 ATs (PRD 2.3)                                                                  |
| Recalibration       | Sprint 2 retrospective sets real velocity; capacity rows above are then replaced with measured values                                  |
| Gate trim interplay | The sprint trim rule shortens announced weeks only when exit criteria are already met, so capacity surprises cannot fake an early gate |

## 8. What is deliberately not scheduled

| Item                                                                     | Reason                                                                                                                                                           |
| ------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| G4 date and Community publication (D11)                                  | Requires user evidence, an approved ADR, and external review of distribution policy before submission (PRD 8.10 G4 row, PRD 8.11 release and platform rows; D11) |
| Performance numbers as targets in reports                                | Only measured values with configuration and exact commit are published (D12, P14)                                                                                |
| Calendar dates                                                           | The PRD gives relative estimates only; kickoff anchors all weeks (PRD 8.10)                                                                                      |
| FR23 starter code, FR20 watch variants beyond the E09 set, E10 templates | P1 and P2 scope is elected at the G2 gate review based on evidence, never by default (PRD 4.2 priority note)                                                     |

## 9. Open questions

1. Whether Sprint 5 starts while the pilot debrief from Sprint 4 is still open depends on the G2
   decision; if G2 fails, all G3 sprints shift by the length of the fix window.
2. QA part-time start date: PRD 8.10 assumes QA during verification-heavy gates; kickoff decides
   whether QA joins at S3 or S4.
