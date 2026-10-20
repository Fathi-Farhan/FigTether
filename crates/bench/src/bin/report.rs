//! The release evidence report generator.
//!
//! The tool renders the release evidence report of the benchmark plan
//! section 3 from the files a benchmark run produced. It never invents a
//! value: a field that no input file carries is printed as `NOT_MEASURED`.
//!
//! ```text
//! cargo run -p figtether-bench --bin report -- \
//!     --measurement benchmarks/results/N01/unknown-commit/run-1.summary.json \
//!     --evidence benchmarks/results/N01/unknown-commit/run-1.evidence.json \
//!     --output benchmarks/results/N01/unknown-commit/summary.md
//! ```
//!
//! Without any input the tool renders the empty template, which is useful as
//! the checklist of a run that has not happened yet.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Parser;
use figtether_bench::evidence::{render_report, MeasurementBundle, ReleaseEvidence};

/// Render a FigTether benchmark release evidence report.
#[derive(Debug, Parser)]
#[command(
    name = "figtether-bench-report",
    about = "Render the release evidence report of a FigTether benchmark run"
)]
struct Cli {
    /// Measurement bundle files written by a benchmark run. May be repeated.
    #[arg(long = "measurement", value_name = "FILE")]
    measurement: Vec<PathBuf>,
    /// Release evidence file written by a benchmark run.
    #[arg(long = "evidence", value_name = "FILE")]
    evidence: Option<PathBuf>,
    /// Where the report is written. Defaults to standard output.
    #[arg(long = "output", value_name = "FILE")]
    output: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let evidence = match &cli.evidence {
        Some(path) => read_evidence(path)
            .with_context(|| format!("failed to read the evidence file {}", path.display()))?,
        None => ReleaseEvidence::default(),
    };
    let mut bundles = Vec::new();
    for path in &cli.measurement {
        let bundle = read_bundle(path)
            .with_context(|| format!("failed to read the measurement file {}", path.display()))?;
        bundles.push(bundle);
    }
    let report = render_report(&evidence, &bundles);
    match &cli.output {
        Some(path) => write_report(path, &report)
            .with_context(|| format!("failed to write the report {}", path.display()))?,
        None => print!("{report}"),
    }
    Ok(())
}

/// Read one measurement bundle.
fn read_bundle(path: &Path) -> Result<MeasurementBundle> {
    let text = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&text)?)
}

/// Read one release evidence file.
fn read_evidence(path: &Path) -> Result<ReleaseEvidence> {
    let text = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&text)?)
}

/// Write the report to disk.
fn write_report(path: &Path, report: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    std::fs::write(path, report)?;
    Ok(())
}
