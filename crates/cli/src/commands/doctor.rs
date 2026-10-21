//! Sanitized health report.
//!
//! The doctor is the paste surface for a support request, so the allowlist is
//! enforced here and not by user caution: versions, the runtime directory and
//! its permissions, the bridge port, the plugin manifest, the pairing state,
//! and the detected capabilities. No secret, no design content, no file
//! identifier, and no layer name ever reaches the output.
//!
//! The command exits with a non zero status when a blocking check fails.

use std::path::{Path, PathBuf};

use figtether_daemon::port::PortProblem;
use figtether_protocol::client::ClientRequest;
use serde::Serialize;
use serde_json::Value;

use crate::cli::DoctorArgs;
use crate::commands::{block_on, CommandOutcome};
use crate::control::{self, ControlClient};
use crate::output;
use crate::runtime;

/// Status of one check.
const OK: &str = "ok";
/// Status of one check that does not block.
const WARN: &str = "warn";
/// Status of one check that blocks.
const FAIL: &str = "fail";

/// One check of the report.
#[derive(Debug, Clone, Serialize)]
pub struct CheckEntry {
    /// Name of the check.
    pub name: String,
    /// One of `ok`, `warn`, or `fail`.
    pub status: String,
    /// Sanitized explanation.
    pub detail: String,
}

/// The whole report.
#[derive(Debug, Clone, Serialize)]
pub struct DoctorReport {
    /// Whether every blocking check passed.
    pub ok: bool,
    /// Version of the CLI.
    pub cli_version: String,
    /// Version of the daemon.
    pub daemon_version: String,
    /// Version of the MCP adapter.
    pub mcp_version: String,
    /// Version of the core.
    pub core_version: String,
    /// Wire protocol version.
    pub protocol_version: String,
    /// Schema version.
    pub schema_version: String,
    /// Resolved runtime directory.
    pub runtime_dir: String,
    /// The port the daemon is expected to use.
    pub port: u16,
    /// A free port the user can pass to `--port`, when the expected port
    /// cannot be bound.
    pub port_suggestion: Option<u16>,
    /// The individual checks.
    pub checks: Vec<CheckEntry>,
}

/// Run the doctor.
pub fn run(args: DoctorArgs) -> anyhow::Result<CommandOutcome> {
    block_on(async move {
        let report = build_report(&args).await?;
        if args.json {
            output::json(&report)?;
        } else {
            print_report(&report);
        }
        if report.ok {
            return Ok(CommandOutcome::ok());
        }
        let failed = report
            .checks
            .iter()
            .filter(|check| check.status == FAIL)
            .count();
        Ok(CommandOutcome::failed(format!(
            "{failed} blocking check(s) failed, see the report above"
        )))
    })
}

/// Build the report.
pub async fn build_report(args: &DoctorArgs) -> anyhow::Result<DoctorReport> {
    let mut checks = Vec::new();
    checks.push(version_check());

    let directory = runtime::check(args.runtime_dir.as_deref());
    checks.push(CheckEntry {
        name: "runtime directory".to_string(),
        status: if directory.is_blocking() { FAIL } else { OK }.to_string(),
        detail: format!(
            "{} ({}, {})",
            directory.path,
            directory.state.as_str(),
            directory.detail
        ),
    });

    let port = control::expected_port(args.port, args.runtime_dir.as_deref());
    let probe = probe_daemon(port, args.runtime_dir.as_deref()).await;
    let PortCheck { entry, suggestion } = port_check(port, &probe);

    checks.push(entry);
    checks.push(manifest_check_for(args, port));
    checks.push(daemon_check(&probe));
    checks.push(pairing_check(&probe));
    checks.push(capability_check(&probe));

    let ok = !checks.iter().any(|check| check.status == FAIL);
    Ok(DoctorReport {
        ok,
        cli_version: env!("CARGO_PKG_VERSION").to_string(),
        daemon_version: figtether_daemon::DAEMON_VERSION.to_string(),
        mcp_version: figtether_mcp::MCP_VERSION.to_string(),
        core_version: figtether_core::CORE_VERSION.to_string(),
        protocol_version: figtether_protocol::PROTOCOL_VERSION.to_string(),
        schema_version: figtether_model::SCHEMA_VERSION.to_string(),
        runtime_dir: directory.path,
        port,
        port_suggestion: suggestion,
        checks,
    })
}

