# FigTether Test Strategy

> **Design reference.** Original requirements and approval labels below are historical design
> records, not a feature-availability list. See [current documentation status](../documentation-status.md)
> for implemented reads/writes, observed desktop evidence, and remaining gaps.

Status: `Proposed`

Purpose. This document defines how FigTether gets validated: the test pyramid, environment matrix,
fixture plan, evidence rules, ownership, and CI stages that make acceptance tests `AT01..AT20`
runnable and auditable for gates `G0` through `G3`. It implements PRD sections 2.1, 2.2, 2.3, 8.8,
8.10, and 8.11 and is the peer of `docs/06-testing/02-acceptance-catalog.md` (one section per `AT`)
and `docs/06-testing/04-benchmark-plan.md` (procedures for `N01..N10`, only referenced). Every
performance number is a target until produced as evidence under section 5. Code paths
(`crates/{cli,daemon,mcp,core,protocol,model,assets}`, `plugins/figma/{main,ui}`, `schemas/`,
`fixtures/`) follow `docs/02-architecture/01-system-design.md` so each scenario binds to a location.

## 1. Scope and guiding principles

1. Division of labor. This file owns organization and evidence acceptance. The step-by-step scenario
   catalog is `docs/06-testing/02-acceptance-catalog.md`; benchmark procedures for `N01..N10` are
   `docs/06-testing/04-benchmark-plan.md` (only referenced, never duplicated here); the `FR` to `AT`
   links live in `docs/06-testing/03-traceability-matrix.md`.
2. Separation of quantitative buckets. Every metric distinguishes cached reads, fresh reads,
   snapshot reads, and asset exports, and splits queue wait, transport, plugin, normalization, and
   rendering time (PRD 2.1; PRD 2.2 step 3). A cached hit count is never counted as proof of live
   read capacity.
3. No silent substitution. No scenario falls back to another transport, source, or endpoint without an
   explicit ADR and a re-run of `AT01` and `AT02` (PRD 3, 7.1; fail closed per
   `docs/00-overview/01-prd-analysis.md` section 10).
4. Negative and blocked results are retained. Tasks recorded as `blocked`, `timeout`, `unsupported`,
   or `cancelled` stay in the gate report (PRD 2.2 step 6); failed tasks are never removed to satisfy
   a target (PRD 2.3).
5. Evidence hierarchy. Runtime proof outranks source review; automation never replaces the real
   editor desktop protocol; source inspection never substitutes for runtime proof (PRD 8.8).
6. Unmeasured is unclaimed. No benchmark number is claimed until the run exists under the rules of
   this file and `docs/06-testing/04-benchmark-plan.md` (PRD 1.1; PRD 2.1).

## 2. Test pyramid

Fast deterministic layers run on every PR; slower environment-heavy layers need Figma Desktop and
run per gate. Lower layers never substitute for higher layers.

