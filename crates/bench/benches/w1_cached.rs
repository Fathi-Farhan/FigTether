//! W1, the cached metadata profile of the benchmark plan.
//!
//! The profile performs 10000 cached reads over the small and the medium
//! fixture, alternating the fixture every 1000 requests, and reports the
//! cached hit latency and the encoded response size against the 64 KiB
//! response budget and the 100 ms p95 target of N01.
//!
//! The reads are served by the in-process metadata cache. No Figma editor is
//! attached, so this is an offline domain measurement and never a live read
//! measurement.

use std::time::{Duration, Instant};

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use figtether_bench::domain::{metadata_fields, Domain, METADATA_FIELDS};
use figtether_bench::evidence::{Denominators, ErrorSummary, MeasurementBundle, Metric};
use figtether_bench::fixtures::{medium_fixture, small_fixture, written_hash};
use figtether_bench::results;
use figtether_bench::stats::Distribution;
use figtether_model::ids::SessionId;
use serde::Serialize;
use std::hint::black_box;

/// Number of cached reads of the profile.
const CACHED_READS: usize = 10_000;

/// Number of requests after which the profile switches fixture.
const FIXTURE_SWITCH: usize = 1_000;

/// Explicit cache time to live of the cached profile.
///
/// The shipped default of 2 s would expire the entries in the middle of the
/// run, and an expiry would turn a cached read into a fresh read, which
/// belongs to another workload class. The cached profile therefore records
/// its own configuration.
const CACHE_TTL: Duration = Duration::from_secs(300);

/// The command that reproduces this profile.
const COMMAND: &str = "cargo bench -p figtether-bench --bench w1_cached";

/// One raw record of the run.
#[derive(Debug, Clone, Serialize)]
struct RawRow {
    /// Index of the request.
    index: usize,
    /// Fixture the request was served from.
    fixture: String,
    /// Whether the cache answered.
    hit: bool,
    /// End to end latency in milliseconds.
    e2e_ms: f64,
    /// Size of the encoded response in bytes.
    response_bytes: usize,
    /// Size of the encoded metadata payload in bytes.
    metadata_bytes: usize,
    /// Age of the cache entry in milliseconds.
    cache_age_ms: Option<u64>,
}

/// The measured values of one run.
struct Workload {
    rows: Vec<RawRow>,
    latencies: Vec<f64>,
    elapsed_seconds: f64,
    hits: u64,
    misses: u64,
    within_budget: u64,
    over_budget: u64,
    fixture_hashes: Vec<String>,
    fixture_file_hashes: Vec<String>,
    /// Counters of the shipped cache, summed over the fixtures.
    cache_hits: u64,
    cache_misses: u64,
    cache_evictions: u64,
}

/// Run the cached profile once.
fn run_cached_workload() -> Workload {
    let small = small_fixture();
    let medium = medium_fixture();
    let session = SessionId::new();
    let mut domains = Vec::new();
    let mut keys = Vec::new();
    for fixture in [&small, &medium] {
        let mut domain = Domain::from_fixture(fixture);
        domain
            .warm_cache(&session, fixture, CACHE_TTL)
            .expect("the cache warms");
        let mut fixture_keys = Vec::new();
        for page in fixture.pages(domain.limits.max_batch_node_ids) {
            fixture_keys.push(domain.cache_key(&session, &page, &metadata_fields()));
        }
        keys.push(fixture_keys);
        domains.push(domain);
    }

    let mut rows = Vec::with_capacity(CACHED_READS);
    let mut latencies = Vec::with_capacity(CACHED_READS);
    let mut hits = 0u64;
    let mut misses = 0u64;
    let mut within_budget = 0u64;
    let mut over_budget = 0u64;
    let started = Instant::now();
    for index in 0..CACHED_READS {
        let which = (index / FIXTURE_SWITCH) % domains.len();
        let page = index % keys[which].len();
        let outcome = domains[which]
            .cached_read(&session, &keys[which][page])
            .expect("the cached read succeeds");
        if outcome.hit {
            hits += 1;
            latencies.push(outcome.e2e_ms);
            if outcome.response_bytes <= domains[which].limits.max_json_response_bytes {
                within_budget += 1;
            } else {
                over_budget += 1;
            }
        } else {
            misses += 1;
        }
        rows.push(RawRow {
            index,
            fixture: if which == 0 {
                small.name.clone()
            } else {
                medium.name.clone()
            },
            hit: outcome.hit,
            e2e_ms: outcome.e2e_ms,
            response_bytes: outcome.response_bytes,
            metadata_bytes: outcome.metadata_bytes,
            cache_age_ms: outcome.cache_age_ms,
        });
    }
    let elapsed_seconds = started.elapsed().as_secs_f64();
    let mut cache_hits = 0u64;
    let mut cache_misses = 0u64;
    let mut cache_evictions = 0u64;
    for domain in &domains {
        let stats = domain.cache.stats();
        cache_hits += stats.hits;
        cache_misses += stats.misses;
        cache_evictions += stats.evictions;
    }
    Workload {
        rows,
        latencies,
        elapsed_seconds,
        hits,
        misses,
        within_budget,
        over_budget,
        fixture_hashes: vec![small.hash.clone(), medium.hash.clone()],
        fixture_file_hashes: vec![
            written_hash(&small).unwrap_or_else(|_| small.hash.clone()),
            written_hash(&medium).unwrap_or_else(|_| medium.hash.clone()),
        ],
        cache_hits,
        cache_misses,
        cache_evictions,
    }
}

