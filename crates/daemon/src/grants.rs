//! The grants the daemon issued.
//!
//! A session can hold several grants over its life: every scope change
//! issues a new grant with a new identity, and the assets of the older grant
//! stay in the asset store. The daemon remembers the identities of the grants
//! of a session, so a revocation drops the assets of every grant of that
//! session and not only of the current one.
//!
//! The ledger holds no scope, no node id, and no design content: only the
//! opaque identities of the grants.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use figtether_model::ids::{GrantId, SessionId};

/// The grants of the known sessions.
#[derive(Debug, Default)]
pub struct GrantLedger {
    inner: Mutex<HashMap<SessionId, Vec<GrantId>>>,
}

impl GrantLedger {
    /// Create an empty ledger.
    pub fn new() -> Self {
        Self::default()
    }

    /// Remember a grant of a session.
    pub fn remember(&self, session_id: &SessionId, grant_id: GrantId) {
        let mut guard = self.lock();
        let grants = guard.entry(*session_id).or_default();
        if !grants.contains(&grant_id) {
            grants.push(grant_id);
        }
    }

    /// The grants of a session.
    pub fn of(&self, session_id: &SessionId) -> Vec<GrantId> {
        self.lock().get(session_id).cloned().unwrap_or_default()
    }

    /// Drop the grants of a session and return them.
    pub fn forget(&self, session_id: &SessionId) -> Vec<GrantId> {
        self.lock().remove(session_id).unwrap_or_default()
    }

    /// Lock the ledger, recovering from a poisoned lock.
    fn lock(&self) -> MutexGuard<'_, HashMap<SessionId, Vec<GrantId>>> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grants_of_a_session_are_remembered() {
        let ledger = GrantLedger::new();
        let session_id = SessionId::new();
        let first = GrantId::new();
        let second = GrantId::new();
        ledger.remember(&session_id, first);
        ledger.remember(&session_id, second);
        ledger.remember(&session_id, first);
        assert_eq!(ledger.of(&session_id), vec![first, second]);
    }

    #[test]
    fn a_forgotten_session_has_no_grants_left() {
        let ledger = GrantLedger::new();
        let session_id = SessionId::new();
        ledger.remember(&session_id, GrantId::new());
        assert_eq!(ledger.forget(&session_id).len(), 1);
        assert!(ledger.of(&session_id).is_empty());
    }
}
