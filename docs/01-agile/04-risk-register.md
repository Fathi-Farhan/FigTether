# FigTether Risk Register

Status: `Proposed`

Purpose. This document maintains the live risk register for FigTether v0.1 and v0.2 delivery. It
carries the full PRD 8.9 risk table `R01`..`R14` in condensed English, adds operational risks
`OR1`..`OR6` discovered while planning docs/01-agile/ work, defines the scoring scale, ranks the
top five interventions, and fixes the review cadence from `docs/01-agile/01-sdlc-process.md`
section 10. Each entry maps to its mitigation items in `docs/01-agile/02-product-backlog.md`.
Gate `G0` has not been run, so probability and impact values are planning claims to re-score at
every gate review.

## 1. Scoring scale

Probability (P) and impact (I) are each scored 1-3; score = P times I.

| Level    | Probability                           | Impact                                                     |
| -------- | ------------------------------------- | ---------------------------------------------------------- |
| 1 low    | Under 15 percent chance in this phase | Minor: workaround exists, no gate effect                   |
| 2 medium | About 15-45 percent                   | Significant: gate delay or scope reduction                 |
| 3 high   | Over 45 percent                       | Critical: gate fail, release blocked, or security incident |

| Score band | Class    | Handling rule                                                       |
| ---------- | -------- | ------------------------------------------------------------------- |
| 1-2        | Low      | Monitor at weekly risk review                                       |
| 3-4        | Medium   | Mitigation tracked; owner reports monthly at minimum                |
| 6          | High     | Named gate risk: contingency check inserted into the next gate plan |
| 9          | Critical | Release or gate blocker; daily attention until score drops          |

PRD 8.9 vocabulary mapping: "sedang" maps to P2/I2, "tinggi" maps to P3/I3, and "kritis" maps to
I3 with a critical classification and release-blocking handling.

## 2. PRD risk table R01..R14 (PRD 8.9)

