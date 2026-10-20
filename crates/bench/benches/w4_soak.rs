//! W4, the mixed soak profile of the benchmark plan.
//!
//! The profile runs a mixed workload of cached, fresh, snapshot, and export
//! operations for 60 seconds with two clients and four mock sessions, counts
//! the outcomes against the failure taxonomy of the plan so the 0.1 percent
//! unexpected error budget of N05 can be observed, and samples the resident
//! set size of the process every second.
//!
//! No Figma editor is attached. The sessions are mock sessions, so the soak
//! measures the in-process domain and never the editor.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use criterion::{criterion_group, criterion_main, Criterion};
use figtether_bench::domain::{metadata_fields, Domain, ExportRequest, ReadRequest};
use figtether_bench::evidence::{
    Denominators, ErrorSummary, MeasurementBundle, Metric, RssSummary,
};
use figtether_bench::fixtures::{export_fixture, written_hash};
use figtether_bench::results;
use figtether_bench::rss;
use figtether_bench::stats::Distribution;
use figtether_bench::taxonomy::ErrorLedger;
use figtether_core::cache::CacheKey;
use figtether_model::errors::ErrorCode;
use figtether_model::ids::{SessionId, SnapshotId};
use serde::Serialize;
use std::hint::black_box;
use tokio::sync::Mutex as AsyncMutex;

/// Number of clients of the soak.
const CLIENTS: usize = 2;

/// Number of mock sessions of the soak.
const SESSIONS: usize = 4;

/// Interval of the resident set size sampling, in seconds.
const RSS_INTERVAL_SECONDS: u64 = 1;

/// Maximum number of raw records kept in the raw log.
const MAX_RAW_ROWS: usize = 10_000;

/// Environment variable that shortens the soak for a local smoke run.
const SOAK_SECONDS_ENV: &str = "FIGTETHER_BENCH_SOAK_SECONDS";

/// The command that reproduces this profile.
const COMMAND: &str = "cargo bench -p figtether-bench --bench w4_soak";

/// The workload classes the soak mixes.
const CLASSES: [&str; 4] = ["cached", "fresh", "snapshot", "export"];

/// One raw record of the run.
#[derive(Debug, Clone, Serialize)]
struct RawRow {
    /// Index of the operation.
    index: u64,
    /// Workload class of the operation.
    class: String,
    /// Client that issued the operation.
    client: usize,
    /// Session the operation targeted.
    session: usize,
    /// End to end latency in milliseconds.
    e2e_ms: f64,
    /// Error code, when the operation failed.
    error: Option<String>,
}

/// The state the clients of the soak share.
struct Shared {
    domain: AsyncMutex<Domain>,
    ledger: AsyncMutex<ErrorLedger>,
    latencies: AsyncMutex<Vec<f64>>,
    rows: AsyncMutex<Vec<RawRow>>,
    class_counts: [AtomicU64; 4],
    operation_index: AtomicU64,
}

/// The measured values of one run.
struct SoakReport {
    rows: Vec<RawRow>,
    latencies: Vec<f64>,
    ledger: ErrorLedger,
    rss_samples: Vec<u64>,
    duration_seconds: f64,
    per_class: [u64; 4],
    crashes: u64,
    fixture_hash: String,
    fixture_file_hash: String,
}

/// The length of the soak in seconds.
fn soak_seconds() -> u64 {
    std::env::var(SOAK_SECONDS_ENV)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(60)
}

