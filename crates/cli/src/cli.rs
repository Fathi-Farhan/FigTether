//! Command line surface of FigTether.
//!
//! The binary is `figtether`. Every subcommand is local only: nothing here
//! contacts Figma, no personal access token is ever requested, and the only
//! secret the CLI prints is the one time pairing secret minted by
//! `figtether pair`.
//!
//! The surface follows PRD section 8.4: `doctor`, `daemon`, `mcp`, `status`,
//! `pair`, `config`, `manifest`, and `revoke`.

use std::path::PathBuf;

use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};
use figtether_model::ids::SessionId;

/// Default TCP port of the local bridge.
pub const DEFAULT_PORT: u16 = figtether_core::DEFAULT_BRIDGE_PORT;

/// Top level command line.
#[derive(Debug, Parser)]
#[command(
    name = "figtether",
    version,
    about = "FigTether: local Figma context for AI agents",
    long_about = "FigTether reads Figma design context through a local plugin bridge and \
                  serves it to AI agents over MCP. Everything runs on this machine: no design \
                  data is sent anywhere, and no Figma access token is ever requested.",
    propagate_version = true,
    disable_help_subcommand = true
)]
pub struct Cli {
    /// Selected subcommand.
    #[command(subcommand)]
    pub command: Command,
}

/// The FigTether subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Check the local installation and print a sanitized health report.
    Doctor(DoctorArgs),
    /// Run the bridge daemon in the foreground.
    Daemon(DaemonArgs),
    /// Run the MCP adapter over stdio.
    Mcp(McpArgs),
    /// Print a sanitized status summary from the running daemon.
    Status(StatusArgs),
    /// Mint a one time pairing secret for the plugin.
    Pair(PairArgs),
    /// Print client configuration snippets.
    Config(ConfigArgs),
    /// Generate a development plugin manifest.
    Manifest(ManifestArgs),
    /// Revoke the grant of a session.
    Revoke(RevokeArgs),
}

/// Arguments of `figtether doctor`.
#[derive(Debug, Args)]
pub struct DoctorArgs {
    /// Print the report as JSON so tooling can consume it.
    #[arg(long)]
    pub json: bool,
    /// Port of the daemon. Defaults to the port in the runtime file, otherwise 38947.
    #[arg(long, value_name = "PORT")]
    pub port: Option<u16>,
    /// Runtime directory to inspect. Defaults to the platform runtime directory.
    #[arg(long, value_name = "PATH")]
    pub runtime_dir: Option<PathBuf>,
    /// Plugin manifest to inspect. Defaults to manifest.json in the current directory.
    #[arg(long, value_name = "PATH")]
    pub manifest: Option<PathBuf>,
}

/// Arguments of `figtether daemon`.
#[derive(Debug, Args)]
pub struct DaemonArgs {
    /// Loopback port to bind. The port is never changed silently.
    #[arg(long, default_value_t = DEFAULT_PORT, value_name = "PORT")]
    pub port: u16,
    /// Runtime directory that holds the owner only runtime files.
    #[arg(long, value_name = "PATH")]
    pub runtime_dir: Option<PathBuf>,
    /// Verbosity of the local log file.
    #[arg(long, value_enum, default_value_t = LogLevel::Info, value_name = "LEVEL")]
    pub log_level: LogLevel,
}

/// Arguments of `figtether mcp`.
#[derive(Debug, Args)]
pub struct McpArgs {
    /// Serve the MCP protocol over stdio. This is the default and the only transport in v0.1.
    #[arg(long, action = ArgAction::SetTrue, default_value_t = true)]
    pub stdio: bool,
    /// Port of the daemon. Defaults to the port in the runtime file, otherwise 38947.
    #[arg(long, value_name = "PORT")]
    pub port: Option<u16>,
    /// Runtime directory that holds the owner only runtime files.
    #[arg(long, value_name = "PATH")]
    pub runtime_dir: Option<PathBuf>,
}

/// Arguments of `figtether status`.
#[derive(Debug, Args)]
pub struct StatusArgs {
    /// Print the status as JSON so tooling can consume it.
    #[arg(long)]
    pub json: bool,
    /// Port of the daemon. Defaults to the port in the runtime file, otherwise 38947.
    #[arg(long, value_name = "PORT")]
    pub port: Option<u16>,
    /// Runtime directory that holds the owner only runtime files.
    #[arg(long, value_name = "PATH")]
    pub runtime_dir: Option<PathBuf>,
}