/// Print the report as text.
pub fn print_report(report: &DoctorReport) {
    output::blank();
    output::line(&format!("FigTether doctor (cli {})", report.cli_version));
    output::blank();
    for check in &report.checks {
        output::line(&format!(
            "[{}] {}: {}",
            check.status, check.name, check.detail
        ));
    }
    output::blank();
    if report.ok {
        output::line("every blocking check passed");
    } else {
        output::line("at least one blocking check failed, see the lines above");
    }
}

/// The versions this build speaks.
fn version_check() -> CheckEntry {
    CheckEntry {
        name: "runtime version".to_string(),
        status: OK.to_string(),
        detail: format!(
            "cli {}, daemon {}, mcp {}, core {}, protocol {}, schema {}",
            env!("CARGO_PKG_VERSION"),
            figtether_daemon::DAEMON_VERSION,
            figtether_mcp::MCP_VERSION,
            figtether_core::CORE_VERSION,
            figtether_protocol::PROTOCOL_VERSION,
            figtether_model::SCHEMA_VERSION,
        ),
    }
}

/// The result of the bridge port check.
struct PortCheck {
    /// The check shown in the report.
    entry: CheckEntry,
    /// A port that can be bound instead, when the expected port cannot.
    suggestion: Option<u16>,
}

/// The bridge port.
///
/// The check names the reason the port cannot be bound and the port to use
/// instead. A port that the operating system reserved is told apart from a
/// port that another process holds, because the two need different steps.
fn port_check(port: u16, probe: &DaemonProbe) -> PortCheck {
    let problem = figtether_daemon::port::diagnose(port);
    let served = probe.status.is_some();
    let suggestion = match problem {
        Some(_) if !served => Some(figtether_daemon::port::suggest_free_port(port)),
        _ => None,
    };
    port_check_from(port, problem, served, suggestion)
}

/// Build the port check from a known problem and a known suggestion.
fn port_check_from(
    port: u16,
    problem: Option<PortProblem>,
    served_by_daemon: bool,
    suggestion: Option<u16>,
) -> PortCheck {
    let name = "bridge port".to_string();
    match problem {
        None => PortCheck {
            entry: CheckEntry {
                name,
                status: OK.to_string(),
                detail: format!("port {port} on 127.0.0.1 is free"),
            },
            suggestion: None,
        },
        Some(_) if served_by_daemon => PortCheck {
            entry: CheckEntry {
                name,
                status: OK.to_string(),
                detail: format!("port {port} on 127.0.0.1 is served by the FigTether daemon"),
            },
            suggestion: None,
        },
        Some(problem) => {
            let suggestion = suggestion.unwrap_or(port);
            PortCheck {
                entry: CheckEntry {
                    name,
                    status: FAIL.to_string(),
                    detail: format!(
                        "port {port} on 127.0.0.1 cannot be bound: {problem} Start the daemon on port {suggestion} and regenerate the plugin manifest for the same port: figtether daemon --port {suggestion}"
                    ),
                },
                suggestion: Some(suggestion),
            }
        }
    }
}

/// The plugin manifest.
fn manifest_check(path: &Path, port: u16) -> CheckEntry {
    let name = "plugin manifest".to_string();
    if !path.exists() {
        return CheckEntry {
            name,
            status: WARN.to_string(),
            detail: format!(
                "no manifest at {}; generate one with figtether manifest --plugin-id <plugin-id>",
                path.display()
            ),
        };
    }
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) => {
            return CheckEntry {
                name,
                status: FAIL.to_string(),
                detail: format!("the manifest {} cannot be read: {err}", path.display()),
            }
        }
    };
    if crate::commands::manifest::has_byte_order_mark(&bytes) {
        return CheckEntry {
            name,
            status: FAIL.to_string(),
            detail: format!(
                "the manifest {} starts with a byte order mark; Figma cannot parse it. Write the \
                 file again without the mark, for example with figtether manifest --plugin-id \
                 <plugin-id> --out {}",
                path.display(),
                path.display()
            ),
        };
    }
    let text = match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(err) => {
            return CheckEntry {
                name,
                status: FAIL.to_string(),
                detail: format!("the manifest {} is not valid UTF-8: {err}", path.display()),
            }
        }
    };
    let manifest: Value = match serde_json::from_str(&text) {
        Ok(manifest) => manifest,
        Err(err) => {
            return CheckEntry {
                name,
                status: FAIL.to_string(),
                detail: format!("the manifest {} is not valid JSON: {err}", path.display()),
            }
        }
    };

    let editor_ok = manifest["editorType"].as_array().is_some_and(|editors| {
        editors
            .iter()
            .any(|editor| editor.as_str() == Some("figma"))
    });
    if !editor_ok {
        return CheckEntry {
            name,
            status: FAIL.to_string(),
            detail: "the manifest does not target the figma editor".to_string(),
        };
    }
    if manifest["documentAccess"].as_str() != Some("dynamic-page") {
        return CheckEntry {
            name,
            status: FAIL.to_string(),
            detail: "the manifest does not use the dynamic-page document access".to_string(),
        };
    }
    let entries = network_entries(&manifest);
    if entries.is_empty() {
        return CheckEntry {
            name,
            status: FAIL.to_string(),
            detail: "the manifest grants no loopback access, the plugin cannot reach the daemon"
                .to_string(),
        };
    }
    for (host, _) in &entries {
        if !is_loopback(host) {
            return CheckEntry {
                name,
                status: FAIL.to_string(),
                detail: format!(
                    "the manifest grants access to {host}, only the loopback address is allowed"
                ),
            };
        }
    }
    let ports: Vec<u16> = entries.iter().filter_map(|(_, port)| *port).collect();
    if !ports.contains(&port) {
        return CheckEntry {
            name,
            status: FAIL.to_string(),
            detail: format!(
                "the manifest does not grant port {port}; regenerate it with figtether manifest --plugin-id <plugin-id> --port {port}"
            ),
        };
    }
    CheckEntry {
        name,
        status: OK.to_string(),
        detail: format!(
            "{} (dynamic-page, figma, loopback {LOOPBACK}:{port})",
            path.display()
        ),
    }
}

