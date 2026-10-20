//! Release evidence and the report renderer.
//!
//! The report is the artifact the benchmark plan section 3 asks for: the
//! release evidence template of the PRD, filled with measured values only.
//! Two rules govern every field:
//!
//! 1. A value that was not measured is printed as `NOT_MEASURED`. The report
//!    never interpolates, never estimates, and never borrows a number from a
//!    different run, a different machine, or a different workload class.
//! 2. The frozen target and the measured value are printed side by side, so a
//!    reader sees the threshold and the observation in the same row.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::stats::Distribution;

/// The placeholder of every value that was not measured.
pub const NOT_MEASURED: &str = "NOT_MEASURED";

/// One measured number with its unit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Metric {
    /// The measured value.
    pub value: f64,
    /// The unit of the value, for example `ms` or `bytes`.
    pub unit: String,
    /// A rendering of the value, when the default rendering is wrong.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
}

impl Metric {
    /// A measured number.
    pub fn new(value: f64, unit: &str) -> Self {
        Self {
            value,
            unit: unit.to_string(),
            display: None,
        }
    }

    /// A measured number with an explicit rendering.
    pub fn with_display(value: f64, unit: &str, display: impl Into<String>) -> Self {
        Self {
            value,
            unit: unit.to_string(),
            display: Some(display.into()),
        }
    }

    /// Render the metric for a report cell.
    pub fn render(&self) -> String {
        match &self.display {
            Some(display) => display.clone(),
            None => format!("{} {}", crate::stats::format_number(self.value), self.unit),
        }
    }
}

/// One frozen target of the benchmark plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    /// Key of the measured value in the measurement bundle.
    pub metric: String,
    /// Human readable name of the metric.
    pub label: String,
    /// The threshold, exactly as the plan states it.
    pub threshold: String,
    /// The denominator the threshold applies to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub denominator: Option<String>,
}