/// Arguments of `figtether pair`.
#[derive(Debug, Args)]
pub struct PairArgs {
    /// Port of the daemon. Defaults to the port in the runtime file, otherwise 38947.
    #[arg(long, value_name = "PORT")]
    pub port: Option<u16>,
    /// Runtime directory that holds the owner only runtime files.
    #[arg(long, value_name = "PATH")]
    pub runtime_dir: Option<PathBuf>,
}

/// Arguments of `figtether config`.
#[derive(Debug, Args)]
pub struct ConfigArgs {
    /// Selected configuration action.
    #[command(subcommand)]
    pub command: ConfigCommand,
}

/// Actions of `figtether config`.
#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Print an MCP client entry. The entry is printed only, never written.
    Print(ConfigPrintArgs),
}

/// Arguments of `figtether config print`.
#[derive(Debug, Args)]
pub struct ConfigPrintArgs {
    /// MCP client the snippet is written for.
    #[arg(long, value_enum, default_value_t = ClientKind::Generic, value_name = "CLIENT")]
    pub client: ClientKind,
}

/// Arguments of `figtether manifest`.
#[derive(Debug, Args)]
pub struct ManifestArgs {
    /// Action to perform. The only action is generate, which is also the default.
    #[arg(value_name = "ACTION", default_value = "generate")]
    pub action: String,
    /// Path of the generated manifest. Defaults to manifest.json in the current directory.
    #[arg(long, value_name = "PATH")]
    pub out: Option<PathBuf>,
    /// Plugin id of the development manifest. It belongs to your own Figma account.
    #[arg(long, value_name = "ID")]
    pub plugin_id: String,
    /// Loopback port the plugin is allowed to reach.
    #[arg(long, default_value_t = DEFAULT_PORT, value_name = "PORT")]
    pub port: u16,
    /// Editor the plugin runs in.
    #[arg(long, value_enum, default_value_t = EditorType::Figma, value_name = "EDITOR")]
    pub editor_type: EditorType,
}

/// Arguments of `figtether revoke`.
#[derive(Debug, Args)]
#[command(group = clap::ArgGroup::new("target").required(true).multiple(false).args(["session", "all"]))]
pub struct RevokeArgs {
    /// Session whose grant is revoked.
    #[arg(long, value_name = "ID", value_parser = parse_session_id)]
    pub session: Option<SessionId>,
    /// Revoke the grant of every session the daemon knows.
    #[arg(long)]
    pub all: bool,
    /// Port of the daemon. Defaults to the port in the runtime file, otherwise 38947.
    #[arg(long, value_name = "PORT")]
    pub port: Option<u16>,
    /// Runtime directory that holds the owner only runtime files.
    #[arg(long, value_name = "PATH")]
    pub runtime_dir: Option<PathBuf>,
}

/// Verbosity of the local log file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LogLevel {
    /// Only errors.
    Error,
    /// Errors and warnings.
    Warn,
    /// Errors, warnings, and progress.
    Info,
    /// Everything, including per request detail.
    Debug,
    /// Everything, including protocol detail.
    Trace,
}

impl LogLevel {
    /// Stable name of the level.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

/// MCP client the configuration snippet is written for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ClientKind {
    /// The generic MCP server entry every client understands.
    Generic,
}

impl ClientKind {
    /// Stable name of the client kind.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Generic => "generic",
        }
    }
}

/// Editor a plugin manifest targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum EditorType {
    /// Figma Design.
    Figma,
}

impl EditorType {
    /// Stable name of the editor type.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Figma => "figma",
        }
    }
}