/// The loopback address the plugin is allowed to reach.
const LOOPBACK: &str = "127.0.0.1";

/// Every host and port the manifest allows.
fn network_entries(manifest: &Value) -> Vec<(String, Option<u16>)> {
    let network = &manifest["networkAccess"];
    let mut entries = Vec::new();
    for key in ["allowedDomains", "devAllowedDomains"] {
        let Some(list) = network[key].as_array() else {
            continue;
        };
        for item in list {
            let Some(text) = item.as_str() else {
                continue;
            };
            if text.eq_ignore_ascii_case("none") {
                continue;
            }
            if let Some(entry) = parse_endpoint(text) {
                entries.push(entry);
            }
        }
    }
    entries
}

/// Split an endpoint into its host and its optional port.
fn parse_endpoint(text: &str) -> Option<(String, Option<u16>)> {
    let rest = text.rsplit("://").next().unwrap_or(text);
    let rest = rest.split('/').next().unwrap_or(rest);
    if let Some(inner) = rest.strip_prefix('[') {
        let (host, tail) = inner.split_once(']')?;
        let port = tail
            .strip_prefix(':')
            .and_then(|port| port.parse::<u16>().ok());
        return Some((format!("[{host}]"), port));
    }
    let mut parts = rest.split(':');
    let host = parts.next()?.trim().to_string();
    let port = parts
        .next()
        .and_then(|port| port.trim().parse::<u16>().ok());
    Some((host, port))
}

/// Whether a host is a loopback address.
fn is_loopback(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]")
}

/// The daemon.
fn daemon_check(probe: &DaemonProbe) -> CheckEntry {
    match &probe.status {
        Some(status) => CheckEntry {
            name: "daemon".to_string(),
            status: OK.to_string(),
            detail: format!(
                "the daemon runs on {} (version {}, protocol {})",
                control::text(status, "bind_address"),
                control::text(status, "daemon_version"),
                control::text(status, "protocol_version")
            ),
        },
        None => CheckEntry {
            name: "daemon".to_string(),
            status: WARN.to_string(),
            detail: format!(
                "no daemon answers on the loopback interface ({}); start it with figtether",
                probe
                    .error
                    .clone()
                    .unwrap_or_else(|| "the daemon does not run".to_string())
            ),
        },
    }
}

/// The pairing state.
fn pairing_check(probe: &DaemonProbe) -> CheckEntry {
    let Some(status) = &probe.status else {
        return CheckEntry {
            name: "pairing".to_string(),
            status: WARN.to_string(),
            detail: "the pairing state is unknown, the daemon does not run".to_string(),
        };
    };
    let sessions = control::number(status, "sessions");
    let secrets = control::number_any(status, &["pairing_active", "pairing_secrets_active"]);
    let connections = control::number(status, "plugin_connections");
    if sessions == 0 && connections == 0 {
        return CheckEntry {
            name: "pairing".to_string(),
            status: WARN.to_string(),
            detail: "no plugin session is paired; open the plugin in Figma and run figtether pair"
                .to_string(),
        };
    }
    CheckEntry {
        name: "pairing".to_string(),
        status: OK.to_string(),
        detail: format!(
            "{sessions} session(s), {connections} plugin connection(s), {secrets} pairing secret(s) waiting"
        ),
    }
}

