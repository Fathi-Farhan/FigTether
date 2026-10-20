# FigTether Approval Record

Status: `Proposed`

This is the governance record for the FigTether documentation set produced from
`FigTether_PRD_Confluence_v1.1.md` (`FGT-PRD-001`). It requests document-level decisions from the five
PRD Appendix D roles, keeps the status registry for every file under `docs/`, records the revision
history, and carries the review checklist applied to this docs set. Status labels follow
`docs/README.md`: `Proposed`, `Approved`, `Implemented-in-PRD-only`. A document approval is a review
decision recorded here; it never substitutes for gate evidence, which is separate and runtime-based
(PRD section 8.10).

## 1. Decision request

These decisions move documents from `Proposed` to `Approved` in section 2. They mirror the PRD Appendix
D approval record, which also shows every role as pending; no runtime gate has been exited yet.

| Role               | Requested decision                                                                                                                                                                                       | Status  | Evidence placeholder                                                                                                                                       |
| ------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Product Maintainer | Approve the docs set purpose, scope, priorities, and definitions of success; confirm the set derives faithfully from PRD v1.1 and never contradicts it silently                                          | Pending | placeholder: sign-off recorded in section 2 status changes plus the fully ticked checklist in section 4; named owner noted at kickoff per the PRD header   |
| Rust/Core Owner    | Approve system design, bridge, MCP adapter, grant and epoch model, envelope and consistency semantics, normalized model, capability probing, resource limits, and ADR-001..ADR-008 plus ADR-010, ADR-011 | Pending | placeholder: per-ADR sign-off lines; resolved review comments for `02-architecture/` and `04-data-contracts/` files                                        |
| Plugin Owner       | Approve plugin adapter design: eligibility and capability model, manifest constraints, runtime and export behavior (ADR-002, ADR-007)                                                                    | Pending | placeholder: plugin design review sign-off; manifest plus CSP and Origin test record from G0 on Windows and macOS                                          |
| Security Reviewer  | Approve threat model, access control, local trust model, privacy and data handling, pairing and revoke semantics                                                                                         | Pending | placeholder: `05-security/` sign-offs; ADR-004 and ADR-012 co-sign; negative-test coverage named for `AT06`, `AT17`; sanitized diagnostics rule signed off |
| QA/Release Owner   | Approve test strategy, acceptance catalog, traceability matrix, benchmark plan, runbook, and release process including evidence slots                                                                    | Pending | placeholder: `06-testing/` and `07-ops/` sign-offs; release-evidence template gaps listed as open slots per gate                                           |

Scope note: gate exits (`G0` feasibility and beyond) are separate decisions requiring produced evidence
(PRD Appendix A: `AT01`, `AT02`, `AT12`, `AT18` at `G0`; and onward). Approving documents here says the
written design is internally consistent and PRD-faithful; it says nothing about measured results.

## 2. Document status registry

Canonical registry of every file under `docs/` at creation of this docs set. Status is `Proposed` for
all entries; owner role is the single primary reviewer, with the co-sign requirements noted after the
table.

