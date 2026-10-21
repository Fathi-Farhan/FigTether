# FigTether Glossary

> **Design reference.** Original requirements and approval labels below are historical design
> records, not a feature-availability list. See [current documentation status](../documentation-status.md)
> for implemented reads/writes, observed desktop evidence, and remaining gaps.

Status: `Proposed`

This glossary is the vocabulary of record for the FigTether documentation set. Part 1 reproduces every
term from `FigTether_PRD_Confluence_v1.1.md` (`FGT-PRD-001`) Appendix B so that docs-only readers never
have to open the PRD. Part 2 adds the engineering vocabulary introduced by this docs set, each entry with
a primary document reference. Harness terms in Part 2 come from the upstream Harness2 project; their
sources are registered in `99-appendices/02-source-register.md`. Usage rules: term IDs and names are
verbatim and must not be paraphrased; the PRD remains normative and wins in any dispute; in prose the
upstream project name is written "Harness2" (no superscript).

## 1. Part 1: PRD Appendix B terms

Reproduced from the PRD Appendix B table. Definitions are rendered from the PRD's Indonesian wording
into English with the meaning left unchanged; the term list and order are preserved.

| Term           | Definition                                                                               |
| -------------- | ---------------------------------------------------------------------------------------- |
| MCP            | Model Context Protocol for exchanging tools and resources between client and server      |
| Rust core      | The daemon, MCP adapter, CLI, normalization, scheduler, and temporary storage            |
| Plugin adapter | The thin TypeScript layer that accesses the Figma API and the UI iframe                  |
| Grant          | User permission on a specific root; changing the selection does not widen the permission |
| Epoch          | Connection or authorization version used to reject results that no longer apply          |
| Live read      | A Plugin API read issued on demand while the session is active                           |
| Fresh read     | A request that reaches the plugin; it is never satisfied from an old cache               |
| Snapshot       | An immutable capture that does not automatically follow document changes                 |
| Observation    | A window in which data was read; not a transaction and not an official Figma version     |
| Coverage       | How much of the data was obtained, truncated, or unavailable                             |
| Backpressure   | Rejecting or deferring requests so resources stay within limits                          |
| Capability     | An ability detected at runtime under a specific permission context                       |
| Resync         | Re-baselining after change gaps or when a cursor is no longer valid                      |
| G0 to G4       | The gates: feasibility, alpha, v0.1, v0.2, and subsequent development                    |

Related ID families carried by the PRD and reused throughout this docs set: gates `G0..G4`, epics
`E01..E10`, requirements `FR01..FR24`, NFR targets `N01..N10`, acceptance tests `AT01..AT20`, problem
evidence `P01..P14`, risks `R01..R14`, decisions `D01..D12`, and PRD sources `S1..S11`. IDs are stable
and are never reassigned (see `docs/README.md`).

## 2. Part 2: FigTether engineering terms

Each entry is one or two lines plus its primary doc refs. Folder-level paths (trailing slash) point at
the document inside that folder that owns the topic; exact file paths follow the canonical registry in
`99-appendices/03-approval-record.md`.

### 2.1 Contract and wire terms

| Term               | Definition                                                                                                                                                                                                                                 | Doc refs                                                                                          |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------- |
| envelope           | The required shape of every read result: `schema_version`, `request_id`, `source` (kind plus session or snapshot identity), `observation` (IDs, timestamps, consistency, freshness, `cache_age_ms`, hash), `coverage`, `data`, `warnings`. | `04-data-contracts/01-mcp-envelope.md`; PRD 8.5.2                                                 |
| projection         | Explicit selection of the fields a read returns; pairs with pagination so MCP responses stay inside the 64 KiB encoded cap instead of flooding agent context.                                                                              | `04-data-contracts/02-tool-catalog.md` (`FR07`, `P04`); `04-data-contracts/06-resource-limits.md` |
| cursor binding     | A live pagination cursor is bound to one observation window, scope, and epoch; detected change or expiry returns `CURSOR_EXPIRED` instead of blended data; snapshot traversal carries its own `next_cursor`.                               | `04-data-contracts/02-tool-catalog.md`; `04-data-contracts/05-error-catalog.md`; PRD 8.3          |
| observation window | The `started_at` / `observed_at` interval of one read; identical concurrent fresh requests may share one window; edits detected inside the window force `best_effort` labeling, a warning, or `DOCUMENT_CHANGED`.                          | `04-data-contracts/01-mcp-envelope.md` (`FR06`, `FR12`); PRD 8.3                                  |
| coalescing window  | The admitted merge period in which identical in-flight fresh requests share one plugin read and one observation window; it never substitutes cached data, and broader coalescing is a v0.2 optimization, not a v0.1 promise.               | `04-data-contracts/01-mcp-envelope.md`; PRD 8.3; PRD section 7 (Live read, v0.2 row)              |
| Tombstone          | A watch journal entry (P1) recording a deleted node ID; deleted IDs are tombstoned rather than silently dropped, and a stale cursor demands `RESYNC_REQUIRED` instead of inferred deletion.                                                | `02-architecture/adr/` (watch and resync ADR, `FR20`, `AT19`); PRD 8.3                            |
| provenance         | Every result carries machine-readable origin: source kind, session or snapshot identity, observation timestamps, and coverage; the same discipline extends to release artifacts (commit, checksums, SBOM, provenance).                     | `04-data-contracts/01-mcp-envelope.md` (`G03`, `FR12`); `07-ops/04-release-process.md`; PRD 8.11  |

