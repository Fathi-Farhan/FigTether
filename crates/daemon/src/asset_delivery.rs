//! Bounded, authenticated image delivery. No URLs or filesystem access.
use crate::{client_api::ClientIdentity, DaemonState};
use base64::Engine;
use chrono::{Duration, Utc};
use figtether_core::{AssetStore, GrantState, SessionState};
use figtether_model::errors::{ErrorCode, FigError};
use figtether_model::tools::GetAssetParams;
use serde_json::{json, Value};
use std::sync::Arc;

/// Called under the domain guard, including the final active-grant check.
pub fn read(
    state: &Arc<DaemonState>,
    identity: &ClientIdentity,
    params: &Value,
) -> Result<Value, FigError> {
    read_at(state, identity, params, Utc::now())
}

fn read_at(
    state: &Arc<DaemonState>,
    identity: &ClientIdentity,
    params: &Value,
    now: chrono::DateTime<Utc>,
) -> Result<Value, FigError> {
    let p: GetAssetParams = serde_json::from_value(params.clone()).map_err(|_| {
        FigError::new(
            ErrorCode::ProtocolMismatch,
            "get_asset requires session_id and asset_id",
        )
    })?;
    let denied = || {
        FigError::new(
            ErrorCode::OutOfScope,
            "the image is not available in this active grant; export it again",
        )
    };
    let session = state.registry.get(&p.session_id).ok_or_else(denied)?;
    if session.client_id != identity.owner {
        return Err(denied());
    }
    if state.hub.is_revoked(&p.session_id) || session.state == SessionState::Revoked {
        return Err(FigError::new(ErrorCode::Revoked, "the user revoked access"));
    }
    if session.state != SessionState::Ready {
        return Err(FigError::new(
            ErrorCode::PluginOffline,
            "the session must be live and ready to retrieve an image",
        ));
    }
    let grant = session.grant.ok_or_else(denied)?;
    if grant.state != GrantState::Active {
        return Err(denied());
    }
    let asset = state.assets.get(&p.asset_id).ok_or_else(denied)?;
    if asset.grant_id != grant.grant_id
        || now.signed_duration_since(asset.stored_at) > Duration::minutes(5)
    {
        return Err(denied());
    }
    if asset.mime != "image/png" {
        return Err(FigError::new(
            ErrorCode::CapabilityUnavailable,
            "inline delivery supports PNG; export the frame as PNG",
        ));
    }
    if asset.bytes.len() as u64 > state.limits.max_inline_image_bytes {
        return Err(FigError::new(
            ErrorCode::PayloadTooLarge,
            "the PNG exceeds the inline image cap; export a smaller section or lower scale",
        ));
    }
    let encoded = base64::engine::general_purpose::STANDARD.encode(&asset.bytes);
    if encoded.len() as u64 + 8192 > state.limits.max_encoded_response_bytes {
        return Err(FigError::new(
            ErrorCode::PayloadTooLarge,
            "the encoded image exceeds the response cap",
        ));
    }
    Ok(
        json!({"session_id":p.session_id,"asset_id":p.asset_id,"mime":asset.mime,"width":asset.width,"height":asset.height,"hash":asset.hash,"size":asset.bytes.len(),"stored_at":asset.stored_at,"image_base64":encoded}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use figtether_model::ids::SessionId;
    use uuid::Uuid;
    fn fixture() -> (Arc<DaemonState>, ClientIdentity, Value) {
        let state = Arc::new(DaemonState::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            38947,
            Default::default(),
            crate::Secret::from_value("test"),
        ));
        let session = SessionId::new();
        state
            .registry
            .register(session, state.owner_key(), Default::default())
            .unwrap();
        state
            .registry
            .set_state(&session, SessionState::Paired)
            .unwrap();
        let grant = state
            .registry
            .grant(
                &session,
                vec!["1:2".into()],
                figtether_core::GrantMode::Read,
            )
            .unwrap();
        state
            .registry
            .set_state(&session, SessionState::Ready)
            .unwrap();
        let asset = state
            .assets
            .put(grant.grant_id, "image/png", 1, 1, vec![1, 2, 3])
            .unwrap();
        let identity = ClientIdentity {
            label: "test".into(),
            owner: state.owner_key(),
        };
        (
            state,
            identity,
            json!({"session_id":session,"asset_id":asset}),
        )
    }
    #[test]
    fn image_read_refuses_other_owners_sessions_epochs_and_expiry() {
        let (state, identity, p) = fixture();
        assert_eq!(read(&state, &identity, &p).unwrap()["image_base64"], "AQID");
        assert!(read_at(&state, &identity, &p, Utc::now() + Duration::minutes(6)).is_err());
        let other = ClientIdentity {
            label: "other".into(),
            owner: "other".into(),
        };
        assert!(read(&state, &other, &p).is_err());
        let mut bad = p.clone();
        bad["session_id"] = json!(SessionId::new());
        assert!(read(&state, &identity, &bad).is_err());
        let sid = serde_json::from_value(p["session_id"].clone()).unwrap();
        state
            .registry
            .grant(&sid, vec!["1:2".into()], figtether_core::GrantMode::Read)
            .unwrap();
        state.registry.set_state(&sid, SessionState::Ready).unwrap();
        assert!(read(&state, &identity, &p).is_err());
    }
    #[test]
    fn revoked_paused_offline_and_oversized_assets_are_refused() {
        for state_to in [
            SessionState::Paused,
            SessionState::Offline,
            SessionState::Revoked,
        ] {
            let (state, identity, p) = fixture();
            let sid = serde_json::from_value(p["session_id"].clone()).unwrap();
            state.registry.set_state(&sid, state_to).unwrap();
            assert!(read(&state, &identity, &p).is_err());
        }
        let (state, identity, mut p) = fixture();
        let sid = serde_json::from_value(p["session_id"].clone()).unwrap();
        let grant = state.registry.get(&sid).unwrap().grant.unwrap();
        p["asset_id"] = json!(state
            .assets
            .put(
                grant.grant_id,
                "image/png",
                1,
                1,
                vec![0; state.limits.max_inline_image_bytes as usize + 1]
            )
            .unwrap());
        assert_eq!(
            read(&state, &identity, &p).unwrap_err().code,
            ErrorCode::PayloadTooLarge
        );
    }
}
