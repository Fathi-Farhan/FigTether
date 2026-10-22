# Current documentation status

Reviewed on 22 October 2026. This page resolves the difference between the original design
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
- The selected local manifest supplies the plugin id and port to the build. The UI connects
  to `ws://localhost:PORT/bridge`; the daemon binds `127.0.0.1:PORT`. Port `38947` is a
  fallback, not an endpoint that overrides the imported manifest. Changing ports requires
  rebuilding and reimporting `plugins/figma/dist/manifest.json`.
- `figtether start` prepares the source installation, builds the matching plugin when needed,
  starts or reuses the daemon and prints a one-use secret in the same terminal. A standalone
  executable does not embed/install the plugin. Figma import and sharing remain user steps.
- The MCP server exposes 17 tools. Snapshot storage and watch/change-journal tools report
  unavailability; their reserved names do not imply working implementations. GROUP is in the
  allowlist but cannot be completed through the current MCP shape. Use FRAME containers.
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

The [22 October feature and error review](06-testing/end-user-feature-review.md) records
calls to all 17 tools on a native session, including expected capability refusals. Selection,
rich-fill reads and inspection succeed after the recorded fixes. Auto-approved native plans
created an editable Studio Notes section and exercised text, geometry, primitive/component
creation and four gradients. Browser launch opened Edge. An oversized SVG returns a structured
error and subsequent PNG export succeeds. The final FILL sizing check after its additional
resize-order fix is still pending restored user access; it is not counted as a native pass.

This is limited compatibility evidence. It is not a release certification, an account/seat
matrix, a complete download-delivery test, a security audit, or model billing telemetry.

## What remains incomplete or unverified

The earlier `get_selection` timeout and `inspect_design` mismatch remain in the dated desktop
report; later native checks and fixes are recorded separately above. Snapshot storage, offline
bundles, watch/resync, complete asset-resource delivery,
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
