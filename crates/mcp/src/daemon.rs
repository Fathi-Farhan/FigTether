//! The client of the local daemon.
//!
//! The adapter is a client of the daemon and nothing else. It opens one
//! authenticated loopback websocket, sends [`ClientAuth`] as its first frame,
//! and then exchanges [`ClientRequest`] and [`ClientResponse`] frames that
//! are correlated by the request id.
//!
//! The adapter never grants access itself: it forwards reads and write plans.
//! The daemon checks the grant, the epochs, and the per node
//! membership on its side.

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use figtether_model::ids::JobId;
use figtether_protocol::client::{ClientAuth, ClientRequest, ClientResponse};
use tokio::sync::{oneshot, Mutex};

use crate::endpoint::Endpoint;
use crate::error::McpError;
use crate::ws::{WsConnection, WsSender};

/// Correlation id of the authentication frame.
const AUTH_CORRELATION_ID: &str = "auth";

/// The correlation id of a request.
pub fn request_id(request: &ClientRequest) -> &str {
    match request {
        ClientRequest::Status { request_id, .. }
        | ClientRequest::PairSecret { request_id, .. }
        | ClientRequest::ListSessions { request_id, .. }
        | ClientRequest::ListNodes { request_id, .. }
        | ClientRequest::GetNodes { request_id, .. }
        | ClientRequest::InspectDesign { request_id, .. }
        | ClientRequest::GetDesignTokens { request_id, .. }
        | ClientRequest::ExportAssets { request_id, .. }
        | ClientRequest::GetAsset { request_id, .. }
        | ClientRequest::GetSelection { request_id, .. }
        | ClientRequest::GetSnapshotInfo { request_id, .. }
        | ClientRequest::WatchScope { request_id, .. }
        | ClientRequest::GetChanges { request_id, .. }
        | ClientRequest::GetJobStatus { request_id, .. }
        | ClientRequest::PreviewWrite { request_id, .. }
        | ClientRequest::ApplyWrite { request_id, .. }
        | ClientRequest::GetWriteApproval { request_id, .. }
        | ClientRequest::Cancel { request_id, .. }
        | ClientRequest::Revoke { request_id, .. } => request_id,
    }
}

/// The correlation id of a response.
pub fn response_request_id(response: &ClientResponse) -> &str {
    match response {
        ClientResponse::Ok { request_id, .. }
        | ClientResponse::Error { request_id, .. }
        | ClientResponse::Accepted { request_id, .. } => request_id,
    }
}

#[derive(Default)]
struct DaemonState {
    sender: Option<WsSender>,
    pending: HashMap<String, oneshot::Sender<Result<ClientResponse, McpError>>>,
}

/// A client of the local daemon.
#[derive(Clone)]
pub struct DaemonClient {
    endpoint: Endpoint,
    auth: ClientAuth,
    state: Arc<Mutex<DaemonState>>,
    connect_timeout: Duration,
    auth_timeout: Duration,
}

impl DaemonClient {
    /// Build a client for one daemon endpoint.
    pub fn new(endpoint: Endpoint, auth: ClientAuth) -> Self {
        Self {
            endpoint,
            auth,
            state: Arc::new(Mutex::new(DaemonState::default())),
            connect_timeout: Duration::from_secs(5),
            auth_timeout: Duration::from_secs(2),
        }
    }

    /// Set the timeout of the connection and of the opening handshake.
    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Set how long the adapter waits for the optional answer to the
    /// authentication frame.
    ///
    /// A daemon that does not answer the authentication frame at all is
    /// accepted; the first request then carries the verdict of the daemon.
    pub fn with_auth_timeout(mut self, timeout: Duration) -> Self {
        self.auth_timeout = timeout;
        self
    }