| Layer | Name                                         | What runs and where                                                                                                                                                                        | Tooling                                                                           | Supports                                                                          | First mandatory at                                                        |
| ----- | -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------- | --------------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| L1    | Rust unit tests                              | Pure modules in `crates/core`, `crates/protocol`, `crates/model`, `crates/assets`: envelope shape, cache keys and TTL, limit tables, normalization rules, error mapping                    | `cargo test`; inline tables mirror `docs/04-data-contracts/06-resource-limits.md` | FR08, FR12, FR13, low level correctness                                           | every PR from E02 on                                                      |
| L2    | Rust integration, in-memory transports       | daemon + MCP + CLI wired to in-memory transport fakes (channel based bridge, stdio pipe) in `crates/daemon/tests`; triggers epochs, deadlines, chunk drops, queue saturation, revoke paths | `cargo test --test`; deterministic runtime injected into `crates/core`            | FR07, FR13, FR14, FR15, FR16 logic without sockets                                | every PR from E02 on                                                      |
| L3    | TS unit tests                                | `plugins/figma/main` handlers and `plugins/figma/ui` flows against typed Figma API stubs; validate postMessage filtering, parser codecs, projection pruning, batching                      | `vitest` or equivalent TS runner from lockfile                                    | FR08, FR11, FR24 plugin side                                                      | every PR                                                                  |
| L4    | Cross-language schema round-trip Rust and TS | `schemas/*.json` compiled to Rust types (`crates/protocol`, `crates/model`) and TS types (`plugins/figma`); canonical JSON byte comparison both directions over fixture corpus             | `schemars`, `serde_json`, TS typegen, scripts under `benchmarks/scripts/`         | FR07, FR12 contract stability, protocol versioning                                | every PR touching `schemas/`                                              |
| L5    | Property and fuzz tests                      | Property tests for scope membership, cursor binding, alias graph cycles, epoch advance; fuzz targets for bridge frame parser, DTO decoder, offline bundle parser (P1 bundle)               | `proptest`; `cargo-fuzz` corpora seeded from `fixtures/fuzz/` (AT13)              | FR07, FR13 parser robustness; supports security posture                           | subset on PR; full nightly; G1 first evidence, G3 re-run for bundle parts |
| L6    | Negative and adversarial suite               | Scripted misuse: wrong or expired or replayed pairing secret, wrong Origin or Host, out-of-scope node, revoke mid transfer, stale epoch frames, oversize payloads                          | harness on L2 plus `crates/mcp` client fakes                                      | FR02, FR03, FR14, FR15 (AT06)                                                     | G1 first evidence; re-verified at each later gate                         |
| L7    | Real editor desktop manual protocol          | Scripted sessions in Figma Desktop per environment matrix below: install, pair, grant, read, edit, export, revoke, disturbance; evidence per section 5 style                               | manual click-through; sanitized captures; timings log                             | FR01, FR02, FR03, FR04, FR06, FR09 to FR12, FR15, FR16, FR24 editor facing claims | G0 for the feasibility subset; per gate thereafter                        |
| L8    | Mixed soak                                   | 60 minutes, 2 clients, 4 mock sessions, at least one real editor session; RSS sampled every 1 s; queue depth, cache, cancel and revoke tracked                                             | soak driver; diagnostics from FR18                                                | FR13, FR14, FR18; N04, N05 (AT08 soak)                                            | G2                                                                        |
| L9    | Benchmark runs                               | `N01..N10` procedures exactly as written in `docs/06-testing/04-benchmark-plan.md`; cached, fresh, snapshot and export buckets always separate                                             | benchmark harness from that plan                                                  | N01 to N09 evidence for gate reports                                              | G0 subset (N09), full pass per gate                                       |
| L10   | G2 pilot                                     | At least 10 real users onboard on their own machines with the released binary and bundle; every step timed; failures logged per step; repeat run after 1 week                              | pilot kit: artifact + `fixtures/` + task script                                   | N10 (onboarding p90 <= 15 min; repeat <= 2 min; PRD 2.3 pilot rule)               | G2                                                                        |

Rules without exception:

1. L7 gates editor facing claims. A claim about Figma Desktop behavior without an L7 capture for the
   exact commit and Figma version is void (PRD 8.8).
2. In-memory transports in L2 are the only sanctioned way to rehearse scheduler, epoch, and
   backpressure behavior in CI; loopback smoke tests are separate and never editor proof.
3. L9 procedures are owned by `docs/06-testing/04-benchmark-plan.md`; this file only fixes where they
   run (L9 row above) and the evidence fields they emit (section 5).
4. L10 pilot users sign a data and time consent form; identities never appear in evidence; published
   pilot data is aggregate only (PRD 2.3, PRD 8.6).

## 3. Environment matrix

Reference profile for L8 and L9 runs (PRD 2.1): minimum 4 logical cores, 8 GB RAM, daemon built in
release mode, Figma Desktop version recorded, plugin in normal runtime without Developer VM, local
non-private fixtures. Reference hardware is listed per run in evidence; CI runners are not that
hardware and never stand in for it.

| Environment                 | Editor under test      | Claims allowed                                   | Exported where                 | Notes                                                                                    |
| --------------------------- | ---------------------- | ------------------------------------------------ | ------------------------------ | ---------------------------------------------------------------------------------------- |
| Windows 10/11 x64           | Figma Desktop          | full editor-facing claims                        | L7, L8, L9, L10                | main G0 desktop target; PRD 8.10 G0 exit requires a pass here                            |
| macOS arm64 (Apple silicon) | Figma Desktop          | full editor-facing claims                        | L7, L8, L9, L10                | second G0 desktop target                                                                 |
| macOS x64                   | Figma Desktop          | full editor-facing claims                        | L7 (release verify)            | listed by PRD 8.11 Platform row                                                          |
| Windows x64 (CI, no Figma)  | none                   | core, protocol, schema, fuzz, fixtures, CLI only | L1 to L6                       | no editor claim possible                                                                 |
| Linux x64 (core only)       | none (editor untested) | core, CLI, fixtures, schema, fuzz only           | L1 to L6 plus CLI subset of L7 | PRD 8.11 and PRD 1.1: Linux stays core and offline until an editor environment is tested |
| Reference profile dev box   | Figma Desktop          | benchmark and soak numbers                       | L8, L9                         | hardware identity recorded per run                                                       |

