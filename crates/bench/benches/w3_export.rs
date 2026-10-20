//! W3, the export profile of the benchmark plan.
//!
//! The profile performs 100 asset exports of 1024x1024 through the asset
//! store and reports the store and the retrieval cost separately against the
//! 5 s p95 target of N03.
//!
//! The payload of an export is a deterministic byte stream, not an editor
//! render. The editor render is therefore reported as `NOT_MEASURED`, and the
//! measured stages are the queue, the chunked transfer and reassembly, the
//! asset store write, and the asset store read.

use std::time::Instant;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use figtether_bench::domain::{Domain, ExportRequest};
use figtether_bench::evidence::{Denominators, ErrorSummary, MeasurementBundle, Metric};
use figtether_bench::fixtures::{export_fixture, written_hash};
use figtether_bench::results;
use figtether_bench::stats::Distribution;
use figtether_model::ids::SessionId;
use serde::Serialize;
use std::hint::black_box;

/// Number of exports of the profile.
const EXPORTS: usize = 100;

/// Pixel width of the export.
const EXPORT_WIDTH: u32 = 1024;

/// Pixel height of the export.
const EXPORT_HEIGHT: u32 = 1024;

/// The command that reproduces this profile.
const COMMAND: &str = "cargo bench -p figtether-bench --bench w3_export";

/// One raw record of the run.
#[derive(Debug, Clone, Serialize)]
struct RawRow {
    /// Index of the export.
    index: usize,
    /// Node that was exported.
    node_id: String,
    /// End to end latency in milliseconds.
    e2e_ms: f64,
    /// Queue wait in milliseconds.
    queue_ms: f64,
    /// Chunked transfer in milliseconds.
    transport_ms: f64,
    /// Preparation of the payload stand in in milliseconds.
    payload_prepare_ms: f64,
    /// Asset store write in milliseconds.
    store_ms: f64,
    /// Asset store read in milliseconds.
    retrieve_ms: f64,
    /// Size of the stored asset in bytes.
    asset_bytes: usize,
}

/// The measured values of one run.
struct Workload {
    rows: Vec<RawRow>,
    e2e: Vec<f64>,
    queue: Vec<f64>,
    transport: Vec<f64>,
    store: Vec<f64>,
    retrieve: Vec<f64>,
    elapsed_seconds: f64,
    completed: u64,
    failed: u64,
    asset_bytes: Vec<u64>,
    fixture_hash: String,
    fixture_file_hash: String,
}

/// Run the export profile once.
fn run_export_workload() -> Workload {
    let fixture = export_fixture();
    let session = SessionId::new();
    let mut domain = Domain::from_fixture(&fixture);
    let node_ids = fixture.node_ids();
    let client_id = "bench-client-1".to_string();

    let mut rows = Vec::with_capacity(EXPORTS);
    let mut e2e = Vec::with_capacity(EXPORTS);
    let mut queue = Vec::with_capacity(EXPORTS);
    let mut transport = Vec::with_capacity(EXPORTS);
    let mut store = Vec::with_capacity(EXPORTS);
    let mut retrieve = Vec::with_capacity(EXPORTS);
    let mut asset_bytes = Vec::with_capacity(EXPORTS);
    let mut completed = 0u64;
    let mut failed = 0u64;

    let started = Instant::now();
    for index in 0..EXPORTS {
        let node_id = node_ids[index % node_ids.len()].clone();
        let request = ExportRequest {
            session_id: session,
            client_id: client_id.clone(),
            node_id: node_id.clone(),
            width: EXPORT_WIDTH,
            height: EXPORT_HEIGHT,
            scale: 1.0,
            format: "PNG".to_string(),
        };
        match domain.export_asset(&request) {
            Ok(outcome) => {
                completed += 1;
                e2e.push(outcome.e2e_ms);
                queue.push(outcome.queue_ms.value_or_zero());
                transport.push(outcome.transport_ms.value_or_zero());
                store.push(outcome.store_ms.value_or_zero());
                retrieve.push(outcome.retrieve_ms.value_or_zero());
                asset_bytes.push(outcome.asset_bytes as u64);
                rows.push(RawRow {
                    index,
                    node_id,
                    e2e_ms: outcome.e2e_ms,
                    queue_ms: outcome.queue_ms.value_or_zero(),
                    transport_ms: outcome.transport_ms.value_or_zero(),
                    payload_prepare_ms: outcome.payload_prepare_ms.value_or_zero(),
                    store_ms: outcome.store_ms.value_or_zero(),
                    retrieve_ms: outcome.retrieve_ms.value_or_zero(),
                    asset_bytes: outcome.asset_bytes,
                });
            }
            Err(_) => {
                failed += 1;
            }
        }
    }
    let elapsed_seconds = started.elapsed().as_secs_f64();
    Workload {
        rows,
        e2e,
        queue,
        transport,
        store,
        retrieve,
        elapsed_seconds,
        completed,
        failed,
        asset_bytes,
        fixture_hash: fixture.hash.clone(),
        fixture_file_hash: written_hash(&fixture).unwrap_or_else(|_| fixture.hash.clone()),
    }
}