    /// The endpoint of the daemon.
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Open the connection to the daemon and authenticate.
    ///
    /// The call fails with a clear error when the daemon is not listening, so
    /// that the process can report the problem on stderr and stop instead of
    /// hanging.
    pub async fn connect(&self) -> Result<(), McpError> {
        let mut state = self.state.lock().await;
        if state.sender.is_some() {
            return Ok(());
        }
        let connection = WsConnection::connect(&self.endpoint, self.connect_timeout).await?;
        let sender = connection.sender();
        let auth = serde_json::to_string(&self.auth)
            .map_err(|error| McpError::DaemonProtocol(error.to_string()))?;
        let (waiter, answer) = oneshot::channel();
        state
            .pending
            .insert(AUTH_CORRELATION_ID.to_string(), waiter);
        if let Err(error) = sender.send_text(&auth).await {
            state.pending.remove(AUTH_CORRELATION_ID);
            return Err(error);
        }
        let reader_state = Arc::clone(&self.state);
        let endpoint = self.endpoint.clone();
        tokio::spawn(async move {
            read_loop(connection, reader_state, endpoint).await;
        });
        state.sender = Some(sender);
        drop(state);

        match tokio::time::timeout(self.auth_timeout, answer).await {
            Ok(Ok(Ok(response))) => match response {
                ClientResponse::Error { error, .. } => Err(McpError::DaemonRejected(error)),
                _ => Ok(()),
            },
            Ok(Ok(Err(error))) => Err(error),
            Ok(Err(_)) => Err(McpError::DaemonConnection {
                endpoint: self.endpoint.url(),
                reason: "the daemon closed the connection during the handshake".to_string(),
            }),
            Err(_) => {
                self.state.lock().await.pending.remove(AUTH_CORRELATION_ID);
                Ok(())
            }
        }
    }

    /// Send one request to the daemon and wait for its answer.
    ///
    /// The call ends when the daemon answers, when the timeout expires, or
    /// when the MCP client cancels the request. A cancellation asks the
    /// daemon to cancel the job of the request, so that no work continues
    /// behind the back of the caller.
    pub async fn call<F>(
        &self,
        request: ClientRequest,
        job_id: JobId,
        timeout: Duration,
        cancelled: F,
    ) -> Result<ClientResponse, McpError>
    where
        F: Future<Output = ()> + Send,
    {
        let request_id = request_id(&request).to_string();
        self.ensure_connected().await?;
        let (waiter, answer) = oneshot::channel();
        {
            let mut state = self.state.lock().await;
            let sender = state
                .sender
                .clone()
                .ok_or_else(|| McpError::DaemonConnection {
                    endpoint: self.endpoint.url(),
                    reason: "the daemon connection is not open".to_string(),
                })?;
            state.pending.insert(request_id.clone(), waiter);
            let frame = serde_json::to_string(&request)
                .map_err(|error| McpError::DaemonProtocol(error.to_string()))?;
            if let Err(error) = sender.send_text(&frame).await {
                state.pending.remove(&request_id);
                return Err(error);
            }
        }
        tokio::select! {
            biased;
            () = cancelled => {
                self.state.lock().await.pending.remove(&request_id);
                let _ = self.cancel(job_id).await;
                Err(McpError::Cancelled)
            }
            answer = tokio::time::timeout(timeout, answer) => {
                match answer {
                    Ok(Ok(Ok(response))) => Ok(response),
                    Ok(Ok(Err(error))) => Err(error),
                    Ok(Err(_)) => Err(McpError::DaemonConnection {
                        endpoint: self.endpoint.url(),
                        reason: "the daemon closed the connection".to_string(),
                    }),
                    Err(_) => {
                        self.state.lock().await.pending.remove(&request_id);
                        let _ = self.cancel(job_id).await;
                        Err(McpError::DaemonTimeout(timeout))
                    }
                }
            }
        }
    }

    /// Ask the daemon to cancel a job.
    pub async fn cancel(&self, job_id: JobId) -> Result<(), McpError> {
        let request = ClientRequest::Cancel {
            request_id: JobId::new().to_string(),
            job_id,
        };
        let frame = serde_json::to_string(&request)
            .map_err(|error| McpError::DaemonProtocol(error.to_string()))?;
        let sender = { self.state.lock().await.sender.clone() };
        match sender {
            Some(sender) => sender.send_text(&frame).await,
            None => Ok(()),
        }
    }

    async fn ensure_connected(&self) -> Result<(), McpError> {
        let connected = { self.state.lock().await.sender.is_some() };
        if connected {
            return Ok(());
        }
        self.connect().await
    }
}

