# FigTether Agile SDLC Process

> **Design reference.** Original requirements and approval labels below are historical design
> records, not a feature-availability list. See [current documentation status](../documentation-status.md)
> for implemented reads/writes, observed desktop evidence, and remaining gaps.

Status: `Proposed`

Purpose. This document defines how FigTether is built under an Agile SDLC mapped onto the PRD
gates `G0` to `G4` (PRD 8.10) for the capacity the PRD assumes: two engineers plus part-time QA
(PRD 8.10). It fixes the process model, roles and approvals, ceremonies, gates as hard quality
gates, DoR and DoD, CI quality gates, estimation, change management, risk review, and docs
governance. The PRD (`FGT-PRD-001`) is the source of truth; binding interpretations live in
`docs/00-overview/01-prd-analysis.md`, and this set never silently contradicts PRD IDs. Gate `G0`
has not been run, so every duration here is a relative estimate after kickoff, not a date
commitment.

## 1. Process model: hybrid Scrum plus Kanban

A pure Scrum service is a poor fit for a 2-person team whose work is structured by gates
(`G0`..`G4`, PRD 8.10) and dominated by high-severity technical risk (R01..R14, PRD 8.9).
Scrum provides the planning cadence, review discipline, and the estimation discipline that the
PRD 12 to 18 engineer-week envelope (PRD 8.10) needs to be validated against actual velocity. Kanban provides WIP limits, a pull
queue, and an explicit interrupt buffer, because gate work (G0 spikes, G2 security/soak streams,
security fixes) arrives at unpredictable granularity (R01/R02/R09). The hybrid:

| Element                                   | Source   | How it is applied here                                                                                                                    |
| ----------------------------------------- | -------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| Sprint cadence, 2 weeks                   | Scrum    | All epics E01..E10 flow through sprints; sprint review merges with gate review when a gate closes                                         |
| Backlog and prioritization                | Scrum    | `docs/01-agile/02-product-backlog.md`; P0 mandatory for v0.1, P1 for v0.2, P2 exploration (PRD 4.2)                                       |
| Pull flow, WIP limits                     | Kanban   | WIP = 1 gate-critical item per engineer; +/- for small support items; no feature WIP above 3 team-wide                                    |
| Interrupt buffer                          | Kanban   | 20 percent of each sprint reserved for bugs, security fixes, and editor or platform drift (R01, R04; freeze evidence tracked against N06) |
| Gate reviews instead of pure sprint goals | PRD 8.10 | A sprint that closes G0/G1/G2/G3 ends with the gate decision, not only a demo                                                             |
| Estimation in SP                          | Scrum    | Fibonacci SP; velocity unknown until the first 2 sprints (section 8)                                                                      |

## 2. Roles mapped to PRD Appendix D approval roles

PRD Appendix D.2 defines five approval roles, all currently "Pending review"; the PRD header says
the responsible people are named at kickoff. Until kickoff, roles stay named by function only.

| Role (PRD D.2)     | Assumed holder                                    | Approval duty (PRD context)                                               | Ceremony duty                          | Notes                                          |
| ------------------ | ------------------------------------------------- | ------------------------------------------------------------------------- | -------------------------------------- | ---------------------------------------------- |
| Product maintainer | Project maintainer (named at kickoff)             | Scope, goals, priorities, success definitions; claim language (AT16, R07) | Chairs kickoff, planning, gate reviews | Owns D10 scope; rejects unlimited claims (P14) |
| Rust/core owner    | Engineer 1 (proposed split)                       | Contracts, scheduler, isolation, resource budget                          | Daily sync owner for core stream       | Owns R03/R10/R12 mitigation decisions          |
| Plugin owner       | Engineer 2 (proposed split)                       | Eligibility, API coverage, runtime, manifest (D06 probes)                 | Daily sync owner for adapter stream    | Owns R01/R04/R08                               |
| Security reviewer  | QA lead or external maintainer (named at kickoff) | Pairing, grant, trust model, revoke (FR02/03/15)                          | Gate security sign-offs G1/G2          | Blocks release on R05 findings                 |
| QA/release owner   | Part-time QA plus core owner                      | Acceptance suite, benchmarks, OS/client matrix, artifacts                 | Runs gate evidence packs               | Owns AT01..AT20 run books                      |

Both engineers share all engineering roles in practice; the split above only fixes review routing.
CODEOWNERS is added only after each person explicitly accepts the role (PRD 8.11 ownership row).

## 3. Ceremonies

