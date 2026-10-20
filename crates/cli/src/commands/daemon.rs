//! The foreground daemon.
//!
//! The daemon owns the sessions, the grants, and the counters, and it is the
//! only listener on the loopback port. It binds `127.0.0.1` and nothing else.
//!
//! A port collision is a loud failure: the daemon never moves to another port
//! on its own, because the plugin reads the port from the manifest. The error
//! message names `--port` and says that the plugin manifest has to be
//! regenerated for the new port.

use std::io::ErrorKind;
use std::net::{Ipv4Addr, TcpListener};

use figtether_daemon::{DaemonConfig, DaemonError};

use crate::cli::DaemonArgs;
use crate::commands::{block_on, CommandOutcome};
use crate::output;

/// Version of the daemon.
pub const DAEMON_VERSION: &str = figtether_daemon::DAEMON_VERSION;

/// The port is already taken by another process.
#[derive(Debug)]
pub struct PortInUse {
    /// The port that could not be bound.
    pub port: u16,
}

impl std::fmt::Display for PortInUse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "port {} on 127.0.0.1 is already in use by another process.\n\
The port is never changed silently, because the plugin manifest references it.\n\
Stop the other process or choose a free port explicitly, then regenerate the plugin manifest for the same port:\n  \
figtether daemon --port <free-port>\n  \
figtether manifest --plugin-id \"<plugin-id>\" --port <free-port>\n\
Then reload the plugin in Figma.",
            self.port
        )
    }
}

impl std::error::Error for PortInUse {}

/// Run the daemon in the foreground.
pub fn run(args: DaemonArgs) -> anyhow::Result<CommandOutcome> {
    ensure_port_is_free(args.port)?;

    let mut config = DaemonConfig::new()
        .with_port(args.port)
        .with_log_level(args.log_level.as_str());
    if let Some(runtime_dir) = args.runtime_dir.clone() {
        config = config.with_runtime_dir(runtime_dir);
    }

    output::blank();
    output::line(&format!("FigTether daemon {DAEMON_VERSION}"));
    output::field("bridge address", &format!("127.0.0.1:{}", args.port));
    if let Some(runtime_dir) = &args.runtime_dir {
        output::field("runtime directory", &runtime_dir.display().to_string());
    }
    output::blank();
    output::note("press Ctrl+C to stop the daemon");

    block_on(async move {
        match figtether_daemon::run_until_stopped(config).await {
            Ok(()) => {
                output::blank();
                output::line("the daemon stopped");
                Ok(CommandOutcome::ok())
            }
            Err(DaemonError::Bind(err)) if err.kind() == ErrorKind::AddrInUse => {
                Err(anyhow::Error::new(PortInUse { port: args.port }))
            }
            Err(err) => Err(anyhow::anyhow!("the daemon could not start: {err}")),
        }
    })
}

/// Fail early when the port is already taken.
///
/// The daemon binds the port itself. The check here only makes the failure
/// message the one the user needs, before the daemon writes any runtime file.
fn ensure_port_is_free(port: u16) -> anyhow::Result<()> {
    match TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
        Ok(listener) => {
            drop(listener);
            Ok(())
        }
        Err(err) if err.kind() == ErrorKind::AddrInUse => {
            Err(anyhow::Error::new(PortInUse { port }))
        }
        Err(err) => Err(anyhow::anyhow!(
            "the bridge port {port} on 127.0.0.1 could not be bound: {err}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_port_error_names_the_flag_and_the_manifest() {
        let text = PortInUse { port: 38947 }.to_string();
        assert!(text.contains("--port"), "{text}");
        assert!(text.contains("manifest"), "{text}");
    }

    #[test]
    fn a_taken_port_is_refused() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("binds");
        let port = listener.local_addr().expect("address").port();
        let error = ensure_port_is_free(port).expect_err("refused");
        assert!(error.to_string().contains("--port"), "{error}");
    }

    #[test]
    fn a_free_port_is_accepted() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("binds");
        let port = listener.local_addr().expect("address").port();
        drop(listener);
        ensure_port_is_free(port).expect("accepted");
    }
}
