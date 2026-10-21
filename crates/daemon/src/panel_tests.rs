use super::*;
use crate::{bridge, ResourceLimits, Secret};
use axum::{
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode},
    Json,
};
use chrono::Utc;
use figtether_model::ids::SessionId;
use figtether_protocol::BridgeFrame;
use serde_json::json;
use std::collections::BTreeMap;
use tokio::sync::mpsc;
use uuid::Uuid;

fn connected() -> (Arc<DaemonState>, SessionId, mpsc::Receiver<BridgeFrame>) {
    let state = Arc::new(DaemonState::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        38947,
        ResourceLimits::default(),
        Secret::from_value("test"),
    ));
    let session: SessionId = "11111111-1111-4111-8111-111111111111".parse().unwrap();
    let epoch = state.hub.epochs().bump(&session);
    let (tx, rx) = mpsc::channel(16);
    state.hub.attach(
        session,
        bridge::SessionLink {
            epoch,
            client_id: "test".into(),
            credential: Secret::from_value("test"),
            tx,
            capabilities: BTreeMap::new(),
        },
    );
    (state, session, rx)
}
fn headers(url: &str) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {}", url.split('#').nth(1).unwrap())).unwrap(),
    );
    h.insert("origin", HeaderValue::from_static("http://127.0.0.1:38947"));
    h
}

#[tokio::test]
async fn companion_requires_its_own_capability_and_exact_origin() {
    let (state, session, _rx) = connected();
    assert_eq!(
        snapshot(State(state.clone()), HeaderMap::new())
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    let url = state.panels.open(session, 1, 38947).unwrap();
    assert_eq!(
        snapshot(State(state.clone()), headers(&url)).await.status(),
        StatusCode::OK
    );
    let mut h = headers(&url);
    h.insert("origin", HeaderValue::from_static("https://example.com"));
    assert_eq!(
        snapshot(State(state), h).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn reopening_rotates_capabilities_and_disconnect_revokes_them() {
    let (state, session, _rx) = connected();
    let old = state.panels.open(session, 1, 38947).unwrap();
    let fresh = state.panels.open(session, 1, 38947).unwrap();
    assert_eq!(
        snapshot(State(state.clone()), headers(&old)).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        snapshot(State(state.clone()), headers(&fresh))
            .await
            .status(),
        StatusCode::OK
    );
    state.hub.detach(&session);
    assert_eq!(
        snapshot(State(state), headers(&fresh)).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test]
async fn companion_commands_are_allowlisted_and_scope_epoch_bound() {
    let (state, session, mut rx) = connected();
    let url = state.panels.open(session, 1, 38947).unwrap();
    state
        .panels
        .publish(session, 1, &json!({"model":{"grantEpoch":2},"logs":[]}));
    let response = action(
        State(state.clone()),
        headers(&url),
        Json(json!({"command":"approve","grant_epoch":1})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert!(rx.try_recv().is_err());
    // Different fresh capability avoids the per-window rapid-action limit.
    let url = state.panels.open(session, 1, 38947).unwrap();
    state
        .panels
        .publish(session, 1, &json!({"model":{"grantEpoch":2},"logs":[]}));
    let response = action(
        State(state.clone()),
        headers(&url),
        Json(json!({"command":"pair","grant_epoch":2})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(rx.try_recv().is_err());
    let url = state.panels.open(session, 1, 38947).unwrap();
    state
        .panels
        .publish(session, 1, &json!({"model":{"grantEpoch":2},"logs":[]}));
    let response = action(
        State(state),
        headers(&url),
        Json(json!({"command":"refresh"})),
    )
    .await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(rx.try_recv().unwrap().payload["command"], "refresh");
}

#[tokio::test]
async fn companion_is_rejected_after_reconnect_or_revoke() {
    for reconnect in [true, false] {
        let (state, session, _rx) = connected();
        let url = state.panels.open(session, 1, 38947).unwrap();
        if reconnect {
            state.hub.epochs().bump(&session);
        } else {
            state.hub.revoke(&session);
        }
        assert_eq!(
            snapshot(State(state), headers(&url)).await.status(),
            StatusCode::UNAUTHORIZED
        );
    }
}

#[tokio::test]
async fn expired_companion_is_refused_without_waiting_in_real_time() {
    let (state, session, _rx) = connected();
    let url = state.panels.open(session, 1, 38947).unwrap();
    state
        .panels
        .0
        .lock()
        .unwrap()
        .get_mut(&session)
        .unwrap()
        .expires = Instant::now() - Duration::from_secs(1);
    assert_eq!(
        snapshot(State(state), headers(&url)).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[test]
fn automatic_approval_requires_warning_free_editor_evidence_in_the_daemon() {
    use crate::write_path::{ApprovalRequest, WritePath};
    let session: SessionId = "11111111-1111-4111-8111-111111111111".parse().unwrap();
    let other: SessionId = "22222222-2222-4222-8222-222222222222".parse().unwrap();
    let path = WritePath::new(ResourceLimits::default());
    let now = Utc::now();
    path.record(ApprovalRequest::pending(
        "a",
        session,
        "hash",
        1,
        now,
        now + chrono::Duration::minutes(5),
        "test",
        "test",
        json!({"operations":[{"kind":"create_node"}]}),
    ));
    assert!(path.may_decide(&session, "a", "hash", false));
    assert!(!path.may_decide(&session, "a", "hash", true));
    let good = json!({"preview":{"plan_hash":"hash","blocked":false,"warnings":[],"removed":[],"operations_count":1}});
    path.observe_preview(&session, &good);
    assert!(path.may_decide(&session, "a", "hash", true));
    assert!(!path.may_decide(&other, "a", "hash", false));
    for (field, value) in [
        ("warnings", json!(["font missing"])),
        ("warnings", json!(null)),
        ("removed", json!(["1:8"])),
        ("blocked", json!(true)),
        ("operations_count", json!(0)),
        ("reasons", json!(["review"])),
    ] {
        let mut bad = good.clone();
        bad["preview"][field] = value;
        path.observe_preview(&session, &bad);
        assert!(!path.may_decide(&session, "a", "hash", true), "{field}");
    }
}
