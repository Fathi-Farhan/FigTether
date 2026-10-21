# Evaluate agent integration

Keep protocol compatibility, host/model behavior, native Figma execution, and visual quality
as separate results. A passing transport test does not establish that every model selects
the right tools or makes a useful design.

The first [implementation/evaluation record](published/agent-integration-2026-10-21.md)
retains the successful protocol checks and unsuccessful host trials separately.

## Deterministic integration check

Build the CLI and plugin, then run from the repository root:

```sh
cargo build --locked --release -p figtether-cli
npm --prefix plugins/figma run build
node scripts/validation/agent-integration.mjs --binary target/release/figtether.exe
```

Use your actual executable path and omit `.exe` on platforms where appropriate. The browser
helper defaults to Windows Chrome; set `CHROME_BINARY` for another installed Chrome location.
This test starts its own daemon/port/runtime and authenticated synthetic editor. It verifies
named/ambiguous target discovery, actual MCP PNG blocks, approved writes, warning/read-only
behavior, cursor continuation, stale/cross-session image refusal, and client setup/removal.
It never connects to the shared Figma session. No model API is invoked.

Rust tests additionally cover asset expiry, revoked/paused/offline sessions, oversized PNGs,
user configuration preservation, and image metadata shaping:

```sh
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
node scripts/generate-agent-workflow.mjs --check
```

## Actual host/model runs

These commands explicitly launch installed agent clients using their existing authentication.
They can consume model quota or incur provider charges. They are not run by CI or installation.
No global authentication, prompts, trust, or approval settings are changed.

```sh
node scripts/validation/agent-eval.mjs --binary target/release/figtether.exe --host codex --scenario create --context empty
node scripts/validation/agent-eval.mjs --binary target/release/figtether.exe --host claude --scenario edit --context source
node scripts/validation/agent-eval.mjs --binary target/release/figtether.exe --host gemini --scenario warning --context empty
```

Choose an exact provider-supported model with `--model MODEL_ID`, or retain the host's
default and mark an unreported model identity unknown. Supported scenarios are `create`,
`edit`, `warning`, and `read-only`; `--host all` tries available hosts. `--context source`
uses a small development-checkout simulation, not the real FigTether working tree.
Missing clients are recorded as unavailable. Codex uses per-invocation MCP configuration;
Claude uses the generated project entry with a strict MCP selection. Each run has a 180-second
host deadline. Timeout cleanup targets only that run's process tree and isolated daemon.

The synthetic editor stores editable scene primitives and renders a deterministic browser
preview. It simulates a test user who opted into clean-plan approvals. Warnings remain pending.
It is a repeatable tool-selection fixture, not a substitute for the Figma Plugin API or
human judgments of design quality. The runner verifies mutations and follow-up reads/exports;
it cannot establish that a model actually evaluated the image well.

Results include host version, reported/requested model, task/context, outcome, canvas/preview
request timing, approved writes, repository-read observations, and host-reported usage/cost.
The first editor request is a transport timing proxy, not a guarantee that the model has
observed the complete result. Filesystem-read counts use recognized host events, so missing
trace detail can leave that measure uncertain. Read restrictions must be reported, not weakened.

Default summaries exclude prompts, reasoning, tool arguments, credentials, and private config.
`--retain-final` and `--retain-errors` retain extra local diagnostic text in the ignored run
directory; review it before sharing. Do not publish runtime credentials or project connection
files. Each fresh output directory preserves prior attempts rather than overwriting them.

## Compare versions honestly

Use the same brief, seeded scene, grant, approval mode, host/model version, and context for
before/after runs. Record binary/workflow hashes and sample counts. Run multiple trials for
claims about reliability or speed. A single failure is diagnostic, not a ranking of models.
Input-token totals, cached tokens, output tokens, provider billing, and wall time are different
measurements. Do not infer dollars from an unknown price or treat missing usage as zero.

Real Figma validation remains a separate phase: reload the updated daemon/plugin when shared
agents are finished, create isolated granted frames, run the same brief through each host,
retain node/readback/export evidence, and review the actual result. Respect manual approval
and any user-enabled Auto warnings. Native read/export benchmarks from older builds remain
historical evidence with their original hashes.
