//! Write path payloads for the bridge.
//!
//! The write path is plan based: the daemon validates a plan against the
//! write grant, the plugin previews it without mutating the document, the
//! user approves or rejects it in the plugin UI, and the plugin applies the
//! approved plan as a single undo step
//! (`docs/11-write-path/01-write-path-design.md`, `ADR-013`).
//!
//! The plan and the preview travel as `serde_json::Value` on purpose: the
//! protocol crate stays independent of the model crate, and the shape of a
//! plan is owned by the model layer. The protocol owns the envelope, the
//! approval binding, and the token that proves the user said yes.
//!
//! The approval token is opaque but not secret. It is a deterministic string
//! that both sides can rebuild from the same five inputs, so the daemon can
//! verify that a token belongs to the plan, the session, and the grant epoch
//! it expects. The daemon keeps the authoritative approval record and the
//! expiry, so the token itself never carries authority on its own.

use std::time::Duration;

use chrono::{DateTime, Utc};
use figtether_model::ids::SessionId;
use serde::{Deserialize, Serialize};

use crate::BridgeOp;

/// Prefix of every approval token.
const TOKEN_PREFIX: &str = "w:";

/// How long an approval stays valid.
pub const WRITE_APPROVAL_TTL: Duration = Duration::from_secs(600);

/// Grant mode that only allows reads inside the granted roots.
pub const GRANT_MODE_READ: &str = "read";

/// Grant mode that allows writes inside the granted roots, after approval.
pub const GRANT_MODE_WRITE: &str = "write";

/// A write plan on its way to the plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WritePlanPayload {
    /// The plan, as the model layer serializes it.
    pub plan: serde_json::Value,
    /// Approval the user has to grant for this plan.
    ///
    /// The daemon mints the id when it previews a plan and sends it with the
    /// plan, so the plugin can echo it in the approval decision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<String>,
    /// Hash the daemon computed for the plan.
    ///
    /// The plugin compares it with its own hash of the plan it received, so a
    /// plan that changed in transit is refused instead of previewed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_hash: Option<String>,
}

/// The preview of a plan, as the plugin resolved it against the live document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WritePreviewPayload {
    /// The preview, as the model layer serializes it.
    pub preview: serde_json::Value,
}

/// An approved plan on its way to the plugin for execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WriteApplyPayload {
    /// The plan that was approved, in the exact shape it was previewed.
    pub plan: serde_json::Value,
    /// The token that binds the approval to the plan, the session, and the grant epoch.
    pub approval_token: String,
}

/// The outcome of an applied plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WriteResultPayload {
    /// The result, as the model layer serializes it.
    pub result: serde_json::Value,
}

/// The decision the user took in the plugin UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalDecisionPayload {
    /// Identity of the approval request the decision answers.
    pub approval_id: String,
    /// Hash of the plan the user was shown.
    pub plan_hash: String,
    /// Whether the user approved the plan.
    pub approved: bool,
    /// Why the user rejected the plan, when they said why.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// The parts an approval token is built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalTokenParts {
    /// Identity of the approval request.
    pub approval_id: String,
    /// Hash of the approved plan.
    pub plan_hash: String,
    /// Session the approval belongs to.
    pub session_id: SessionId,
    /// Grant epoch the approval is bound to.
    pub grant_epoch: u64,
    /// Moment the approval was issued.
    pub issued_at: DateTime<Utc>,
}

impl ApprovalTokenParts {
    /// The moment this approval stops being usable under the fixed TTL.
    pub fn expires_at(&self) -> DateTime<Utc> {
        // The TTL is a fixed ten minutes, so the conversion never fails. The
        // fallback keeps the function total instead of panicking on a peer.
        self.issued_at + chrono::Duration::from_std(WRITE_APPROVAL_TTL).unwrap_or_default()
    }
}

/// The wire name of a write operation, or `None` when the operation is a read.
///
/// The daemon uses this to tell a write request apart from a read request
/// without matching on the enum twice.
pub fn write_op_name(op: BridgeOp) -> Option<&'static str> {
    match op {
        BridgeOp::PreviewWrite => Some("preview_write"),
        BridgeOp::ApplyWrite => Some("apply_write"),
        _ => None,
    }
}

