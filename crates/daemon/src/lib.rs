//! FigTether daemon crate.
//!
//! The daemon is the local process that owns the sessions, the grants, the
//! scheduler, and the cache. It speaks the bridge protocol to the Figma
//! plugin over an authenticated loopback WebSocket and serves the local
//! control surface used by the CLI and the MCP adapter.
//!
//! The daemon binds `127.0.0.1` only. It never opens a connection to a
//! remote host, and it never writes design content to disk.
//!
//! # Ownership
//!
//! A session belongs to the local installation that granted it. The
//! installation is named by the identity in the runtime file, and the client
//! credential in that file is the only thing that binds a local client to it.
//! The client id a client presents is a label: it names the caller and never
//! decides which sessions the caller may see.
//!
//! # Layout
//!
//! - [`config`] holds the configuration of one daemon instance;
//! - [`runtime_dir`] resolves and protects the owner only runtime directory;
//! - [`secrets`] mints secrets and compares them in constant time;
//! - [`bridge`] owns the plugin connection, the auth window, the heartbeat,
//!   and the connection epochs;
//! - [`client_api`] owns the local client connection and the request
//!   routing;
//! - [`control`] owns the pairing secret, the sanitized status, and the
//!   revocation;
//! - [`write_path`] owns the approvals, the approval tokens, and the audit
//!   trail of the write path;
//! - [`server`] owns the router and the loopback listener;
//! - [`logging`] owns the sanitized diagnostic log.

pub mod bridge;
pub mod client_api;
pub mod config;
pub mod control;
pub mod cursor;
pub mod grants;
pub mod logging;
pub mod runtime_dir;
pub mod secrets;
pub mod server;
pub mod write_path;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Utc};
use figtether_core::{
    Cache, InMemoryAssetStore, Metrics, ResourceLimits, Scheduler, SessionRegistry,
};
use figtether_protocol::PROTOCOL_VERSION;
use tokio::sync::{watch, Notify};
use uuid::Uuid;

pub use config::{DaemonConfig, LogFormat};
pub use control::{DaemonStatus, PairSecretResponse, RevokeOutcome};
pub use runtime_dir::{DaemonFile, RuntimeDir, RuntimeDirError};
pub use secrets::Secret;
pub use server::DaemonError;

use bridge::BridgeHub;
use client_api::JobLedger;

/// Version of the daemon.
pub const DAEMON_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The shared state of one daemon instance.
///
/// The state owns the domain: the session registry, the scheduler, the
/// cache, and the asset store. It also owns the bridge hub, which connects
/// the scheduler with the live plugin connections.
pub struct DaemonState {
    /// Identity of this daemon instance.
    pub instance_id: Uuid,
    /// Identity of the local installation that owns the sessions.
    ///
    /// The installation is the machine together with the runtime directory
    /// that holds the client credential. Every local client that reads that
    /// credential belongs to this installation, and the installation owns the
    /// sessions that were granted on it.
    pub installation_id: Uuid,
    /// Port of the loopback listener.
    pub port: u16,
    /// Address the daemon binds.
    pub bind_address: String,
    /// Resource budgets of the daemon.
    pub limits: ResourceLimits,
    /// The session and grant registry.
    pub registry: SessionRegistry,
    /// The scheduler of the editor operations.
    pub scheduler: Scheduler,
    /// The design cache.
    pub cache: Cache,
    /// The asset store.
    pub assets: InMemoryAssetStore,
    /// The counters of the daemon.
    pub metrics: Metrics,
    /// The pending pairing secrets.
    pub pairing: Arc<secrets::PairingSecrets>,
    /// The credential the local clients present.
    pub client_credential: Arc<Secret>,
    /// The hub between the scheduler and the plugin connections.
    pub hub: Arc<BridgeHub>,
    /// The ledger of the dispatched jobs.
    pub jobs: Arc<JobLedger>,
    /// The cursors of the truncated traversals.
    pub cursors: Arc<cursor::CursorStore>,
    /// The grants the daemon issued for the sessions.
    pub grants: Arc<grants::GrantLedger>,
    /// The write path: the approvals, the tokens, and the audit trail.
    pub writes: Arc<write_path::WritePath>,
    /// Signal of the graceful shutdown.
    shutdown_tx: watch::Sender<bool>,
    /// Signal that the daemon stopped serving.
    stopped_tx: watch::Sender<bool>,
    /// Signal that work is waiting in the scheduler.
    pub work_notify: Arc<Notify>,
    /// When the daemon started.
    pub started_at: DateTime<Utc>,
}

