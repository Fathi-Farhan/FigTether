//! Local user companion. Credentials are session/connection bound, memory-only,
//! expire after 30 minutes, and are never available through MCP discovery.

use crate::{
    secrets::{constant_time_eq, Secret},
    DaemonState,
};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    Json,
};
use figtether_model::ids::SessionId;
use figtether_protocol::{BridgeFrame, MessageType};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const TTL: Duration = Duration::from_secs(30 * 60);

#[cfg(test)]
#[path = "panel_tests.rs"]
mod tests;

struct Panel {
    token: Secret,
    epoch: u64,
    expires: Instant,
    snapshot: Value,
    last_action: Option<Instant>,
}

/// One user-opened companion per plugin session, at most four per daemon.
#[derive(Default)]
pub struct PanelHub(Mutex<HashMap<SessionId, Panel>>);

impl PanelHub {
    /// Revoke browser capabilities when the owning plugin disconnects.
    pub fn invalidate(&self, session: &SessionId) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(session);
    }
    /// Explicit plugin-UI action rotates any prior browser capability.
    pub fn open(&self, session: SessionId, epoch: u64, port: u16) -> Option<String> {
        let mut panels = self.0.lock().unwrap_or_else(|e| e.into_inner());
        panels.retain(|_, p| p.expires > Instant::now());
        if panels.len() >= 4 && !panels.contains_key(&session) {
            return None;
        }
        let token = Secret::generate();
        let url = format!("http://127.0.0.1:{port}/panel#{}", token.as_str());
        panels.insert(
            session,
            Panel {
                token,
                epoch,
                expires: Instant::now() + TTL,
                snapshot: Value::Null,
                last_action: None,
            },
        );
        Some(url)
    }

    /// Publish only for an already opened companion, never create one implicitly.
    pub fn publish(&self, session: SessionId, epoch: u64, payload: &Value) {
        let mut panels = self.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(panel) = panels.get_mut(&session) {
            if panel.epoch == epoch
                && panel.expires > Instant::now()
                && serde_json::to_vec(payload).is_ok_and(|v| v.len() <= 96 * 1024)
            {
                panel.snapshot = json!({"model":payload.get("model"),"logs":payload.get("logs")});
            }
        }
    }

    fn authorize(
        &self,
        token: &str,
        state: &DaemonState,
        action: bool,
    ) -> Option<(SessionId, Value)> {
        let mut panels = self.0.lock().unwrap_or_else(|e| e.into_inner());
        panels.retain(|session, p| {
            p.expires > Instant::now()
                && state.hub.epochs().current(session) == Some(p.epoch)
                && state.hub.link(session).is_some()
                && !state.hub.is_revoked(session)
        });
        for (session, panel) in panels.iter_mut() {
            if constant_time_eq(panel.token.as_str(), token) {
                if action {
                    let now = Instant::now();
                    if panel
                        .last_action
                        .is_some_and(|last| now.duration_since(last) < Duration::from_millis(100))
                    {
                        return None;
                    }
                    panel.last_action = Some(now);
                }
                return Some((*session, panel.snapshot.clone()));
            }
        }
        None
    }
}

fn capability(headers: &HeaderMap, port: u16) -> Option<&str> {
    if let Some(origin) = headers.get("origin") {
        if origin.to_str().ok()? != format!("http://127.0.0.1:{port}") {
            return None;
        }
    }
    headers
        .get("authorization")?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

fn protected(body: impl IntoResponse) -> Response {
    let mut response = body.into_response();
    let h = response.headers_mut();
    h.insert("cache-control", "no-store".parse().unwrap());
    h.insert("referrer-policy", "no-referrer".parse().unwrap());
    h.insert("x-content-type-options", "nosniff".parse().unwrap());
    h.insert("content-security-policy", "default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'".parse().unwrap());
    response
}

/// Static shell contains no credential or session data.
pub async fn page() -> Response {
    protected(Html(include_str!("panel.html")))
}
/// Browser code uses an in-memory fragment capability, not installation credentials.
pub async fn script() -> Response {
    protected((
        [("content-type", "text/javascript; charset=utf-8")],
        include_str!("panel.js"),
    ))
}
/// Local stylesheet; no external assets or fonts.
pub async fn style() -> Response {
    protected((
        [("content-type", "text/css; charset=utf-8")],
        include_str!("../../../plugins/figma/ui/styles.css"),
    ))
}

/// Read the session-specific snapshot with the private browser capability.
pub async fn snapshot(State(state): State<Arc<DaemonState>>, headers: HeaderMap) -> Response {
    let Some(token) = capability(&headers, state.port) else {
        return protected(StatusCode::UNAUTHORIZED);
    };
    match state.panels.authorize(token, &state, false) {
        Some((_, snapshot)) => protected(Json(snapshot)),
        None => protected(StatusCode::UNAUTHORIZED),
    }
}

/// Relay allowlisted human controls to the same plugin controller as its native UI.
pub async fn action(
    State(state): State<Arc<DaemonState>>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> Response {
    let Some(token) = capability(&headers, state.port) else {
        return protected(StatusCode::UNAUTHORIZED);
    };
    let Some((session, snapshot)) = state.panels.authorize(token, &state, true) else {
        return protected(StatusCode::UNAUTHORIZED);
    };
    let command = payload.get("command").and_then(Value::as_str).unwrap_or("");
    let snapshot_epoch = snapshot
        .get("model")
        .and_then(|m| m.get("grantEpoch"))
        .and_then(Value::as_u64);
    let supplied_epoch = payload.get("grant_epoch").and_then(Value::as_u64);
    if command != "refresh" && (snapshot_epoch.is_none() || snapshot_epoch != supplied_epoch) {
        return protected(StatusCode::CONFLICT);
    }
    if ![
        "refresh",
        "select",
        "share",
        "editing",
        "auto",
        "approve",
        "reject",
        "stop",
        "stop_confirm",
        "stop_cancel",
    ]
    .contains(&command)
    {
        return protected(StatusCode::BAD_REQUEST);
    }
    let frame=BridgeFrame::new(MessageType::UiControl,"panel-action",json!({"action":"command","command":command,"value":payload.get("value"),"grant_epoch":payload.get("grant_epoch")}))
        .with_session(session).with_connection_epoch(state.hub.epochs().current(&session).unwrap_or(0));
    match state.hub.try_send(&session, frame) {
        Ok(()) => protected(StatusCode::ACCEPTED),
        Err(_) => protected(StatusCode::SERVICE_UNAVAILABLE),
    }
}
