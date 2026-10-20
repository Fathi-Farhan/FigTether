//! A fake daemon for the tests of the adapter.
//!
//! The fake daemon speaks the same websocket handshake and the same frames
//! as the real daemon, records every frame it receives, and answers the
//! requests with an envelope. It lets the tests of the adapter run without a
//! daemon and without a network.
//!
//! The fake daemon also knows the write path: a write request is answered
//! with a preview, and a failing daemon answers every request with a
//! structured error of the write taxonomy.

use std::io::ErrorKind;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use crate::ws::accept_key;

/// What the fake daemon answers with.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Answer {
    /// Every request is answered with an envelope.
    Envelope,
    /// Every request is answered with an error of this code.
    Failure(String),
}

/// A daemon that records the frames it receives.
pub struct FakeDaemon {
    /// The port the fake daemon listens on.
    pub port: u16,
    received: Arc<Mutex<Vec<serde_json::Value>>>,
}

impl FakeDaemon {
    /// Start a fake daemon on a free port of the loopback interface.
    ///
    /// With `answer` set, the fake daemon answers every request with an
    /// envelope. Without it, the fake daemon stays silent, which is the
    /// shape of a daemon that runs a long operation.
    pub async fn start(answer: bool) -> Self {
        Self::spawn(Answer::Envelope, answer).await
    }

    /// Start a fake daemon that answers every request with an error.
    ///
    /// The daemon refuses the request the way the real daemon refuses a
    /// write without a grant, so that the tests can prove that a business
    /// failure travels as a structured tool result.
    pub async fn start_failing(code: &str) -> Self {
        Self::spawn(Answer::Failure(code.to_string()), true).await
    }

    async fn spawn(answer: Answer, respond: bool) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("the fake daemon binds");
        let port = listener
            .local_addr()
            .expect("the fake daemon has an address")
            .port();
        let received = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&received);
        tokio::spawn(async move {
            loop {
                let (stream, _) = match listener.accept().await {
                    Ok(pair) => pair,
                    Err(_) => return,
                };
                let (reader, writer) = stream.into_split();
                let mut session = Session {
                    reader: BufReader::new(reader),
                    writer,
                    received: Arc::clone(&sink),
                    answer: answer.clone(),
                    respond,
                };
                if session.run().await.is_err() {
                    return;
                }
            }
        });
        Self { port, received }
    }

    /// The frames the fake daemon has received so far.
    pub async fn frames(&self) -> Vec<serde_json::Value> {
        self.received.lock().await.clone()
    }

    /// Wait until the fake daemon has received at least `count` frames.
    pub async fn wait_for_frames(&self, count: usize) -> Vec<serde_json::Value> {
        for _ in 0..200 {
            let frames = self.frames().await;
            if frames.len() >= count {
                return frames;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        self.frames().await
    }
}

struct Session {
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
    received: Arc<Mutex<Vec<serde_json::Value>>>,
    answer: Answer,
    respond: bool,
}

impl Session {
    async fn run(&mut self) -> std::io::Result<()> {
        self.handshake().await?;
        loop {
            let Some((opcode, payload)) = read_frame(&mut self.reader).await else {
                return Ok(());
            };
            if opcode != 0x1 {
                continue;
            }
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(&payload) else {
                continue;
            };
            self.received.lock().await.push(value.clone());
            if !self.respond {
                continue;
            }
            let kind = value
                .get("type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            if kind.is_empty() || kind == "cancel" {
                continue;
            }
            let request_id = value
                .get("request_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            let response = match &self.answer {
                Answer::Envelope => serde_json::json!({
                    "type": "ok",
                    "request_id": request_id,
                    "envelope": envelope_value(&kind),
                }),
                Answer::Failure(code) => serde_json::json!({
                    "type": "error",
                    "request_id": request_id,
                    "error": {
                        "code": code,
                        "message": "the daemon refused the request",
                        "retryable": false,
                        "action": "ask the user to approve the plan in the plugin",
                        "request_id": request_id,
                    }
                }),
            };
            write_text(&mut self.writer, &response.to_string()).await?;
        }
    }

    async fn handshake(&mut self) -> std::io::Result<()> {
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            let read = self.reader.read(&mut byte).await?;
            if read == 0 {
                return Err(std::io::Error::new(
                    ErrorKind::UnexpectedEof,
                    "the client closed the connection",
                ));
            }
            head.push(byte[0]);
            if head.ends_with(b"\r\n\r\n") {
                break;
            }
        }
        let text = String::from_utf8_lossy(&head).to_string();
        let key = text
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                if name.trim().eq_ignore_ascii_case("sec-websocket-key") {
                    Some(value.trim().to_string())
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                std::io::Error::new(ErrorKind::InvalidData, "the request has no websocket key")
            })?;
        let response = format!(
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n\r\n",
            accept_key(&key)
        );
        self.writer.write_all(response.as_bytes()).await?;
        self.writer.flush().await?;
        Ok(())
    }
}