impl DaemonState {
    /// Build the state of a daemon instance.
    ///
    /// The scheduler receives its own counter set because
    /// [`figtether_core::Metrics`] is not shareable. The daemon reports the
    /// counters of the shared instance.
    pub fn new(
        instance_id: Uuid,
        installation_id: Uuid,
        port: u16,
        limits: ResourceLimits,
        client_credential: Secret,
    ) -> Self {
        let metrics = Metrics::new();
        Self {
            instance_id,
            installation_id,
            port,
            bind_address: format!("127.0.0.1:{port}"),
            registry: SessionRegistry::new(limits.clone()),
            scheduler: Scheduler::new(limits.clone(), Metrics::new()),
            cache: Cache::new(limits.clone()),
            assets: InMemoryAssetStore::new(limits.clone()),
            metrics,
            pairing: Arc::new(secrets::PairingSecrets::default()),
            client_credential: Arc::new(client_credential),
            hub: Arc::new(BridgeHub::new()),
            jobs: Arc::new(JobLedger::new()),
            cursors: Arc::new(cursor::CursorStore::new()),
            grants: Arc::new(grants::GrantLedger::new()),
            writes: Arc::new(write_path::WritePath::new(limits.clone())),
            shutdown_tx: watch::channel(false).0,
            stopped_tx: watch::channel(false).0,
            work_notify: Arc::new(Notify::new()),
            started_at: Utc::now(),
            limits,
        }
    }

    /// The key of the sessions this installation owns.
    ///
    /// The key is the identity of the installation, never the client id a
    /// local client presented. Two clients of the same installation share the
    /// key, and a client of another installation never has it.
    pub fn owner_key(&self) -> String {
        self.installation_id.to_string()
    }

    /// Whether the presented credential is the credential of this installation.
    ///
    /// The comparison runs in constant time, so a caller cannot learn the
    /// credential by timing the answer.
    pub fn credential_matches(&self, presented: &str) -> bool {
        secrets::constant_time_eq(presented, self.client_credential.as_str())
    }

    /// The owner of the sessions a local client may see.
    ///
    /// The credential is the only thing that binds a client to an
    /// installation. A client that presents a wrong credential has no owner at
    /// all and never sees a session.
    pub fn owner_of(&self, presented_credential: &str) -> Option<String> {
        if self.credential_matches(presented_credential) {
            Some(self.owner_key())
        } else {
            None
        }
    }

    /// Whether the daemon was asked to stop.
    pub fn shutdown_requested(&self) -> bool {
        *self.shutdown_tx.borrow()
    }

    /// Ask the daemon to stop.
    pub fn request_shutdown(&self) {
        self.shutdown_tx.send_replace(true);
        self.work_notify.notify_waiters();
    }
}

/// Read a domain value defensively.
///
/// The domain implementation lives in the core crate. A panic inside it must
/// not take the daemon down: the health endpoint still answers with the
/// values it knows, and the shutdown still removes the runtime files.
pub(crate) fn guarded<T>(fallback: T, read: impl FnOnce() -> T) -> T {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(read)) {
        Ok(value) => value,
        Err(_) => fallback,
    }
}

/// Run a domain call that may fail.
///
/// A panic inside the domain is reported as a structured error instead of
/// taking the connection down.
pub(crate) fn domain_call<T>(
    fallback_code: figtether_model::errors::ErrorCode,
    read: impl FnOnce() -> Result<T, figtether_model::errors::FigError>,
) -> Result<T, figtether_model::errors::FigError> {
    guarded(
        Err(figtether_model::errors::FigError::new(
            fallback_code,
            "the domain refused the operation",
        )),
        read,
    )
}

