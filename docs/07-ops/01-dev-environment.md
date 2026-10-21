# Developer Environment Setup

> **Design reference.** Original requirements and approval labels below are historical design
> records, not a feature-availability list. See [current documentation status](../documentation-status.md)
> for implemented reads/writes, observed desktop evidence, and remaining gaps.

Status: `Proposed`

This document describes how a contributor prepares a workstation to build and test the FigTether
workspace: host prerequisites, the intended workspace scaffold, the local development loops for
Rust, TypeScript, contracts, and documentation, the proposed manifest generation workflow, and a
local run of the full stack. Everything below is written before Gate `G0` has run: the `figtether`
subcommands shown are the **proposed CLI surface** from PRD section 8.4, not shipped behavior.

## 1. Prerequisites

| Component           | Minimum                                                                     | Reason                                                                                                                                  |
| ------------------- | --------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| rustup              | Installed via the official installer                                        | Pin and switch Rust toolchains per project                                                                                              |
| Rust stable         | Stable channel, pinned to the MSRV recorded at `G0` per `D08` and `ADR-003` | Build and test crates `cli`, `daemon`, `mcp`, `core`, `protocol`, `model`, `assets`; the MSRV is recorded once at `G0`, never guessed   |
| Node.js             | Active LTS for plugin TS and the bundler                                    | Compile `plugins/figma/main` and `plugins/figma/ui`; `D01` keeps the adapter TypeScript-only                                            |
| npm                 | Bundled with Node LTS (doc tooling only today)                              | Run Prettier formatting for this doc set; plugin scripts appear with the `E01` scaffold                                                 |
| Python 3.12 with uv | Optional                                                                    | Harness experiments referenced by `docs/08-harness2/`; not required for core or plugin work                                             |
| git                 | Any current version                                                         | Source of truth; release tags must point to the tested commit per PRD 8.11                                                              |
| Figma Desktop       | Current build on Windows or macOS with a plugin-capable seat                | Real editor evidence is mandatory; a licensed Starter seat that supports Design plugins must be verified at runtime (`S2`), not assumed |

No installation step requires administrator rights by default, per PRD 8.11. End users never need
Rust or Node (FR01); contributors do.

## 2. Workspace scaffold

The PRD section 8.4 sketches this layout. Crates are created only after `G0` evidence lands
(critical path `E01` onward). The right column maps every path to the document that governs it.

| Path                    | Purpose                                                                                                                  | Mapped documentation                                                                     |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------- |
| `crates/cli`            | One binary exposing the proposed subcommands `doctor`, `daemon`, `mcp`, `status`, `pair`, `config`, `manifest`, `revoke` | `docs/07-ops/01-dev-environment.md`, `docs/07-ops/02-end-user-install.md`                |
| `crates/daemon`         | Loopback server, grant registry, scheduler, cache lifecycle                                                              | `docs/02-architecture/01-system-design.md`                                               |
| `crates/mcp`            | MCP adapter over stdio, one process per client                                                                           | `docs/04-data-contracts/01-mcp-envelope.md`, `docs/04-data-contracts/02-tool-catalog.md` |
| `crates/core`           | Normalization, scope validation, scheduler, cache                                                                        | `docs/04-data-contracts/04-normalized-model.md`                                          |
| `crates/protocol`       | Bridge framing, handshake, heartbeat                                                                                     | `docs/04-data-contracts/03-bridge-protocol.md`                                           |
| `crates/model`          | Shared contract types for Rust                                                                                           | `docs/04-data-contracts/`                                                                |
| `crates/assets`         | Asset store with size and MIME discipline                                                                                | `docs/04-data-contracts/06-resource-limits.md`                                           |
| `plugins/figma/main`    | Plugin main sandbox, allowlisted read handlers                                                                           | `docs/02-architecture/`                                                                  |
| `plugins/figma/ui`      | UI iframe: pairing, grant, status, revoke                                                                                | `docs/02-architecture/`                                                                  |
| `schemas/`              | JSON schema source of truth for Rust and TypeScript                                                                      | `docs/04-data-contracts/`                                                                |
| `fixtures/`             | Public, non-private curated fixtures for tests                                                                           | `docs/06-testing/01-test-strategy.md`                                                    |
| `benchmarks/`           | Benchmark procedures and result slots                                                                                    | `docs/06-testing/04-benchmark-plan.md`                                                   |
| `docs/adr`              | Implementation-time ADR index                                                                                            | Mirrors `docs/02-architecture/adr/`                                                      |
| `docs/runbooks`         | Implementation-time runbooks                                                                                             | Mirrors `docs/07-ops/03-runbook.md`                                                      |
| `docs/release-evidence` | Release evidence records per Appendix C template                                                                         | Mirrors `docs/07-ops/04-release-process.md`                                              |

Crate boundaries may be simplified during implementation as long as domain separation survives; the
PRD allows this explicitly.

## 3. Local development loops

