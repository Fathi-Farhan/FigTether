//! The local control channel of the CLI.
//!
//! The CLI is a client of the local daemon, exactly like the MCP adapter. It
//! opens one authenticated loopback websocket, presents the credential from
//! the owner only runtime directory, and then exchanges the frozen request
//! and response frames of `figtether_protocol::client`.
//!
//! The credential never reaches an output: it is read from the runtime file,
//! used for the handshake, and forgotten.

use std::path::Path;
use std::time::Duration;

use figtether_mcp::{DaemonClient, Endpoint, McpError};
use figtether_model::ids::JobId;
use figtether_protocol::client::{ClientAuth, ClientRequest, ClientResponse};

use crate::cli::DEFAULT_PORT;
use crate::runtime;

/// How long the CLI waits for one answer of the daemon.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// The client id the CLI presents to the daemon.
pub const CLIENT_ID: &str = "figtether-cli";

/// Path of the local client surface of the daemon.
pub const CLIENT_PATH: &str = "/client";

/// Failures of the control channel.
#[derive(Debug, thiserror::Error)]
pub enum ControlError {
    /// The runtime directory of the daemon is not usable.
    #[error("the runtime directory of the daemon is not usable: {0}")]
    Runtime(String),
    /// No daemon is running.
    #[error("no daemon is running ({0})")]
    NotRunning(String),
    /// The daemon answered with a structured error.
    #[error("the daemon answered {code}: {message}")]
    Daemon {
        /// Error code of the answer.
        code: String,
        /// Human readable message.
        message: String,
    },
    /// The channel itself failed.
    #[error("the local control channel failed: {0}")]
    Channel(String),
}

/// A connected local client.
pub struct ControlClient {
    client: DaemonClient,
}

impl ControlClient {
    /// Connect to the daemon and authenticate with the local credential.
    ///
    /// The port is taken from the runtime file unless the caller asked for a
    /// specific one. The credential is read from the same file, so a daemon
    /// that is not running is reported as such instead of as an
    /// authentication failure.
    pub async fn connect(
        port: Option<u16>,
        runtime_dir: Option<&Path>,
    ) -> Result<Self, ControlError> {
        let dir =
            runtime::resolve(runtime_dir).map_err(|err| ControlError::Runtime(err.to_string()))?;
        let descriptor = runtime::daemon_file(&dir).ok_or_else(|| {
            ControlError::NotRunning(format!(
                "the runtime file {} does not name a running daemon",
                dir.daemon_file().display()
            ))
        })?;
        let port = port.unwrap_or(descriptor.port);
        let auth = ClientAuth {
            credential: descriptor.client_credential,
            client_id: CLIENT_ID.to_string(),
            protocol_version: figtether_protocol::PROTOCOL_VERSION.to_string(),
        };
        let endpoint = Endpoint::new("127.0.0.1", port, CLIENT_PATH);
        let client = DaemonClient::new(endpoint, auth);
        client.connect().await.map_err(map_error)?;
        Ok(Self { client })
    }

    /// Send one request and return the payload of a successful answer.
    pub async fn request(&self, request: ClientRequest) -> Result<serde_json::Value, ControlError> {
        let response = self
            .client
            .call(
                request,
                JobId::new(),
                REQUEST_TIMEOUT,
                std::future::pending::<()>(),
            )
            .await
            .map_err(map_error)?;
        match response {
            ClientResponse::Ok { envelope, .. } => Ok(envelope.data),
            ClientResponse::Error { error, .. } => Err(ControlError::Daemon {
                code: error.code.as_str().to_string(),
                message: error.message,
            }),
            ClientResponse::Accepted { .. } => Err(ControlError::Channel(
                "the daemon accepted the request and did not answer yet".to_string(),
            )),
        }
    }
}

/// The port the daemon is expected to use.
///
/// A port that the user names wins. The runtime file names the port of the
/// running daemon. Without it, the local plugin manifest names the port this
/// machine uses, because the plugin reads its port from that manifest. The
/// default port is the last resort.
pub async fn expected_port(port: Option<u16>, runtime_dir: Option<&Path>) -> u16 {
    expected_port_for(port, runtime_dir, None).await
}

pub async fn expected_port_for(
    port: Option<u16>,
    runtime_dir: Option<&Path>,
    manifest: Option<&Path>,
) -> u16 {
    if let Some(port) = port {
        return port;
    }
    if let Some(port) = running_daemon_port(runtime_dir).await {
        return port;
    }
    expected_port_from(port, || {
        crate::commands::manifest::discover_manifest_in(manifest, runtime_dir)
            .and_then(|path| crate::commands::manifest::read_manifest(&path).ok())
            .and_then(|value| crate::commands::manifest::manifest_port(&value))
    })
}

/// The port, with the manifest lookup kept separate.
///
/// The separation keeps the rule testable without a checkout next to the test.
pub fn expected_port_from(port: Option<u16>, manifest_port: impl FnOnce() -> Option<u16>) -> u16 {
    if let Some(port) = port {
        return port;
    }
    manifest_port().unwrap_or(DEFAULT_PORT)
}