### 2.2 Scope, enforcement, and audit terms

| Term               | Definition                                                                                                                                                                                                          | Doc refs                                                                              |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------- |
| double enforcement | Scope membership, session, and `grant_epoch` are verified in both the Rust daemon and the plugin on every request, so a bridge bug or malicious frame cannot widen access on its own.                               | `05-security/02-access-control.md` (`FR03`, `R05`)                                    |
| scope root         | The node ID named at grant time; the permitted set is that root's subtree with descendants deduplicated, selection moves never widen it, and out-of-subtree nodes stop resolving.                                   | `05-security/02-access-control.md` (`FR03`, PRD 8.4); PRD 5.1                         |
| deny-egress audit  | The `FR19` evidence ritual: source review plus a mock environment that denies egress plus process connection capture at the exact commit, proving the core makes zero official MCP or REST requests (`N09`, `G01`). | `06-testing/` (acceptance catalog: `AT01`, `AT02`, `AT16`); PRD section 2.1 and `N09` |

### 2.3 Scheduler and resource terms

| Term                  | Definition                                                                                                                                                                                                                  | Doc refs                                                            |
| --------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| global export lane    | The single asset-export slot held by the scheduler across the whole daemon (one PNG/SVG export active at a time), separate from the one active editor operation per session queue.                                          | `04-data-contracts/06-resource-limits.md` (PRD 8.5.5)               |
| adaptive slicing      | Traversal runs in time-sliced batches with a target of about 8 ms per slice, then yields back to the editor runtime when it permits; under pressure batches shrink and progress is surfaced instead of freezing the editor. | `04-data-contracts/06-resource-limits.md` (PRD 8.5.5; `P05`, `R04`) |
| aging anti-starvation | Scheduler priority runs revoke, cancel, and heartbeat first, then small reads, then exports; long-running large jobs accumulate priority over time (aging) so heavy traversals cannot starve forever.                       | `04-data-contracts/06-resource-limits.md` (PRD 8.5.5)               |

### 2.4 Operations, privacy, and release terms

| Term                    | Definition                                                                                                                                                                                                                                                                                                                                             | Doc refs                                                                            |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------- |
| sanitized diagnostics   | Local diagnostics carry error codes, versions, metrics, and sanitized correlation IDs only; no layer names, text, file IDs, or tokens by default, so an issue report never leaks client designs.                                                                                                                                                       | `07-ops/03-runbook.md` (`FR18`, `R13`); `05-security/` (privacy rules)              |
| runtime directory       | The per-user, per-run host directory holding daemon credentials and runtime files, with owner-only permissions that are verified (Windows DACL / POSIX); `figtether doctor` checks it, and stale files after a crash are detected but not trusted.                                                                                                     | `07-ops/01-dev-environment.md`; `05-security/02-access-control.md` (PRD 5.1, 8.6.2) |
| release-evidence record | The filled release evidence template for an exact tested commit: commit, artifact sha256, schema and protocol versions, OS/CPU/RAM, Figma version, plugin manifest hash, plan and seat context, fixture hash, test id, commands, expected and actual, measured latencies and RSS, network path evidence, known limitations, reviewer, and tested time. | `07-ops/04-release-process.md`; PRD Appendix C (release evidence template)          |

### 2.5 Agent harness terms (Harness2)

The definitions marked "upstream" are the wording of the upstream Harness2 project, quoted here in
ASCII form; sources are `HS1`..`HS5` in `99-appendices/02-source-register.md`.