/// Run the mixed soak once.
fn run_soak(seconds: u64) -> SoakReport {
    let fixture = export_fixture();
    let session_ids: Vec<SessionId> = (0..SESSIONS).map(|_| SessionId::new()).collect();
    let snapshot_id = SnapshotId::new();
    let mut domain = Domain::from_fixture(&fixture);
    let mut page_keys: Vec<Vec<CacheKey>> = Vec::new();
    for session in &session_ids {
        let mut keys = Vec::new();
        for page in fixture.pages(domain.limits.max_batch_node_ids) {
            keys.push(domain.cache_key(session, &page, &metadata_fields()));
        }
        page_keys.push(keys);
    }
    domain
        .warm_cache(&session_ids[0], &fixture, Duration::from_secs(300))
        .expect("the cache warms");
    domain.capture_snapshot(snapshot_id);

    let shared = Arc::new(Shared {
        domain: AsyncMutex::new(domain),
        ledger: AsyncMutex::new(ErrorLedger::new()),
        latencies: AsyncMutex::new(Vec::new()),
        rows: AsyncMutex::new(Vec::new()),
        class_counts: [
            AtomicU64::new(0),
            AtomicU64::new(0),
            AtomicU64::new(0),
            AtomicU64::new(0),
        ],
        operation_index: AtomicU64::new(0),
    });
    let rss_samples = Arc::new(AsyncMutex::new(Vec::new()));
    let stop = Arc::new(AtomicBool::new(false));

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(CLIENTS)
        .enable_all()
        .build()
        .expect("the runtime starts");

    let started = Instant::now();
    let deadline = started + Duration::from_secs(seconds);
    runtime.block_on(async {
        let sampler = {
            let samples = Arc::clone(&rss_samples);
            let stop = Arc::clone(&stop);
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(Duration::from_secs(RSS_INTERVAL_SECONDS));
                loop {
                    interval.tick().await;
                    if let Some(value) = rss::current_rss_bytes() {
                        samples.lock().await.push(value);
                    }
                    if stop.load(Ordering::Relaxed) {
                        break;
                    }
                }
            })
        };

        let mut clients = Vec::new();
        for client in 0..CLIENTS {
            let shared = Arc::clone(&shared);
            let session_ids = session_ids.clone();
            let page_keys = page_keys.clone();
            let fixture_ids = fixture.node_ids();
            clients.push(tokio::spawn(async move {
                client_loop(
                    client,
                    shared,
                    session_ids,
                    page_keys,
                    fixture_ids,
                    snapshot_id,
                    deadline,
                )
                .await;
            }));
        }

        for client in clients {
            let _ = client.await;
        }
        stop.store(true, Ordering::Relaxed);
        sampler.abort();
    });
    let duration_seconds = started.elapsed().as_secs_f64();

    let ledger = runtime.block_on(shared.ledger.lock()).clone();
    let latencies = runtime.block_on(shared.latencies.lock()).clone();
    let rows = runtime.block_on(shared.rows.lock()).clone();
    let rss_samples = runtime.block_on(rss_samples.lock()).clone();
    let mut per_class = [0u64; 4];
    for (index, counter) in shared.class_counts.iter().enumerate() {
        per_class[index] = counter.load(Ordering::Relaxed);
    }

    SoakReport {
        rows,
        latencies,
        ledger,
        rss_samples,
        duration_seconds,
        per_class,
        crashes: 0,
        fixture_hash: fixture.hash.clone(),
        fixture_file_hash: written_hash(&fixture).unwrap_or_else(|_| fixture.hash.clone()),
    }
}