/// Parse a session id from the command line.
fn parse_session_id(raw: &str) -> Result<SessionId, String> {
    raw.parse::<SessionId>()
        .map_err(|_| format!("{raw} is not a session id"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn command_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn the_surface_has_the_documented_subcommands() {
        let command = Cli::command();
        let names: Vec<String> = command
            .get_subcommands()
            .map(|sub| sub.get_name().to_string())
            .collect();
        assert_eq!(
            names,
            vec!["doctor", "daemon", "mcp", "status", "pair", "config", "manifest", "revoke",]
        );
    }

    #[test]
    fn the_defaults_match_the_documented_surface() {
        let cli = Cli::try_parse_from(["figtether", "doctor"]).expect("parses");
        match cli.command {
            Command::Doctor(args) => {
                assert_eq!(args.port, None);
                assert!(!args.json);
                assert!(args.runtime_dir.is_none());
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn the_daemon_accepts_the_documented_flags() {
        let cli = Cli::try_parse_from([
            "figtether",
            "daemon",
            "--port",
            "38948",
            "--runtime-dir",
            "runtime",
            "--log-level",
            "debug",
        ])
        .expect("parses");
        match cli.command {
            Command::Daemon(args) => {
                assert_eq!(args.port, 38948);
                assert_eq!(args.runtime_dir, Some(PathBuf::from("runtime")));
                assert_eq!(args.log_level, LogLevel::Debug);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn the_daemon_port_defaults_to_the_bridge_port() {
        let cli = Cli::try_parse_from(["figtether", "daemon"]).expect("parses");
        match cli.command {
            Command::Daemon(args) => assert_eq!(args.port, DEFAULT_PORT),
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn the_mcp_transport_defaults_to_stdio() {
        let cli = Cli::try_parse_from(["figtether", "mcp"]).expect("parses");
        match cli.command {
            Command::Mcp(args) => {
                assert!(args.stdio);
                assert_eq!(args.port, None);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn the_mcp_accepts_the_explicit_stdio_flag() {
        let cli = Cli::try_parse_from(["figtether", "mcp", "--stdio"]).expect("parses");
        match cli.command {
            Command::Mcp(args) => assert!(args.stdio),
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn the_config_print_selects_the_generic_client() {
        let cli = Cli::try_parse_from(["figtether", "config", "print", "--client", "generic"])
            .expect("parses");
        match cli.command {
            Command::Config(args) => match args.command {
                ConfigCommand::Print(print_args) => {
                    assert_eq!(print_args.client, ClientKind::Generic)
                }
            },
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn the_config_client_defaults_to_generic() {
        let cli = Cli::try_parse_from(["figtether", "config", "print"]).expect("parses");
        match cli.command {
            Command::Config(args) => match args.command {
                ConfigCommand::Print(print_args) => {
                    assert_eq!(print_args.client, ClientKind::Generic)
                }
            },
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn an_unknown_config_client_is_refused() {
        let err = Cli::try_parse_from(["figtether", "config", "print", "--client", "nope"])
            .expect_err("refused");
        assert_eq!(err.kind(), clap::error::ErrorKind::InvalidValue);
    }

    #[test]
    fn the_manifest_takes_the_documented_flags() {
        let cli = Cli::try_parse_from([
            "figtether",
            "manifest",
            "--plugin-id",
            "123456789",
            "--port",
            "38948",
            "--out",
            "out.json",
            "--editor-type",
            "figma",
        ])
        .expect("parses");
        match cli.command {
            Command::Manifest(args) => {
                assert_eq!(args.plugin_id, "123456789");
                assert_eq!(args.port, 38948);
                assert_eq!(args.out, Some(PathBuf::from("out.json")));
                assert_eq!(args.editor_type, EditorType::Figma);
                assert_eq!(args.action, "generate");
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn the_manifest_accepts_the_explicit_generate_action() {
        let cli = Cli::try_parse_from([
            "figtether",
            "manifest",
            "generate",
            "--plugin-id",
            "123456789",
        ])
        .expect("parses");
        match cli.command {
            Command::Manifest(args) => {
                assert_eq!(args.action, "generate");
                assert_eq!(args.plugin_id, "123456789");
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn the_manifest_requires_a_plugin_id() {
        let err = Cli::try_parse_from(["figtether", "manifest"]).expect_err("refused");
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn the_revoke_needs_a_target() {
        let err = Cli::try_parse_from(["figtether", "revoke"]).expect_err("refused");
        assert_eq!(err.kind(), clap::error::ErrorKind::MissingRequiredArgument);
    }

    #[test]
    fn the_revoke_refuses_two_targets() {
        let err = Cli::try_parse_from([
            "figtether",
            "revoke",
            "--session",
            "11111111-1111-4111-8111-111111111111",
            "--all",
        ])
        .expect_err("refused");
        assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
    }

    #[test]
    fn the_revoke_accepts_one_target() {
        let cli = Cli::try_parse_from([
            "figtether",
            "revoke",
            "--session",
            "11111111-1111-4111-8111-111111111111",
        ])
        .expect("parses");
        match cli.command {
            Command::Revoke(args) => {
                assert!(args.session.is_some());
                assert!(!args.all);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn the_revoke_accepts_all_sessions() {
        let cli = Cli::try_parse_from(["figtether", "revoke", "--all"]).expect("parses");
        match cli.command {
            Command::Revoke(args) => {
                assert!(args.session.is_none());
                assert!(args.all);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn an_invalid_session_id_is_refused() {
        let err = Cli::try_parse_from(["figtether", "revoke", "--session", "not-a-uuid"])
            .expect_err("refused");
        assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
    }

    #[test]
    fn the_log_levels_have_stable_names() {
        assert_eq!(LogLevel::Info.as_str(), "info");
        assert_eq!(LogLevel::Trace.as_str(), "trace");
    }
}
