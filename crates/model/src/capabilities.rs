//! Capability model.
//!
//! Capabilities are probed at runtime for the concrete seat, editor version,
//! and API surface. A capability is never inferred from an account name and a
//! missing value is never silently reported as "no such thing".

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// State of one probed capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CapabilityState {
    /// The capability is usable in this context.
    Available,
    /// The capability is not usable; `reason` explains why.
    Unavailable {
        /// Why the capability is unavailable.
        reason: String,
    },
    /// The capability exists in the product but not in this platform or seat.
    Unsupported {
        /// Where the capability would be supported.
        reason: String,
    },
}

impl CapabilityState {
    /// Whether the capability can be used right now.
    pub fn is_available(&self) -> bool {
        matches!(self, Self::Available)
    }

    /// Short reason when not available.
    pub fn reason(&self) -> Option<&str> {
        match self {
            Self::Available => None,
            Self::Unavailable { reason } | Self::Unsupported { reason } => Some(reason),
        }
    }
}

/// Outcome of one probe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProbeOutcome {
    /// Name of the probed capability.
    pub name: String,
    /// Resulting state.
    pub state: CapabilityState,
    /// When the probe ran.
    pub observed_at: chrono::DateTime<chrono::Utc>,
}

/// The capability report of one session.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Capabilities {
    /// Probed capabilities by name.
    #[serde(default)]
    pub probes: BTreeMap<String, CapabilityState>,
}

impl Capabilities {
    /// Build an empty report.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a probe result.
    pub fn record(&mut self, name: impl Into<String>, state: CapabilityState) {
        self.probes.insert(name.into(), state);
    }

    /// Look up one capability.
    pub fn get(&self, name: &str) -> Option<&CapabilityState> {
        self.probes.get(name)
    }

    /// Whether a capability is usable.
    pub fn is_available(&self, name: &str) -> bool {
        self.get(name).is_some_and(CapabilityState::is_available)
    }

    /// Names of all unavailable capabilities with their reason.
    pub fn unavailable(&self) -> Vec<(String, String)> {
        self.probes
            .iter()
            .filter_map(|(name, state)| {
                state
                    .reason()
                    .map(|reason| (name.clone(), reason.to_string()))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn availability_and_reasons() {
        let mut caps = Capabilities::new();
        caps.record("variables", CapabilityState::Available);
        caps.record(
            "export_svg",
            CapabilityState::Unavailable {
                reason: "seat does not expose SVG export".to_string(),
            },
        );
        assert!(caps.is_available("variables"));
        assert!(!caps.is_available("export_svg"));
        assert_eq!(caps.unavailable().len(), 1);
    }
}