/// A handle to a running daemon.
///
/// The handle owns the stop signal of the daemon and the identity of the
/// instance. It never carries a secret.
#[derive(Clone)]
pub struct DaemonHandle {
    /// Identity of the daemon instance.
    instance_id: Uuid,
    /// Identity of the local installation.
    installation_id: Uuid,
    /// Port of the loopback listener.
    port: u16,
    /// Path of the runtime directory.
    runtime_dir: PathBuf,
    /// The shared state of the daemon.
    state: Arc<DaemonState>,
}

impl std::fmt::Debug for DaemonHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DaemonHandle")
            .field("instance_id", &self.instance_id)
            .field("installation_id", &self.installation_id)
            .field("port", &self.port)
            .field("runtime_dir", &self.runtime_dir)
            .finish_non_exhaustive()
    }
}

impl DaemonHandle {
    /// The port the daemon listens on.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The identity of the daemon instance.
    pub fn instance_id(&self) -> Uuid {
        self.instance_id
    }

    /// The identity of the local installation.
    ///
    /// The installation owns every session that was granted on this machine.
    /// The identity is not a secret: it names the owner of the sessions, not
    /// a way to reach them.
    pub fn installation_id(&self) -> Uuid {
        self.installation_id
    }

    /// The runtime directory of the daemon.
    pub fn runtime_dir(&self) -> &std::path::Path {
        &self.runtime_dir
    }

    /// The sanitized status of the daemon.
    pub fn status(&self) -> DaemonStatus {
        control::status(&self.state)
    }

    /// Ask the daemon to stop gracefully.
    pub fn shutdown(&self) {
        self.state.request_shutdown();
    }

    /// Wait until the daemon stopped serving.
    pub async fn wait_stopped(&self) {
        let mut receiver = self.state.stopped_tx.subscribe();
        if *receiver.borrow() {
            return;
        }
        let _ = receiver.changed().await;
    }
}

/// Start the daemon and return a handle to it.
///
/// The daemon runs in the background. The caller stops it with
/// [`DaemonHandle::shutdown`] or with the control message of the shutdown
/// endpoint. A `SIGINT` stops it as well.
pub async fn run(config: DaemonConfig) -> Result<DaemonHandle, DaemonError> {
    let (handle, _task) = start(config).await?;
    Ok(handle)
}

/// Start the daemon and run it in the foreground.
///
/// The function returns when the daemon stopped, either because of a
/// `SIGINT` or because of the control message of the shutdown endpoint.
pub async fn run_until_stopped(config: DaemonConfig) -> Result<(), DaemonError> {
    let (_handle, task) = start(config).await?;
    task.await.map_err(|err| {
        DaemonError::Serve(std::io::Error::other(format!(
            "the daemon task ended unexpectedly: {err}"
        )))
    })
}

/// Start the daemon and return the handle together with the server task.
async fn start(
    config: DaemonConfig,
) -> Result<(DaemonHandle, tokio::task::JoinHandle<()>), DaemonError> {
    let runtime_dir = RuntimeDir::resolve(config.runtime_dir()).map_err(DaemonError::Runtime)?;

    let credential = Secret::generate();
    let installation_id = Uuid::new_v4();
    let daemon_file = DaemonFile {
        instance_id: config.instance_id(),
        installation_id,
        port: config.port(),
        client_credential: credential.as_str().to_string(),
        protocol_version: PROTOCOL_VERSION.to_string(),
        created_at: Utc::now(),
    };
    runtime_dir
        .write_daemon_file(&daemon_file)
        .map_err(DaemonError::Runtime)?;

    // The credential the clients present is the one that lives in the
    // runtime directory. Reading it back keeps the file and the daemon in
    // sync, and it makes the file the single source of truth. The same holds
    // for the identity of the installation: the file names the owner of the
    // sessions, and every local client of this daemon resolves the same
    // owner from the credential it reads there.
    let stored = runtime_dir
        .read_daemon_file()
        .map_err(DaemonError::Runtime)?;
    let credential = Secret::from_value(stored.client_credential.as_str());
    let installation_id = stored.installation_id;

    let _ = logging::purge_old_logs(runtime_dir.path());
    let _ = logging::install(
        config.log_format(),
        config.log_level(),
        &runtime_dir.log_file(),
    );

    let state = Arc::new(DaemonState::new(
        config.instance_id(),
        installation_id,
        config.port(),
        config.limits().clone(),
        credential,
    ));

    let listener = server::bind_listener(config.port()).await?;
    let addr: SocketAddr = listener.local_addr().map_err(DaemonError::Bind)?;

    let handle = DaemonHandle {
        instance_id: config.instance_id(),
        installation_id,
        port: addr.port(),
        runtime_dir: runtime_dir.path().to_path_buf(),
        state: state.clone(),
    };

    tracing::info!(
        code = "DAEMON_STARTED",
        "daemon listening on the loopback interface"
    );

    tokio::spawn(client_api::run_worker(state.clone()));

    let app = server::router(state.clone());
    let shutdown = state.shutdown_tx.clone();
    let stop_signal = async move {
        let mut receiver = shutdown.subscribe();
        if *receiver.borrow() {
            // The stop was already requested before the server started.
            return;
        }
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = receiver.changed() => {}
        }
    };

    let server_state = state.clone();
    let server_runtime = runtime_dir.clone();
    let task = tokio::spawn(async move {
        let result = axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(stop_signal)
        .await;
        graceful_shutdown(&server_state, &server_runtime);
        server_state.stopped_tx.send_replace(true);
        if let Err(err) = result {
            tracing::error!(
                code = "INTERNAL_ERROR",
                "the daemon stopped with an error: {err}"
            );
        }
    });

    Ok((handle, task))
}

