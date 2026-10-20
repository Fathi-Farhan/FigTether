//! Result files of a benchmark run.
//!
//! The benchmark plan files its evidence under
//! `benchmarks/results/<test id>/<commit short>/`. This module writes the
//! three files a run produces:
//!
//! - `run-<n>.raw.jsonl`, one JSON object per operation, the raw log;
//! - `run-<n>.summary.json`, the measurement bundle the report generator
//!   consumes;
//! - `run-<n>.evidence.json`, the release evidence the report generator
//!   merges with the bundle.
//!
//! The commit label comes from the `FIGTETHER_BENCH_COMMIT` environment
//! variable. When it is not set the label is `unknown-commit`, because a run
//! without a commit is not publishable and must not borrow one.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::Serialize;

use crate::evidence::{MeasurementBundle, ReleaseEvidence};
use crate::host::HostContext;

/// Environment variable that carries the tested commit.
pub const COMMIT_ENV: &str = "FIGTETHER_BENCH_COMMIT";

/// Environment variable that overrides the results directory.
pub const RESULTS_DIR_ENV: &str = "FIGTETHER_BENCH_RESULTS_DIR";

/// The root of the workspace, derived from the location of the crate.
///
/// Cargo runs a benchmark with the package directory as the working
/// directory, so a relative path would land inside the crate. The result
/// files belong to the workspace, next to the `benchmarks` directory of the
/// repository.
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// The root directory of the result files.
pub fn results_root() -> PathBuf {
    match std::env::var(RESULTS_DIR_ENV) {
        Ok(path) if !path.trim().is_empty() => PathBuf::from(path),
        _ => workspace_root().join("benchmarks").join("results"),
    }
}

/// The commit label of the run.
pub fn commit_label() -> String {
    match std::env::var(COMMIT_ENV) {
        Ok(commit) if !commit.trim().is_empty() => commit.trim().to_string(),
        _ => "unknown-commit".to_string(),
    }
}

/// The directory that holds the result files of one test id.
pub fn run_directory(test_id: &str) -> PathBuf {
    results_root().join(test_id).join(commit_label())
}

/// The next free run index of a directory.
pub fn next_run_index(directory: &Path) -> u32 {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return 1;
    };
    let mut highest = 0u32;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if let Some(rest) = name.strip_prefix("run-") {
            if let Some(number) = rest.split('.').next() {
                if let Ok(parsed) = number.parse::<u32>() {
                    highest = highest.max(parsed);
                }
            }
        }
    }
    highest + 1
}

/// Write a raw log of JSON objects.
pub fn write_jsonl<T: Serialize>(path: &Path, rows: &[T]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::File::create(path)?;
    let mut writer = std::io::BufWriter::new(file);
    for row in rows {
        let line = serde_json::to_string(row).map_err(std::io::Error::other)?;
        writeln!(writer, "{line}")?;
    }
    writer.flush()
}

/// Write one JSON document.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(value).map_err(std::io::Error::other)?;
    std::fs::write(path, bytes)
}

/// The limitations that every run of this crate carries.
///
/// They are part of the evidence, not a footnote: a number produced here is
/// an offline domain measurement and never an editor measurement.
pub fn standard_limitations() -> Vec<String> {
    vec![
        "Offline domain benchmark: no Figma editor was attached, so plugin_ms and the editor export render are NOT_MEASURED.".to_string(),
        "The scheduler, the metadata cache, the normalizer, and the asset store are the shipped figtether-core implementation, driven over the real contract types; only the editor half of the system is absent.".to_string(),
        "Not a release claim: the full protocol of docs/06-testing/04-benchmark-plan.md, a real editor session, the frozen device profile, and an independent review are required before publication.".to_string(),
    ]
}