Constraints carried across environments:

1. Editor wins on Windows x64 and macOS arm64 (and macOS x64 for v0.1 release notes) are required
   before any "desktop ready" statement enters a release note (PRD 8.11 Platform row).
2. Linux evidence may only label core pieces (`crates/*`, `fixtures/`, CLI); any Linux statement
   about plugin behavior is prohibited until an editor environment is verifiably tested.
3. Every L7, L8, or L9 evidence block records OS build, CPU model, RAM, exact Figma Desktop version,
   and the `Figma_version` template field; a claim limited to one OS inherits that limitation in
   release notes until the missing environment is run.

## 4. Fixture plan

### 4.1 Sizes and page shapes

PRD 2.1 fixture clause: small 100 nodes; medium 1000 nodes; large 10000 nodes in scope; big
cross-page documents to pressure lazy loading. Fixture IDs below are stable and must not be reused
for different shapes; content revisions carry a revision suffix.

| Fixture ID       | Shape                                                                                         | Purpose                                                           | Consumed by                              |
| ---------------- | --------------------------------------------------------------------------------------------- | ----------------------------------------------------------------- | ---------------------------------------- |
| `FX-SMALL`       | 100 nodes, single page, plain frames plus text                                                | baseline fresh reads, install, no throttling                      | AT02, AT03, AT18, N02                    |
| `FX-MEDIUM`      | 1000 nodes, nested frames, component instances, mixed text                                    | steady state traversal, cache, pagination at moderate size        | AT04, AT07, AT08, AT10, AT12, AT17       |
| `FX-LARGE`       | 10000 nodes in scope, deep nesting                                                            | pagination limits, cursor binding, truncation, partiality         | AT07, AT08, AT13                         |
| `FX-CROSSPAGE`   | multi-page document sized to exceed dynamic-page defaults                                     | lazy loading behavior, no up front full page load                 | AT07, AT19                               |
| `FX-MIXED`       | 200 to 500 nodes carrying the full attribute checklist of 4.2                                 | normalization and export fidelity; ground truth for expected JSON | AT04, AT05, AT10, AT11, AT12, AT17, AT18 |
| `FX-ADVERSARIAL` | extreme depth, huge names, very large text, unknown types, oversize assets, malformed exports | parser and limit hardening                                        | AT06, AT07, AT10, AT11, AT13             |

### 4.2 Attribute checklist

Every attribute row must exist in at least one fixture and have a matching expected JSON block so
`AT10` can require exact presence (PRD 2.1 attribute list verbatim).

| Attribute       | Required synthesis in fixtures                                                   | Verified primarily by | Notes                                        |
| --------------- | -------------------------------------------------------------------------------- | --------------------- | -------------------------------------------- |
| Nested layout   | at least 4 levels of frame nesting; auto layout axis, gap, padding, sizing modes | FR08 / AT10           | fixed vs hug vs fill distinguished           |
| Mixed text      | multilingual styled text block with mixed families, weights, sizes               | FR08 / AT10           | becomes runs per PRD 8.5.3                   |
| Variants        | component set with at least 3 variants plus instance overrides                   | FR08 / AT10           | references stay opaque IDs when out of scope |
| Aliases/modes   | variable alias across collections and at least 2 modes                           | FR10 / AT10           | unresolved and cyclic alias reported         |
| Gradients       | linear and radial gradient fills                                                 | FR08 / AT10           | stops and transform reported                 |
| Masks           | masked group plus clipped frame                                                  | FR08 / AT10           | visibility semantics preserved               |
| Image fills     | raster fills at multiple scales and hashes                                       | FR11 / AT11           | never passed as a permalink                  |
| SVG             | vector nodes exported via `export_assets`                                        | FR11 / AT11           | no active content survives                   |
| Missing nodes   | deleted child reference inside an instance override                              | FR09 / AT10           | reported, not silently dropped               |
| Hidden/locked   | hidden nodes, locked groups, descendant visibility                               | FR24 / AT10           | visibility flags in normalized output        |
| Non-Latin text  | CJK, RTL, and emoji runs inside the mixed text block                             | FR08 / AT10           | encoding preserved byte for byte             |
| Unknown types   | valid but unrecognized future node types                                         | FR08 / AT10           | preserved as `unsupported` with reason       |
| Traversal scale | node counts of `FX-LARGE` and `FX-CROSSPAGE`                                     | FR07 / AT07           | partial and cursor semantics                 |

