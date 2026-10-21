# Current documentation status

Reviewed on 21 October 2026. This page resolves the difference between the original design
reference, the current source implementation, and recorded test evidence. Public documentation
and examples are maintained in English.

## What works in the source build

- FigTether exposes its own local MCP stdio server. The Rust adapter connects to the daemon
  through authenticated loopback WebSocket; the plugin UI connects through another local
  WebSocket and relays requests to its sandbox.
- The plugin uses the documented Figma Plugin API for scoped reads and native mutations.
  The read/write path does not call Figma's official MCP service or REST API.
- Agents can create and edit supported native nodes with `preview_write`,
  `get_write_approval`, and `apply_write`. A separate write grant and user approval for the
  exact plan are required. Read tools remain non-mutating.
- The UI connects to `ws://localhost:38947/bridge`; the daemon binds `127.0.0.1:38947`.
  Development manifest patterns use `localhost`. Changing only the manifest port does not
  change the UI's endpoint.
- The build inlines UI JavaScript, CSS, and its small icon into `ui.html` so the Figma iframe
  does not depend on sibling asset URLs.

See the [quickstart](quickstart.md), [Bridge guide](bridge-ui-guide.md),
[code verification](write-path-verification.md), and
[implementation status](10-implementation/01-implementation-status.md).

## What has been observed

The [desktop report](../benchmarks/published/desktop-2026-10-20.md) records one Windows
machine with Figma Desktop 126.10.10: plugin loading, pairing, approved native creation of
71 editable children, scoped reads, text readback, and PNG export metadata. It retains prior
builds, failures, sample counts, and artifact hashes. The separate
[UI verification](../benchmarks/published/readme-ui-2026-10-21.json) records the later UI
build, native screenshot, tests, and automation limits.

This is limited compatibility evidence. It is not a release certification, an account/seat
matrix, a complete download-delivery test, a security audit, or model billing telemetry.

## What remains incomplete or unverified

`get_selection` timed out and `inspect_design` returned a protocol mismatch in final desktop
controls. Snapshot storage, offline bundles, watch/resync, complete asset-resource delivery,
and large chunked transfers need further work. Manual undo restoration, version checkpoints,
macOS, multiple clients, and a 60-minute soak remain unverified. Model cost and tokens are
**not measured**; the absence of an official MCP dependency does not imply zero AI cost.

## How to read the older documents

The numbered PRD, architecture, UML, security, acceptance, operations, and research documents
preserve the original proposal. Their requirements, example commands, rollout stages, and
approval labels are not an inventory of available features. Earlier blanket read-only language
describes that baseline; the current implementation adds approval-gated writes.

`ADR-013` remains formally **Proposed** in the approval registry. This audit does not invent
a release sign-off or change a historical decision. The implementation and desktop evidence
exist independently of that pending formal decision. Use the current guides for executable
setup and the original documents to understand design rationale or planned workflows.

Historical benchmark and publication-review reports remain dated records. Their hashes and
results are not silently updated to describe a newer build.