| Term                    | Definition                                                                                                                                                                                                                                                                                                                                       | Doc refs                                                                                                |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------- |
| harness (agent system)  | The system around a model that enables it to act as an agent: prompts, tools, memory, skills, orchestration, and more (upstream definition).                                                                                                                                                                                                     | `08-harness2/` (harness design); `HS1`                                                                  |
| harnessing              | The process of optimizing that surrounding system; upstream shortens this to "the process of optimizing that system" (upstream definition).                                                                                                                                                                                                      | `08-harness2/` (harness design); `HS1`                                                                  |
| Harness2 components (8) | The eight editable components the framework exposes around the model: system prompt, guardrails, memory, tool guidance, subagents, skills, scripts, and plugins; they control context construction, the tool interface, delegation, and middleware behavior inside the tool loop.                                                                | `08-harness2/` (method doc); `HS1`, `HS5`                                                               |
| Propose-Probe-Compose   | The contrastive edit loop of task-level recursion: propose invariant and domain edits, probe candidate harnesses on the same task, then compose the refined harness from behavioral contrasts between executions, without ground-truth verifiers; scaled by parallel recursion (wider exploration) and sequential recursion (deeper refinement). | `08-harness2/` (method and evaluation docs); `HS3`, `HS5`                                               |
| improvement harness     | The persistent state updated by harnessing-level recursion: adopted harnesses plus evidence-supported lessons, retained per domain, that steer refinement of future tasks.                                                                                                                                                                       | `08-harness2/` (method and improvement workflow docs); `HS1`, `HS3`, `HS5`                              |
| harness pack            | A FigTether-local packaging term: a reviewable bundle carrying one adopted harness (component edits), the probe evidence that justified it, and the lessons accepted into the improvement harness; it carries provenance and must pass review before reuse. Last word on the exact shape belongs to the harness docs.                            | `08-harness2/` (harness pack doc); wrapped by the review gates in `99-appendices/03-approval-record.md` |

Two cautions when reading Part 2.5. First, Harness2 figures reported upstream (for example 11.5 points
on WorkBuddy-Bench, 4.5 points on JobBench, 11.8 percentage points on LAB's partial pass rate) are
upstream results on upstream benchmarks; FigTether restates them only with `HS` attribution and never
as FigTether experiment results. Second, "harness pack" is not an upstream term; it is defined locally
by the `08-harness2` docs and this entry defers to them.

## 3. Cross-reference: keyword to primary doc

| Keyword                                                                                 | Primary doc                                               |
| --------------------------------------------------------------------------------------- | --------------------------------------------------------- |
| naming, goals, requirements inventory, committed interpretations                        | `docs/00-overview/01-prd-analysis.md`                     |
| landscape, personas                                                                     | `docs/00-overview/`                                       |
| sprints, backlog, roadmap, risks, gates `G0..G4`                                        | `docs/01-agile/01-sdlc-process.md`                        |
| architecture, components, transport (`D06`)                                             | `docs/02-architecture/01-system-design.md`                |
| scheduling, backpressure, limits                                                        | `docs/04-data-contracts/06-resource-limits.md`            |
| envelope, observation window, coalescing window, provenance                             | `docs/04-data-contracts/01-mcp-envelope.md`               |
| tools, projection, cursor binding                                                       | `docs/04-data-contracts/02-tool-catalog.md`               |
| errors (`CURSOR_EXPIRED`, `BUSY`, and friends)                                          | `docs/04-data-contracts/05-error-catalog.md`              |
| normalized design model                                                                 | `docs/04-data-contracts/04-normalized-model.md`           |
| ADRs (architecture decisions `ADR-001` and up)                                          | `docs/02-architecture/adr/`                               |
| UML views (use case, class, sequence, state, activity, deployment)                      | `docs/03-uml/`                                            |
| grants, epochs, double enforcement, scope root, loopback trust                          | `docs/05-security/02-access-control.md`                   |
| threat model, privacy                                                                   | `docs/05-security/`                                       |
| tests, acceptance tests `AT01..AT20`, deny-egress audit                                 | `docs/06-testing/` (test strategy and acceptance catalog) |
| traceability                                                                            | `docs/06-testing/03-traceability-matrix.md`               |
| benchmarks `N01..N10`                                                                   | `docs/06-testing/04-benchmark-plan.md`                    |
| development environment, runtime directory                                              | `docs/07-ops/01-dev-environment.md`                       |
| end user install                                                                        | `docs/07-ops/02-end-user-install.md`                      |
| runbook, sanitized diagnostics                                                          | `docs/07-ops/03-runbook.md`                               |
| release evidence, provenance of artifacts                                               | `docs/07-ops/04-release-process.md`                       |
| harness, harnessing, Harness2, Propose-Probe-Compose, improvement harness, harness pack | `docs/08-harness2/`                                       |
| PRD sources `S1..S11`, Harness2 sources `HS1..HS5`                                      | `docs/99-appendices/02-source-register.md`                |
| approvals, statuses, revision history                                                   | `docs/99-appendices/03-approval-record.md`                |

Registry note: paths for files still being authored in parallel reflect the canonical names committed
in `99-appendices/03-approval-record.md`; if a file lands under a different name, the integration pass
updates the path cells above and in that registry without changing terms, definitions, IDs, or owners.