| Ceremony               | Cadence                         | Timebox                   | Owner                          | Output                                                                                   |
| ---------------------- | ------------------------------- | ------------------------- | ------------------------------ | ---------------------------------------------------------------------------------------- |
| Kickoff                | Once, day 1                     | 2 h                       | Product maintainer             | Named owners per PRD D.2, gate calendar, D08 pin decision opened, kickoff ADRs           |
| Sprint planning        | Every 2 weeks                   | 60 min                    | Product maintainer + engineers | Sprint backlog of DoR-passed items with estimates                                        |
| Daily sync             | Daily                           | 15 min asynchronous       | Engineers (rotating writer)    | Blockers moved to interrupt buffer; risk triggers flagged (PRD 8.9)                      |
| Backlog refinement     | Weekly                          | 45 min                    | Product maintainer             | Estimated, DoR-ready items; spike results posted (SPK table in backlog)                  |
| Gate review            | When a gate-closing sprint ends | 90 min                    | QA/release owner               | Go, restrict-support, or fail decision per PRD 8.10; evidence pack (Appendix C template) |
| Sprint review and demo | End of each non-gate sprint     | 30 min                    | Engineers                      | Working increment against its `AT*` anchors                                              |
| Retrospective          | End of each sprint              | 30 min                    | All                            | Max 3 action items with owners                                                           |
| Risk review            | Weekly, 30 min                  | 1 h total per gate window | All roles                      | Updated `docs/01-agile/04-risk-register.md`                                              |
| Security review        | Per gate plus ad hoc            | As needed                 | Security reviewer              | Sign-off line in gate evidence                                                           |

Ceremony trim rule: with 3 people total, no ceremony is kept if it produces no artifact listed in
the Output column; the gate review is never trimmed.

## 4. Gates G0..G4 as hard quality gates

Exit criteria and failure decisions are taken from PRD 8.10 verbatim in condensed form. Gates are
hard: work does not advance past a failed gate and no later-stage implementation starts before
G0 core risks are closed (PRD 8.10, closing rule).

| Gate                 | Phase estimate (PRD 8.10) | Deliverables                                                                                | Hard exit criteria                                                                                                | Decision if failed                                                                                        |
| -------------------- | ------------------------- | ------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| G0 feasibility spike | 1-2 weeks                 | Pairing WebSocket, Starter read, JSON and PNG export, permission probes, Rust MCP handshake | AT01, AT02, AT12, AT18 pass on real desktop Windows plus macOS                                                    | If one OS fails: restrict support to the passing OS. If the Starter path fails: reposition before MVP     |
| G1 alpha             | 3-4 weeks                 | Scoped live and snapshot, model, pagination, cache, cancellation, diagnostics               | All P0 requirements functional; integration and negative tests                                                    | Alpha stays developer-only with published limitations; fix, then re-review                                |
| G2 public v0.1       | 2-3 weeks                 | Binary and bundle, doctor, docs, security and resource tests, benchmark, pilot              | AT P0 pass with two clients; NFRs recorded; zero critical leak/crash/misrouting bugs; artifacts from exact commit | Pilot decision rule per PRD 2.3; revise capacity profile and claims, never drop failed tasks from reports |
| G3 v0.2              | 3-4 weeks after G2        | Watch or resync, offline bundle, multi-client scheduling                                    | AT19, AT20 pass; honest watch coverage; schema migration done                                                     | Features are not published as stable (PRD 8.10)                                                           |
| G4 v0.3+             | After need is validated   | Starter templates, package managers, Community submission                                   | ADR plus user evidence (ADR-009 for D10 scope)                                                                    | Community acceptance is never assumed on schedule (D11)                                                   |

Gate entry requires the gate evidence template fields of PRD Appendix C: commit, artifact hash,
OS and editor versions, fixture hashes, test ids, outcomes, log links, reviewer (Appendix C).

## 5. Definition of Ready (PRD 8.10.1)

An item enters a sprint only when all of the following hold:

| #     | Ready condition                                                                                                                                                                     | PRD anchor         |
| ----- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------ |
| DoR-1 | Owner, acceptance test, scope state, and example fixtures exist                                                                                                                     | PRD 8.10.1         |
| DoR-2 | Decisions on schema, grants, credential storage, and resource limits are approved through a maintainer ADR (`docs/02-architecture/adr/`, ADR-001..ADR-003 reserved for D01/D06/D08) | PRD 8.10.1         |
| DoR-3 | SDK dependencies pinned to tested versions; target MCP clients and release platforms fixed                                                                                          | PRD 8.10.1, D08    |
| DoR-4 | Unproven features labeled experimental or unsupported; never stated as marketing fact                                                                                               | PRD 8.10.1, AT16   |
| DoR-5 | Estimated in SP and traceability row prepared in `docs/06-testing/03-traceability-matrix.md`                                                                                        | PRD 8.8 discipline |

