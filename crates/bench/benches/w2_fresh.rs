//! W2, the fresh sequential profile of the benchmark plan.
//!
//! The profile performs 1000 sequential fresh reads of at most 50 node ids
//! per read through the scheduler and the normalizer, and reports the queue
//! wait and the normalization cost separately against the 2 s p95 target of
//! N02.
//!
//! The reads never consult the cache, exactly like a fresh read in the
//! product. No Figma editor is attached, so `plugin_ms` and the proof that a
//! read reached the plugin are reported as `NOT_MEASURED`.

use std::time::Instant;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use figtether_bench::domain::{Domain, ReadRequest};
use figtether_bench::evidence::{Denominators, ErrorSummary, MeasurementBundle, Metric};
use figtether_bench::fixtures::{small_fixture, written_hash};
use figtether_bench::results;
use figtether_bench::stats::Distribution;
use figtether_model::ids::SessionId;
use serde::Serialize;
use std::hint::black_box;

/// Number of fresh reads of the profile.
const FRESH_READS: usize = 1_000;

/// The command that reproduces this profile.
const COMMAND: &str = "cargo bench -p figtether-bench --bench w2_fresh";

/// One raw record of the run.
#[derive(Debug, Clone, Serialize)]
struct RawRow {
    /// Index of the read.
    index: usize,
    /// Number of node ids of the batch.
    batch: usize,
    /// End to end latency in milliseconds.
    e2e_ms: f64,
    /// Queue wait in milliseconds.
    queue_ms: f64,
    /// Bridge framing in milliseconds.
    transport_ms: f64,
    /// Model build and validation in milliseconds.
    normalization_ms: f64,
    /// Response shaping in milliseconds.
    shaping_ms: f64,
    /// Size of the encoded response in bytes.
    response_bytes: usize,
}

/// The measured values of one run.
struct Workload {
    rows: Vec<RawRow>,
    e2e: Vec<f64>,
    queue: Vec<f64>,
    transport: Vec<f64>,
    normalization: Vec<f64>,
    shaping: Vec<f64>,
    elapsed_seconds: f64,
    completed: u64,
    failed: u64,
    within_budget: u64,
    over_budget: u64,
    fixture_hash: String,
    fixture_file_hash: String,
    cache_substitutions: u64,
    /// The distinct fields the shipped normalizer could not provide.
    unavailable_fields: Vec<String>,
}

/// Run the fresh profile once.
fn run_fresh_workload() -> Workload {
    let fixture = small_fixture();
    let session = SessionId::new();
    let mut domain = Domain::from_fixture(&fixture);
    let pages = fixture.pages(domain.limits.max_batch_node_ids);
    let client_id = "bench-client-1".to_string();

    let mut rows = Vec::with_capacity(FRESH_READS);
    let mut e2e = Vec::with_capacity(FRESH_READS);
    let mut queue = Vec::with_capacity(FRESH_READS);
    let mut transport = Vec::with_capacity(FRESH_READS);
    let mut normalization = Vec::with_capacity(FRESH_READS);
    let mut shaping = Vec::with_capacity(FRESH_READS);
    let mut completed = 0u64;
    let mut failed = 0u64;
    let mut within_budget = 0u64;
    let mut over_budget = 0u64;
    let mut unavailable_fields: Vec<String> = Vec::new();

    let started = Instant::now();
    for index in 0..FRESH_READS {
        let page = &pages[index % pages.len()];
        let request = ReadRequest {
            session_id: session,
            client_id: client_id.clone(),
            node_ids: page.clone(),
        };
        match domain.fresh_read(&request) {
            Ok(outcome) => {
                completed += 1;
                if outcome.within_budget {
                    within_budget += 1;
                } else {
                    over_budget += 1;
                }
                for field in &outcome.unavailable_fields {
                    if !unavailable_fields.contains(field) {
                        unavailable_fields.push(field.clone());
                    }
                }
                e2e.push(outcome.e2e_ms);
                queue.push(outcome.queue_ms.value_or_zero());
                transport.push(outcome.transport_ms.value_or_zero());
                normalization.push(outcome.normalization_ms.value_or_zero());
                shaping.push(outcome.shaping_ms.value_or_zero());
                rows.push(RawRow {
                    index,
                    batch: page.len(),
                    e2e_ms: outcome.e2e_ms,
                    queue_ms: outcome.queue_ms.value_or_zero(),
                    transport_ms: outcome.transport_ms.value_or_zero(),
                    normalization_ms: outcome.normalization_ms.value_or_zero(),
                    shaping_ms: outcome.shaping_ms.value_or_zero(),
                    response_bytes: outcome.response_bytes,
                });
            }
            Err(_) => {
                failed += 1;
            }
        }
    }
    let elapsed_seconds = started.elapsed().as_secs_f64();
    unavailable_fields.sort();
    Workload {
        rows,
        e2e,
        queue,
        transport,
        normalization,
        shaping,
        elapsed_seconds,
        completed,
        failed,
        within_budget,
        over_budget,
        fixture_hash: fixture.hash.clone(),
        fixture_file_hash: written_hash(&fixture).unwrap_or_else(|_| fixture.hash.clone()),
        cache_substitutions: 0,
        unavailable_fields,
    }
}

