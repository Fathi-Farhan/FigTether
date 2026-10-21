//! Session and grant registry.
//!
//! The registry is the authority on who may read what. It holds one entry per
//! plugin run, the grant bound to that run, and the epochs that make stale
//! answers detectable.
//!
//! Public signatures in this file are frozen for the v0.1 implementation
//! round: the daemon, the MCP adapter, and the CLI all code against them.

use std::collections::HashMap;
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::Instant;

use chrono::{DateTime, Utc};
use figtether_model::capabilities::Capabilities;
use figtether_model::errors::{ErrorCode, FigError};
use figtether_model::ids::{GrantId, SessionId};

use crate::limits::ResourceLimits;
use crate::scope::NodeIndex;
use crate::state::{transition, SessionEvent, SessionState};

pub use figtether_model::write::GrantMode;

/// State of a grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantState {
    /// Reads are allowed inside the scope.
    Active,
    /// The user paused the session; reads are refused.
    Paused,
    /// The grant was revoked and can never come back.
    Revoked,
}

/// One authorization grant.
#[derive(Debug, Clone, PartialEq)]
pub struct Grant {
    /// Identity of the grant.
    pub grant_id: GrantId,
    /// Root node ids the user granted.
    pub root_ids: Vec<String>,
    /// Mode of the grant: read only, or read and write.
    pub mode: GrantMode,
    /// Epoch of the grant. Incremented on every scope change and on revoke.
    pub grant_epoch: u64,
    /// State of the grant.
    pub state: GrantState,
    /// When the grant was created.
    pub created_at: DateTime<Utc>,
}

/// One plugin session.
#[derive(Debug, Clone)]
pub struct Session {
    /// Identity of the plugin run.
    pub session_id: SessionId,
    /// Identity of the client installation that owns the session.
    pub client_id: String,
    /// Current state.
    pub state: SessionState,
    /// The grant of this session, when one exists.
    pub grant: Option<Grant>,
    /// Connection epoch, incremented on every reconnect.
    pub connection_epoch: u64,
    /// Capabilities probed in the plugin runtime.
    pub capabilities: Capabilities,
    /// When the session was registered.
    pub created_at: DateTime<Utc>,
    /// When the session was last seen.
    pub last_seen_at: DateTime<Utc>,
    /// Monotonic instant of the last heartbeat.
    pub last_heartbeat: Instant,
    /// Node index used for scope membership checks.
    pub node_index: NodeIndex,
}

impl Session {
    /// Epoch of the current grant, when one exists.
    pub fn grant_epoch(&self) -> Option<u64> {
        self.grant.as_ref().map(|grant| grant.grant_epoch)
    }

    /// Root ids of the current grant.
    pub fn root_ids(&self) -> Vec<String> {
        self.grant
            .as_ref()
            .map(|grant| grant.root_ids.clone())
            .unwrap_or_default()
    }

    /// Mode of the current grant, when one exists.
    pub fn grant_mode(&self) -> Option<GrantMode> {
        self.grant.as_ref().map(|grant| grant.mode)
    }
}

/// A copy of a session, safe to hand to another thread.
#[derive(Debug, Clone)]
pub struct SessionSnapshot {
    /// Identity of the session.
    pub session_id: SessionId,
    /// Owning client.
    pub client_id: String,
    /// Current state.
    pub state: SessionState,
    /// Granted root ids.
    pub root_ids: Vec<String>,
    /// Grant epoch.
    pub grant_epoch: Option<u64>,
    /// Connection epoch.
    pub connection_epoch: u64,
    /// Capabilities of the plugin runtime.
    pub capabilities: Capabilities,
}

/// Registry of live sessions.
#[derive(Debug)]
pub struct SessionRegistry {
    inner: RwLock<HashMap<SessionId, Session>>,
    limits: ResourceLimits,
}

