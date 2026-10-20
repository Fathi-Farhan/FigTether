//! The loopback HTTP surface of the daemon.
//!
//! The daemon binds `127.0.0.1` only. Four endpoints exist:
//!
//! | Route              | Method | Purpose                                             |
//! | ------------------ | ------ | --------------------------------------------------- |
//! | `/bridge`          | GET    | WebSocket for the Figma plugin                       |
//! | `/client`          | GET    | WebSocket for the local clients                      |
//! | `/health`          | GET    | Sanitized status as JSON                             |
//! | `/control/pair`    | POST   | Mint a one time pairing secret for the plugin UI     |
//! | `/control/shutdown` | POST  | Ask the daemon to stop gracefully                    |
//!
//! Every request is checked before the socket is upgraded: the `Host` header
//! has to name the loopback interface, and the `Origin` header has to be one
//! of the admitted origins. No wildcard origin is accepted.

use std::io;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::extract::{ConnectInfo, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use tokio::net::TcpListener;

use crate::bridge;
use crate::client_api;
use crate::control;
use crate::logging::LoggingError;
use crate::runtime_dir::RuntimeDirError;
use crate::DaemonState;

/// Failures of the daemon.
#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    /// The port must not be zero.
    #[error("the bridge port must not be zero")]
    InvalidPort,
    /// The listener could not be opened.
    #[error("the loopback listener could not be opened: {0}")]
    Bind(#[source] io::Error),
    /// The server stopped with an error.
    #[error("the daemon stopped with an error: {0}")]
    Serve(#[source] io::Error),
    /// The runtime directory is not usable.
    #[error("the runtime directory is not usable: {0}")]
    Runtime(#[source] RuntimeDirError),
    /// The logging setup failed.
    #[error("the logging setup failed: {0}")]
    Logging(#[source] LoggingError),
}

/// Bind the loopback listener.
///
/// The daemon never binds `0.0.0.0` and never binds an arbitrary port: the
/// plugin manifest references a fixed port.
pub async fn bind_listener(port: u16) -> Result<TcpListener, DaemonError> {
    if port == 0 {
        return Err(DaemonError::InvalidPort);
    }
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    TcpListener::bind(addr).await.map_err(DaemonError::Bind)
}

/// The address the daemon listens on.
pub fn bind_addr(port: u16) -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, port))
}

/// Build the router of the daemon.
pub fn router(state: Arc<DaemonState>) -> Router {
    Router::new()
        .route("/bridge", get(bridge_handler))
        .route("/client", get(client_handler))
        .route("/health", get(health_handler))
        .route("/control/pair", post(pair_handler))
        .route("/control/shutdown", post(shutdown_handler))
        .layer(middleware::from_fn_with_state(state.clone(), guard_headers))
        .with_state(state)
}

/// Reject requests that do not come from the loopback interface.
async fn guard_headers(
    State(state): State<Arc<DaemonState>>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let host_ok = host_is_allowed(request.headers(), state.port);
    let origin_ok = origin_is_allowed(request.headers());
    if !host_ok || !origin_ok {
        tracing::warn!(
            code = "PERMISSION_DENIED",
            "request refused before the upgrade"
        );
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "code": "PERMISSION_DENIED" })),
        )
            .into_response();
    }
    next.run(request).await
}

/// Whether the `Host` header names the loopback interface.
fn host_is_allowed(headers: &axum::http::HeaderMap, port: u16) -> bool {
    let Some(host) = headers
        .get(axum::http::header::HOST)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let host = host.trim();
    let allowed = ["127.0.0.1", "localhost", "[::1]"];
    for name in allowed {
        if host == name {
            return true;
        }
        if let Some(port_text) = host
            .strip_prefix(name)
            .and_then(|rest| rest.strip_prefix(':'))
        {
            return port_text.parse::<u16>().is_ok_and(|value| value == port);
        }
    }
    false
}