| Path                                                        | Status     | Owner role         | Review gate |
| ----------------------------------------------------------- | ---------- | ------------------ | ----------- |
| `docs/README.md`                                            | `Proposed` | Product Maintainer | G0          |
| `docs/00-overview/01-prd-analysis.md`                       | `Proposed` | Product Maintainer | G0          |
| `docs/00-overview/02-competitive-landscape.md`              | `Proposed` | Product Maintainer | G0          |
| `docs/00-overview/03-personas-journeys.md`                  | `Proposed` | Product Maintainer | G0          |
| `docs/01-agile/01-sdlc-process.md`                          | `Proposed` | Product Maintainer | G0          |
| `docs/01-agile/02-product-backlog.md`                       | `Proposed` | Product Maintainer | G0          |
| `docs/01-agile/03-roadmap-sprints.md`                       | `Proposed` | Product Maintainer | G0          |
| `docs/01-agile/04-risk-register.md`                         | `Proposed` | Product Maintainer | G0          |
| `docs/02-architecture/01-system-design.md`                  | `Proposed` | Rust/Core Owner    | G0          |
| `docs/02-architecture/02-bridge-design.md`                  | `Proposed` | Rust/Core Owner    | G0          |
| `docs/02-architecture/03-mcp-design.md`                     | `Proposed` | Rust/Core Owner    | G0          |
| `docs/02-architecture/04-plugin-design.md`                  | `Proposed` | Plugin Owner       | G0          |
| `docs/02-architecture/05-state-machines.md`                 | `Proposed` | Rust/Core Owner    | G1          |
| `docs/02-architecture/adr/ADR-000-template.md`              | `Proposed` | Product Maintainer | G0          |
| `docs/02-architecture/adr/ADR-001-language-boundary.md`     | `Proposed` | Rust/Core Owner    | G0          |
| `docs/02-architecture/adr/ADR-002-transport-selection.md`   | `Proposed` | Rust/Core Owner    | G0          |
| `docs/02-architecture/adr/ADR-003-mcp-sdk-rmcp.md`          | `Proposed` | Rust/Core Owner    | G0          |
| `docs/02-architecture/adr/ADR-004-grant-scope-model.md`     | `Proposed` | Rust/Core Owner    | G0          |
| `docs/02-architecture/adr/ADR-005-consistency-model.md`     | `Proposed` | Rust/Core Owner    | G0          |
| `docs/02-architecture/adr/ADR-006-pagination-projection.md` | `Proposed` | Rust/Core Owner    | G0          |
| `docs/02-architecture/adr/ADR-007-cache-asset-store.md`     | `Proposed` | Rust/Core Owner    | G0          |
| `docs/02-architecture/adr/ADR-008-resource-budgets.md`      | `Proposed` | Rust/Core Owner    | G1          |
| `docs/02-architecture/adr/ADR-009-licensing.md`             | `Proposed` | Product Maintainer | G2          |
| `docs/02-architecture/adr/ADR-010-session-identity.md`      | `Proposed` | Rust/Core Owner    | G0          |
| `docs/02-architecture/adr/ADR-011-error-taxonomy.md`        | `Proposed` | Rust/Core Owner    | G0          |
| `docs/02-architecture/adr/ADR-012-harness2-integration.md`  | `Proposed` | Product Maintainer | G1          |
| `docs/03-uml/01-use-case-diagrams.md`                       | `Proposed` | Rust/Core Owner    | G0          |
| `docs/03-uml/02-class-diagrams.md`                          | `Proposed` | Rust/Core Owner    | G0          |
| `docs/03-uml/03-sequence-diagrams.md`                       | `Proposed` | Rust/Core Owner    | G1          |
| `docs/03-uml/04-state-diagrams.md`                          | `Proposed` | Rust/Core Owner    | G1          |
| `docs/03-uml/05-activity-diagrams.md`                       | `Proposed` | Rust/Core Owner    | G1          |
| `docs/03-uml/06-deployment.md`                              | `Proposed` | Rust/Core Owner    | G2          |
| `docs/04-data-contracts/01-mcp-envelope.md`                 | `Proposed` | Rust/Core Owner    | G0          |
| `docs/04-data-contracts/02-tool-catalog.md`                 | `Proposed` | Rust/Core Owner    | G0          |
| `docs/04-data-contracts/03-bridge-protocol.md`              | `Proposed` | Rust/Core Owner    | G0          |
| `docs/04-data-contracts/04-normalized-model.md`             | `Proposed` | Rust/Core Owner    | G0          |
| `docs/04-data-contracts/05-error-catalog.md`                | `Proposed` | Rust/Core Owner    | G0          |
| `docs/04-data-contracts/06-resource-limits.md`              | `Proposed` | Rust/Core Owner    | G0          |
| `docs/05-security/01-threat-model.md`                       | `Proposed` | Security Reviewer  | G0          |
| `docs/05-security/02-access-control.md`                     | `Proposed` | Security Reviewer  | G0          |
| `docs/05-security/03-privacy-data-handling.md`              | `Proposed` | Security Reviewer  | G1+G2       |
| `docs/06-testing/01-test-strategy.md`                       | `Proposed` | QA/Release Owner   | G0          |
| `docs/06-testing/02-acceptance-catalog.md`                  | `Proposed` | QA/Release Owner   | G0..G3      |
| `docs/06-testing/03-traceability-matrix.md`                 | `Proposed` | QA/Release Owner   | G1          |
| `docs/06-testing/04-benchmark-plan.md`                      | `Proposed` | QA/Release Owner   | G1+G2       |
| `docs/07-ops/01-dev-environment.md`                         | `Proposed` | Rust/Core Owner    | G0          |
| `docs/07-ops/02-end-user-install.md`                        | `Proposed` | QA/Release Owner   | G2          |
| `docs/07-ops/03-runbook.md`                                 | `Proposed` | QA/Release Owner   | G2          |
| `docs/07-ops/04-release-process.md`                         | `Proposed` | QA/Release Owner   | G2          |
| `docs/08-harness2/01-harness2-overview.md`                  | `Proposed` | Product Maintainer | G0          |
| `docs/08-harness2/02-integration-architecture.md`           | `Proposed` | Rust/Core Owner    | G1          |
| `docs/08-harness2/03-harness-pack-spec.md`                  | `Proposed` | Product Maintainer | G1          |
| `docs/08-harness2/04-evaluation-protocol.md`                | `Proposed` | QA/Release Owner   | G2          |
| `docs/08-harness2/05-runbook.md`                            | `Proposed` | Product Maintainer | G1          |
| `docs/10-implementation/01-implementation-status.md`        | `Proposed` | Rust/Core Owner    | G0          |
| `docs/10-implementation/02-code-map.md`                     | `Proposed` | QA/Release Owner   | G1          |
| `docs/10-implementation/03-integration-findings.md`         | `Proposed` | Rust/Core Owner    | G1          |
| `docs/99-appendices/01-glossary.md`                         | `Proposed` | Product Maintainer | G0          |
| `docs/99-appendices/02-source-register.md`                  | `Proposed` | Product Maintainer | G0          |
| `docs/99-appendices/03-approval-record.md`                  | `Proposed` | Product Maintainer | G0          |