/// Verify the authenticated protocol and instance, not just an open TCP port.
/// Bound the entire probe so a stale file naming an unrelated service cannot hang setup.
pub async fn running_daemon_port(runtime_dir: Option<&Path>) -> Option<u16> {
    let dir = runtime::resolve(runtime_dir).ok()?;
    let descriptor = runtime::daemon_file(&dir)?;
    tokio::time::timeout(Duration::from_millis(750), async {
        let client = ControlClient::connect(Some(descriptor.port), runtime_dir)
            .await
            .ok()?;
        let data = client
            .request(ClientRequest::Status {
                request_id: new_request_id(),
            })
            .await
            .ok()?;
        (data["instance_id"].as_str()? == descriptor.instance_id.to_string()
            && data["port"].as_u64()? == u64::from(descriptor.port))
        .then_some(descriptor.port)
    })
    .await
    .ok()
    .flatten()
}

/// Mint a correlation id for one request.
pub fn new_request_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Map a failure of the adapter onto a failure of the control channel.
fn map_error(error: McpError) -> ControlError {
    match error {
        McpError::DaemonUnavailable { reason, .. } => ControlError::NotRunning(reason),
        McpError::DaemonRejected(error) => ControlError::Daemon {
            code: error.code.as_str().to_string(),
            message: error.message,
        },
        other => ControlError::Channel(other.to_string()),
    }
}

/// Read a string field of a sanitized payload.
pub fn text(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Read a number field of a sanitized payload.
pub fn number(value: &serde_json::Value, key: &str) -> u64 {
    value
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0)
}

/// Read the first number field that is present.
///
/// A counter may be named in more than one way across the build rounds, and
/// the CLI must not depend on the exact spelling of a sanitized field.
pub fn number_any(value: &serde_json::Value, keys: &[&str]) -> u64 {
    for key in keys {
        if let Some(found) = value.get(*key).and_then(serde_json::Value::as_u64) {
            return found;
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn an_open_port_with_a_stale_descriptor_is_not_a_running_daemon() {
        let temp = tempfile::tempdir().expect("temp");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("listener");
        let dir = runtime::resolve(Some(temp.path())).expect("runtime");
        dir.write_daemon_file(&figtether_daemon::DaemonFile {
            instance_id: uuid::Uuid::new_v4(),
            installation_id: uuid::Uuid::new_v4(),
            port: listener.local_addr().expect("address").port(),
            client_credential: "synthetic-stale-credential".into(),
            protocol_version: figtether_protocol::PROTOCOL_VERSION.into(),
            created_at: chrono::Utc::now(),
        })
        .expect("stale descriptor");
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            running_daemon_port(Some(temp.path())),
        )
        .await
        .expect("bounded probe");
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn the_authenticated_daemon_is_recognised_and_a_wrong_instance_is_rejected() {
        let temp = tempfile::tempdir().expect("temp");
        let socket = std::net::TcpListener::bind("127.0.0.1:0").expect("free port");
        let port = socket.local_addr().expect("address").port();
        drop(socket);
        let handle = figtether_daemon::run(
            figtether_daemon::DaemonConfig::new()
                .with_port(port)
                .with_runtime_dir(temp.path()),
        )
        .await
        .expect("daemon");
        assert_eq!(running_daemon_port(Some(temp.path())).await, Some(port));
        let dir = runtime::resolve(Some(temp.path())).expect("runtime");
        let mut file = dir.read_daemon_file().expect("descriptor");
        file.instance_id = uuid::Uuid::new_v4();
        dir.write_daemon_file(&file).expect("stale identity");
        assert_eq!(running_daemon_port(Some(temp.path())).await, None);
        handle.shutdown();
        handle.wait_stopped().await;
    }

    /// Run one future on a throwaway runtime.
    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(future)
    }

    #[test]
    fn a_missing_runtime_file_means_no_daemon_runs() {
        let temp = tempfile::tempdir().expect("temp dir");
        let error = match block_on(ControlClient::connect(None, Some(temp.path()))) {
            Ok(_) => panic!("the connection must be refused"),
            Err(error) => error,
        };
        assert!(matches!(error, ControlError::NotRunning(_)), "{error}");
    }

    #[test]
    fn the_expected_port_falls_back_to_the_bridge_port() {
        assert_eq!(expected_port_from(None, || None), DEFAULT_PORT);
        assert_eq!(expected_port_from(Some(1234), || None), 1234);
    }

    #[test]
    fn the_manifest_port_is_used_before_the_default_port() {
        assert_eq!(expected_port_from(None, || Some(39029)), 39029);
    }

    #[test]
    fn a_sanitized_field_is_read() {
        let value = json!({ "daemon_version": "0.1.0", "sessions": 3 });
        assert_eq!(text(&value, "daemon_version"), "0.1.0");
        assert_eq!(number(&value, "sessions"), 3);
        assert_eq!(text(&value, "missing"), "");
    }

    #[test]
    fn the_first_present_field_wins() {
        let value = json!({ "pairing_active": 1, "pairing_secrets_active": 2 });
        assert_eq!(
            number_any(&value, &["pairing_active", "pairing_secrets_active"]),
            1
        );
        assert_eq!(
            number_any(&value, &["missing", "pairing_secrets_active"]),
            2
        );
        assert_eq!(number_any(&value, &["missing"]), 0);
    }
}
