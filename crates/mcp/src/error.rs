//! Failures of the adapter outside the tool lane.
//!
//! The adapter separates two kinds of failure:
//!
//! - a business failure of a read (the daemon refused the request, the plugin
//!   is offline, the node is out of scope) travels as a structured tool
//!   result, never as an error of this type;
//! - a failure of the adapter itself (the runtime file is unreadable, the
//!   daemon is not listening, the MCP transport broke) is reported by this
//!   type. The CLI turns it into one clear stderr line and a non zero exit
//!   code.

use std::path::PathBuf;
use std::time::Duration;

use figtether_model::errors::FigError;

/// Everything that can go wrong in the adapter outside the tool lane.
#[derive(Debug, thiserror::Error)]
pub enum McpError {
    /// The runtime directory of the daemon could not be resolved.
    #[error("the FigTether runtime directory could not be resolved: {0}")]
    RuntimeDirectory(String),

    /// The daemon runtime file could not be read.
    #[error("the daemon runtime file {path} could not be read: {source}")]
    RuntimeFileUnreadable {
        /// Path of the runtime file.
        path: PathBuf,
        /// The underlying I/O failure.
        #[source]
        source: std::io::Error,
    },

    /// The daemon runtime file does not describe a usable daemon.
    #[error("the daemon runtime file {path} is not usable: {source}")]
    RuntimeFileInvalid {
        /// Path of the runtime file.
        path: PathBuf,
        /// The underlying parse failure.
        #[source]
        source: crate::runtime::RuntimeError,
    },

    /// The endpoint is not a usable daemon URL.
    #[error("{0} is not a usable daemon endpoint")]
    InvalidEndpoint(String),

    /// The daemon is not listening.
    #[error("the daemon at {endpoint} is not reachable: {reason}")]
    DaemonUnavailable {
        /// The endpoint that was tried.
        endpoint: String,
        /// Why the connection failed.
        reason: String,
    },

    /// The daemon refused the websocket handshake.
    #[error("the daemon at {endpoint} refused the connection: {reason}")]
    DaemonHandshake {
        /// The endpoint that was tried.
        endpoint: String,
        /// Why the handshake failed.
        reason: String,
    },

    /// The connection to the daemon broke while a request was running.
    #[error("the connection to the daemon at {endpoint} failed: {reason}")]
    DaemonConnection {
        /// The endpoint that was used.
        endpoint: String,
        /// Why the connection failed.
        reason: String,
    },

    /// The daemon answered with something that is not a protocol frame.
    #[error("the daemon answer is not a valid frame: {0}")]
    DaemonProtocol(String),

    /// The daemon did not answer in time.
    #[error("the daemon did not answer within {0:?}")]
    DaemonTimeout(Duration),

    /// The daemon refused the client credential.
    #[error("the daemon refused the client credential: {0}")]
    DaemonRejected(FigError),

    /// The MCP client cancelled the request.
    #[error("the request was cancelled")]
    Cancelled,

    /// The MCP transport failed.
    #[error("the MCP transport failed: {0}")]
    Transport(String),

    /// The MCP server stopped.
    #[error("the MCP server stopped: {0}")]
    ServerStopped(String),
}