/// The frozen targets of one test id.
///
/// The thresholds are copied from `docs/00-overview/01-prd-analysis.md`
/// section 6 and from the protocols of
/// `docs/06-testing/04-benchmark-plan.md`. They are frozen before a run and
/// are never edited to match a result.
pub fn targets_for(test_id: &str) -> Vec<Target> {
    match test_id {
        "N01" => vec![
            Target {
                metric: "cached_read.e2e.p95_ms".to_string(),
                label: "Cached metadata read p95".to_string(),
                threshold: "<= 100 ms".to_string(),
                denominator: Some("10000 cached reads".to_string()),
            },
            Target {
                metric: "cached_read.response_bytes.max".to_string(),
                label: "Largest encoded response".to_string(),
                threshold: "<= 65536 bytes (64 KiB budget)".to_string(),
                denominator: Some("10000 cached reads".to_string()),
            },
            Target {
                metric: "cached_read.metadata_bytes.max".to_string(),
                label: "Largest metadata projection".to_string(),
                threshold: "<= 8192 bytes (8 KiB metadata target)".to_string(),
                denominator: Some("10000 cached reads".to_string()),
            },
            Target {
                metric: "cached_read.throughput.req_per_s".to_string(),
                label: "Sustained throughput".to_string(),
                threshold: ">= 20 req/s".to_string(),
                denominator: Some("10000 cached reads".to_string()),
            },
            Target {
                metric: "cached_read.hit_rate".to_string(),
                label: "Cache hit rate".to_string(),
                threshold: "100 % of the cached class".to_string(),
                denominator: Some("10000 cached reads".to_string()),
            },
        ],
        "N02" => vec![
            Target {
                metric: "fresh_read.e2e.p95_ms".to_string(),
                label: "Fresh read p95".to_string(),
                threshold: "<= 2000 ms".to_string(),
                denominator: Some("1000 fresh reads".to_string()),
            },
            Target {
                metric: "fresh_read.queue.p95_ms".to_string(),
                label: "Queue wait p95".to_string(),
                threshold: "recorded, part of the 2000 ms budget".to_string(),
                denominator: Some("1000 fresh reads".to_string()),
            },
            Target {
                metric: "fresh_read.normalization.p95_ms".to_string(),
                label: "Normalization p95".to_string(),
                threshold: "recorded, part of the 2000 ms budget".to_string(),
                denominator: Some("1000 fresh reads".to_string()),
            },
            Target {
                metric: "fresh_read.reached_plugin.count".to_string(),
                label: "Reads that reached the plugin".to_string(),
                threshold: "1000 of 1000".to_string(),
                denominator: Some("1000 fresh reads".to_string()),
            },
            Target {
                metric: "fresh_read.cache_substitutions".to_string(),
                label: "Cache substitutions".to_string(),
                threshold: "0".to_string(),
                denominator: Some("1000 fresh reads".to_string()),
            },
        ],
        "N03" => vec![
            Target {
                metric: "export.e2e.p95_ms".to_string(),
                label: "Export p95".to_string(),
                threshold: "<= 5000 ms".to_string(),
                denominator: Some("100 exports".to_string()),
            },
            Target {
                metric: "export.store.p95_ms".to_string(),
                label: "Asset store write p95".to_string(),
                threshold: "recorded, part of the 5000 ms budget".to_string(),
                denominator: Some("100 exports".to_string()),
            },
            Target {
                metric: "export.retrieve.p95_ms".to_string(),
                label: "Asset store read p95".to_string(),
                threshold: "recorded, part of the 5000 ms budget".to_string(),
                denominator: Some("100 exports".to_string()),
            },
            Target {
                metric: "export.completed".to_string(),
                label: "Completed exports".to_string(),
                threshold: "100 of 100".to_string(),
                denominator: Some("100 exports".to_string()),
            },
            Target {
                metric: "export.asset_bytes.mean".to_string(),
                label: "Mean asset size".to_string(),
                threshold: "recorded".to_string(),
                denominator: Some("100 exports".to_string()),
            },
            Target {
                metric: "export.render.p95_ms".to_string(),
                label: "Editor export render p95".to_string(),
                threshold: "recorded, needs a Figma session".to_string(),
                denominator: Some("100 exports".to_string()),
            },
        ],
        "N04" => vec![
            Target {
                metric: "soak.rss.peak_bytes".to_string(),
                label: "Mixed load peak resident set size".to_string(),
                threshold: "<= 268435456 bytes (256 MiB)".to_string(),
                denominator: Some("1 s samples of the soak".to_string()),
            },
            Target {
                metric: "soak.rss.idle_p95_bytes".to_string(),
                label: "Idle resident set size p95".to_string(),
                threshold: "<= 52428800 bytes (50 MiB)".to_string(),
                denominator: Some("10 min idle phase".to_string()),
            },
            Target {
                metric: "soak.rss.sample_count".to_string(),
                label: "Resident set size samples".to_string(),
                threshold: ">= 60 samples in a 60 s soak".to_string(),
                denominator: Some("1 s cadence".to_string()),
            },
        ],
        "N05" => vec![
            Target {
                metric: "soak.errors.unexpected_rate".to_string(),
                label: "Unexpected error rate".to_string(),
                threshold: "< 0.1 %".to_string(),
                denominator: Some("all classified operations".to_string()),
            },
            Target {
                metric: "soak.errors.unexpected_count".to_string(),
                label: "Unexpected errors".to_string(),
                threshold: "recorded".to_string(),
                denominator: Some("all classified operations".to_string()),
            },
            Target {
                metric: "soak.crashes".to_string(),
                label: "Crashes".to_string(),
                threshold: "0".to_string(),
                denominator: Some("the soak".to_string()),
            },
            Target {
                metric: "soak.oom".to_string(),
                label: "Out of memory events".to_string(),
                threshold: "0".to_string(),
                denominator: Some("the soak".to_string()),
            },
        ],
        "W5" => vec![
            Target {
                metric: "w5.queue.p95_ms".to_string(),
                label: "Queue stage p95".to_string(),
                threshold: "recorded".to_string(),
                denominator: Some("decomposition samples".to_string()),
            },
            Target {
                metric: "w5.transport.p95_ms".to_string(),
                label: "Transport stage p95".to_string(),
                threshold: "recorded".to_string(),
                denominator: Some("decomposition samples".to_string()),
            },
            Target {
                metric: "w5.normalization.p95_ms".to_string(),
                label: "Normalization stage p95".to_string(),
                threshold: "recorded".to_string(),
                denominator: Some("decomposition samples".to_string()),
            },
            Target {
                metric: "w5.render.p95_ms".to_string(),
                label: "Response rendering stage p95".to_string(),
                threshold: "recorded".to_string(),
                denominator: Some("decomposition samples".to_string()),
            },
            Target {
                metric: "w5.plugin.p95_ms".to_string(),
                label: "Editor handler p95".to_string(),
                threshold: "recorded, needs a Figma session".to_string(),
                denominator: Some("decomposition samples".to_string()),
            },
            Target {
                metric: "w5.editor_render.p95_ms".to_string(),
                label: "Editor export render p95".to_string(),
                threshold: "recorded, needs a Figma session".to_string(),
                denominator: Some("decomposition samples".to_string()),
            },
        ],
        _ => Vec::new(),
    }
}

