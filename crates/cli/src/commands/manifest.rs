//! Development plugin manifest generator.
//!
//! The generated manifest is a development manifest for a plugin bundle that
//! belongs to the user who runs the command. It follows the plugin manifest
//! rules of the project:
//!
//! - `documentAccess` is `dynamic-page`, never `full`;
//! - `editorType` is `figma`;
//! - `networkAccess` grants the loopback address and the daemon port only, and
//!   never a wildcard domain.
//!
//! The manifest is regenerated whenever the daemon port changes. The port is
//! never moved silently, because the plugin reads the port from the manifest.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::cli::{EditorType, ManifestArgs};
use crate::commands::CommandOutcome;
use crate::output;

/// Loopback address the plugin is allowed to reach.
pub const LOOPBACK_ADDRESS: &str = "127.0.0.1";

/// Name of the generated manifest when no output path is given.
pub const DEFAULT_FILE_NAME: &str = "manifest.json";

/// Generate a development plugin manifest.
pub fn run(args: ManifestArgs) -> anyhow::Result<CommandOutcome> {
    if args.action != "generate" {
        anyhow::bail!(
            "the only manifest action is generate, {} is unknown",
            args.action
        );
    }
    let plugin_id = args.plugin_id.trim();
    if plugin_id.is_empty() {
        anyhow::bail!("the plugin id must not be empty");
    }
    if !plugin_id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.')
    {
        anyhow::bail!("the plugin id must contain only letters, digits, dash, underscore, and dot");
    }
    let path = args
        .out
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_FILE_NAME));
    let manifest = build_manifest(plugin_id, args.port, args.editor_type);
    write_manifest(&path, &manifest)?;
    print_summary(&path, &manifest, args.port, args.editor_type);
    Ok(CommandOutcome::ok())
}

/// Build the manifest document.
pub fn build_manifest(plugin_id: &str, port: u16, editor_type: EditorType) -> Value {
    json!({
        "name": "FigTether",
        "id": plugin_id,
        "api": "1.0.0",
        "editorType": [editor_type.as_str()],
        "documentAccess": "dynamic-page",
        "main": "dist/main.js",
        "ui": "dist/ui.html",
        "networkAccess": {
            "allowedDomains": ["none"],
            "allowedDomainsReasoning": format!(
                "FigTether contacts no remote service. The plugin talks to the local daemon on {LOOPBACK_ADDRESS}:{port} only."
            ),
            "devAllowedDomains": [
                format!("http://{LOOPBACK_ADDRESS}:{port}"),
                format!("ws://{LOOPBACK_ADDRESS}:{port}")
            ]
        }
    })
}

/// Write the manifest to disk.
fn write_manifest(path: &Path, manifest: &Value) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|err| {
                anyhow::anyhow!(
                    "the directory {} could not be created: {err}",
                    parent.display()
                )
            })?;
        }
    }
    let mut text = serde_json::to_string_pretty(manifest)?;
    text.push('\n');
    std::fs::write(path, text).map_err(|err| {
        anyhow::anyhow!(
            "the manifest {} could not be written: {err}",
            path.display()
        )
    })?;
    Ok(())
}

/// Print the result and the regeneration rule.
fn print_summary(path: &Path, manifest: &Value, port: u16, editor_type: EditorType) {
    output::blank();
    output::line(&format!("Manifest written to {}", absolute(path).display()));
    output::blank();
    output::field("plugin id", manifest["id"].as_str().unwrap_or_default());
    output::field("editor type", editor_type.as_str());
    output::field("document access", "dynamic-page");
    output::field("bridge address", &format!("{LOOPBACK_ADDRESS}:{port}"));
    output::blank();
    output::line("The manifest grants the plugin access to the loopback daemon only.");
    output::line("No wildcard domain and no remote service is referenced.");
    output::blank();
    output::line("If the daemon port changes, regenerate this manifest with the same port");
    output::line("and reload the plugin in Figma. The port is never changed silently,");
    output::line("because the plugin reads it from the manifest:");
    output::blank();
    output::line(&format!(
        "  figtether manifest --plugin-id \"<plugin-id>\" --port <new-port> --out {}",
        path.display()
    ));
}

/// Resolve a path for display.
fn absolute(path: &Path) -> PathBuf {
    match std::env::current_dir() {
        Ok(current) if path.is_relative() => current.join(path),
        _ => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_manifest_uses_dynamic_page_access() {
        let manifest = build_manifest("123456789", 38947, EditorType::Figma);
        assert_eq!(manifest["documentAccess"], "dynamic-page");
    }

    #[test]
    fn the_manifest_targets_figma_only() {
        let manifest = build_manifest("123456789", 38947, EditorType::Figma);
        assert_eq!(manifest["editorType"], json!(["figma"]));
    }

    #[test]
    fn the_manifest_carries_the_plugin_id() {
        let manifest = build_manifest("123456789", 38947, EditorType::Figma);
        assert_eq!(manifest["id"], "123456789");
    }

    #[test]
    fn the_network_access_allows_the_loopback_port_only() {
        let manifest = build_manifest("123456789", 38948, EditorType::Figma);
        let network = &manifest["networkAccess"];
        let domains = network["devAllowedDomains"]
            .as_array()
            .expect("an array of domains");
        assert_eq!(domains.len(), 2);
        for domain in domains {
            let text = domain.as_str().expect("a string");
            assert!(
                text.contains("127.0.0.1:38948"),
                "the domain {text} does not carry the loopback address and the port"
            );
        }
    }

    #[test]
    fn the_network_access_never_names_a_wildcard() {
        let manifest = build_manifest("123456789", 38947, EditorType::Figma);
        let text = manifest.to_string();
        assert!(!text.contains('*'), "{text}");
        assert!(!text.contains("localhost"), "{text}");
    }

    #[test]
    fn the_remote_access_is_switched_off() {
        let manifest = build_manifest("123456789", 38947, EditorType::Figma);
        assert_eq!(manifest["networkAccess"]["allowedDomains"], json!(["none"]));
    }

    #[test]
    fn the_manifest_is_written_to_disk() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join("manifest.json");
        let manifest = build_manifest("123456789", 38947, EditorType::Figma);
        write_manifest(&path, &manifest).expect("writes");
        let text = std::fs::read_to_string(&path).expect("reads");
        let parsed: Value = serde_json::from_str(&text).expect("parses");
        assert_eq!(parsed, manifest);
    }

    #[test]
    fn an_empty_plugin_id_is_refused() {
        let args = ManifestArgs {
            action: "generate".to_string(),
            out: None,
            plugin_id: "   ".to_string(),
            port: 38947,
            editor_type: EditorType::Figma,
        };
        let result = run(args);
        assert!(result.is_err());
    }

    #[test]
    fn a_plugin_id_with_a_path_separator_is_refused() {
        let args = ManifestArgs {
            action: "generate".to_string(),
            out: None,
            plugin_id: "../etc/passwd".to_string(),
            port: 38947,
            editor_type: EditorType::Figma,
        };
        let result = run(args);
        assert!(result.is_err());
    }

    #[test]
    fn an_unknown_action_is_refused() {
        let args = ManifestArgs {
            action: "destroy".to_string(),
            out: None,
            plugin_id: "123456789".to_string(),
            port: 38947,
            editor_type: EditorType::Figma,
        };
        let result = run(args);
        assert!(result.is_err());
    }
}
