//! The FigTether subcommands.
//!
//! Every command is local. The CLI never asks for a Figma personal access
//! token, never sends design data anywhere, and prints a secret only for the
//! explicit local pair action.

pub mod config;
pub mod daemon;
pub mod doctor;
pub mod manifest;
pub mod mcp;
pub mod pair;
pub mod revoke;
pub mod status;

use std::future::Future;

use crate::cli::Command;

/// Result of a subcommand.
#[derive(Debug, Default)]
pub struct CommandOutcome {
    /// Whether the command succeeded.
    pub success: bool,
    /// Message shown to the user when the command failed.
    pub message: Option<String>,
}

impl CommandOutcome {
    /// A successful outcome.
    pub fn ok() -> Self {
        Self {
            success: true,
            message: None,
        }
    }

    /// A failed outcome with a message.
    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: Some(message.into()),
        }
    }
}

/// Run the selected subcommand.
pub fn dispatch(command: Command) -> anyhow::Result<CommandOutcome> {
    match command {
        Command::Doctor(args) => doctor::run(args),
        Command::Daemon(args) => daemon::run(args),
        Command::Mcp(args) => mcp::run(args),
        Command::Status(args) => status::run(args),
        Command::Pair(args) => pair::run(args),
        Command::Config(args) => config::run(args),
        Command::Manifest(args) => manifest::run(args),
        Command::Revoke(args) => revoke::run(args),
    }
}

/// Run an asynchronous subcommand on a dedicated runtime.
///
/// The commands that talk to the daemon need an asynchronous runtime, and the
/// daemon and the MCP adapter need one of their own. Every command therefore
/// builds the runtime it needs and tears it down again.
pub fn block_on<F, T>(future: F) -> anyhow::Result<T>
where
    F: Future<Output = anyhow::Result<T>>,
{
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(future)
}