| ID  | Risk                                                             | Cause                                                                                                   | P   | I   | Score | Mitigation and trigger                                                                                                                                                                      | Owner                    | Fallback                                                              |
| --- | ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- | --- | --- | ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------ | --------------------------------------------------------------------- |
| R01 | Figma changes API, eligibility, or restrictions                  | Platform evolves independently of this product; official limits are plan and seat based (S1, S2)        | 2   | 3   | 6     | Periodic smoke tests plus pre-release runs; pinned typings; runtime feature probes (FR09). Trigger: editor version bump breaking smoke tests or a probe with missing coverage               | Plugin owner             | Patch the adapter and revise the support matrix                       |
| R02 | Localhost WebSocket and CSP behavior differences across runtimes | Plugin UI iframe connection to daemon crosses CSP and manifest rules (S5)                               | 2   | 3   | 6     | G0 matrix per OS, exact manifest, Origin and port diagnosis (D06). Trigger: failure recorded in the OS matrix for any supported OS                                                          | Transport (core owner)   | Alternate transport ADR (ADR-002) with no default cloud relay         |
| R03 | Live data goes stale from incomplete events                      | `documentchange` needs full page loading and has non-delivery cases (S6); events are not a truth source | 3   | 3   | 9     | Fresh first: `consistency=fresh` is the only live default; cached is opt-in (D04); watch is invalidation only (D07). Trigger: any event-based shortcut proposal, or `N02` shows stale reads | Core owner               | Request-based reads; drop the watch feature (E09 shrinks)             |
| R04 | Editor freezes during export or traversal                        | Scene traversal and export run on the plugin runtime                                                    | 2   | 3   | 6     | Bounded scope, lazy traversal, serial export, adaptive batching (E04/E05). Trigger: freeze over 1 s on the reference fixture (`N06`)                                                        | Plugin owner             | Refuse large scope; ask for a narrower root                           |
| R05 | Scope leakage or session misrouting                              | Wrong tab reads, stale cursors, or bridge bugs (P06, P10)                                               | 2   | 3   | 6     | Double enforcement with epochs, isolation tests, no auto tab select (FR03, FR04). Trigger: any isolation test failure or adversarial probe finding                                          | Security reviewer        | Block release and revoke all sessions                                 |
| R06 | Community rejects the distribution                               | Submission policy may refuse the development model                                                      | 2   | 2   | 4     | Start with the development plugin distribution; manifest and policy review before submission (PRD 8.11, D11). Trigger: policy friction surfaced during pre-submission review                | Release maintainer       | Continue development-only distribution                                |
| R07 | Unlimited claims mislead users                                   | Marketing pressure; unfalsifiable claims are easy to copy from other tools (P14)                        | 3   | 3   | 9     | Bounded claim language, benchmark tables with commits, `AT16` review at G2 (D12). Trigger: any unlimited or all-features phrasing found in README, site, or release notes                   | Product maintainer       | Correct documents before release                                      |
| R08 | Variable and component library features unavailable              | Seat or plan context blocks Variables API coverage (S10, D09)                                           | 2   | 2   | 4     | Per-API capability and coverage reporting; unresolved refs surfaced (FR10, SPK-03). Trigger: probe matrix gap on a supported seat                                                           | Plugin owner             | Targeted export or user level additional permissions                  |
| R09 | MCP client and SDK fragmentation                                 | Client capability negotiation differs; SDK churn (S8)                                                   | 2   | 2   | 4     | Protocol negotiation, tools plus stdio baseline, compatibility tests against two clients (FR17, `AT14`, D08). Trigger: a new client fails the handshake suite                               | MCP (core owner)         | Deactivate optional features on clients that cannot support them      |
| R10 | Local credentials attacked by a same-user process                | Local trust model is never absolute; credentials sit in the runtime directory                           | 2   | 3   | 6     | ACL, expiry, least scope, documented trust model (FR02, FR03). Trigger: any new local attack surface report                                                                                 | Core owner               | Rotate and revoke; never claim resistance to local malware            |
| R11 | Few contributors, high bus factor                                | 2 engineer core team; release knowledge concentrated                                                    | 2   | 3   | 6     | Maintainer guide, modular crates, issue templates, shared release knowledge (PRD 8.11 contribution row). Trigger: any release step that only one person can perform                         | Project lead             | Reduce scope and cadence                                              |
| R12 | Performance targets fail on small devices                        | Reference profile 4 cores and 8 GB is an assumption, not a measurement (PRD 3)                          | 2   | 2   | 4     | Measure at G0 and G2; budget profiling; define a tested minimum (`N01`..`N07`). Trigger: repeated misses on the reference profile                                                           | Performance (core owner) | Lower the default profile with the measured evidence (D12)            |
| R13 | Private data lands in issues or logs                             | Users paste diagnostics; unsanitized tool output (PRD 8.9 R13; FR18 privacy guard)                      | 2   | 3   | 6     | Sanitized doctor output, no content logging by default, public fixture reproduction (FR18, US-05). Trigger: any issue containing design content or tokens                                   | Community maintainer     | Remove the disclosure and rotate the exposed credential               |
| R14 | Inconsistent data during collaborative edits                     | Multiplayer edits change values mid-observation (PRD 8.3 mixed editing)                                 | 3   | 2   | 6     | Best-effort metadata, bounded retries, snapshot guidance (FR12, D04). Trigger: mid-edit mismatch reports during pilot or soak                                                               | Core owner               | Never promise atomic transactions; document the observation semantics |

Risks that cannot be fully eliminated remain external: platform change (R01), file access rights
(P13 chain), editor capacity (R04), and the user's own model AI path (PRD 8.9 closing note).
Security and feasibility blockers are never closed by documentation alone (PRD 8.9).

## 3. New operational risks OR1..OR6

New risks discovered while planning the docs set; owners are the PRD D.2 roles. New entries take
the next OR id after OR6.

