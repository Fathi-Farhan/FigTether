//! Daemon configuration.
//!
//! The configuration is deliberately small. The daemon always binds the
//! loopback interface, always uses the same wire protocol, and always keeps
//! its state in the owner only runtime directory. Only the port, the
//! resource limits, the runtime directory, and the log format are tunable.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

use figtether_core::ResourceLimits;
use figtether_core::DEFAULT_BRIDGE_PORT;
use uuid::Uuid;

/// Shape of the diagnostic log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    /// One human readable line per event.
    Compact,
    /// One JSON object per event.
    Json,
}

impl LogFormat {
    /// Stable wire string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Json => "json",
        }
    }
}

/// Configuration of one daemon instance.
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    /// Port of the loopback listener.
    port: u16,
    /// Resource budgets of the daemon.
    limits: ResourceLimits,
    /// Explicit runtime directory, when the caller chose one.
    runtime_dir: Option<PathBuf>,
    /// Shape of the diagnostic log.
    log_format: LogFormat,
    /// Filter of the diagnostic log.
    log_level: String,
    /// Identity of this daemon instance.
    instance_id: Uuid,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            port: DEFAULT_BRIDGE_PORT,
            limits: ResourceLimits::default(),
            runtime_dir: None,
            log_format: LogFormat::Compact,
            log_level: "info".to_string(),
            instance_id: Uuid::new_v4(),
        }
    }
}

impl DaemonConfig {
    /// A configuration with the documented defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Use another port.
    ///
    /// The port must not be zero: the daemon never binds an arbitrary port,
    /// because the plugin manifest references a fixed one.
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Use another set of resource budgets.
    pub fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Use another runtime directory.
    pub fn with_runtime_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.runtime_dir = Some(path.into());
        self
    }

    /// Use another log format.
    pub fn with_log_format(mut self, format: LogFormat) -> Self {
        self.log_format = format;
        self
    }

    /// Use another log filter.
    pub fn with_log_level(mut self, level: impl Into<String>) -> Self {
        self.log_level = level.into();
        self
    }

    /// Use a fixed instance id instead of a fresh one.
    pub fn with_instance_id(mut self, instance_id: Uuid) -> Self {
        self.instance_id = instance_id;
        self
    }

    /// The configured port.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The configured resource budgets.
    pub fn limits(&self) -> &ResourceLimits {
        &self.limits
    }

    /// The configured runtime directory, when one was chosen.
    pub fn runtime_dir(&self) -> Option<&Path> {
        self.runtime_dir.as_deref()
    }

    /// The configured log format.
    pub fn log_format(&self) -> LogFormat {
        self.log_format
    }

    /// The configured log filter.
    pub fn log_level(&self) -> &str {
        &self.log_level
    }

    /// The identity of this daemon instance.
    pub fn instance_id(&self) -> Uuid {
        self.instance_id
    }

    /// The address the daemon binds.
    ///
    /// The daemon never binds `0.0.0.0`. The loopback interface is the only
    /// reachable one, so a remote peer can never reach the bridge.
    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, self.port))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_port_is_the_documented_bridge_port() {
        let config = DaemonConfig::new();
        assert_eq!(config.port(), DEFAULT_BRIDGE_PORT);
        assert_eq!(config.port(), 38947);
    }

    #[test]
    fn bind_address_is_loopback_only() {
        let config = DaemonConfig::new().with_port(38948);
        let addr = config.bind_addr();
        assert!(addr.ip().is_loopback());
        assert!(!addr.ip().is_unspecified());
        assert_eq!(addr.port(), 38948);
    }

    #[test]
    fn instance_ids_are_unique_per_configuration() {
        let first = DaemonConfig::new().instance_id();
        let second = DaemonConfig::new().instance_id();
        assert_ne!(first, second);
    }

    #[test]
    fn log_format_has_a_stable_name() {
        assert_eq!(LogFormat::Compact.as_str(), "compact");
        assert_eq!(LogFormat::Json.as_str(), "json");
    }
}
