//! FigTether MCP adapter crate.
//!
//! The adapter exposes the tool surface to MCP clients over stdio. Twelve of
//! the tools are read only: they never mutate a document and never widen a
//! grant. Three of them are the write path: an agent proposes a plan, the
//! user approves it in the plugin, and only then is the plan applied.
//!
//! The adapter is a client of the local daemon. It opens one authenticated
//! loopback websocket to the daemon, sends the client credential from the
//! owner only runtime directory, and translates every tool call into a
//! request of the client protocol. The answer of the daemon is shaped into
//! the common response envelope, or into a structured error.
//!
//! Rules that hold for the whole crate:
//!
//! - stdout carries the MCP protocol only, every log line goes to stderr;
//! - a business failure is a structured tool result, never a protocol error
//!   and never a stack trace;
//! - a cancelled MCP request cancels the daemon side job;
//! - a daemon that is not running is reported on stderr and stops the
//!   process instead of hanging.
//!
//! The adapter logs through `tracing`. The process that embeds it owns the
//! subscriber and has to send it to stderr: stdout belongs to the MCP
//! protocol alone.

mod daemon;
mod endpoint;
mod error;
mod result;
mod runtime;
mod tools;
mod tools_canvas;
mod tools_write;
mod ws;

#[cfg(test)]
mod fake_daemon;
#[cfg(test)]
mod handshake;

use std::path::PathBuf;
use std::time::Duration;

pub use daemon::DaemonClient;
pub use endpoint::Endpoint;
pub use error::McpError;
pub use runtime::{DaemonRuntime, PlatformDirs, RUNTIME_FILE_NAME};
pub use tools::{
    tool_router, CancelSource, FigTetherService, READ_TOOL_NAMES, TOOL_NAMES, WRITE_TOOL_NAMES,
};
pub use tools_write::{
    ApplyWriteParams, GetWriteApprovalParams, PreviewWriteParams, OPERATION_KINDS,
};

/// Version of the MCP adapter.
pub const MCP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The builder of the MCP server.
///
/// The builder exists so that the command line and the tests can start the
/// adapter against a known daemon endpoint instead of the one the runtime
/// file names.
#[derive(Debug, Clone)]
pub struct McpServerBuilder {
    endpoint: Option<Endpoint>,
    credential: Option<(String, String)>,
    runtime_dir: Option<PathBuf>,
    connect_timeout: Duration,
    auth_timeout: Duration,
}

impl Default for McpServerBuilder {
    fn default() -> Self {
        Self {
            endpoint: None,
            credential: None,
            runtime_dir: None,
            connect_timeout: Duration::from_secs(5),
            auth_timeout: Duration::from_secs(2),
        }
    }
}

impl McpServerBuilder {
    /// A builder with the defaults of the product.
    pub fn new() -> Self {
        Self::default()
    }

    /// Use an explicit daemon endpoint instead of the one in the runtime file.
    pub fn with_endpoint(mut self, endpoint: Endpoint) -> Self {
        self.endpoint = Some(endpoint);
        self
    }

    /// Use an explicit daemon endpoint given as a URL.
    pub fn with_endpoint_url(mut self, url: &str) -> Result<Self, McpError> {
        self.endpoint = Some(Endpoint::parse(url)?);
        Ok(self)
    }

    /// Use an explicit client credential instead of the one in the runtime file.
    pub fn with_credential(
        mut self,
        credential: impl Into<String>,
        client_id: impl Into<String>,
    ) -> Self {
        self.credential = Some((credential.into(), client_id.into()));
        self
    }

    /// Use an explicit runtime directory instead of the platform one.
    pub fn with_runtime_dir(mut self, directory: impl Into<PathBuf>) -> Self {
        self.runtime_dir = Some(directory.into());
        self
    }

    /// Set the timeout of the connection to the daemon.
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Set how long the adapter waits for the answer to the authentication
    /// frame of the daemon.
    pub fn with_auth_timeout(mut self, timeout: Duration) -> Self {
        self.auth_timeout = timeout;
        self
    }

    /// Build the server.
    ///
    /// The build reads the runtime file of the daemon unless an explicit
    /// credential was given. It never opens a connection: the connection is
    /// made when the server starts serving.
    pub fn build(self) -> Result<McpServer, McpError> {
        let runtime = match self.credential {
            // An explicit credential replaces the credential of the runtime file.
            Some(_) => None,
            None => Some(self.read_runtime()?),
        };
        let auth = match (self.credential, runtime.as_ref()) {
            (Some((credential, client_id)), _) => figtether_protocol::client::ClientAuth {
                credential,
                client_id,
                protocol_version: figtether_protocol::PROTOCOL_VERSION.to_string(),
            },
            (None, Some(runtime)) => runtime.client_auth(),
            (None, None) => {
                return Err(McpError::RuntimeDirectory(
                    "the client credential is unknown".to_string(),
                ));
            }
        };
        let endpoint = match self.endpoint {
            Some(endpoint) => endpoint,
            None => match runtime.as_ref() {
                Some(runtime) => runtime.endpoint(),
                None => Endpoint::new(
                    "127.0.0.1",
                    figtether_core::DEFAULT_BRIDGE_PORT,
                    endpoint::DEFAULT_CLIENT_PATH,
                ),
            },
        };
        let client = DaemonClient::new(endpoint, auth)
            .with_connect_timeout(self.connect_timeout)
            .with_auth_timeout(self.auth_timeout);
        Ok(McpServer {
            service: FigTetherService::new(client),
        })
    }

