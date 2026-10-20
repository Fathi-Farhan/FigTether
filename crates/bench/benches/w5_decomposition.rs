//! W5, the pipeline decomposition of the benchmark plan.
//!
//! The profile measures the four stages of a request separately, so the
//! decomposition the plan asks for is reproducible: the queue, the transport,
//! the normalization, and the response rendering.
//!
//! Two stages of the plan cannot be measured here. The editor handler and
//! the editor export render need a running Figma session, so they are
//! reported as `NOT_MEASURED` and never estimated.

use std::time::Instant;

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use figtether_bench::domain::{project_metadata, render_response, Domain, METADATA_FIELDS};
use figtether_bench::evidence::{Denominators, ErrorSummary, MeasurementBundle, Metric};
use figtether_bench::fixtures::{small_fixture, written_hash};
use figtether_bench::results;
use figtether_bench::stats::Distribution;
use figtether_core::normalize_document;
use figtether_core::scheduler::{Job, JobKind, Priority};
use figtether_model::ids::{JobId, SessionId};
use figtether_model::{Consistency, Coverage, Freshness};
use figtether_protocol::{BridgeFrame, MessageType};
use serde::Serialize;
use std::hint::black_box;

/// Number of stage samples of one decomposition run.
const STAGE_SAMPLES: usize = 2_000;

/// The command that reproduces this profile.
const COMMAND: &str = "cargo bench -p figtether-bench --bench w5_decomposition";

/// One raw record of the run.
#[derive(Debug, Clone, Serialize)]
struct RawRow {
    /// Stage of the sample.
    stage: String,
    /// Index of the sample.
    index: usize,
    /// Duration of the stage in milliseconds.
    duration_ms: f64,
}

/// The measured values of one run.
struct Decomposition {
    rows: Vec<RawRow>,
    queue: Vec<f64>,
    transport: Vec<f64>,
    normalization: Vec<f64>,
    render: Vec<f64>,
    fixture_hash: String,
    fixture_file_hash: String,
    elapsed_seconds: f64,
}

/// Run the decomposition once.
fn run_decomposition() -> Decomposition {
    let fixture = small_fixture();
    let session = SessionId::new();
    let domain = Domain::from_fixture(&fixture);
    let page = fixture.pages(domain.limits.max_batch_node_ids)[0].clone();
    let mut rows = Vec::with_capacity(STAGE_SAMPLES * 4);
    let mut queue = Vec::with_capacity(STAGE_SAMPLES);
    let mut transport = Vec::with_capacity(STAGE_SAMPLES);
    let mut normalization = Vec::with_capacity(STAGE_SAMPLES);
    let mut render = Vec::with_capacity(STAGE_SAMPLES);

    let started = Instant::now();
    for index in 0..STAGE_SAMPLES {
        let enqueued_at = Instant::now();
        let job = Job {
            job_id: JobId::new(),
            kind: JobKind::Read,
            session_id: Some(session),
            client_id: "bench-client-1".to_string(),
            params: serde_json::json!({ "node_ids": page }),
            deadline: enqueued_at + domain.limits.read_timeout,
            enqueued_at,
            priority: Priority::SmallRead,
            attempt: 0,
        };
        let job_id = domain.scheduler.submit(job).expect("the job is admitted");
        let granted_at = Instant::now();
        let granted = domain.scheduler.next_ready().expect("the job runs");
        let queue_ms = granted.queue_wait(granted_at).as_secs_f64() * 1000.0;
        domain
            .scheduler
            .complete(&job_id, figtether_core::scheduler::JobStatus::Completed);
        queue.push(queue_ms);
        rows.push(RawRow {
            stage: "queue".to_string(),
            index,
            duration_ms: queue_ms,
        });

        let transport_started = Instant::now();
        let frame = BridgeFrame::new(
            MessageType::Request,
            job_id.to_string(),
            serde_json::json!({
                "op": "read_nodes",
                "params": { "node_ids": page },
            }),
        )
        .with_session(session)
        .with_deadline_ms(domain.limits.read_timeout.as_millis() as u64);
        let encoded = frame.to_bytes().expect("the frame encodes");
        BridgeFrame::from_bytes(&encoded).expect("the frame decodes");
        let transport_ms = transport_started.elapsed().as_secs_f64() * 1000.0;
        transport.push(transport_ms);
        rows.push(RawRow {
            stage: "transport".to_string(),
            index,
            duration_ms: transport_ms,
        });

        let normalization_started = Instant::now();
        let (nodes, unavailable) = normalize_document(&fixture.document);
        let normalization_ms = normalization_started.elapsed().as_secs_f64() * 1000.0;
        normalization.push(normalization_ms);
        rows.push(RawRow {
            stage: "normalization".to_string(),
            index,
            duration_ms: normalization_ms,
        });

        let render_started = Instant::now();
        let projection = project_metadata(&nodes);
        let bytes = render_response(
            &session,
            serde_json::json!({ "nodes": projection }),
            Coverage {
                root_ids: page.clone(),
                returned_nodes: nodes.len() as u32,
                truncated: false,
                next_cursor: None,
                unavailable_fields: unavailable,
            },
            Consistency::Fresh,
            Freshness::FreshRead,
            None,
        );
        black_box(bytes);
        let render_ms = render_started.elapsed().as_secs_f64() * 1000.0;
        render.push(render_ms);
        rows.push(RawRow {
            stage: "render".to_string(),
            index,
            duration_ms: render_ms,
        });
    }

    Decomposition {
        rows,
        queue,
        transport,
        normalization,
        render,
        fixture_hash: fixture.hash.clone(),
        fixture_file_hash: written_hash(&fixture).unwrap_or_else(|_| fixture.hash.clone()),
        elapsed_seconds: started.elapsed().as_secs_f64(),
    }
}