| ID  | Risk                                                    | Cause                                                                                                                                   | P   | I   | Score | Mitigation and trigger                                                                                                                                                                                                                                           | Owner              | Fallback                                                                                         |
| --- | ------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- | --- | --- | ----- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------ | ------------------------------------------------------------------------------------------------ |
| OR1 | Toolchain drift breaks builds or contract tests         | rmcp main-branch churn (S8), Rust stable releases, Node LTS updates; D08 pin not yet done                                               | 2   | 2   | 4     | Lockfiles covering all languages, MSRV check in CI, pin decision at G0 (SPK-02), toolchain envelope in CI-01..CI-04. Trigger: any red CI run after a toolchain update that is not caused by code change                                                          | Core owner         | Pin the last known-good toolchain set; reopen ADR-003                                            |
| OR2 | Fixture licensing gaps block release evidence           | PRD 2.1 requires clearly licensed and non-client datasets; missing attribution breaks E08 and G2 evidence (PRD 8.11 repository hygiene) | 2   | 3   | 6     | License column in `fixtures/` manifest, provenance review at Sprint 1, synthetic fixtures where license documentation is absent. Trigger: any fixture without a recorded license or origin                                                                       | Release maintainer | Regenerate the fixture synthetically; drop the dataset from the corpus                           |
| OR3 | Allocator RSS exceeds `N04` budget                      | Daemon idle bounds (50 MiB) and mixed-load peak (256 MiB) are targets, not results; allocator behavior is platform dependent            | 2   | 2   | 4     | Allocator experiment documented per `N04` method, per-crate budgets, peak sampling in soak (E06). Trigger: peak RSS over 256 MiB in soak or idle over 50 MiB                                                                                                     | Core owner         | Change the default allocator with published evidence; revise the capacity profile honestly (D12) |
| OR4 | macOS runner unavailability delays OS matrix            | Windows-first team and machine park; macOS runners or editor environment may be unavailable in a sprint window                          | 2   | 2   | 4     | OS matrix scheduled in Sprint 0 (G0 requires Windows plus macOS, PRD 8.10), early booking, one OS primary in CI. Trigger: two consecutive missed macOS evidence windows                                                                                          | Release maintainer | PRD 8.10 G0 rule: restrict support to the tested OS and defer macOS claims                       |
| OR5 | AI model or API cost shifts alter the evaluation budget | Comparative evaluation and pilot quality checks run on user model paths (PRD 2.2, 2.3); tier prices can change                          | 1   | 2   | 2     | Cap evaluation spend per gate, reuse recorded sessions and the fuzz corpus, disclose protocol status per PRD 2.2 step 6. Trigger: pricing change that breaks the evaluation budget                                                                               | Product maintainer | Shrink the evaluation matrix and document which comparison rows were cut                         |
| OR6 | Roadmap docs drift from the PRD                         | Fourteen parallel doc writers and later contributors restate or reinterpret PRD content                                                 | 2   | 2   | 4     | PRD-first change rule (`docs/01-agile/01-sdlc-process.md` section 9), committed interpretations in `docs/00-overview/01-prd-analysis.md`, gate review checks every doc claim against PRD ids. Trigger: any contradiction found in review or by CI claim scanning | Product maintainer | Revert the drifting change; move the real requirement into the PRD, then re-propagate            |

## 4. Top five interventions (ranked)

Ranked by score band, release-blocking power, and cost of mitigation.

