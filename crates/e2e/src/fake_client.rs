//! The fake local client.
//!
//! The fake local client is what an MCP adapter or the CLI behaves like: it
//! speaks `figtether_protocol::client::ClientRequest` and receives
//! `ClientResponse` on the daemon client endpoint. Every request carries a
//! correlation id and the client refuses an answer that does not carry the
//! same id.

use std::time::Duration;

use figtether_model::envelope::Envelope;
use figtether_model::errors::FigError;
use figtether_model::ids::SessionId;
use figtether_model::tools::{GetNodesParams, ListNodesParams};
use figtether_protocol::client::{ClientAuth, ClientRequest, ClientResponse};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

use crate::harness::HarnessError;

/// How long the client waits for one answer.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

/// The fake local client.
pub struct FakeLocalClient {
    socket: WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    client_id: String,
    counter: std::sync::atomic::AtomicU64,
}

impl FakeLocalClient {
    /// Connect to the client endpoint and authenticate with the credential.
    pub async fn connect(
        url: &str,
        credential: &str,
        client_id: &str,
    ) -> Result<Self, HarnessError> {
        let (mut socket, _response) = connect_async(url)
            .await
            .map_err(|err| HarnessError::Connection(err.to_string()))?;
        let auth = ClientAuth {
            credential: credential.to_string(),
            client_id: client_id.to_string(),
            protocol_version: figtether_protocol::PROTOCOL_VERSION.to_string(),
        };
        let encoded =
            serde_json::to_string(&auth).map_err(|err| HarnessError::Protocol(err.to_string()))?;
        socket
            .send(Message::Text(encoded.into()))
            .await
            .map_err(|err| HarnessError::Connection(err.to_string()))?;
        // The daemon answers the authentication, so a refused credential is
        // visible here and not at the first request.
        let ack = tokio::time::timeout(Duration::from_secs(10), socket.next()).await;
        match ack {
            Ok(Some(Ok(Message::Text(_)))) => {}
            Ok(Some(Ok(Message::Close(Some(frame))))) => {
                return Err(HarnessError::Connection(format!(
                    "the daemon refused the credential: {}",
                    frame.reason
                )));
            }
            Ok(Some(Ok(Message::Close(None)))) => {
                return Err(HarnessError::Connection(
                    "the daemon refused the credential".to_string(),
                ));
            }
            Ok(Some(Ok(_))) => {
                return Err(HarnessError::Protocol(
                    "the daemon answered the authentication with an unexpected message".to_string(),
                ))
            }
            Ok(Some(Err(err))) => return Err(HarnessError::Connection(err.to_string())),
            Ok(None) => {
                return Err(HarnessError::Connection(
                    "the daemon closed the connection".to_string(),
                ))
            }
            Err(_) => {
                return Err(HarnessError::Timeout(
                    "the daemon did not answer the authentication in time".to_string(),
                ))
            }
        }
        Ok(Self {
            socket,
            client_id: client_id.to_string(),
            counter: std::sync::atomic::AtomicU64::new(0),
        })
    }

    /// The client id this client authenticated with.
    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    /// Mint a fresh correlation id.
    pub fn next_request_id(&self) -> String {
        let counter = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1;
        format!("client-req-{}-{}", self.client_id, counter)
    }

    /// Send a request and wait for the answer.
    pub async fn request(
        &mut self,
        request: ClientRequest,
    ) -> Result<ClientResponse, HarnessError> {
        let request_id = match &request {
            ClientRequest::Status { request_id }
            | ClientRequest::PairSecret { request_id }
            | ClientRequest::ListSessions { request_id, .. }
            | ClientRequest::ListNodes { request_id, .. }
            | ClientRequest::GetNodes { request_id, .. }
            | ClientRequest::InspectDesign { request_id, .. }
            | ClientRequest::GetDesignTokens { request_id, .. }
            | ClientRequest::ExportAssets { request_id, .. }
            | ClientRequest::GetSelection { request_id, .. }
            | ClientRequest::GetSnapshotInfo { request_id, .. }
            | ClientRequest::WatchScope { request_id, .. }
            | ClientRequest::GetChanges { request_id, .. }
            | ClientRequest::GetJobStatus { request_id, .. }
            | ClientRequest::Cancel { request_id, .. }
            | ClientRequest::Revoke { request_id, .. } => request_id.clone(),
        };
        self.send(request).await?;
        let response = self.receive().await?;
        if response_request_id(&response) != request_id {
            return Err(HarnessError::Protocol(format!(
                "the answer carries {} instead of {request_id}",
                response_request_id(&response)
            )));
        }
        Ok(response)
    }