### 3.1 Rust core loop

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release
```

`cargo clippy` warnings fail the build because PRD 8.11 wires `fmt` and `clippy` into required CI.

### 3.2 TypeScript plugin loop

The doc tooling package currently only carries Prettier (`format`, `format:check`). After the `E01`
plugin scaffold lands, the expected scripts are `typecheck`, `lint`, `test`, and `build` in the
plugin workspace. The intended loop once scaffolded:

```sh
npm ci
npm run typecheck
npm run lint
npm run build
```

These script names are proposed; do not cite them as existing until the scaffold lands.

### 3.3 Contract and schema loop

Schemas under `schemas/` are the cross-language contract: Rust types and TypeScript DTOs must both
round-trip them. The required test class is a schema round-trip suite that serializes a fixture,
deserializes it into both languages, and re-serializes for byte comparison per the canonicalization
rules in `docs/04-data-contracts/01-mcp-envelope.md`:

```sh
cargo test -p figtether-protocol schema_roundtrip
```

### 3.4 Documentation loop

Docs are Prettier-formatted at the repository root, config: `printWidth 100`, `proseWrap preserve`,
`endOfLine lf`:

```sh
npm run format
npm run format:check
```

`docs/README.md` change management expects `format:check` to pass on every docs PR.

## 4. Manifest generation workflow (proposed)

Development manifests with a user-owned plugin id are the distribution mode for v0.1 per FR01 and
`D11` (Community publication is deferred). The proposed command shape:

```sh
figtether manifest generate --plugin-id "<your-plugin-id>" --port 38947
```

Rules binding this workflow:

1. The generated development manifest allows only loopback addresses and the needed port,
   `documentAccess: dynamic-page`, and `editorType: ["figma"]` per PRD 8.6.1.
2. Regenerate the manifest whenever the daemon port changes. PRD 5.1 step 3 makes this explicit:
   a port collision must be resolved by choosing an explicit port **and** regenerating the manifest,
   never by silently changing ports under the old manifest.
3. The release bundle ships the prebuilt UI and adapter; contributors building from source use the
   same manifest flow with their own plugin id.

## 5. Running the stack locally

Full local run, in order. All commands are the proposed surface:

```sh
figtether doctor
figtether daemon
figtether daemon --port 38948
figtether mcp --stdio
figtether status
```

Expected behavior:

| Step                            | Expected local behavior                                                            | Checks performed                                                                                                                |
| ------------------------------- | ---------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `figtether doctor`              | Sanitized report, exit 0 when healthy                                              | Versions, runtime directory ACL, bound port, manifest presence and port match, pairing and session state, detected capabilities |
| `figtether daemon`              | Binds `127.0.0.1:38947` (project default, loopback only), waits for plugin pairing | Nothing serves before authentication                                                                                            |
| `figtether daemon --port 38948` | Override example; requires manifest regeneration afterwards                        | Fails loudly on collision with `--port` guidance; never silently falls back                                                     |
| `figtether mcp --stdio`         | MCP lifecycle over stdio; stdout carries protocol only                             | Handshake against the pinned rmcp version per `D08`                                                                             |
| `figtether status`              | Session, grant, queue state summary                                                | Matches plugin UI status                                                                                                        |

`doctor` output is sanitized per FR18: versions, resource counts, and state, but never design
content, layer names, file ids, or credentials.

## 6. Troubleshooting

| Symptom                                              | Likely cause                                                                              | Recovery                                                                                                                                                                                          |
| ---------------------------------------------------- | ----------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `BIND_FAILED` style report; daemon exits on startup  | Port `38947` taken by another process                                                     | Identify the owner only through what the OS reveals without privilege elevation; either stop it or run `figtether daemon --port <other>` and regenerate the manifest for the new port per PRD 8.7 |
| Daemon fails with runtime directory permission error | ACL/POSIX permissions on the runtime directory are too permissive or unusable             | Fix ownership to the current user; doctor re-checks ACLs and must fail closed when the directory is inadequate per PRD 8.6.2                                                                      |
| Plugin never connects                                | Manifest absent, stale port, plugin not running, or wrong build deployed to Figma Desktop | Rerun `figtether doctor`, regenerate the manifest on a port mismatch, redeploy the plugin build, and re-pair                                                                                      |
| `UNSUPPORTED_SCHEMA` or `PROTOCOL_MISMATCH` in reads | Plugin, daemon, or MCP client on incompatible versions                                    | Rerun `figtether doctor`; it prints known-compatible versions. Align plugin bundle and daemon to the same release before continuing                                                               |

If a local runtime violation involves authentication or scope, stop and read
`docs/05-security/01-threat-model.md` before investigating further; scope bugs are release blockers
per `R05`.

## 7. Open questions

1. Exact output format of `figtether doctor` (machine-readable section names) is to be fixed with
   the `E01` CLI ADR.
2. Whether plugin tooling is a workspace member of the root package or a nested package; affects
   the TypeScript script names above.
3. MSRV value and rmcp pin land only from `D08` and `ADR-003` at `G0`; this doc records them there.