| Rank | Risk                     | Primary intervention                                                                                                 | Backlog and gate anchors                                                                                      |
| ---- | ------------------------ | -------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| 1    | R03 (9)                  | Fresh first with no event-based truth source; observation windows in every read                                      | E02 freshness types, E04 pipeline item, `AT04`, enforced at G1 on the G1 gate plan; D04/D07 locked as decided |
| 2    | R07 (9)                  | Claim policy with `AT16` language review as a G2 gate step; benchmark tables mandatory before any published claim    | E08 release docs item, CI-13; owner product maintainer                                                        |
| 3    | R05 (6, critical impact) | Double enforcement plus isolation and adversarial test set as a release blocker                                      | E03 items, `AT05`, `AT06`, `AT17`; security reviewer sign-off required at G1 and G2                           |
| 4    | R01 (6)                  | Platform drift control: pinned typings, version probes, smoke suite in CI, support matrix updates per editor version | E01 probes, CI-02 environment; support matrix owned by plugin owner                                           |
| 5    | R10 (6)                  | Credential hygiene: single-use secret, fast expiry, least scope, published trust model, revoke paths exercised       | E03 pairing items, `AT06` replay and expiry checks; documented in `docs/05-security/` threat model            |

R02 and R11 remain on the watch list: their mitigations are structural (ADR-002 transport
evidence, maintainer guide) and require no additional backlog item beyond what E01 and E08
already carry.

## 5. Risk to gate coverage matrix

Every gate review re-scores the full register; this matrix fixes which gate must prove which
mitigation, so no risk can pass silently between gates.

| Risk cluster (ids)                    | Proven at | Proving anchors                                                                                    | If unproven at that gate                                                                     |
| ------------------------------------- | --------- | -------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| Feasibility drift: R01, R02, OR1, OR4 | G0        | Egress pack, OS matrix, export proof (AT01, AT02, AT12), ADR-002 and ADR-003 evidence              | Gate decision: restrict OS or SDK support per PRD 8.10; no later-stage implementation starts |
| Data truth: R03, R14                  | G1        | AT04 freshness contracts, AT10 curated-fixture comparison, N02 fresh workload spot check           | Live mode stays request-based only; watch work (E09) is re-scoped                            |
| Access control: R05, R10              | G1 and G2 | AT05, AT06, AT15, AT17 isolation suites plus security reviewer blocking vote                       | Release is blocked; sessions revoked; re-review required                                     |
| Platform safety: R04, OR3             | G1 and G2 | N04 and N06 profiling, AT07 bounds, AT08 overload suite                                            | Scope bounds tightened; capacity profile revised with evidence (D12)                         |
| Claim hygiene: R07, OR6               | G2        | AT16 language review, PRD consistency checks in the gate docs review                               | Release candidate is not tagged; documents corrected first                                   |
| Client compatibility: R09             | G2        | AT14 on two clients                                                                                | Unsupported client features deactivated (R09 fallback)                                       |
| Report hygiene: R13, OR2              | G2        | Sanitized diagnostics walkthrough (US-05), issue template check, license manifest audit (AT16 set) | Release candidate is not tagged; documents corrected first                                   |
| Longevity: R06, R08, R11, R12, OR5    | G3 and G4 | Support matrix updates, capability matrix refresh, maintainer guide, budgeted evaluation reports   | v0.3 scope re-elected at the G4 review with evidence                                         |
| v0.2 features: FR20 to FR22 set       | G3        | AT19, AT20, AT08 soak extension                                                                    | Features stay developer-only (PRD 8.10 G3 exit rule)                                         |

## 6. Trigger checklist

Concrete detections the weekly risk review runs first; each maps one row above to an observable
signal instead of a feeling.

