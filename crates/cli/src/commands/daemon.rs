//! The foreground daemon.
//!
//! The daemon owns the sessions, the grants, and the counters, and it is the
//! only listener on the loopback port. It binds `127.0.0.1` and nothing else.
//!
//! A port collision is a loud failure: the daemon never moves to another port
//! on its own, because the plugin reads the port from the manifest. The error
//! message names the port, the reason, the port to use instead, and the
//! commands that start the daemon and the plugin on that port.

use std::io::ErrorKind;

use figtether_daemon::{DaemonConfig, DaemonError};

use crate::cli::DaemonArgs;
use crate::commands::{block_on, CommandOutcome};
use crate::output;

/// Version of the daemon.
pub const DAEMON_VERSION: &str = figtether_daemon::DAEMON_VERSION;

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
    output::note(
        "leave this terminal open; run figtether pair in another terminal to pair the Figma plugin",
    );

    block_on(async move {
        match figtether_daemon::run_until_stopped(config).await {
            Ok(()) => {
                output::blank();
                output::line("the daemon stopped");
                Ok(CommandOutcome::ok())
            }
            Err(DaemonError::PortUnavailable {
                port,
                problem,
                suggestion,
            }) => Err(anyhow::Error::new(DaemonError::PortUnavailable {
                port,
                problem,
                suggestion,
            })),
            Err(DaemonError::Bind(err)) if err.kind() == ErrorKind::AddrInUse => {
                Err(anyhow::Error::new(port_unavailable(
                    args.port,
                    figtether_daemon::port::PortProblem::InUse,
                )))
            }
            Err(err) => Err(anyhow::anyhow!("the daemon could not start: {err}")),
        }
    })
}

/// Fail early when the port cannot be bound.
///
/// The daemon binds the port itself. The check here only makes the failure
/// message the one the user needs, before the daemon writes any runtime file.
fn ensure_port_is_free(port: u16) -> anyhow::Result<()> {
    match figtether_daemon::port::diagnose(port) {
        None => Ok(()),
        Some(problem) => Err(anyhow::Error::new(port_unavailable(port, problem))),
    }
}

/// The error of a port that cannot be bound.
///
/// The error names the port, the reason, and a port that can be bound
/// instead. The port is never changed on its own, so the message always
/// carries the commands that move the daemon and the plugin together.
fn port_unavailable(port: u16, problem: figtether_daemon::port::PortProblem) -> DaemonError {
    DaemonError::PortUnavailable {
        port,
        problem,
        suggestion: figtether_daemon::port::suggest_free_port(port),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, TcpListener};

    #[test]
    fn the_port_error_names_the_flag_and_the_manifest() {
        let text = port_unavailable(38947, figtether_daemon::port::PortProblem::InUse).to_string();
        assert!(text.contains("--port"), "{text}");
        assert!(text.contains("manifest"), "{text}");
    }

    #[test]
    fn the_port_error_names_a_port_to_use_instead() {
        let error = port_unavailable(
            38947,
            figtether_daemon::port::PortProblem::ReservedByOs {
                start: 38929,
                end: 39028,
            },
        );
        let text = error.to_string();
        assert!(text.contains("38929-39028"), "{text}");
        match error {
            DaemonError::PortUnavailable { suggestion, .. } => {
                assert!(
                    text.contains(&format!("figtether daemon --port {suggestion}")),
                    "{text}"
                );
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn the_port_error_never_asks_for_a_policy_change() {
        let text = port_unavailable(38947, figtether_daemon::port::PortProblem::InUse)
            .to_string()
            .to_lowercase();
        assert!(!text.contains("firewall"), "{text}");
        assert!(!text.contains("delete"), "{text}");
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