/// The detected capabilities.
///
/// The daemon reports the capabilities of the sessions it knows. The check is
/// a real check: a daemon that reports the capabilities of at least one
/// session passes it, and the report names the capabilities that are not
/// usable.
fn capability_check(probe: &DaemonProbe) -> CheckEntry {
    let Some(status) = &probe.status else {
        return CheckEntry {
            name: "capabilities".to_string(),
            status: WARN.to_string(),
            detail: "no capability probe has run, the daemon does not run".to_string(),
        };
    };
    let Some(summary) = capability_summary(status) else {
        return CheckEntry {
            name: "capabilities".to_string(),
            status: WARN.to_string(),
            detail: "the daemon does not report the capabilities of its sessions".to_string(),
        };
    };
    if summary.sessions == 0 {
        return CheckEntry {
            name: "capabilities".to_string(),
            status: WARN.to_string(),
            detail: "no capability probe has run yet, the plugin is not connected".to_string(),
        };
    }
    let mut detail = format!(
        "{} session(s) reported {} capabilities",
        summary.sessions, summary.total
    );
    if !summary.unavailable.is_empty() {
        detail.push_str(&format!(
            ", {} not usable ({})",
            summary.unavailable.len(),
            summary.unavailable.join(", ")
        ));
    }
    CheckEntry {
        name: "capabilities".to_string(),
        status: OK.to_string(),
        detail,
    }
}

/// How many capabilities the daemon reported.
struct CapabilitySummary {
    /// Sessions that reported capabilities.
    sessions: usize,
    /// Reported capabilities in total.
    total: usize,
    /// The capabilities that are not usable, with their reason.
    unavailable: Vec<String>,
}

/// Read the capability report of the sanitized status.
///
/// The answer is `None` when the daemon does not carry the report at all,
/// which is the case for a daemon of an older build.
fn capability_summary(status: &Value) -> Option<CapabilitySummary> {
    let reports = status.get("capabilities").and_then(Value::as_array)?;
    let mut summary = CapabilitySummary {
        sessions: 0,
        total: 0,
        unavailable: Vec::new(),
    };
    for report in reports {
        let capabilities = report
            .get("capabilities")
            .and_then(Value::as_array)
            .map(|list| list.as_slice())
            .unwrap_or_default();
        if capabilities.is_empty() {
            continue;
        }
        summary.sessions += 1;
        for capability in capabilities {
            summary.total += 1;
            let state = control::text(capability, "state");
            if state == "available" {
                continue;
            }
            let name = control::text(capability, "capability");
            let reason = control::text(capability, "reason");
            if reason.is_empty() {
                summary
                    .unavailable
                    .push(output::redact(&format!("{name}: {state}")));
            } else {
                summary
                    .unavailable
                    .push(output::redact(&format!("{name}: {reason}")));
            }
        }
    }
    Some(summary)
}

/// The manifest path the doctor inspects.
///
/// The path that the user names wins. Otherwise the nearest manifest of the
/// current directory or of one of its parents is used, so the doctor works
/// from any directory inside a checkout.
fn manifest_path(args: &DoctorArgs) -> Option<PathBuf> {
    crate::commands::manifest::discover_manifest(args.manifest.as_deref())
}

/// The manifest check, with the search for the file included.
fn manifest_check_for(args: &DoctorArgs, port: u16) -> CheckEntry {
    match manifest_path(args) {
        Some(path) => manifest_check(&path, port),
        None => CheckEntry {
            name: "plugin manifest".to_string(),
            status: WARN.to_string(),
            detail: "no plugin manifest was found in this directory or in a parent directory; \
                     generate one with figtether manifest --plugin-id <plugin-id>, or name the \
                     file with --manifest <path>"
                .to_string(),
        },
    }
}

/// The result of the daemon probe.
struct DaemonProbe {
    status: Option<Value>,
    error: Option<String>,
}

