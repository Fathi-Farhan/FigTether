# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this
project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The core and the CLI follow SemVer. The protocol version and the schema version are separate
streams: a core patch release never moves them. Unknown optional fields are tolerated by
every consumer, and an unsupported major version is rejected at runtime rather than
half negotiated.

## [Unreleased]

### Added

- Complete engineering documentation set derived from PRD v1.1 (`FGT-PRD-001`): PRD analysis,
  Agile SDLC process with the `G0` to `G4` gates, system design with the C4 views, UML diagrams,
  data contracts, security threat model, test strategy with the `AT01` to `AT20` acceptance
  catalog, traceability matrix, benchmark plan, operations runbook, and the Harness2 integration
  design with a harness pack specification.
- Implementation status document, requirement to code map, and the integration findings record
  (`docs/10-implementation/`).
- Requirement traceability from `FR01` to `FR24` and `N01` to `N10` down to the file that
  implements them and the test that proves them.

### Fixed

- The plugin message path is repaired end to end: one message vocabulary, the request id survives
  every hop, the UI forwards sandbox answers to the daemon, daemon request frames reach the
  sandbox, and the `pluginMessage` wrapper is unwrapped before validation.
- The contract that crosses the plugin package boundary is declared once, in the shared package,
  with a guard test that fails on drift.
- The benchmarks measure the shipped `figtether-core` implementation instead of a local copy.
- The daemon completes a read end to end: the session reaches `Ready` after a grant, the operation
  of a request travels with the job, a truncated traversal reports `truncated` and an opaque
  cursor, the cache and the asset store are written, and every job ends in a terminal state so a
  lane is never held forever.
- A local client sees and can revoke the sessions of the plugin that paired on the same
  installation, without introducing a privileged list every session call.
- `figtether doctor` reports the plugin capability report as a real check.
- The asset store returns the hash in the same `sha256:` form as the response envelope.

### Known limitations

- Gate `G0` is not closed. Every plugin side test runs against a fake plugin client, and no claim
  about a real Figma Desktop session is made anywhere in this repository.

## [0.1.0] - Unreleased

The initial release candidate. Gate `G0` has not been closed, so this entry stays open and the
section is a placeholder until the release candidate checklist in
`docs/07-ops/04-release-process.md` is complete.

### Added

- Rust workspace with the `model`, `protocol`, `core`, `assets`, `daemon`, `mcp`, `cli`, `e2e`,
  and `bench` crates.
- Published JSON Schema for every published contract, generated from the Rust model.
- Figma plugin adapter and plugin UI in `plugins/figma/`.
- CI pipeline with the Rust format, lint, and test jobs, the schema contract check, the
  TypeScript job, the documentation format check, the dependency audit, the SBOM, and the
  license check.
- Release pipeline that builds the CLI for Windows x64, macOS x64, and macOS arm64 from the
  tagged commit, and produces checksums, an SBOM, and a provenance record.

### Known limitations

- The project has not run Gate `G0`. No measured performance number is published yet.
- Community plugin publication is deferred to Gate `G4`. The distribution path is a
  development manifest with the user's own plugin id.

[Unreleased]: https://github.com/<owner>/<repo>/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/<owner>/<repo>/releases/tag/v0.1.0
