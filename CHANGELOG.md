# Changelog

User-visible changes are listed here. No release has been published yet. Version `0.1.0` in the
workspace is a development version, not a download or a tested compatibility promise.

## Unreleased

### Added

- Rust daemon, MCP stdio adapter, and CLI for a local, read-only Figma bridge.
- Development plugin with pairing, root grants, scoped reads, and PNG/SVG export handlers.
- Shared TypeScript wire contracts, generated JSON schemas, and synthetic fixtures.
- Unit, integration, and adversarial tests plus offline benchmark workloads.
- CI and release-candidate build definitions, license checks, and SBOM tooling.
- Public quickstart, contributor guide, architecture illustration, and a reproducible offline chart.
- Optional Harness² evaluation design and PentAGI assessment scripts.
- Write path, implemented across the Rust crates and the plugin and proposed for a future
  release: declarative write plans, a preview that resolves against the document without changing
  it, a separate write grant, a per plan approval taken in the plugin UI, write budgets, a content
  free audit record, and one editor undo step per applied plan. The plan, the preview, the
  approval, the result, the grant mode, and the parameters of the three write tools are published
  as JSON Schema in `schemas/`.

### Fixed

- Request IDs and plugin replies now survive the UI/sandbox relay.
- The scheduler releases jobs on terminal paths; operation mapping and pagination coverage are retained.
- Benchmarks use the actual core implementations rather than test-local copies.
- Local client sessions are attributed to the installation credential for discovery and revocation.

### Security

- Control endpoints reject browser origins and require the native control request header.
- Daemon-side scope checks reject unknown/out-of-grant node IDs.
- Transfer announcements above the configured budget receive `PAYLOAD_TOO_LARGE`.
- Publication checks exclude local credentials, raw measurements, runtime state, and assessment evidence.

### Limitations

- Real Figma Desktop compatibility, account/seat behavior, and two-client MCP interoperability
  remain unverified.
- Snapshot storage, watch/resync, offline bundles, and complete asset-resource delivery are not ready.
- Offline timings are diagnostics, not live-editor benchmarks or release targets met.
- PentAGI scripts exist, but no autonomous assessment result or external audit is claimed.
- The write path is pending product approval. The automated suites use a simulated editor, so no
  write has been run in a Figma Desktop session, and the editor undo behaviour of an applied plan,
  the version checkpoint, and the approval panel in a real session are unverified.