/// Turn a workload into the measurement bundle of the run.
fn build_bundle(workload: &Workload) -> MeasurementBundle {
    let e2e = Distribution::from_samples(&workload.e2e);
    let mut bundle = results::new_bundle("N03", "W3 export", "export", COMMAND);
    bundle.duration_seconds = Some(workload.elapsed_seconds);
    bundle.samples = Some(workload.completed);
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
        "store".to_string(),
        Distribution::from_samples(&workload.store).unwrap_or_else(Distribution::empty),
    );
    bundle.latency.insert(
        "retrieve".to_string(),
        Distribution::from_samples(&workload.retrieve).unwrap_or_else(Distribution::empty),
    );
    bundle.metrics.insert(
        "export.e2e.p95_ms".to_string(),
        Metric::new(e2e.map(|value| value.p95).unwrap_or(0.0), "ms"),
    );
    bundle.metrics.insert(
        "export.e2e.p50_ms".to_string(),
        Metric::new(e2e.map(|value| value.p50).unwrap_or(0.0), "ms"),
    );
    bundle.metrics.insert(
        "export.e2e.p99_ms".to_string(),
        Metric::new(e2e.map(|value| value.p99).unwrap_or(0.0), "ms"),
    );
    bundle.metrics.insert(
        "export.store.p95_ms".to_string(),
        Metric::new(
            Distribution::from_samples(&workload.store)
                .map(|value| value.p95)
                .unwrap_or(0.0),
            "ms",
        ),
    );
    bundle.metrics.insert(
        "export.retrieve.p95_ms".to_string(),
        Metric::new(
            Distribution::from_samples(&workload.retrieve)
                .map(|value| value.p95)
                .unwrap_or(0.0),
            "ms",
        ),
    );
    bundle.metrics.insert(
        "export.queue.p95_ms".to_string(),
        Metric::new(
            Distribution::from_samples(&workload.queue)
                .map(|value| value.p95)
                .unwrap_or(0.0),
            "ms",
        ),
    );
    bundle.metrics.insert(
        "export.completed".to_string(),
        Metric::new(workload.completed as f64, "exports"),
    );
    let mean_bytes = if workload.asset_bytes.is_empty() {
        0.0
    } else {
        (workload.asset_bytes.iter().sum::<u64>() as f64) / (workload.asset_bytes.len() as f64)
    };
    bundle.metrics.insert(
        "export.asset_bytes.mean".to_string(),
        Metric::new(mean_bytes, "bytes"),
    );
    bundle.denominators = Some(Denominators {
        total: EXPORTS as u64,
        completed: workload.completed,
        passed: workload.completed,
        failed: workload.failed,
        blocked: 0,
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
    bundle.notes.push(format!(
        "export geometry: {EXPORT_WIDTH}x{EXPORT_HEIGHT} PNG at scale 1.0"
    ));
    bundle.notes.push(
        "the editor render is NOT_MEASURED: the exported bytes are a deterministic offline stand in, not a Figma render"
            .to_string(),
    );
    bundle
}

/// Write the result files of the run.
fn write_results(workload: &Workload) {
    let bundle = build_bundle(workload);
    let evidence = results::run_evidence(
        "N03",
        "export",
        std::slice::from_ref(&workload.fixture_hash),
        COMMAND,
    );
    match results::finish_run("N03", &workload.rows, &bundle, evidence) {
        Ok(path) => println!("w3_export: results written to {}", path.display()),
        Err(error) => println!("w3_export: the results were not written: {error}"),
    }
}

/// The benchmark of the export profile.
fn bench_w3_export(c: &mut Criterion) {
    let mut group = c.benchmark_group("w3_export");
    group.throughput(Throughput::Elements(EXPORTS as u64));
    let mut last: Option<Workload> = None;
    group.bench_function("asset_exports", |b| {
        b.iter(|| {
            let workload = run_export_workload();
            black_box(&workload);
            last = Some(workload);
        })
    });
    group.finish();
    if let Some(workload) = last {
        write_results(&workload);
    }
}

criterion_group!(benches, bench_w3_export);
criterion_main!(benches);
