//! Startup must never overwrite the credentials of another live daemon.
use std::net::{Ipv4Addr, TcpListener};

use figtether_daemon::{run, DaemonConfig, RuntimeDir};

fn free_port() -> u16 {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .expect("listener")
        .local_addr()
        .expect("address")
        .port()
}

#[tokio::test]
async fn a_failed_bind_preserves_the_existing_descriptor() {
    let temp = tempfile::tempdir().expect("temp");
    let dir = RuntimeDir::resolve(Some(temp.path())).expect("runtime");
    let previous = b"previous descriptor must survive";
    std::fs::write(dir.daemon_file(), previous).expect("fixture");
    let held = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("held port");
    let config = DaemonConfig::new()
        .with_port(held.local_addr().expect("address").port())
        .with_runtime_dir(temp.path());
    assert!(run(config).await.is_err());
    assert_eq!(
        std::fs::read(dir.daemon_file()).expect("descriptor"),
        previous
    );
    // The failed attempt released its runtime lock.
    let _lock = dir.lock().expect("lock released");
}

#[tokio::test]
async fn two_different_ports_cannot_share_a_runtime_and_shutdown_releases_it() {
    let temp = tempfile::tempdir().expect("temp");
    let first = run(DaemonConfig::new()
        .with_port(free_port())
        .with_runtime_dir(temp.path()))
    .await
    .expect("first daemon");
    let before = std::fs::read(temp.path().join("daemon.json")).expect("descriptor");
    let error = run(DaemonConfig::new()
        .with_port(free_port())
        .with_runtime_dir(temp.path()))
    .await
    .expect_err("runtime already owned");
    assert!(error.to_string().contains("runtime directory"));
    assert_eq!(
        std::fs::read(temp.path().join("daemon.json")).expect("descriptor"),
        before
    );
    first.shutdown();
    first.wait_stopped().await;
    assert!(!temp.path().join("daemon.json").exists());
    let second = run(DaemonConfig::new()
        .with_port(free_port())
        .with_runtime_dir(temp.path()))
    .await
    .expect("restart");
    second.shutdown();
    second.wait_stopped().await;
}
