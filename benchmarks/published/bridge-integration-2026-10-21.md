# Bridge integration verification - 21 October 2026

This verification addresses friction reported during agent-driven Figma design. It covers the
updated source, plugin build, and simulated-editor transport. It is **not a new native design
quality or desktop latency benchmark**. The [machine-readable record](bridge-integration-2026-10-21.json)
retains versions, command timings, hashes, failures, and cost uncertainty.

## Result

| Check                                          | Result                                                        |
| ---------------------------------------------- | ------------------------------------------------------------- |
| Rust workspace tests                           | 620 passed, 1 ignored, 0 failed                               |
| Plugin tests                                   | 475 passed across 39 files                                    |
| Rust formatting and Clippy                     | Passed with warnings denied                                   |
| Plugin types, type checks, lint and build      | Passed after one corrected unused test variable               |
| Generated schemas                              | 44 files; repeated generation unchanged                       |
| Focused design skill                           | Passed the skill creator's validation                         |
| Real socket timeout and authenticated recovery | Passed; test uses the production 15-second heartbeat deadline |
| Windows Default-desktop discovery              | Forced fallback captured the actual Figma window              |
| Updated build in Figma Desktop                 | Not loaded or verified in this run                            |
| Model tokens and billed cost                   | Unmeasured                                                    |

Raw logs remain under ignored `benchmarks/results/bridge-integration-2026-10-21`. Command
durations include process startup and, where relevant, compilation. They are not per-tool
latency or model inference measurements. The full Rust command took approximately 65.03 s;
the final plugin test command took 3.92 s. Concurrent validation on one Windows machine is
not a controlled performance comparison.

## Changes exercised

The reproducible [isolated smoke check](../../scripts/validation/bridge-smoke.mjs) starts a
real CLI/daemon/MCP adapter and authenticates a simulated plugin. It verifies the pairing
JSON allowlist, automatic workflow instructions, all three write tools in discovery, and
the granted roots/write mode returned through MCP. Its sanitized output retains binary,
plugin, UI and harness hashes plus individual request timings. Each output directory is new.
This does not exercise native Figma rendering or approval clicks.

- An advancing monotonic write clock replaces the session-lifetime rate-limit bug. Tests
  cover refusal immediately before 60 seconds and admission at the boundary.
- A creation operation can include fills, text and fonts. Budgets include those fields;
  unavailable initial fonts fail before node creation. This reduces required operations for
  the supported case without claiming measured percentage savings.
- Default `get_nodes` reads use summary, parent and geometry fields. Projection applies
  before expensive reference/font normalization. Explicit rich reads remain available.
- Complete successful creation receipts teach new IDs under known granted parents. Partial,
  inconsistent, stale-grant, read-only and outside-scope cases remain untrusted.
- MCP discovery provides actual granted roots and grant mode. Initialization supplies
  canvas-first workflow instructions, with a focused project skill for supporting clients.
- The daemon closes sockets after missed heartbeats. A rotating credential authenticates
  reconnect to the same session; label matching alone never restores access. The UI restores
  only its previous grant, preserves pause, and discards pending write decisions.
- `pair --json` provides machine-readable local setup output. The desktop helper can
  enumerate Figma windows on the Default desktop using read-object access.

The socket test receives daemon heartbeats while intentionally sending none, reproducing the
condition that could leave the old UI looking connected. It checks closure, offline discovery,
credential authentication, a higher connection epoch, a paired state before the grant, and a
ready state after the existing scope is resent. Other tests cover credential rotation,
revocation, wrong labels/values, and UI grant isolation. These tests use a simulated plugin,
not a running Figma editor.

## Native observation and remaining validation

The existing Figma Desktop 126.10.10 process used the earlier release daemon/plugin. Status
and discovery answered in approximately 4.17 ms and 2.37 ms. Two bounded frame reads returned
`PLUGIN_OFFLINE` in approximately 2.20 ms and 1.73 ms while the plugin displayed Ready.
Each observation has one sample; these values do not establish a latency distribution.
Raw responses and artifact hashes remain in `benchmarks/results/live-feedback-2026-10-21`.

The existing canvas was inspected visually. Its dense specification/audit content does not
establish that the original website brief was met. No new canvas design or user approval was
performed during this verification. The Windows helper fallback was forced from the available
desktop; a separate sandbox window station reporting zero handles was not reproduced.

Load the updated daemon and plugin, pair locally, and explicitly share the intended frame to
validate the new flow in Figma. Native styled creation, follow-up editing without traversal,
reconnect, rich-read performance, and visual quality still need a fresh recorded desktop run.
Existing `get_selection`, `inspect_design`, and asset delivery limits remain documented.

Draft command/test mistakes and the initial lint failure are retained in the JSON record.
Missing cost telemetry stays null. The prior desktop measurements and the other agent's
reported timings have not been relabeled as results of this build.

See the [canvas-first workflow](../../docs/agent-workflow.md) for setup and the design loop.
