//! Integration tests of the loopback HTTP surface.
//!
//! The tests drive the real router over a real loopback socket. They only
//! use the standard library, so the daemon keeps its dependency list small.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use figtether_daemon::{run, DaemonConfig, DaemonHandle};

/// A free loopback port.
fn free_port() -> u16 {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("bound");
    let port = listener.local_addr().expect("address").port();
    drop(listener);
    port
}

/// Start a daemon on a free port with a temporary runtime directory.
async fn start_daemon() -> (DaemonHandle, tempfile::TempDir) {
    let temp = tempfile::tempdir().expect("temp dir");
    let mut last_error = None;
    for _ in 0..5 {
        let config = DaemonConfig::new()
            .with_port(free_port())
            .with_runtime_dir(temp.path().join("figtether"));
        match run(config).await {
            Ok(handle) => return (handle, temp),
            Err(err) => last_error = Some(err),
        }
    }
    panic!("the daemon did not start: {last_error:?}");
}

/// Send a raw HTTP request and read the whole answer.
fn request(port: u16, raw: &str) -> String {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut stream = TcpStream::connect(addr).expect("connected");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("timeout");
    stream.write_all(raw.as_bytes()).expect("written");
    let mut answer = String::new();
    let _ = stream.read_to_string(&mut answer);
    answer
}

/// The body of an HTTP answer.
fn body_of(answer: &str) -> &str {
    match answer.split_once("\r\n\r\n") {
        Some((_head, body)) => body,
        None => "",
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_health_endpoint_answers_with_a_sanitized_status() {
    let (handle, _temp) = start_daemon().await;
    let port = handle.port();

    let answer = request(
        port,
        &format!("GET /health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"),
    );

    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    let body = body_of(&answer);
    assert!(body.contains("\"daemon_version\""), "{body}");
    assert!(body.contains("\"protocol_version\""), "{body}");
    assert!(body.contains("\"queue_depth\""), "{body}");
    assert!(body.contains("\"sessions\""), "{body}");
    assert!(!body.to_lowercase().contains("credential"), "{body}");
    assert!(!body.to_lowercase().contains("\"secret\""), "{body}");

    handle.shutdown();
    handle.wait_stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_pair_endpoint_mints_a_secret_for_the_loopback_peer() {
    let (handle, _temp) = start_daemon().await;
    let port = handle.port();

    let answer = request(port, &figtether_daemon::control::pair_request(port));

    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    let body = body_of(&answer);
    assert!(body.contains("\"secret\""), "{body}");
    assert!(body.contains("\"expires_in_seconds\""), "{body}");

    handle.shutdown();
    handle.wait_stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_foreign_host_is_refused() {
    let (handle, _temp) = start_daemon().await;
    let port = handle.port();

    let answer = request(
        port,
        "GET /health HTTP/1.1\r\nHost: evil.example\r\nConnection: close\r\n\r\n",
    );

    assert!(answer.starts_with("HTTP/1.1 403"), "{answer}");
    assert!(!body_of(&answer).contains("daemon_version"), "{answer}");

    handle.shutdown();
    handle.wait_stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_runtime_file_holds_the_client_credential() {
    let (handle, temp) = start_daemon().await;
    let path = temp.path().join("figtether").join("daemon.json");
    let body = std::fs::read_to_string(&path).expect("runtime file");

    assert!(body.contains("\"instance_id\""), "{body}");
    assert!(body.contains("\"port\""), "{body}");
    assert!(body.contains("\"client_credential\""), "{body}");

    handle.shutdown();
    handle.wait_stopped().await;

    assert!(
        !path.exists(),
        "the credential file is removed when the daemon stops"
    );
}
