//! A minimal websocket client for the local daemon.
//!
//! The adapter speaks RFC 6455 with the daemon over the loopback interface.
//! The crate carries no websocket dependency, so the small part of the
//! protocol that the daemon link needs is implemented here: the opening
//! handshake, masked client frames, and the reading of text frames including
//! fragmentation and the ping and pong control frames.
//!
//! Nothing in this module is part of the MCP surface. The frames carry the
//! FigTether client protocol, never MCP traffic.

use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::TcpStream;
use tokio::sync::Mutex;

use crate::endpoint::Endpoint;
use crate::error::McpError;

/// The magic value of the websocket handshake.
const HANDSHAKE_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// Largest frame the adapter accepts from the daemon.
const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// Largest handshake response the adapter accepts.
const MAX_HANDSHAKE_BYTES: usize = 8 * 1024;

const OP_CONTINUATION: u8 = 0x0;
const OP_TEXT: u8 = 0x1;
const OP_BINARY: u8 = 0x2;
const OP_CLOSE: u8 = 0x8;
const OP_PING: u8 = 0x9;
const OP_PONG: u8 = 0xa;

/// The write half of a daemon connection, shared between the reader task and
/// the callers that send requests.
#[derive(Clone)]
pub struct WsSender {
    writer: Arc<Mutex<OwnedWriteHalf>>,
}

impl WsSender {
    /// Send one text frame.
    pub async fn send_text(&self, text: &str) -> Result<(), McpError> {
        write_frame(&self.writer, OP_TEXT, text.as_bytes()).await
    }
}

/// A websocket connection to the daemon.
pub struct WsConnection {
    reader: BufReader<OwnedReadHalf>,
    writer: Arc<Mutex<OwnedWriteHalf>>,
}

impl WsConnection {
    /// Open a connection to the daemon and run the opening handshake.
    pub async fn connect(endpoint: &Endpoint, timeout: Duration) -> Result<Self, McpError> {
        let address = (endpoint.host(), endpoint.port());
        let stream = tokio::time::timeout(timeout, TcpStream::connect(address))
            .await
            .map_err(|_| McpError::DaemonUnavailable {
                endpoint: endpoint.url(),
                reason: format!("no answer within {timeout:?}"),
            })?
            .map_err(|source| McpError::DaemonUnavailable {
                endpoint: endpoint.url(),
                reason: source.to_string(),
            })?;
        let _ = stream.set_nodelay(true);
        let (reader, writer) = stream.into_split();
        let mut connection = Self {
            reader: BufReader::new(reader),
            writer: Arc::new(Mutex::new(writer)),
        };
        connection.handshake(endpoint, timeout).await?;
        Ok(connection)
    }

    /// The sender of this connection.
    pub fn sender(&self) -> WsSender {
        WsSender {
            writer: Arc::clone(&self.writer),
        }
    }

    /// Read the next text message.
    ///
    /// Ping frames are answered with a pong, pong frames are ignored, and a
    /// close frame ends the connection with an error.
    pub async fn read_message(&mut self) -> Result<String, McpError> {
        let mut message = Vec::new();
        let mut message_opcode = 0u8;
        loop {
            let (opcode, fin, payload) = self.read_frame().await?;
            match opcode {
                OP_CONTINUATION => {
                    if message_opcode == 0 {
                        return Err(McpError::DaemonProtocol(
                            "the daemon sent a continuation frame without a message".to_string(),
                        ));
                    }
                    message.extend_from_slice(&payload);
                }
                OP_TEXT | OP_BINARY => {
                    message_opcode = opcode;
                    message.extend_from_slice(&payload);
                }
                OP_CLOSE => {
                    return Err(McpError::DaemonConnection {
                        endpoint: "the daemon".to_string(),
                        reason: "the daemon closed the connection".to_string(),
                    });
                }
                OP_PING => {
                    write_frame(&self.writer, OP_PONG, &payload).await?;
                    continue;
                }
                OP_PONG => continue,
                other => {
                    return Err(McpError::DaemonProtocol(format!(
                        "the daemon sent the unknown opcode {other}"
                    )));
                }
            }
            if message.len() > MAX_FRAME_BYTES {
                return Err(McpError::DaemonProtocol(
                    "the daemon message is larger than the frame budget".to_string(),
                ));
            }
            if fin {
                break;
            }
        }
        if message_opcode != OP_TEXT {
            return Err(McpError::DaemonProtocol(
                "the daemon sent a frame that is not a text frame".to_string(),
            ));
        }
        String::from_utf8(message)
            .map_err(|_| McpError::DaemonProtocol("the daemon message is not UTF-8".to_string()))
    }