### 4.3 Licensing and sourcing

1. All fixture datasets are clearly licensed, contain no client designs, and carry a per file license
   record in `fixtures/README.md` (PRD 2.1 fixture clause).
2. Third party material is copied only with its original license and attribution intact; a language
   change or rewrite never removes a license duty (PRD 8.11).
3. Every fixture file records origin, creation script, revision, and sha256 in
   `fixtures/README.md`; undocumented content blocks a release, and license drift blocks the release
   after re-check (PRD 8.11 Repository hygiene row).

### 4.4 Curated normalized JSON expectations

1. Each fixture carries `fixtures/expected/<fixture>.json`: canonical expected normalized output in
   `schema_version` 1.0, aligned with `schemas/` and the envelope contract in
   `docs/04-data-contracts/01-mcp-envelope.md`. They are the only source of exact match assertions for
   `AT10`, regenerated only by an explicit contract migration, never by hand editing during a fix.
2. Every `schema_version` bump regenerates all `fixtures/expected/*` files in the same commit and
   re-runs the L4 round-trip; gate evidence must show the regeneration commit. Desktop and CI runs
   compare against the same expected JSON; a difference is a finding, not a tolerable variance.

### 4.5 Fixture provenance and hashing in release evidence

1. Every gate run records the fixture revision ID and sha256 of every fixture file used via the
   `fixture_hash` template field; hashes are recomputed at run start and a mismatch invalidates the
   run (PRD 2.2 step 1 freezes fixtures before the run).
2. After a fixture revision changes, prior evidence for the affected scenarios is invalidated and
   re-run at the next gate; a fixture change is never merged in the same commit as a product change
   so each gate run keeps a single variable.

## 5. Evidence rules

### 5.1 Evidence template (PRD Appendix C)

Every evidence record, from any layer, uses these exact fields. The template is normative; fields
may be empty only when the test genuinely produces no such data, and the empty state is stated.

| Field                       | Contract                                                                          |
| --------------------------- | --------------------------------------------------------------------------------- |
| `commit`                    | exact source commit under test; binary and bundle must be built from it           |
| `artifact_sha256`           | sha256 of the binary, plugin bundle, and any package artifact used                |
| `schema/protocol_version`   | `schema_version` and bridge `protocol_version` pinned for the run                 |
| `OS/CPU/RAM`                | OS build, CPU model, total RAM; CI runners named as such                          |
| `Figma_version`             | exact Figma Desktop version from the About panel; empty only for pure core claims |
| `plugin_manifest_hash`      | sha256 of the plugin manifest bytes actually running                              |
| `account_plan/seat_context` | plan and seat class only, no identity or file names                               |
| `fixture_hash`              | sha256 list of fixture files and fixture revision ID                              |
| `test_id`                   | `AT01..AT20` or `N01..N10` plus revision, for example `AT04-r2`                   |
| `commands`                  | exact commands and client calls executed, placeholders expanded                   |
| `expected/actual`           | target value vs measured value per step                                           |
| `duration/latency/RSS`      | wall time, p50/p95/p99 where applicable, peak RSS sampled at 1 s                  |
| `network_path_evidence`     | capture basis attributing connections to a named process                          |
| `known_limitations`         | accepted gaps with owning FR or ADR and the gate that will close them             |
| `reviewer`                  | named human with sign-off authority; automation never signs                       |
| `tested_at`                 | UTC timestamp                                                                     |

### 5.2 Hard rules

1. Exact commit. Evidence is void if the binary under test cannot be reproduced from `commit` plus
   lockfiles; the PRD 8.10.2 Definition of Done requires artifacts to be tested from the distribution
   artifacts, not only from the source tree.
2. Source inspection never substitutes runtime proof. Reviewing `crates/core` code may support a
   claim but never replaces a captured runtime outcome; the same holds in reverse for code-only
   reasoning about plugin behavior (PRD 8.8).
