//! The runtime directory of the daemon.
//!
//! The daemon and the adapter agree on one convention for the owner only
//! runtime directory:
//!
//! - Windows: `%LOCALAPPDATA%\figtether`
//! - unix: `$XDG_STATE_HOME/figtether`, or `~/.local/state/figtether`
//!
//! The directory holds `daemon.json`, the file that carries the port of the
//! daemon and the credential of this client installation. The adapter reads
//! the file, never writes it, and never puts its content into a tool result.

use std::path::{Path, PathBuf};

use figtether_protocol::client::ClientAuth;
use figtether_protocol::PROTOCOL_VERSION;
use serde_json::Value;

use crate::endpoint::Endpoint;
use crate::error::McpError;

/// Name of the runtime file written by the daemon.
pub const RUNTIME_FILE_NAME: &str = "daemon.json";

/// Directory name used under the platform state directory.
pub const APP_DIR_NAME: &str = "figtether";

/// Default client id of an MCP adapter installation.
pub const DEFAULT_CLIENT_ID: &str = "figtether-mcp";

/// The platform directories the runtime directory can be derived from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlatformDirs {
    /// Windows only: the value of `LOCALAPPDATA`.
    pub local_app_data: Option<PathBuf>,
    /// unix only: the value of `XDG_STATE_HOME`.
    pub xdg_state_home: Option<PathBuf>,
    /// unix only: the value of `HOME`.
    pub home: Option<PathBuf>,
}

impl PlatformDirs {
    /// Read the platform directories from the process environment.
    pub fn from_env() -> Self {
        Self {
            local_app_data: std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
            xdg_state_home: std::env::var_os("XDG_STATE_HOME").map(PathBuf::from),
            home: std::env::var_os("HOME").map(PathBuf::from),
        }
    }
}

/// The runtime directory of the daemon.
pub fn runtime_dir() -> Result<PathBuf, McpError> {
    resolve_runtime_dir(&PlatformDirs::from_env())
}

/// Resolve the runtime directory from explicit platform directories.
///
/// The resolution is a pure function so that the convention can be tested on
/// every platform without touching the environment of the test run.
pub fn resolve_runtime_dir(dirs: &PlatformDirs) -> Result<PathBuf, McpError> {
    #[cfg(windows)]
    {
        let base = dirs
            .local_app_data
            .as_ref()
            .ok_or_else(|| McpError::RuntimeDirectory("LOCALAPPDATA is not set".to_string()))?;
        Ok(base.join(APP_DIR_NAME))
    }
    #[cfg(not(windows))]
    {
        if let Some(base) = dirs.xdg_state_home.as_ref() {
            return Ok(base.join(APP_DIR_NAME));
        }
        let home = dirs.home.as_ref().ok_or_else(|| {
            McpError::RuntimeDirectory("neither XDG_STATE_HOME nor HOME is set".to_string())
        })?;
        Ok(home.join(".local").join("state").join(APP_DIR_NAME))
    }
}

/// Failures of the runtime file format.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    /// The file is not valid JSON.
    #[error("the runtime file is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    /// A required field is missing.
    #[error("the runtime file does not contain the {0} field")]
    MissingField(&'static str),
    /// A field has the wrong shape.
    #[error("the runtime file field {field} is not a valid {expected}")]
    InvalidField {
        /// Name of the field.
        field: &'static str,
        /// What the field should have been.
        expected: &'static str,
    },
}

/// The content of the runtime file of the daemon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonRuntime {
    /// The port the daemon listens on.
    pub port: u16,
    /// The credential of this client installation.
    pub credential: String,
    /// The stable id of this client installation.
    pub client_id: String,
    /// The path of the client surface of the daemon.
    pub path: String,
    /// The identity of the daemon instance, when the file names one.
    pub daemon_instance_id: Option<String>,
}

impl DaemonRuntime {
    /// The endpoint of the client surface of the daemon.
    pub fn endpoint(&self) -> Endpoint {
        Endpoint::new("127.0.0.1", self.port, self.path.clone())
    }

    /// The first frame the adapter sends to the daemon.
    pub fn client_auth(&self) -> ClientAuth {
        ClientAuth {
            credential: self.credential.clone(),
            client_id: self.client_id.clone(),
            protocol_version: PROTOCOL_VERSION.to_string(),
        }
    }
}

/// Read the runtime file of the daemon from a runtime directory.
pub fn read_runtime_file(directory: &Path) -> Result<DaemonRuntime, McpError> {
    let path = directory.join(RUNTIME_FILE_NAME);
    let raw = std::fs::read_to_string(&path).map_err(|source| McpError::RuntimeFileUnreadable {
        path: path.clone(),
        source,
    })?;
    parse_runtime(&raw).map_err(|source| McpError::RuntimeFileInvalid { path, source })
}

