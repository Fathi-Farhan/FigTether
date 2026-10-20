//! Acceptance flows of the FigTether product.
//!
//! Every test in this file is named after the acceptance id of
//! `docs/06-testing/02-acceptance-catalog.md` that it covers.
//!
//! The file holds two families of tests:
//!
//! - the flows that run against the harness daemon, the fake plugin client
//!   and the fake local client of this crate. They cover the disturbances
//!   and the negative paths of the protocol;
//! - the flows that run against the shipped `figtether-daemon` and the
//!   shipped `figtether-mcp`. They prove the behaviour of the crates that
//!   are actually shipped.
//!
//! The tests that need a real Figma Desktop carry a clear `#[ignore]` and
//! state what they need. See the crate documentation of `figtether-e2e` for
//! the scope rules of PRD 8.8.

use std::collections::BTreeSet;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use figtether_e2e::daemon::DaemonProfile;
use figtether_e2e::fake_client::FakeLocalClient;
use figtether_e2e::fake_plugin::{FakePlugin, PluginBehavior, PluginError};
use figtether_e2e::fixtures::{FixtureCorpus, ROOT_ID};
use figtether_e2e::harness::{assert_loopback_address, assert_loopback_only, DaemonHarness};
use figtether_e2e::mcp_stdio::{self, MCP_TOOLS};
use figtether_e2e::params;
use figtether_e2e::shipped::ShippedDaemon;
use figtether_mcp::{Endpoint, McpServer};
use figtether_model::envelope::{Consistency, Freshness, SourceKind};
use figtether_model::errors::ErrorCode;
use figtether_model::ids::SessionId;
use figtether_model::tools::AssetFormat;
use figtether_protocol::client::ClientRequest;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::sync::RwLock;

/// The client id the harness daemon and the plugins share.
const CLIENT_ID: &str = "harness-client";

/// The client id the MCP adapter of the harness uses.
const MCP_CLIENT_ID: &str = "figtether-e2e-mcp";

/// A text node of the generated corpus.
const TEXT_NODE: &str = "1:5";

/// A node id that is in no document of the corpus.
const OUTSIDE_NODE: &str = "9:999999";

/// The page size the shipped daemon traversal test asks for.
const PAGE_LIMIT: u32 = 25;

/// A running harness: daemon, fake plugin and fake local client.
struct Harness {
    daemon: DaemonHarness,
    plugin: FakePlugin,
    client: FakeLocalClient,
    corpus: Arc<RwLock<FixtureCorpus>>,
    root_id: String,
    text_node: String,
}

impl Harness {
    fn session_id(&self) -> SessionId {
        self.plugin.session_id()
    }

    /// The root node id of the corpus of the harness.
    fn root_id(&self) -> &str {
        &self.root_id
    }

    /// A text node of the corpus of the harness.
    fn text_node(&self) -> &str {
        &self.text_node
    }
}

/// Start the daemon, pair the plugin, grant the root and connect the client.
async fn harness_with(corpus: FixtureCorpus) -> Harness {
    harness_with_profile(corpus, DaemonProfile::fast()).await
}

/// Start the harness with an explicit daemon profile.
async fn harness_with_profile(corpus: FixtureCorpus, profile: DaemonProfile) -> Harness {
    let daemon = DaemonHarness::start_with(profile)
        .await
        .expect("the daemon starts");
    let secret = daemon.daemon().mint_pairing_secret();
    let root_id = corpus.root_id().to_string();
    let text_node = corpus.first_text_node().unwrap_or(TEXT_NODE).to_string();
    let corpus = Arc::new(RwLock::new(corpus));
    let plugin = FakePlugin::connect(&daemon.bridge_url(), &secret, CLIENT_ID, corpus.clone())
        .await
        .expect("the plugin pairs");
    plugin
        .grant(std::slice::from_ref(&root_id))
        .await
        .expect("the grant is accepted");
    let client =
        FakeLocalClient::connect(&daemon.client_url(), &daemon.client_credential(), CLIENT_ID)
            .await
            .expect("the client connects");
    Harness {
        daemon,
        plugin,
        client,
        corpus,
        root_id,
        text_node,
    }
}