/// The denominators of a run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Denominators {
    /// Operations in the denominator.
    pub total: u64,
    /// Operations that finished.
    pub completed: u64,
    /// Operations that met their threshold.
    pub passed: u64,
    /// Operations that failed.
    pub failed: u64,
    /// Operations that were refused by a limit.
    pub blocked: u64,
    /// Operations that hit their deadline.
    pub timeout: u64,
    /// Operations the platform does not support.
    pub unsupported: u64,
    /// Operations that were cancelled.
    pub cancelled: u64,
}

/// The error summary of a run.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ErrorSummary {
    /// Outcomes outside the pre-declared expected set.
    pub unexpected_count: u64,
    /// Outcomes inside the pre-declared expected set.
    pub expected_count: u64,
    /// Unexpected outcomes over all classified operations.
    pub unexpected_rate: f64,
    /// Outcomes per error code.
    #[serde(default)]
    pub by_code: BTreeMap<String, u64>,
}

/// The resident set size summary of a run.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RssSummary {
    /// Number of samples.
    pub sample_count: u64,
    /// Cadence of the samples in seconds.
    pub interval_seconds: f64,
    /// Largest sample in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub peak_bytes: Option<u64>,
    /// Median sample in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p50_bytes: Option<u64>,
    /// 95th percentile sample in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p95_bytes: Option<u64>,
    /// Smallest sample in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_bytes: Option<u64>,
    /// Every sample, in order.
    #[serde(default)]
    pub samples: Vec<u64>,
}

/// The measured values of one benchmark run.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MeasurementBundle {
    /// Test id, for example `N01`.
    pub test_id: String,
    /// The other test ids the run feeds, for example `N04` for a soak run.
    #[serde(default)]
    pub related_test_ids: Vec<String>,
    /// Workload profile, for example `W1`.
    pub profile: String,
    /// Workload class, for example `cached`.
    pub workload_class: String,
    /// The exact command that produced the run.
    pub command: String,
    /// When the run finished, in UTC.
    pub generated_at: String,
    /// Wall clock of the run in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<f64>,
    /// Number of samples behind the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub samples: Option<u64>,
    /// Measured values by metric key.
    #[serde(default)]
    pub metrics: BTreeMap<String, Metric>,
    /// Latency distributions by stage name.
    #[serde(default)]
    pub latency: BTreeMap<String, Distribution>,
    /// The denominators of the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub denominators: Option<Denominators>,
    /// The error summary of the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub errors: Option<ErrorSummary>,
    /// The resident set size summary of the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rss: Option<RssSummary>,
    /// Notes that belong to the run.
    #[serde(default)]
    pub notes: Vec<String>,
}