/// Turn a decomposition into the measurement bundle of the run.
fn build_bundle(decomposition: &Decomposition) -> MeasurementBundle {
    let queue = Distribution::from_samples(&decomposition.queue);
    let transport = Distribution::from_samples(&decomposition.transport);
    let normalization = Distribution::from_samples(&decomposition.normalization);
    let render = Distribution::from_samples(&decomposition.render);
    let mut bundle =
        results::new_bundle("W5", "W5 pipeline decomposition", "decomposition", COMMAND);
    bundle.samples = Some(STAGE_SAMPLES as u64);
    bundle.duration_seconds = Some(decomposition.elapsed_seconds);
    bundle.latency.insert(
        "queue".to_string(),
        queue.unwrap_or_else(Distribution::empty),
    );
    bundle.latency.insert(
        "transport".to_string(),
        transport.unwrap_or_else(Distribution::empty),
    );
    bundle.latency.insert(
        "normalization".to_string(),
        normalization.unwrap_or_else(Distribution::empty),
    );
    bundle.latency.insert(
        "render".to_string(),
        render.unwrap_or_else(Distribution::empty),
    );
    bundle.metrics.insert(
        "w5.queue.p95_ms".to_string(),
        Metric::new(queue.map(|value| value.p95).unwrap_or(0.0), "ms"),
    );
    bundle.metrics.insert(
        "w5.transport.p95_ms".to_string(),
        Metric::new(transport.map(|value| value.p95).unwrap_or(0.0), "ms"),
    );
    bundle.metrics.insert(
        "w5.normalization.p95_ms".to_string(),
        Metric::new(normalization.map(|value| value.p95).unwrap_or(0.0), "ms"),
    );
    bundle.metrics.insert(
        "w5.render.p95_ms".to_string(),
        Metric::new(render.map(|value| value.p95).unwrap_or(0.0), "ms"),
    );
    bundle.denominators = Some(Denominators {
        total: STAGE_SAMPLES as u64,
        completed: STAGE_SAMPLES as u64,
        passed: STAGE_SAMPLES as u64,
        failed: 0,
        blocked: 0,
        timeout: 0,
        unsupported: 0,
        cancelled: 0,
    });
    bundle.errors = Some(ErrorSummary::default());
    bundle
        .notes
        .push(format!("fixture hash: {}", decomposition.fixture_hash));
    bundle.notes.push(format!(
        "fixture file hash: {}",
        decomposition.fixture_file_hash
    ));
    bundle.notes.push(format!(
        "fixed field projection: {}",
        METADATA_FIELDS.join(", ")
    ));
    bundle.notes.push(
        "the editor handler and the editor export render are NOT_MEASURED: they need a running Figma session"
            .to_string(),
    );
    bundle
}

/// Write the result files of the run.
fn write_results(decomposition: &Decomposition) {
    let bundle = build_bundle(decomposition);
    let evidence = results::run_evidence(
        "W5",
        "decomposition",
        std::slice::from_ref(&decomposition.fixture_hash),
        COMMAND,
    );
    match results::finish_run("W5", &decomposition.rows, &bundle, evidence) {
        Ok(path) => println!("w5_decomposition: results written to {}", path.display()),
        Err(error) => println!("w5_decomposition: the results were not written: {error}"),
    }
}

/// The benchmark of the pipeline decomposition.
fn bench_w5_decomposition(c: &mut Criterion) {
    let mut group = c.benchmark_group("w5_decomposition");
    group.throughput(Throughput::Elements(STAGE_SAMPLES as u64));
    let mut last: Option<Decomposition> = None;
    group.bench_function("pipeline_stages", |b| {
        b.iter(|| {
            let decomposition = run_decomposition();
            black_box(&decomposition);
            last = Some(decomposition);
        })
    });
    group.finish();
    if let Some(decomposition) = last {
        write_results(&decomposition);
    }
}

criterion_group!(benches, bench_w5_decomposition);
criterion_main!(benches);
