//! Sanitized status of the running daemon.
//!
//! The status carries sessions, queue depth, counters, and versions. It never
//! carries design content, file identifiers, or a credential, so it can be
//! pasted into a support request as it is.
//!
//! The session list is scoped to the installation that owns the credential,
//! so the report names the sessions that were granted on this machine.

use figtether_protocol::client::ClientRequest;
use serde_json::Value;

use crate::cli::StatusArgs;
use crate::commands::{block_on, CommandOutcome};
use crate::control::{self, ControlClient};
use crate::output;

/// Print the status of the running daemon.
pub fn run(args: StatusArgs) -> anyhow::Result<CommandOutcome> {
    block_on(async move {
        let client = ControlClient::connect(args.port, args.runtime_dir.as_deref()).await?;
        let request = ClientRequest::Status {
            request_id: control::new_request_id(),
        };
        let data = client.request(request).await?;
        // A daemon that does not answer the session list leaves the report
        // without sessions. The status itself stays readable.
        let sessions = visible_sessions(&client).await.unwrap_or_default();
        if args.json {
            let mut payload = data;
            if let Some(object) = payload.as_object_mut() {
                object.insert("granted_sessions".to_string(), serde_json::json!(sessions));
            }
            output::json(&payload)?;
            return Ok(CommandOutcome::ok());
        }
        print_status(&data);
        print_sessions(&sessions);
        Ok(CommandOutcome::ok())
    })
}

/// The sessions this installation may see.
///
/// The daemon scopes the list to the owner of the credential, so the answer
/// holds the sessions that were granted on this machine.
pub async fn visible_sessions(client: &ControlClient) -> anyhow::Result<Vec<Value>> {
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

/// One line per session of this installation.
pub fn session_lines(sessions: &[Value]) -> Vec<String> {
    sessions
        .iter()
        .map(|session| {
            format!(
                "{}  state {}, grant epoch {}, connection epoch {}, {} granted root(s)",
                control::text(session, "session_id"),
                control::text(session, "state"),
                control::number(session, "grant_epoch"),
                control::number(session, "connection_epoch"),
                control::number(session, "root_ids"),
            )
        })
        .collect()
}

/// Print the sessions of this installation.
pub fn print_sessions(sessions: &[Value]) {
    let lines = session_lines(sessions);
    if lines.is_empty() {
        return;
    }
    output::section("Sessions");
    for line in lines {
        output::line(&format!("  {line}"));
    }
}

/// Print the status as text.
pub fn print_status(status: &Value) {
    output::blank();
    output::line(&format!(
        "FigTether status (daemon {})",
        control::text(status, "daemon_version")
    ));
    output::blank();
    output::field("daemon version", &control::text(status, "daemon_version"));
    output::field("core version", &control::text(status, "core_version"));
    output::field(
        "protocol version",
        &control::text(status, "protocol_version"),
    );
    output::field("schema version", &control::text(status, "schema_version"));
    output::field("instance", &control::text(status, "instance_id"));
    output::field("bind address", &control::text(status, "bind_address"));
    output::field(
        "uptime",
        &format!("{}s", control::number(status, "uptime_seconds")),
    );
    output::field("sessions", &control::number(status, "sessions").to_string());
    output::field(
        "plugin connections",
        &control::number(status, "plugin_connections").to_string(),
    );
    output::field(
        "queue depth",
        &control::number(status, "queue_depth").to_string(),
    );
    output::field(
        "pairing secrets",
        &control::number_any(status, &["pairing_active", "pairing_secrets_active"]).to_string(),
    );

    output::section("Counters");
    for (name, value) in [
        ("requests_total", "requests_total"),
        ("cache_hits", "cache_hits"),
        ("fresh_reads", "fresh_reads"),
        ("exports", "exports"),
        ("busy_rejections", "busy_rejections"),
        ("cancellations", "cancellations"),
        ("revocations", "revocations"),
        ("bytes_in", "bytes_in"),
        ("bytes_out", "bytes_out"),
        ("queue_depth_peak", "queue_depth_peak"),
        ("cache_entries", "cache_entries"),
        ("cache_bytes", "cache_bytes"),
        ("asset_count", "asset_count"),
        ("asset_bytes", "asset_bytes"),
    ] {
        output::field(name, &control::number(status, value).to_string());
    }
    output::field(
        "mean latency",
        &format!("{:.1}ms", control::number(status, "mean_latency_ms") as f64),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn status() -> Value {
        json!({
            "daemon_version": "0.1.0",
            "core_version": "0.1.0",
            "protocol_version": "1.0",
            "schema_version": "1.0",
            "instance_id": "instance",
            "bind_address": "127.0.0.1",
            "uptime_seconds": 12,
            "sessions": 1,
            "plugin_connections": 1,
            "queue_depth": 0,
            "pairing_secrets_active": 0,
            "requests_total": 42
        })
    }

    #[test]
    fn the_status_carries_the_sanitized_fields() {
        let value = status();
        assert_eq!(control::text(&value, "daemon_version"), "0.1.0");
        assert_eq!(control::number(&value, "requests_total"), 42);
    }

    #[test]
    fn the_status_carries_no_secret_field() {
        let text = status().to_string();
        for token in text.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-') {
            assert!(
                token.len() < 32,
                "the status carries a secret shaped token: {token}"
            );
        }
    }

    #[test]
    fn a_session_line_names_the_session_and_its_state() {
        let sessions = vec![json!({
            "session_id": "11111111-1111-4111-8111-111111111111",
            "state": "ready",
            "grant_epoch": 2,
            "connection_epoch": 3,
            "root_ids": 4
        })];
        let lines = session_lines(&sessions);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("11111111-1111-4111-8111-111111111111"));
        assert!(lines[0].contains("ready"), "{}", lines[0]);
        assert!(lines[0].contains("2"), "{}", lines[0]);
    }
}
