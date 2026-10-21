//! Mint a one time pairing secret.
//!
//! The secret is shown once, in this terminal, and nowhere else. It is never
//! written to a log file, it never reaches the agent, and it expires after
//! five minutes. A new request replaces an older pending secret.

use figtether_protocol::client::ClientRequest;

use crate::cli::PairArgs;
use crate::commands::{block_on, CommandOutcome};
use crate::control::{self, ControlClient};
use crate::output;

/// Ask the daemon for a pairing secret and print it.
pub fn run(args: PairArgs) -> anyhow::Result<CommandOutcome> {
    block_on(async move {
        let client = ControlClient::connect(args.port, args.runtime_dir.as_deref()).await?;
        let request = ClientRequest::PairSecret {
            request_id: control::new_request_id(),
        };
        let data = client.request(request).await?;
        if args.json {
            output::json(&secret_payload(&data))?;
        } else {
            print_secret(&data);
        }
        Ok(CommandOutcome::ok())
    })
}

/// Restrict machine-readable output to the explicit pairing credential fields.
fn secret_payload(data: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "secret": control::text(data, "secret"),
        "expires_in_seconds": control::number(data, "expires_in_seconds"),
    })
}

/// Print the secret and what the user has to do with it.
pub fn print_secret(data: &serde_json::Value) {
    let secret = control::text(data, "secret");
    let ttl = control::number(data, "expires_in_seconds");
    output::blank();
    output::line(&format!(
        "Pairing secret (single use, valid for {ttl} seconds)"
    ));
    output::secret(&secret);
    output::line("Enter the secret in the FigTether plugin UI to pair this machine.");
    output::line("The secret is shown once, is never stored, and is never written to a");
    output::line("log file. A new secret replaces an older pending one.");
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_secret_carries_its_lifetime() {
        let data = json!({ "secret": "a".repeat(64), "expires_in_seconds": 300 });
        assert_eq!(control::number(&data, "expires_in_seconds"), 300);
        assert_eq!(control::text(&data, "secret").len(), 64);
    }

    #[test]
    fn json_pairing_outputs_only_the_secret_and_ttl() {
        let payload = secret_payload(&json!({
            "secret": "synthetic-pair-fixture", "expires_in_seconds": 300,
            "session_credential": "must-not-be-copied", "internal": "diagnostics"
        }));
        assert_eq!(
            payload,
            json!({"secret":"synthetic-pair-fixture", "expires_in_seconds":300})
        );
        let parsed =
            <crate::cli::Cli as clap::Parser>::try_parse_from(["figtether", "pair", "--json"]);
        assert!(matches!(
            parsed.expect("valid arguments").into_command(),
            crate::cli::Command::Pair(PairArgs { json: true, .. })
        ));
    }
}