    /// Send a request and unwrap the envelope of a successful answer.
    pub async fn envelope(&mut self, request: ClientRequest) -> Result<Envelope, HarnessError> {
        match self.request(request).await? {
            ClientResponse::Ok { envelope, .. } => Ok(envelope),
            ClientResponse::Error { error, .. } => Err(HarnessError::Other(error.to_string())),
            ClientResponse::Accepted { .. } => Err(HarnessError::Other(
                "the daemon accepted the request in the background".to_string(),
            )),
        }
    }

    /// Send a request and unwrap the error of a refused answer.
    pub async fn error(&mut self, request: ClientRequest) -> Result<FigError, HarnessError> {
        match self.request(request).await? {
            ClientResponse::Error { error, .. } => Ok(error),
            ClientResponse::Ok { .. } => Err(HarnessError::Other(
                "the daemon accepted a request that should have been refused".to_string(),
            )),
            ClientResponse::Accepted { .. } => Err(HarnessError::Other(
                "the daemon accepted the request in the background".to_string(),
            )),
        }
    }

    /// List the sessions of this client.
    pub async fn list_sessions(&mut self) -> Result<Vec<Value>, HarnessError> {
        let request_id = self.next_request_id();
        let envelope = self
            .envelope(ClientRequest::ListSessions {
                request_id,
                cursor: None,
            })
            .await?;
        let sessions = envelope
            .data
            .get("sessions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(sessions)
    }

    /// Ask the daemon for a pairing secret.
    pub async fn pair_secret(&mut self) -> Result<String, HarnessError> {
        let request_id = self.next_request_id();
        let envelope = self
            .envelope(ClientRequest::PairSecret { request_id })
            .await?;
        envelope
            .data
            .get("secret")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| HarnessError::Protocol("the daemon minted no secret".to_string()))
    }

    /// List the children of a root.
    pub async fn list_nodes(&mut self, params: ListNodesParams) -> Result<Envelope, HarnessError> {
        let request_id = self.next_request_id();
        let params =
            serde_json::to_value(params).map_err(|err| HarnessError::Protocol(err.to_string()))?;
        self.envelope(ClientRequest::ListNodes { request_id, params })
            .await
    }

    /// Read nodes by id.
    pub async fn get_nodes(&mut self, params: GetNodesParams) -> Result<Envelope, HarnessError> {
        let request_id = self.next_request_id();
        let params =
            serde_json::to_value(params).map_err(|err| HarnessError::Protocol(err.to_string()))?;
        self.envelope(ClientRequest::GetNodes { request_id, params })
            .await
    }

    /// Revoke the grant of a session.
    pub async fn revoke(&mut self, session_id: SessionId) -> Result<(), HarnessError> {
        let request_id = self.next_request_id();
        self.envelope(ClientRequest::Revoke {
            request_id,
            session_id,
        })
        .await?;
        Ok(())
    }

    async fn send(&mut self, request: ClientRequest) -> Result<(), HarnessError> {
        let encoded = serde_json::to_string(&request)
            .map_err(|err| HarnessError::Protocol(err.to_string()))?;
        self.socket
            .send(Message::Text(encoded.into()))
            .await
            .map_err(|err| HarnessError::Connection(err.to_string()))
    }

    async fn receive(&mut self) -> Result<ClientResponse, HarnessError> {
        let next = tokio::time::timeout(RESPONSE_TIMEOUT, self.socket.next()).await;
        let message = match next {
            Ok(Some(Ok(message))) => message,
            Ok(Some(Err(err))) => return Err(HarnessError::Connection(err.to_string())),
            Ok(None) => {
                return Err(HarnessError::Connection(
                    "the daemon closed the connection".to_string(),
                ))
            }
            Err(_) => {
                return Err(HarnessError::Timeout(
                    "the daemon did not answer in time".to_string(),
                ))
            }
        };
        let text = match message {
            Message::Text(text) => text.to_string(),
            Message::Binary(bytes) => String::from_utf8_lossy(&bytes).to_string(),
            Message::Close(_) => {
                return Err(HarnessError::Connection(
                    "the daemon closed the connection".to_string(),
                ))
            }
            _ => {
                return Err(HarnessError::Protocol(
                    "the daemon answered with an unexpected message".to_string(),
                ))
            }
        };
        serde_json::from_str(&text).map_err(|err| HarnessError::Protocol(err.to_string()))
    }
}

impl std::fmt::Debug for FakeLocalClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FakeLocalClient")
            .field("client_id", &self.client_id)
            .finish()
    }
}

/// The correlation id of a response.
fn response_request_id(response: &ClientResponse) -> String {
    match response {
        ClientResponse::Ok { request_id, .. } => request_id.clone(),
        ClientResponse::Error { request_id, .. } => request_id.clone(),
        ClientResponse::Accepted { request_id, .. } => request_id.clone(),
    }
}