3. Automation never replaces desktop tests. Scenarios whose scenario text names Figma Desktop
   behavior (AT03, AT04, AT09, AT10, AT11, AT12, AT17, AT18, N06) require the L7 protocol; CI
   outcomes for those scenarios are supporting evidence only.
4. Sanitized logs. Evidence logs contain no design text, layer names, file IDs, tokens, or secrets;
   the sanitized form mandated by PRD 8.6.2 also governs evidence documents and pilot notes.
5. Network attribution. `network_path_evidence` must attribute connections to a specific process
   because Figma itself uses the network; a whole machine capture never proves a claim about core
   egress (PRD Appendix A note after the AT table).
6. Negative results stand. Any failed, blocked, or unsupported outcome is recorded with cause and
   stays in the gate report (PRD 2.2 step 6; PRD 2.3 final sentence).
7. Dual gate reruns. A scenario listed for two gates in PRD Appendix A (for example `G0/G2`) is
   first evidenced at the earlier gate and re-evidenced at the later gate on the release candidate;
   the later run supersedes nothing, both records stay.
8. Reviewer sign off. Every evidence block names a reviewer; a bot or a CI job never signs. The PRD
   D.2 approval record is the register of who is authorized per role.

### 5.3 Where evidence lives

1. Gate evidence files land under `docs/07-ops/release-evidence/` as `G{gate}/AT{nn}.md` and
   `G{gate}/N{nn}.md` per `docs/07-ops/04-release-process.md`; soak, benchmark, and pilot runs nest
   in the same directory with one file per run; reruns append, never overwrite.
2. The catalog `docs/06-testing/02-acceptance-catalog.md` enumerates evidence requirements per
   scenario; this file only fixes the shared template and storage rules.

## 6. Ownership matrix

Roles mirror PRD D.2. Legend: `A` accountable and signs, `R` responsible for execution or review,
`-` not applicable.

| Test class                                | Core owner (Rust) | Plugin owner (TS) | Security reviewer | QA and release owner | Product maintainer |
| ----------------------------------------- | ----------------- | ----------------- | ----------------- | -------------------- | ------------------ |
| L1 Rust unit                              | A                 | -                 | -                 | R                    | -                  |
| L2 Rust integration, in-memory transports | A                 | R                 | R                 | R                    | -                  |
| L3 TS unit                                | R                 | A                 | -                 | R                    | -                  |
| L4 schema round-trip Rust and TS          | A                 | A                 | R                 | R                    | -                  |
| L5 property and fuzz                      | A                 | R                 | A                 | R                    | -                  |
| L6 negative and adversarial suite         | R                 | R                 | A                 | R                    | -                  |
| L7 desktop manual protocol                | R                 | A                 | R                 | A                    | R                  |
| L8 mixed soak                             | R                 | -                 | -                 | A                    | -                  |
| L9 benchmark runs                         | R                 | R                 | -                 | A                    | -                  |
| L10 G2 pilot                              | -                 | R                 | -                 | A                    | A                  |
| Fixture plan and licenses                 | A                 | R                 | R                 | R                    | -                  |
| Evidence intake and sign off              | C                 | C                 | A                 | A                    | C                  |
| CI stages                                 | R                 | R                 | R                 | A                    | -                  |

Cross checks: the security reviewer additionally owns alignment between `docs/05-security/01-threat-model.md`
rows and L6 scenarios; the product maintainer owns claim language reviewed by AT16 and the pilot
decision rule of PRD 2.3.

## 7. CI stages

Aligned with PRD 8.11 CI row: fmt/clippy/test, TS typecheck/lint/test, schema contracts, parser fuzz
corpus, artifact checks, dependency audit; supply chain adds SBOM, checksums, provenance, signing
per availability.

