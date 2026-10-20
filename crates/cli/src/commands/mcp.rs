//! The MCP adapter over stdio.
//!
//! The adapter is the crate `figtether-mcp`: it serves the read only tool
//! surface to an MCP client over the standard streams. The standard output
//! carries the protocol and nothing else, so an MCP client can spawn this
//! process and talk to it directly.
//!
//! The command only wires the adapter to the runtime directory and to the
//! port of the daemon. The adapter itself opens the authenticated loopback
//! connection to the daemon and forwards every tool call.

use figtether_mcp::McpServer;

use crate::cli::McpArgs;
use crate::commands::{block_on, CommandOutcome};

/// Run the MCP adapter over stdio.
pub fn run(args: McpArgs) -> anyhow::Result<CommandOutcome> {
    let mut builder = McpServer::builder();
    if let Some(runtime_dir) = args.runtime_dir.clone() {
        builder = builder.with_runtime_dir(runtime_dir);
    }
    if let Some(port) = args.port {
        let url = format!("ws://127.0.0.1:{port}/client");
        builder = builder.with_endpoint_url(&url)?;
    }
    let server = builder.build()?;
    block_on(async move {
        server.run_stdio().await?;
        Ok(CommandOutcome::ok())
    })
}

#[cfg(test)]
mod tests {
    use figtether_mcp::TOOL_NAMES;

    #[test]
    fn the_adapter_publishes_the_read_only_tools() {
        assert_eq!(TOOL_NAMES.len(), 12);
        assert!(TOOL_NAMES.contains(&"list_sessions"));
        assert!(TOOL_NAMES.contains(&"get_selection"));
        assert!(TOOL_NAMES.contains(&"inspect_design"));
    }

    #[test]
    fn the_adapter_publishes_no_mutating_tool() {
        for name in TOOL_NAMES {
            for banned in ["create", "delete", "update", "mutate"] {
                assert!(!name.contains(banned), "the tool {name} mutates");
            }
        }
    }
}
