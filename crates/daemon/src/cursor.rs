//! The traversal cursors of the daemon.
//!
//! A cursor continues a truncated traversal. It is an opaque token: the
//! client never sees the position, the scope, or the epoch it is bound to,
//! and it never carries a secret.
//!
//! A cursor is bound to the observation that produced it, to the session, to
//! the grant epoch in force, to the root of the traversal, and to the field
//! projection of the request. A cursor that no longer matches is refused with
//! `CURSOR_EXPIRED`, so a stale cursor can never serve stale data
//! (`ADR-006`, PRD 8.3).

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use figtether_model::errors::{ErrorCode, FigError};
use figtether_model::ids::{ObservationId, SessionId};
use uuid::Uuid;

/// One minted cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorRecord {
    /// Session the traversal belongs to.
    pub session_id: SessionId,
    /// Grant epoch in force when the cursor was minted.
    pub grant_epoch: u64,
    /// Root of the traversal.
    pub root_id: String,
    /// Field projection of the traversal.
    pub fields: Vec<String>,
    /// Observation the cursor belongs to.
    pub observation_id: ObservationId,
    /// The last node the traversal returned.
    pub position: String,
}

/// The store of the minted cursors.
///
/// The store is bounded by the grant lifecycle: a revoke drops the cursors of
/// the session, and a grant epoch that no longer serves reads drops the
/// cursors that were minted under it.
#[derive(Debug, Default)]
pub struct CursorStore {
    inner: Mutex<HashMap<String, CursorRecord>>,
}

impl CursorStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Mint a cursor and return its opaque token.
    pub fn mint(&self, record: CursorRecord) -> String {
        let token = format!("cursor-{}", Uuid::new_v4().simple());
        self.lock().insert(token.clone(), record);
        token
    }

    /// The record of a token.
    pub fn get(&self, token: &str) -> Option<CursorRecord> {
        self.lock().get(token).cloned()
    }

    /// Number of the cursors the store holds.
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Whether the store holds no cursor.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Drop every cursor of a session.
    pub fn drop_session(&self, session_id: &SessionId) -> usize {
        self.drop_matching(|record| record.session_id == *session_id)
    }

    /// Drop every cursor that was minted under a grant epoch.
    pub fn drop_grant(&self, grant_epoch: u64) -> usize {
        self.drop_matching(|record| record.grant_epoch == grant_epoch)
    }

    /// Drop every cursor that matches the predicate.
    fn drop_matching(&self, predicate: impl Fn(&CursorRecord) -> bool) -> usize {
        let mut guard = self.lock();
        let victims: Vec<String> = guard
            .iter()
            .filter(|(_, record)| predicate(record))
            .map(|(token, _)| token.clone())
            .collect();
        let removed = victims.len();
        for victim in victims {
            guard.remove(&victim);
        }
        removed
    }

    /// Lock the store, recovering from a poisoned lock.
    fn lock(&self) -> MutexGuard<'_, HashMap<String, CursorRecord>> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Check a cursor against the request that continues the traversal.