| Stage              | Checks                                                                                                                           | Blocks                     | Feeds                                 |
| ------------------ | -------------------------------------------------------------------------------------------------------------------------------- | -------------------------- | ------------------------------------- |
| ci-rust-fmt-clippy | `cargo fmt --all --check`; `cargo clippy --workspace --all-targets -- -D warnings`                                               | all PRs                    | L1 readiness                          |
| ci-rust-test       | `cargo test --workspace` (L1 and L2)                                                                                             | all PRs                    | gate reports                          |
| ci-ts              | TS typecheck, lint, unit tests for `plugins/figma/{main,ui}` (L3)                                                                | all PRs                    | gate reports                          |
| ci-schema-contract | L4 round-trip both directions; diff `schemas/` against last released schema; regenerate `fixtures/expected/*` when version bumps | all PRs touching contracts | AT10 ground truth                     |
| ci-fuzz-corpus     | reduced fuzz corpus run on PR; full corpus nightly (L5)                                                                          | PRs touching parsers       | AT13                                  |
| ci-deps-audit      | `cargo audit`; npm audit; license report diff                                                                                    | all PRs                    | release process                       |
| ci-supply-chain    | SBOM generation, checksum job, provenance fields, pinned actions verification                                                    | release candidate PRs      | PRD 8.11 Supply chain row, PRD 8.10.2 |
| ci-docs            | docs prettier check, ID lint (no renumbered `FR`/`AT`/`N`), em dash ban, link check                                              | all PRs                    | doc set integrity                     |

Nightly and pre gate extras: full fuzz sweep, schema drift report against last release, and a
compile of `fixtures/expected/*` against `schemas/` byte for byte. Benchmark and soak jobs are
never per PR; they run per gate on the reference profile per `docs/06-testing/04-benchmark-plan.md`.

## 8. Gate coverage summary

Gate mapping is verbatim from PRD Appendix A and PRD 8.10. A scenario listed at two gates is
evidenced at both (rule 5.2.7).

| Gate           | Scenarios that must be closed at the gate                                                                                     | Extra obligations                                                                                                        |
| -------------- | ----------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| G0 feasibility | AT01, AT02, AT12, AT18; desktop Windows plus macOS; N09 evidence via L9                                                       | Starter path proven; if one OS fails, support scope narrows; no later stage starts before G0 core risks close (PRD 8.10) |
| G1 alpha       | first full evidence for AT04, AT07, AT10; negative work of AT05, AT06, AT13, AT17; P0 functional requirement set complete     | alpha is developer only with limitations disclosed                                                                       |
| G2 public v0.1 | AT03, AT08, AT09, AT11, AT14, AT15, AT16; rerun evidence for AT01, AT06, AT10, AT12, AT18; all P0 FRs; L10 pilot measures N10 | no critical data leak, crash, or misrouting bug; artifacts from exact commit (PRD 8.10 G2 row)                           |
| G3 v0.2        | AT19, AT20 with P1 FRs; AT13 rerun for bundle fuzz half                                                                       | features are not published as stable while the gate fails                                                                |

Scenario to gate detail: AT05, AT06, AT07, AT09, AT10, AT13, AT15, AT17 carry dual or cross gate
obligations as written in PRD Appendix A; the catalog entry for each states its own gate line.

## 9. Failure handling

1. A gate report never asserts pass while any included scenario is failing, blocked, or unsupported
   without an explicit `known_limitations` entry naming the owning FR or ADR and the gate that will
   close it (PRD 8.10.2; PRD 2.3).
2. When automation is inconclusive, for example client timing drift or editor cache flake, the
   scenario is downgraded to an L7 manual run whose result outranks automation (rule 5.2.3).
3. When automation and desktop disagree, the desktop result blocks the gate and the automation gap
   becomes a tracked issue with an owner (PRD 2.2 steps 5 and 6).
4. Reaching a target by changing the fixture, the environment, or the metric definition invalidates
   the run; the PRD 2.2 protocol requires frozen fixtures and success definitions before the run.

## 10. Related documents

- `docs/00-overview/01-prd-analysis.md`: binding interpretations used by every scenario here.
- `docs/01-agile/01-sdlc-process.md` and `docs/02-architecture/01-system-design.md` with
  `docs/02-architecture/adr/`: gate cadence, architecture, and ADRs (ADR-002 transport, ADR-003 SDK
  pinning) that L2 and L4 must respect; `docs/03-uml/` shows where harnesses run.
- `docs/04-data-contracts/06-resource-limits.md`: numeric limits cited by AT07, AT08, AT11, AT13.
- `docs/05-security/01-threat-model.md`: threat rows mapped to L6 and AT06.
- `docs/06-testing/02-acceptance-catalog.md`: `AT01..AT20` detailed steps.
- `docs/06-testing/03-traceability-matrix.md`: `FR` to `AT` mapping.
- `docs/06-testing/04-benchmark-plan.md`: `N01..N10` procedures; benchmark runs point here.
- `docs/07-ops/03-runbook.md`, `docs/07-ops/04-release-process.md`: where evidence files land and how
  SBOM, checksums, and provenance attach per PRD 8.10.