/// Build the approval token of an approval.
///
/// The token is deterministic: the daemon and the plugin rebuild it from the
/// same inputs and compare the strings. It is not a secret, it carries no key
/// material, and it grants nothing on its own: the daemon holds the
/// authoritative approval record, checks the hash, the session, the grant
/// epoch, and the expiry, and refuses everything else.
///
/// The format is `w:<approval_id>:<plan_hash>:<session_id>:<grant_epoch>:<issued_at>`,
/// where the timestamp is RFC 3339. The plan hash is embedded without its
/// `sha256:` prefix, because the prefix contains a colon and the fields are
/// separated by colons. The parser restores the prefix, so the parsed hash
/// compares equal to `figtether_model::write::canonical_plan_hash`.
pub fn approval_token(
    approval_id: &str,
    plan_hash: &str,
    session_id: &SessionId,
    grant_epoch: u64,
    issued_at: DateTime<Utc>,
) -> String {
    format!(
        "{TOKEN_PREFIX}{approval_id}:{}:{session_id}:{grant_epoch}:{}",
        hash_field(plan_hash),
        issued_at.to_rfc3339()
    )
}

/// The hash as it travels inside a token: no `sha256:` prefix.
///
/// A colon inside the hash would break the field split, and the canonical
/// hash carries exactly one colon.
fn hash_field(plan_hash: &str) -> &str {
    plan_hash.strip_prefix("sha256:").unwrap_or(plan_hash)
}

/// The hash as the rest of the code compares it: always with the prefix.
fn full_hash(field: &str) -> String {
    if field.starts_with("sha256:") {
        field.to_string()
    } else {
        format!("sha256:{field}")
    }
}