## 6. Definition of Done v0.1 (PRD 8.10.2)

| #    | Done condition                                                                                                 | Evidence rest                               |
| ---- | -------------------------------------------------------------------------------------------------------------- | ------------------------------------------- |
| DD-1 | All P0 FR and P0 AT pass on the exact release candidate                                                        | `docs/06-testing/03-traceability-matrix.md` |
| DD-2 | G0 proved the Starter path with no PAT and no official MCP or REST on core                                     | AT01/AT02 egress pack                       |
| DD-3 | Cached, fresh, and export benchmarks separated and published with configuration                                | `docs/06-testing/04-benchmark-plan.md`      |
| DD-4 | Revoke, scope isolation, stale epoch, parser limits, and permissions checked                                   | AT06, AT07, AT15, AT16, AT17                |
| DD-5 | Rust and TypeScript build, format, lint, tests, dependency audit, SBOM, checksums, provenance complete         | CI gates CI-01..CI-13 below                 |
| DD-6 | Chosen-OS binary and plugin bundle tested from distribution artifacts, not source tree                         | AT03, FR01                                  |
| DD-7 | README, quickstart, troubleshooting, support matrix, privacy, security reporting, changelog, licenses complete | `docs/07-ops/` set; AT16 check              |
| DD-8 | Known limitations recorded; zero P0 blockers; rollback or revert available                                     | `docs/07-ops/04-release-process.md`         |

## 7. CI quality gates

Commands run on every PR unless scoped to release stages. Anchors: PRD 8.11 CI and supply chain
rows, AT13 fuzz, AT16 docs, FR01/AT03 exact-commit artifacts.

| ID    | Gate                                                                 | Stage                               | PRD anchor                      |
| ----- | -------------------------------------------------------------------- | ----------------------------------- | ------------------------------- |
| CI-01 | `cargo fmt --check`                                                  | every PR                            | PRD 8.11 CI                     |
| CI-02 | `cargo clippy -D warnings` over `crates/`                            | every PR                            | PRD 8.11 CI                     |
| CI-03 | `cargo test` unit and integration                                    | every PR                            | PRD 8.8                         |
| CI-04 | Cross-language schema contract tests (`schemas/` versus Rust and TS) | every PR after E02 freeze           | PRD 8.11 schema contracts, FR12 |
| CI-05 | TypeScript typecheck strict (`plugins/figma/`)                       | every PR                            | PRD 8.11 TS typecheck           |
| CI-06 | TypeScript lint                                                      | every PR                            | PRD 8.11 TS lint                |
| CI-07 | TypeScript test                                                      | every PR                            | PRD 8.11 TS test                |
| CI-08 | Fuzz corpus and parser property tests                                | after G1                            | AT13                            |
| CI-09 | Dependency audit (cargo plus frontend lockfiles)                     | advisory on PR, blocking on release | PRD 8.11 audit                  |
| CI-10 | SBOM generation                                                      | release builds                      | PRD 8.11 supply chain           |
| CI-11 | Provenance, checksums, and exact-commit artifact checks              | release builds                      | PRD 8.11, DD-5                  |
| CI-12 | Install smoke from distribution artifacts (binary plus plugin)       | release milestones                  | FR01, AT03                      |
| CI-13 | Docs prettier check (`npm run format:check`), no em dash scan        | every PR                            | PRD 8.12, D.1                   |

## 8. Estimation

