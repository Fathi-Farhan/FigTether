# Benchmarks

This directory holds the benchmark evidence of FigTether. The benchmark code
lives in `crates/bench`, the result files live under `benchmarks/results/`, and
the report generator lives in `crates/bench/src/bin/report.rs`.

## Status: offline domain benchmarks, not release claims

The benchmarks in `crates/bench` are **offline domain benchmarks**. They run
in process, with no Figma editor attached, and they measure the daemon side
of FigTether: the metadata cache, the scheduler, the bridge framing, the
normalizer, the response shaping, and the asset store.

They measure the **shipped `figtether-core` implementation** over the real
contract types, not a copy of it:

| Part of the domain | Shipped type                                                                |
| ------------------ | --------------------------------------------------------------------------- |
| design cache       | `figtether_core::Cache` with `figtether_core::cache::CacheKey`              |
| request queue      | `figtether_core::Scheduler` with `figtether_core::scheduler::Job`           |
| normalizer         | `figtether_core::normalize_document`                                        |
| asset store        | `figtether_core::InMemoryAssetStore` on `figtether_core::assets::AssetStore` |
| resource contract  | `figtether_core::limits::ResourceLimits`                                    |
| scope index        | `figtether_core::scope::NodeIndex`                                          |
| response envelope  | `figtether_model::Envelope`                                                 |
| bridge frames      | `figtether_protocol::BridgeFrame` and `figtether_protocol::ChunkAssembler`  |

The integration test `crates/bench/tests/shipped_core.rs` is the guard: it
fails when the benchmark stops using the shipped core, either because the
domain holds a local copy of the cache, the scheduler, or the asset store, or
because the benchmark reads and normalizes with anything but the shipped
implementation.

They are **not** editor benchmarks and they are **not** performance claims.
Every stage that only a running Figma plugin can produce is reported as
`NOT_MEASURED`; it is never estimated, never interpolated, and never carried
over from another run. The editor dependent stages, the plugin handler and
the editor export render, still need a Figma Desktop session.

| Stage              | Measured in this crate | Source                                            |
| ------------------ | ---------------------- | ------------------------------------------------- |
| `queue_ms`         | yes                    | the shipped daemon scheduler                     |
| `transport_ms`     | yes                    | the bridge protocol crate                         |
| `plugin_ms`        | no, `NOT_MEASURED`     | the Figma plugin handler, needs an editor session |
| `normalization_ms` | yes                    | the shipped daemon side normalizer                |
| `shaping_ms`       | yes                    | the daemon side envelope shaping and encoding     |
| `render_ms`        | no, `NOT_MEASURED`     | the editor export render, needs an editor session |
| `e2e_ms`           | yes                    | the in-process path, editor time excluded         |

## Profiles and the targets they feed

| Profile | Benchmark target                   | Feeds                                      | Editor needed |
| ------- | ---------------------------------- | ------------------------------------------ | ------------- |
| W1      | `w1_cached/cached_metadata_reads`  | N01 cached metadata                        | no            |
| W2      | `w2_fresh/fresh_pipeline_reads`    | N02 fresh sequential read                  | no            |
| W3      | `w3_export/asset_exports`          | N03 export, daemon side of the path only   | no            |
| W4      | `w4_soak/mixed_soak`               | N04 resident set size, N05 unexpected rate | no            |
| W5      | `w5_decomposition/pipeline_stages` | the stage decomposition of N01 to N03      | no            |

The benchmark names are stable, so results stay comparable over time.

## What this crate does not cover

| Target | Why it is not covered here                                                                                                                                                                                           |
| ------ | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| N03    | The target is a PNG 1024x1024 export p95 in the editor. This crate measures the queue, the chunked transfer, the asset store write, and the asset store read. The editor render is `NOT_MEASURED`.                   |
| N06    | N06 asks for the absence of an editor freeze longer than 1 s caused by our traversal. It is an editor side measurement with screen recording and input probes. It cannot exist without a real Figma Desktop session. |
| N08    | N08 asks for offline detection within 15 s and reconnect readiness within 10 s across sleep and wake, plugin kill, daemon restart, and port collision. The scenarios need a real editor session and a real plugin.   |

N07, N09, and N10 are out of scope for this crate as well: cancellation and
revocation need the transport, N09 needs the egress capture harness, and N10
needs pilot users on a distribution bundle.

The N01, N02, N03, N04, and N05 targets themselves also stay unproven here.
The numbers this crate produces are the daemon side of those targets. A target
is only met when the full protocol of the benchmark plan has been executed.

## The rule for published numbers

**No published number may be taken from this crate without the full protocol of
`docs/06-testing/04-benchmark-plan.md`.** That protocol requires, at minimum:

