//! Stop the local daemon.
//!
//! The daemon runs the bridge, so stopping it is a normal operation, not an
//! emergency: the plugin loses its connection and the agent loses its tools.
//! The command asks the daemon to stop through its own control endpoint and
//! then waits for the listener to disappear.
//!
//! The graceful path matters. A daemon that is killed with a signal does not
//! cancel its queued work, does not revoke its credentials, does not close its
//! sockets, and does not wipe the design memory it holds. A daemon that stops
//! through this command does all four, and removes its runtime file, so the next
//! start does not read a stale credential.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use crate::cli::StopArgs;
use crate::commands::{block_on, CommandOutcome};
use crate::control;
use crate::output;

/// Default time to wait for the daemon to release its port.
const DEFAULT_TIMEOUT_SECONDS: u64 = 10;

/// How often the port is checked while the daemon shuts down.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// What the stop attempt achieved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopOutcome {
    /// The daemon was running and stopped.
    Stopped,
    /// No daemon was listening, so there was nothing to stop.
    NotRunning,
    /// The daemon was asked to stop but kept its port.
    StillRunning,
}

/// Run the stop command.
pub fn run(args: StopArgs) -> anyhow::Result<CommandOutcome> {
    block_on(async move {
        let port = control::expected_port(args.port, args.runtime_dir.as_deref()).await;
        let timeout = Duration::from_secs(args.timeout_seconds.unwrap_or(DEFAULT_TIMEOUT_SECONDS));
        let outcome = stop_daemon(port, timeout).await;
        let payload = serde_json::json!({
            "port": port,
            "outcome": outcome.as_str(),
        });
        if args.json {
            output::json(&payload)?;
        } else {
            print_summary(port, &outcome);
        }
        match outcome {
            StopOutcome::Stopped | StopOutcome::NotRunning => Ok(CommandOutcome::ok()),
            StopOutcome::StillRunning => Ok(CommandOutcome::failed(format!(
                "the daemon still holds port {port}; it may be busy, and stopping it with a signal skips the \
                 graceful cleanup"
            ))),
        }
    })
}

/// Ask the daemon to stop and wait for the port to be released.
pub async fn stop_daemon(port: u16, timeout: Duration) -> StopOutcome {
    if !port_is_open(port).await {
        return StopOutcome::NotRunning;
    }
    if send_shutdown(port).is_err() {
        return StopOutcome::StillRunning;
    }
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !port_is_open(port).await {
            return StopOutcome::Stopped;
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
    StopOutcome::StillRunning
}

/// Whether something accepts a connection on the loopback port.
async fn port_is_open(port: u16) -> bool {
    let address = SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, port));
    tokio::net::TcpStream::connect(address).await.is_ok()
}

/// Send the native control request that asks the daemon to stop.
fn send_shutdown(port: u16) -> std::io::Result<String> {
    let address = SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, port));
    let mut stream = TcpStream::connect(address)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.write_all(figtether_daemon::control::shutdown_request(port).as_bytes())?;
    let mut answer = String::new();
    let _ = stream.read_to_string(&mut answer);
    Ok(answer)
}

/// Print what the stop attempt achieved.
fn print_summary(port: u16, outcome: &StopOutcome) {
    output::blank();
    match outcome {
        StopOutcome::Stopped => {
            output::line("The local daemon stopped.");
            output::note(
                "Queued work was cancelled, credentials were revoked, sockets were closed, and the",
            );
            output::note(
                "design memory was wiped. The plugin and the agent lose their connection.",
            );
        }
        StopOutcome::NotRunning => {
            output::line(&format!("No daemon is listening on port {port}."));
            output::note("Nothing to stop. A daemon that is already gone needs no cleanup.");
        }
        StopOutcome::StillRunning => {
            output::line(&format!("The daemon still holds port {port}."));
            output::note(
                "It may be busy with an editor operation. Wait and run the command again.",
            );
        }
    }
    output::blank();
}

impl StopOutcome {
    /// Stable machine readable name of the outcome.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::NotRunning => "not_running",
            Self::StillRunning => "still_running",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use figtether_daemon::DaemonConfig;

    /// A port that nothing listens on.
    fn free_port() -> u16 {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("a free port");
        let port = listener.local_addr().expect("an address").port();
        drop(listener);
        port
    }

    #[test]
    fn the_outcome_names_are_stable() {
        assert_eq!(StopOutcome::Stopped.as_str(), "stopped");
        assert_eq!(StopOutcome::NotRunning.as_str(), "not_running");
        assert_eq!(StopOutcome::StillRunning.as_str(), "still_running");
    }

    #[test]
    fn stopping_a_daemon_that_is_not_running_says_so() {
        let port = free_port();
        let outcome: StopOutcome = block_on(async {
            Ok::<StopOutcome, anyhow::Error>(stop_daemon(port, Duration::from_millis(200)).await)
        })
        .expect("no error");
        assert_eq!(outcome, StopOutcome::NotRunning);
    }

    #[test]
    fn stopping_a_running_daemon_releases_its_port() {
        let port = free_port();
        let runtime = tempfile::tempdir().expect("a temp dir");
        let config = DaemonConfig::new()
            .with_port(port)
            .with_runtime_dir(runtime.path().join("figtether"));
        // The daemon runs on a runtime of its own that outlives the call, so it
        // is still listening when the stop request arrives.
        let executor = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("a runtime");
        let handle = executor
            .block_on(async {
                figtether_daemon::run(config)
                    .await
                    .map_err(anyhow::Error::from)
            })
            .expect("the daemon starts");
        assert_eq!(handle.port(), port);

        let outcome: StopOutcome = executor
            .block_on(async {
                Ok::<StopOutcome, anyhow::Error>(stop_daemon(port, Duration::from_secs(10)).await)
            })
            .expect("no error");
        assert_eq!(outcome, StopOutcome::Stopped);
        executor
            .block_on(async {
                handle.wait_stopped().await;
                Ok::<(), anyhow::Error>(())
            })
            .expect("no error");

        // The port is free again, and a second stop reports that nothing runs.
        let second: StopOutcome = executor
            .block_on(async {
                Ok::<StopOutcome, anyhow::Error>(
                    stop_daemon(port, Duration::from_millis(200)).await,
                )
            })
            .expect("no error");
        assert_eq!(second, StopOutcome::NotRunning);
    }

    #[test]
    fn the_shutdown_request_carries_the_native_control_header() {
        let request = figtether_daemon::control::shutdown_request(38947);
        assert!(request.starts_with("POST /control/shutdown HTTP/1.1"));
        assert!(request.contains("x-figtether-control: 1"));
    }
}