impl MeasurementBundle {
    /// Look up a measured value.
    pub fn metric(&self, key: &str) -> Option<&Metric> {
        self.metrics.get(key)
    }
}

/// The release evidence of the PRD template.
///
/// Every field is optional in the data model and mandatory in the report: a
/// field that carries no value is printed as `NOT_MEASURED`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReleaseEvidence {
    /// Core commit and plugin commit of the tested tree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// Hashes of the tested binary and plugin bundle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_sha256: Option<String>,
    /// Envelope schema in use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<String>,
    /// Bridge protocol version in use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<String>,
    /// Version of the daemon crate the run was built against.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daemon_version: Option<String>,
    /// Operating system of the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
    /// Processor of the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu: Option<String>,
    /// Memory of the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ram: Option<String>,
    /// Exact Figma Desktop version and build.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub figma_version: Option<String>,
    /// Hash of the plugin manifest that was loaded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_manifest_hash: Option<String>,
    /// Plan and seat context, without personal identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_plan_context: Option<String>,
    /// One hash per fixture used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixture_hash: Option<String>,
    /// Test id and the linked acceptance ids.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_id: Option<String>,
    /// Workload class of the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workload_class: Option<String>,
    /// The exact commands of the run.
    #[serde(default)]
    pub commands: Vec<String>,
    /// The thresholds and denominators fixed before the run.
    #[serde(default)]
    pub expected: Vec<String>,
    /// The measured values of the run.
    #[serde(default)]
    pub actual: Vec<String>,
    /// Wall clock of the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<String>,
    /// Latency summary of the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency: Option<String>,
    /// Resident set size summary of the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rss: Option<String>,
    /// The denominators of the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub denominators: Option<String>,
    /// The error ledger of the run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub errors: Option<String>,
    /// Network path evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network_path_evidence: Option<String>,
    /// Blocked contexts, rerun history, and operator interventions.
    #[serde(default)]
    pub known_limitations: Vec<String>,
    /// The result files of the run.
    #[serde(default)]
    pub profile_refs: Vec<String>,
    /// The independent reviewer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewer: Option<String>,
    /// When the run happened, in UTC.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tested_at: Option<String>,
}

/// Render a value that may be missing.
fn value_or_not_measured(value: &Option<String>) -> String {
    value
        .clone()
        .filter(|text| !text.trim().is_empty())
        .unwrap_or_else(|| NOT_MEASURED.to_string())
}

/// Render a list that may be empty.
fn list_or_not_measured(values: &[String]) -> String {
    if values.is_empty() {
        NOT_MEASURED.to_string()
    } else {
        values.join("<br>")
    }
}

