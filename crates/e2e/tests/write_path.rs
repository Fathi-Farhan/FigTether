//! The write path of the FigTether product.
//!
//! Every test in this file is named after the case it covers. The cases are
//! the ones of `docs/11-write-path/01-write-path-design.md` and of
//! `ADR-013`: the grant, the preview, the approval, the budget, the scope,
//! the destructive policy, and the audit.
//!
//! # What these tests are and what they are not
//!
//! These are **simulated editor tests**. The plugin is the fake plugin of
//! this crate and the document is the fixture corpus, so the tests prove the
//! contracts of the Rust side of the write path: the daemon refuses what it
//! has to refuse, the preview never mutates, the approval binds to one plan,
//! one session and one grant epoch, and the audit holds counts and no
//! content.
//!
//! They do **not** prove anything about Figma Desktop. The undo behaviour of
//! an applied plan is editor behaviour: the plugin calls `figma.commitUndo()`
//! once at the end of a plan, and only a real editor session can prove that
//! the plan is one step in the undo history of Figma. The same is true for
//! the mutation refusal cases of the editor API, for the font loading a text
//! change needs, and for the version history. Those cases need a real
//! Figma Desktop run with the plugin installed, and the tests here are
//! supporting evidence for them only.
//!
//! # Determinism rules
//!
//! - No test sleeps a fixed amount of time. Every state change is polled
//!   with a deadline.
//! - Every test cleans up after itself: the daemon of a test is stopped and
//!   its runtime directory is removed with the temporary directory that holds
//!   it.

use std::sync::Arc;
use std::time::{Duration, Instant};

use figtether_e2e::fake_client::FakeLocalClient;
use figtether_e2e::fake_plugin::{FakePlugin, SESSION_MODE_LIVE, SESSION_MODE_PAUSED};
use figtether_e2e::fixtures::FixtureCorpus;
use figtether_e2e::params;
use figtether_e2e::shipped::{ShippedDaemon, ShortLivedPairingDaemon};
use figtether_model::ids::SessionId;
use figtether_protocol::client::{ClientRequest, ClientResponse};
use figtether_protocol::write::{GRANT_MODE_READ, GRANT_MODE_WRITE};
use serde_json::{json, Value};
use tokio::sync::RwLock;

/// The client id of the plugin and of the client of a flow.
const CLIENT_ID: &str = "write-path-client";

/// The client id of the second client of the isolation flow.
const OTHER_CLIENT_ID: &str = "write-path-other-client";

/// The label of the first plugin of the isolation flow.
const PLUGIN_A_CLIENT_ID: &str = "write-path-plugin-a";

/// The label of the second plugin of the isolation flow.
const PLUGIN_B_CLIENT_ID: &str = "write-path-plugin-b";

/// How long a flow waits for a state change of the daemon.
const DEADLINE: Duration = Duration::from_secs(20);

/// A running daemon with a plugin, a client, and a document.
struct WriteHarness {
    daemon: ShippedDaemon,
    plugin: FakePlugin,
    client: FakeLocalClient,
    corpus: Arc<RwLock<FixtureCorpus>>,
    root_id: String,
}

impl WriteHarness {
    /// The session of the plugin.
    fn session_id(&self) -> SessionId {
        self.plugin.session_id()
    }

    /// The root the user granted.
    fn root_id(&self) -> &str {
        &self.root_id
    }
}

/// Start the shipped daemon, pair the plugin, grant the scope, and connect
/// the client.
///
/// The mode of the grant is a user decision: a read grant never authorizes a
/// write, and a write grant is a separate action in the plugin UI.
async fn write_harness(corpus: FixtureCorpus, mode: &str) -> WriteHarness {
    let daemon = ShippedDaemon::start().await.expect("the daemon starts");
    let secret = daemon
        .mint_pairing_secret()
        .await
        .expect("the daemon mints a pairing secret");
    let root_id = corpus.root_id().to_string();
    let corpus = Arc::new(RwLock::new(corpus));
    let plugin = FakePlugin::connect(&daemon.bridge_url(), &secret, CLIENT_ID, corpus.clone())
        .await
        .expect("the plugin pairs");
    let mut client = FakeLocalClient::connect(
        &daemon.client_url(),
        &daemon
            .client_credential()
            .expect("the daemon file holds the credential"),
        CLIENT_ID,
    )
    .await
    .expect("the client connects");
    let session_id = plugin.session_id();
    plugin
        .grant_with_mode(std::slice::from_ref(&root_id), mode)
        .await
        .expect("the daemon records the grant");
    wait_for_grant(&mut client, session_id, 1).await;
    WriteHarness {
        daemon,
        plugin,
        client,
        corpus,
        root_id,
    }
}