impl SessionRegistry {
    /// Create an empty registry.
    pub fn new(limits: ResourceLimits) -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
            limits,
        }
    }

    /// Register a new session.
    ///
    /// Fails with `SESSION_LIMIT` when the daemon already serves the maximum
    /// number of sessions.
    ///
    /// Registering a session id that is already known refreshes that record
    /// instead of creating a second one: the session cap counts plugin runs,
    /// not registrations. The epoch counters stay monotonic across the
    /// refresh, so a client that still holds an old epoch cannot use it.
    pub fn register(
        &self,
        session_id: SessionId,
        client_id: impl Into<String>,
        capabilities: Capabilities,
    ) -> Result<Session, FigError> {
        let client_id = client_id.into();
        let mut sessions = self.write_sessions();
        let previous = sessions.get(&session_id).cloned();
        if previous.is_none() && sessions.len() >= self.limits.max_sessions {
            return Err(FigError::new(
                ErrorCode::SessionLimit,
                format!(
                    "the daemon already serves the maximum of {} sessions",
                    self.limits.max_sessions
                ),
            ));
        }

        let now = Utc::now();
        let connection_epoch = previous
            .as_ref()
            .map(|session| session.connection_epoch)
            .unwrap_or(1);
        // A refresh revokes the previous authorization but keeps its epoch, so
        // the next grant is strictly newer than every epoch ever handed out.
        let grant = previous.and_then(|session| session.grant).map(|mut grant| {
            grant.state = GrantState::Revoked;
            grant.root_ids.clear();
            grant
        });

        let session = Session {
            session_id,
            client_id,
            state: SessionState::Unpaired,
            grant,
            connection_epoch,
            capabilities,
            created_at: now,
            last_seen_at: now,
            last_heartbeat: Instant::now(),
            node_index: NodeIndex::new(),
        };
        sessions.insert(session_id, session.clone());
        Ok(session)
    }

    /// Get a session.
    pub fn get(&self, session_id: &SessionId) -> Option<Session> {
        let sessions = self.read_sessions();
        sessions.get(session_id).cloned()
    }

    /// List the sessions granted to one client.
    pub fn list_for_client(&self, client_id: &str) -> Vec<SessionSnapshot> {
        let sessions = self.read_sessions();
        let mut found: Vec<SessionSnapshot> = sessions
            .values()
            .filter(|session| session.client_id == client_id)
            .map(snapshot_of)
            .collect();
        found.sort_by_key(|snapshot| snapshot.session_id);
        found
    }

    /// List every session.
    pub fn list_all(&self) -> Vec<SessionSnapshot> {
        let sessions = self.read_sessions();
        let mut found: Vec<SessionSnapshot> = sessions.values().map(snapshot_of).collect();
        found.sort_by_key(|snapshot| snapshot.session_id);
        found
    }

    /// Move a session to a new state.
    ///
    /// The move is validated against the session state machine, so an
    /// impossible move is refused instead of being applied silently. `READY`
    /// has no inbound event in the machine: the daemon enters it once the
    /// grant is bound and revalidated.
    pub fn set_state(&self, session_id: &SessionId, state: SessionState) -> Result<(), FigError> {
        let mut sessions = self.write_sessions();
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| session_not_found(session_id))?;
        if session.state == state {
            return Ok(());
        }
        if state == SessionState::Ready {
            if !matches!(
                session.state,
                SessionState::Paired | SessionState::AwaitingGrant | SessionState::Paused
            ) {
                return Err(FigError::new(
                    ErrorCode::InvalidLimit,
                    format!(
                        "a session in state {} cannot become ready",
                        session.state.as_str()
                    ),
                ));
            }
            session.state = state;
            return Ok(());
        }
        let event = event_for(session.state, state).ok_or_else(|| {
            FigError::new(
                ErrorCode::InvalidLimit,
                format!(
                    "no event moves a session from {} to {}",
                    session.state.as_str(),
                    state.as_str()
                ),
            )
        })?;
        let result = transition(session.state, event)?;
        session.state = result.to;
        if result.bump_grant_epoch {
            bump_grant_epoch(session);
        }
        if result.bump_connection_epoch {
            session.connection_epoch = session.connection_epoch.saturating_add(1);
        }
        Ok(())
    }

    /// Create or replace the grant of a session.
    ///
    /// The grant epoch is incremented, the session moves to `AWAITING_GRANT`,
    /// and the previous scope stops being readable immediately.
    ///
    /// The mode is part of the grant. A read grant never authorizes a
    /// mutation, a write grant authorizes one inside the roots after an
    /// approval. Changing the mode is a grant change, so the epoch moves and
    /// every outstanding cursor, asset, and approval of the old grant dies.
    pub fn grant(
        &self,
        session_id: &SessionId,
        root_ids: Vec<String>,
        mode: GrantMode,
    ) -> Result<Grant, FigError> {
        let mut sessions = self.write_sessions();
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| session_not_found(session_id))?;
        if session.state == SessionState::Revoked {
            return Err(FigError::new(
                ErrorCode::Revoked,
                "the grant was revoked; only the user can create a new one",
            ));
        }
        match transition(session.state, SessionEvent::GrantCreated) {
            Ok(result) => session.state = result.to,
            Err(error) => {
                // A scope change of a live session is legal, but a session that
                // never authenticated, or that is paused or offline, is not.
                if !matches!(
                    session.state,
                    SessionState::AwaitingGrant | SessionState::Ready
                ) {
                    return Err(error);
                }
                session.state = SessionState::AwaitingGrant;
            }
        }

        let grant_epoch = session
            .grant
            .as_ref()
            .map(|grant| grant.grant_epoch)
            .unwrap_or(0)
            .saturating_add(1);
        let grant = Grant {
            grant_id: GrantId::new(),
            root_ids,
            mode,
            grant_epoch,
            state: GrantState::Active,
            created_at: Utc::now(),
        };
        session.grant = Some(grant.clone());
        Ok(grant)
    }

    /// The mode of the active grant of a session.
    ///
    /// `None` when the session holds no grant, or when the grant is paused
    /// or revoked: an inactive grant authorizes nothing.
    pub fn grant_mode(&self, session_id: &SessionId) -> Option<GrantMode> {
        let sessions = self.read_sessions();
        let session = sessions.get(session_id)?;
        let grant = session.grant.as_ref()?;
        if grant.state != GrantState::Active {
            return None;
        }
        Some(grant.mode)
    }

    /// Whether the session may mutate the document.
    ///
    /// Only an active write grant authorizes a mutation. A read grant never
    /// does, and a paused or revoked grant authorizes nothing at all.
    pub fn may_write(&self, session_id: &SessionId) -> bool {
        self.grant_mode(session_id) == Some(GrantMode::Write)
    }

    /// Pause the session. New live and snapshot reads are refused.
    pub fn pause(&self, session_id: &SessionId) -> Result<(), FigError> {
        let mut sessions = self.write_sessions();
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| session_not_found(session_id))?;
        let result = transition(session.state, SessionEvent::Paused)?;
        session.state = result.to;
        if result.bump_grant_epoch {
            bump_grant_epoch(session);
        }
        if let Some(grant) = session.grant.as_mut() {
            grant.state = GrantState::Paused;
        }
        Ok(())
    }

    /// Resume a paused session with a fresh grant epoch.
    pub fn resume(&self, session_id: &SessionId) -> Result<(), FigError> {
        let mut sessions = self.write_sessions();
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| session_not_found(session_id))?;
        let result = transition(session.state, SessionEvent::Resumed)?;
        session.state = result.to;
        if result.bump_grant_epoch {
            bump_grant_epoch(session);
        }
        if let Some(grant) = session.grant.as_mut() {
            grant.state = GrantState::Active;
        }
        Ok(())
    }

    /// Revoke the grant. The grant epoch is incremented and the scope stops
    /// being readable immediately.
    pub fn revoke(&self, session_id: &SessionId) -> Result<(), FigError> {
        let mut sessions = self.write_sessions();
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| session_not_found(session_id))?;
        let result = transition(session.state, SessionEvent::Revoked)?;
        session.state = result.to;
        if result.bump_grant_epoch {
            bump_grant_epoch(session);
        }
        if let Some(grant) = session.grant.as_mut() {
            grant.state = GrantState::Revoked;
        }
        // The scope relations die with the grant: nothing of the revoked scope
        // stays resolvable, not even as a stale membership fact.
        session.node_index = NodeIndex::new();
        Ok(())
    }

    /// Increment and return the connection epoch of a session.
    pub fn bump_connection_epoch(&self, session_id: &SessionId) -> Result<u64, FigError> {
        let mut sessions = self.write_sessions();
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| session_not_found(session_id))?;
        session.connection_epoch = session.connection_epoch.saturating_add(1);
        Ok(session.connection_epoch)
    }

    /// Record that the session was seen.
    ///
    /// The heartbeat watchdog reads the monotonic instant, the status view
    /// reads the wall clock.
    pub fn touch(&self, session_id: &SessionId) -> Result<(), FigError> {
        let mut sessions = self.write_sessions();
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| session_not_found(session_id))?;
        session.last_seen_at = Utc::now();
        session.last_heartbeat = Instant::now();
        Ok(())
    }

    /// Whether a node is inside the granted scope of a session.
    ///
    /// The root itself and every descendant of a root count as inside. A
    /// paused or revoked grant never counts as inside.
    pub fn is_in_scope(&self, session_id: &SessionId, node_id: &str) -> bool {
        let sessions = self.read_sessions();
        let Some(session) = sessions.get(session_id) else {
            return false;
        };
        let Some(grant) = session.grant.as_ref() else {
            return false;
        };
        if grant.state != GrantState::Active {
            return false;
        }
        session.node_index.in_any_scope(&grant.root_ids, node_id)
    }

    /// Refuse a node that is outside the granted scope.
    ///
    /// This is the check the daemon runs on every request; the plugin repeats
    /// it on its side (`FR03`).
    pub fn require_in_scope(&self, session_id: &SessionId, node_id: &str) -> Result<(), FigError> {
        if self.is_in_scope(session_id, node_id) {
            Ok(())
        } else {
            Err(out_of_scope(node_id))
        }
    }

    /// Record observed parent relations for scope checks.
    pub fn record_nodes(
        &self,
        session_id: &SessionId,
        nodes: &[(String, Option<String>)],
    ) -> Result<(), FigError> {
        let mut sessions = self.write_sessions();
        let session = sessions
            .get_mut(session_id)
            .ok_or_else(|| session_not_found(session_id))?;
        for (node_id, parent_id) in nodes {
            let Some(parent_id) = parent_id else {
                continue;
            };
            session
                .node_index
                .insert(node_id.clone(), parent_id.clone());
        }
        Ok(())
    }

    /// Atomically learn created descendants under the unchanged active write grant.
    /// Invalid receipts and stale epochs cannot expand or rewrite the scope index.
    pub fn record_created_nodes(
        &self,
        session_id: &SessionId,
        grant_epoch: u64,
        nodes: &[(String, String)],
    ) -> bool {
        let mut sessions = self.write_sessions();
        let Some(session) = sessions.get_mut(session_id) else {
            return false;
        };
        let Some(grant) = session.grant.as_ref() else {
            return false;
        };
        if grant.grant_epoch != grant_epoch
            || grant.state != GrantState::Active
            || grant.mode != GrantMode::Write
        {
            return false;
        }
        let mut seen = std::collections::HashSet::new();
        for (node, parent) in nodes {
            if node.is_empty()
                || node == parent
                || !seen.insert(node)
                || grant.root_ids.contains(node)
                || session.node_index.parent_of(node).is_some()
                || !session.node_index.in_any_scope(&grant.root_ids, parent)
            {
                return false;
            }
        }
        for (node, parent) in nodes {
            session.node_index.insert(node.clone(), parent.clone());
        }
        true
    }

    /// Remove a session and its learned descendants.
    pub fn remove(&self, session_id: &SessionId) {
        let mut sessions = self.write_sessions();
        if let Some(mut session) = sessions.remove(session_id) {
            for root in session.root_ids() {
                session.node_index.remove_scope(&root);
            }
        }
    }

    /// Number of registered sessions.
    pub fn len(&self) -> usize {
        let sessions = self.read_sessions();
        sessions.len()
    }

    /// Whether no session is registered.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Read the session map.
    fn read_sessions(&self) -> RwLockReadGuard<'_, HashMap<SessionId, Session>> {
        match self.inner.read() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Write the session map.
    fn write_sessions(&self) -> RwLockWriteGuard<'_, HashMap<SessionId, Session>> {
        match self.inner.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// Build the copy of a session handed to other threads.
