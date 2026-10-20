//! Failure taxonomy of the benchmark plan.
//!
//! Section 1.3 of `docs/06-testing/04-benchmark-plan.md` pre-declares the
//! error classes an operator may expect on the valid workload. Every other
//! outcome is unexpected, and N05 passes only when the unexpected rate stays
//! below 0.1 percent of the classified operations.
//!
//! The classifier lives in the benchmark crate so that the expected set is
//! fixed before a run and cannot be widened afterwards to make a run look
//! clean.

use std::collections::BTreeMap;

use figtether_model::errors::ErrorCode;
use serde::{Deserialize, Serialize};

/// The error codes the plan declares as expected on the valid workload.
pub const EXPECTED_CODES: [ErrorCode; 13] = [
    ErrorCode::Busy,
    ErrorCode::Paused,
    ErrorCode::PluginOffline,
    ErrorCode::Timeout,
    ErrorCode::Cancelled,
    ErrorCode::PayloadTooLarge,
    ErrorCode::ScopeTooLarge,
    ErrorCode::InvalidLimit,
    ErrorCode::CursorExpired,
    ErrorCode::DocumentChanged,
    ErrorCode::CapabilityUnavailable,
    ErrorCode::NodeNotFound,
    ErrorCode::AuthExpired,
];

/// Whether a code belongs to the pre-declared expected set.
pub fn is_expected(code: ErrorCode) -> bool {
    EXPECTED_CODES.contains(&code)
}

/// How one outcome is classified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Classification {
    /// The outcome belongs to the pre-declared expected set.
    Expected,
    /// The outcome is not in the pre-declared set.
    Unexpected,
}

/// Classify one error code.
pub fn classify(code: ErrorCode) -> Classification {
    if is_expected(code) {
        Classification::Expected
    } else {
        Classification::Unexpected
    }
}

/// Counters of one benchmark run, split into expected and unexpected.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorLedger {
    /// Number of classified operations.
    pub classified: u64,
    /// Number of outcomes in the expected set.
    pub expected: u64,
    /// Number of outcomes outside the expected set.
    pub unexpected: u64,
    /// Outcomes per error code.
    pub by_code: BTreeMap<String, u64>,
}

impl ErrorLedger {
    /// Create an empty ledger.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one successful operation.
    pub fn record_success(&mut self) {
        self.classified += 1;
    }

    /// Record one failed operation and classify it.
    pub fn record_error(&mut self, code: ErrorCode) -> Classification {
        self.classified += 1;
        *self.by_code.entry(code.as_str().to_string()).or_insert(0) += 1;
        match classify(code) {
            Classification::Expected => {
                self.expected += 1;
                Classification::Expected
            }
            Classification::Unexpected => {
                self.unexpected += 1;
                Classification::Unexpected
            }
        }
    }

    /// Rate of unexpected outcomes over all classified operations.
    pub fn unexpected_rate(&self) -> f64 {
        if self.classified == 0 {
            0.0
        } else {
            (self.unexpected as f64) / (self.classified as f64)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_expected_set_matches_the_plan() {
        assert!(is_expected(ErrorCode::Busy));
        assert!(is_expected(ErrorCode::NodeNotFound));
        assert!(!is_expected(ErrorCode::InternalError));
        assert!(!is_expected(ErrorCode::Revoked));
    }

    #[test]
    fn the_ledger_splits_expected_from_unexpected() {
        let mut ledger = ErrorLedger::new();
        ledger.record_success();
        ledger.record_error(ErrorCode::Busy);
        ledger.record_error(ErrorCode::InternalError);
        assert_eq!(ledger.classified, 3);
        assert_eq!(ledger.expected, 1);
        assert_eq!(ledger.unexpected, 1);
        assert!((ledger.unexpected_rate() - 1.0 / 3.0).abs() < 1e-9);
    }
}
