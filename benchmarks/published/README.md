# README chart data

Historical source commit IDs predate the first public push. The
[history mapping](../../docs/maintainers/history-provenance.md) locates identical source trees
after the owner's pre-publication author/subject correction. Recorded measurements and hashes
remain unchanged.

The [agent integration implementation and evaluation](agent-integration-2026-10-21.md)
separates deterministic adapter checks from real host trials, including policy restrictions,
timeouts, unavailable clients, and unmeasured billing. Its [data](agent-integration-2026-10-21.json)
identifies each build. See the [reproduction guide](../agent-integration.md).

The [Bridge UX verification](bridge-ux-2026-10-21.json) records controller/daemon checks,
browser interactions, version hashes, ten local companion snapshot timings, and unmeasured
model cost. It uses a synthetic plugin and is separate from live Figma design benchmarks.
The [usability review](../../docs/audits/2026-10-21-bridge-ux.md) explains the product changes
and remaining user needs.

The [21 October native website test](website-2026-10-21.md) records six approved writes,
123 verified nodes, concurrent native reads, a Figma PNG, and the remaining rich-read and
pagination failures. [Its source data](website-2026-10-21.json) identifies the tested build.
The [source updater verification](source-update-2026-10-21.json) records separate Windows
installation tests; it is not a Figma performance comparison.
The [default daemon launch verification](default-daemon-2026-10-21.json) records the bare
`figtether` command, compatibility with `figtether daemon`, and separate pairing behavior.

The [21 October bridge integration verification](bridge-integration-2026-10-21.md) records
the connection and editing fixes, automated checks, and remaining native-editor validation.
Its measurements are separate from the historical charts below.

The [offline stage chart](../../assets/benchmark-stages.svg) is generated from
[`offline-stages.json`](offline-stages.json), a reviewed aggregate from one local diagnostic run.
It contains no design data, username, machine hostname, filesystem path, or runtime credential.

## What was measured

`w5_decomposition/pipeline_stages` executes 2,000 samples of each operation against the actual
Rust core. The source commit is `e9e23b0ed210268d06e6697aad8b0356c32e530a`; the executable and
original result hashes are recorded in the JSON. The run used an optimized bench build on
Windows 11, an AMD Ryzen 5 4500 (12 logical processors), and approximately 16 GiB of RAM.

| Offline operation                           | p50 (ms) | p95 (ms) | p99 (ms) |
| ------------------------------------------- | -------: | -------: | -------: |
| Queue admission                             |   0.0019 |   0.0029 |   0.0044 |
| Frame encoding/decoding                     |   0.0084 |   0.0110 |   0.0177 |
| Normalize 100 synthetic nodes               |   0.0634 |   0.0969 |   0.1279 |
| Project metadata and encode a JSON envelope |   0.1179 |   0.1568 |   0.2073 |

The chart converts milliseconds to microseconds and uses a linear, zero-based axis. Frame work
names 50 IDs; normalization and shaping use the 100-node fixture. These are independent stage
operations, **not a single request trace**. Do not sum their percentiles.

## What these numbers do not say

- No Figma editor, network, MCP client, or model inference was involved.
- This was Criterion's test mode, one diagnostic invocation, not a repeated statistical study.
- There is no confidence interval, competitor baseline, live-editor latency, or claim that a
  product target has been met. A smaller bar is not a cross-product comparison.
- Raw samples are local and identified by hash. This export is reproducible input for the chart,
  not independently attested release evidence. Full release evidence follows the
  [benchmark protocol](../../docs/06-testing/04-benchmark-plan.md).

## Reproduce

From the repository root, choose a local, ignored results directory. For example, in PowerShell:

```powershell
$env:FIGTETHER_BENCH_COMMIT = git rev-parse HEAD
$env:FIGTETHER_BENCH_RESULTS_DIR = 'tmp/readme-benchmark'
cargo bench --locked -p figtether-bench --bench w5_decomposition -- --test
```

Or in a POSIX shell:

```sh
FIGTETHER_BENCH_COMMIT="$(git rev-parse HEAD)" \
FIGTETHER_BENCH_RESULTS_DIR=tmp/readme-benchmark \
cargo bench --locked -p figtether-bench --bench w5_decomposition -- --test
```

Without `--test`, Criterion performs repeated measurements; that is a different run protocol.
Expect your measurements to differ. Inspect the `.summary.json`, review the environment and
exclusions, then update the published aggregate deliberately. Do not auto-publish raw logs.

Regenerate or check all five README figures:

```sh
node scripts/render-readme-assets.mjs
node scripts/render-readme-assets.mjs --check
```
