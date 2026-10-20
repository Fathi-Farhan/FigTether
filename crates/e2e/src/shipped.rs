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
use std::sync::Arc;
use std::time::Duration;

use figtether_daemon::client_api;
use figtether_daemon::logging;
use figtether_daemon::secrets::PairingSecrets;
use figtether_daemon::server;
use figtether_daemon::{run, DaemonConfig, DaemonFile, DaemonHandle, DaemonState, LogFormat};
use figtether_daemon::{RuntimeDir, Secret};
use serde_json::Value;
use tempfile::TempDir;
use tokio::sync::watch;
use uuid::Uuid;

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
        mint_pairing_secret_at(self.addr(), self.port()).await
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

/// Mint a one time pairing secret over the control endpoint of a daemon.
///
/// The request names the loopback host and carries no origin, so the header
/// guard admits it: a local tool is not a browser.
pub async fn mint_pairing_secret_at(addr: SocketAddr, port: u16) -> Result<String, HarnessError> {
    let mut stream = tokio::net::TcpStream::connect(addr)
        .await
        .map_err(HarnessError::Io)?;
    let request = figtether_daemon::control::pair_request(port);
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
        .ok_or_else(|| HarnessError::Protocol("the pairing endpoint minted no secret".to_string()))
}

/// A running instance of the shipped daemon surface with a shortened pairing
/// secret lifetime.
///
/// The helper serves the shipped router with the shipped state, so the
/// transport, the header guard, the bridge handlers, the session registry,
/// and the constant time comparison are the shipped ones. Only the lifetime
/// of the pairing secrets differs: the shipped lifetime is five minutes, and
/// a test may not sleep for five minutes to observe the expiry of a secret.
pub struct ShortLivedPairingDaemon {
    state: Arc<DaemonState>,
    shutdown_tx: watch::Sender<bool>,
    _runtime: TempDir,
    runtime_dir: PathBuf,
    port: u16,
    credential: String,
    _task: tokio::task::JoinHandle<()>,
}

impl ShortLivedPairingDaemon {
    /// Start the shipped daemon surface with a shortened pairing secret
    /// lifetime, and wait until the health endpoint answers.
    pub async fn start(ttl: Duration) -> Result<Self, HarnessError> {
        let port = free_loopback_port()?;
        let runtime = TempDir::new().map_err(HarnessError::Io)?;
        let runtime_dir = runtime.path().join("figtether");
        let directory = RuntimeDir::resolve(Some(&runtime_dir))
            .map_err(|err| HarnessError::Other(err.to_string()))?;

        let credential = Secret::generate();
        let instance_id = Uuid::new_v4();
        let installation_id = Uuid::new_v4();
        let daemon_file = DaemonFile {
            instance_id,
            installation_id,
            port,
            client_credential: credential.as_str().to_string(),
            protocol_version: figtether_protocol::PROTOCOL_VERSION.to_string(),
            created_at: chrono::Utc::now(),
        };
        directory
            .write_daemon_file(&daemon_file)
            .map_err(|err| HarnessError::Other(err.to_string()))?;

        let mut state = Arc::new(DaemonState::new(
            instance_id,
            installation_id,
            port,
            figtether_core::ResourceLimits::default(),
            credential.clone(),
        ));
        // The pairing registry is the only part of the state the helper
        // shortens. Everything else is the shipped default.
        Arc::get_mut(&mut state)
            .expect("the state is not shared yet")
            .pairing = Arc::new(PairingSecrets::new(ttl));

        let _ = logging::install(LogFormat::Compact, "info", &directory.log_file());

        let listener = server::bind_listener(port)
            .await
            .map_err(|err| HarnessError::Other(err.to_string()))?;
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let stop_signal = async move {
            let mut receiver = shutdown_rx;
            if *receiver.borrow() {
                return;
            }
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = receiver.changed() => {}
            }
        };

        let app = server::router(state.clone());
        tokio::spawn(client_api::run_worker(state.clone()));
        let server_state = state.clone();
        let server_runtime = directory.clone();
        let task = tokio::spawn(async move {
            let _ = axum::serve(
                listener,
                app.into_make_service_with_connect_info::<SocketAddr>(),
            )
            .with_graceful_shutdown(stop_signal)
            .await;
            figtether_daemon::graceful_shutdown(&server_state, &server_runtime);
        });

        let daemon = Self {
            state,
            shutdown_tx,
            _runtime: runtime,
            runtime_dir,
            port,
            credential: credential.as_str().to_string(),
            _task: task,
        };
        daemon.wait_healthy(READY_TIMEOUT).await?;
        Ok(daemon)
    }

    /// The port the daemon listens on.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The address the daemon listens on.
    pub fn addr(&self) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, self.port))
    }

    /// The bridge endpoint of the daemon.
    pub fn bridge_url(&self) -> String {
        format!("ws://127.0.0.1:{}/bridge", self.port)
    }

    /// The client endpoint of the daemon.
    pub fn client_url(&self) -> String {
        format!("ws://127.0.0.1:{}/client", self.port)
    }

    /// The runtime directory of the daemon.
    pub fn runtime_dir(&self) -> &Path {
        &self.runtime_dir
    }

    /// The credential a local client presents on the client endpoint.
    pub fn client_credential(&self) -> &str {
        &self.credential
    }

    /// Mint a one time pairing secret over the control endpoint.
    pub async fn mint_pairing_secret(&self) -> Result<String, HarnessError> {
        mint_pairing_secret_at(self.addr(), self.port).await
    }

    /// Number of sessions the daemon serves.
    pub fn session_count(&self) -> usize {
        self.state.registry.len()
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
        let _ = self.shutdown_tx.send(true);
        self.state.request_shutdown();
        Ok(())
    }
}

impl Drop for ShortLivedPairingDaemon {
    fn drop(&mut self) {
        // A failed test never leaves a listener behind.
        let _ = self.shutdown_tx.send(true);
        self.state.request_shutdown();
    }
}

impl std::fmt::Debug for ShortLivedPairingDaemon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShortLivedPairingDaemon")
            .field("port", &self.port)
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn the_short_lived_pairing_daemon_answers_and_holds_no_session() {
        let daemon = ShortLivedPairingDaemon::start(Duration::from_millis(50))
            .await
            .expect("the daemon starts");
        assert!(daemon.port() > 0);
        assert!(!daemon.client_credential().is_empty());
        assert_eq!(daemon.session_count(), 0);
        let secret = daemon
            .mint_pairing_secret()
            .await
            .expect("the daemon mints a pairing secret");
        assert!(!secret.is_empty());
        daemon.shutdown().await.expect("the daemon stops");
    }
}