Reconciliation note: this registry was corrected by the integration pass on 2026-10-20. Every Path
cell now points at a landed file; ADR rows were remapped to the landed series topics (pagination and
projection, cache and asset store, resource budgets, licensing, session identity, error taxonomy,
Harness2 integration). The intended counts match the authoring plan: 3 plus 4 plus 5 plus 13 plus 6
plus 6 plus 4 plus 4 plus 5 plus 3 files plus the README rows.

Gate meaning for the Review gate column: it is the first gate whose exit forces this document to be
review-complete (`G0` blocked on it means it cannot lag past feasibility). Compound values name every
gate the content is accountable to; for example ADR-008 (resource budgets) is design-reviewed at
`G1`, but its claims stay unproven until `G2` soak and benchmark evidence lands.

## 3. Revision history

| Version | Date       | Change                                                                                                                                                                                                                                                            |
| ------- | ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 0.1     | 2026-10-20 | Docs set v0.1 created, derived from PRD v1.1 (`FGT-PRD-001`), in a parallel authoring pass; every file registered at Status `Proposed`; no approvals recorded yet; registry names still subject to the integration reconciliation pass                            |
| 0.1.1   | 2026-10-20 | Integration reconciliation pass: cross-reference slugs aligned to the landed filenames across `05-security/`, `06-testing/`, `07-ops/`, `02-architecture/adr/ADR-012`; registry Path and gate cells corrected to the landed ADR topics; requirement IDs unchanged |

Future changes do not rewrite history; they append rows here. Contract-affecting changes open an ADR
first (`02-architecture/adr/`), get sign-off from the owning role, then update the contracts and the
traceability matrix in the same PR (see `docs/README.md` change management). Spec-affecting changes
update the PRD source document first, then propagate here so this set never contradicts the PRD
silently. Docs-only changes go by direct PR with the format check. Every acceptance of a decision in
section 1 or a status flip in section 2 is recorded as a new revision row with the date and the role.

## 4. Review checklist

Applied to this docs set as a whole and to any docs-level PR. Items mirror PRD Appendix D.1 and add
docs-specific gates from `docs/README.md`.

- [ ] Naming is consistent: project, CLI, and metadata use FigTether everywhere; upstream references
      (`figma-mcp-free`, Harness2) appear only as attributed sources.
- [ ] Rust core and TypeScript adapter boundaries are described without claiming the whole editor
      runtime is native Rust (`D01`; plugin runtime is JavaScript per `S3`).
- [ ] Differences are preserved everywhere data is shown: source, freshness, scope, observation, and
      unsupported coverage are never blended, and cached, fresh, snapshot, and export lanes are never
      mixed in one benchmark denominator.
- [ ] No unconditional claims: no unlimited claims, no fabricated benchmark numbers, targets are
      labeled as targets (`N01..N10`), and upstream capability claims carry a source ID
      (`S1..S11`, `HS1..HS5`) and are never restated as FigTether results.
- [ ] No silent fallback to official MCP or REST endpoints exists in any design, diagram, error doc,
      or runbook (`FR19`, `AT01`, `AT02`).
- [ ] Gate honesty: any claim past `G0` feasibility is tied to its gate and `AT` IDs, and planned
      behavior is never presented as shipped behavior.
- [ ] Formatting hygiene: prettier pass clean; no em dashes anywhere; ASCII punctuation only; the
      upstream project name is written Harness2 without the superscript.
- [ ] IDs preserved verbatim: `G0..G4`, `E01..E10`, `FR01..FR24`, `N01..N10`, `AT01..AT20`, `P01..P14`,
      `R01..R14`, `D01..D12`, `S1..S11`, `HS1..HS5`; new IDs arrive with an owned namespace and are
      never reassigned.
- [ ] Diagram validity: every Mermaid block has a caption and a one-line "Read as" summary and parses
      under the `docs/README.md` conventions.
- [ ] Registry sync: added, renamed, or restatused files update section 2 in the same PR, and the
      revision history records each transition.