    async fn handshake(&mut self, endpoint: &Endpoint, timeout: Duration) -> Result<(), McpError> {
        let nonce = uuid::Uuid::new_v4();
        let key = base64_encode(nonce.as_bytes());
        let request = format!(
            "GET {path} HTTP/1.1\r\nHost: {authority}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n",
            path = endpoint.path(),
            authority = endpoint.authority(),
            key = key,
        );
        {
            let mut writer = self.writer.lock().await;
            writer
                .write_all(request.as_bytes())
                .await
                .map_err(|source| McpError::DaemonHandshake {
                    endpoint: endpoint.url(),
                    reason: source.to_string(),
                })?;
            writer
                .flush()
                .await
                .map_err(|source| McpError::DaemonHandshake {
                    endpoint: endpoint.url(),
                    reason: source.to_string(),
                })?;
        }
        let head = self.read_handshake_head(timeout).await?;
        let text = String::from_utf8_lossy(&head).to_string();
        let mut lines = text.lines();
        let status = lines.next().unwrap_or_default().trim().to_string();
        if !status.contains(" 101") {
            return Err(McpError::DaemonHandshake {
                endpoint: endpoint.url(),
                reason: format!("the daemon answered {status}"),
            });
        }
        let expected = accept_key(&key);
        let accepted = lines.find_map(|line| {
            let (name, value) = line.split_once(':')?;
            if name.trim().eq_ignore_ascii_case("sec-websocket-accept") {
                Some(value.trim().to_string())
            } else {
                None
            }
        });
        if accepted.as_deref() != Some(expected.as_str()) {
            return Err(McpError::DaemonHandshake {
                endpoint: endpoint.url(),
                reason: "the daemon did not confirm the websocket key".to_string(),
            });
        }
        Ok(())
    }

    async fn read_handshake_head(&mut self, timeout: Duration) -> Result<Vec<u8>, McpError> {
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        loop {
            let read = tokio::time::timeout(timeout, self.reader.read(&mut byte))
                .await
                .map_err(|_| McpError::DaemonHandshake {
                    endpoint: "the daemon".to_string(),
                    reason: format!("the handshake did not finish within {timeout:?}"),
                })?
                .map_err(|source| McpError::DaemonHandshake {
                    endpoint: "the daemon".to_string(),
                    reason: source.to_string(),
                })?;
            if read == 0 {
                return Err(McpError::DaemonHandshake {
                    endpoint: "the daemon".to_string(),
                    reason: "the daemon closed the connection during the handshake".to_string(),
                });
            }
            head.push(byte[0]);
            if head.ends_with(b"\r\n\r\n") {
                break;
            }
            if head.len() > MAX_HANDSHAKE_BYTES {
                return Err(McpError::DaemonHandshake {
                    endpoint: "the daemon".to_string(),
                    reason: "the handshake response is too large".to_string(),
                });
            }
        }
        Ok(head)
    }

    async fn read_frame(&mut self) -> Result<(u8, bool, Vec<u8>), McpError> {
        let mut head = [0u8; 2];
        self.reader
            .read_exact(&mut head)
            .await
            .map_err(|source| McpError::DaemonConnection {
                endpoint: "the daemon".to_string(),
                reason: source.to_string(),
            })?;
        let fin = head[0] & 0x80 != 0;
        let opcode = head[0] & 0x0f;
        let masked = head[1] & 0x80 != 0;
        let mut length = usize::from(head[1] & 0x7f);
        if length == 126 {
            let mut extended = [0u8; 2];
            self.reader
                .read_exact(&mut extended)
                .await
                .map_err(|source| McpError::DaemonConnection {
                    endpoint: "the daemon".to_string(),
                    reason: source.to_string(),
                })?;
            length = usize::from(u16::from_be_bytes(extended));
        } else if length == 127 {
            let mut extended = [0u8; 8];
            self.reader
                .read_exact(&mut extended)
                .await
                .map_err(|source| McpError::DaemonConnection {
                    endpoint: "the daemon".to_string(),
                    reason: source.to_string(),
                })?;
            length = u64::from_be_bytes(extended) as usize;
        }
        if length > MAX_FRAME_BYTES {
            return Err(McpError::DaemonProtocol(
                "the daemon frame is larger than the frame budget".to_string(),
            ));
        }
        let mut mask = [0u8; 4];
        if masked {
            self.reader.read_exact(&mut mask).await.map_err(|source| {
                McpError::DaemonConnection {
                    endpoint: "the daemon".to_string(),
                    reason: source.to_string(),
                }
            })?;
        }
        let mut payload = vec![0u8; length];
        if length > 0 {
            self.reader
                .read_exact(&mut payload)
                .await
                .map_err(|source| McpError::DaemonConnection {
                    endpoint: "the daemon".to_string(),
                    reason: source.to_string(),
                })?;
        }
        if masked {
            for (index, byte) in payload.iter_mut().enumerate() {
                *byte ^= mask[index % 4];
            }
        }
        Ok((opcode, fin, payload))
    }
}

