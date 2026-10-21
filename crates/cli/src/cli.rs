//! Command line surface of FigTether.
//!
//! The binary is `figtether`. Bridge commands contact the local daemon; the
//! source updater also invokes Git, Cargo, and npm. No Figma personal access
//! token is ever requested, and the only
//! secret the CLI prints is the one time pairing secret minted by
//! `figtether pair`.
//!
//! The surface follows PRD section 8.4: `doctor`, `daemon`, `mcp`, `status`,
//! `pair`, `config`, `manifest`, and `revoke`, plus source installation updates.

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
    long_about = "FigTether reads, creates, and edits native Figma designs through a local \
                  plugin bridge and serves AI agents over MCP. Design access stays within \
                  user-granted frames; writes require approval. The update command uses \
                  Git and package registries. No Figma access token is requested.",
    propagate_version = true,
    disable_help_subcommand = true,
    after_help = "Run figtether without a subcommand to start the foreground daemon.\n\
                  In another terminal, run figtether pair to generate a pairing secret."
)]
pub struct Cli {
    /// Selected subcommand.
    #[command(subcommand)]
    pub command: Option<Command>,
}

impl Cli {
    /// Resolve a bare invocation to the same daemon defaults as `figtether daemon`.
    pub fn into_command(self) -> Command {
        self.command
            .unwrap_or_else(|| Command::Daemon(DaemonArgs::default()))
    }
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
    /// Stop the local daemon.
    #[command(visible_alias = "terminate")]
    Stop(StopArgs),
    /// Update a source installation and rebuild the Figma plugin.
    Update(UpdateArgs),
    /// Preview or install a project-local MCP connection and design skill.
    Setup(SetupArgs),
}

/// Project-local integration. No global settings or host approvals are changed.
#[derive(Debug, Args)]
pub struct SetupArgs {
    /// Agent host to configure.
    #[arg(long, value_enum)]
    pub client: ClientKind,
    /// Design workspace; it need not contain the FigTether source.
    #[arg(long, default_value = ".")]
    pub workspace: PathBuf,
    /// Write the reviewed configuration and skill. Without it, only preview.
    #[arg(long, conflicts_with_all = ["remove", "check"])]
    pub apply: bool,
    /// Remove only unchanged entries installed by this command.
    #[arg(long, conflicts_with = "check")]
    pub remove: bool,
    /// Test the installed entry and actual MCP adapter without changing files.
    #[arg(long)]
    pub check: bool,
    /// Emit machine-readable output.
    #[arg(long)]
    pub json: bool,
    /// Isolated/custom daemon runtime directory.
    #[arg(long)]
    pub runtime_dir: Option<PathBuf>,
    /// Explicit daemon port.
    #[arg(long)]
    pub port: Option<u16>,
}

/// Arguments of `figtether update`.
#[derive(Debug, Args)]
pub struct UpdateArgs {
    /// Source checkout. Defaults to the checkout this executable was built from.
    #[arg(long, value_name = "PATH")]
    pub source: Option<PathBuf>,
    /// Rebuild the current local source without Git/network source updates.
    #[arg(long)]
    pub local: bool,
    /// Show the update steps without fetching, building, or replacing files.
    #[arg(long)]
    pub dry_run: bool,
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
    /// Loopback port to bind. Without it the port of the local plugin manifest
    /// is used, because the plugin reads its port from that manifest. The
    /// port is never changed silently.
    #[arg(long, value_name = "PORT")]
    pub port: Option<u16>,
    /// Runtime directory that holds the owner only runtime files.
    #[arg(long, value_name = "PATH")]
    pub runtime_dir: Option<PathBuf>,
    /// Verbosity of the local log file.
    #[arg(long, value_enum, default_value_t = LogLevel::Info, value_name = "LEVEL")]
    pub log_level: LogLevel,
}

