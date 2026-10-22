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
    run_with_pairing(args, false)
}

/// Guided startup shows its secret only after the daemon has actually started.
pub fn run_with_pairing(args: DaemonArgs, pairing: bool) -> anyhow::Result<CommandOutcome> {
    let start = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let choice = if args.port.is_none() {
        match crate::commands::manifest::discover_manifest_in(None, args.runtime_dir.as_deref()) {
            Some(path) => manifest_choice(path)?,
            None => resolve_port(args.port, &start)?,
        }
    } else {
        resolve_port(args.port, &start)?
    };
    let port = choice.port();
    ensure_port_is_free(port)?;

    let mut config = DaemonConfig::new()
        .with_port(port)
        .with_log_level(args.log_level.as_str());
    if let Some(runtime_dir) = args.runtime_dir.clone() {
        config = config.with_runtime_dir(runtime_dir);
    }

    output::blank();
    output::line(&format!("FigTether daemon {DAEMON_VERSION}"));
    output::field("bridge address", &format!("127.0.0.1:{port}"));
    if let Some(source) = choice.source() {
        output::field("port source", &source.display().to_string());
    }
    if let Some(runtime_dir) = &args.runtime_dir {
        output::field("runtime directory", &runtime_dir.display().to_string());
    }
    output::blank();
    choice.explain();
    output::note("press Ctrl+C to stop the daemon");
    if !pairing {
        output::note(
        "leave this terminal open; run figtether pair in another terminal to pair the Figma plugin",
    );
    }

    block_on(async move {
        if pairing {
            let handle = figtether_daemon::run(config).await?;
            let result = async {
                let client =
                    crate::control::ControlClient::connect(Some(port), Some(handle.runtime_dir()))
                        .await?;
                let data = client
                    .request(figtether_protocol::client::ClientRequest::PairSecret {
                        request_id: crate::control::new_request_id(),
                    })
                    .await?;
                crate::commands::pair::print_secret(&data);
                // The daemon also listens for Ctrl+C itself. Wait for either path.
                tokio::select! {
                    result = tokio::signal::ctrl_c() => { result?; }
                    _ = handle.wait_stopped() => {}
                }
                anyhow::Ok(())
            }
            .await;
            handle.shutdown();
            handle.wait_stopped().await;
            result?;
            return Ok(CommandOutcome::ok());
        }
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
                    port,
                    figtether_daemon::port::PortProblem::InUse,
                )))
            }
            Err(err) => Err(anyhow::anyhow!("the daemon could not start: {err}")),
        }
    })
}

/// The port the daemon binds.
///
/// The default port stays the default, so that `figtether` alone is the whole
/// command an end user needs. Two rules keep the plugin and the daemon in
/// agreement:
///
/// - A port that the user names wins.
/// - The port of the local plugin manifest wins over the default, because the
///   plugin reads its port from that manifest.
///
/// Only when no manifest names a port, and the default cannot be bound, is a
/// free port picked. The choice is announced with the command that moves the
/// plugin to the same port, so it is never a silent change.
#[derive(Debug, Clone, PartialEq, Eq)]
enum PortChoice {
    /// The user named the port.
    Named(u16),
    /// The local plugin manifest names the port.
    Manifest(u16, std::path::PathBuf),
    /// The default port can be bound.
    Default(u16),
    /// The default port cannot be bound, and a free port was picked.
    Picked {
        /// The port that could not be bound.
        default: u16,
        /// The port the daemon binds.
        port: u16,
        /// Why the default could not be bound.
        problem: figtether_daemon::port::PortProblem,
    },
}

impl PortChoice {
    /// The port the daemon binds.
    fn port(&self) -> u16 {
        match self {
            Self::Named(port) => *port,
            Self::Manifest(port, _) => *port,
            Self::Default(port) => *port,
            Self::Picked { port, .. } => *port,
        }
    }

    /// The file that named the port, when a file named it.
    fn source(&self) -> Option<&std::path::Path> {
        match self {
            Self::Manifest(_, source) => Some(source.as_path()),
            _ => None,
        }
    }