async fn read_loop(
    mut connection: WsConnection,
    state: Arc<Mutex<DaemonState>>,
    endpoint: Endpoint,
) {
    loop {
        let message = match connection.read_message().await {
            Ok(message) => message,
            Err(error) => {
                tracing::debug!(error = %error, "the daemon connection ended");
                break;
            }
        };
        match serde_json::from_str::<ClientResponse>(&message) {
            Ok(response) => {
                let id = response_request_id(&response).to_string();
                let waiter = { state.lock().await.pending.remove(&id) };
                match waiter {
                    Some(waiter) => {
                        let _ = waiter.send(Ok(response));
                    }
                    None => {
                        tracing::debug!(request_id = %id, "the daemon answer has no pending request");
                    }
                }
            }
            Err(error) => {
                tracing::warn!(error = %error, "the daemon answer could not be decoded");
            }
        }
    }
    let mut state = state.lock().await;
    state.sender = None;
    for (_, waiter) in state.pending.drain() {
        let _ = waiter.send(Err(McpError::DaemonConnection {
            endpoint: endpoint.url(),
            reason: "the daemon closed the connection".to_string(),
        }));
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use figtether_protocol::PROTOCOL_VERSION;

    use super::*;
    use crate::fake_daemon::FakeDaemon;

    fn client(port: u16) -> DaemonClient {
        DaemonClient::new(
            Endpoint::new("127.0.0.1", port, "/client"),
            ClientAuth {
                credential: "s3cret".to_string(),
                client_id: "test-client".to_string(),
                protocol_version: PROTOCOL_VERSION.to_string(),
            },
        )
        .with_connect_timeout(Duration::from_secs(5))
        .with_auth_timeout(Duration::from_millis(50))
    }

    #[tokio::test]
    async fn the_authentication_frame_is_the_first_frame() {
        let daemon = FakeDaemon::start(true).await;
        let client = client(daemon.port);
        client
            .connect()
            .await
            .expect("the daemon accepts the connection");
        let frames = daemon.wait_for_frames(1).await;
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0]["credential"], "s3cret");
        assert_eq!(frames[0]["client_id"], "test-client");
        assert_eq!(frames[0]["protocol_version"], PROTOCOL_VERSION);
    }

    #[tokio::test]
    async fn a_request_is_answered_with_the_envelope_of_the_daemon() {
        let daemon = FakeDaemon::start(true).await;
        let client = client(daemon.port);
        let request = ClientRequest::ListNodes {
            request_id: "req_1".to_string(),
            params: serde_json::json!({ "root_id": "1:2" }),
        };
        let response = client
            .call(
                request,
                JobId::new(),
                Duration::from_secs(5),
                std::future::pending(),
            )
            .await
            .expect("the daemon answers");
        let ClientResponse::Ok { envelope, .. } = response else {
            panic!("the daemon answers with an envelope");
        };
        assert_eq!(envelope.schema_version, "1.0");
        assert_eq!(envelope.data["nodes"], serde_json::json!([]));
        let frames = daemon.wait_for_frames(2).await;
        assert_eq!(frames[1]["type"], "list_nodes");
        assert_eq!(frames[1]["request_id"], "req_1");
        assert_eq!(frames[1]["params"]["root_id"], "1:2");
    }

    #[tokio::test]
    async fn a_cancelled_request_cancels_the_daemon_side_job() {
        let daemon = FakeDaemon::start(false).await;
        let client = client(daemon.port);
        let job_id = JobId::new();
        let request = ClientRequest::ListNodes {
            request_id: "req_2".to_string(),
            params: serde_json::json!({ "root_id": "1:2" }),
        };
        let (cancel, cancelled) = oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            client
                .call(request, job_id, Duration::from_secs(30), async move {
                    let _ = cancelled.await;
                })
                .await
        });
        let frames = daemon.wait_for_frames(2).await;
        assert_eq!(frames[1]["type"], "list_nodes");
        cancel.send(()).expect("the call is cancelled");
        let outcome = handle.await.expect("the call ends");
        assert!(matches!(outcome, Err(McpError::Cancelled)));
        let frames = daemon.wait_for_frames(3).await;
        assert_eq!(frames[2]["type"], "cancel");
        assert_eq!(frames[2]["job_id"], job_id.to_string());
    }

    #[tokio::test]
    async fn a_daemon_that_is_not_listening_fails_without_hanging() {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("a free port");
        let port = listener.local_addr().expect("an address").port();
        drop(listener);
        let client = client(port).with_connect_timeout(Duration::from_millis(500));
        let error = client.connect().await.expect_err("the daemon is not there");
        assert!(matches!(error, McpError::DaemonUnavailable { .. }));
    }
}
