//! The endpoint of the local daemon.
//!
//! The adapter is a client of the daemon. The daemon listens on the loopback
//! interface only and serves the client surface on `/client`. The endpoint is
//! never a tool parameter and never reaches an agent: it is read from the
//! owner only runtime directory and stays inside the adapter.

use crate::error::McpError;

/// Default path of the client surface of the local daemon.
pub const DEFAULT_CLIENT_PATH: &str = "/client";

/// Default port of the local daemon.
pub const DEFAULT_PORT: u16 = figtether_core::DEFAULT_BRIDGE_PORT;

/// The websocket endpoint of the local daemon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    host: String,
    port: u16,
    path: String,
}

impl Endpoint {
    /// Parse a `ws://host:port/path` URL.
    ///
    /// The port is optional and defaults to the bridge port of the core
    /// crate. The path is optional and defaults to the client surface of the
    /// daemon.
    pub fn parse(raw: &str) -> Result<Self, McpError> {
        let trimmed = raw.trim();
        let invalid = || McpError::InvalidEndpoint(raw.to_string());
        let rest = trimmed
            .strip_prefix("ws://")
            .or_else(|| trimmed.strip_prefix("WS://"))
            .ok_or_else(invalid)?;
        let (authority, path) = match rest.find('/') {
            Some(index) => (&rest[..index], &rest[index..]),
            None => (rest, ""),
        };
        if authority.is_empty() {
            return Err(invalid());
        }
        let (host, port) = if let Some(literal) = authority.strip_prefix('[') {
            // An IPv6 literal keeps its brackets in the authority.
            let (host, tail) = literal.split_once(']').ok_or_else(invalid)?;
            let port = match tail.strip_prefix(':') {
                Some(port) => port.parse::<u16>().map_err(|_| invalid())?,
                None => DEFAULT_PORT,
            };
            (format!("[{host}]"), port)
        } else {
            match authority.rsplit_once(':') {
                Some((host, port)) => (
                    host.to_string(),
                    port.parse::<u16>().map_err(|_| invalid())?,
                ),
                None => (authority.to_string(), DEFAULT_PORT),
            }
        };
        if host.is_empty() {
            return Err(invalid());
        }
        let path = if path.is_empty() {
            DEFAULT_CLIENT_PATH.to_string()
        } else {
            path.to_string()
        };
        Ok(Self { host, port, path })
    }

    /// Build an endpoint from its parts.
    pub fn new(host: impl Into<String>, port: u16, path: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            port,
            path: path.into(),
        }
    }

    /// The host of the daemon.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The port of the daemon.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The path of the client surface.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The authority used in the websocket handshake.
    pub fn authority(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// The full URL of the endpoint.
    pub fn url(&self) -> String {
        format!("ws://{}:{}{}", self.host, self.port, self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_url_is_parsed() {
        let endpoint = Endpoint::parse("ws://127.0.0.1:38947/client").expect("parses");
        assert_eq!(endpoint.host(), "127.0.0.1");
        assert_eq!(endpoint.port(), 38947);
        assert_eq!(endpoint.path(), "/client");
        assert_eq!(endpoint.authority(), "127.0.0.1:38947");
        assert_eq!(endpoint.url(), "ws://127.0.0.1:38947/client");
    }

    #[test]
    fn the_port_and_the_path_are_optional() {
        let endpoint = Endpoint::parse("ws://127.0.0.1").expect("parses");
        assert_eq!(endpoint.port(), DEFAULT_PORT);
        assert_eq!(endpoint.path(), DEFAULT_CLIENT_PATH);
    }

    #[test]
    fn an_ipv6_literal_keeps_its_brackets() {
        let endpoint = Endpoint::parse("ws://[::1]:38947/client").expect("parses");
        assert_eq!(endpoint.host(), "[::1]");
        assert_eq!(endpoint.port(), 38947);
    }

    #[test]
    fn a_broken_url_is_refused() {
        assert!(Endpoint::parse("http://127.0.0.1:38947/client").is_err());
        assert!(Endpoint::parse("ws://127.0.0.1:not-a-port/client").is_err());
        assert!(Endpoint::parse("ws://").is_err());
        assert!(Endpoint::parse("").is_err());
    }
}