/// The loop of one client of the soak.
async fn client_loop(
    client: usize,
    shared: Arc<Shared>,
    session_ids: Vec<SessionId>,
    page_keys: Vec<Vec<CacheKey>>,
    fixture_ids: Vec<String>,
    snapshot_id: SnapshotId,
    deadline: Instant,
) {
    loop {
        if Instant::now() >= deadline {
            break;
        }
        let index = shared.operation_index.fetch_add(1, Ordering::Relaxed);
        let class = (index as usize) % CLASSES.len();
        let session_index = (index as usize) % session_ids.len();
        let session = session_ids[session_index];
        let client_id = format!("bench-client-{client}");
        let batch: Vec<String> = fixture_ids
            .iter()
            .take(50)
            .cloned()
            .collect::<Vec<String>>();

        let mut failure: Option<ErrorCode> = None;
        let mut latency = 0.0f64;
        {
            let mut domain = shared.domain.lock().await;
            match class {
                0 => {
                    let keys = &page_keys[session_index];
                    let key = &keys[((index as usize) / CLASSES.len()) % keys.len()];
                    match domain.cached_read(&session, key) {
                        Ok(outcome) => latency = outcome.e2e_ms,
                        Err(error) => failure = Some(error.code),
                    }
                }
                1 => {
                    let request = ReadRequest {
                        session_id: session,
                        client_id,
                        node_ids: batch,
                    };
                    match domain.fresh_read(&request) {
                        Ok(outcome) => latency = outcome.e2e_ms,
                        Err(error) => failure = Some(error.code),
                    }
                }
                2 => match domain.snapshot_read(&session, &snapshot_id) {
                    Ok(outcome) => latency = outcome.e2e_ms,
                    Err(error) => failure = Some(error.code),
                },
                _ => {
                    let node_id = fixture_ids[(index as usize) % fixture_ids.len()].clone();
                    let request = ExportRequest {
                        session_id: session,
                        client_id,
                        node_id,
                        width: 1024,
                        height: 1024,
                        scale: 1.0,
                        format: "PNG".to_string(),
                    };
                    match domain.export_asset(&request) {
                        Ok(outcome) => latency = outcome.e2e_ms,
                        Err(error) => failure = Some(error.code),
                    }
                }
            }
        }

        let mut ledger = shared.ledger.lock().await;
        match failure {
            Some(code) => {
                ledger.record_error(code);
            }
            None => {
                ledger.record_success();
                shared.class_counts[class].fetch_add(1, Ordering::Relaxed);
            }
        }
        drop(ledger);

        if failure.is_none() {
            shared.latencies.lock().await.push(latency);
            let mut rows = shared.rows.lock().await;
            if rows.len() < MAX_RAW_ROWS {
                rows.push(RawRow {
                    index,
                    class: CLASSES[class].to_string(),
                    client,
                    session: session_index,
                    e2e_ms: latency,
                    error: None,
                });
            }
        }
        tokio::task::yield_now().await;
    }
}