async fn read_frame(reader: &mut BufReader<OwnedReadHalf>) -> Option<(u8, Vec<u8>)> {
    let mut head = [0u8; 2];
    reader.read_exact(&mut head).await.ok()?;
    let opcode = head[0] & 0x0f;
    let masked = head[1] & 0x80 != 0;
    let mut length = usize::from(head[1] & 0x7f);
    if length == 126 {
        let mut extended = [0u8; 2];
        reader.read_exact(&mut extended).await.ok()?;
        length = usize::from(u16::from_be_bytes(extended));
    } else if length == 127 {
        let mut extended = [0u8; 8];
        reader.read_exact(&mut extended).await.ok()?;
        length = u64::from_be_bytes(extended) as usize;
    }
    let mut mask = [0u8; 4];
    if masked {
        reader.read_exact(&mut mask).await.ok()?;
    }
    let mut payload = vec![0u8; length];
    if length > 0 {
        reader.read_exact(&mut payload).await.ok()?;
    }
    if masked {
        for (index, byte) in payload.iter_mut().enumerate() {
            *byte ^= mask[index % 4];
        }
    }
    Some((opcode, payload))
}

async fn write_text(writer: &mut OwnedWriteHalf, text: &str) -> std::io::Result<()> {
    let payload = text.as_bytes();
    let mut frame = vec![0x81];
    if payload.len() < 126 {
        frame.push(payload.len() as u8);
    } else {
        frame.push(126);
        frame.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    }
    frame.extend_from_slice(payload);
    writer.write_all(&frame).await?;
    writer.flush().await?;
    Ok(())
}

/// An envelope the fake daemon answers with.
///
/// A write request is answered with the preview of a plan, so that the tests
/// of the write path can prove that the preview reaches the agent.
pub fn envelope_value(kind: &str) -> serde_json::Value {
    let data = if kind.starts_with("preview_write") {
        serde_json::json!({
            "preview": {
                "plan_id": "plan_1",
                "plan_hash": "sha256:abc",
                "created": ["1:9"],
                "changed": [],
                "removed": [],
                "moved": [],
                "operations_count": 1,
                "warnings": ["the parent is a component instance"],
                "blocked": false
            },
            "approval_id": "apr_1",
            "plan_id": "plan_1",
            "plan_hash": "sha256:abc"
        })
    } else if kind.starts_with("apply_write") {
        serde_json::json!({
            "result": {
                "plan_id": "plan_1",
                "plan_hash": "sha256:abc",
                "approval_id": "apr_1",
                "created": ["1:9"],
                "changed": [],
                "removed": [],
                "moved": [],
                "undo_committed": true,
                "version_saved": "v_1",
                "warnings": []
            },
            "approval_id": "apr_1",
            "plan_id": "plan_1",
            "plan_hash": "sha256:abc"
        })
    } else if kind.starts_with("get_write_approval") {
        serde_json::json!({
            "approval_id": "apr_1",
            "status": "approved",
            "approval_token": "w:apr_1:sha256:abc:session:3:2026-10-20T08:00:00Z",
            "warnings": []
        })
    } else {
        serde_json::json!({ "nodes": [] })
    };
    serde_json::json!({
        "schema_version": "1.0",
        "request_id": "req_fake",
        "source": {
            "kind": "plugin_live",
            "session_id": "11111111-1111-4111-8111-111111111111",
            "grant_epoch": 3,
            "connection_epoch": 2
        },
        "observation": {
            "observation_id": "22222222-2222-4222-8222-222222222222",
            "started_at": "2026-10-20T08:00:00Z",
            "observed_at": "2026-10-20T08:00:00.150Z",
            "consistency": "fresh",
            "freshness": "fresh_read"
        },
        "coverage": {
            "root_ids": ["1:2"],
            "returned_nodes": 1,
            "truncated": false,
            "unavailable_fields": []
        },
        "data": data,
        "warnings": []
    })
}