/// Wait until the daemon reports the grant of the session.
///
/// The daemon records the grant of a `grant_update` frame, so the harness
/// learns about the grant from the status of the session and not from the
/// bridge.
async fn wait_for_grant(client: &mut FakeLocalClient, session_id: SessionId, root_count: usize) {
    let deadline = Instant::now() + DEADLINE;
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
                return;
            }
        }
        assert!(
            Instant::now() < deadline,
            "the daemon did not record the grant of the session {session_id}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Let the daemon learn the shape of the granted tree.
///
/// The daemon checks the scope of a plan against the tree it has proven to be
/// inside the grant, so a flow that writes into a subtree reads the scope
/// before it writes into it.
async fn learn_scope(client: &mut FakeLocalClient, session_id: SessionId, root_id: &str) {
    let request_id = client.next_request_id();
    client
        .request(ClientRequest::ListNodes {
            request_id,
            params: serde_json::to_value(params::list_nodes(session_id, root_id))
                .expect("the parameters serialize"),
        })
        .await
        .expect("the daemon reads the scope");
}

/// A plan that creates one frame below the granted root.
fn create_frame_plan(session_id: &SessionId, parent_id: &str, plan_id: &str) -> Value {
    json!({
        "plan_id": plan_id,
        "session_id": session_id.to_string(),
        "intent": "add one frame below the granted root",
        "destructive": false,
        "operations": [
            { "kind": "create_node", "node_type": "FRAME", "parent_id": parent_id }
        ],
    })
}

/// A plan that replaces the characters of a text node.
fn set_text_plan(session_id: &SessionId, node_id: &str, text: &str, plan_id: &str) -> Value {
    json!({
        "plan_id": plan_id,
        "session_id": session_id.to_string(),
        "intent": "write a new label into the text node",
        "destructive": false,
        "operations": [
            { "kind": "set_text", "node_id": node_id, "text": text }
        ],
    })
}

/// A plan that removes a node.
fn delete_plan(session_id: &SessionId, node_id: &str, destructive: bool, plan_id: &str) -> Value {
    json!({
        "plan_id": plan_id,
        "session_id": session_id.to_string(),
        "intent": "remove the node the user does not want any more",
        "destructive": destructive,
        "operations": [
            { "kind": "delete_node", "node_id": node_id }
        ],
    })
}

/// Ask the daemon to preview a plan.
async fn preview(client: &mut FakeLocalClient, plan: &Value) -> ClientResponse {
    let request_id = client.next_request_id();
    client
        .request(ClientRequest::PreviewWrite {
            request_id,
            params: json!({ "plan": plan.clone() }),
        })
        .await
        .expect("the daemon answers the preview")
}

/// Ask the daemon to apply a plan.
async fn apply(client: &mut FakeLocalClient, plan: &Value, token: Option<&str>) -> ClientResponse {
    let request_id = client.next_request_id();
    let mut params = json!({ "plan": plan.clone() });
    if let Some(token) = token {
        params["approval_token"] = Value::String(token.to_string());
    }
    client
        .request(ClientRequest::ApplyWrite { request_id, params })
        .await
        .expect("the daemon answers the apply")
}

/// Ask the daemon about an approval.
async fn approval(client: &mut FakeLocalClient, approval_id: &str) -> ClientResponse {
    let request_id = client.next_request_id();
    client
        .request(ClientRequest::GetWriteApproval {
            request_id,
            params: json!({ "approval_id": approval_id }),
        })
        .await
        .expect("the daemon answers the approval question")
}

/// The payload of a successful answer.
fn data_of(response: &ClientResponse) -> &Value {
    match response {
        ClientResponse::Ok { envelope, .. } => &envelope.data,
        other => panic!("the daemon must answer the request, got {other:?}"),
    }
}

/// Assert that the daemon refused the request with the given code.
fn assert_refused(response: &ClientResponse, code: &str) {
    match response {
        ClientResponse::Error { error, .. } => assert_eq!(
            error.code.as_str(),
            code,
            "the daemon refused the request with a different code: {error}"
        ),
        other => panic!("the daemon must refuse the request, got {other:?}"),
    }
}

/// The identity of the approval the daemon minted for a preview.
fn approval_id_of(response: &ClientResponse) -> String {
    data_of(response)
        .get("approval_id")
        .and_then(Value::as_str)
        .expect("the preview names the approval")
        .to_string()
}

/// The hash of the plan the daemon previewed.
fn plan_hash_of(response: &ClientResponse) -> String {
    data_of(response)
        .get("plan_hash")
        .and_then(Value::as_str)
        .expect("the preview names the hash of the plan")
        .to_string()
}

/// Let the user approve the plan the plugin previewed.
///
/// The plugin reports the decision with the identity of the approval request
/// and the hash of the plan it showed, the way the plugin UI reports it.
async fn approve(plugin: &FakePlugin) {
    let preview = plugin
        .last_preview()
        .await
        .expect("the plugin previewed the plan");
    let approval_id = preview
        .approval_id
        .clone()
        .expect("the daemon named the approval request");
    plugin
        .report_approval(&approval_id, &preview.plan_hash, true, None)
        .await
        .expect("the plugin reports the decision");
}

/// Wait until the user approved the plan and return the token.
///
/// The decision of the user reaches the daemon asynchronously, so the client
/// polls the approval until it carries the token.
async fn await_token(client: &mut FakeLocalClient, approval_id: &str) -> String {
    let deadline = Instant::now() + DEADLINE;
    loop {
        let response = approval(client, approval_id).await;
        let data = data_of(&response);
        if data.get("status").and_then(Value::as_str) == Some("approved") {
            return data
                .get("approval_token")
                .and_then(Value::as_str)
                .expect("an approved plan carries a token")
                .to_string();
        }
        assert!(
            Instant::now() < deadline,
            "the user did not approve the plan {approval_id} in time"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

// ---------------------------------------------------------------------------
// The grant of the session
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_write_plan_on_a_read_grant_is_refused_before_the_plugin_is_asked() {
    let mut harness =
        write_harness(FixtureCorpus::labelled("FX-WRITE-A", 40), GRANT_MODE_READ).await;
    let session_id = harness.session_id();
    let root_id = harness.root_id().to_string();
    let plan = create_frame_plan(&session_id, &root_id, "plan_read_grant");

    let response = preview(&mut harness.client, &plan).await;

    assert_refused(&response, "WRITE_NOT_GRANTED");
    assert_eq!(
        harness.plugin.previews_handled(),
        0,
        "the plugin was never asked to preview the plan"
    );
    assert_eq!(
        harness.plugin.applies_handled(),
        0,
        "the plugin was never asked to apply the plan"
    );
}

// ---------------------------------------------------------------------------
// The preview
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_preview_leaves_the_document_unchanged() {
    let mut harness =
        write_harness(FixtureCorpus::labelled("FX-WRITE-B", 40), GRANT_MODE_WRITE).await;
    let session_id = harness.session_id();
    let root_id = harness.root_id().to_string();
    let before = harness.corpus.read().await.hash();

    let plan = create_frame_plan(&session_id, &root_id, "plan_preview");
    let response = preview(&mut harness.client, &plan).await;
    let data = data_of(&response);
    let preview = data
        .get("preview")
        .expect("the answer carries the preview of the plan");

    assert_eq!(
        preview["blocked"], false,
        "the plan can be applied as written"
    );
    assert_eq!(preview["operations_count"], 1);
    assert_eq!(
        preview["created"].as_array().map(Vec::len),
        Some(1),
        "the preview names the node the plan would create"
    );
    assert_eq!(
        preview["warnings"].as_array().map(Vec::len),
        Some(0),
        "the preview carries no warning"
    );

    let created_id = preview["created"][0]
        .as_str()
        .expect("the created node has an id")
        .to_string();
    let corpus = harness.corpus.read().await;
    assert_eq!(before, corpus.hash(), "the preview changed the document");
    assert!(
        corpus.node(&created_id).is_none(),
        "the preview created the node {created_id}"
    );
    assert_eq!(
        harness.plugin.applies_handled(),
        0,
        "the preview applied nothing"
    );
}

// ---------------------------------------------------------------------------
// The approval
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_apply_without_an_approval_token_is_refused() {
    let mut harness =
        write_harness(FixtureCorpus::labelled("FX-WRITE-C", 40), GRANT_MODE_WRITE).await;
    let session_id = harness.session_id();
    let root_id = harness.root_id().to_string();
    let before = harness.corpus.read().await.hash();
    let plan = create_frame_plan(&session_id, &root_id, "plan_no_token");

    let response = preview(&mut harness.client, &plan).await;
    let approval_id = approval_id_of(&response);
    approve(&harness.plugin).await;

    let response = apply(&mut harness.client, &plan, None).await;

    assert_refused(&response, "APPROVAL_REQUIRED");
    assert_eq!(
        harness.corpus.read().await.hash(),
        before,
        "the refused apply changed the document"
    );
    let status_response = approval(&mut harness.client, &approval_id).await;
    let status = data_of(&status_response);
    assert_eq!(
        status["status"], "approved",
        "the user did approve the plan, the apply was still refused"
    );
}

#[tokio::test]
async fn an_apply_with_the_token_of_another_plan_is_refused() {
    let mut harness =
        write_harness(FixtureCorpus::labelled("FX-WRITE-D", 40), GRANT_MODE_WRITE).await;
    let session_id = harness.session_id();
    let root_id = harness.root_id().to_string();
    let before = harness.corpus.read().await.hash();

    let approved_plan = create_frame_plan(&session_id, &root_id, "plan_approved");
    let response = preview(&mut harness.client, &approved_plan).await;
    let approval_id = approval_id_of(&response);
    approve(&harness.plugin).await;
    let token = await_token(&mut harness.client, &approval_id).await;

    // The plan the agent sends now is not the plan the user approved.
    let other_plan = create_frame_plan(&session_id, &root_id, "plan_other");
    let response = apply(&mut harness.client, &other_plan, Some(&token)).await;

    assert_refused(&response, "PLAN_HASH_MISMATCH");
    assert_eq!(
        harness.corpus.read().await.hash(),
        before,
        "the refused apply changed the document"
    );
    assert_eq!(
        harness.plugin.applies_handled(),
        0,
        "the refused apply reached the plugin"
    );
}

#[tokio::test]
async fn an_apply_after_the_grant_moved_is_refused() {
    let mut harness =
        write_harness(FixtureCorpus::labelled("FX-WRITE-E", 40), GRANT_MODE_WRITE).await;
    let session_id = harness.session_id();
    let root_id = harness.root_id().to_string();
    let before = harness.corpus.read().await.hash();
    let plan = create_frame_plan(&session_id, &root_id, "plan_moved_grant");

    let response = preview(&mut harness.client, &plan).await;
    let approval_id = approval_id_of(&response);
    approve(&harness.plugin).await;
    let token = await_token(&mut harness.client, &approval_id).await;

    // The user changes the grant. The scope moves, and so does the epoch the
    // approval was bound to.
    harness
        .plugin
        .grant_with_mode(std::slice::from_ref(&root_id), GRANT_MODE_WRITE)
        .await
        .expect("the grant moves");

    let response = apply(&mut harness.client, &plan, Some(&token)).await;

    assert_refused(&response, "APPROVAL_EXPIRED");
    assert_eq!(
        harness.corpus.read().await.hash(),
        before,
        "the refused apply changed the document"
    );
    assert_eq!(
        harness.plugin.applies_handled(),
        0,
        "the refused apply reached the plugin"
    );
}

// ---------------------------------------------------------------------------
// The budget, the scope, and the destructive policy
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_plan_over_the_operation_budget_is_refused() {
    let mut harness =
        write_harness(FixtureCorpus::labelled("FX-WRITE-F", 40), GRANT_MODE_WRITE).await;
    let session_id = harness.session_id();
    let root_id = harness.root_id().to_string();
    // The budget allows fifty operations per plan, so a hundred and twenty
    // operations are over it whatever the installation configured.
    let operations: Vec<Value> = (0..120)
        .map(|_| json!({ "kind": "create_node", "node_type": "FRAME", "parent_id": root_id }))
        .collect();
    let plan = json!({
        "plan_id": "plan_budget",
        "session_id": session_id.to_string(),
        "intent": "change far too much at once",
        "destructive": false,
        "operations": operations,
    });

    let response = preview(&mut harness.client, &plan).await;

    assert_refused(&response, "BUDGET_EXCEEDED");
    assert_eq!(
        harness.plugin.previews_handled(),
        0,
        "the daemon refused the plan before the plugin saw it"
    );
}

#[tokio::test]
async fn a_plan_that_names_a_node_outside_the_grant_is_refused() {
    let mut harness =
        write_harness(FixtureCorpus::labelled("FX-WRITE-G", 40), GRANT_MODE_WRITE).await;
    let session_id = harness.session_id();
    let root_id = harness.root_id().to_string();
    // The daemon checks the scope of a plan against the tree it has proven to
    // be inside the grant, so the flow reads the scope first.
    learn_scope(&mut harness.client, session_id, &root_id).await;

    // The user narrows the grant to one child of the root. The sibling of
    // that child lives in the same document and stays outside the scope.
    let children: Vec<String> = harness
        .corpus
        .read()
        .await
        .descendants(&root_id, 1)
        .into_iter()
        .map(|node| node.id)
        .collect();
    let granted = children.first().expect("the root has a child").clone();
    let outside = children
        .get(1)
        .expect("the root has a second child")
        .clone();
    harness
        .plugin
        .grant_with_mode(std::slice::from_ref(&granted), GRANT_MODE_WRITE)
        .await
        .expect("the grant moves");
    wait_for_grant(&mut harness.client, session_id, 1).await;

    let plan = set_text_plan(&session_id, &outside, "outside the grant", "plan_scope");

    let response = preview(&mut harness.client, &plan).await;

    assert_refused(&response, "OUT_OF_SCOPE");
    assert_eq!(
        harness.plugin.previews_handled(),
        0,
        "the daemon refused the plan before the plugin saw it"
    );
}

#[tokio::test]
async fn a_destructive_plan_without_the_destructive_flag_is_refused() {
    let mut harness =
        write_harness(FixtureCorpus::labelled("FX-WRITE-H", 40), GRANT_MODE_WRITE).await;
    let session_id = harness.session_id();
    let root_id = harness.root_id().to_string();
    learn_scope(&mut harness.client, session_id, &root_id).await;
    let text_node = harness
        .corpus
        .read()
        .await
        .first_text_node()
        .expect("the corpus holds a text node")
        .to_string();

    let plan = delete_plan(&session_id, &text_node, false, "plan_destructive");

    let response = preview(&mut harness.client, &plan).await;

    assert_refused(&response, "DESTRUCTIVE_NOT_ALLOWED");
    assert_eq!(
        harness.plugin.previews_handled(),
        0,
        "the daemon refused the plan before the plugin saw it"
    );
    assert!(
        harness.corpus.read().await.node(&text_node).is_some(),
        "the refused plan removed the node"
    );
}

// ---------------------------------------------------------------------------
// The apply and the audit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn an_approved_plan_reaches_the_plugin_and_the_daemon_audits_it() {
    // The daemon keeps the audit trail in its own state and does not expose
    // it over the wire, so the flow reads the state of the running daemon.
    let daemon = ShortLivedPairingDaemon::start(Duration::from_secs(300))
        .await
        .expect("the daemon starts");
    let secret = daemon
        .mint_pairing_secret()
        .await
        .expect("the daemon mints a pairing secret");
    let corpus = Arc::new(RwLock::new(FixtureCorpus::labelled("FX-WRITE-I", 40)));
    let root_id = corpus.read().await.root_id().to_string();
    let plugin = FakePlugin::connect(&daemon.bridge_url(), &secret, CLIENT_ID, corpus.clone())
        .await
        .expect("the plugin pairs");
    let mut client =
        FakeLocalClient::connect(&daemon.client_url(), daemon.client_credential(), CLIENT_ID)
            .await
            .expect("the client connects");
    let session_id = plugin.session_id();
    plugin
        .grant_with_mode(std::slice::from_ref(&root_id), GRANT_MODE_WRITE)
        .await
        .expect("the daemon records the grant");
    wait_for_grant(&mut client, session_id, 1).await;

    let before = corpus.read().await.hash();
    let plan = create_frame_plan(&session_id, &root_id, "plan_approved_apply");
    let response = preview(&mut client, &plan).await;
    let approval_id = approval_id_of(&response);
    let plan_hash = plan_hash_of(&response);
    approve(&plugin).await;
    let token = await_token(&mut client, &approval_id).await;

    let response = apply(&mut client, &plan, Some(&token)).await;
    let data = data_of(&response);
    let result = data
        .get("result")
        .expect("the answer carries the result of the plan");
    let created = result["created"]
        .as_array()
        .expect("the result names the created nodes");
    assert_eq!(created.len(), 1, "the plan created one node");
    let created_id = created[0]
        .as_str()
        .expect("the created node has an id")
        .to_string();
    assert_eq!(result["undo_committed"], true);
    assert_eq!(
        plugin.applies_handled(),
        1,
        "the approved plan reached the plugin"
    );

    let corpus_after = corpus.read().await;
    assert!(
        corpus_after.node(&created_id).is_some(),
        "the created node {created_id} is in the document"
    );
    assert_ne!(
        before,
        corpus_after.hash(),
        "the apply changed the document"
    );
    drop(corpus_after);

    // The daemon holds an audit record of the applied plan. The record
    // carries counts and hashes, never design content.
    let records = daemon.write_audit_records();
    assert_eq!(records.len(), 1, "the daemon holds one audit record");
    let record = &records[0];
    assert_eq!(record.plan_id, "plan_approved_apply");
    assert_eq!(record.plan_hash, plan_hash);
    assert_eq!(record.approval_id, approval_id);
    assert_eq!(record.operations_count, 1);
    assert_eq!(record.created, 1);
    assert_eq!(record.changed, 0);
    assert_eq!(record.removed, 0);
    assert_eq!(record.moved, 0);
    let recorded = format!("{record:?}");
    assert!(
        !recorded.contains("add one frame below the granted root"),
        "the audit record holds the intent of the plan: {recorded}"
    );
    assert!(
        !recorded.contains("created node"),
        "the audit record holds a layer name: {recorded}"
    );
}

// ---------------------------------------------------------------------------
// The isolation between two sessions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn two_sessions_with_the_same_node_ids_cannot_write_into_each_other_scope() {
    let daemon = ShippedDaemon::start().await.expect("the daemon starts");
    let secret_a = daemon
        .mint_pairing_secret()
        .await
        .expect("the daemon mints a pairing secret");
    let secret_b = daemon
        .mint_pairing_secret()
        .await
        .expect("the daemon mints a pairing secret");
    // Two documents that share every node id, the way two tabs of two files
    // happen to use the same identifiers.
    let corpus_a = Arc::new(RwLock::new(FixtureCorpus::labelled("FX-FILE-A", 40)));
    let corpus_b = Arc::new(RwLock::new(FixtureCorpus::labelled("FX-FILE-B", 40)));
    // The two plugins are two tabs, so they carry two labels: a label names
    // the plugin UI, and one UI cannot be two sessions.
    let plugin_a = FakePlugin::connect(
        &daemon.bridge_url(),
        &secret_a,
        PLUGIN_A_CLIENT_ID,
        corpus_a.clone(),
    )
    .await
    .expect("the first plugin pairs");
    let plugin_b = FakePlugin::connect(
        &daemon.bridge_url(),
        &secret_b,
        PLUGIN_B_CLIENT_ID,
        corpus_b.clone(),
    )
    .await
    .expect("the second plugin pairs");
    // The first session may write below the root, the second one only below
    // one of its children.
    plugin_a
        .grant_with_mode(&["1:2".to_string()], GRANT_MODE_WRITE)
        .await
        .expect("the first grant is accepted");
    plugin_b
        .grant_with_mode(&["1:3".to_string()], GRANT_MODE_WRITE)
        .await
        .expect("the second grant is accepted");
    let mut client = FakeLocalClient::connect(
        &daemon.client_url(),
        &daemon
            .client_credential()
            .expect("the daemon file holds the credential"),
        CLIENT_ID,
    )
    .await
    .expect("the client connects");
    wait_for_grant(&mut client, plugin_a.session_id(), 1).await;
    wait_for_grant(&mut client, plugin_b.session_id(), 1).await;

    // The node the first session may write into is outside the scope of the
    // second one, even though both documents use the same identifiers.
    let plan_for_a = create_frame_plan(&plugin_a.session_id(), "1:2", "plan_session_a");
    let response = preview(&mut client, &plan_for_a).await;
    assert!(
        matches!(response, ClientResponse::Ok { .. }),
        "the first session may write into its own scope, got {response:?}"
    );
    let approval_id = approval_id_of(&response);

    let plan_for_b = create_frame_plan(&plugin_b.session_id(), "1:2", "plan_session_b");
    let response = preview(&mut client, &plan_for_b).await;
    assert_refused(&response, "OUT_OF_SCOPE");
    assert_eq!(
        plugin_b.previews_handled(),
        0,
        "the daemon refused the plan before the plugin saw it"
    );

    // The approval of the first session does not open the second one.
    let before_a = corpus_a.read().await.hash();
    let before_b = corpus_b.read().await.hash();
    approve(&plugin_a).await;
    let token = await_token(&mut client, &approval_id).await;

    let plan_inside_b = create_frame_plan(&plugin_b.session_id(), "1:3", "plan_inside_b");
    let response = apply(&mut client, &plan_inside_b, Some(&token)).await;
    assert_refused(&response, "APPROVAL_REQUIRED");
    assert_eq!(
        plugin_b.applies_handled(),
        0,
        "the refused apply reached the plugin of the second session"
    );
    assert_eq!(
        corpus_b.read().await.hash(),
        before_b,
        "the refused apply changed the second document"
    );
    assert_eq!(
        corpus_a.read().await.hash(),
        before_a,
        "the refused apply changed the first document"
    );
}

// ---------------------------------------------------------------------------
// The answer of the approval
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_approval_answer_reaches_only_the_client_that_asked_for_the_preview() {
    let mut harness =
        write_harness(FixtureCorpus::labelled("FX-WRITE-J", 40), GRANT_MODE_WRITE).await;
    let session_id = harness.session_id();
    let root_id = harness.root_id().to_string();
    // A second client of the same installation.
    let mut other = FakeLocalClient::connect(
        &harness.daemon.client_url(),
        &harness
            .daemon
            .client_credential()
            .expect("the daemon file holds the credential"),
        OTHER_CLIENT_ID,
    )
    .await
    .expect("the second client connects");

    let plan = create_frame_plan(&session_id, &root_id, "plan_private_approval");
    let response = preview(&mut harness.client, &plan).await;
    let approval_id = approval_id_of(&response);
    approve(&harness.plugin).await;

    // The client that did not ask for the preview learns nothing.
    let response = approval(&mut other, &approval_id).await;
    match &response {
        ClientResponse::Error { error, .. } => assert_eq!(
            error.code.as_str(),
            "PERMISSION_DENIED",
            "the daemon refused the other client with a different code: {error}"
        ),
        ClientResponse::Ok { envelope, .. } => {
            assert_ne!(
                envelope.data.get("status").and_then(Value::as_str),
                Some("approved"),
                "the other client learned the decision of the user"
            );
            assert!(
                envelope.data.get("approval_token").is_none(),
                "the other client learned the token"
            );
        }
        other => panic!("the daemon must answer the approval question, got {other:?}"),
    }

    // The client that asked for the preview learns the decision and the
    // token.
    let token = await_token(&mut harness.client, &approval_id).await;
    assert!(
        token.starts_with("w:"),
        "the approval token has the documented shape, got {token}"
    );
    let reported = harness.plugin.approvals_reported().await;
    assert_eq!(reported.len(), 1, "the plugin reported one decision");
    assert!(reported[0].approved);
    assert_eq!(reported[0].approval_id, approval_id);
}

// ---------------------------------------------------------------------------
// The session mode
// ---------------------------------------------------------------------------

#[tokio::test]
async fn the_session_mode_pauses_and_resumes_the_session() {
    let mut harness =
        write_harness(FixtureCorpus::labelled("FX-WRITE-K", 40), GRANT_MODE_WRITE).await;
    let session_id = harness.session_id();
    let root_id = harness.root_id().to_string();
    let plan = create_frame_plan(&session_id, &root_id, "plan_paused");

    // The user pauses the session in the plugin UI. The daemon is the
    // authority on the pause, so the session serves nothing from now on.
    harness
        .plugin
        .set_session_mode(SESSION_MODE_PAUSED)
        .await
        .expect("the session is paused");
    assert_eq!(harness.plugin.session_mode().await, SESSION_MODE_PAUSED);

    let response = preview(&mut harness.client, &plan).await;
    assert_refused(&response, "PAUSED");
    assert_eq!(
        harness.plugin.previews_handled(),
        0,
        "the plugin was never asked to preview the plan"
    );

    // The user resumes the session.
    harness
        .plugin
        .set_session_mode(SESSION_MODE_LIVE)
        .await
        .expect("the session is resumed");
    assert_eq!(harness.plugin.session_mode().await, SESSION_MODE_LIVE);

    let response = preview(&mut harness.client, &plan).await;
    assert!(
        matches!(response, ClientResponse::Ok { .. }),
        "the resumed session writes again: {response:?}"
    );
}

// ---------------------------------------------------------------------------
// The audit trail
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_client_reads_the_audit_records_of_its_own_installation() {
    let mut harness =
        write_harness(FixtureCorpus::labelled("FX-WRITE-L", 40), GRANT_MODE_WRITE).await;
    let session_id = harness.session_id();
    let root_id = harness.root_id().to_string();
    let plan = create_frame_plan(&session_id, &root_id, "plan_audited");

    let response = preview(&mut harness.client, &plan).await;
    let approval_id = approval_id_of(&response);
    let plan_hash = plan_hash_of(&response);
    approve(&harness.plugin).await;
    let token = await_token(&mut harness.client, &approval_id).await;

    // The plugin reports the decision with the hash the daemon computed.
    let reported = harness.plugin.approvals_reported().await;
    assert_eq!(reported.len(), 1, "the plugin reported one decision");
    assert_eq!(reported[0].plan_hash, plan_hash);

    let response = apply(&mut harness.client, &plan, Some(&token)).await;
    assert!(
        matches!(response, ClientResponse::Ok { .. }),
        "the approved plan is applied: {response:?}"
    );

    // The audit trail of the daemon is observable by the client. The record
    // carries identifiers, counts, and hashes, never design content.
    let request_id = harness.client.next_request_id();
    let response = harness
        .client
        .request(ClientRequest::Status { request_id })
        .await
        .expect("the daemon answers the status request");
    let data = data_of(&response);
    let audit = data["write_audit"]
        .as_array()
        .expect("the status carries the audit trail")
        .clone();
    assert_eq!(audit.len(), 1, "{audit:?}");
    assert_eq!(audit[0]["plan_id"], "plan_audited", "{audit:?}");
    assert_eq!(audit[0]["approval_id"], approval_id, "{audit:?}");
    assert_eq!(audit[0]["plan_hash"], plan_hash, "{audit:?}");
    assert_eq!(audit[0]["operations_count"], 1, "{audit:?}");
    assert_eq!(audit[0]["created"], 1, "{audit:?}");
    let rendered = audit[0].to_string();
    assert!(
        !rendered.contains("add one frame below the granted root"),
        "the audit record holds the intent of the plan: {rendered}"
    );
}