1. the pre-run freeze checklist of section 1.1, including the commit, the
   fixture set, the Figma Desktop version, the account and seat context, the
   device profile of 4 logical cores and 8 GB of RAM, the release build, and
   the released configuration;
2. the run discipline of section 1.2, including the workload class separation,
   the cache hit accounting, the negative and adversarial controls, and the
   independent reviewer;
3. the failure taxonomy of section 1.3, so unexpected errors stay below
   0.1 percent of the classified operations;
4. the protocol of the target itself, including the real editor session for
   N02, N03, N06, and N08;
5. the report template of section 3, with every field filled with a measured
   value.

A number without a commit, without a fixture hash, without the reviewer, and
without the editor context is not publishable, no matter how it was produced.

## Running the benchmarks

```sh
cargo bench -p figtether-bench                      # every profile
cargo bench -p figtether-bench --bench w1_cached    # one profile
```

The mixed soak runs for 60 seconds per iteration, which is the length the
benchmark plan asks for. A local smoke run shortens it:

```sh
FIGTETHER_BENCH_SOAK_SECONDS=5 cargo bench -p figtether-bench --bench w4_soak
```

Two environment variables steer the result files:

| Variable                      | Meaning                                                |
| ----------------------------- | ------------------------------------------------------ |
| `FIGTETHER_BENCH_COMMIT`      | the commit label of the run, `unknown-commit` if unset |
| `FIGTETHER_BENCH_RESULTS_DIR` | the results root, `benchmarks/results` by default      |

A run writes three files per run index under
`benchmarks/results/<test id>/<commit>/`:

| File                    | Content                                     |
| ----------------------- | ------------------------------------------- |
| `run-<n>.raw.jsonl`     | one JSON object per operation, the raw log  |
| `run-<n>.summary.json`  | the measurement bundle, the measured values |
| `run-<n>.evidence.json` | the release evidence, the run context       |

## Rendering the report

```sh
cargo run -p figtether-bench --bin report -- \
    --measurement benchmarks/results/N01/unknown-commit/run-1.summary.json \
    --evidence benchmarks/results/N01/unknown-commit/run-1.evidence.json \
    --output benchmarks/results/N01/unknown-commit/summary.md
```

Without any input the generator renders the empty template, which is useful
as the checklist of a run that has not happened yet. The report prints the
frozen target and the measured value side by side, and every value that no
input file carries is printed as `NOT_MEASURED`.

## Measurement rules

- **Deterministic fixtures.** Every fixture is generated from a fixed seed by
  `crates/bench/src/fixtures.rs`, so two runs see byte identical inputs. The
  fixture hash of every run is part of its evidence.
- **Distributions, not verdicts.** The benchmarks report p50, p95, and p99 of
  every stage. They never assert a timing threshold, because a shared
  machine is not a measurement device. The verdict belongs to the reviewer.
- **Stable names.** The benchmark names are stable, so results stay comparable
  over time.
- **No timing assertions.** No benchmark panics, fails, or reports a pass or
  fail verdict based on a duration.

## Known limitations of the crate

1. The scheduler, the metadata cache, the normalizer, and the asset store are
   the shipped `figtether-core` implementation, so the numbers describe the
   daemon code path. What is missing is the editor half of the system, not
   the domain half.
2. The editor dependent stages, the plugin handler (`plugin_ms`) and the
   editor export render (`render_ms`), are `NOT_MEASURED` and need a Figma
   Desktop session.
3. The export payload is a deterministic byte stand in, not a rendered PNG.
4. The resident set size is the resident set size of the benchmark process,
   not of the daemon, and it includes the criterion harness.
5. The idle phase of N04 is not measured here.
6. The soak uses mock sessions. The real editor session that N05 requires is
   not part of this crate.

## Layout

```text
crates/bench/
  src/
    domain.rs        the in-process domain: the shipped core types wired to the contract types
    evidence.rs      the release evidence model and the report renderer
    fixtures.rs      the deterministic fixture corpus
    host.rs          the operating system, processor, and memory context
    results.rs       the result file writer
    rss.rs           the resident set size sampler
    stats.rs         the percentile helpers
    taxonomy.rs      the failure taxonomy of the benchmark plan
    bin/report.rs    the report generator
  tests/
    shipped_core.rs  the guard that keeps the benchmark on the shipped core
  benches/
    w1_cached.rs     the cached metadata profile
    w2_fresh.rs      the fresh sequential profile
    w3_export.rs     the export profile
    w4_soak.rs       the mixed soak profile
    w5_decomposition.rs  the stage decomposition profile
benchmarks/
  README.md          this file
  results/           the result files of the runs
```
