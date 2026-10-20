//! Lifecycle helpers for the harness daemon.
//!
//! The helpers start the daemon on an ephemeral loopback port, wait until it
//! answers, and shut it down cleanly even when a test fails. The loopback
//! helper proves that the daemon never listens on a routable address, which
//! is the runtime half of `AT01` and `AT02`.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::daemon::{DaemonProfile, HarnessDaemon};

/// Failure of a harness operation.
#[derive(Debug)]
pub enum HarnessError {
    /// An input or output failure.
    Io(std::io::Error),
    /// A connection could not be established or was closed.
    Connection(String),
    /// The peer spoke something the harness does not understand.
    Protocol(String),
    /// The operation ran out of time.
    Timeout(String),
    /// Any other failure.
    Other(String),
}

impl std::fmt::Display for HarnessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "io failure: {err}"),
            Self::Connection(message) => write!(f, "connection failure: {message}"),
            Self::Protocol(message) => write!(f, "protocol failure: {message}"),
            Self::Timeout(message) => write!(f, "timeout: {message}"),
            Self::Other(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for HarnessError {}

impl From<std::io::Error> for HarnessError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

/// A running harness daemon with a lifecycle around it.
pub struct DaemonHarness {
    daemon: Option<HarnessDaemon>,
    profile: DaemonProfile,
}

impl DaemonHarness {
    /// Start a daemon with the fast test profile.
    pub async fn start() -> Result<Self, HarnessError> {
        Self::start_with(DaemonProfile::fast()).await
    }

    /// Start a daemon with an explicit profile and wait until it is ready.
    pub async fn start_with(profile: DaemonProfile) -> Result<Self, HarnessError> {
        let daemon = HarnessDaemon::start(profile.clone()).await?;
        let harness = Self {
            daemon: Some(daemon),
            profile,
        };
        harness.wait_ready(Duration::from_secs(10)).await?;
        Ok(harness)
    }

    /// The profile the daemon runs with.
    pub fn profile(&self) -> &DaemonProfile {
        &self.profile
    }

    /// The daemon behind the harness.
    pub fn daemon(&self) -> &HarnessDaemon {
        self.daemon
            .as_ref()
            .expect("the daemon is running until shutdown")
    }

    /// The address the daemon listens on.
    pub fn addr(&self) -> SocketAddr {
        self.daemon().addr()
    }

    /// The bridge endpoint of the daemon.
    pub fn bridge_url(&self) -> String {
        self.daemon().bridge_url()
    }

    /// The client endpoint of the daemon.
    pub fn client_url(&self) -> String {
        self.daemon().client_url()
    }

    /// The instance identity of the daemon run.
    pub fn instance_id(&self) -> String {
        self.daemon().instance_id()
    }

    /// The credential a local client uses on the client endpoint.
    pub fn client_credential(&self) -> String {
        self.daemon().client_credential()
    }

    /// Wait until the daemon answers on its health endpoint.
    pub async fn wait_ready(&self, timeout: Duration) -> Result<(), HarnessError> {
        let addr = self.addr();
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if probe_health(addr).await.is_ok() {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(HarnessError::Timeout(format!(
                    "the daemon at {addr} did not become ready within {timeout:?}"
                )));
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// Stop the daemon and wait for the listener to close.
    pub async fn shutdown(mut self) -> Result<(), HarnessError> {
        if let Some(daemon) = self.daemon.take() {
            daemon.shutdown().await?;
        }
        Ok(())
    }
}

impl Drop for DaemonHarness {
    fn drop(&mut self) {
        if let Some(daemon) = self.daemon.take() {
            // The daemon stops itself when it is dropped, so a failed test
            // never leaves a listener behind.
            drop(daemon);
        }
    }
}

/// Send one health request and report whether the daemon answered.
pub async fn probe_health(addr: SocketAddr) -> Result<(), HarnessError> {
    let mut stream = TcpStream::connect(addr).await?;
    let request = format!("GET /health HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await?;
    let mut buffer = Vec::new();
    stream.read_to_end(&mut buffer).await?;
    let text = String::from_utf8_lossy(&buffer);
    if text.contains("200 OK") {
        Ok(())
    } else {
        Err(HarnessError::Protocol(
            "the health endpoint did not answer".to_string(),
        ))
    }
}

/// Assert that an address is a loopback address.
pub fn assert_loopback_address(addr: SocketAddr) -> Result<(), String> {
    if addr.ip().is_loopback() {
        Ok(())
    } else {
        Err(format!(
            "the daemon listens on {addr}, which is not a loopback address"
        ))
    }
}

/// Assert that a port is not reachable on a routable address of this machine.
///
/// The daemon binds `127.0.0.1`, so a connection to the routable address of
/// the same machine must not be answered. When the machine has no routable
/// address the probe is skipped and only the binding check applies. A
/// connection that is refused and a connection that never completes both
/// prove the same thing: nothing answers on the routable address.
pub async fn assert_loopback_only(port: u16) -> Result<(), String> {
    let Some(ip) = non_loopback_ipv4() else {
        return Ok(());
    };
    let addr = SocketAddr::from((ip, port));
    match tokio::time::timeout(Duration::from_millis(1500), TcpStream::connect(addr)).await {
        Ok(Ok(_stream)) => Err(format!(
            "the daemon answered on the non loopback address {addr}"
        )),
        Ok(Err(_)) => Ok(()),
        Err(_) => Ok(()),
    }
}

/// The routable IPv4 address of this machine, when it has one.
///
/// The probe binds a datagram socket and connects it to a documentation
/// address from RFC 5737. No packet leaves the machine, the kernel only
/// resolves the route that a connection would take.
pub fn non_loopback_ipv4() -> Option<Ipv4Addr> {
    let socket = std::net::UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    let local = socket.local_addr().ok()?;
    match local.ip() {
        IpAddr::V4(address) if !address.is_loopback() && !address.is_unspecified() => Some(address),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_daemon_binds_loopback_only() {
        let harness = DaemonHarness::start().await.expect("the daemon starts");
        let addr = harness.addr();
        assert_loopback_address(addr).expect("the daemon binds loopback");
        assert_loopback_only(addr.port())
            .await
            .expect("no routable listener");
        harness.shutdown().await.expect("the daemon stops");
    }
}
