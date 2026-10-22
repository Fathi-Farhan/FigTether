# Contributing to FigTether

Useful contributions include small bug fixes, protocol tests, clearer diagnostics, and reports
from real Figma Desktop sessions. The bridge is experimental. Please use a synthetic design or
one of the [public fixtures](fixtures/README.md), never a client's file.

For a suspected vulnerability, use [SECURITY.md](SECURITY.md) instead of a public issue.

## Set up a checkout

Write public documentation, UI help, examples, and research notes in English. Keep historical
decisions distinct from current behavior: link to [documentation status](docs/documentation-status.md)
and measured evidence when changing capability claims. Research-derived design checks must
state their inputs, exceptions, and limits; visual style is not evidence of AI authorship.

Install git, [rustup](https://rustup.rs/), and Node.js **22.14 or newer** with npm. On Windows,
Rust's MSVC target also needs the Visual Studio C++ build tools. `just` is optional.

From the repository root:

```sh
rustup show
rustup component add rustfmt clippy
npm ci
npm --prefix plugins/figma ci
cargo build --workspace --locked
npm --prefix plugins/figma run build:types
npm --prefix plugins/figma run build
```

`rust-toolchain.toml` pins the tested compiler. The workspace declares Rust 1.88 as its intended
minimum, but that minimum is not independently verified. Use the pinned toolchain for PRs.
Keep `Cargo.lock` and the npm lockfiles committed; do not regenerate them just to fix formatting.

Agent guidance comes from `integrations/figtether/workflow.json`. After changing it, run
`npm run agents:generate`; commit the generated MCP instructions and both SKILL.md copies.
`npm run agents:check` catches drift. Rebuild the CLI to embed updated guidance, then refresh
managed host skills with `figtether setup --client YOUR_HOST --apply` in their workspaces.
Do not rewrite dated test results to describe a new build; record new evidence separately.

To run the desktop integration, follow the [quickstart](docs/quickstart.md). Keep a local
manifest in `plugins/figma/manifest.local.json` so your plugin ID does not modify the shared example.

## Find the right component

| Path                   | Responsibility                                                    |
| ---------------------- | ----------------------------------------------------------------- |
| `crates/model`         | Response envelopes, normalized data, tool parameters, error codes |
| `crates/protocol`      | Plugin bridge and local client wire contracts                     |
| `crates/core`          | Session/grant rules, scope index, scheduler, cache, normalization |
| `crates/assets`        | Asset-store facade                                                |
| `crates/daemon`        | Loopback endpoints, authentication, request routing, cursors      |
| `crates/mcp`           | MCP lifecycle, tool schemas, result/error mapping                 |
| `crates/cli`           | Commands, configuration output, diagnostics, manifest generation  |
| `crates/e2e`           | Simulated peers and shipped-daemon integration/adversarial tests  |
| `crates/bench`         | Offline workloads and measurement reports                         |
| `plugins/figma/main`   | Document reads through the Figma Plugin API                       |
| `plugins/figma/ui`     | Pairing, controls, status, WebSocket relay                        |
| `plugins/figma/shared` | Shared TypeScript contracts and encoding helpers                  |

The core must not call Figma REST or official MCP endpoints. The sandbox must not open network
connections. Read handlers must not mutate nodes, selection, or the current page. Keep secrets
out of tool arguments, logs, error messages, and fixtures.

## Run the checks

These commands work without a Figma account or an LLM API key:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
npm run format:check
npm run public:check
npm run public:test
npm run ci:test
npm run design:test
npm --prefix plugins/figma run build:types
npm --prefix plugins/figma run typecheck
npm --prefix plugins/figma run lint:all
npm --prefix plugins/figma test
npm --prefix plugins/figma run build
```

For a focused Rust change, start with `cargo test -p figtether-core` or the affected crate.
For a plugin change, use `npm --prefix plugins/figma test -- main` or `-- ui`.

Existing `scripts/check.sh` and `scripts/check.ps1` cover the Rust and documentation checks;
run the plugin checks separately. `just --list` shows optional task aliases. On Windows, the
shell recipes require Git Bash; the PowerShell scripts are the alternative.

### Tests that exercise the real services

```sh
cargo test -p figtether-e2e --test acceptance
cargo test -p figtether-e2e --test adversarial
```

Some acceptance scenarios use an in-process reference daemon to simulate faults; others exercise
the shipped daemon and MCP adapter. Do not cite a reference-harness result as proof about the
shipped service. The ignored real-editor scenario is a manual gate, not a passing test.

When reporting a desktop test, record OS/architecture, Figma build, commit, plugin manifest,
fixture, steps, and outcome. Never remove a failing scenario just to make a support claim green.

## Change a contract

A published schema, tool signature, bridge frame, grant rule, error code, resource budget, or
credential lifetime is a contract. Discuss the change first, then add or amend an
[architecture decision record](docs/02-architecture/adr/ADR-000-template.md).

1. Change the Rust types in `model` or `protocol`.
2. Update their TypeScript counterparts in `plugins/figma/shared`; do not add a second copy in
   `main` or `ui`.
3. Update serialization and boundary tests, including a rejected-input case.
4. Regenerate schemas and review the diff:

   ```sh
   cargo run --locked -p figtether-model --example export_schemas
   ```

5. Run `sh scripts/schema-check.sh` or `pwsh -File scripts/schema-check.ps1`.
6. Update the relevant design document, compatibility notes, and
   [traceability matrix](docs/06-testing/03-traceability-matrix.md).

The schema exporter still includes checked mirrors for some types; it verifies them against the
Rust source. If it rejects a change, update the mirror and its checks rather than bypassing them.
Generated `schemas/*.json` are tracked but excluded from Prettier to keep regeneration byte-stable.

## Benchmarks and visuals

Use [the benchmark guide](benchmarks/README.md). Cached reads, normalization, asset storage,
editor rendering, and model inference are different measurements; do not combine their numbers.
Commit, fixture hash, sample count, environment, build profile, and exclusions belong beside
every published chart. One diagnostic run is not a release benchmark or a regression verdict.

The README artwork is generated locally, with no image service:

```sh
node scripts/render-readme-assets.mjs
node scripts/render-readme-assets.mjs --check
```

The chart reads [reviewed data](benchmarks/published/offline-stages.json), not raw local logs.
Only replace that data with a new run after checking it for secrets and private machine paths.
Keep unpublished measurements and assessment reports ignored.

## Write documentation people can use

- Put install and usage instructions in the [quickstart](docs/quickstart.md), not in an ADR.
- Use relative Markdown links so forks work without changing repository URLs.
- Label design proposals as proposals. A requirement ID is not proof of implementation.
- Prefer an example and its recovery action to a long list of promises.
- Run `npm run format`; keep ASCII hyphens in prose, as required by the existing docs checks.

The [design archive](docs/README.md#design-reference) keeps requirement IDs stable. Ordinary typo
fixes and small documentation improvements do not need a new requirement or a design review.

## Open a pull request

For a small fix, include the bug, the change, and the command that proves it. For larger work,
open an issue first so the scope and compatibility impact are clear.

Every pull request runs the [CI quality checks](docs/ci.md), including Rust tests across
Linux, Windows, and both macOS architectures. `CI required` fails if any prerequisite fails,
is cancelled, or is skipped. Maintainers must configure it as a required GitHub status check
to prevent merges; workflow YAML alone does not enforce that repository setting.

Before requesting review:

- [ ] One problem per PR, with unrelated formatting or dependency changes left out.
- [ ] A regression test for changed behavior, including a negative case at a trust boundary.
- [ ] Relevant checks above pass; list anything you could not run.
- [ ] Contract changes include schema/TypeScript updates and an ADR.
- [ ] User-visible changes appear under `Unreleased` in [CHANGELOG.md](CHANGELOG.md).
- [ ] No design data, credentials, local runtime files, or generated bundles in the diff.

There is no fixed line-count limit. Split a change when doing so makes it easier to review, not
to meet an arbitrary number. Do not publish a security fix or an exploit PR before coordinating
privately with maintainers.

## Dependencies and licensing

New dependencies need a reason, compatible licensing, and a lockfile update. Optional tools:

```sh
cargo install cargo-audit cargo-deny
cargo audit
cargo deny check
npm audit
npm --prefix plugins/figma audit
```

Don't hide an advisory with a broad allowlist; document the affected path and the resolution.
Contributions are made under the project's **MIT OR Apache-2.0** license unless explicitly
agreed otherwise. Retain third-party notices. The [code of conduct](CODE_OF_CONDUCT.md) applies.