/// The graceful shutdown sequence.
///
/// The order is fixed: stop accepting, cancel the queue, revoke the
/// credentials, close the sockets, wipe the design memory, and remove the
/// runtime files.
pub fn graceful_shutdown(state: &Arc<DaemonState>, runtime_dir: &RuntimeDir) {
    tracing::info!(code = "DAEMON_STOPPING", "daemon is shutting down");

    state.request_shutdown();

    let sessions = guarded(Vec::new(), || state.registry.list_all());
    for session in &sessions {
        let _ = guarded(0, || state.scheduler.cancel_session(&session.session_id));
    }

    state.pairing.revoke_all();

    for session in &sessions {
        state.hub.detach(&session.session_id);
    }

    control::wipe_design_memory(state);

    if let Err(err) = runtime_dir.remove_runtime_files() {
        tracing::warn!(
            code = "INTERNAL_ERROR",
            "runtime files could not be removed: {err}"
        );
    }

    tracing::info!(code = "DAEMON_STOPPED", "daemon stopped");
}

/// The uptime of the daemon in seconds.
pub fn uptime_seconds(state: &DaemonState) -> u64 {
    let now = Utc::now();
    now.signed_duration_since(state.started_at)
        .num_seconds()
        .max(0) as u64
}

/// The instant the daemon started.
pub fn started_at(state: &DaemonState) -> DateTime<Utc> {
    state.started_at
}

/// The moment the daemon started, as a monotonic instant.
pub fn started_instant() -> Instant {
    Instant::now()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_daemon_version_is_the_package_version() {
        assert_eq!(DAEMON_VERSION, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn the_state_reports_the_loopback_address() {
        let state = DaemonState::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            38947,
            ResourceLimits::default(),
            Secret::generate(),
        );
        assert_eq!(state.bind_address, "127.0.0.1:38947");
        assert!(!state.shutdown_requested());
    }

    #[test]
    fn a_shutdown_request_is_visible() {
        let state = DaemonState::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            38947,
            ResourceLimits::default(),
            Secret::generate(),
        );
        state.request_shutdown();
        assert!(state.shutdown_requested());
    }

    #[test]
    fn the_owner_is_the_installation_and_not_the_client_id() {
        let installation_id = Uuid::new_v4();
        let credential = Secret::generate();
        let state = DaemonState::new(
            Uuid::new_v4(),
            installation_id,
            38947,
            ResourceLimits::default(),
            credential.clone(),
        );

        let owner = state
            .owner_of(credential.as_str())
            .expect("the credential names the owner");
        assert_eq!(owner, installation_id.to_string());
        assert_eq!(state.owner_key(), owner);
    }

    #[test]
    fn a_wrong_credential_has_no_owner() {
        let state = DaemonState::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            38947,
            ResourceLimits::default(),
            Secret::generate(),
        );
        assert!(state.owner_of("wrong-credential").is_none());
        assert!(state.owner_of("").is_none());
    }
}