/// Turn a workload into the measurement bundle of the run.
fn build_bundle(workload: &Workload) -> MeasurementBundle {
    let distribution = Distribution::from_samples(&workload.latencies);
    let mut bundle = results::new_bundle("N01", "W1 cached metadata", "cached", COMMAND);
    bundle.duration_seconds = Some(workload.elapsed_seconds);
    bundle.samples = Some(workload.hits);
    bundle.latency.insert(
        "e2e".to_string(),
        distribution.unwrap_or_else(Distribution::empty),
    );
    let throughput = if workload.elapsed_seconds > 0.0 {
        (CACHED_READS as f64) / workload.elapsed_seconds
    } else {
        0.0
    };
    let hit_rate = (workload.hits as f64) / (CACHED_READS as f64);
    bundle.metrics.insert(
        "cached_read.e2e.p50_ms".to_string(),
        Metric::new(distribution.map(|value| value.p50).unwrap_or(0.0), "ms"),
    );
    bundle.metrics.insert(
        "cached_read.e2e.p95_ms".to_string(),
        Metric::new(distribution.map(|value| value.p95).unwrap_or(0.0), "ms"),
    );
    bundle.metrics.insert(
        "cached_read.e2e.p99_ms".to_string(),
        Metric::new(distribution.map(|value| value.p99).unwrap_or(0.0), "ms"),
    );
    bundle.metrics.insert(
        "cached_read.response_bytes.max".to_string(),
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
        "cached_read.metadata_bytes.max".to_string(),
        Metric::new(
            workload
                .rows
                .iter()
                .map(|row| row.metadata_bytes)
                .max()
                .unwrap_or(0) as f64,
            "bytes",
        ),
    );
    bundle.metrics.insert(
        "cached_read.throughput.req_per_s".to_string(),
        Metric::new(throughput, "req/s"),
    );
    bundle.metrics.insert(
        "cached_read.hit_rate".to_string(),
        Metric::with_display(hit_rate * 100.0, "%", format!("{:.3} %", hit_rate * 100.0)),
    );
    bundle.metrics.insert(
        "cached_read.misses".to_string(),
        Metric::new(workload.misses as f64, "reads"),
    );
    bundle.metrics.insert(
        "cached_read.cache_hits".to_string(),
        Metric::new(workload.cache_hits as f64, "hits"),
    );
    bundle.metrics.insert(
        "cached_read.cache_misses".to_string(),
        Metric::new(workload.cache_misses as f64, "misses"),
    );
    bundle.metrics.insert(
        "cached_read.cache_evictions".to_string(),
        Metric::new(workload.cache_evictions as f64, "evictions"),
    );
    bundle.denominators = Some(Denominators {
        total: CACHED_READS as u64,
        completed: workload.hits,
        passed: workload.within_budget,
        failed: 0,
        blocked: workload.over_budget,
        timeout: 0,
        unsupported: 0,
        cancelled: 0,
    });
    bundle.errors = Some(ErrorSummary::default());
    bundle.notes.push(format!(
        "cache time to live of the cached profile: {} ms (the shipped default of 2000 ms would expire mid run)",
        CACHE_TTL.as_millis()
    ));
    bundle.notes.push(format!(
        "fixed field projection: {}",
        METADATA_FIELDS.join(", ")
    ));
    bundle.notes.push(format!(
        "fixture hashes: {}",
        workload.fixture_hashes.join(", ")
    ));
    bundle.notes.push(format!(
        "fixture file hashes: {}",
        workload.fixture_file_hashes.join(", ")
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
    let evidence = results::run_evidence("N01", "cached", &workload.fixture_hashes, COMMAND);
    match results::finish_run("N01", &workload.rows, &bundle, evidence) {
        Ok(path) => println!("w1_cached: results written to {}", path.display()),
        Err(error) => println!("w1_cached: the results were not written: {error}"),
    }
}

/// The benchmark of the cached metadata profile.
fn bench_w1_cached(c: &mut Criterion) {
    let mut group = c.benchmark_group("w1_cached");
    group.throughput(Throughput::Elements(CACHED_READS as u64));
    let mut last: Option<Workload> = None;
    group.bench_function("cached_metadata_reads", |b| {
        b.iter(|| {
            let workload = run_cached_workload();
            black_box(&workload);
            last = Some(workload);
        })
    });
    group.finish();
    if let Some(workload) = last {
        write_results(&workload);
    }
}

criterion_group!(benches, bench_w1_cached);
criterion_main!(benches);
