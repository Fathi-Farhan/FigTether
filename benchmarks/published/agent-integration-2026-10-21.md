# Agent integration record - 21 October 2026

This record covers the integration around the bridge: how an agent host reaches the local
daemon, how it finds the tools, and what a real host session still does not prove. The
successful protocol checks and the unsuccessful host trials are kept apart on purpose. A
passing transport check does not establish that a model selects the right tool or that the
resulting design matches the brief.

- Machine-readable data: [agent-integration-2026-10-21.json](agent-integration-2026-10-21.json)
- Analysis and implementation order: [the audit](../../docs/audits/2026-10-21-agent-integration.md)
- Reproduction: [the integration guide](../agent-integration.md)

## Result

| Check                                                | Result                                                       |
| ---------------------------------------------------- | ------------------------------------------------------------ |
| Isolated protocol smoke, real binary and MCP adapter | Passed: initialize, tools/list, and one scoped call answered |
| Discovered tools                                     | 16: twelve read tools, three write tools, and `open_design`  |
| Granted roots and grant mode through MCP             | The synthetic granted root was returned with write access    |
| Single call timings                                  | 19.46 ms initialize, 1.62 ms tools/list, 1.11 ms scoped call |
| Rust workspace tests                                 | 684 passed, 1 ignored, 0 failed                              |
| Plugin tests                                         | 505 passed across 41 files                                   |
| Host versions found on the machine                   | Codex CLI 0.163.0, Claude Code 2.1.198, Gemini CLI not found |
| Real model task in a real host                       | Not run: no paid model task was launched                     |
| Live Figma mutation through the plugin               | Not run in this record                                       |
| Model tokens and billed cost                         | Unmeasured                                                   |

The timings are single protocol observations on a synthetic session. They are not model
inference times, not live editor latencies, and not a distribution. One sample per call is
not a benchmark.

## What the protocol check proved

The smoke check started an isolated daemon and a simulated plugin, initialized the real MCP
adapter over stdio, and listed the tools. The adapter answered `initialize` with the
canvas-first instructions, listed every tool of the three routers, and returned the granted
root of the synthetic session with the write grant mode.

The check also confirmed that the pairing secret is a local setup value and not a host
registration: `pair --json` returns only the secret and its expiry, and the secret is never
needed by the agent or by the MCP configuration.

The build under test is identified by its hash. The record keeps the binary, harness, plugin
and UI hashes, so a later run can be compared with the same code.

## What the host trials did not prove

The current shell could find Codex CLI and Claude Code. Gemini CLI and Docker were not on the
PATH. That is a fact about this machine at this moment, not a statement about the Gemini
application the user runs.

No personal host configuration was inspected or changed, no paid model task was launched, and
no real host session produced a design. The tool catalog that was visible in one session held
zero FigTether tools, while the adapter itself answered when it was started explicitly. That
observation shows that a green paired status in the Figma plugin does not imply that every
local agent can already call the tools. It does not explain why a particular model behaved
the way it did.

The untested list is kept explicit: real Codex, Claude, and Gemini task behavior, the
contents of a host configuration, vision rendering in each harness, fresh live Figma
mutations, and an autonomous PentAGI assessment.

## What was implemented after the analysis

The analysis recommended a task-oriented entry tool, one canonical workflow with host
adapters, and a host readiness check. The tree now carries all three.

- `open_design` opens a granted frame in one call. It resolves a named frame inside the
  existing grants, returns choices instead of guessing when several frames match, and reports
  the next permitted action. It never pairs, never expands a grant, and never approves.
- `figtether setup --client codex|gemini|claude|generic` writes a project-local MCP entry with
  an absolute path to the executable, installs the design skill next to it, and probes the
  real stdio adapter afterwards. The command previews its changes and refuses to overwrite a
  configuration it does not own.
- The canonical workflow lives in one document, `docs/agent-workflow.md`, and is generated
  into the MCP instructions and into the skill, so the wording cannot drift between them.

The remaining gap from the analysis is the one that needs a real host: a model task in a
real session, measured, repeated, and compared. That is a test plan, not a claim.

## Cost and limits

Cost stays unmeasured. No model-backed compatibility run was launched, so there are no token
counts and no billed amount. The numbers are not zero; they are absent.

The MCP surface returns text and structured content. Image content blocks and resource
delivery are not implemented, so a host that wants a rendered preview receives asset metadata
rather than a picture through MCP alone. Export bytes stay in the daemon, and the asset
reference is checked against the current grant when it is read.

## Reproduction

```sh
cargo build --locked --release -p figtether-cli
npm --prefix plugins/figma run build
node scripts/validation/agent-integration.mjs --binary target/release/figtether.exe
```

The script writes a sanitized JSON record with the same shape as the data file next to this
report. Each run uses a fresh output directory. The reproduction does not launch a model, so
it cannot produce a design or a cost measurement.

See the [canvas-first workflow](../../docs/agent-workflow.md) for the design loop and
[the agent setup guide](../../docs/agent-setup.md) for the host configuration.