/// Build the release evidence of a run from what the run can honestly know.
pub fn run_evidence(
    test_id: &str,
    workload_class: &str,
    fixture_hashes: &[String],
    command: &str,
) -> ReleaseEvidence {
    let host = HostContext::capture();
    let commit = std::env::var(COMMIT_ENV)
        .ok()
        .filter(|c| !c.trim().is_empty());
    ReleaseEvidence {
        commit,
        artifact_sha256: None,
        schema_version: Some(figtether_model::SCHEMA_VERSION.to_string()),
        protocol_version: Some(figtether_protocol::PROTOCOL_VERSION.to_string()),
        daemon_version: Some(figtether_daemon::DAEMON_VERSION.to_string()),
        os: Some(host.os_label()),
        cpu: Some(host.cpu_label()),
        ram: host.ram_label(),
        figma_version: None,
        plugin_manifest_hash: None,
        account_plan_context: None,
        fixture_hash: if fixture_hashes.is_empty() {
            None
        } else {
            Some(fixture_hashes.join(", "))
        },
        test_id: Some(test_id.to_string()),
        workload_class: Some(workload_class.to_string()),
        commands: vec![command.to_string()],
        expected: Vec::new(),
        actual: Vec::new(),
        duration: None,
        latency: None,
        rss: None,
        denominators: None,
        errors: None,
        network_path_evidence: Some(
            "No socket was opened: the run is in process and the domain has no network path at all. N09 capture evidence is out of scope for this crate."
                .to_string(),
        ),
        known_limitations: standard_limitations(),
        profile_refs: Vec::new(),
        reviewer: None,
        tested_at: Some(Utc::now().to_rfc3339()),
    }
}

/// Build a measurement bundle with the common fields of a run filled in.
pub fn new_bundle(
    test_id: &str,
    profile: &str,
    workload_class: &str,
    command: &str,
) -> MeasurementBundle {
    MeasurementBundle {
        test_id: test_id.to_string(),
        profile: profile.to_string(),
        workload_class: workload_class.to_string(),
        command: command.to_string(),
        generated_at: Utc::now().to_rfc3339(),
        ..MeasurementBundle::default()
    }
}

/// Write the result files of a run and return the path of the summary.
pub fn finish_run<T: Serialize>(
    test_id: &str,
    rows: &[T],
    bundle: &MeasurementBundle,
    evidence: ReleaseEvidence,
) -> std::io::Result<PathBuf> {
    let directory = run_directory(test_id);
    std::fs::create_dir_all(&directory)?;
    let run_index = next_run_index(&directory);
    let files = write_run(test_id, run_index, rows, bundle, evidence)?;
    Ok(files.summary)
}

/// The result files of a run.
pub struct RunFiles {
    /// The raw log of the run.
    pub raw: PathBuf,
    /// The measurement bundle of the run.
    pub summary: PathBuf,
    /// The release evidence of the run.
    pub evidence: PathBuf,
}

/// Write the result files of a run.
pub fn write_run<T: Serialize>(
    test_id: &str,
    run_index: u32,
    rows: &[T],
    bundle: &MeasurementBundle,
    mut evidence: ReleaseEvidence,
) -> std::io::Result<RunFiles> {
    let directory = run_directory(test_id);
    std::fs::create_dir_all(&directory)?;
    let raw = directory.join(format!("run-{run_index}.raw.jsonl"));
    let summary = directory.join(format!("run-{run_index}.summary.json"));
    let evidence_path = directory.join(format!("run-{run_index}.evidence.json"));
    evidence.profile_refs = vec![
        file_name(&raw),
        file_name(&summary),
        file_name(&evidence_path),
    ];
    write_jsonl(&raw, rows)?;
    write_json(&summary, bundle)?;
    write_json(&evidence_path, &evidence)?;
    Ok(RunFiles {
        raw,
        summary,
        evidence: evidence_path,
    })
}

/// The file name of a path.
fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_commit_label_is_never_invented() {
        let label = commit_label();
        assert!(!label.is_empty());
        if std::env::var(COMMIT_ENV).is_err() {
            assert_eq!(label, "unknown-commit");
        }
    }

    #[test]
    fn a_missing_commit_is_not_reported_as_a_commit() {
        let evidence = run_evidence("N01", "cached", &[], "cargo bench");
        if std::env::var(COMMIT_ENV).is_err() {
            assert!(evidence.commit.is_none());
        }
    }
}
