//! Lifecycle helpers for the shipped `figtether-daemon` crate.
//!
//! The helpers start the real daemon on a free loopback port, wait until the
//! health endpoint answers, and stop it cleanly at the end of the test. The
//! runtime directory is a temporary directory, so a test never touches the
//! runtime directory of the user and never leaves a credential behind.
//!
//! The free port is picked with a standard library listener: the listener
//! binds `127.0.0.1:0`, the assigned port is read, and the listener is
//! dropped before the daemon binds the port. The daemon refuses port zero,
//! so the port has to be picked this way.

use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::time::Duration;

use figtether_daemon::{run, DaemonConfig, DaemonHandle};
use serde_json::Value;
use tempfile::TempDir;

use crate::harness::HarnessError;

/// How long the harness waits for the daemon to answer on its health
/// endpoint. The deadline is generous, so a slow machine does not turn a
/// passing flow into a failure.
pub const READY_TIMEOUT: Duration = Duration::from_secs(20);

/// Name of the daemon descriptor inside the runtime directory.
pub const DAEMON_FILE_NAME: &str = "daemon.json";

/// Pick a free port on the loopback interface.
///
/// The listener is closed before the port is handed to the daemon, so the
/// port is free when the daemon binds it.
pub fn free_loopback_port() -> Result<u16, HarnessError> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(HarnessError::Io)?;
    let port = listener.local_addr().map_err(HarnessError::Io)?.port();
    drop(listener);
    Ok(port)
}

/// A running instance of the shipped daemon.
pub struct ShippedDaemon {
    handle: DaemonHandle,
    _runtime: TempDir,
    runtime_dir: PathBuf,
}

impl ShippedDaemon {
    /// Start the daemon on a free loopback port with a temporary runtime
    /// directory, and wait until it answers on its health endpoint.
    pub async fn start() -> Result<Self, HarnessError> {
        let port = free_loopback_port()?;
        let runtime = TempDir::new().map_err(HarnessError::Io)?;
        let runtime_dir = runtime.path().join("figtether");
        let config = DaemonConfig::new()
            .with_port(port)
            .with_runtime_dir(&runtime_dir);
        let handle = run(config)
            .await
            .map_err(|err| HarnessError::Other(format!("the daemon did not start: {err}")))?;
        let daemon = Self {
            handle,
            _runtime: runtime,
            runtime_dir,
        };
        daemon.wait_healthy(READY_TIMEOUT).await?;
        Ok(daemon)
    }

    /// The handle of the running daemon.
    pub fn handle(&self) -> &DaemonHandle {
        &self.handle
    }

    /// The port the daemon listens on.
    pub fn port(&self) -> u16 {
        self.handle.port()
    }

    /// The address the daemon listens on.
    pub fn addr(&self) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, self.port()))
    }

    /// The identity of the daemon instance.
    pub fn instance_id(&self) -> String {
        self.handle.instance_id().to_string()
    }

    /// The runtime directory of the daemon.
    pub fn runtime_dir(&self) -> &Path {
        &self.runtime_dir
    }

    /// The path of the daemon descriptor.
    pub fn daemon_file(&self) -> PathBuf {
        self.runtime_dir.join(DAEMON_FILE_NAME)
    }

    /// The bridge endpoint of the daemon.
    pub fn bridge_url(&self) -> String {
        format!("ws://127.0.0.1:{}/bridge", self.port())
    }

    /// The client endpoint of the daemon.
    pub fn client_url(&self) -> String {
        format!("ws://127.0.0.1:{}/client", self.port())
    }

    /// The credential a local client presents on the client endpoint.
    ///
    /// The credential lives in the daemon descriptor of the owner only
    /// runtime directory. The harness reads it from there, exactly like the
    /// MCP adapter does.
    pub fn client_credential(&self) -> Result<String, HarnessError> {
        let body = std::fs::read_to_string(self.daemon_file()).map_err(HarnessError::Io)?;
        let value: Value =
            serde_json::from_str(&body).map_err(|err| HarnessError::Protocol(err.to_string()))?;
        value
            .get("client_credential")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                HarnessError::Protocol("the daemon file carries no client credential".to_string())
            })
    }

    /// The content of the daemon descriptor.
    pub fn daemon_file_value(&self) -> Result<Value, HarnessError> {
        let body = std::fs::read_to_string(self.daemon_file()).map_err(HarnessError::Io)?;
        serde_json::from_str(&body).map_err(|err| HarnessError::Protocol(err.to_string()))
    }

    /// Mint a one time pairing secret over the control endpoint.
    ///
    /// The secret is the only credential the plugin needs, it lives in memory
    /// only, and it is spent by the first frame of the plugin.
    pub async fn mint_pairing_secret(&self) -> Result<String, HarnessError> {
        let port = self.port();
        let addr = self.addr();
        let mut stream = tokio::net::TcpStream::connect(addr)
            .await
            .map_err(HarnessError::Io)?;
        let request = format!(
            "POST /control/pair HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
        tokio::io::AsyncWriteExt::write_all(&mut stream, request.as_bytes())
            .await
            .map_err(HarnessError::Io)?;
        let mut body = Vec::new();
        tokio::io::AsyncReadExt::read_to_end(&mut stream, &mut body)
            .await
            .map_err(HarnessError::Io)?;
        let text = String::from_utf8_lossy(&body).to_string();
        let payload = text
            .split_once("\r\n\r\n")
            .map(|(_, body)| body)
            .unwrap_or_default();
        let value: Value =
            serde_json::from_str(payload).map_err(|err| HarnessError::Protocol(err.to_string()))?;
        value
            .get("secret")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                HarnessError::Protocol("the pairing endpoint minted no secret".to_string())
            })
    }

    /// Wait until the health endpoint answers.
    pub async fn wait_healthy(&self, timeout: Duration) -> Result<(), HarnessError> {
        let addr = self.addr();
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if crate::harness::probe_health(addr).await.is_ok() {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(HarnessError::Timeout(format!(
                    "the daemon at {addr} did not answer within {timeout:?}"
                )));
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Ask the daemon to stop and wait for the listener to close.
    pub async fn shutdown(self) -> Result<(), HarnessError> {
        self.handle.shutdown();
        tokio::time::timeout(Duration::from_secs(10), self.handle.wait_stopped())
            .await
            .map_err(|_elapsed| {
                HarnessError::Timeout("the daemon did not stop within ten seconds".to_string())
            })?;
        Ok(())
    }
}

impl Drop for ShippedDaemon {
    fn drop(&mut self) {
        // A failed test never leaves a listener behind.
        self.handle.shutdown();
    }
}

impl std::fmt::Debug for ShippedDaemon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShippedDaemon")
            .field("port", &self.port())
            .field("runtime_dir", &self.runtime_dir)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_free_port_is_free_again_after_the_helper_returns() {
        let port = free_loopback_port().expect("a free port");
        assert_ne!(port, 0, "the daemon refuses port zero");
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).expect("the port is free");
        drop(listener);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_shipped_daemon_answers_and_stops() {
        let daemon = ShippedDaemon::start().await.expect("the daemon starts");
        assert!(daemon.port() > 0);
        assert!(daemon.daemon_file().is_file());
        assert!(!daemon.client_credential().expect("a credential").is_empty());
        let descriptor = daemon.daemon_file();
        daemon.shutdown().await.expect("the daemon stops");
        assert!(
            !descriptor.exists(),
            "the descriptor is removed when the daemon stops"
        );
    }
}
