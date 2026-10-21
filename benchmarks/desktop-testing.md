# Repeatable Figma Desktop testing

The first real-editor run is recorded in [the 2026-10-20 report](published/desktop-2026-10-20.md).
It uses the local project MCP adapter, not the official Figma MCP service.

## Prepare a version

Build the CLI in release mode and the plugin. Import `plugins/figma/manifest.json` in Figma
Desktop, run the development plugin, start the local daemon, and pair using the CLI's one-time
secret. Keep that secret in the plugin; do not put it into reports or model prompts.

For the existing synthetic website fixture, select the website frame, refresh candidate roots,
check its checkbox and allow a read session. Discover the active session using `figtether status
--json`: use the connected session in `capabilities`, not an old `ready` entry.

The initial design was built with `scripts/validation/desktop-website.mjs` through
`preview_write`, UI preview approval, `get_write_approval` and `apply_write`. This helper assumes
the synthetic root is `2:2` and stores resume state in `tmp/desktop-validation/website-state.json`.
Do not replay it against a different document or reuse state with another session. Writes require
a write grant. Its UI automation is for explicitly authorized desktop test files only.

## Measure

Choose a new run directory for each version and attempt. Do not overwrite a previous version's
directory. In two terminals, run from the checkout:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/validation/sample-resources.ps1 `
  -Output benchmarks/results/YOUR_VERSION_RUN/resources.raw.jsonl

node scripts/validation/desktop-benchmark.mjs SESSION_UUID `
  D:/rust/kiln-target/release/figtether.exe benchmarks/results/YOUR_VERSION_RUN
```

Replace the executable with the release binary you actually built. `FIGTETHER_DESKTOP_ROOT`
can override the default `2:2` root. The fixture must have the same website contents and size for
an apples-to-apples comparison. Keep the plugin open and the machine otherwise quiet.

The default workload contains 10,000 cached root reads, 1,000 sequential fresh 72-node reads and
100 PNG exports of the 1440×1640 website. Fresh and cache-priming requests are separate lanes.
The script writes its exact workload before starting. The export size is different from the
planned 1024×1024 N03 workload, so its results must not be labeled N03 acceptance.

The fresh lane requests depth 2 because this traversal counts the root as level 1. It asserts
exactly 72 returned nodes per call. Root-only reads assert one node. Fresh fallbacks from cached
requests remain recorded separately by the response's `freshness` value.

For a smoke test, set `FIGTETHER_DESKTOP_CACHED_READS`, `FIGTETHER_DESKTOP_FRESH_READS` and
`FIGTETHER_DESKTOP_EXPORTS` to smaller positive counts. Keep those counts in the report. A smoke
run does not establish the full workload targets.

The resource sampler stops when the benchmark writes `sampling.stop`. If interrupted, create
that file yourself. Let it sample an idle daemon for at least ten seconds before starting a run.

## Retain and compare

Every MCP call is appended to `calls.raw.jsonl`, including errors and blocked plans. Measurements
include wall time, response sizes, source/freshness, coverage, operation parameters, errors,
commit, dirty state, source-diff hash and binary/plugin/UI hashes. Approval tokens are redacted.
Per-run summaries contain nearest-rank p50/p95/p99. Resource samples include daemon working set,
private bytes and cumulative CPU seconds. Keep Windows working set and private bytes distinct.

The model usage and dollar cost fields stay `NOT_MEASURED` unless actual usage/billing telemetry
is supplied. This test runner does not call a model. Do not substitute a price estimate or zero
for the unknown chat cost.

Generate a reviewable aggregate after the run:

```powershell
node scripts/validation/desktop-report.mjs benchmarks/results/YOUR_VERSION_RUN YOUR_VERSION_RUN
```

The report helper uses the website fixture and its native PNG under `tmp/desktop-validation/`.
It archives the current source patch and runner next to the raw data. Freeze/archive these at
the start of a run too if source changes are expected during testing. Review the generated
summary before sharing it. Raw evidence stays ignored; sanitized JSON/Markdown can be retained
under `benchmarks/published/`. Record unsupported and unmeasured checks as such.

Use `figma-export.ps1` only when the current page contains this single synthetic website frame.
It runs Figma's native Copy as PNG and saves only a newly produced image with the expected frame
dimensions. This visual artifact is separate from the MCP export/asset-metadata measurements.