async fn write_frame(
    writer: &Arc<Mutex<OwnedWriteHalf>>,
    opcode: u8,
    payload: &[u8],
) -> Result<(), McpError> {
    let mut frame = Vec::with_capacity(payload.len() + 14);
    frame.push(0x80 | opcode);
    let length = payload.len();
    if length < 126 {
        frame.push(0x80 | length as u8);
    } else if length <= usize::from(u16::MAX) {
        frame.push(0x80 | 126);
        frame.extend_from_slice(&(length as u16).to_be_bytes());
    } else {
        frame.push(0x80 | 127);
        frame.extend_from_slice(&(length as u64).to_be_bytes());
    }
    let mask = uuid::Uuid::new_v4();
    frame.extend_from_slice(&mask.as_bytes()[..4]);
    for (index, byte) in payload.iter().enumerate() {
        frame.push(byte ^ mask.as_bytes()[index % 4]);
    }
    let mut guard = writer.lock().await;
    guard
        .write_all(&frame)
        .await
        .map_err(|source| McpError::DaemonConnection {
            endpoint: "the daemon".to_string(),
            reason: source.to_string(),
        })?;
    guard
        .flush()
        .await
        .map_err(|source| McpError::DaemonConnection {
            endpoint: "the daemon".to_string(),
            reason: source.to_string(),
        })?;
    Ok(())
}

/// The value the daemon has to answer with for the given key.
pub(crate) fn accept_key(key: &str) -> String {
    let mut input = key.as_bytes().to_vec();
    input.extend_from_slice(HANDSHAKE_GUID.as_bytes());
    base64_encode(&sha1(&input))
}

/// Standard base64 with padding.
pub(crate) fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        let triple = (u32::from(first) << 16) | (u32::from(second) << 8) | u32::from(third);
        out.push(ALPHABET[((triple >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((triple >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((triple >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(triple & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// SHA-1 of a byte slice.
///
/// The websocket handshake needs SHA-1 and nothing else in the adapter does,
/// so the digest lives here instead of adding a dependency.
fn sha1(data: &[u8]) -> [u8; 20] {
    let mut state: [u32; 5] = [
        0x6745_2301,
        0xefcd_ab89,
        0x98ba_dcfe,
        0x1032_5476,
        0xc3d2_e1f0,
    ];
    let mut message = data.to_vec();
    let bit_length = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_length.to_be_bytes());
    for block in message.chunks(64) {
        let mut words = [0u32; 80];
        for (index, word) in words.iter_mut().enumerate().take(16) {
            let start = index * 4;
            *word = u32::from_be_bytes([
                block[start],
                block[start + 1],
                block[start + 2],
                block[start + 3],
            ]);
        }
        for index in 16..80 {
            words[index] =
                (words[index - 3] ^ words[index - 8] ^ words[index - 14] ^ words[index - 16])
                    .rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = state;
        for (index, word) in words.iter().enumerate() {
            let (mixed, constant) = match index {
                0..=19 => ((b & c) | ((!b) & d), 0x5a82_7999_u32),
                20..=39 => (b ^ c ^ d, 0x6ed9_eba1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1b_bcdc),
                _ => (b ^ c ^ d, 0xca62_c1d6),
            };
            let temp = a
                .rotate_left(5)
                .wrapping_add(mixed)
                .wrapping_add(e)
                .wrapping_add(constant)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }
        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
        state[4] = state[4].wrapping_add(e);
    }
    let mut digest = [0u8; 20];
    for (index, word) in state.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_matches_the_published_test_vectors() {
        assert_eq!(
            hex(&sha1(b"abc")),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(hex(&sha1(b"")), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(
            hex(&sha1(
                b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
            )),
            "84983e441c3bd26ebaae4aa1f95129e5e54670f1"
        );
    }

    #[test]
    fn the_handshake_accept_key_matches_the_specification() {
        assert_eq!(
            accept_key("dGhlIHNhbXBsZSBub25jZQ=="),
            "s3pPLMBiTxaQ9kYGzzhZRbK+xOo="
        );
    }

    #[test]
    fn base64_encodes_with_padding() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}