/// Parse the content of the runtime file of the daemon.
///
/// The parser is tolerant about the spelling of the optional fields and
/// strict about the two fields the adapter cannot work without: the port and
/// the client credential.
pub fn parse_runtime(raw: &str) -> Result<DaemonRuntime, RuntimeError> {
    let value: Value = serde_json::from_str(raw)?;
    let port = match lookup(&value, &["port"]) {
        Some(Value::Number(number)) => number
            .as_u64()
            .and_then(|port| u16::try_from(port).ok())
            .ok_or(RuntimeError::InvalidField {
                field: "port",
                expected: "tcp port",
            })?,
        Some(_) => {
            return Err(RuntimeError::InvalidField {
                field: "port",
                expected: "tcp port",
            })
        }
        None => return Err(RuntimeError::MissingField("port")),
    };
    let credential = match lookup(
        &value,
        &["credential", "client_credential", "clientCredential"],
    ) {
        Some(Value::String(text)) if !text.trim().is_empty() => text.clone(),
        Some(_) => {
            return Err(RuntimeError::InvalidField {
                field: "credential",
                expected: "client credential",
            })
        }
        None => return Err(RuntimeError::MissingField("credential")),
    };
    let client_id = match lookup(&value, &["client_id", "clientId"]) {
        Some(Value::String(text)) if !text.trim().is_empty() => text.clone(),
        _ => DEFAULT_CLIENT_ID.to_string(),
    };
    let path = match lookup(&value, &["path", "client_path", "clientPath"]) {
        Some(Value::String(text)) if !text.trim().is_empty() => text.clone(),
        _ => crate::endpoint::DEFAULT_CLIENT_PATH.to_string(),
    };
    let daemon_instance_id = match lookup(
        &value,
        &["daemon_instance_id", "daemonInstanceId", "instance_id"],
    ) {
        Some(Value::String(text)) if !text.trim().is_empty() => Some(text.clone()),
        _ => None,
    };
    Ok(DaemonRuntime {
        port,
        credential,
        client_id,
        path,
        daemon_instance_id,
    })
}

fn lookup<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|key| value.get(*key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_runtime_file_carries_the_port_and_the_credential() {
        let runtime = parse_runtime(
            r#"{"port": 38947, "credential": "s3cret", "client_id": "mcp-1", "path": "/client"}"#,
        )
        .expect("parses");
        assert_eq!(runtime.port, 38947);
        assert_eq!(runtime.credential, "s3cret");
        assert_eq!(runtime.client_id, "mcp-1");
        assert_eq!(runtime.path, "/client");
        assert_eq!(runtime.endpoint().url(), "ws://127.0.0.1:38947/client");
    }

    #[test]
    fn the_client_id_and_the_path_have_defaults() {
        let runtime = parse_runtime(r#"{"port": 1234, "credential": "s3cret"}"#).expect("parses");
        assert_eq!(runtime.client_id, DEFAULT_CLIENT_ID);
        assert_eq!(runtime.path, "/client");
        assert!(runtime.daemon_instance_id.is_none());
    }

    #[test]
    fn a_file_without_a_credential_is_refused() {
        assert!(parse_runtime(r#"{"port": 38947}"#).is_err());
    }

    #[test]
    fn a_file_without_a_port_is_refused() {
        assert!(parse_runtime(r#"{"credential": "s3cret"}"#).is_err());
    }

    #[test]
    fn a_broken_file_is_refused() {
        assert!(parse_runtime("not json").is_err());
    }

    #[test]
    fn the_auth_frame_carries_the_credential_and_the_protocol_version() {
        let runtime = parse_runtime(r#"{"port": 38947, "credential": "s3cret"}"#).expect("parses");
        let auth = runtime.client_auth();
        assert_eq!(auth.credential, "s3cret");
        assert_eq!(auth.client_id, DEFAULT_CLIENT_ID);
        assert_eq!(auth.protocol_version, PROTOCOL_VERSION);
    }

    #[cfg(windows)]
    #[test]
    fn windows_uses_the_local_app_data_directory() {
        let dirs = PlatformDirs {
            local_app_data: Some(PathBuf::from(r"C:\Users\agent\AppData\Local")),
            ..PlatformDirs::default()
        };
        let resolved = resolve_runtime_dir(&dirs).expect("resolves");
        assert!(resolved.ends_with(APP_DIR_NAME));
    }

    #[cfg(not(windows))]
    #[test]
    fn unix_prefers_the_state_directory() {
        let dirs = PlatformDirs {
            xdg_state_home: Some(PathBuf::from("/state")),
            home: Some(PathBuf::from("/home/agent")),
            ..PlatformDirs::default()
        };
        assert_eq!(
            resolve_runtime_dir(&dirs).expect("resolves"),
            PathBuf::from("/state").join(APP_DIR_NAME)
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn unix_falls_back_to_the_local_state_directory() {
        let dirs = PlatformDirs {
            home: Some(PathBuf::from("/home/agent")),
            ..PlatformDirs::default()
        };
        assert_eq!(
            resolve_runtime_dir(&dirs).expect("resolves"),
            PathBuf::from("/home/agent")
                .join(".local")
                .join("state")
                .join(APP_DIR_NAME)
        );
    }
}