/// The text of one node inside an envelope payload.
fn node_text(envelope: &figtether_model::envelope::Envelope, node_id: &str) -> Option<String> {
    envelope
        .data
        .get("nodes")
        .and_then(Value::as_array)
        .and_then(|nodes| {
            nodes
                .iter()
                .find(|node| node.get("id").and_then(Value::as_str) == Some(node_id))
        })
        .and_then(|node| node.get("text"))
        .and_then(|text| text.get("text"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// The ids of the nodes inside an envelope payload.
fn node_ids(envelope: &figtether_model::envelope::Envelope) -> Vec<String> {
    envelope
        .data
        .get("nodes")
        .and_then(Value::as_array)
        .map(|nodes| {
            nodes
                .iter()
                .filter_map(|node| node.get("id").and_then(Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Send one plain HTTP request to the daemon and return the status and body.
async fn http_get(
    addr: SocketAddr,
    path: &str,
    headers: &[(&str, &str)],
) -> Result<(u16, Vec<u8>), String> {
    let mut stream = TcpStream::connect(addr)
        .await
        .map_err(|err| err.to_string())?;
    let mut request = format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n");
    for (name, value) in headers {
        request.push_str(&format!("{name}: {value}\r\n"));
    }
    request.push_str("\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .map_err(|err| err.to_string())?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .map_err(|err| err.to_string())?;
    let text = String::from_utf8_lossy(&response).to_string();
    let status = text
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .unwrap_or(0);
    let body = text
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.as_bytes().to_vec())
        .unwrap_or_default();
    Ok((status, body))
}

/// The workspace root of the repository.
fn workspace_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("the crate lives in the workspace")
        .to_path_buf()
}

/// Strip the comments of a source file so the scan looks at code only.
fn strip_comments(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if in_line_comment {
            if character == '\n' {
                in_line_comment = false;
                result.push('\n');
            }
            continue;
        }
        if in_block_comment {
            if character == '*' && chars.peek() == Some(&'/') {
                chars.next();
                in_block_comment = false;
            }
            continue;
        }
        if character == '/' && chars.peek() == Some(&'/') {
            chars.next();
            in_line_comment = true;
            continue;
        }
        if character == '/' && chars.peek() == Some(&'*') {
            chars.next();
            in_block_comment = true;
            continue;
        }
        result.push(character);
    }
    result
}

/// The source files of one crate of the workspace.
fn crate_sources(crate_name: &str) -> Vec<(std::path::PathBuf, String)> {
    let root = workspace_root().join("crates").join(crate_name).join("src");
    let mut files = Vec::new();
    let mut stack = vec![root];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&path) {
                files.push((path, strip_comments(&text)));
            }
        }
    }
    files.sort();
    files
}

/// The rules the shipped daemon has to implement before a read flow runs.
///
/// The list is the contract of the acceptance catalog. Every entry names the
/// place in the daemon crate that has to change.
const SHIPPED_READ_PATH_GAPS: &str = "\
the shipped daemon does not serve a read yet, so the flow cannot run; the daemon crate has to \
implement four rules: (1) `crates/daemon/src/bridge.rs::serve_plugin` registers the session in \
the state `unpaired` and never moves it, so `SessionRegistry::grant` refuses the `grant_update` \
frame with `INVALID_LIMIT` and every read is refused with `SESSION_NOT_FOUND`; the daemon has to \
move the session to `SessionState::Paired` after the authentication and to `SessionState::Ready` \
after the grant. (2) `crates/daemon/src/client_api.rs::plan_of` maps every read to \
`JobKind::Read`, and `JobKind::Read::op` is `BridgeOp::ReadNodes`, so a `list_nodes` request \
reaches the plugin as `read_nodes` and the plugin answers with an empty node list; the daemon has \
to carry the operation of the request into the job. (3) \
`crates/daemon/src/client_api.rs::normalize_result` hardcodes `truncated: false` and \
`next_cursor: None`, so the traversal of a large scope is never reported as partial and no \
traversal cursor is ever minted (ADR-006). (4) \
`crates/daemon/src/client_api.rs::run_job` never calls `Cache::put` and never calls \
`AssetStore::put`, so the design cache and the asset store of the daemon stay empty and a revoke \
has nothing to purge (ADR-007)";

/// Wait until the daemon reports the grant of the session.
///
/// The shipped daemon records the grant of a `grant_update` frame and sends
/// no answer, so the harness learns about the grant from the status of the
/// session instead of from the bridge.
async fn wait_for_grant(
    client: &mut FakeLocalClient,
    session_id: SessionId,
    root_count: usize,
) -> Value {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let sessions = client
            .list_sessions()
            .await
            .expect("the daemon lists the sessions");
        if let Some(session) = sessions
            .iter()
            .find(|session| session["session_id"].as_str() == Some(&session_id.to_string()))
        {
            let granted = session["root_ids"].as_u64().unwrap_or(0) as usize;
            let epoch = session["grant_epoch"].as_u64().unwrap_or(0);
            if granted == root_count && epoch > 0 {
                return session.clone();
            }
        }
        assert!(
            Instant::now() < deadline,
            "the daemon did not record the grant of the session {session_id}: {SHIPPED_READ_PATH_GAPS}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Grant a scope to the session of the shipped daemon.
///
/// The plugin sends the `grant_update` frame and the harness waits until the
/// daemon reports the grant.
async fn grant_to_shipped_daemon(plugin: &FakePlugin, client: &mut FakeLocalClient, root: &str) {
    let roots = vec![root.to_string()];
    plugin
        .grant_quiet(&roots)
        .await
        .expect("the grant frame is sent");
    wait_for_grant(client, plugin.session_id(), roots.len()).await;
}

/// Connect a fake local client to the shipped daemon.
async fn connect_to_shipped(daemon: &ShippedDaemon) -> FakeLocalClient {
    FakeLocalClient::connect(
        &daemon.client_url(),
        &daemon
            .client_credential()
            .expect("the daemon file holds the credential"),
        CLIENT_ID,
    )
    .await
    .expect("the client connects")
}

// ---------------------------------------------------------------------------
// AT01 - the core path uses no official MCP or REST endpoint
// ---------------------------------------------------------------------------

#[tokio::test]
async fn at01_the_core_never_opens_a_non_loopback_connection() {
    let harness = harness_with(FixtureCorpus::small()).await;

    let addr = harness.daemon.addr();
    assert_loopback_address(addr).expect("the daemon binds a loopback address");
    assert_loopback_only(addr.port())
        .await
        .expect("the daemon does not answer on a routable address");

    let peer = harness.plugin.peer_addr();
    assert!(
        peer.ip().is_loopback(),
        "the plugin talked to {peer}, which is not loopback"
    );

    let mut scanned = 0usize;
    for crate_name in ["core", "protocol"] {
        for (path, source) in crate_sources(crate_name) {
            for marker in [
                "api.figma.com",
                "rest.figma.com",
                "figma.com/api",
                "figma.com/v1",
                "FIGMA_TOKEN",
                "FIGMA_API_TOKEN",
                "personal access token",
            ] {
                assert!(
                    !source.contains(marker),
                    "{} mentions {marker}, which the core must not use",
                    path.display()
                );
            }
            scanned += 1;
        }
    }
    assert!(scanned > 0, "the source scan found no file to review");

    for crate_name in ["core", "protocol"] {
        let manifest = workspace_root()
            .join("crates")
            .join(crate_name)
            .join("Cargo.toml");
        let text = std::fs::read_to_string(&manifest).expect("the manifest exists");
        for client in ["reqwest", "ureq", "isahc", "surf", "hyper =", "curl"] {
            assert!(
                !text.contains(client),
                "{} depends on the HTTP client {client}",
                manifest.display()
            );
        }
    }
}

// ---------------------------------------------------------------------------
// AT02 - no personal access token and loopback only egress
// ---------------------------------------------------------------------------

#[tokio::test]
async fn at02_no_token_is_required_and_the_read_succeeds() {
    let mut harness = harness_with(FixtureCorpus::small()).await;
    let session = harness.session_id();
    let root = harness.root_id().to_string();

    let envelope = harness
        .client
        .list_nodes(params::list_nodes(session, &root))
        .await
        .expect("the read succeeds without any credential");

    assert!(envelope.is_live(), "the read is a live read");
    assert_eq!(envelope.observation.consistency, Consistency::Fresh);
    assert_eq!(envelope.observation.freshness, Freshness::FreshRead);
    assert!(envelope.observation.cache_age_ms.is_none());
    assert!(envelope.observation.content_hash.is_some());
    assert!(!node_ids(&envelope).is_empty());

    let status = harness
        .client
        .request(ClientRequest::Status {
            request_id: harness.client.next_request_id(),
        })
        .await
        .expect("the daemon answers the status request");
    let payload = serde_json::to_value(&status).expect("the status serializes");
    let text = payload.to_string();
    for marker in ["token", "pat", "bearer", "authorization"] {
        assert!(
            !text.to_lowercase().contains(marker),
            "the daemon status mentions {marker}"
        );
    }

    // The only credential in the flow is the one time pairing secret, which
    // the client mints through the daemon itself.
    let secret = harness
        .client
        .pair_secret()
        .await
        .expect("the daemon mints a pairing secret");
    assert!(!secret.is_empty());
}

// ---------------------------------------------------------------------------
// AT04 - a fresh read tracks edits while a snapshot stays stable
// ---------------------------------------------------------------------------

#[tokio::test]
async fn at04_a_fresh_read_follows_an_edit_while_a_snapshot_keeps_the_old_value() {
    // The scenario mutates the document while the plugin serves it, so it
    // uses the inline generator of the harness instead of a committed file.
    let mut harness = harness_with(FixtureCorpus::generated_mixed()).await;
    let session = harness.session_id();
    let root = harness.root_id().to_string();
    let text_node = harness.text_node().to_string();

    let first = harness
        .client
        .get_nodes(params::get_nodes(session, &[text_node.as_str()]))
        .await
        .expect("the first fresh read succeeds");
    let first_text = node_text(&first, &text_node).expect("the node carries text");
    let first_hash = first
        .observation
        .content_hash
        .clone()
        .expect("the envelope carries a content hash");

    let snapshot_id = harness
        .daemon
        .daemon()
        .capture_snapshot(&session)
        .await
        .expect("the snapshot is captured");
    let snapshot_read = harness
        .client
        .list_nodes(params::list_nodes_of_snapshot(snapshot_id, &root))
        .await
        .expect("the snapshot read succeeds");
    assert_eq!(snapshot_read.source.kind, SourceKind::PluginSnapshot);
    assert_eq!(
        snapshot_read.observation.consistency,
        Consistency::BestEffort
    );
    let snapshot_text = node_text(&snapshot_read, &text_node).expect("the snapshot carries text");
    assert_eq!(snapshot_text, first_text);
    let snapshot_hash = snapshot_read
        .observation
        .content_hash
        .clone()
        .expect("the snapshot carries a content hash");

    harness
        .corpus
        .write()
        .await
        .apply_edit(&text_node, "edited label");
    let second = harness
        .client
        .get_nodes(params::get_nodes(session, &[text_node.as_str()]))
        .await
        .expect("the second fresh read succeeds");
    let second_text = node_text(&second, &text_node).expect("the node carries text");
    assert_eq!(second_text, "edited label");
    assert_ne!(second_text, first_text);
    let second_hash = second
        .observation
        .content_hash
        .clone()
        .expect("the envelope carries a content hash");
    assert_ne!(second_hash, first_hash, "the content hash follows the edit");
    assert!(second.observation.observed_at >= first.observation.observed_at);

    let snapshot_again = harness
        .client
        .list_nodes(params::list_nodes_of_snapshot(snapshot_id, &root))
        .await
        .expect("the snapshot read succeeds again");
    assert_eq!(
        node_text(&snapshot_again, &text_node).as_deref(),
        Some(snapshot_text.as_str()),
        "the snapshot never changes"
    );
    assert_eq!(
        snapshot_again.observation.content_hash.as_deref(),
        Some(snapshot_hash.as_str()),
        "the snapshot hash stays stable"
    );
    assert_eq!(snapshot_again.source.kind, SourceKind::PluginSnapshot);

    let mut cached = params::list_nodes(session, &root);
    cached.consistency = figtether_model::tools::ConsistencyRequest::Cached;
    cached.max_age_ms = Some(5_000);
    let first_cached = harness
        .client
        .list_nodes(cached.clone())
        .await
        .expect("the first cached read populates the cache");
    assert_eq!(
        first_cached.observation.freshness,
        Freshness::FreshRead,
        "a cache miss falls back to a fresh read"
    );
    let cached_read = harness
        .client
        .list_nodes(cached)
        .await
        .expect("the cached read succeeds");
    assert_eq!(cached_read.observation.freshness, Freshness::CacheHit);
    assert!(cached_read.observation.cache_age_ms.is_some());

    harness
        .plugin
        .set_behavior(PluginBehavior::DocumentChanged)
        .await;
    let error = harness
        .client
        .error(ClientRequest::GetNodes {
            request_id: harness.client.next_request_id(),
            params: serde_json::to_value(params::get_nodes(session, &[text_node.as_str()]))
                .expect("the parameters serialize"),
        })
        .await
        .expect("the daemon reports the document change");
    assert_eq!(error.code, ErrorCode::DocumentChanged);
    assert!(!error.retryable || error.retry_after_ms.is_some());
}

// ---------------------------------------------------------------------------
// AT05 - two sessions over identical node ids stay isolated
// ---------------------------------------------------------------------------

#[tokio::test]
async fn at05_two_sessions_with_identical_node_ids_never_exchange_data() {
    let daemon = DaemonHarness::start().await.expect("the daemon starts");
    let secret_a = daemon.daemon().mint_pairing_secret();
    let secret_b = daemon.daemon().mint_pairing_secret();
    // The scenario needs two documents that share every node id, so it uses
    // the inline generator of the harness.
    let corpus_a = Arc::new(RwLock::new(FixtureCorpus::labelled("FX-FILE-A", 600)));
    let corpus_b = Arc::new(RwLock::new(FixtureCorpus::labelled("FX-FILE-B", 600)));

    let plugin_a = FakePlugin::connect(&daemon.bridge_url(), &secret_a, CLIENT_ID, corpus_a)
        .await
        .expect("the first plugin pairs");
    plugin_a
        .grant(&[ROOT_ID.to_string()])
        .await
        .expect("the first grant is accepted");
    let plugin_b = FakePlugin::connect(&daemon.bridge_url(), &secret_b, CLIENT_ID, corpus_b)
        .await
        .expect("the second plugin pairs");
    plugin_b
        .grant(&[ROOT_ID.to_string()])
        .await
        .expect("the second grant is accepted");
    assert_ne!(plugin_a.session_id(), plugin_b.session_id());

    let mut client =
        FakeLocalClient::connect(&daemon.client_url(), &daemon.client_credential(), CLIENT_ID)
            .await
            .expect("the client connects");

    let sessions = client
        .list_sessions()
        .await
        .expect("the sessions are listed");
    assert_eq!(sessions.len(), 2, "both sessions belong to this client");
    for session in &sessions {
        assert!(
            session["capabilities"].is_object(),
            "every session reports its capabilities"
        );
        assert_eq!(session["capabilities"]["nodes"]["status"], "available");
        assert_eq!(session["capabilities"]["watch"]["status"], "unsupported");
    }

    let read_a = client
        .list_nodes(params::list_nodes(plugin_a.session_id(), ROOT_ID))
        .await
        .expect("the first session reads");
    let read_b = client
        .list_nodes(params::list_nodes(plugin_b.session_id(), ROOT_ID))
        .await
        .expect("the second session reads");
    assert_eq!(read_a.source.session_id, Some(plugin_a.session_id()));
    assert_eq!(read_b.source.session_id, Some(plugin_b.session_id()));
    assert_ne!(read_a.source.session_id, read_b.source.session_id);
    assert!(read_a.source.grant_epoch.is_some());
    assert!(read_b.source.grant_epoch.is_some());
    assert_ne!(
        read_a.observation.observation_id, read_b.observation.observation_id,
        "every observation window is its own"
    );
    let text_a = node_text(&read_a, TEXT_NODE).expect("the first file carries text");
    let text_b = node_text(&read_b, TEXT_NODE).expect("the second file carries text");
    assert_ne!(
        text_a, text_b,
        "the same node id carries the content of its own file"
    );

    assert!(read_a.coverage.truncated, "the large file is paginated");
    let cursor_a = read_a
        .coverage
        .next_cursor
        .clone()
        .expect("the first session produced a cursor");

    let mut crossed = params::list_nodes(plugin_b.session_id(), ROOT_ID);
    crossed.cursor = Some(cursor_a);
    let error = client
        .error(ClientRequest::ListNodes {
            request_id: client.next_request_id(),
            params: serde_json::to_value(crossed).expect("the parameters serialize"),
        })
        .await
        .expect("the daemon refuses the foreign cursor");
    assert_eq!(error.code, ErrorCode::CursorExpired);

    let export = client
        .envelope(ClientRequest::ExportAssets {
            request_id: client.next_request_id(),
            params: serde_json::to_value(params::export_assets(
                plugin_a.session_id(),
                &[TEXT_NODE],
                AssetFormat::Png,
            ))
            .expect("the parameters serialize"),
        })
        .await
        .expect("the export succeeds");
    let asset_id = export
        .data
        .get("asset_id")
        .and_then(Value::as_str)
        .expect("the export mints an asset id")
        .to_string();

    let (status, body) = http_get(
        daemon.addr(),
        &format!("/assets/{asset_id}?session_id={}", plugin_b.session_id()),
        &[("x-figtether-client-credential", &daemon.client_credential())],
    )
    .await
    .expect("the asset request runs");
    assert_eq!(status, 403, "the asset belongs to another session");
    assert!(
        !body.windows(4).any(|window| window == b"PNG\x0d"),
        "no asset bytes are served"
    );

    client
        .revoke(plugin_a.session_id())
        .await
        .expect("the first grant is revoked");
    let error = client
        .error(ClientRequest::ListNodes {
            request_id: client.next_request_id(),
            params: serde_json::to_value(params::list_nodes(plugin_a.session_id(), ROOT_ID))
                .expect("the parameters serialize"),
        })
        .await
        .expect("the revoked session is refused");
    assert_eq!(error.code, ErrorCode::Revoked);

    let still = client
        .list_nodes(params::list_nodes(plugin_b.session_id(), ROOT_ID))
        .await
        .expect("the other session keeps serving");
    assert_eq!(still.source.session_id, Some(plugin_b.session_id()));
    assert_eq!(
        node_text(&still, TEXT_NODE).as_deref(),
        Some(text_b.as_str()),
        "the other session keeps its own content"
    );
}

// ---------------------------------------------------------------------------
// AT06 - negative authentication, transport, scope and revoke mid transfer
// ---------------------------------------------------------------------------

#[tokio::test]
async fn at06_a_wrong_secret_is_refused_without_leaking_data() {
    let daemon = DaemonHarness::start().await.expect("the daemon starts");
    let corpus = Arc::new(RwLock::new(FixtureCorpus::small()));
    let error = FakePlugin::connect(
        &daemon.bridge_url(),
        "ftsecret-this-is-not-the-real-secret",
        CLIENT_ID,
        corpus,
    )
    .await
    .expect_err("the wrong secret is refused");
    match error {
        PluginError::Rejected(rejected) => {
            assert_eq!(rejected.code, "AUTH_REQUIRED");
            assert!(
                !rejected.message.contains("FX-"),
                "the refusal carries no design data"
            );
        }
        other => panic!("the wrong secret must be refused, got {other}"),
    }
    assert!(
        daemon.daemon().session_ids().is_empty(),
        "no session exists after a refused authentication"
    );
}

#[tokio::test]
async fn at06_an_expired_secret_is_refused() {
    let daemon = DaemonHarness::start().await.expect("the daemon starts");
    let secret = daemon
        .daemon()
        .mint_pairing_secret_with_ttl(Duration::from_millis(1));
    tokio::time::sleep(Duration::from_millis(20)).await;
    let corpus = Arc::new(RwLock::new(FixtureCorpus::small()));
    let error = FakePlugin::connect(&daemon.bridge_url(), &secret, CLIENT_ID, corpus)
        .await
        .expect_err("the expired secret is refused");
    match error {
        PluginError::Rejected(rejected) => assert_eq!(rejected.code, "AUTH_EXPIRED"),
        other => panic!("the expired secret must be refused, got {other}"),
    }
}

#[tokio::test]
async fn at06_a_replayed_secret_is_refused() {
    let daemon = DaemonHarness::start().await.expect("the daemon starts");
    let secret = daemon.daemon().mint_pairing_secret();
    let corpus = Arc::new(RwLock::new(FixtureCorpus::small()));
    let first = FakePlugin::connect(&daemon.bridge_url(), &secret, CLIENT_ID, corpus.clone())
        .await
        .expect("the first connection pairs");
    let error = FakePlugin::connect(&daemon.bridge_url(), &secret, CLIENT_ID, corpus)
        .await
        .expect_err("the replay is refused");
    match error {
        PluginError::Rejected(rejected) => assert_eq!(rejected.code, "AUTH_REQUIRED"),
        other => panic!("the replay must be refused, got {other}"),
    }
    assert_eq!(daemon.daemon().session_ids().len(), 1);
    assert_eq!(first.session_id(), daemon.daemon().session_ids()[0]);
}

#[tokio::test]
async fn at06_a_wrong_host_or_origin_is_refused() {
    let daemon = DaemonHarness::start().await.expect("the daemon starts");
    let secret = daemon.daemon().mint_pairing_secret();
    let corpus = Arc::new(RwLock::new(FixtureCorpus::small()));

    let error = FakePlugin::connect_with(
        &daemon.bridge_url(),
        vec![("Host".to_string(), "evil.example.com".to_string())],
        &secret,
        CLIENT_ID,
        corpus.clone(),
        Duration::from_millis(50),
    )
    .await
    .expect_err("the wrong host is refused");
    assert!(
        matches!(error, PluginError::Connection(_)),
        "the wrong host must fail the handshake, got {error}"
    );

    let error = FakePlugin::connect_with(
        &daemon.bridge_url(),
        vec![("Origin".to_string(), "http://evil.example.com".to_string())],
        &secret,
        CLIENT_ID,
        corpus,
        Duration::from_millis(50),
    )
    .await
    .expect_err("the wrong origin is refused");
    assert!(
        matches!(error, PluginError::Connection(_)),
        "the wrong origin must fail the handshake, got {error}"
    );

    assert!(
        daemon.daemon().session_ids().is_empty(),
        "a refused transport creates no session"
    );
}

#[tokio::test]
async fn at06_a_node_outside_the_grant_is_refused() {
    let mut harness = harness_with(FixtureCorpus::small()).await;
    let session = harness.session_id();
    let error = harness
        .client
        .error(ClientRequest::GetNodes {
            request_id: harness.client.next_request_id(),
            params: serde_json::to_value(params::get_nodes(session, &[OUTSIDE_NODE]))
                .expect("the parameters serialize"),
        })
        .await
        .expect("the daemon refuses the node");
    assert_eq!(error.code, ErrorCode::OutOfScope);
    assert!(
        !error.message.contains("FX-"),
        "the refusal carries no design data"
    );
}

#[tokio::test]
async fn at06_a_revoke_during_a_transfer_stops_it() {
    let mut harness = harness_with(FixtureCorpus::small()).await;
    let session = harness.session_id();
    let text_node = harness.text_node().to_string();

    harness
        .plugin
        .set_behavior(PluginBehavior::Chunked {
            chunks: 40,
            chunk_bytes: 4096,
            delay: Duration::from_millis(40),
        })
        .await;

    let mut client = FakeLocalClient::connect(
        &harness.daemon.client_url(),
        &harness.daemon.client_credential(),
        CLIENT_ID,
    )
    .await
    .expect("the client connects");
    let request = ClientRequest::ExportAssets {
        request_id: client.next_request_id(),
        params: serde_json::to_value(params::export_assets(
            session,
            &[text_node.as_str()],
            AssetFormat::Png,
        ))
        .expect("the parameters serialize"),
    };
    let export = tokio::spawn(async move { client.request(request).await });

    let deadline = Instant::now() + Duration::from_secs(10);
    while harness.plugin.chunks_sent() < 2 {
        assert!(
            Instant::now() < deadline,
            "the plugin never started the transfer"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let chunks_at_revoke = harness.plugin.chunks_sent();

    harness
        .client
        .revoke(session)
        .await
        .expect("the grant is revoked");

    let response = export
        .await
        .expect("the export task finishes")
        .expect("the daemon answers");
    match response {
        figtether_protocol::client::ClientResponse::Error { error, .. } => {
            assert_eq!(error.code, ErrorCode::Revoked);
        }
        other => panic!("the revoked transfer must fail, got {other:?}"),
    }
    assert_eq!(
        harness.daemon.daemon().asset_count(),
        0,
        "no asset is published from a revoked transfer"
    );
    assert!(
        chunks_at_revoke >= 2,
        "the transfer was running when the revoke arrived"
    );
}

#[tokio::test]
async fn at06_an_oversized_payload_is_refused() {
    let mut harness = harness_with(FixtureCorpus::small()).await;
    let session = harness.session_id();
    let root = harness.root_id().to_string();

    harness.plugin.set_behavior(PluginBehavior::Oversized).await;
    let error = harness
        .client
        .error(ClientRequest::ListNodes {
            request_id: harness.client.next_request_id(),
            params: serde_json::to_value(params::list_nodes(session, &root))
                .expect("the parameters serialize"),
        })
        .await
        .expect("the daemon refuses the oversized frame");
    assert_eq!(error.code, ErrorCode::PayloadTooLarge);
    assert!(
        !error.message.contains("FX-"),
        "the refusal carries no design data"
    );
}

// ---------------------------------------------------------------------------
// AT08 - a slow plugin misses the deadline
// ---------------------------------------------------------------------------

#[tokio::test]
async fn at08_a_slow_plugin_misses_the_read_deadline() {
    let mut harness = harness_with(FixtureCorpus::small()).await;
    let session = harness.session_id();
    let root = harness.root_id().to_string();

    harness
        .plugin
        .set_behavior(PluginBehavior::Slow {
            delay: Duration::from_secs(30),
        })
        .await;
    let started = Instant::now();
    let error = harness
        .client
        .error(ClientRequest::ListNodes {
            request_id: harness.client.next_request_id(),
            params: serde_json::to_value(params::list_nodes(session, &root))
                .expect("the parameters serialize"),
        })
        .await
        .expect("the daemon reports the timeout");
    assert_eq!(error.code, ErrorCode::Timeout);
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "the deadline of the read did not fire"
    );
}

// ---------------------------------------------------------------------------
// AT07 - pagination, truncation and no leakage on oversized scopes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn at07_a_large_fixture_returns_a_partial_result_and_a_cursor() {
    let corpus = FixtureCorpus::generated_large(2000);
    let expected = corpus.len();
    let mut harness = harness_with(corpus).await;
    let session = harness.session_id();
    let root = harness.root_id().to_string();

    let first = harness
        .client
        .list_nodes(params::list_nodes(session, &root))
        .await
        .expect("the first page is served");
    assert!(
        first.coverage.truncated,
        "a large scope is reported as truncated"
    );
    let cursor = first
        .coverage
        .next_cursor
        .clone()
        .expect("a truncated traversal carries a cursor");
    let encoded = serde_json::to_vec(&first).expect("the envelope serializes");
    assert!(
        encoded.len() <= 64 * 1024,
        "the response stays inside the 64 KiB budget, got {} bytes",
        encoded.len()
    );

    let mut seen: BTreeSet<String> = node_ids(&first).into_iter().collect();
    let mut next = Some(cursor);
    let mut pages = 1usize;
    while let Some(cursor) = next {
        let mut request = params::list_nodes(session, &root);
        request.cursor = Some(cursor);
        let page = harness
            .client
            .list_nodes(request)
            .await
            .expect("the next page is served");
        for id in node_ids(&page) {
            seen.insert(id);
        }
        pages += 1;
        assert!(pages < 100, "the traversal does not end");
        next = page.coverage.next_cursor.clone();
    }
    assert_eq!(pages, 10, "the traversal walks every page");
    assert_eq!(
        seen.len(),
        expected - 1,
        "the pages cover the whole scope exactly once"
    );

    let mut too_deep = params::list_nodes(session, &root);
    too_deep.depth = Some(33);
    let error = harness
        .client
        .error(ClientRequest::ListNodes {
            request_id: harness.client.next_request_id(),
            params: serde_json::to_value(too_deep).expect("the parameters serialize"),
        })
        .await
        .expect("the daemon refuses the depth");
    assert_eq!(error.code, ErrorCode::InvalidLimit);

    let batch: Vec<String> = (0..51).map(|index| format!("1:{index}")).collect();
    let error = harness
        .client
        .error(ClientRequest::GetNodes {
            request_id: harness.client.next_request_id(),
            params: serde_json::to_value(figtether_model::tools::GetNodesParams {
                session_id: Some(session),
                snapshot_id: None,
                node_ids: batch.clone(),
                depth: None,
                fields: Vec::new(),
                consistency: figtether_model::tools::ConsistencyRequest::Fresh,
                max_age_ms: None,
            })
            .expect("the parameters serialize"),
        })
        .await
        .expect("the daemon refuses the batch");
    assert_eq!(error.code, ErrorCode::InvalidLimit);

    let mut sibling = params::list_nodes(session, &root);
    sibling.root_id = OUTSIDE_NODE.to_string();
    let error = harness
        .client
        .error(ClientRequest::ListNodes {
            request_id: harness.client.next_request_id(),
            params: serde_json::to_value(sibling).expect("the parameters serialize"),
        })
        .await
        .expect("the daemon refuses a node outside the grant");
    assert_eq!(error.code, ErrorCode::OutOfScope);

    let corpus = harness.corpus.read().await;
    let index = corpus.node_index();
    for id in &seen {
        assert!(
            index.contains(&root, id),
            "the page leaked the node {id} from outside the grant"
        );
    }
}

// ---------------------------------------------------------------------------
// AT09 - connection disturbance
// ---------------------------------------------------------------------------

#[tokio::test]
async fn at09_a_closed_plugin_is_detected_as_offline() {
    let mut harness = harness_with(FixtureCorpus::small()).await;
    let session = harness.session_id();
    let root = harness.root_id().to_string();

    harness.plugin.close().await.expect("the plugin closes");
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if harness.daemon.daemon().session_state(&session)
            == Some(figtether_core::state::SessionState::Offline)
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the offline detection missed the 15 s window"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let error = harness
        .client
        .error(ClientRequest::ListNodes {
            request_id: harness.client.next_request_id(),
            params: serde_json::to_value(params::list_nodes(session, &root))
                .expect("the parameters serialize"),
        })
        .await
        .expect("the daemon refuses the read");
    assert_eq!(error.code, ErrorCode::PluginOffline);
}

#[tokio::test]
async fn at09_a_reopened_plugin_creates_a_new_session() {
    let mut harness = harness_with(FixtureCorpus::small()).await;
    let old_session = harness.session_id();
    let root = harness.root_id().to_string();
    harness.plugin.close().await.expect("the plugin closes");

    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if harness.daemon.daemon().session_state(&old_session)
            == Some(figtether_core::state::SessionState::Offline)
        {
            break;
        }
        assert!(Instant::now() < deadline, "the session never went offline");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let secret = harness.daemon.daemon().mint_pairing_secret();
    let reopened = FakePlugin::connect(
        &harness.daemon.bridge_url(),
        &secret,
        CLIENT_ID,
        harness.corpus.clone(),
    )
    .await
    .expect("the plugin pairs again");
    reopened
        .grant(std::slice::from_ref(&root))
        .await
        .expect("the grant is accepted");
    assert_ne!(
        reopened.session_id(),
        old_session,
        "a reopen creates a new session identity"
    );

    let error = harness
        .client
        .error(ClientRequest::ListNodes {
            request_id: harness.client.next_request_id(),
            params: serde_json::to_value(params::list_nodes(old_session, &root))
                .expect("the parameters serialize"),
        })
        .await
        .expect("the daemon refuses the old session");
    assert_eq!(error.code, ErrorCode::SessionNotFound);
}

#[tokio::test]
async fn at09_a_daemon_restart_invalidates_the_old_credential() {
    let first = DaemonHarness::start().await.expect("the daemon starts");
    let first_instance = first.instance_id();
    let first_credential = first.client_credential();
    first.shutdown().await.expect("the daemon stops");

    let second = DaemonHarness::start().await.expect("the daemon starts");
    assert_ne!(
        second.instance_id(),
        first_instance,
        "a restart changes the daemon instance identity"
    );
    assert_ne!(second.client_credential(), first_credential);
    let error = FakeLocalClient::connect(&second.client_url(), &first_credential, CLIENT_ID)
        .await
        .expect_err("the old credential is refused");
    assert!(matches!(
        error,
        figtether_e2e::harness::HarnessError::Connection(_)
    ));
}

#[tokio::test]
async fn at09_a_late_frame_with_an_old_epoch_is_discarded() {
    let mut harness = harness_with(FixtureCorpus::small()).await;
    let session = harness.session_id();
    let root = harness.root_id().to_string();

    harness
        .plugin
        .set_behavior(PluginBehavior::StaleEpoch)
        .await;
    let error = harness
        .client
        .error(ClientRequest::ListNodes {
            request_id: harness.client.next_request_id(),
            params: serde_json::to_value(params::list_nodes(session, &root))
                .expect("the parameters serialize"),
        })
        .await
        .expect("the daemon refuses the stale answer");
    assert_eq!(error.code, ErrorCode::Timeout);
    assert!(
        !error.message.contains("FX-"),
        "the stale frame never reaches the client"
    );
}

// ---------------------------------------------------------------------------
// AT14 - the MCP lifecycle over stdio
// ---------------------------------------------------------------------------

#[tokio::test]
async fn at14_the_mcp_handshake_works_over_stdio_and_stdout_carries_protocol_only() {
    let harness = harness_with(FixtureCorpus::small()).await;
    let session = harness.session_id();
    let root = harness.root_id().to_string();
    let client = FakeLocalClient::connect(
        &harness.daemon.client_url(),
        &harness.daemon.client_credential(),
        CLIENT_ID,
    )
    .await
    .expect("the client connects");
    let mut stdio = mcp_stdio::spawn(client);

    let initialize = stdio
        .call(
            1,
            "initialize",
            json!({
                "protocolVersion": mcp_stdio::MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "figtether-e2e", "version": "0.1.0" },
            }),
        )
        .await
        .expect("the handshake is answered");
    let result = initialize
        .get("result")
        .expect("the handshake carries a result");
    assert_eq!(result["protocolVersion"], mcp_stdio::MCP_PROTOCOL_VERSION);
    assert!(result["capabilities"].is_object());
    assert_eq!(initialize["jsonrpc"], "2.0");

    let tools = stdio
        .call(2, "tools/list", json!({}))
        .await
        .expect("the tool list is answered");
    let listed = tools["result"]["tools"]
        .as_array()
        .expect("the tool list is an array");
    assert_eq!(listed.len(), MCP_TOOLS.len());
    for tool in listed {
        let name = tool["name"].as_str().expect("the tool has a name");
        assert!(
            MCP_TOOLS.contains(&name),
            "the tool {name} is not part of the frozen surface"
        );
        assert_eq!(
            tool["annotations"]["readOnlyHint"],
            json!(true),
            "the tool {name} must be read only"
        );
        assert_eq!(
            tool["annotations"]["destructiveHint"],
            json!(false),
            "the tool {name} must not be destructive"
        );
    }

    let call = stdio
        .call(
            3,
            "tools/call",
            json!({
                "name": "list_nodes",
                "arguments": params::list_nodes(session, &root),
            }),
        )
        .await
        .expect("the tool call is answered");
    let envelope = &call["result"]["structuredContent"];
    assert_eq!(envelope["schema_version"], figtether_model::SCHEMA_VERSION);
    assert_eq!(envelope["source"]["kind"], "plugin_live");
    assert_eq!(envelope["observation"]["consistency"], "fresh");
    assert!(envelope["coverage"].is_object());
    assert!(envelope["warnings"].is_array());

    // A notification produces no answer on stdout, only a diagnostic on
    // stderr.
    stdio
        .write(&json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
        }))
        .await
        .expect("the notification is written");
    let quiet = tokio::time::timeout(Duration::from_millis(500), stdio.read_stdout()).await;
    assert!(
        quiet.is_err(),
        "stdout must stay free of anything but protocol data"
    );
    let diagnostic = tokio::time::timeout(Duration::from_secs(5), stdio.read_stderr())
        .await
        .expect("the server writes a diagnostic")
        .expect("the diagnostic is readable");
    assert!(
        diagnostic.contains("notification"),
        "the diagnostic names the notification, got {diagnostic}"
    );
}

// ---------------------------------------------------------------------------
// AT15 - revoke cleanup and no replay
// ---------------------------------------------------------------------------

#[tokio::test]
async fn at15_a_revoke_clears_the_cache_and_the_assets_and_blocks_retries() {
    let mut harness = harness_with(FixtureCorpus::small()).await;
    let session = harness.session_id();
    let root = harness.root_id().to_string();
    let text_node = harness.text_node().to_string();

    let mut cached = params::list_nodes(session, &root);
    cached.consistency = figtether_model::tools::ConsistencyRequest::Cached;
    cached.max_age_ms = Some(60_000);
    harness
        .client
        .list_nodes(cached)
        .await
        .expect("the cached read populates the cache");
    assert_eq!(harness.daemon.daemon().cache_entry_count(), 1);

    let export = harness
        .client
        .envelope(ClientRequest::ExportAssets {
            request_id: harness.client.next_request_id(),
            params: serde_json::to_value(params::export_assets(
                session,
                &[text_node.as_str()],
                AssetFormat::Png,
            ))
            .expect("the parameters serialize"),
        })
        .await
        .expect("the export populates the asset store");
    let asset_id = export
        .data
        .get("asset_id")
        .and_then(Value::as_str)
        .expect("the export mints an asset id")
        .to_string();
    assert_eq!(harness.daemon.daemon().asset_count(), 1);

    harness
        .client
        .revoke(session)
        .await
        .expect("the grant is revoked");
    assert_eq!(
        harness.daemon.daemon().cache_entry_count(),
        0,
        "the revoke clears the cache"
    );
    assert_eq!(
        harness.daemon.daemon().asset_count(),
        0,
        "the revoke clears the asset store"
    );

    let error = harness
        .client
        .error(ClientRequest::ListNodes {
            request_id: harness.client.next_request_id(),
            params: serde_json::to_value(params::list_nodes(session, &root))
                .expect("the parameters serialize"),
        })
        .await
        .expect("the replay is refused");
    assert_eq!(error.code, ErrorCode::Revoked);

    let (status, body) = http_get(
        harness.daemon.addr(),
        &format!("/assets/{asset_id}?session_id={session}"),
        &[(
            "x-figtether-client-credential",
            &harness.daemon.client_credential(),
        )],
    )
    .await
    .expect("the asset request runs");
    assert_ne!(status, 200, "the asset is not served after the revoke");
    assert!(
        !body.windows(4).any(|window| window == b"PNG\x0d"),
        "no asset bytes are served after the revoke"
    );

    // A pause refuses new reads, a resume brings them back, a revoke removes
    // the results.
    let mut second = harness_with(FixtureCorpus::small()).await;
    let session = second.session_id();
    let root = second.root_id().to_string();
    second
        .daemon
        .daemon()
        .pause(&session)
        .expect("the session is paused");
    let error = second
        .client
        .error(ClientRequest::ListNodes {
            request_id: second.client.next_request_id(),
            params: serde_json::to_value(params::list_nodes(session, &root))
                .expect("the parameters serialize"),
        })
        .await
        .expect("the paused session refuses the read");
    assert_eq!(error.code, ErrorCode::Paused);

    second
        .daemon
        .daemon()
        .resume(&session)
        .expect("the session is resumed");
    second
        .client
        .list_nodes(params::list_nodes(session, &root))
        .await
        .expect("the resumed session serves reads again");

    second
        .client
        .revoke(session)
        .await
        .expect("the grant is revoked");
    let error = second
        .client
        .error(ClientRequest::ListNodes {
            request_id: second.client.next_request_id(),
            params: serde_json::to_value(params::list_nodes(session, &root))
                .expect("the parameters serialize"),
        })
        .await
        .expect("the revoked session refuses the read");
    assert_eq!(error.code, ErrorCode::Revoked);
}

// ---------------------------------------------------------------------------
// AT18 - no document mutation from the read or export flows
// ---------------------------------------------------------------------------

#[tokio::test]
async fn at18_the_fixture_is_unchanged_after_reads_and_exports() {
    let mut harness = harness_with(FixtureCorpus::mixed()).await;
    let session = harness.session_id();
    let root = harness.root_id().to_string();
    let text_node = harness.text_node().to_string();
    let before = harness.corpus.read().await.hash();

    harness
        .client
        .list_nodes(params::list_nodes(session, &root))
        .await
        .expect("the list read succeeds");
    harness
        .client
        .get_nodes(params::get_nodes(session, &[text_node.as_str()]))
        .await
        .expect("the node read succeeds");
    let inspect = harness
        .client
        .envelope(ClientRequest::InspectDesign {
            request_id: harness.client.next_request_id(),
            params: serde_json::to_value(params::inspect_design(session, &root))
                .expect("the parameters serialize"),
        })
        .await
        .expect("the inspection succeeds");
    assert!(inspect.data.get("node").is_some());
    harness
        .client
        .envelope(ClientRequest::GetDesignTokens {
            request_id: harness.client.next_request_id(),
            params: serde_json::to_value(params::design_tokens(session, &root))
                .expect("the parameters serialize"),
        })
        .await
        .expect("the token read succeeds");
    harness
        .client
        .envelope(ClientRequest::ExportAssets {
            request_id: harness.client.next_request_id(),
            params: serde_json::to_value(params::export_assets(
                session,
                &[text_node.as_str()],
                AssetFormat::Svg,
            ))
            .expect("the parameters serialize"),
        })
        .await
        .expect("the export succeeds");
    let snapshot_id = harness
        .daemon
        .daemon()
        .capture_snapshot(&session)
        .await
        .expect("the snapshot is captured");
    harness
        .client
        .list_nodes(params::list_nodes_of_snapshot(snapshot_id, &root))
        .await
        .expect("the snapshot read succeeds");

    let after = harness.corpus.read().await.hash();
    assert_eq!(
        before, after,
        "the read and export pass changed the fixture"
    );

    // The used tool surface carries no write operation.
    for tool in MCP_TOOLS {
        assert!(
            !tool.contains("write")
                && !tool.contains("delete")
                && !tool.contains("update")
                && !tool.contains("create"),
            "the tool {tool} looks like a write operation"
        );
    }
}

// ---------------------------------------------------------------------------
// The flows that run against the shipped crates
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn at01_the_shipped_daemon_never_opens_a_non_loopback_connection() {
    let daemon = ShippedDaemon::start()
        .await
        .expect("the shipped daemon starts");

    // The health endpoint answers on the loopback interface.
    let (status, body) = http_get(daemon.addr(), "/health", &[])
        .await
        .expect("the health request runs");
    assert_eq!(status, 200, "the health endpoint answers");
    let health: Value = serde_json::from_slice(&body).expect("the health answer is JSON");
    let bind_address = health["bind_address"]
        .as_str()
        .expect("the health endpoint names the bind address");
    assert!(
        bind_address.starts_with("127.0.0.1"),
        "the daemon reports the loopback interface: {health}"
    );
    assert_eq!(
        health["port"].as_u64(),
        Some(u64::from(daemon.port())),
        "the health endpoint reports the port of the listener"
    );

    // The daemon file records the loopback port.
    let file = daemon
        .daemon_file_value()
        .expect("the daemon file is readable");
    assert_eq!(
        file["port"].as_u64(),
        Some(u64::from(daemon.port())),
        "the daemon file records the port of the listener: {file}"
    );
    assert_eq!(
        file["instance_id"].as_str(),
        Some(daemon.instance_id().as_str()),
        "the daemon file records the identity of the instance: {file}"
    );

    // Nothing answers on the routable address of the machine.
    let routable = figtether_e2e::harness::non_loopback_ipv4();
    if let Some(ip) = routable {
        let addr = SocketAddr::from((ip, daemon.port()));
        let outcome = tokio::time::timeout(Duration::from_secs(5), TcpStream::connect(addr)).await;
        match outcome {
            Ok(Err(err)) => assert_eq!(
                err.kind(),
                std::io::ErrorKind::ConnectionRefused,
                "the connection to {addr} is refused, the daemon binds the loopback interface only"
            ),
            Ok(Ok(_)) => panic!("the daemon answered on the routable address {addr}"),
            Err(_elapsed) => panic!("the connection to {addr} never completed"),
        }
    }

    // The core and the protocol carry no REST client and no token client.
    let mut scanned = 0usize;
    for crate_name in ["core", "protocol"] {
        for (path, source) in crate_sources(crate_name) {
            for marker in [
                "api.figma.com",
                "rest.figma.com",
                "figma.com/api",
                "figma.com/v1",
                "FIGMA_TOKEN",
                "FIGMA_API_TOKEN",
                "personal access token",
            ] {
                assert!(
                    !source.contains(marker),
                    "{} mentions {marker}, which the core must not use",
                    path.display()
                );
            }
            scanned += 1;
        }
    }
    assert!(scanned > 0, "the source scan found no file to review");

    for crate_name in ["core", "protocol"] {
        let manifest = workspace_root()
            .join("crates")
            .join(crate_name)
            .join("Cargo.toml");
        let text = std::fs::read_to_string(&manifest).expect("the manifest exists");
        for client in ["reqwest", "ureq", "isahc", "surf", "hyper =", "curl"] {
            assert!(
                !text.contains(client),
                "{} depends on the HTTP client {client}",
                manifest.display()
            );
        }
    }

    daemon.shutdown().await.expect("the daemon stops");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn at07_the_shipped_daemon_paginates_a_large_scope() {
    let daemon = ShippedDaemon::start()
        .await
        .expect("the shipped daemon starts");
    let corpus = Arc::new(RwLock::new(FixtureCorpus::medium()));
    let root = corpus.read().await.root_id().to_string();
    let expected = corpus.read().await.len();

    let secret = daemon
        .mint_pairing_secret()
        .await
        .expect("the daemon mints a pairing secret");
    let plugin = FakePlugin::connect(&daemon.bridge_url(), &secret, CLIENT_ID, corpus.clone())
        .await
        .expect("the plugin pairs");
    let mut client = connect_to_shipped(&daemon).await;
    grant_to_shipped_daemon(&plugin, &mut client, &root).await;

    let mut request = params::list_nodes(plugin.session_id(), &root);
    request.limit = Some(PAGE_LIMIT);
    let first = client
        .list_nodes(request)
        .await
        .expect("the first page is served");
    assert!(
        first.coverage.truncated,
        "a large scope is reported as truncated: the plugin answers a page with the flag \
         `truncated`, and the daemon has to copy it into `coverage.truncated` and mint the \
         `coverage.next_cursor` of the traversal (ADR-006); \
         `crates/daemon/src/client_api.rs::normalize_result` hardcodes `truncated: false` and \
         `next_cursor: None`"
    );
    let cursor = first
        .coverage
        .next_cursor
        .clone()
        .expect("a truncated traversal carries a cursor");
    assert!(
        node_ids(&first).len() <= PAGE_LIMIT as usize,
        "the page holds no more than the requested number of nodes"
    );

    // The cursor continues the traversal and never repeats a node.
    let mut seen: BTreeSet<String> = node_ids(&first).into_iter().collect();
    let mut next = Some(cursor);
    let mut pages = 1usize;
    while let Some(cursor) = next {
        let mut request = params::list_nodes(plugin.session_id(), &root);
        request.limit = Some(PAGE_LIMIT);
        request.cursor = Some(cursor);
        let page = client
            .list_nodes(request)
            .await
            .expect("the next page is served");
        for id in node_ids(&page) {
            assert!(
                seen.insert(id.clone()),
                "the traversal returned the node {id} twice"
            );
        }
        pages += 1;
        assert!(pages < 200, "the traversal does not end");
        next = page.coverage.next_cursor.clone();
    }
    assert_eq!(
        seen.len(),
        expected - 1,
        "the traversal covers every node of the scope exactly once"
    );

    // The result never holds a node outside the granted scope.
    let index = corpus.read().await.node_index();
    for id in &seen {
        assert!(
            index.contains(&root, id),
            "the page leaked the node {id} from outside the grant"
        );
    }

    daemon.shutdown().await.expect("the daemon stops");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn at14_the_shipped_mcp_adapter_completes_the_stdio_handshake() {
    let daemon = ShippedDaemon::start()
        .await
        .expect("the shipped daemon starts");
    // The adapter is built against the endpoint of the shipped daemon and
    // against the credential of its runtime directory.
    let server = McpServer::builder()
        .with_endpoint(Endpoint::new("127.0.0.1", daemon.port(), "/client"))
        .with_credential(
            daemon
                .client_credential()
                .expect("the daemon file holds the credential"),
            MCP_CLIENT_ID,
        )
        .build()
        .expect("the MCP server is built");

    // The adapter serves the surface over a pair of pipes, so the test
    // speaks the raw JSON-RPC of the MCP specification.
    let (server_side, client_side) = tokio::io::duplex(1024 * 1024);
    let (reader, writer) = tokio::io::split(client_side);
    let mut reader = BufReader::new(reader);
    let mut writer = writer;
    let handle = tokio::spawn(async move { server.serve(server_side).await });

    write_line(
        &mut writer,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "figtether-e2e", "version": "0.1.0" }
            }
        }),
    )
    .await;

    let answer = read_line(&mut reader).await;
    assert_eq!(answer["jsonrpc"], "2.0");
    assert_eq!(answer["id"], 1);
    let result = &answer["result"];
    assert!(
        result["protocolVersion"].is_string(),
        "the handshake names the protocol version: {answer}"
    );
    assert_eq!(
        result["serverInfo"]["name"], "figtether-mcp",
        "the handshake names the server: {answer}"
    );
    assert!(
        result["serverInfo"]["version"].is_string(),
        "the handshake names the version of the server: {answer}"
    );
    assert!(
        result["capabilities"]["tools"].is_object(),
        "the server advertises the tools capability: {answer}"
    );

    write_line(
        &mut writer,
        &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
    )
    .await;
    write_line(
        &mut writer,
        &json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
    )
    .await;

    let answer = read_line(&mut reader).await;
    assert_eq!(answer["id"], 2);
    let tools = answer["result"]["tools"]
        .as_array()
        .expect("the tool list is an array");
    assert_eq!(
        tools.len(),
        figtether_mcp::TOOL_NAMES.len(),
        "the surface holds exactly the twelve read only tools"
    );
    for tool in tools {
        let name = tool["name"].as_str().expect("the tool has a name");
        assert!(
            figtether_mcp::TOOL_NAMES.contains(&name),
            "the tool {name} is not part of the frozen surface"
        );
        assert_eq!(
            tool["annotations"]["readOnlyHint"],
            json!(true),
            "the tool {name} must be read only"
        );
        assert_eq!(
            tool["annotations"]["destructiveHint"],
            json!(false),
            "the tool {name} must not be destructive"
        );
    }

    drop(writer);
    drop(reader);
    let outcome = tokio::time::timeout(Duration::from_secs(20), handle)
        .await
        .expect("the server stops in time")
        .expect("the server task does not panic");
    assert!(outcome.is_ok(), "{outcome:?}");

    daemon.shutdown().await.expect("the daemon stops");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn at15_the_shipped_daemon_purges_the_cache_and_the_assets_on_revoke() {
    let daemon = ShippedDaemon::start()
        .await
        .expect("the shipped daemon starts");
    let corpus = Arc::new(RwLock::new(FixtureCorpus::small()));
    let root = corpus.read().await.root_id().to_string();
    let text_node = corpus
        .read()
        .await
        .first_text_node()
        .expect("the corpus carries a text node")
        .to_string();

    let secret = daemon
        .mint_pairing_secret()
        .await
        .expect("the daemon mints a pairing secret");
    let plugin = FakePlugin::connect(&daemon.bridge_url(), &secret, CLIENT_ID, corpus.clone())
        .await
        .expect("the plugin pairs");
    let mut client = connect_to_shipped(&daemon).await;
    grant_to_shipped_daemon(&plugin, &mut client, &root).await;

    // A read fills the design cache of the daemon.
    let mut cached = params::list_nodes(plugin.session_id(), &root);
    cached.consistency = figtether_model::tools::ConsistencyRequest::Cached;
    cached.max_age_ms = Some(60_000);
    let read = client.list_nodes(cached).await.expect("the read succeeds");
    assert!(!node_ids(&read).is_empty(), "the read returns nodes");
    let status = daemon.handle().status();
    assert!(
        status.cache_entries > 0,
        "the read has to fill the design cache of the daemon: \
         `crates/daemon/src/client_api.rs::run_job` never calls `Cache::put`, so the \
         cache of the daemon stays empty and a revoke has nothing to purge"
    );

    // An export fills the asset store of the daemon.
    let export = client
        .envelope(ClientRequest::ExportAssets {
            request_id: client.next_request_id(),
            params: serde_json::to_value(params::export_assets(
                plugin.session_id(),
                &[text_node.as_str()],
                AssetFormat::Png,
            ))
            .expect("the parameters serialize"),
        })
        .await
        .expect("the export succeeds");
    assert!(
        export.data.get("asset_id").is_some(),
        "the export mints an asset id"
    );
    let status = daemon.handle().status();
    assert!(
        status.asset_count > 0,
        "the export has to fill the asset store of the daemon: \
         `crates/daemon/src/client_api.rs::run_job` never calls `AssetStore::put`, so the \
         asset store of the daemon stays empty and a revoke has nothing to purge"
    );

    // The revoke drops the cache entries and the assets of the grant.
    client
        .revoke(plugin.session_id())
        .await
        .expect("the grant is revoked");
    let status = daemon.handle().status();
    assert_eq!(
        status.cache_entries, 0,
        "the revoke clears the cache of the grant"
    );
    assert_eq!(
        status.asset_count, 0,
        "the revoke clears the assets of the grant"
    );

    // A later read is refused.
    let error = client
        .error(ClientRequest::ListNodes {
            request_id: client.next_request_id(),
            params: serde_json::to_value(params::list_nodes(plugin.session_id(), &root))
                .expect("the parameters serialize"),
        })
        .await
        .expect("the revoked session is refused");
    assert_eq!(error.code, ErrorCode::Revoked);

    daemon.shutdown().await.expect("the daemon stops");
}

// ---------------------------------------------------------------------------
// Scenarios that still need a real editor
// ---------------------------------------------------------------------------

#[test]
#[ignore = "blocked: PRD 8.8 requires a real Figma Desktop run with the plugin installed, CI evidence is supporting only"]
fn at04_the_shipped_plugin_tracks_a_real_editor_edit() {
    panic!("blocked: the desktop protocol run is a manual gate");
}

// ---------------------------------------------------------------------------
// Helpers of the raw JSON-RPC lane
// ---------------------------------------------------------------------------

/// Write one JSON-RPC message on the pipe.
async fn write_line<W: tokio::io::AsyncWrite + Unpin>(writer: &mut W, value: &Value) {
    let mut line = serde_json::to_vec(value).expect("the message serializes");
    line.push(b'\n');
    writer
        .write_all(&line)
        .await
        .expect("the message is written");
    writer.flush().await.expect("the message is flushed");
}

/// Read one JSON-RPC message from the pipe.
async fn read_line<R: tokio::io::AsyncBufRead + Unpin>(reader: &mut R) -> Value {
    let mut line = String::new();
    let read = tokio::time::timeout(Duration::from_secs(30), reader.read_line(&mut line))
        .await
        .expect("the server answers in time")
        .expect("the answer is read");
    assert!(read > 0, "the server closed the connection");
    serde_json::from_str(line.trim()).expect("the answer is JSON")
}
