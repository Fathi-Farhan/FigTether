# FigTether Documentation

This folder is the complete engineering documentation set for FigTether v0.1 and beyond, produced
from `FigTether_PRD_Confluence_v1.1` (`FGT-PRD-001`). It is written **before implementation**, under
an Agile SDLC whose increments are the PRD gates `G0` (feasibility) through `G4` (v0.3+).

## Reading order

1. `00-overview/01-prd-analysis.md` - what the product is, what the PRD promises, where it is
   ambiguous, and which interpretations this doc set commits to.
2. `01-agile/01-sdlc-process.md` - how the work is organized (sprints, gates, DoR/DoD, CI).
3. `02-architecture/01-system-design.md` - the system, then detailed sub-designs and ADRs.
4. `03-uml/` - the same architecture expressed as UML (use case, class, sequence, state, activity,
   deployment).
5. `04-data-contracts/` - frozen-after-G0 surface: tools, envelopes, bridge frames, errors, limits.
6. `05-security/` - threat model and privacy rules that block release.
7. `06-testing/` - how every `FR*` becomes observable evidence (`AT*`), plus benchmarks `N01..N10`.
8. `07-ops/` - setup for developers and end users, runbook, release process.
9. `08-harness2/` - how FigTether plugs into an agent harness and how the Harness² methodology
   (Google Research) is used to keep improving that harness.
10. `10-implementation/` - what the code implements, which invariants are enforced, and the
    open gaps between the running system and the PRD.
11. `99-appendices/` - glossary, source register, approval record.

## Folder index

| Folder               | Focus                                                  | Primary audience                  |
| -------------------- | ------------------------------------------------------ | --------------------------------- |
| `00-overview/`       | PRD analysis, landscape, personas                      | Product maintainer, all engineers |
| `01-agile/`          | SDLC Agile: sprints, backlog, roadmap, risks           | Project lead, eng, QA             |
| `02-architecture/`   | System design, containers, ADRs                        | Rust/plugin engineers, reviewers  |
| `03-uml/`            | UML diagrams (Mermaid source)                          | Engineers, reviewers              |
| `04-data-contracts/` | Wire and product contracts                             | Rust + TypeScript engineers       |
| `05-security/`       | Threat model, access control, privacy                  | Security reviewer, core owner     |
| `06-testing/`        | Strategy, acceptance tests, traceability, benchmarks   | QA, eng, product                  |
| `07-ops/`            | Environments, installs, runbook, releases              | Maintainers, support, users       |
| `08-harness2/`       | Agent harness design + Harness² evaluation             | Maintainers, product, evaluation  |
| `10-implementation/` | Code to design mapping, enforced invariants, open gaps | Engineers, reviewers, product     |
| `99-appendices/`     | Glossary, sources, approvals                           | Everyone                          |

Note: the PRD sketches a future repository layout `docs/{adr,runbooks,release-evidence}` alongside
`crates/` and `plugins/`. Those paths will be created at implementation time; they mirror content
from `docs/02-architecture/adr/`, `docs/07-ops/03-runbook.md`, and
`docs/07-ops/04-release-process.md` respectively.

## Conventions

- **Language.** English with PRD vocabulary preserved. Key Indonesian-source terms (grant, root,
  scope, epoch) keep their PRD meaning.
- **IDs.** `G0..G4` gates, `E01..E10` epics, `FR01..FR24` functional requirements, `N01..N10` NFR
  targets, `AT01..AT20` acceptance tests, `P01..P14` problem evidence, `R01..R14` risks, `D01..D12`
  PRD decisions. IDs are stable and must not be reassigned. Documents may mint doc-local ID
  families with a scoped prefix, for example `FX-*` fixtures, `US-*` backlog stories, `CI-*` CI
  gates, `L1..L10` test layers, `W1..W5` benchmark profiles, `UC-*` use cases, `TH-*` threats,
  `SK-*` and `PR-*` harness playbooks and policies; doc-local IDs never collide with PRD families
  and live only inside their owning file.
- **Formatting.** Prettier (`npm run format`). No em dashes anywhere. Headings use `##` numbered
  folders; document titles use `#`.
- **Diagram format.** Mermaid blocks. Each diagram has a caption and a one-line "Read as" summary.
- **Status labels.** `Proposed` / `Approved` / `Implemented-in-PRD-only` are tracked per document in
  the approval record (`99-appendices/03-approval-record.md`).

## Change management

1. Contract-affecting change: open an ADR first (`02-architecture/adr/`), get sign-off from the
   owning role, then update contracts and the traceability matrix in the same PR.
2. Spec-affecting change: update the PRD source document, then propagate here; never let this doc
   set contradict the PRD silently.
3. Docs-only change: direct PR, CI runs `npm run format:check`.

## What is deliberately missing

- Benchmark results: every performance number is a target until measured (`N01..N10`, PRD section
  2.1). Reports land in `07-ops/04-release-process.md` evidence slots per gate.
- Version pins for the MCP Rust SDK (`D08`), transport choice (`D06`), and variable API coverage
  (`D09`): documented as open ADR criteria, resolved only by G0 evidence.