    /// Tell the user how the port was chosen.
    fn explain(&self) {
        match self {
            Self::Named(_) => {}
            Self::Manifest(_, source) => {
                output::note(&format!(
                    "the port comes from {}, the file the plugin reads",
                    source.display()
                ));
            }
            Self::Default(_) => {
                output::note(&format!(
                    "the default bridge port {} is used",
                    crate::cli::DEFAULT_PORT
                ));
            }
            Self::Picked {
                default,
                port,
                problem,
            } => {
                output::note(&format!(
                    "the default bridge port {default} cannot be bound: {}",
                    problem.message()
                ));
                output::note(&format!(
                    "the daemon listens on {port} instead, so that one command is enough."
                ));
                output::note("no plugin manifest was found for this command. Prepare the source plugin in another terminal:");
                output::blank();
                output::line(&format!("  figtether start --port {port}"));
                output::blank();
                output::note(
                    "use --plugin-dir <source/plugins/figma> if the source folder cannot be found.",
                );
                output::note("then import the printed manifest path in Figma with Plugins, Development, Import plugin from manifest.");
            }
        }
    }
}

/// The port the daemon binds, and the file that named it.
///
/// A port that the user names wins. Without one, the port of the local plugin
/// manifest is used, because the plugin reads its port from that manifest and
/// the two have to agree. The default port is the last resort, and it is only
/// replaced by a free port when it cannot be bound and no manifest names one.
fn resolve_port(requested: Option<u16>, start: &std::path::Path) -> anyhow::Result<PortChoice> {
    if let Some(port) = requested {
        anyhow::ensure!(port != 0, "the bridge port must be between 1 and 65535");
        return Ok(PortChoice::Named(port));
    }
    let manifest = crate::commands::manifest::manifest_candidates(start)
        .into_iter()
        .find(|path| path.is_file());
    if let Some(source) = manifest {
        return manifest_choice(source);
    }
    let default = crate::cli::DEFAULT_PORT;
    Ok(match figtether_daemon::port::diagnose(default) {
        None => PortChoice::Default(default),
        Some(problem) => PortChoice::Picked {
            default,
            port: figtether_daemon::port::suggest_free_port(default),
            problem,
        },
    })
}

fn manifest_choice(source: std::path::PathBuf) -> anyhow::Result<PortChoice> {
    let value = crate::commands::manifest::read_manifest(&source)?;
    let port = crate::commands::manifest::manifest_port(&value).ok_or_else(|| anyhow::anyhow!(
        "{} must name one ws://localhost:<port> endpoint; regenerate it with figtether manifest", source.display()
    ))?;
    Ok(PortChoice::Manifest(port, source))
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
    fn an_invalid_manifest_is_an_error_instead_of_a_default_port() {
        let temp = tempfile::tempdir().expect("temp");
        std::fs::write(temp.path().join("manifest.json"), "\u{feff}{}").expect("BOM fixture");
        let error = resolve_port(None, temp.path()).expect_err("invalid contract");
        assert!(error.to_string().contains("byte order mark"));
    }

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

    #[test]
    fn a_named_port_wins() {
        let temp = tempfile::tempdir().expect("temp dir");
        let choice = resolve_port(Some(39123), temp.path()).expect("port");
        assert_eq!(choice, PortChoice::Named(39123));
        assert_eq!(choice.port(), 39123);
    }

    #[test]
    fn the_manifest_port_is_used_when_the_manifest_names_one() {
        let temp = tempfile::tempdir().expect("temp dir");
        let folder = temp.path().join(crate::commands::manifest::PLUGIN_FOLDER);
        std::fs::create_dir_all(&folder).expect("the plugin folder");
        let manifest = crate::commands::manifest::build_manifest(
            "figtether-dev",
            39124,
            crate::cli::EditorType::Figma,
        );
        std::fs::write(
            folder.join(crate::commands::manifest::LOCAL_FILE_NAME),
            serde_json::to_string_pretty(&manifest).expect("serializes"),
        )
        .expect("writes");
        let choice = resolve_port(None, temp.path()).expect("port");
        assert_eq!(choice.port(), 39124);
        assert!(
            choice.source().is_some(),
            "the manifest is named as the source"
        );
    }

    #[test]
    fn without_a_manifest_the_default_is_used_or_a_free_port_is_picked() {
        let temp = tempfile::tempdir().expect("temp dir");
        let choice = resolve_port(None, temp.path()).expect("port");
        match choice {
            PortChoice::Default(port) => assert_eq!(port, crate::cli::DEFAULT_PORT),
            PortChoice::Picked { default, port, .. } => {
                assert_eq!(default, crate::cli::DEFAULT_PORT);
                assert_ne!(port, default, "a port that cannot be bound is not used");
                assert!(
                    figtether_daemon::port::diagnose(port).is_none(),
                    "the picked port {port} can be bound"
                );
            }
            other => panic!("unexpected choice: {other:?}"),
        }
    }
}