/// Render the report of one evidence record and its measurement bundles.
pub fn render_report(evidence: &ReleaseEvidence, bundles: &[MeasurementBundle]) -> String {
    let mut report = String::new();
    let headline = match bundles.len() {
        0 => evidence
            .test_id
            .clone()
            .unwrap_or_else(|| NOT_MEASURED.to_string()),
        1 => bundles[0].test_id.clone(),
        _ => bundles
            .iter()
            .map(|bundle| bundle.test_id.clone())
            .collect::<Vec<String>>()
            .join(", "),
    };
    let profile = match bundles.len() {
        0 => NOT_MEASURED.to_string(),
        1 => bundles[0].profile.clone(),
        _ => "offline domain runs".to_string(),
    };
    let _ = writeln!(report, "# {headline} benchmark report - {profile}");
    let _ = writeln!(report);
    let _ = writeln!(
        report,
        "> Offline domain benchmark produced by the `figtether-bench` crate. The measured code is the \
         shipped `figtether-core` implementation, driven over the real contract types of the model and \
         the protocol crates; no Figma editor was attached to this run. This document is not a release \
         claim. The protocol of `docs/06-testing/04-benchmark-plan.md`, including the real editor \
         session, the frozen device profile, the commit, and the independent review, is required before \
         any number here is published."
    );
    let _ = writeln!(report);

    let _ = writeln!(report, "## Release evidence");
    let _ = writeln!(report);
    let _ = writeln!(report, "| Field | Value |");
    let _ = writeln!(report, "| --- | --- |");
    let mut row = |field: &str, value: String| {
        let _ = writeln!(report, "| {field} | {value} |");
    };
    row("commit", value_or_not_measured(&evidence.commit));
    row(
        "artifact_sha256",
        value_or_not_measured(&evidence.artifact_sha256),
    );
    row(
        "schema_version",
        value_or_not_measured(&evidence.schema_version),
    );
    row(
        "protocol_version",
        value_or_not_measured(&evidence.protocol_version),
    );
    row(
        "daemon_version",
        value_or_not_measured(&evidence.daemon_version),
    );
    row("os", value_or_not_measured(&evidence.os));
    row("cpu", value_or_not_measured(&evidence.cpu));
    row("ram", value_or_not_measured(&evidence.ram));
    row(
        "figma_version",
        value_or_not_measured(&evidence.figma_version),
    );
    row(
        "plugin_manifest_hash",
        value_or_not_measured(&evidence.plugin_manifest_hash),
    );
    row(
        "account_plan_context",
        value_or_not_measured(&evidence.account_plan_context),
    );
    row(
        "fixture_hash",
        value_or_not_measured(&evidence.fixture_hash),
    );
    row("test_id", value_or_not_measured(&evidence.test_id));
    row(
        "workload_class",
        value_or_not_measured(&evidence.workload_class),
    );
    row("commands", list_or_not_measured(&evidence.commands));
    row("expected", expected_summary(evidence, bundles));
    row("actual", actual_summary(evidence, bundles));
    row(
        "duration",
        value_or_not_measured(&evidence.duration.clone().or_else(|| duration_of(bundles))),
    );
    row(
        "latency",
        value_or_not_measured(&evidence.latency.clone().or_else(|| latency_of(bundles))),
    );
    row(
        "rss",
        value_or_not_measured(&evidence.rss.clone().or_else(|| rss_of(bundles))),
    );
    row(
        "denominators",
        value_or_not_measured(
            &evidence
                .denominators
                .clone()
                .or_else(|| denominators_of(bundles)),
        ),
    );
    row(
        "errors",
        value_or_not_measured(&evidence.errors.clone().or_else(|| errors_of(bundles))),
    );
    row(
        "network_path_evidence",
        value_or_not_measured(&evidence.network_path_evidence),
    );
    row(
        "known_limitations",
        list_or_not_measured(&evidence.known_limitations),
    );
    row("profile_refs", list_or_not_measured(&evidence.profile_refs));
    row("reviewer", value_or_not_measured(&evidence.reviewer));
    row("tested_at", value_or_not_measured(&evidence.tested_at));
    let _ = writeln!(report);

    if bundles.is_empty() {
        let _ = writeln!(report, "## Targets and measured values");
        let _ = writeln!(report);
        let _ = writeln!(
            report,
            "No measurement bundle was supplied, so every measured value is {NOT_MEASURED}."
        );
        let _ = writeln!(report);
    }

    for bundle in bundles {
        render_bundle(&mut report, bundle);
    }

    report
}

/// The wall clock of the first bundle that carries one.
fn duration_of(bundles: &[MeasurementBundle]) -> Option<String> {
    bundles.iter().find_map(|bundle| {
        bundle.duration_seconds.map(|seconds| {
            format!(
                "{} s (run {})",
                crate::stats::format_number(seconds),
                bundle.test_id
            )
        })
    })
}

/// The latency distribution of the first bundle that carries one.
fn latency_of(bundles: &[MeasurementBundle]) -> Option<String> {
    bundles.iter().find_map(|bundle| {
        bundle.latency.get("e2e").map(|distribution| {
            format!(
                "p50 {} ms, p95 {} ms, p99 {} ms over {} samples (run {})",
                crate::stats::format_number(distribution.p50),
                crate::stats::format_number(distribution.p95),
                crate::stats::format_number(distribution.p99),
                distribution.count,
                bundle.test_id
            )
        })
    })
}

