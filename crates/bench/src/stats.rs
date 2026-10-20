//! Distribution helpers.
//!
//! The benchmark plan requires p50, p95, and p99 for every stage and for the
//! end-to-end latency of every request. Criterion reports a mean and a median
//! of its own sampling, so the benchmark crate computes the percentiles it
//! publishes from the full population of the run.
//!
//! The percentile method is the nearest-rank method: for a sorted sample of
//! `n` values, the `q` percentile is the value at index
//! `ceil(q / 100 * n) - 1`. The method is stated here because a percentile
//! without a method is not a measurement.

use serde::{Deserialize, Serialize};

/// Summary of a sample distribution.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Distribution {
    /// Number of samples.
    pub count: usize,
    /// Smallest sample.
    pub min: f64,
    /// Largest sample.
    pub max: f64,
    /// Arithmetic mean.
    pub mean: f64,
    /// 50th percentile.
    pub p50: f64,
    /// 95th percentile.
    pub p95: f64,
    /// 99th percentile.
    pub p99: f64,
}

impl Distribution {
    /// Summarize a sample. Returns `None` for an empty sample.
    pub fn from_samples(samples: &[f64]) -> Option<Distribution> {
        if samples.is_empty() {
            return None;
        }
        let mut sorted = samples.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let sum: f64 = sorted.iter().sum();
        Some(Distribution {
            count: sorted.len(),
            min: sorted[0],
            max: sorted[sorted.len() - 1],
            mean: sum / sorted.len() as f64,
            p50: percentile(&sorted, 50.0),
            p95: percentile(&sorted, 95.0),
            p99: percentile(&sorted, 99.0),
        })
    }

    /// Whether the distribution holds no sample.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// A distribution without any sample.
    pub fn empty() -> Distribution {
        Distribution {
            count: 0,
            min: 0.0,
            max: 0.0,
            mean: 0.0,
            p50: 0.0,
            p95: 0.0,
            p99: 0.0,
        }
    }
}

/// Nearest-rank percentile of an already sorted slice.
pub fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let rank = ((q / 100.0) * (sorted.len() as f64)).ceil().max(1.0) as usize;
    let index = rank.min(sorted.len()) - 1;
    sorted[index]
}

/// Render a number with at most three decimals and no trailing zeros.
pub fn format_number(value: f64) -> String {
    if !value.is_finite() {
        return "NOT_A_NUMBER".to_string();
    }
    let mut text = format!("{value:.3}");
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_rank_percentiles_are_stable() {
        let samples: Vec<f64> = (1..=100).map(f64::from).collect();
        let distribution = Distribution::from_samples(&samples).expect("summarized");
        assert_eq!(distribution.count, 100);
        assert_eq!(distribution.min, 1.0);
        assert_eq!(distribution.max, 100.0);
        assert_eq!(distribution.p50, 50.0);
        assert_eq!(distribution.p95, 95.0);
        assert_eq!(distribution.p99, 99.0);
    }

    #[test]
    fn an_empty_sample_has_no_distribution() {
        assert!(Distribution::from_samples(&[]).is_none());
    }

    #[test]
    fn numbers_are_rendered_without_trailing_zeros() {
        assert_eq!(format_number(1.5), "1.5");
        assert_eq!(format_number(2.0), "2");
        assert_eq!(format_number(0.125), "0.125");
    }
}