| Trigger check                                       | Detects                 | Procedure                                                                                                      |
| --------------------------------------------------- | ----------------------- | -------------------------------------------------------------------------------------------------------------- |
| Smoke suite result against the pinned Figma version | R01 editor or API drift | Run the Editor smoke script on the current pinned version; compare against the last recorded baseline          |
| OS matrix status board                              | R02, OR4                | Check that the latest Windows and macOS loopback captures date from the current release window                 |
| Freshness spot audit                                | R03, R14                | Sample ten fresh reads and verify `source`, `observed_at`, and `coverage` fields against contract types (FR12) |
| Isolation suite result                              | R05                     | Run AT05 and AT17 suites; any failure escalates immediately to the security reviewer                           |
| Freeze and RSS samples                              | R04, OR3                | Review the latest soak profile for freezes over 1 s (N06) and peak RSS against N04 bounds                      |
| Claim scan of README and site text                  | R07                     | Re-run the claim language list and verify benchmark tables still carry exact commits (AT16, D12)               |
| Diagnostics leak scan                               | R13                     | Sample `figtether doctor` output and recent issue templates for design content or token artifacts              |
| Toolchain and lockfile diff                         | OR1                     | Compare lockfile changes and rmcp commits since the last green CI run (D08 pin)                                |
| Fixture license audit                               | OR2                     | Verify every new fixture carries origin, license, and hash before entering the corpus (PRD 2.1)                |
| Evaluation budget ledger                            | OR5                     | Compare spend against the per-gate cap; disclose any protocol step cut per PRD 2.2                             |

## 7. Watchlist

Risks proceeding without extra backlog items because their mitigations are structural; they are
re-scored weekly and promoted to section 4 interventions if a trigger fires.

| Risk                             | Watch level  | Why no extra item is added now                                                      |
| -------------------------------- | ------------ | ----------------------------------------------------------------------------------- |
| R02 transport variance           | High watch   | ADR-002 evidence is already the Sprint 0 deliverable (SPK-01); fallback exists      |
| R06 community rejection          | Medium watch | Development-distribution path is the default pre-G2 (D11 remains out of v0.1 scope) |
| R11 bus factor                   | Medium watch | Maintainer guide and modular crates are standing E08/E10 deliverables (PRD 8.11)    |
| R12 performance on small devices | Medium watch | Measurement is built into G0/G2 evidence; profile changes are evidence-driven (D12) |
| OR5 evaluation costs             | Low watch    | Capped per gate; pilot still runs under the PRD 2.3 decision rule                   |

## 8. Risk intake template (OR7 and beyond)

| Field                  | Entry rule                                                                                                                                    |
| ---------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| id and statement       | Next OR id; one sentence risk statement plus root cause                                                                                       |
| p and i                | Scored on the section 1 scale with a one-line justification                                                                                   |
| Mitigation and trigger | At least one mitigation mapped to a backlog item in `docs/01-agile/02-product-backlog.md` and one observable trigger from the section 6 style |
| Owner and fallback     | A PRD D.2 role plus an operational fallback that does not close the risk on documentation alone (PRD 8.9 closing note)                        |
| Gate mapping           | Which gate proves the mitigation (section 5 style)                                                                                            |

## 9. Review cadence and escalation

| Activity           | Cadence                                        | Owner                         | Output                                                                                         |
| ------------------ | ---------------------------------------------- | ----------------------------- | ---------------------------------------------------------------------------------------------- |
| Weekly risk review | Weekly, inside refinement                      | All roles; core owner records | Updated scores, new OR ids, trigger flags                                                      |
| Gate re-score      | Every gate review (G0..G3)                     | QA/release owner              | Score snapshot appended to the gate evidence pack                                              |
| Escalation         | Any score 6 or higher, or any critical class   | Product maintainer            | Contingency check added to the next gate plan; mitigation items pulled into the sprint backlog |
| Closure            | Mitigation verified by its AT anchor in a gate | Gate review                   | Risk re-scored to Medium or lower; fallback exercised before the gate if the mitigation fails  |

## 10. Open questions

1. R13 quantification: whether the 0.1 percent unexpected error budget (`N05`) doubles as the
   drift metric for privacy leaks in logs needs a definition in `docs/06-testing/`.
2. OR4 fallback trade-off: restricting to Windows for G0 would force re-testing macOS claims at
   G2; kickoff decides whether macOS belongs to the G0 evidence window at any cost (PRD 8.10 G0
   row requires both OSs).
3. PRD 8.9 shows no explicit owner assignment beyond role names; the kickoff meeting must bind
   role names to people or merge roles (for example security reviewer into QA) before G1.