/// The resident set size summary of the first bundle that carries one.
fn rss_of(bundles: &[MeasurementBundle]) -> Option<String> {
    bundles.iter().find_map(|bundle| {
        bundle.rss.as_ref().map(|rss| {
            format!(
                "{} samples every {} s, peak {} bytes (run {})",
                rss.sample_count,
                crate::stats::format_number(rss.interval_seconds),
                rss.peak_bytes
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| NOT_MEASURED.to_string()),
                bundle.test_id
            )
        })
    })
}

/// The denominators of the first bundle that carries them.
fn denominators_of(bundles: &[MeasurementBundle]) -> Option<String> {
    bundles.iter().find_map(|bundle| {
        bundle.denominators.as_ref().map(|denominators| {
            format!(
                "total {}, completed {}, passed {}, failed {}, blocked {}, timeout {}, unsupported {}, cancelled {} (run {})",
                denominators.total,
                denominators.completed,
                denominators.passed,
                denominators.failed,
                denominators.blocked,
                denominators.timeout,
                denominators.unsupported,
                denominators.cancelled,
                bundle.test_id
            )
        })
    })
}

/// The error summary of the first bundle that carries it.
fn errors_of(bundles: &[MeasurementBundle]) -> Option<String> {
    bundles.iter().find_map(|bundle| {
        bundle.errors.as_ref().map(|errors| {
            format!(
                "unexpected {}, expected {}, unexpected rate {} % (run {})",
                errors.unexpected_count,
                errors.expected_count,
                crate::stats::format_number(errors.unexpected_rate * 100.0),
                bundle.test_id
            )
        })
    })
}

/// The targets of a bundle, including the targets of the test ids it feeds.
fn all_targets(bundle: &MeasurementBundle) -> Vec<Target> {
    let mut targets = targets_for(&bundle.test_id);
    for related_id in &bundle.related_test_ids {
        targets.extend(targets_for(related_id));
    }
    targets
}

/// Render the expected column of the evidence table.
fn expected_summary(evidence: &ReleaseEvidence, bundles: &[MeasurementBundle]) -> String {
    if !evidence.expected.is_empty() {
        return list_or_not_measured(&evidence.expected);
    }
    let mut lines = Vec::new();
    for bundle in bundles {
        for target in all_targets(bundle) {
            lines.push(format!(
                "{} {}: {} ({})",
                bundle.test_id, target.label, target.threshold, target.metric
            ));
        }
    }
    if lines.is_empty() {
        NOT_MEASURED.to_string()
    } else {
        lines.join("<br>")
    }
}

/// Render the actual column of the evidence table.
fn actual_summary(evidence: &ReleaseEvidence, bundles: &[MeasurementBundle]) -> String {
    if !evidence.actual.is_empty() {
        return list_or_not_measured(&evidence.actual);
    }
    let mut lines = Vec::new();
    for bundle in bundles {
        for target in all_targets(bundle) {
            let measured = match bundle.metric(&target.metric) {
                Some(metric) => metric.render(),
                None => NOT_MEASURED.to_string(),
            };
            lines.push(format!("{} {}: {measured}", bundle.test_id, target.label));
        }
    }
    if lines.is_empty() {
        NOT_MEASURED.to_string()
    } else {
        lines.join("<br>")
    }
}