| Parameter           | Rule                                                                                                                                                              | Anchor and caveat                                                                                 |
| ------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| Sprint length       | 2 weeks default; gate-closing sprint may shorten to 1-2 weeks once exit criteria are demonstrably met (keeps G0+G1+G2 within the PRD 6-9 week band)               | PRD 8.10 gate estimates are calendar-anchored; sprints are trimmed, never the acceptance evidence |
| Scale               | Fibonacci 1, 2, 3, 5, 8, 13                                                                                                                                       | Items above 13 are split at refinement                                                            |
| Velocity            | Unknown until the first 2 sprints. Until then sprint scope is planned from PRD gate durations (G0 1-2 wks, G1 3-4 wks, G2 2-3 wks), not from a fictional velocity | PRD 8.10; recalibrate in the Sprint 2 retrospective                                               |
| Provisional mapping | 1 SP is sized about 1 engineer-day for budget comparison only                                                                                                     | SP stays team-relative; the mapping is a reporting convention, not a commitment                   |
| Capacity            | 2 engineers x 10 days = 20 SP per sprint before part-time QA support                                                                                              | PRD 8.10 assumes QA part-time; gate ceremonies consume part of capacity                           |
| Contingency         | 20-30 percent on gate durations, driven by R01, R02, R09, R12; consumed on the interrupt buffer first                                                             | PRD 8.9 risk table                                                                                |
| v0.1 envelope       | 12-18 engineer-weeks for G0+G1+G2 with two engineers                                                                                                              | PRD 8.10; overflow response: cut P1/P2 scope, never cut P0 ATs (PRD 2.3)                          |
| Re-estimation       | Only at gate reviews or DoR failures; mid-sprint scope growth re-estimates before entry                                                                           | Hybrid Scrum rule                                                                                 |

## 9. Change management and claim policy

1. ADR first. Contract-affecting change: open an ADR in `docs/02-architecture/adr/`, get sign-off
   from the owning PRD D.2 role, then update contracts and the traceability matrix in the same
   change set (PRD 8.10.1; this is also DoR rule DoR-2 of section 5).
2. PRD precedence. Spec-affecting change: update the PRD first, then propagate here; this doc set
   never contradicts the PRD silently. Contradictions found in review are fixed or reverted within
   one sprint (OR6 in `docs/01-agile/04-risk-register.md`).
3. Docs-only change: direct change set with CI-13 passing.
4. Claim policy (AT16, P14, D12, G01/G02): marketing and README language may only claim that the
   core path does not consume official MCP or REST quota, with measured boundary tables and exact
   commits (D12). Forbidden: unlimited capacity phrasing, all-features phrasing, silent fallback
   to official endpoints, unmeasured performance numbers. Gate G2 owns the AT16 review; the
   release evidence template (Appendix C) is the required claim format.

## 10. Risk review cadence

Weekly 30 minute risk review inside refinement; full re-score of
`docs/01-agile/04-risk-register.md` at every gate review using the P/I 1-3 scale defined there.
Any entry scoring 6 or higher becomes a named gate risk with a contingency check inserted into
the next gate plan, and its mitigation items enter the sprint backlog. New risks get the next
OR id with owner and trigger. Security and feasibility blockers are never closed by
documentation alone (PRD 8.9 closing note).

## 11. Docs governance

- Formatting: Prettier via `npm run format` and `npm run format:check`; no em dashes anywhere
  (PRD 8.12, appendix D.1); ASCII hyphens only.
- ADRs live in `docs/02-architecture/adr/` with sequential ids; ADR-001 (D01 core split),
  ADR-002 (D06 transport), ADR-003 (D08 rmcp), ADR-009 (D10 name and license) are reserved.
- Every doc starts with a `# Title`, a `Status: Proposed` line, and a purpose paragraph; status
  values `Proposed`, `Approved`, `Implemented-in-PRD-only` are tracked in
  `docs/99-appendices/03-approval-record.md`.
- Cross references use repo-relative `docs/...` and code paths such as `crates/daemon/`,
  `plugins/figma/main/`, `schemas/`, `fixtures/`, `benchmarks/` (PRD 8.4 workspace sketch).

## 12. Related documents

- `docs/01-agile/02-product-backlog.md`: epics, stories, item tables, spikes.
- `docs/01-agile/03-roadmap-sprints.md`: Sprint 0..7 sequence and gate mapping.
- `docs/01-agile/04-risk-register.md`: R01..R14 plus OR1..OR6 register.
- `docs/00-overview/01-prd-analysis.md`: committed interpretations (fresh-first, fail-closed,
  bounded everything) that both the DoD and claim policy rely on.

## 13. Open questions

1. Confirmation of the role split in section 2 and acceptance of CODEOWNERS entries is pending
   kickoff (PRD header, D.2 all Pending review).
2. Whether review gate G1 security sign-off needs the security reviewer in the loop continuously
   (highest role load) is re-evaluated in the Sprint 1 retrospective.
3. Velocity recalibration after the first 2 sprints may change the SP totals in
   `docs/01-agile/02-product-backlog.md`; this policy doc takes precedence over stale numbers.