/// Parse an approval token back into its parts.
///
/// The token has to carry the `w:` prefix, and its body has to hold exactly
/// five fields: the approval id, the plan hash, the session id, the grant
/// epoch, and the RFC 3339 timestamp. The split is bounded to five fields
/// because the timestamp itself contains colons, so a sixth field can only
/// appear inside the timestamp and makes the timestamp unparseable. A token
/// that does not have that shape, or whose fields do not parse, is refused.
///
/// The plan hash is returned in the canonical `sha256:<hex>` form, whether the
/// token carried the prefix or only the bare digest.
pub fn parse_approval_token(token: &str) -> Option<ApprovalTokenParts> {
    let body = token.strip_prefix(TOKEN_PREFIX)?;
    let mut fields = body.splitn(5, ':');
    let approval_id = fields.next()?.to_string();
    let plan_hash = fields.next()?.to_string();
    let session_id = fields.next()?.to_string();
    let grant_epoch = fields.next()?.to_string();
    let issued_at = fields.next()?.to_string();
    if approval_id.is_empty()
        || plan_hash.is_empty()
        || session_id.is_empty()
        || grant_epoch.is_empty()
        || issued_at.is_empty()
    {
        return None;
    }
    let session_id = session_id.parse::<SessionId>().ok()?;
    let grant_epoch = grant_epoch.parse::<u64>().ok()?;
    let issued_at = DateTime::parse_from_rfc3339(&issued_at)
        .ok()?
        .with_timezone(&Utc);
    Some(ApprovalTokenParts {
        approval_id,
        plan_hash: full_hash(&plan_hash),
        session_id,
        grant_epoch,
        issued_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> SessionId {
        SessionId::new()
    }

    fn issued_at() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-20T12:34:56.789+00:00")
            .expect("timestamp parses")
            .with_timezone(&Utc)
    }

    #[test]
    fn the_token_round_trips() {
        let session_id = session();
        let token = approval_token("apr_1", "abc123", &session_id, 7, issued_at());
        let parts = parse_approval_token(&token).expect("token parses");
        assert_eq!(parts.approval_id, "apr_1");
        assert_eq!(
            parts.plan_hash, "sha256:abc123",
            "the parsed hash is the canonical form, with the prefix"
        );
        assert_eq!(parts.session_id, session_id);
        assert_eq!(parts.grant_epoch, 7);
        assert_eq!(parts.issued_at, issued_at());
    }

    #[test]
    fn a_canonical_hash_survives_the_token() {
        // The real case: the daemon hashes the plan with
        // `canonical_plan_hash`, which returns the prefixed form.
        let session_id = session();
        let hash = format!("sha256:{}", "ab".repeat(32));
        let token = approval_token("apr_1", &hash, &session_id, 7, issued_at());
        assert!(
            !token[2..].contains(":sha256:"),
            "the token must not carry a colon inside the hash field: {token}"
        );
        let parts = parse_approval_token(&token).expect("token parses");
        assert_eq!(parts.plan_hash, hash);
    }

    #[test]
    fn the_token_has_the_documented_shape() {
        let session_id = session();
        let token = approval_token("apr_1", "abc123", &session_id, 7, issued_at());
        let expected = format!("w:apr_1:abc123:{session_id}:7:2026-10-20T12:34:56.789+00:00");
        assert_eq!(token, expected);
    }

    #[test]
    fn the_token_is_deterministic() {
        let session_id = session();
        let first = approval_token("apr_1", "abc123", &session_id, 7, issued_at());
        let second = approval_token("apr_1", "abc123", &session_id, 7, issued_at());
        assert_eq!(first, second);
    }

    #[test]
    fn a_different_binding_makes_a_different_token() {
        let session_id = session();
        let base = approval_token("apr_1", "abc123", &session_id, 7, issued_at());
        let other_epoch = approval_token("apr_1", "abc123", &session_id, 8, issued_at());
        let other_hash = approval_token("apr_1", "def456", &session_id, 7, issued_at());
        let other_session = approval_token("apr_1", "abc123", &session(), 7, issued_at());
        assert_ne!(base, other_epoch);
        assert_ne!(base, other_hash);
        assert_ne!(base, other_session);
    }

    #[test]
    fn a_token_without_the_prefix_is_refused() {
        let session_id = session();
        let token = approval_token("apr_1", "abc123", &session_id, 7, issued_at());
        let stripped = token.trim_start_matches("w:");
        assert!(parse_approval_token(stripped).is_none());
        assert!(parse_approval_token("").is_none());
        assert!(parse_approval_token("not-a-token").is_none());
    }

    #[test]
    fn a_token_with_too_few_parts_is_refused() {
        assert!(parse_approval_token("w:apr_1:abc123:session:7").is_none());
        assert!(parse_approval_token("w:apr_1:abc123").is_none());
        assert!(parse_approval_token("w:").is_none());
    }

    #[test]
    fn a_token_with_too_many_parts_is_refused() {
        let session_id = session();
        let token = format!("w:apr_1:abc123:{session_id}:7:2026-10-20T12:34:56+00:00:extra");
        assert!(parse_approval_token(&token).is_none());
    }

    #[test]
    fn a_token_with_a_broken_field_is_refused() {
        let session_id = session();
        let bad_time = format!("w:apr_1:abc123:{session_id}:7:not-a-timestamp");
        let bad_epoch = format!("w:apr_1:abc123:{session_id}:seven:2026-10-20T12:34:56+00:00");
        let bad_session = "w:apr_1:abc123:not-a-session:7:2026-10-20T12:34:56+00:00";
        let empty_field = format!("w::abc123:{session_id}:7:2026-10-20T12:34:56+00:00");
        assert!(parse_approval_token(&bad_time).is_none());
        assert!(parse_approval_token(&bad_epoch).is_none());
        assert!(parse_approval_token(bad_session).is_none());
        assert!(parse_approval_token(&empty_field).is_none());
    }

    #[test]
    fn the_expiry_is_the_issued_at_plus_the_ttl() {
        let session_id = session();
        let token = approval_token("apr_1", "abc123", &session_id, 7, issued_at());
        let parts = parse_approval_token(&token).expect("token parses");
        let expires_at = parts.expires_at();
        let expected = issued_at() + chrono::Duration::seconds(600);
        assert_eq!(expires_at, expected);
    }

    #[test]
    fn the_approval_ttl_is_ten_minutes() {
        assert_eq!(WRITE_APPROVAL_TTL, Duration::from_secs(600));
    }

    #[test]
    fn the_grant_modes_have_stable_names() {
        assert_eq!(GRANT_MODE_READ, "read");
        assert_eq!(GRANT_MODE_WRITE, "write");
    }

    #[test]
    fn the_write_operations_have_stable_names() {
        assert_eq!(write_op_name(BridgeOp::PreviewWrite), Some("preview_write"));
        assert_eq!(write_op_name(BridgeOp::ApplyWrite), Some("apply_write"));
        assert_eq!(write_op_name(BridgeOp::ListNodes), None);
        assert_eq!(write_op_name(BridgeOp::ReadNodes), None);
    }

    #[test]
    fn the_plan_payload_round_trips() {
        let payload = WritePlanPayload {
            plan: serde_json::json!({
                "plan_id": "plan_1",
                "operations": [{"kind": "create_node", "node_type": "FRAME"}]
            }),
            approval_id: None,
            plan_hash: None,
        };
        let json = serde_json::to_string(&payload).expect("encodes");
        assert!(json.contains("\"plan\""), "{json}");
        let decoded: WritePlanPayload = serde_json::from_str(&json).expect("decodes");
        assert_eq!(decoded, payload);
    }

    #[test]
    fn the_preview_payload_round_trips() {
        let payload = WritePreviewPayload {
            preview: serde_json::json!({
                "plan_hash": "abc123",
                "created": ["1:2"],
                "operations_count": 1,
                "blocked": false
            }),
        };
        let json = serde_json::to_string(&payload).expect("encodes");
        assert!(json.contains("\"preview\""), "{json}");
        let decoded: WritePreviewPayload = serde_json::from_str(&json).expect("decodes");
        assert_eq!(decoded, payload);
    }

    #[test]
    fn the_apply_payload_round_trips() {
        let payload = WriteApplyPayload {
            plan: serde_json::json!({"plan_id": "plan_1"}),
            approval_token: "w:apr_1:abc123:session:7:2026-10-20T12:34:56+00:00".to_string(),
        };
        let json = serde_json::to_string(&payload).expect("encodes");
        assert!(json.contains("\"approval_token\""), "{json}");
        let decoded: WriteApplyPayload = serde_json::from_str(&json).expect("decodes");
        assert_eq!(decoded, payload);
    }

    #[test]
    fn the_result_payload_round_trips() {
        let payload = WriteResultPayload {
            result: serde_json::json!({
                "plan_id": "plan_1",
                "created": ["1:2"],
                "undo_committed": true
            }),
        };
        let json = serde_json::to_string(&payload).expect("encodes");
        assert!(json.contains("\"result\""), "{json}");
        let decoded: WriteResultPayload = serde_json::from_str(&json).expect("decodes");
        assert_eq!(decoded, payload);
    }

    #[test]
    fn the_approval_decision_round_trips() {
        let payload = ApprovalDecisionPayload {
            approval_id: "apr_1".to_string(),
            plan_hash: "abc123".to_string(),
            approved: true,
            reason: None,
        };
        let json = serde_json::to_string(&payload).expect("encodes");
        assert!(json.contains("\"approved\":true"), "{json}");
        assert!(!json.contains("\"reason\""), "{json}");
        let decoded: ApprovalDecisionPayload = serde_json::from_str(&json).expect("decodes");
        assert_eq!(decoded, payload);
    }

    #[test]
    fn a_rejection_keeps_its_reason() {
        let payload = ApprovalDecisionPayload {
            approval_id: "apr_1".to_string(),
            plan_hash: "abc123".to_string(),
            approved: false,
            reason: Some("wrong parent".to_string()),
        };
        let json = serde_json::to_string(&payload).expect("encodes");
        let decoded: ApprovalDecisionPayload = serde_json::from_str(&json).expect("decodes");
        assert_eq!(decoded, payload);
    }
}
