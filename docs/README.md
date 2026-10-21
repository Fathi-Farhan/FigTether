# Documentation

## Start here

For agent-driven canvas work, use the [canvas-first workflow](agent-workflow.md). It explains
automatic MCP guidance, the focused design skill, and the approval/edit/visual-check loop.

| I want to...                              | Read                                                                                                        |
| ----------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| Install and make a first design           | [README tutorial](../README.md#build-and-connect) and [Quickstart](quickstart.md)                           |
| Connect Codex, Gemini CLI, or Claude Code | [Agent setup and readiness checks](agent-setup.md)                                                          |
| Understand pairing and the Bridge UI      | [Illustrated Bridge guide](bridge-ui-guide.md)                                                              |
| Verify native design creation and edits   | [Rust write-path verification](write-path-verification.md)                                                  |
| Build, test, or send a patch              | [Contributing](../CONTRIBUTING.md)                                                                          |
| Understand the process boundaries         | [Architecture diagram](../assets/architecture.svg) and [system design](02-architecture/01-system-design.md) |
| Check what is implemented                 | [Implementation status](10-implementation/01-implementation-status.md)                                      |
| Reproduce the README chart                | [Published diagnostic data](../benchmarks/published/README.md)                                              |
| Report a bug or vulnerability             | [Support](../SUPPORT.md) / [security policy](../SECURITY.md)                                                |
| Prepare the repository for publication    | [Publication checklist](maintainers/publication.md)                                                         |

## Design reference

Use [current documentation status](documentation-status.md) to resolve historical proposal
language against implemented behavior. The [design review guide](design-review.md) links
the reusable skill, deterministic rule contract, primary-source research, and browser evidence.

These documents preserve the original requirements and design decisions. Much of the material
was written before implementation and remains **proposed**, not a list of working features.
In particular, snapshot storage, watch/resync, offline bundles, asset-resource delivery, and
desktop compatibility are not proven by the diagrams or acceptance catalog. The write path is
implemented and has limited Windows/Figma Desktop evidence: the agent created and read back a
native website design through approved plans. Its original ADR remains a proposal for a future
release; that document status should not be confused with absence of the implementation. See
the [code verification](write-path-verification.md) and [desktop report](../benchmarks/published/desktop-2026-10-20.md).

| Area                                                                  | Contents                                                                                                                                                  |
| --------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [Overview](00-overview/01-prd-analysis.md)                            | Product goals, scope, assumptions, alternatives                                                                                                           |
| [Agile delivery](01-agile/01-sdlc-process.md)                         | Backlog, relative roadmap, risks, review gates                                                                                                            |
| [Architecture](02-architecture/01-system-design.md)                   | Rust/TypeScript boundaries, transport, state machines                                                                                                     |
| [Decisions](02-architecture/adr/ADR-000-template.md)                  | ADRs and the template for changing a contract                                                                                                             |
| [UML](03-uml/01-use-case-diagrams.md)                                 | Use cases, classes, sequences, states, deployment                                                                                                         |
| [Contracts](04-data-contracts/01-mcp-envelope.md)                     | Envelope, tool surface, wire protocol, normalized model, budgets                                                                                          |
| [Security](05-security/01-threat-model.md)                            | Threat model, access control, privacy, [PentAGI plan](05-security/04-pentagi-validation.md)                                                               |
| [Testing](06-testing/01-test-strategy.md)                             | Acceptance catalog, traceability, benchmark protocol                                                                                                      |
| [Write path](11-write-path/01-write-path-design.md)                   | Plan, preview, approval, apply, and undo contracts of the proposed write path (`ADR-013`), published as JSON Schema in [`schemas/`](../schemas/README.md) |
| [Operations](07-ops/03-runbook.md)                                    | Diagnostics, recovery, planned release process                                                                                                            |
| [Harness² research](08-harness2/01-harness2-overview.md)              | Optional evaluation design; not an installed product feature                                                                                              |
| [Implementation notes](10-implementation/01-implementation-status.md) | Code map, limitations, integration history                                                                                                                |
| [Reference](99-appendices/01-glossary.md)                             | Glossary, sources, review record                                                                                                                          |

The source PRD is identified as `FGT-PRD-001`, version 1.1. It is not shipped as an external
Word document or a path into a maintainer's workspace. Relevant requirements are preserved here;
a contributor should not need that external file to build or test a change.

## Reading the status labels

- **Proposed** means a design or process is awaiting review. It is not implementation evidence.
- **Accepted** in an ADR records a design decision, not a release certification.
- **G0** is the feasibility/editor-validation gate; it is still open. Later release gates are
  plans, not announced dates.
- `FR*`, `N*`, and `AT*` are stable requirement, target, and acceptance-test identifiers. Keep
  them when editing design references; don't require them in every ordinary bug report.

Wire behavior comes from the Rust types, [generated schemas](../schemas/README.md), shared
TypeScript contracts, and tests. If a design document disagrees with code, record the discrepancy
and resolve it explicitly. Don't silently turn a desired property into a claim about this build.

## Editing

Use relative links, concrete commands, and a clear statement of what's untested. Put longer
design discussions here, not in the root README. Run `npm run format` and `npm run public:check`
after changes. Mermaid source stays in Markdown; README SVGs are generated with
`node scripts/render-readme-assets.mjs`.

Keep `docs/` tracked. It contains reviewable design history and helps contributors understand
security boundaries. Local notes containing customer data, raw logs, and private incident reports
belong outside it. The [publication policy](maintainers/publication.md#what-belongs-in-git)
lists what to keep and what to ignore.