fn snapshot_of(session: &Session) -> SessionSnapshot {
    SessionSnapshot {
        session_id: session.session_id,
        client_id: session.client_id.clone(),
        state: session.state,
        root_ids: session.root_ids(),
        grant_epoch: session.grant_epoch(),
        connection_epoch: session.connection_epoch,
        capabilities: session.capabilities.clone(),
    }
}

/// The event of the state machine that moves a session from one state to
/// another.
fn event_for(current: SessionState, target: SessionState) -> Option<SessionEvent> {
    const EVENTS: [SessionEvent; 8] = [
        SessionEvent::Authenticated,
        SessionEvent::GrantCreated,
        SessionEvent::Paused,
        SessionEvent::Resumed,
        SessionEvent::Revoked,
        SessionEvent::HeartbeatLost,
        SessionEvent::HeartbeatRestored,
        SessionEvent::PluginClosed,
    ];
    EVENTS
        .iter()
        .copied()
        .find(|event| matches!(transition(current, *event), Ok(result) if result.to == target))
}

/// Increment the grant epoch of a session, when it holds a grant.
fn bump_grant_epoch(session: &mut Session) {
    if let Some(grant) = session.grant.as_mut() {
        grant.grant_epoch = grant.grant_epoch.saturating_add(1);
    }
}