impl Default for DaemonArgs {
    fn default() -> Self {
        Self {
            port: None,
            runtime_dir: None,
            log_level: LogLevel::Info,
        }
    }
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
    /// Print only the one-use secret and its lifetime as JSON on stdout.
    #[arg(long)]
    pub json: bool,
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
    /// Path of the generated manifest. Defaults to the machine local manifest
    /// inside a checkout, otherwise to manifest.json in the current directory.
    #[arg(long, value_name = "PATH")]
    pub out: Option<PathBuf>,
    /// Plugin id of the development manifest. It belongs to your own Figma
    /// account. Without it the id of the existing manifest is kept.
    #[arg(long, value_name = "ID")]
    pub plugin_id: Option<String>,
    /// Loopback port the plugin is allowed to reach. Use `auto` to keep the
    /// port that the machine and the plugin already agree on, or to pick a
    /// free one.
    #[arg(
        long,
        default_value_t = PortSelection::Auto,
        value_name = "PORT",
        value_parser = parse_port_selection
    )]
    pub port: PortSelection,
    /// Editor the plugin runs in.
    #[arg(long, value_enum, default_value_t = EditorType::Figma, value_name = "EDITOR")]
    pub editor_type: EditorType,
    /// Runtime directory that holds the owner only runtime files. It is read
    /// to keep the port of the running daemon.
    #[arg(long, value_name = "PATH")]
    pub runtime_dir: Option<PathBuf>,
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

/// Arguments of `figtether stop`.
#[derive(Debug, Args)]
pub struct StopArgs {
    /// Port of the daemon. Defaults to the port in the runtime file, otherwise 38947.
    #[arg(long, value_name = "PORT")]
    pub port: Option<u16>,
    /// Runtime directory that holds the owner only runtime files.
    #[arg(long, value_name = "PATH")]
    pub runtime_dir: Option<PathBuf>,
    /// Seconds to wait for the daemon to release its port.
    #[arg(long, value_name = "SECONDS")]
    pub timeout_seconds: Option<u64>,
    /// Print the result as JSON so tooling can consume it.
    #[arg(long)]
    pub json: bool,
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
    /// Codex CLI / local IDE extension (TOML).
    Codex,
    /// Gemini CLI (settings JSON).
    Gemini,
    /// Claude Code (project MCP JSON).
    Claude,
}

impl ClientKind {
    /// Stable name of the client kind.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Generic => "generic",
            Self::Codex => "codex",
            Self::Gemini => "gemini",
            Self::Claude => "claude",
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

/// The port a command works with.
///
/// A command takes a fixed port, or `auto` to let the CLI pick a free one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortSelection {
    /// The port the user asked for.
    Fixed(u16),
    /// Let the CLI pick a free port.
    Auto,
}

impl std::fmt::Display for PortSelection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fixed(port) => write!(f, "{port}"),
            Self::Auto => f.write_str("auto"),
        }
    }
}