/// Ask the daemon for its status.
async fn probe_daemon(port: u16, runtime_dir: Option<&Path>) -> DaemonProbe {
    let client = match ControlClient::connect(Some(port), runtime_dir).await {
        Ok(client) => client,
        Err(err) => {
            return DaemonProbe {
                status: None,
                error: Some(err.to_string()),
            }
        }
    };
    let request = ClientRequest::Status {
        request_id: control::new_request_id(),
    };
    match client.request(request).await {
        Ok(data) => DaemonProbe {
            status: Some(data),
            error: None,
        },
        Err(err) => DaemonProbe {
            status: None,
            error: Some(err.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_manifest(text: &str) -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join("manifest.json");
        std::fs::write(&path, text).expect("writes");
        (temp, path)
    }

    #[test]
    fn a_missing_manifest_only_warns() {
        let check = manifest_check(Path::new("no-such-manifest-12345.json"), 38947);
        assert_eq!(check.status, WARN);
    }

    #[test]
    fn a_manifest_with_a_byte_order_mark_fails() {
        let (_temp, path) = temp_manifest("\u{feff}{\"id\": \"figtether-dev\"}");
        let check = manifest_check(&path, 38947);
        assert_eq!(check.status, FAIL);
        assert!(
            check.detail.contains("byte order mark"),
            "the detail does not name the cause: {}",
            check.detail
        );
    }

    #[test]
    fn a_valid_manifest_passes() {
        let manifest = crate::commands::manifest::build_manifest(
            "123456789",
            38947,
            crate::cli::EditorType::Figma,
        );
        let (_temp, path) = temp_manifest(&manifest.to_string());
        let check = manifest_check(&path, 38947);
        assert_eq!(check.status, OK, "{}", check.detail);
    }

    #[test]
    fn a_manifest_for_another_port_fails() {
        let manifest = crate::commands::manifest::build_manifest(
            "123456789",
            38948,
            crate::cli::EditorType::Figma,
        );
        let (_temp, path) = temp_manifest(&manifest.to_string());
        let check = manifest_check(&path, 38947);
        assert_eq!(check.status, FAIL);
        assert!(check.detail.contains("38947"), "{}", check.detail);
    }

    #[test]
    fn a_manifest_with_a_remote_domain_fails() {
        let (_temp, path) = temp_manifest(
            r#"{"editorType":["figma"],"documentAccess":"dynamic-page","networkAccess":{"allowedDomains":["none"],"devAllowedDomains":["https://example.com"]}}"#,
        );
        let check = manifest_check(&path, 38947);
        assert_eq!(check.status, FAIL);
    }

    #[test]
    fn a_manifest_without_dynamic_page_fails() {
        let (_temp, path) = temp_manifest(
            r#"{"editorType":["figma"],"documentAccess":"full","networkAccess":{"allowedDomains":["none"],"devAllowedDomains":["http://127.0.0.1:38947"]}}"#,
        );
        let check = manifest_check(&path, 38947);
        assert_eq!(check.status, FAIL);
    }

    #[test]
    fn a_manifest_for_another_editor_fails() {
        let (_temp, path) = temp_manifest(
            r#"{"editorType":["figjam"],"documentAccess":"dynamic-page","networkAccess":{"allowedDomains":["none"],"devAllowedDomains":["http://127.0.0.1:38947"]}}"#,
        );
        let check = manifest_check(&path, 38947);
        assert_eq!(check.status, FAIL);
    }

    #[test]
    fn an_endpoint_is_split_into_host_and_port() {
        assert_eq!(
            parse_endpoint("http://127.0.0.1:38947"),
            Some(("127.0.0.1".to_string(), Some(38947)))
        );
        assert_eq!(
            parse_endpoint("ws://127.0.0.1:38947"),
            Some(("127.0.0.1".to_string(), Some(38947)))
        );
        assert_eq!(
            parse_endpoint("http://localhost"),
            Some(("localhost".to_string(), None))
        );
    }

    #[test]
    fn the_loopback_hosts_are_recognized() {
        assert!(is_loopback("127.0.0.1"));
        assert!(is_loopback("localhost"));
        assert!(!is_loopback("example.com"));
        assert!(!is_loopback("0.0.0.0"));
    }

    #[test]
    fn a_free_port_passes_the_port_check() {
        let check = port_check_from(38947, None, false, None);
        assert_eq!(check.entry.status, OK, "{}", check.entry.detail);
        assert!(
            check.entry.detail.contains("38947"),
            "{}",
            check.entry.detail
        );
        assert_eq!(check.suggestion, None);
    }

    #[test]
    fn a_reserved_port_names_the_range_and_the_port_to_use() {
        let problem = PortProblem::ReservedByOs {
            start: 38929,
            end: 39028,
        };
        let check = port_check_from(38947, Some(problem), false, Some(39147));
        assert_eq!(check.entry.status, FAIL, "{}", check.entry.detail);
        assert!(
            check.entry.detail.contains("38929-39028"),
            "{}",
            check.entry.detail
        );
        assert!(
            check.entry.detail.contains("figtether daemon --port 39147"),
            "{}",
            check.entry.detail
        );
        assert_eq!(check.suggestion, Some(39147));
    }

    #[test]
    fn a_port_in_use_names_the_port_to_use() {
        let check = port_check_from(38947, Some(PortProblem::InUse), false, Some(39147));
        assert_eq!(check.entry.status, FAIL, "{}", check.entry.detail);
        assert!(
            check.entry.detail.contains("figtether daemon --port 39147"),
            "{}",
            check.entry.detail
        );
        assert_eq!(check.suggestion, Some(39147));
    }

    #[test]
    fn a_port_that_the_daemon_serves_passes_the_port_check() {
        let check = port_check_from(38947, Some(PortProblem::InUse), true, None);
        assert_eq!(check.entry.status, OK, "{}", check.entry.detail);
        assert_eq!(check.suggestion, None);
    }

    #[test]
    fn the_report_carries_the_port_suggestion() {
        let args = DoctorArgs {
            json: false,
            port: None,
            runtime_dir: None,
            manifest: None,
        };
        let report = block_on(build_report(&args)).expect("builds");
        let value = serde_json::to_value(&report).expect("encodes");
        assert!(value.get("port_suggestion").is_some(), "{value}");
    }

    #[test]
    fn the_report_carries_no_secret() {
        let args = DoctorArgs {
            json: false,
            port: None,
            runtime_dir: None,
            manifest: None,
        };
        let report = block_on(build_report(&args)).expect("builds");
        let text = serde_json::to_string(&report).expect("encodes");
        for token in text.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-') {
            assert!(
                token.len() < 32,
                "the report carries a secret shaped token: {token}"
            );
        }
    }

    /// A probe that carries the sanitized status of a daemon.
    fn probe_with(status: Value) -> DaemonProbe {
        DaemonProbe {
            status: Some(status),
            error: None,
        }
    }

    #[test]
    fn the_capability_check_passes_when_the_daemon_reports_the_capabilities() {
        let status = serde_json::json!({
            "plugin_connections": 1,
            "capabilities": [
                {
                    "session_id": "session-1",
                    "capabilities": [
                        { "capability": "nodes", "state": "available" },
                        {
                            "capability": "export_svg",
                            "state": "unavailable",
                            "reason": "the seat does not expose SVG export"
                        }
                    ]
                }
            ]
        });
        let check = capability_check(&probe_with(status));
        assert_eq!(check.status, OK, "{}", check.detail);
        assert!(check.detail.contains("1 session(s)"), "{}", check.detail);
        assert!(check.detail.contains("2 capabilities"), "{}", check.detail);
        assert!(
            check.detail.contains("export_svg"),
            "the unavailable capability is named: {}",
            check.detail
        );
    }

    #[test]
    fn the_capability_check_warns_without_a_plugin() {
        let status = serde_json::json!({ "plugin_connections": 0, "capabilities": [] });
        let check = capability_check(&probe_with(status));
        assert_eq!(check.status, WARN, "{}", check.detail);
    }

    #[test]
    fn the_capability_check_warns_against_a_daemon_without_the_report() {
        let status = serde_json::json!({ "plugin_connections": 1 });
        let check = capability_check(&probe_with(status));
        assert_eq!(check.status, WARN, "{}", check.detail);
    }

    #[test]
    fn the_capability_check_warns_without_a_daemon() {
        let check = capability_check(&DaemonProbe {
            status: None,
            error: Some("the daemon does not run".to_string()),
        });
        assert_eq!(check.status, WARN, "{}", check.detail);
    }

    #[test]
    fn the_capability_check_masks_a_credential_shaped_reason() {
        let status = serde_json::json!({
            "capabilities": [
                {
                    "session_id": "session-1",
                    "capabilities": [
                        {
                            "capability": "export_svg",
                            "state": "unavailable",
                            "reason": "a".repeat(64)
                        }
                    ]
                }
            ]
        });
        let check = capability_check(&probe_with(status));
        assert_eq!(check.status, OK, "{}", check.detail);
        assert!(!check.detail.contains(&"a".repeat(64)), "{}", check.detail);
        assert!(check.detail.contains("redacted"), "{}", check.detail);
    }
}