    /// Read the runtime file of the daemon.
    fn read_runtime(&self) -> Result<DaemonRuntime, McpError> {
        let directory = match &self.runtime_dir {
            Some(directory) => directory.clone(),
            None => runtime::runtime_dir()?,
        };
        runtime::read_runtime_file(&directory)
    }
}

/// The MCP server of the FigTether tool surface.
pub struct McpServer {
    service: FigTetherService,
}

impl McpServer {
    /// A builder for the server.
    pub fn builder() -> McpServerBuilder {
        McpServerBuilder::new()
    }

    /// The service behind the tools.
    pub fn service(&self) -> &FigTetherService {
        &self.service
    }

    /// Connect to the daemon and serve the surface over stdio.
    ///
    /// The call fails when the daemon is not reachable. The failure is
    /// reported on stderr before it is returned, so that the process can stop
    /// with a non zero exit code and a clear line.
    pub async fn run_stdio(self) -> Result<(), McpError> {
        if let Err(error) = self.service.client().connect().await {
            report_startup_failure(&error);
            return Err(error);
        }
        let (reader, writer) = rmcp::transport::io::stdio();
        self.serve((reader, writer)).await
    }

    /// Serve the surface over an arbitrary transport.
    pub async fn serve<T, E, A>(self, transport: T) -> Result<(), McpError>
    where
        T: rmcp::transport::IntoTransport<rmcp::RoleServer, E, A>,
        E: std::error::Error + Send + Sync + 'static,
    {
        use rmcp::ServiceExt;
        let running = self
            .service
            .serve(transport)
            .await
            .map_err(|error| McpError::ServerStopped(error.to_string()))?;
        running
            .waiting()
            .await
            .map_err(|error| McpError::ServerStopped(error.to_string()))?;
        Ok(())
    }
}

/// Run the MCP server over stdio.
///
/// This is the entry point of the `figtether mcp` command. The call blocks
/// until the MCP client closes the connection and returns an error when the
/// daemon is not reachable. It must not be called from inside a tokio
/// runtime; use [`run_stdio_async`] there.
pub fn run_stdio() -> Result<(), McpError> {
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err(McpError::ServerStopped(
            "run_stdio must not be called from inside a tokio runtime, use run_stdio_async"
                .to_string(),
        ));
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| McpError::ServerStopped(error.to_string()))?;
    runtime.block_on(run_stdio_async())
}

/// The asynchronous form of [`run_stdio`].
pub async fn run_stdio_async() -> Result<(), McpError> {
    match McpServer::builder().build() {
        Ok(server) => server.run_stdio().await,
        Err(error) => {
            report_startup_failure(&error);
            Err(error)
        }
    }
}

/// Report a failure of the startup on stderr.
///
/// stdout carries the MCP protocol only, so every diagnostic of the adapter
/// goes to stderr.
fn report_startup_failure(error: &McpError) {
    eprintln!("figtether mcp: {error}");
    eprintln!(
        "figtether mcp: start the local daemon with `figtether daemon`, then run `figtether doctor`"
    );
}

/// The correlation id of a request of the client protocol.
pub fn request_id(request: &figtether_protocol::client::ClientRequest) -> &str {
    daemon::request_id(request)
}

/// The correlation id of a response of the client protocol.
pub fn response_request_id(response: &figtether_protocol::client::ClientResponse) -> &str {
    daemon::response_request_id(response)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime_directory(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("figtether-mcp-{name}-{}", std::process::id()))
    }

    #[test]
    fn the_builder_reads_the_credential_and_the_port_of_the_runtime_file() {
        let directory = runtime_directory("runtime");
        std::fs::create_dir_all(&directory).expect("the test directory is created");
        std::fs::write(
            directory.join(runtime::RUNTIME_FILE_NAME),
            r#"{"port": 38947, "credential": "s3cret", "client_id": "cli"}"#,
        )
        .expect("the runtime file is written");
        let server = McpServer::builder()
            .with_runtime_dir(&directory)
            .build()
            .expect("the server is built");
        assert_eq!(server.service().client().endpoint().port(), 38947);
        assert_eq!(
            server.service().client().endpoint().url(),
            "ws://127.0.0.1:38947/client"
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_missing_runtime_file_fails_with_a_clear_error() {
        let error = match McpServer::builder()
            .with_runtime_dir(runtime_directory("missing"))
            .build()
        {
            Ok(_) => panic!("the runtime file is missing"),
            Err(error) => error,
        };
        assert!(matches!(error, McpError::RuntimeFileUnreadable { .. }));
    }

    #[tokio::test]
    async fn the_stdio_entry_point_refuses_to_start_without_a_daemon() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("a free port");
        let port = listener.local_addr().expect("an address").port();
        drop(listener);
        let server = McpServer::builder()
            .with_credential("s3cret", "test-client")
            .with_endpoint(Endpoint::new("127.0.0.1", port, "/client"))
            .with_connect_timeout(Duration::from_millis(300))
            .build()
            .expect("the server is built");
        let error = server
            .run_stdio()
            .await
            .expect_err("the daemon is not running");
        assert!(matches!(error, McpError::DaemonUnavailable { .. }));
    }
}
