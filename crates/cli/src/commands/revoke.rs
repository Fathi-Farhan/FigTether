//! Revoke the grant of a session.
//!
//! A revoke cancels the queued work of the session, drops the answers that
//! were not sent yet, invalidates the cache entries and the assets of the
//! grant, and makes every later read answer with `REVOKED`. It works even
//! when the plugin is unresponsive, because the daemon owns the grant.
//!
//! The daemon scopes the session list to the installation that owns the
//! credential. `figtether revoke --all` therefore reaches the sessions that
//! were granted on this machine, including the session of the plugin that
//! paired here.

use figtether_model::ids::SessionId;
use figtether_protocol::client::ClientRequest;
use serde_json::Value;

use crate::cli::RevokeArgs;
use crate::commands::{block_on, CommandOutcome};
use crate::control::{self, ControlClient};
use crate::output;

/// Revoke one session or every session.
pub fn run(args: RevokeArgs) -> anyhow::Result<CommandOutcome> {
    block_on(async move {
        let client = ControlClient::connect(args.port, args.runtime_dir.as_deref()).await?;
        if let Some(session_id) = args.session {
            let outcome = revoke_one(&client, session_id).await?;
            output::blank();
            print_revoked(&session_id.to_string(), &outcome);
            return Ok(CommandOutcome::ok());
        }

        let sessions = list_sessions(&client).await?;
        if sessions.is_empty() {
            output::blank();
            output::line(
                "no session of this installation is known to the daemon, nothing was revoked",
            );
            output::line(
                "The session list of the daemon is scoped to the installation that owns the credential.",
            );
            output::line("Use figtether revoke --session <id> to revoke a session by its id.");
            return Ok(CommandOutcome::ok());
        }
        let outcomes = revoke_all(&client).await?;
        output::blank();
        for (session_id, outcome) in &outcomes {
            print_revoked(&session_id.to_string(), outcome);
        }
        output::blank();
        output::line(&format!("revoked {} session(s)", outcomes.len()));
        output::line("Further reads of the revoked sessions answer with REVOKED.");
        Ok(CommandOutcome::ok())
    })
}

/// Revoke one session.
async fn revoke_one(client: &ControlClient, session_id: SessionId) -> anyhow::Result<Value> {
    let request = ClientRequest::Revoke {
        request_id: control::new_request_id(),
        session_id,
    };
    Ok(client.request(request).await?)
}

/// The sessions the daemon serves to this installation.
///
/// The daemon scopes the list to the owner of the credential, so the answer
/// holds the sessions that were granted on this machine.
pub async fn list_sessions(client: &ControlClient) -> anyhow::Result<Vec<Value>> {
    let request = ClientRequest::ListSessions {
        request_id: control::new_request_id(),
        cursor: None,
    };
    let data = client.request(request).await?;
    let sessions = data
        .get("sessions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(sessions)
}

/// Revoke every session of this installation.
///
/// The function returns the id and the outcome of every revoke.
pub async fn revoke_all(client: &ControlClient) -> anyhow::Result<Vec<(SessionId, Value)>> {
    let sessions = list_sessions(client).await?;
    let mut revoked = Vec::with_capacity(sessions.len());
    for session in sessions {
        let session_id = session
            .get("session_id")
            .and_then(Value::as_str)
            .and_then(|raw| raw.parse::<SessionId>().ok())
            .ok_or_else(|| anyhow::anyhow!("the daemon reported a session id that is not valid"))?;
        let outcome = revoke_one(client, session_id).await?;
        revoked.push((session_id, outcome));
    }
    Ok(revoked)
}

/// Print what one revoke changed.
fn print_revoked(session_id: &str, outcome: &Value) {
    let revoked = outcome.get("revoked").unwrap_or(&Value::Null);
    output::line(&format!(
        "revoked the grant of session {session_id} ({} job(s) cancelled, {} cache entries and {} assets invalidated)",
        control::number(revoked, "cancelled_jobs"),
        control::number(revoked, "invalidated_cache_entries"),
        control::number(revoked, "invalidated_assets"),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_revoke_reports_what_it_changed() {
        let outcome = json!({
            "revoked": {
                "cancelled_jobs": 2,
                "invalidated_cache_entries": 5,
                "invalidated_assets": 1
            }
        });
        let revoked = outcome.get("revoked").expect("the outcome");
        assert_eq!(control::number(revoked, "cancelled_jobs"), 2);
        assert_eq!(control::number(revoked, "invalidated_cache_entries"), 5);
        assert_eq!(control::number(revoked, "invalidated_assets"), 1);
    }
}