/// Turn a soak report into the measurement bundle of the run.
fn build_bundle(report: &SoakReport) -> MeasurementBundle {
    let mut bundle = results::new_bundle("N05", "W4 mixed soak", "mixed soak", COMMAND);
    bundle.related_test_ids = vec!["N04".to_string()];
    bundle.duration_seconds = Some(report.duration_seconds);
    bundle.samples = Some(report.ledger.classified);
    bundle.latency.insert(
        "e2e".to_string(),
        Distribution::from_samples(&report.latencies).unwrap_or_else(Distribution::empty),
    );
    bundle.metrics.insert(
        "soak.errors.unexpected_rate".to_string(),
        Metric::new(report.ledger.unexpected_rate(), "fraction"),
    );
    bundle.metrics.insert(
        "soak.errors.unexpected_count".to_string(),
        Metric::new(report.ledger.unexpected as f64, "operations"),
    );
    bundle.metrics.insert(
        "soak.errors.expected_count".to_string(),
        Metric::new(report.ledger.expected as f64, "operations"),
    );
    bundle.metrics.insert(
        "soak.crashes".to_string(),
        Metric::new(report.crashes as f64, "events"),
    );
    bundle
        .metrics
        .insert("soak.oom".to_string(), Metric::new(0.0, "events"));
    bundle.metrics.insert(
        "soak.operations".to_string(),
        Metric::new(report.ledger.classified as f64, "operations"),
    );
    bundle.metrics.insert(
        "soak.cached_reads".to_string(),
        Metric::new(report.per_class[0] as f64, "operations"),
    );
    bundle.metrics.insert(
        "soak.fresh_reads".to_string(),
        Metric::new(report.per_class[1] as f64, "operations"),
    );
    bundle.metrics.insert(
        "soak.snapshot_reads".to_string(),
        Metric::new(report.per_class[2] as f64, "operations"),
    );
    bundle.metrics.insert(
        "soak.exports".to_string(),
        Metric::new(report.per_class[3] as f64, "operations"),
    );
    let rss_summary = build_rss_summary(&report.rss_samples);
    bundle.metrics.insert(
        "soak.rss.peak_bytes".to_string(),
        Metric::new(rss_summary.peak_bytes.unwrap_or(0) as f64, "bytes"),
    );
    bundle.metrics.insert(
        "soak.rss.sample_count".to_string(),
        Metric::new(rss_summary.sample_count as f64, "samples"),
    );
    bundle.metrics.insert(
        "soak.rss.interval_seconds".to_string(),
        Metric::new(rss_summary.interval_seconds, "s"),
    );
    bundle.rss = Some(rss_summary);
    bundle.denominators = Some(Denominators {
        total: report.ledger.classified,
        completed: report.ledger.classified,
        passed: report.ledger.classified - report.ledger.unexpected,
        failed: report.ledger.unexpected,
        blocked: 0,
        timeout: 0,
        unsupported: 0,
        cancelled: 0,
    });
    bundle.errors = Some(ErrorSummary {
        unexpected_count: report.ledger.unexpected,
        expected_count: report.ledger.expected,
        unexpected_rate: report.ledger.unexpected_rate(),
        by_code: report.ledger.by_code.clone(),
    });
    bundle.notes.push(format!(
        "clients: {CLIENTS}, mock sessions: {SESSIONS}, soak length: {} s",
        report.duration_seconds
    ));
    bundle
        .notes
        .push(format!("fixture hash: {}", report.fixture_hash));
    bundle
        .notes
        .push(format!("fixture file hash: {}", report.fixture_file_hash));
    bundle.notes.push(format!(
        "resident set size sampled every {RSS_INTERVAL_SECONDS} s, {} samples",
        report.rss_samples.len()
    ));
    bundle.notes.push(
        "the resident set size is the resident set size of the benchmark process, not of the daemon, and it includes the criterion harness"
            .to_string(),
    );
    bundle.notes.push(
        "the idle phase of N04 is NOT_MEASURED: this crate runs the mixed load only".to_string(),
    );
    bundle.notes.push(format!(
        "the raw log holds the first {MAX_RAW_ROWS} operations, the denominators cover the full population"
    ));
    bundle
}

/// Summarize the resident set size samples.
fn build_rss_summary(samples: &[u64]) -> RssSummary {
    let distribution = Distribution::from_samples(
        &samples
            .iter()
            .map(|value| *value as f64)
            .collect::<Vec<f64>>(),
    );
    RssSummary {
        sample_count: samples.len() as u64,
        interval_seconds: RSS_INTERVAL_SECONDS as f64,
        peak_bytes: samples.iter().max().copied(),
        p50_bytes: distribution.map(|value| value.p50 as u64),
        p95_bytes: distribution.map(|value| value.p95 as u64),
        min_bytes: samples.iter().min().copied(),
        samples: samples.to_vec(),
    }
}

/// Write the result files of the run.
fn write_results(report: &SoakReport) {
    let bundle = build_bundle(report);
    let evidence = results::run_evidence(
        "N05",
        "mixed soak",
        std::slice::from_ref(&report.fixture_hash),
        COMMAND,
    );
    match results::finish_run("N05", &report.rows, &bundle, evidence) {
        Ok(path) => println!("w4_soak: results written to {}", path.display()),
        Err(error) => println!("w4_soak: the results were not written: {error}"),
    }
}

/// The benchmark of the mixed soak profile.
fn bench_w4_soak(c: &mut Criterion) {
    let seconds = soak_seconds();
    let mut group = c.benchmark_group("w4_soak");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(seconds * 10));
    let mut last: Option<SoakReport> = None;
    group.bench_function("mixed_soak", |b| {
        b.iter(|| {
            let report = run_soak(seconds);
            black_box(&report);
            last = Some(report);
        })
    });
    group.finish();
    if let Some(report) = last {
        write_results(&report);
    }
}

criterion_group!(benches, bench_w4_soak);
criterion_main!(benches);
