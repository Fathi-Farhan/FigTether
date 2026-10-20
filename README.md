# FigTether

**Local Figma context for AI agents.** A local MCP server (Rust core) that obtains Figma design data
through a plugin running inside the Figma editor, then serves structured, size-bounded results with
explicit source, freshness, and coverage semantics to MCP clients and coding agents.

> **Status: discovery.** Gate G0 (feasibility spike) has not been executed yet. Every target,
> benchmark number, and product claim in this repository is _proposed_, not measured. The source
> requirement document is `FigTether_PRD_Confluence_v1.1` (document ID `FGT-PRD-001`, v1.1,
> 2026-10-20). Approval record: see `docs/99-appendices/03-approval-record.md`.

## Repository purpose

This workspace holds two things:

1. **The engineering documentation set** produced from the PRD: deep analysis, Agile/SDLC process,
   system design, UML, data contracts, security, testing, operations, and the Harness² integration
   design. See `docs/README.md`.
2. **The implementation** (in progress, Gate G0 stage): a Rust workspace for the core plus a
   TypeScript Figma plugin.

## Implementation

Rust workspace, edition 2021, MSRV 1.88 (toolchain pinned to 1.95.0 in `rust-toolchain.toml`):

| Crate             | Role                                                                                                                                                                                             |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `crates/model`    | Contract model: response envelope, normalized design model, tool parameters, error taxonomy, capability model. Serde + Schemars, so the published JSON Schema is generated, never written twice. |
| `crates/protocol` | Bridge protocol: frames, payload contracts, authentication handshake, chunked transfers, and the plugin payload shape. Shared by the daemon and the plugin.                                      |
| `crates/core`     | Domain rules: session and grant registry, scheduler with backpressure, bounded LRU cache, asset store, normalization, state machine, resource budgets.                                           |
| `crates/assets`   | Asset store facade used by the daemon and the CLI.                                                                                                                                               |
| `crates/daemon`   | Local daemon: loopback bridge for the plugin, authenticated client IPC for the MCP adapter and the CLI, scheduler loop, heartbeat, revoke, graceful shutdown.                                    |
| `crates/mcp`      | MCP adapter: the twelve read-only tools over stdio, using the official `rmcp` SDK (pinned to 3.6.0).                                                                                             |
| `crates/cli`      | `figtether` binary: `doctor`, `daemon`, `mcp`, `status`, `pair`, `config print`, `manifest`, `revoke`.                                                                                           |
| `crates/e2e`      | End-to-end harness: fake plugin client, fake local client, acceptance flows.                                                                                                                     |
| `crates/bench`    | Criterion workloads (W1-W5) plus the release-evidence report generator.                                                                                                                          |

The Figma plugin lives in `plugins/figma` (`main` sandbox, `ui` iframe, `shared` wire types).
Published contracts live in `schemas/`, the public fixture corpus in `fixtures/`, the criterion
workloads in `crates/bench`, and the release evidence template in `crates/bench/src/bin/report.rs`.

```sh
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check

npm run format:check                  # docs formatting
npm --prefix plugins/figma install    # plugin toolchain, once
npm --prefix plugins/figma run typecheck
npm --prefix plugins/figma test
npm --prefix plugins/figma run build

node fixtures/tools/generate.mjs       # regenerate the large fixtures
cargo run -p figtether-model --example export_schemas   # regenerate the schemas
```

Try the local stack (no Figma needed for the first two steps):

```sh
cargo run -p figtether-cli -- doctor
cargo run -p figtether-cli -- manifest --out plugins/figma/manifest.json --plugin-id "<your-id>"
cargo run -p figtether-cli -- daemon --port 38947
cargo run -p figtether-cli -- config print --client generic
```

Design rules that the code enforces, not just the docs: read-only, no official Figma MCP or REST
path in the core, explicit source/freshness/coverage on every result, bounded queues, caches,
transfers, and response sizes, fail-closed authentication, and scope enforcement on both the daemon
and the plugin side.

## Documentation map

| Area          | Path                               | Contents                                                                                                       |
| ------------- | ---------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| Start here    | [`docs/README.md`](docs/README.md) | Reading order, conventions, document statuses                                                                  |
| Analysis      | `docs/00-overview/`                | PRD deep analysis, competitive landscape, personas and journeys                                                |
| Process       | `docs/01-agile/`                   | SDLC Agile framework, product backlog, sprint roadmap, risk register                                           |
| Design        | `docs/02-architecture/`            | System design (C4), bridge, MCP, plugin, state machines, ADR 001-012                                           |
| UML           | `docs/03-uml/`                     | Use case, class, sequence, state, activity, deployment diagrams (Mermaid)                                      |
| Contracts     | `docs/04-data-contracts/`          | MCP envelope, tool catalog, bridge protocol, normalized model, errors, resource limits                         |
| Security      | `docs/05-security/`                | STRIDE threat model, access control, privacy and data handling                                                 |
| Quality       | `docs/06-testing/`                 | Test strategy, acceptance catalog AT01-AT20, traceability matrix, benchmark plan                               |
| Operations    | `docs/07-ops/`                     | Dev environment, end-user install, runbook, release process                                                    |
| Agent harness | `docs/08-harness2/`                | Harness² (Google Research) overview, integration architecture, harness pack spec, evaluation protocol, runbook |
| Reference     | `docs/99-appendices/`              | Glossary, source register, approval record                                                                     |

## Tooling

The docs are formatted with **Prettier**. From the repository root:

```sh
npm install
npm run format          # write formatting fixes
npm run format:check    # CI-style check, exit 1 on drift
```

Conventions enforced across all documents:

- No em dashes (project-wide rule inherited from the PRD style guide).
- Required IDs are preserved verbatim: `G0..G4`, `E01..E10`, `FR01..FR24`, `N01..N10`, `AT01..AT20`,
  `P01..P14`, `R01..R14`, `D01..D12`. Docs must never renumber or reassign them.
- Diagrams are Mermaid, renderable in VS Code (Mermaid preview) and GitHub.
- Every implementation or claim must trace to at least one `FR*` or `AT*` ID. See
  `docs/06-testing/03-traceability-matrix.md`.

## Licensing

Proposed dual license `MIT OR Apache-2.0`. Name/trademark and dependency license review are pending
(decision `D10`). See `docs/02-architecture/adr/ADR-009-licensing.md`. FigTether is not affiliated
with Figma.
