# Benchmarks

FigTether has **offline Rust-core benchmarks**, not a validated live-editor performance claim.
They use the real `figtether-core` scheduler, cache, normalizer, and asset store with synthetic
data. They do not start Figma or a model provider.

## Read the README chart

![Offline stage timing chart, p50 and p95](../assets/benchmark-stages.svg)

The chart is one optimized diagnostic run, not a competitor comparison or SLA. The
[reviewed aggregate and methodology](published/README.md) include commit, fixture, sample count,
environment, hashes, and exclusions. Values from different stages are not additive percentiles.
The [source JSON](published/offline-stages.json) regenerates the SVG without an external service.

## Run a workload

```sh
# A single diagnostic invocation; quick, not a statistical study
cargo bench --locked -p figtether-bench --bench w5_decomposition -- --test

# Criterion's repeated measurements for one workload
cargo bench --locked -p figtether-bench --bench w1_cached
```

| Target             | Workload                         | What it measures                                          |
| ------------------ | -------------------------------- | --------------------------------------------------------- |
| `w1_cached`        | 10,000 cached metadata reads     | Core cache lookup, response shaping, encoded size         |
| `w2_fresh`         | 1,000 sequential synthetic reads | Queue and normalization; no Plugin API read               |
| `w3_export`        | 100 synthetic 1024x1024 payloads | Framing and asset-store writes/reads; no PNG render       |
| `w4_soak`          | Two clients, four mock sessions  | Mixed operations, errors, benchmark-process RSS           |
| `w5_decomposition` | 2,000 samples per stage          | Queue admission, frame codec, normalization, JSON shaping |

These workload names are stable for comparison within this project. The
[`shipped_core` tests](../crates/bench/tests/shipped_core.rs) guard against accidentally replacing
the measured implementation with a benchmark-only copy.

`w4_soak` defaults to 60 seconds **per invocation**; Criterion can repeat it many times. For a
local smoke run, set `FIGTETHER_BENCH_SOAK_SECONDS=5`. That does not satisfy the planned
60-minute stability test.

## Results stay local by default

| Variable                       | Purpose                                                                         |
| ------------------------------ | ------------------------------------------------------------------------------- |
| `FIGTETHER_BENCH_COMMIT`       | Set explicitly to `git rev-parse HEAD`; otherwise results say `unknown-commit`. |
| `FIGTETHER_BENCH_RESULTS_DIR`  | Output directory; defaults to ignored `benchmarks/results/`.                    |
| `FIGTETHER_BENCH_SOAK_SECONDS` | Duration override for local soak diagnostics.                                   |

Each run writes a `.raw.jsonl`, `.summary.json`, and `.evidence.json` under
`<results>/<workload>/<commit>/`. Treat the files as local until reviewed: evidence may contain
machine details, commands, and paths. Publish only sanitized summaries in `benchmarks/published/`.

Render a report from a particular run:

```sh
cargo run --locked -p figtether-bench --bin report -- \
  --measurement benchmarks/results/W5/COMMIT/run-1.summary.json \
  --evidence benchmarks/results/W5/COMMIT/run-1.evidence.json \
  --output tmp/benchmark-report.md
```

Replace `COMMIT` with the run's actual label. With no inputs the report command prints a template;
missing measurements remain `NOT_MEASURED` rather than being guessed.

## Diagnostics versus release evidence

A local diagnostic can be published if its source, method, data, and exclusions are clear, as in
the README chart. It must not be presented as proof that a product requirement is met.

For a release performance claim, follow the full
[benchmark protocol](../docs/06-testing/04-benchmark-plan.md): freeze the exact build, fixture,
hardware, editor, seat context, operation definition, and failure denominator before running.
Keep cached/fresh/snapshot/export lanes separate and retain failures in reports.

The offline crate cannot establish:

- live Plugin API latency or exported PNG/SVG rendering time;
- editor responsiveness, sleep/wake, or real reconnect readiness;
- daemon idle RSS (the benchmark process includes Criterion);
- onboarding time, seat eligibility, or end-to-end client compatibility.

Those need editor sessions and pilot users. There is no assertion that the PRD's `N01` to `N10`
targets have been met, and no claim of superiority to the official Figma MCP service.