/// Turn a workload into the measurement bundle of the run.
fn build_bundle(workload: &Workload) -> MeasurementBundle {
    let mut bundle = results::new_bundle("N02", "W2 fresh sequential", "fresh", COMMAND);
    bundle.duration_seconds = Some(workload.elapsed_seconds);
    bundle.samples = Some(workload.completed);
    let e2e = Distribution::from_samples(&workload.e2e);
    bundle
        .latency
        .insert("e2e".to_string(), e2e.unwrap_or_else(Distribution::empty));
    bundle.latency.insert(
        "queue".to_string(),
        Distribution::from_samples(&workload.queue).unwrap_or_else(Distribution::empty),
    );
    bundle.latency.insert(
        "transport".to_string(),
        Distribution::from_samples(&workload.transport).unwrap_or_else(Distribution::empty),
    );
    bundle.latency.insert(
        "normalization".to_string(),
        Distribution::from_samples(&workload.normalization).unwrap_or_else(Distribution::empty),
    );
    bundle.latency.insert(
        "shaping".to_string(),
        Distribution::from_samples(&workload.shaping).unwrap_or_else(Distribution::empty),
    );
    bundle.metrics.insert(
        "fresh_read.e2e.p95_ms".to_string(),
        Metric::new(e2e.map(|value| value.p95).unwrap_or(0.0), "ms"),
    );
    bundle.metrics.insert(
        "fresh_read.e2e.p50_ms".to_string(),
        Metric::new(e2e.map(|value| value.p50).unwrap_or(0.0), "ms"),
    );
    bundle.metrics.insert(
        "fresh_read.e2e.p99_ms".to_string(),
        Metric::new(e2e.map(|value| value.p99).unwrap_or(0.0), "ms"),
    );
    bundle.metrics.insert(
        "fresh_read.queue.p95_ms".to_string(),
        Metric::new(
            Distribution::from_samples(&workload.queue)
                .map(|value| value.p95)
                .unwrap_or(0.0),
            "ms",
        ),
    );
    bundle.metrics.insert(
        "fresh_read.normalization.p95_ms".to_string(),
        Metric::new(
            Distribution::from_samples(&workload.normalization)
                .map(|value| value.p95)
                .unwrap_or(0.0),
            "ms",
        ),
    );
    bundle.metrics.insert(
        "fresh_read.transport.p95_ms".to_string(),
        Metric::new(
            Distribution::from_samples(&workload.transport)
                .map(|value| value.p95)
                .unwrap_or(0.0),
            "ms",
        ),
    );
    bundle.metrics.insert(
        "fresh_read.cache_substitutions".to_string(),
        Metric::new(workload.cache_substitutions as f64, "reads"),
    );
    bundle.metrics.insert(
        "fresh_read.response_bytes.max".to_string(),
        Metric::new(
            workload
                .rows
                .iter()
                .map(|row| row.response_bytes)
                .max()
                .unwrap_or(0) as f64,
            "bytes",
        ),
    );
    bundle.metrics.insert(
        "fresh_read.unavailable_fields.count".to_string(),
        Metric::new(workload.unavailable_fields.len() as f64, "fields"),
    );
    bundle.denominators = Some(Denominators {
        total: FRESH_READS as u64,
        completed: workload.completed,
        passed: workload.within_budget,
        failed: workload.failed,
        blocked: workload.over_budget,
        timeout: 0,
        unsupported: 0,
        cancelled: 0,
    });
    bundle.errors = Some(ErrorSummary::default());
    bundle
        .notes
        .push(format!("fixture hash: {}", workload.fixture_hash));
    bundle
        .notes
        .push(format!("fixture file hash: {}", workload.fixture_file_hash));
    bundle.notes.push(
        "reached_plugin is NOT_MEASURED: the offline domain has no Figma plugin session, so no read can prove that it reached the editor"
            .to_string(),
    );
    bundle.notes.push(format!(
        "unavailable fields of the response, as reported by the shipped figtether-core normalizer: {}",
        if workload.unavailable_fields.is_empty() {
            "none".to_string()
        } else {
            workload.unavailable_fields.join(", ")
        }
    ));
    bundle.notes.push(
        "the latency verdict is not computed by the benchmark; the report prints the distribution next to the frozen threshold"
            .to_string(),
    );
    bundle
}

/// Write the result files of the run.
fn write_results(workload: &Workload) {
    let bundle = build_bundle(workload);
    let evidence = results::run_evidence(
        "N02",
        "fresh",
        std::slice::from_ref(&workload.fixture_hash),
        COMMAND,
    );
    match results::finish_run("N02", &workload.rows, &bundle, evidence) {
        Ok(path) => println!("w2_fresh: results written to {}", path.display()),
        Err(error) => println!("w2_fresh: the results were not written: {error}"),
    }
}

/// The benchmark of the fresh sequential profile.
fn bench_w2_fresh(c: &mut Criterion) {
    let mut group = c.benchmark_group("w2_fresh");
    group.throughput(Throughput::Elements(FRESH_READS as u64));
    let mut last: Option<Workload> = None;
    group.bench_function("fresh_pipeline_reads", |b| {
        b.iter(|| {
            let workload = run_fresh_workload();
            black_box(&workload);
            last = Some(workload);
        })
    });
    group.finish();
    if let Some(workload) = last {
        write_results(&workload);
    }
}

criterion_group!(benches, bench_w2_fresh);
criterion_main!(benches);
