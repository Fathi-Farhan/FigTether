//! Client configuration snippets.
//!
//! `figtether config print` only prints. The CLI never writes another
//! application's configuration file, because that decision belongs to the
//! person who runs it. The snippet carries no token and no credential: the
//! pairing path stays between the plugin and the local CLI, so the model never
//! sees a secret.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::cli::{ClientKind, ConfigArgs, ConfigCommand, ConfigPrintArgs};
use crate::commands::CommandOutcome;
use crate::output;

/// Name of the MCP server entry.
pub const SERVER_NAME: &str = "figtether";

/// Arguments the MCP adapter is started with.
pub const MCP_ARGUMENTS: [&str; 2] = ["mcp", "--stdio"];

/// Print the requested configuration snippet.
pub fn run(args: ConfigArgs) -> anyhow::Result<CommandOutcome> {
    match args.command {
        ConfigCommand::Print(print_args) => print_config(print_args),
    }
}

/// Print the MCP client entry.
pub fn print_config(args: ConfigPrintArgs) -> anyhow::Result<CommandOutcome> {
    let binary = current_binary();
    let snippet = snippet(args.client, &binary);
    output::blank();
    output::line(&format!(
        "MCP client entry for FigTether ({})",
        args.client.as_str()
    ));
    output::blank();
    output::line("Copy this entry into your MCP client configuration.");
    output::line("FigTether only prints it and never writes another application");
    output::line("configuration file.");
    output::blank();
    let text = serde_json::to_string_pretty(&snippet)?;
    output::line(&text);
    output::blank();
    output::line("The entry starts the local MCP adapter over stdio. It carries no");
    output::line("token: the pairing secret never reaches the agent.");
    Ok(CommandOutcome::ok())
}

/// Build the configuration snippet for a client.
pub fn snippet(client: ClientKind, binary: &Path) -> Value {
    match client {
        ClientKind::Generic => json!({
            "mcpServers": {
                SERVER_NAME: {
                    "command": binary.display().to_string(),
                    "args": MCP_ARGUMENTS,
                }
            }
        }),
    }
}

/// The path of the running binary.
pub fn current_binary() -> PathBuf {
    match std::env::current_exe() {
        Ok(path) => path,
        Err(_) => PathBuf::from("figtether"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_snippet_points_at_the_binary_with_the_mcp_subcommand() {
        let snippet = snippet(ClientKind::Generic, Path::new("C:/tools/figtether.exe"));
        let server = &snippet["mcpServers"][SERVER_NAME];
        assert_eq!(server["command"], "C:/tools/figtether.exe");
        assert_eq!(server["args"], json!(["mcp", "--stdio"]));
    }

    #[test]
    fn the_snippet_carries_no_secret() {
        let snippet = snippet(ClientKind::Generic, Path::new("figtether"));
        let text = snippet.to_string();
        assert!(!text.contains("token"), "{text}");
        assert!(!text.contains("secret"), "{text}");
        assert!(!text.contains("credential"), "{text}");
    }

    #[test]
    fn the_snippet_is_valid_json() {
        let snippet = snippet(ClientKind::Generic, Path::new("figtether"));
        let parsed: Value = serde_json::from_str(&snippet.to_string()).expect("parses");
        assert!(parsed["mcpServers"][SERVER_NAME].is_object());
    }

    #[test]
    fn the_generic_client_is_the_only_variant() {
        assert_eq!(ClientKind::Generic.as_str(), "generic");
    }
}