///
/// The check is the revalidation of ADR-006: the session, the grant epoch,
/// the scope, and the projection have to match the ones the cursor was minted
/// with. A cursor that does not match is refused with `CURSOR_EXPIRED`,
/// never with an empty result.
pub fn validate(
    store: &CursorStore,
    token: &str,
    session_id: &SessionId,
    grant_epoch: u64,
    root_id: &str,
    fields: &[String],
) -> Result<CursorRecord, FigError> {
    let record = store.get(token).ok_or_else(|| {
        FigError::new(
            ErrorCode::CursorExpired,
            "the cursor is unknown, restart the traversal",
        )
    })?;
    if record.session_id != *session_id {
        return Err(FigError::new(
            ErrorCode::CursorExpired,
            "the cursor belongs to another session, restart the traversal",
        ));
    }
    if record.grant_epoch != grant_epoch {
        return Err(FigError::new(
            ErrorCode::CursorExpired,
            "the grant changed, restart the traversal",
        ));
    }
    if record.root_id != root_id {
        return Err(FigError::new(
            ErrorCode::CursorExpired,
            "the cursor belongs to another traversal, restart it",
        ));
    }
    if record.fields != fields {
        return Err(FigError::new(
            ErrorCode::CursorExpired,
            "the projection changed, restart the traversal",
        ));
    }
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(session_id: SessionId, grant_epoch: u64) -> CursorRecord {
        CursorRecord {
            session_id,
            grant_epoch,
            root_id: "1:2".to_string(),
            fields: vec!["name".to_string()],
            observation_id: ObservationId::new(),
            position: "1:9".to_string(),
        }
    }

    #[test]
    fn a_minted_cursor_is_found_again() {
        let store = CursorStore::new();
        let session_id = SessionId::new();
        let token = store.mint(record(session_id, 3));
        let found = store.get(&token).expect("the cursor is known");
        assert_eq!(found.session_id, session_id);
        assert_eq!(found.grant_epoch, 3);
        assert_eq!(found.position, "1:9");
    }

    #[test]
    fn a_cursor_of_another_session_is_refused() {
        let store = CursorStore::new();
        let token = store.mint(record(SessionId::new(), 1));
        let error = validate(
            &store,
            &token,
            &SessionId::new(),
            1,
            "1:2",
            &["name".to_string()],
        )
        .expect_err("the cursor belongs to another session");
        assert_eq!(error.code, ErrorCode::CursorExpired);
    }

    #[test]
    fn a_cursor_of_an_older_grant_is_refused() {
        let store = CursorStore::new();
        let session_id = SessionId::new();
        let token = store.mint(record(session_id, 1));
        let error = validate(&store, &token, &session_id, 2, "1:2", &["name".to_string()])
            .expect_err("the grant moved");
        assert_eq!(error.code, ErrorCode::CursorExpired);
    }

    #[test]
    fn a_cursor_of_another_traversal_is_refused() {
        let store = CursorStore::new();
        let session_id = SessionId::new();
        let token = store.mint(record(session_id, 1));
        let error = validate(&store, &token, &session_id, 1, "9:9", &["name".to_string()])
            .expect_err("the root moved");
        assert_eq!(error.code, ErrorCode::CursorExpired);
    }

    #[test]
    fn a_cursor_of_another_projection_is_refused() {
        let store = CursorStore::new();
        let session_id = SessionId::new();
        let token = store.mint(record(session_id, 1));
        let error =
            validate(&store, &token, &session_id, 1, "1:2", &[]).expect_err("the projection moved");
        assert_eq!(error.code, ErrorCode::CursorExpired);
    }

    #[test]
    fn an_unknown_cursor_is_refused() {
        let store = CursorStore::new();
        let error = validate(
            &store,
            "cursor-does-not-exist",
            &SessionId::new(),
            1,
            "1:2",
            &[],
        )
        .expect_err("the cursor is unknown");
        assert_eq!(error.code, ErrorCode::CursorExpired);
    }

    #[test]
    fn a_matching_cursor_is_accepted() {
        let store = CursorStore::new();
        let session_id = SessionId::new();
        let token = store.mint(record(session_id, 1));
        let found = validate(&store, &token, &session_id, 1, "1:2", &["name".to_string()])
            .expect("the cursor matches");
        assert_eq!(found.position, "1:9");
    }

    #[test]
    fn a_revoked_session_loses_its_cursors() {
        let store = CursorStore::new();
        let session_id = SessionId::new();
        store.mint(record(session_id, 1));
        store.mint(record(SessionId::new(), 1));
        assert_eq!(store.drop_session(&session_id), 1);
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn an_old_grant_loses_its_cursors() {
        let store = CursorStore::new();
        let session_id = SessionId::new();
        store.mint(record(session_id, 1));
        store.mint(record(session_id, 2));
        assert_eq!(store.drop_grant(1), 1);
        assert_eq!(store.len(), 1);
        assert!(!store.is_empty());
    }
}
