# Implementation status

FigTether has a working automated test environment, not a verified Figma Desktop release.
This page separates those two claims. The design reference describes the intended product;
it must not be read as proof that every feature is available.

## Implemented and tested locally

| Component                                                        | Evidence                                                                                   |
| ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| Response model and wire types                                    | Serialization tests and generated schemas in `crates/model`, `crates/protocol`, `schemas/` |
| Registry, grants, scheduler, cache, normalizer                   | Unit tests in `crates/core`                                                                |
| Loopback daemon, native control guard, first-frame auth          | Daemon integration tests and `crates/e2e/tests/adversarial.rs`                             |
| Request routing, pagination metadata, cache/asset writes, revoke | `crates/daemon/tests/read_path.rs`; shipped-service acceptance scenarios                   |
| MCP initialization and 12 advertised tool schemas                | `crates/mcp/src/handshake.rs` and the shipped-adapter acceptance scenario                  |
| CLI argument handling, manifest/config output, diagnostics       | Tests in `crates/cli`                                                                      |
| Plugin sandbox and UI message relay                              | Vitest with a fake Plugin API and real runtime/controller classes                          |
| Offline performance diagnostics                                  | Workloads in `crates/bench`, using the actual Rust core                                    |

The reference daemon under `crates/e2e/src/daemon.rs` simulates features and faults not present
in the shipped service. Passing a test against it is not evidence that the shipped daemon has
the same feature. The ignored editor acceptance test still requires a manual Figma session.

## Not ready or unverified

| Area                      | Current boundary                                                                                                                                                     |
| ------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Desktop setup             | Figma Desktop import, iframe asset loading, CSP/Origin behavior, and per-seat capabilities need real editor tests on each claimed platform.                          |
| Snapshots/offline bundles | The plugin can construct a capture, but the daemon has no snapshot store. `get_snapshot_info` returns `CAPABILITY_UNAVAILABLE`.                                      |
| Watch/resync              | No supported watch or change journal. The plugin handlers return unavailable.                                                                                        |
| Asset delivery            | Export handlers and an in-memory store exist; protected MCP resource/download delivery is incomplete.                                                                |
| Chunked responses         | Oversized announcements are rejected; full bounded reassembly and completion still need implementation/validation in the shipped daemon.                             |
| Pause/resume              | The UI sends a mode extension. Verify daemon-side mode handling and grant revalidation before treating this as an effective control. Revoke has regression coverage. |
| Reconnect                 | UI retries exist. Session-credential reconnection and editor sleep/wake behavior are not a validated workflow.                                                       |
| Custom ports              | The CLI accepts a port, but the plugin UI endpoint is hardcoded to 38947. A manifest alone cannot configure it.                                                      |
| Windows credentials       | Runtime-directory permission checking is best-effort, not a verified owner-only DACL implementation.                                                                 |
| Scope freshness           | Daemon membership uses previously observed parent relationships; fresh reparenting/deletion and hostile response content need further independent review.            |
| Client isolation          | Local processes share an installation credential. This is not isolation from another process with the same OS user rights.                                           |
| Interoperability          | Automated stdio tests are not a compatibility matrix of two real MCP clients.                                                                                        |
| Performance               | The published diagnostic excludes Figma, network, and AI inference. No live NFR target is established.                                                               |
| PentAGI                   | Assessment prompts/scripts exist; no autonomous assessment or external audit has been completed.                                                                     |
| Releases                  | Workflows build candidates only. Hosted CI, signing, and actual distribution artifacts need separate verification.                                                   |

## Verify this checkout

```sh
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo fmt --all --check
npm --prefix plugins/figma run build:types
npm --prefix plugins/figma run typecheck
npm --prefix plugins/figma test
npm run public:check
```

Use the [quickstart](../quickstart.md) for the experimental desktop workflow. A production release
requires resolving the relevant gaps, recording editor/client versions and commit hashes, and
reviewing the release evidence. Tests alone are not a security certification.