/// Whether the `Origin` header is admitted.
///
/// A missing origin belongs to a non browser client and is admitted. A
/// browser origin has to be one of the known application origins. No
/// wildcard is accepted.
fn origin_is_allowed(headers: &axum::http::HeaderMap) -> bool {
    let Some(origin) = headers
        .get(axum::http::header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    else {
        return true;
    };
    let origin = origin.trim();
    if origin == "null" {
        return true;
    }
    const ALLOWED: [&str; 4] = [
        "https://www.figma.com",
        "https://figma.com",
        "http://127.0.0.1",
        "http://localhost",
    ];
    ALLOWED.iter().any(|allowed| {
        origin == *allowed
            || origin
                .strip_prefix(allowed)
                .is_some_and(|rest| rest.starts_with(':') || rest.starts_with('/'))
    })
}

/// The plugin endpoint.
async fn bridge_handler(
    State(state): State<Arc<DaemonState>>,
    upgrade: axum::extract::ws::WebSocketUpgrade,
) -> Response {
    upgrade.on_upgrade(move |socket| bridge::serve_plugin(socket, state))
}

/// The local client endpoint.
async fn client_handler(
    State(state): State<Arc<DaemonState>>,
    upgrade: axum::extract::ws::WebSocketUpgrade,
) -> Response {
    upgrade.on_upgrade(move |socket| client_api::serve_client(socket, state))
}

/// The sanitized status endpoint.
async fn health_handler(State(state): State<Arc<DaemonState>>) -> Response {
    let status = control::status(&state);
    (StatusCode::OK, Json(status)).into_response()
}

/// The pairing endpoint.
async fn pair_handler(
    State(state): State<Arc<DaemonState>>,
    peer: ConnectInfo<SocketAddr>,
) -> Response {
    if !peer_is_loopback(Some(peer)) {
        tracing::warn!(
            code = "PERMISSION_DENIED",
            "pairing refused for a remote peer"
        );
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "code": "PERMISSION_DENIED" })),
        )
            .into_response();
    }
    let pair = control::mint_pairing_secret(&state);
    (StatusCode::OK, Json(pair)).into_response()
}

/// The graceful shutdown endpoint.
async fn shutdown_handler(
    State(state): State<Arc<DaemonState>>,
    peer: ConnectInfo<SocketAddr>,
) -> Response {
    if !peer_is_loopback(Some(peer)) {
        tracing::warn!(
            code = "PERMISSION_DENIED",
            "shutdown refused for a remote peer"
        );
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "code": "PERMISSION_DENIED" })),
        )
            .into_response();
    }
    state.request_shutdown();
    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({ "stopping": true })),
    )
        .into_response()
}

/// Whether the peer of a request is on the loopback interface.
fn peer_is_loopback(peer: Option<ConnectInfo<SocketAddr>>) -> bool {
    match peer {
        Some(ConnectInfo(addr)) => addr.ip().is_loopback(),
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn the_bind_address_is_loopback_only() {
        let addr = bind_addr(38947);
        assert!(addr.ip().is_loopback());
        assert!(!addr.ip().is_unspecified());
        assert_eq!(addr.port(), 38947);
    }

    #[tokio::test]
    async fn the_listener_refuses_port_zero() {
        let err = bind_listener(0).await.expect_err("port zero is refused");
        assert!(matches!(err, DaemonError::InvalidPort));
    }

    #[tokio::test]
    async fn a_bound_listener_reports_a_loopback_address() {
        let listener = tokio::net::TcpListener::bind(bind_addr(0))
            .await
            .expect("bound");
        let addr = listener.local_addr().expect("address");
        assert!(addr.ip().is_loopback());
        assert!(!addr.ip().is_unspecified());
    }

    #[test]
    fn only_loopback_hosts_are_admitted() {
        let mut headers = HeaderMap::new();
        headers.insert("host", "127.0.0.1:38947".parse().expect("header"));
        assert!(host_is_allowed(&headers, 38947));

        headers.insert("host", "localhost:38947".parse().expect("header"));
        assert!(host_is_allowed(&headers, 38947));

        headers.insert("host", "evil.example:38947".parse().expect("header"));
        assert!(!host_is_allowed(&headers, 38947));

        headers.insert("host", "127.0.0.1:1234".parse().expect("header"));
        assert!(!host_is_allowed(&headers, 38947));

        assert!(!host_is_allowed(&HeaderMap::new(), 38947));
    }

    #[test]
    fn only_known_origins_are_admitted() {
        let mut headers = HeaderMap::new();
        assert!(
            origin_is_allowed(&headers),
            "a client without origin is admitted"
        );

        headers.insert("origin", "null".parse().expect("header"));
        assert!(origin_is_allowed(&headers));

        headers.insert("origin", "https://www.figma.com".parse().expect("header"));
        assert!(origin_is_allowed(&headers));

        headers.insert("origin", "https://evil.example".parse().expect("header"));
        assert!(!origin_is_allowed(&headers));

        headers.insert("origin", "*".parse().expect("header"));
        assert!(!origin_is_allowed(&headers));
    }

    #[test]
    fn a_remote_peer_is_refused() {
        let remote = ConnectInfo(SocketAddr::from(([203, 0, 113, 5], 40000)));
        assert!(!peer_is_loopback(Some(remote)));
        let local = ConnectInfo(SocketAddr::from((Ipv4Addr::LOCALHOST, 40000)));
        assert!(peer_is_loopback(Some(local)));
    }
}
