//! Session state machine.
//!
//! The session lifecycle is explicit and enforced in one place, because every
//! other component must agree on when reads are allowed:
//!
//! ```text
//! UNPAIRED -> PAIRED -> AWAITING_GRANT -> READY <-> PAUSED -> REVOKED
//!                    \                    |                  /
//!                     `-------> OFFLINE <-'-----------------'
//! ```
//!
//! Rules:
//! - a grant is never revived: after `REVOKED` only the user can create a new
//!   one, and that means a new grant epoch;
//! - epochs are monotonic: a reconnect or a scope change always increments;
//! - `PAUSED` refuses new live reads *and* new snapshot reads;
//! - `OFFLINE` never becomes `READY` again without authentication.

use figtether_model::errors::{ErrorCode, FigError};

/// State of one plugin session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SessionState {
    /// The connection exists but did not authenticate yet.
    Unpaired,
    /// Authenticated, waiting for the user to grant a scope.
    Paired,
    /// A grant exists but the session is not serving reads yet.
    AwaitingGrant,
    /// Serving reads.
    Ready,
    /// The user paused the session; new reads are refused.
    Paused,
    /// The grant was revoked.
    Revoked,
    /// The plugin stopped answering.
    Offline,
}

impl SessionState {
    /// Stable wire string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unpaired => "unpaired",
            Self::Paired => "paired",
            Self::AwaitingGrant => "awaiting_grant",
            Self::Ready => "ready",
            Self::Paused => "paused",
            Self::Revoked => "revoked",
            Self::Offline => "offline",
        }
    }

    /// Whether the session may serve reads right now.
    pub fn serves_reads(self) -> bool {
        matches!(self, Self::Ready)
    }
}

/// Events that move a session between states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEvent {
    /// The first frame authenticated successfully.
    Authenticated,
    /// The user granted a scope.
    GrantCreated,
    /// The user paused the session.
    Paused,
    /// The user resumed the session.
    Resumed,
    /// The grant was revoked.
    Revoked,
    /// Heartbeats stopped.
    HeartbeatLost,
    /// Heartbeats resumed.
    HeartbeatRestored,
    /// The plugin closed the connection.
    PluginClosed,
}

/// The result of a transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transition {
    /// State before the event.
    pub from: SessionState,
    /// State after the event.
    pub to: SessionState,
    /// Whether the grant epoch must be incremented.
    pub bump_grant_epoch: bool,
    /// Whether the connection epoch must be incremented.
    pub bump_connection_epoch: bool,
}

/// Apply an event to a session state.
///
/// Invalid transitions are refused with a structured error rather than being
/// silently ignored, so a stale client cannot resurrect a revoked grant.
pub fn transition(current: SessionState, event: SessionEvent) -> Result<Transition, FigError> {
    use SessionEvent as E;
    use SessionState as S;

    let (to, bump_grant, bump_conn) = match (current, event) {
        (S::Unpaired, E::Authenticated) => (S::Paired, false, true),
        (S::Paired, E::GrantCreated) => (S::AwaitingGrant, true, false),
        (S::AwaitingGrant, E::Authenticated) => (S::AwaitingGrant, false, true),
        (S::AwaitingGrant, E::Paused) => (S::Paused, true, false),
        (S::AwaitingGrant, E::Revoked) => (S::Revoked, true, false),
        (S::AwaitingGrant, E::HeartbeatLost) => (S::Offline, false, false),
        (S::Ready, E::Paused) => (S::Paused, true, false),
        (S::Ready, E::Revoked) => (S::Revoked, true, false),
        (S::Ready, E::HeartbeatLost) => (S::Offline, false, false),
        (S::Paused, E::Resumed) => (S::AwaitingGrant, true, false),
        (S::Paused, E::Revoked) => (S::Revoked, true, false),
        (S::Paused, E::HeartbeatLost) => (S::Offline, false, false),
        (S::Offline, E::HeartbeatRestored) => (S::Paired, false, true),
        (S::Offline, E::Authenticated) => (S::Paired, false, true),
        (S::Offline, E::Revoked) => (S::Revoked, true, false),
        (S::Revoked, E::Revoked) => (S::Revoked, false, false),
        (S::Unpaired, E::PluginClosed) => (S::Unpaired, false, false),
        (S::Paired, E::PluginClosed) => (S::Paired, false, false),
        (S::AwaitingGrant, E::PluginClosed) => (S::AwaitingGrant, false, false),
        (S::Ready, E::PluginClosed) => (S::Ready, false, false),
        (S::Paused, E::PluginClosed) => (S::Paused, false, false),
        (S::Offline, E::PluginClosed) => (S::Offline, false, false),
        (S::Unpaired, E::Revoked) => (S::Revoked, true, false),
        (S::Paired, E::Revoked) => (S::Revoked, true, false),
        (S::Revoked, E::Authenticated)
        | (S::Revoked, E::GrantCreated)
        | (S::Revoked, E::Resumed) => {
            return Err(FigError::new(
                ErrorCode::Revoked,
                "the grant was revoked; only the user can create a new one",
            ))
        }
        (state, event) => {
            return Err(FigError::new(
                ErrorCode::InvalidLimit,
                format!("event {event:?} is not valid in state {}", state.as_str()),
            ))
        }
    };

    Ok(Transition {
        from: current,
        to,
        bump_grant_epoch: bump_grant,
        bump_connection_epoch: bump_conn,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_requires_authentication_first() {
        assert!(transition(SessionState::Unpaired, SessionEvent::Authenticated).is_ok());
        assert!(transition(SessionState::Unpaired, SessionEvent::GrantCreated).is_err());
    }

    #[test]
    fn grant_creation_bumps_the_grant_epoch() {
        let result = transition(SessionState::Paired, SessionEvent::GrantCreated).expect("valid");
        assert_eq!(result.to, SessionState::AwaitingGrant);
        assert!(result.bump_grant_epoch);
        assert!(!result.bump_connection_epoch);
    }

    #[test]
    fn revoked_sessions_never_revive() {
        let err =
            transition(SessionState::Revoked, SessionEvent::GrantCreated).expect_err("refused");
        assert_eq!(err.code, ErrorCode::Revoked);
    }

    #[test]
    fn reconnect_bumps_the_connection_epoch() {
        let result =
            transition(SessionState::Offline, SessionEvent::HeartbeatRestored).expect("valid");
        assert_eq!(result.to, SessionState::Paired);
        assert!(result.bump_connection_epoch);
    }

    #[test]
    fn only_ready_serves_reads() {
        assert!(SessionState::Ready.serves_reads());
        assert!(!SessionState::Paused.serves_reads());
        assert!(!SessionState::Offline.serves_reads());
        assert!(!SessionState::Revoked.serves_reads());
    }
}