/// Parse a port argument that also accepts `auto`.
fn parse_port_selection(raw: &str) -> Result<PortSelection, String> {
    let trimmed = raw.trim();
    if trimmed.eq_ignore_ascii_case("auto") {
        return Ok(PortSelection::Auto);
    }
    let port: u16 = trimmed
        .parse()
        .map_err(|_| format!("{raw} is neither a port number nor auto"))?;
    Ok(PortSelection::Fixed(port))
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
    fn bare_invocation_matches_explicit_daemon_defaults() {
        let implicit = Cli::try_parse_from(["figtether"])
            .expect("bare command parses")
            .into_command();
        let explicit = Cli::try_parse_from(["figtether", "daemon"])
            .expect("daemon parses")
            .into_command();
        let (Command::Daemon(implicit), Command::Daemon(explicit)) = (implicit, explicit) else {
            panic!("both invocations must launch the daemon");
        };
        assert_eq!(implicit.port, explicit.port);
        assert_eq!(implicit.runtime_dir, explicit.runtime_dir);
        assert_eq!(implicit.log_level, explicit.log_level);
    }

    #[test]
    fn pair_remains_a_separate_explicit_command() {
        let cli = Cli::try_parse_from(["figtether", "pair"]).expect("pair parses");
        assert!(matches!(cli.into_command(), Command::Pair(_)));
    }

    #[test]
    fn help_version_and_typos_do_not_start_the_daemon() {
        for (argument, kind) in [
            ("--help", clap::error::ErrorKind::DisplayHelp),
            ("--version", clap::error::ErrorKind::DisplayVersion),
            ("daemno", clap::error::ErrorKind::InvalidSubcommand),
        ] {
            assert_eq!(
                Cli::try_parse_from(["figtether", argument])
                    .unwrap_err()
                    .kind(),
                kind
            );
        }
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
            vec![
                "doctor", "daemon", "mcp", "status", "pair", "config", "manifest", "revoke",
                "stop", "update", "setup",
            ]
        );
    }

    #[test]
    fn the_defaults_match_the_documented_surface() {
        let cli = Cli::try_parse_from(["figtether", "doctor"]).expect("parses");
        match cli.into_command() {
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
        match cli.into_command() {
            Command::Daemon(args) => {
                assert_eq!(args.port, Some(38948));
                assert_eq!(args.runtime_dir, Some(PathBuf::from("runtime")));
                assert_eq!(args.log_level, LogLevel::Debug);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn the_daemon_port_is_not_set_without_the_flag() {
        let cli = Cli::try_parse_from(["figtether", "daemon"]).expect("parses");
        match cli.into_command() {
            Command::Daemon(args) => assert_eq!(args.port, None),
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn the_mcp_transport_defaults_to_stdio() {
        let cli = Cli::try_parse_from(["figtether", "mcp"]).expect("parses");
        match cli.into_command() {
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
        match cli.into_command() {
            Command::Mcp(args) => assert!(args.stdio),
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn the_config_print_selects_the_generic_client() {
        let cli = Cli::try_parse_from(["figtether", "config", "print", "--client", "generic"])
            .expect("parses");
        match cli.into_command() {
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
        match cli.into_command() {
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
        match cli.into_command() {
            Command::Manifest(args) => {
                assert_eq!(args.plugin_id.as_deref(), Some("123456789"));
                assert_eq!(args.port, PortSelection::Fixed(38948));
                assert_eq!(args.out, Some(PathBuf::from("out.json")));
                assert_eq!(args.editor_type, EditorType::Figma);
                assert_eq!(args.action, "generate");
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn the_manifest_port_defaults_to_the_agreed_port() {
        let cli = Cli::try_parse_from(["figtether", "manifest", "--plugin-id", "123456789"])
            .expect("parses");
        match cli.into_command() {
            Command::Manifest(args) => assert_eq!(args.port, PortSelection::Auto),
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn the_manifest_accepts_the_auto_port() {
        let cli = Cli::try_parse_from([
            "figtether",
            "manifest",
            "--plugin-id",
            "123456789",
            "--port",
            "auto",
        ])
        .expect("parses");
        match cli.into_command() {
            Command::Manifest(args) => {
                assert_eq!(args.port, PortSelection::Auto);
                assert!(matches!(args.port, PortSelection::Auto));
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn an_unknown_manifest_port_is_refused() {
        let err = Cli::try_parse_from([
            "figtether",
            "manifest",
            "--plugin-id",
            "123456789",
            "--port",
            "nope",
        ])
        .expect_err("refused");
        assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation);
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
        match cli.into_command() {
            Command::Manifest(args) => {
                assert_eq!(args.action, "generate");
                assert_eq!(args.plugin_id.as_deref(), Some("123456789"));
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn the_manifest_accepts_a_missing_plugin_id() {
        let cli = Cli::try_parse_from(["figtether", "manifest"]).expect("parsed");
        match cli.into_command() {
            Command::Manifest(args) => assert_eq!(args.plugin_id, None),
            other => panic!("unexpected command: {other:?}"),
        }
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
        match cli.into_command() {
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
        match cli.into_command() {
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