/// Render one measurement bundle.
fn render_bundle(report: &mut String, bundle: &MeasurementBundle) {
    let _ = writeln!(report, "## Run {} - {}", bundle.test_id, bundle.profile);
    let _ = writeln!(report);
    let _ = writeln!(report, "| Field | Value |");
    let _ = writeln!(report, "| --- | --- |");
    let _ = writeln!(report, "| workload_class | {} |", bundle.workload_class);
    let _ = writeln!(report, "| command | {} |", bundle.command);
    let _ = writeln!(report, "| generated_at | {} |", bundle.generated_at);
    let _ = writeln!(
        report,
        "| duration | {} |",
        match bundle.duration_seconds {
            Some(seconds) => format!("{} s", crate::stats::format_number(seconds)),
            None => NOT_MEASURED.to_string(),
        }
    );
    let _ = writeln!(
        report,
        "| samples | {} |",
        match bundle.samples {
            Some(samples) => samples.to_string(),
            None => NOT_MEASURED.to_string(),
        }
    );
    let _ = writeln!(report);

    let mut targets = targets_for(&bundle.test_id);
    for related_id in &bundle.related_test_ids {
        targets.extend(targets_for(related_id));
    }
    let _ = writeln!(report, "### Targets and measured values");
    let _ = writeln!(report);
    let _ = writeln!(
        report,
        "| Metric | Target | Measured | Denominator | Metric key |"
    );
    let _ = writeln!(report, "| --- | --- | --- | --- | --- |");
    if targets.is_empty() {
        let _ = writeln!(
            report,
            "| {NOT_MEASURED} | {NOT_MEASURED} | {NOT_MEASURED} | {NOT_MEASURED} | {NOT_MEASURED} |"
        );
    }
    for target in targets {
        let measured = match bundle.metric(&target.metric) {
            Some(metric) => metric.render(),
            None => NOT_MEASURED.to_string(),
        };
        let _ = writeln!(
            report,
            "| {} | {} | {} | {} | `{}` |",
            target.label,
            target.threshold,
            measured,
            target
                .denominator
                .clone()
                .unwrap_or_else(|| NOT_MEASURED.to_string()),
            target.metric
        );
    }
    let _ = writeln!(report);

    if !bundle.latency.is_empty() {
        let _ = writeln!(report, "### Latency distribution (ms)");
        let _ = writeln!(report);
        let _ = writeln!(
            report,
            "| Stage | Samples | p50 | p95 | p99 | min | max | mean |"
        );
        let _ = writeln!(report, "| --- | --- | --- | --- | --- | --- | --- | --- |");
        for (stage, distribution) in &bundle.latency {
            let _ = writeln!(
                report,
                "| {} | {} | {} | {} | {} | {} | {} | {} |",
                stage,
                distribution.count,
                crate::stats::format_number(distribution.p50),
                crate::stats::format_number(distribution.p95),
                crate::stats::format_number(distribution.p99),
                crate::stats::format_number(distribution.min),
                crate::stats::format_number(distribution.max),
                crate::stats::format_number(distribution.mean)
            );
        }
        let _ = writeln!(report);
    }

    if let Some(denominators) = bundle.denominators {
        let _ = writeln!(report, "### Denominators");
        let _ = writeln!(report);
        let _ = writeln!(
            report,
            "| total | completed | passed | failed | blocked | timeout | unsupported | cancelled |"
        );
        let _ = writeln!(report, "| --- | --- | --- | --- | --- | --- | --- | --- |");
        let _ = writeln!(
            report,
            "| {} | {} | {} | {} | {} | {} | {} | {} |",
            denominators.total,
            denominators.completed,
            denominators.passed,
            denominators.failed,
            denominators.blocked,
            denominators.timeout,
            denominators.unsupported,
            denominators.cancelled
        );
        let _ = writeln!(report);
    }

    if let Some(errors) = &bundle.errors {
        let _ = writeln!(report, "### Errors");
        let _ = writeln!(report);
        let _ = writeln!(
            report,
            "| unexpected_count | expected_count | unexpected_rate |"
        );
        let _ = writeln!(report, "| --- | --- | --- |");
        let rate = crate::stats::format_number(errors.unexpected_rate * 100.0);
        let _ = writeln!(
            report,
            "| {} | {} | {} % |",
            errors.unexpected_count, errors.expected_count, rate
        );
        let _ = writeln!(report);
        if !errors.by_code.is_empty() {
            let _ = writeln!(report, "| Error code | Count |");
            let _ = writeln!(report, "| --- | --- |");
            for (code, count) in &errors.by_code {
                let _ = writeln!(report, "| {code} | {count} |");
            }
            let _ = writeln!(report);
        }
    }

    if let Some(rss) = &bundle.rss {
        let _ = writeln!(report, "### Resident set size");
        let _ = writeln!(report);
        let _ = writeln!(report, "| samples | interval | min | p50 | p95 | peak |");
        let _ = writeln!(report, "| --- | --- | --- | --- | --- | --- |");
        let _ = writeln!(
            report,
            "| {} | {} s | {} | {} | {} | {} |",
            rss.sample_count,
            crate::stats::format_number(rss.interval_seconds),
            rss.min_bytes
                .map(|value| value.to_string())
                .unwrap_or_else(|| NOT_MEASURED.to_string()),
            rss.p50_bytes
                .map(|value| value.to_string())
                .unwrap_or_else(|| NOT_MEASURED.to_string()),
            rss.p95_bytes
                .map(|value| value.to_string())
                .unwrap_or_else(|| NOT_MEASURED.to_string()),
            rss.peak_bytes
                .map(|value| value.to_string())
                .unwrap_or_else(|| NOT_MEASURED.to_string()),
        );
        let _ = writeln!(report);
    }

    if !bundle.notes.is_empty() {
        let _ = writeln!(report, "### Notes");
        let _ = writeln!(report);
        for note in &bundle.notes {
            let _ = writeln!(report, "- {note}");
        }
        let _ = writeln!(report);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle_with_metric() -> MeasurementBundle {
        let mut bundle = MeasurementBundle {
            test_id: "N01".to_string(),
            profile: "W1".to_string(),
            workload_class: "cached".to_string(),
            command: "cargo bench -p figtether-bench --bench w1_cached".to_string(),
            generated_at: "2026-10-20T00:00:00Z".to_string(),
            ..MeasurementBundle::default()
        };
        bundle
            .metrics
            .insert("cached_read.e2e.p95_ms".to_string(), Metric::new(0.5, "ms"));
        bundle
    }

    #[test]
    fn a_missing_value_is_never_claimed() {
        let evidence = ReleaseEvidence::default();
        let report = render_report(&evidence, &[bundle_with_metric()]);
        assert!(report.contains("| commit | NOT_MEASURED |"));
        assert!(report.contains("| figma_version | NOT_MEASURED |"));
        assert!(report.contains("| reviewer | NOT_MEASURED |"));
    }

    #[test]
    fn a_missing_measurement_is_never_claimed() {
        let evidence = ReleaseEvidence::default();
        let report = render_report(&evidence, &[bundle_with_metric()]);
        assert!(report.contains("| Cached metadata read p95 | <= 100 ms | 0.5 ms |"));
        assert!(report.contains(
            "| Largest encoded response | <= 65536 bytes (64 KiB budget) | NOT_MEASURED |"
        ));
    }

    #[test]
    fn the_report_without_measurements_marks_every_value() {
        let evidence = ReleaseEvidence::default();
        let report = render_report(&evidence, &[]);
        assert!(report.contains(NOT_MEASURED));
        assert!(report.contains("No measurement bundle was supplied"));
    }

    #[test]
    fn the_report_names_the_shipped_core() {
        let evidence = ReleaseEvidence::default();
        let report = render_report(&evidence, &[bundle_with_metric()]);
        assert!(report.contains("shipped `figtether-core` implementation"));
    }

    #[test]
    fn every_known_target_has_a_threshold() {
        for test_id in ["N01", "N02", "N03", "N04", "N05", "W5"] {
            let targets = targets_for(test_id);
            assert!(!targets.is_empty(), "{test_id} has no targets");
            for target in targets {
                assert!(!target.threshold.is_empty());
                assert!(!target.metric.is_empty());
            }
        }
        assert!(targets_for("unknown").is_empty());
    }
}