/// The error of an unknown session.
fn session_not_found(session_id: &SessionId) -> FigError {
    FigError::new(
        ErrorCode::SessionNotFound,
        format!("session {session_id} is not registered"),
    )
}

/// Convenience constructor for the "not allowed" error.
pub(crate) fn out_of_scope(node_id: &str) -> FigError {
    FigError::new(
        ErrorCode::OutOfScope,
        format!("node {node_id} is outside the granted scope"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use figtether_model::capabilities::CapabilityState;

    fn limits() -> ResourceLimits {
        ResourceLimits::default()
    }

    fn registry() -> SessionRegistry {
        SessionRegistry::new(limits())
    }

    fn capabilities() -> Capabilities {
        let mut caps = Capabilities::new();
        caps.record("variables", CapabilityState::Available);
        caps
    }

    fn paired_session(registry: &SessionRegistry) -> SessionId {
        let id = SessionId::new();
        registry
            .register(id, "client-a", capabilities())
            .expect("registers");
        registry
            .set_state(&id, SessionState::Paired)
            .expect("authenticates");
        id
    }

    #[test]
    fn creation_receipts_learn_descendants_without_expanding_roots() {
        let registry = registry();
        let id = paired_session(&registry);
        let grant = registry
            .grant(&id, vec!["1:2".to_string()], GrantMode::Write)
            .expect("grant");
        let nodes = vec![("1:9".to_string(), "1:2".to_string())];
        assert!(registry.record_created_nodes(&id, grant.grant_epoch, &nodes));
        assert!(registry.is_in_scope(&id, "1:9"));
        assert_eq!(registry.get(&id).expect("session").root_ids(), vec!["1:2"]);
        assert!(
            !registry.record_created_nodes(&id, grant.grant_epoch, &nodes),
            "an existing relation is not rewritten"
        );
    }

    #[test]
    fn creation_receipts_refuse_stale_or_outside_relations_atomically() {
        let registry = registry();
        let id = paired_session(&registry);
        let grant = registry
            .grant(&id, vec!["1:2".to_string()], GrantMode::Write)
            .expect("grant");
        let nodes = vec![
            ("1:9".to_string(), "1:2".to_string()),
            ("1:10".to_string(), "outside".to_string()),
        ];
        assert!(!registry.record_created_nodes(&id, grant.grant_epoch, &nodes));
        assert!(!registry.is_in_scope(&id, "1:9"));
        assert!(!registry.record_created_nodes(&id, grant.grant_epoch + 1, &nodes[..1]));
        registry
            .grant(&id, vec!["1:2".to_string()], GrantMode::Read)
            .expect("read only");
        let epoch = registry
            .get(&id)
            .expect("session")
            .grant_epoch()
            .expect("epoch");
        assert!(!registry.record_created_nodes(&id, epoch, &nodes[..1]));
    }

    #[test]
    fn register_starts_unpaired_with_the_first_connection_epoch() {
        let registry = registry();
        let id = SessionId::new();
        let session = registry
            .register(id, "client-a", capabilities())
            .expect("registers");
        assert_eq!(session.state, SessionState::Unpaired);
        assert_eq!(session.connection_epoch, 1);
        assert!(session.grant.is_none());
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn register_refuses_beyond_the_session_limit() {
        let mut limits = limits();
        limits.max_sessions = 2;
        let registry = SessionRegistry::new(limits);
        registry
            .register(SessionId::new(), "client-a", capabilities())
            .expect("first session");
        registry
            .register(SessionId::new(), "client-a", capabilities())
            .expect("second session");
        let error = registry
            .register(SessionId::new(), "client-a", capabilities())
            .expect_err("refused");
        assert_eq!(error.code, ErrorCode::SessionLimit);
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn registering_the_same_session_twice_does_not_consume_a_slot() {
        let mut limits = limits();
        limits.max_sessions = 1;
        let registry = SessionRegistry::new(limits);
        let id = SessionId::new();
        registry
            .register(id, "client-a", capabilities())
            .expect("registers");
        registry
            .register(id, "client-a", capabilities())
            .expect("refreshes");
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn get_returns_a_copy_of_the_session() {
        let registry = registry();
        let id = SessionId::new();
        registry
            .register(id, "client-a", capabilities())
            .expect("registers");
        let mut session = registry.get(&id).expect("known session");
        session.client_id = "changed".to_string();
        let stored = registry.get(&id).expect("known session");
        assert_eq!(stored.client_id, "client-a");
        assert!(registry.get(&SessionId::new()).is_none());
    }

    #[test]
    fn list_for_client_returns_only_the_sessions_of_that_client() {
        let registry = registry();
        let first = SessionId::new();
        let second = SessionId::new();
        let third = SessionId::new();
        registry
            .register(first, "client-a", capabilities())
            .expect("a");
        registry
            .register(second, "client-b", capabilities())
            .expect("b");
        registry
            .register(third, "client-a", capabilities())
            .expect("a");
        let listed = registry.list_for_client("client-a");
        assert_eq!(listed.len(), 2);
        assert!(listed.iter().all(|item| item.client_id == "client-a"));
        assert_eq!(registry.list_for_client("client-c").len(), 0);
        assert_eq!(registry.list_all().len(), 3);
    }

    #[test]
    fn grant_replaces_the_scope_and_bumps_the_epoch() {
        let registry = registry();
        let id = paired_session(&registry);
        let first = registry
            .grant(&id, vec!["1:2".to_string()], GrantMode::Read)
            .expect("grants");
        assert_eq!(first.grant_epoch, 1);
        assert_eq!(first.state, GrantState::Active);
        let second = registry
            .grant(&id, vec!["9:9".to_string()], GrantMode::Read)
            .expect("grants again");
        assert_eq!(second.grant_epoch, 2);
        assert_eq!(second.root_ids, vec!["9:9".to_string()]);
        let session = registry.get(&id).expect("known session");
        assert_eq!(session.state, SessionState::AwaitingGrant);
        assert_eq!(session.root_ids(), vec!["9:9".to_string()]);
    }

    #[test]
    fn the_previous_scope_is_not_readable_after_a_new_grant() {
        let registry = registry();
        let id = paired_session(&registry);
        registry
            .grant(&id, vec!["1:2".to_string()], GrantMode::Read)
            .expect("grants");
        registry
            .record_nodes(
                &id,
                &[
                    ("1:3".to_string(), Some("1:2".to_string())),
                    ("1:4".to_string(), Some("1:3".to_string())),
                ],
            )
            .expect("records");
        assert!(registry.is_in_scope(&id, "1:3"));
        registry
            .grant(&id, vec!["9:9".to_string()], GrantMode::Read)
            .expect("grants again");
        assert!(!registry.is_in_scope(&id, "1:2"));
        assert!(!registry.is_in_scope(&id, "1:3"));
        assert!(!registry.is_in_scope(&id, "1:4"));
    }

    #[test]
    fn scope_membership_covers_the_root_and_its_descendants() {
        let registry = registry();
        let id = paired_session(&registry);
        registry
            .grant(&id, vec!["1:2".to_string()], GrantMode::Read)
            .expect("grants");
        registry
            .record_nodes(
                &id,
                &[
                    ("1:3".to_string(), Some("1:2".to_string())),
                    ("1:4".to_string(), Some("1:3".to_string())),
                    ("2:1".to_string(), Some("2:0".to_string())),
                ],
            )
            .expect("records");
        assert!(registry.is_in_scope(&id, "1:2"), "the root is inside");
        assert!(registry.is_in_scope(&id, "1:3"), "a child is inside");
        assert!(registry.is_in_scope(&id, "1:4"), "a grandchild is inside");
        assert!(!registry.is_in_scope(&id, "2:1"), "a sibling is outside");
        assert!(
            !registry.is_in_scope(&id, "7:7"),
            "an unknown node is outside"
        );
    }

    #[test]
    fn require_in_scope_reports_the_out_of_scope_error() {
        let registry = registry();
        let id = paired_session(&registry);
        registry
            .grant(&id, vec!["1:2".to_string()], GrantMode::Read)
            .expect("grants");
        assert!(registry.require_in_scope(&id, "1:2").is_ok());
        let error = registry
            .require_in_scope(&id, "2:1")
            .expect_err("outside the scope");
        assert_eq!(error.code, ErrorCode::OutOfScope);
    }

    #[test]
    fn revoke_makes_the_scope_unreadable_at_once() {
        let registry = registry();
        let id = paired_session(&registry);
        let grant = registry
            .grant(&id, vec!["1:2".to_string()], GrantMode::Read)
            .expect("grants");
        registry
            .record_nodes(&id, &[("1:3".to_string(), Some("1:2".to_string()))])
            .expect("records");
        registry.revoke(&id).expect("revokes");
        assert!(!registry.is_in_scope(&id, "1:2"));
        assert!(!registry.is_in_scope(&id, "1:3"));
        let session = registry.get(&id).expect("known session");
        assert_eq!(session.state, SessionState::Revoked);
        assert_eq!(
            session.grant.as_ref().map(|grant| grant.grant_epoch),
            Some(grant.grant_epoch + 1)
        );
        assert_eq!(
            session.grant.as_ref().map(|grant| grant.state),
            Some(GrantState::Revoked)
        );
    }

    #[test]
    fn a_revoked_session_never_revives() {
        let registry = registry();
        let id = paired_session(&registry);
        registry
            .grant(&id, vec!["1:2".to_string()], GrantMode::Read)
            .expect("grants");
        registry.revoke(&id).expect("revokes");
        let error = registry
            .grant(&id, vec!["1:2".to_string()], GrantMode::Read)
            .expect_err("no new grant");
        assert_eq!(error.code, ErrorCode::Revoked);
    }

    #[test]
    fn pause_and_resume_bump_the_grant_epoch() {
        let registry = registry();
        let id = paired_session(&registry);
        let grant = registry
            .grant(&id, vec!["1:2".to_string()], GrantMode::Read)
            .expect("grants");
        registry.pause(&id).expect("pauses");
        let paused = registry.get(&id).expect("known session");
        assert_eq!(paused.state, SessionState::Paused);
        assert_eq!(
            paused.grant_epoch(),
            Some(grant.grant_epoch + 1),
            "pause issues a new epoch"
        );
        assert!(
            !registry.is_in_scope(&id, "1:2"),
            "a paused grant reads nothing"
        );
        registry.resume(&id).expect("resumes");
        let resumed = registry.get(&id).expect("known session");
        assert_eq!(
            resumed.grant_epoch(),
            Some(grant.grant_epoch + 2),
            "resume issues a new epoch"
        );
        assert!(
            registry.is_in_scope(&id, "1:2"),
            "the scope is readable again"
        );
    }

    #[test]
    fn a_paused_session_refuses_new_grants() {
        let registry = registry();
        let id = paired_session(&registry);
        registry
            .grant(&id, vec!["1:2".to_string()], GrantMode::Read)
            .expect("grants");
        registry.pause(&id).expect("pauses");
        let error = registry
            .grant(&id, vec!["1:2".to_string()], GrantMode::Read)
            .expect_err("refused while paused");
        assert_eq!(error.code, ErrorCode::InvalidLimit);
    }

    #[test]
    fn the_connection_epoch_grows_monotonically() {
        let registry = registry();
        let id = SessionId::new();
        registry
            .register(id, "client-a", capabilities())
            .expect("registers");
        let first = registry.bump_connection_epoch(&id).expect("bumps");
        let second = registry.bump_connection_epoch(&id).expect("bumps");
        assert_eq!(first, 2);
        assert_eq!(second, 3);
        assert!(registry.bump_connection_epoch(&SessionId::new()).is_err());
    }

    #[test]
    fn unknown_sessions_are_reported_as_not_found() {
        let registry = registry();
        let id = SessionId::new();
        assert_eq!(
            registry.pause(&id).expect_err("unknown").code,
            ErrorCode::SessionNotFound
        );
        assert_eq!(
            registry.resume(&id).expect_err("unknown").code,
            ErrorCode::SessionNotFound
        );
        assert_eq!(
            registry.revoke(&id).expect_err("unknown").code,
            ErrorCode::SessionNotFound
        );
        assert_eq!(
            registry
                .record_nodes(&id, &[("1:2".to_string(), None)])
                .expect_err("unknown")
                .code,
            ErrorCode::SessionNotFound
        );
    }

    #[test]
    fn remove_drops_the_session_and_its_scope() {
        let registry = registry();
        let id = paired_session(&registry);
        registry
            .grant(&id, vec!["1:2".to_string()], GrantMode::Read)
            .expect("grants");
        registry
            .record_nodes(&id, &[("1:3".to_string(), Some("1:2".to_string()))])
            .expect("records");
        registry.remove(&id);
        assert!(registry.is_empty());
        assert!(registry.get(&id).is_none());
        assert!(!registry.is_in_scope(&id, "1:3"));
    }

    #[test]
    fn a_touched_session_keeps_its_heartbeat_fresh() {
        let registry = registry();
        let id = SessionId::new();
        registry
            .register(id, "client-a", capabilities())
            .expect("registers");
        let before = registry.get(&id).expect("known session");
        registry.touch(&id).expect("touches");
        let after = registry.get(&id).expect("known session");
        assert!(after.last_heartbeat >= before.last_heartbeat);
        assert!(after.last_seen_at >= before.last_seen_at);
        assert!(registry.touch(&SessionId::new()).is_err());
    }

    #[test]
    fn set_state_refuses_a_move_the_machine_does_not_allow() {
        let registry = registry();
        let id = SessionId::new();
        registry
            .register(id, "client-a", capabilities())
            .expect("registers");
        let error = registry
            .set_state(&id, SessionState::Paused)
            .expect_err("no pause before pairing");
        assert_eq!(error.code, ErrorCode::InvalidLimit);
        registry
            .set_state(&id, SessionState::Paired)
            .expect("authenticates");
        assert_eq!(
            registry.get(&id).expect("known session").state,
            SessionState::Paired
        );
    }

    #[test]
    fn set_state_can_enter_ready_after_a_grant() {
        let registry = registry();
        let id = paired_session(&registry);
        registry
            .grant(&id, vec!["1:2".to_string()], GrantMode::Read)
            .expect("grants");
        registry
            .set_state(&id, SessionState::Ready)
            .expect("the grant is bound");
        let session = registry.get(&id).expect("known session");
        assert_eq!(session.state, SessionState::Ready);
        assert!(session.state.serves_reads());
    }

    #[test]
    fn a_read_grant_never_authorizes_a_write() {
        let registry = registry();
        let id = paired_session(&registry);
        let grant = registry
            .grant(&id, vec!["1:2".to_string()], GrantMode::Read)
            .expect("grants");
        assert_eq!(grant.mode, GrantMode::Read);
        assert_eq!(grant.mode.as_str(), "read");
        assert_eq!(registry.grant_mode(&id), Some(GrantMode::Read));
        assert!(!registry.may_write(&id));
        assert!(!GrantMode::Read.allows_writes());
    }

    #[test]
    fn a_write_grant_authorizes_a_write_inside_the_roots() {
        let registry = registry();
        let id = paired_session(&registry);
        let grant = registry
            .grant(&id, vec!["1:2".to_string()], GrantMode::Write)
            .expect("grants");
        assert_eq!(grant.mode, GrantMode::Write);
        assert_eq!(grant.mode.as_str(), "write");
        assert_eq!(registry.grant_mode(&id), Some(GrantMode::Write));
        assert!(registry.may_write(&id));
        assert!(GrantMode::Write.allows_writes());
        // The write grant reads like every other grant.
        registry
            .record_nodes(&id, &[("1:3".to_string(), Some("1:2".to_string()))])
            .expect("records");
        assert!(registry.is_in_scope(&id, "1:3"));
    }

    #[test]
    fn the_mode_change_bumps_the_grant_epoch() {
        let registry = registry();
        let id = paired_session(&registry);
        let read = registry
            .grant(&id, vec!["1:2".to_string()], GrantMode::Read)
            .expect("grants");
        let write = registry
            .grant(&id, vec!["1:2".to_string()], GrantMode::Write)
            .expect("grants again");
        assert_eq!(
            write.grant_epoch,
            read.grant_epoch + 1,
            "a mode change is a grant change"
        );
        assert!(registry.may_write(&id));
        // And back to a read grant, which dies with a new epoch as well.
        let back = registry
            .grant(&id, vec!["1:2".to_string()], GrantMode::Read)
            .expect("grants a third time");
        assert_eq!(back.grant_epoch, write.grant_epoch + 1);
        assert!(!registry.may_write(&id));
    }

    #[test]
    fn an_inactive_grant_authorizes_nothing() {
        let registry = registry();
        let id = paired_session(&registry);
        registry
            .grant(&id, vec!["1:2".to_string()], GrantMode::Write)
            .expect("grants");
        assert!(registry.may_write(&id));
        registry.pause(&id).expect("pauses");
        assert_eq!(registry.grant_mode(&id), None);
        assert!(!registry.may_write(&id));
        registry.resume(&id).expect("resumes");
        assert_eq!(
            registry.grant_mode(&id),
            Some(GrantMode::Write),
            "a resumed grant is active again"
        );
        assert!(registry.may_write(&id));
        registry.revoke(&id).expect("revokes");
        assert_eq!(registry.grant_mode(&id), None);
        assert!(!registry.may_write(&id));
    }

    #[test]
    fn an_unknown_session_holds_no_grant() {
        let registry = registry();
        let id = SessionId::new();
        assert_eq!(registry.grant_mode(&id), None);
        assert!(!registry.may_write(&id));
    }
}
