# Changelog

User-visible changes are listed here. No release has been published yet. Version `0.1.0` in the
workspace is a development version, not a download or a tested compatibility promise.

## Unreleased

### Added

- Project-local `figtether setup` profiles for Codex, Gemini CLI, and Claude Code, with preview,
  installation, removal, and a staged connection check against the registered MCP executable.
- `open_design` for bounded shared-frame discovery and image context, and `get_asset` for
  grant-bound PNG delivery as standard MCP image blocks. The inline cap is 1 MiB, with a
  five-minute lifetime and immediate invalidation when the grant changes.
- A canonical design workflow generates MCP initialization guidance and discoverable project
  skills. Deterministic and opt-in real-host evaluation runners retain versioned evidence;
  installation alone does not certify host tool visibility or model behavior.
- End-user Bridge flow with named frames, a separate currently-shared summary, readable bounded
  activity without list numbering, and a private local browser companion opened from Figma.
- Session-only Auto mode with explicit user opt-in. New plans need complete warning-free
  previews and no deletions; warnings, blocks, incomplete checks, grant changes, reconnects,
  and partial writes stop automation. The daemon independently checks automatic approvals.
- Running `figtether` without a subcommand starts the foreground daemon, with the same defaults
  as `figtether daemon`. `figtether pair` remains a separate explicit action.
- `figtether update` for source installations: clean upstream fast-forward, staged CLI build,
  plugin rebuild, executable replacement with rollback on replacement failure, and an update
  lock. `--source`, `--local`, and `--dry-run` support moved checkouts, ZIPs, and previewing steps.
- End-user installation tutorial covering executable paths, plugin IDs/import, pairing secrets,
  MCP configuration, read/write grants, first-design approvals, and shared-agent shutdown.
- Rust daemon, MCP stdio adapter, and CLI for local Figma reads and approval-gated design creation and edits.
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

- Custom-port plugin builds use the local manifest's port, or `FIGTETHER_PORT`, for the socket
  and companion validation. Startup diagnostics explain Windows reserved-port error 10013.
- Native plugin pagination cursors survive daemon normalization and continuation requests.
- Typechecking rebuilds the shared TypeScript declarations before checking their consumers.
- Explicit re-pairing after local or daemon revocation clears the terminal UI state and enables
  fresh sharing. Revoked credentials/grants remain invalid, daemon revocation clears sandbox
  scope, and callbacks from replaced sockets cannot corrupt the new connection.
- Write admission uses an advancing monotonic clock, allowing the rolling minute budget to expire.
- Styled node creation accepts initial fills, text and fonts with consistent property/text budgets.
- Default node reads avoid font/style/variable resolution until those fields are requested.
- Complete write receipts teach the daemon created IDs inside their existing granted scope.
- Timed-out plugin sockets close; reconnect authenticates rotating credentials and restores only
  the same session's existing user grant. Fresh pairing cannot inherit scope from a plugin label.
- Session discovery exposes granted root IDs and grant mode; MCP initialization supplies a
  canvas-first design workflow. A project design skill and `pair --json` support local setup.
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

- Windows/Figma Desktop 126.10.10 has limited synthetic-file read/write evidence. Other
  platforms, seat behavior, and two-client MCP interoperability remain unverified.
- Snapshot storage, watch/resync, offline bundles, and complete asset-resource delivery are not ready.
- Offline timings are diagnostics. Separate versioned desktop measurements cover cached/fresh
  reads and PNG export metadata; neither dataset establishes every release target.
- PentAGI scripts exist, but no autonomous assessment result or external audit is claimed.
- The original write-path release proposal remains pending. Automated suites use a simulated
  editor; separate Windows desktop evidence records previews, approval controls, native writes,
  and readback. Manual undo restoration and version checkpoints remain unverified.
