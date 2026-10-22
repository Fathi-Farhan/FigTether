//! User-facing failures keep their cause and an actionable recovery step.
//! JSON diagnostics stay on stderr; MCP stdout remains protocol-only.
use crate::{cli::Command, control::ControlError, output};
use serde::Serialize;

pub struct Context {
    command: &'static str,
    json: bool,
}

impl From<&Command> for Context {
    fn from(command: &Command) -> Self {
        let (name, json) = match command {
            Command::Doctor(a) => ("doctor", a.json),
            Command::Status(a) => ("status", a.json),
            Command::Pair(a) => ("pair", a.json),
            Command::Stop(a) => ("stop", a.json),
            Command::Setup(a) => ("setup", a.json),
            Command::Start(_) => ("start", false),
            Command::Daemon(_) => ("daemon", false),
            Command::Manifest(_) => ("manifest", false),
            Command::Update(_) => ("update", false),
            Command::Mcp(_) => ("mcp", false),
            Command::Revoke(_) => ("revoke", false),
            Command::Config(_) => ("config", false),
        };
        Self {
            command: name,
            json,
        }
    }
}

#[derive(Debug, Serialize)]
struct Diagnostic {
    code: String,
    message: String,
    action: String,
    retryable: bool,
}

fn describe(error: &anyhow::Error, context: &Context) -> Diagnostic {
    let mut diagnostic = Diagnostic {
        code: "COMMAND_FAILED".into(),
        message: output::redact(&format!("{error:#}")),
        action: format!("Run figtether {} --help to check this command, then figtether doctor for the installation checks.", context.command),
        retryable: false,
    };
    if context.command == "doctor" {
        diagnostic.action =
            "Follow the failed checks in the doctor report above, then run figtether doctor again."
                .into();
    }
    if context.command == "start" {
        diagnostic.action = "Choose the plugin source folder containing build.mjs and package.json with --plugin-dir. Install its dependencies with npm ci, then run figtether start again.".into();
    }
    if let Some(control) = error.downcast_ref::<ControlError>() {
        match control {
            ControlError::NotRunning(_) => {
                diagnostic.code = "DAEMON_NOT_RUNNING".into();
                diagnostic.action = "Run figtether start and keep its terminal open, then repeat this command. Use the same --runtime-dir and --port settings for both commands if you customized them.".into();
                diagnostic.retryable = true;
            }
            ControlError::Runtime(_) => {
                diagnostic.code = "RUNTIME_UNAVAILABLE".into();
                diagnostic.action = "Run figtether doctor with the same --runtime-dir. Choose a writable runtime folder owned by your account.".into();
            }
            ControlError::Channel(_) => {
                diagnostic.code = "BRIDGE_CONNECTION_FAILED".into();
                diagnostic.action = "Check figtether status and figtether doctor with the same runtime settings. Check the canvas before repeating a write; the connection may have failed after it started.".into();
            }
            ControlError::Daemon { code, .. } => {
                diagnostic.code = code.clone();
                diagnostic.action = "Run figtether doctor. If pairing expired, run figtether pair; if access changed, review the shared frames in the Bridge.".into();
            }
        }
    } else if let Some(io) = error.downcast_ref::<std::io::Error>() {
        match io.kind() {
            std::io::ErrorKind::NotFound => diagnostic.code = "FILE_OR_PROGRAM_MISSING".into(),
            std::io::ErrorKind::PermissionDenied => {
                diagnostic.code = "ACCESS_DENIED".into();
                diagnostic.action = "Check the path and its permissions. Use a folder owned by your account, then repeat the command.".into();
            }
            std::io::ErrorKind::AddrInUse => {
                diagnostic.code = "PORT_IN_USE".into();
                diagnostic.action = "Run figtether status and figtether doctor. To change ports, stop the daemon, run figtether start --port <free-port>, and import the printed dist/manifest.json in Figma.".into();
            }
            _ => {}
        }
    }
    diagnostic
}

pub fn report(error: &anyhow::Error, context: &Context) {
    let diagnostic = describe(error, context);
    if context.json {
        // Write to stderr directly: output::json is intentionally stdout-only.
        use std::io::Write;
        let mut stderr = std::io::stderr().lock();
        if let Ok(json) = serde_json::to_string(&diagnostic) {
            let _ = writeln!(stderr, "{json}");
        }
    } else {
        output::error(&format!("{}: {}", diagnostic.code, diagnostic.message));
        output::error(&format!("Next: {}", diagnostic.action));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_missing_daemon_has_a_recovery_command() {
        let diagnostic = describe(
            &ControlError::NotRunning("missing descriptor".into()).into(),
            &Context {
                command: "pair",
                json: true,
            },
        );
        assert_eq!(diagnostic.code, "DAEMON_NOT_RUNNING");
        assert!(diagnostic.action.contains("figtether start"));
        assert!(diagnostic.action.contains("--runtime-dir"));
    }
    #[test]
    fn the_error_chain_keeps_the_actual_cause_and_redacts_secrets() {
        let error = anyhow::anyhow!("{}", "a".repeat(64)).context("plugin build failed");
        let diagnostic = describe(
            &error,
            &Context {
                command: "start",
                json: false,
            },
        );
        assert!(diagnostic.message.contains("plugin build failed"));
        assert!(diagnostic.message.contains("redacted"));
        assert!(!diagnostic.message.contains(&"a".repeat(64)));
    }
}
