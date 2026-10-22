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
//! The `--port auto` selection asks the port helper for a free port, so a
//! port that the operating system reserved never reaches the manifest. The
//! command then prints the daemon command that uses the same port, because
//! the plugin and the daemon have to agree on it.

use anyhow::Context;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::cli::{EditorType, ManifestArgs, PortSelection, DEFAULT_PORT};
use crate::commands::CommandOutcome;
use crate::output;

/// Loopback address the plugin is allowed to reach.
pub const LOOPBACK_ADDRESS: &str = "127.0.0.1";

/// Name of the generated manifest when no output path is given.
pub const DEFAULT_FILE_NAME: &str = "manifest.json";

/// The plugin folder inside the repository.
pub const PLUGIN_FOLDER: &str = "plugins/figma";

/// Machine-local build input. Figma imports the generated dist/manifest.json.
pub const LOCAL_FILE_NAME: &str = "manifest.local.json";

/// The generated build artifact accepted by Figma's import dialog.
pub fn import_path(source: &Path) -> PathBuf {
    source
        .parent()
        .unwrap_or(Path::new("."))
        .join("dist")
        .join(DEFAULT_FILE_NAME)
}

/// The plugin id used when the user names none.
pub const DEFAULT_PLUGIN_ID: &str = "figtether-local";

/// The name Figma shows in the plugin list.
///
/// It matches the heading of the plugin panel, so the entry in Figma and the
/// panel that opens carry the same name.
pub const PLUGIN_NAME: &str = "FigTether Bridge";

/// Generate a development plugin manifest.
pub fn run(args: ManifestArgs) -> anyhow::Result<CommandOutcome> {
    let start = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let generated = generate(&args, &start)?;
    write_manifest(&generated.path, &generated.manifest)?;
    remember_manifest(&generated.path, args.runtime_dir.as_deref())?;
    print_summary(
        &generated.path,
        &generated.manifest,
        generated.port,
        args.editor_type,
        generated.automatic,
    );
    Ok(CommandOutcome::ok())
}

/// The result of one manifest generation.
struct Generated {
    /// Path the manifest is written to.
    path: PathBuf,
    /// The manifest document.
    manifest: Value,
    /// The port the manifest names.
    port: u16,
    /// Whether the port was picked automatically.
    automatic: bool,
}

/// Build the manifest and resolve the path and the port.
///
/// The function carries the validation of the action and of the plugin id,
/// so a refused request never reaches the file system.
fn generate(args: &ManifestArgs, start: &Path) -> anyhow::Result<Generated> {
    if args.action != "generate" {
        anyhow::bail!(
            "the only manifest action is generate, {} is unknown",
            args.action
        );
    }
    let path = args.out.clone().unwrap_or_else(|| {
        if plugin_folder(start).is_none()
            && !start.join(DEFAULT_FILE_NAME).is_file()
            && !start.join(LOCAL_FILE_NAME).is_file()
        {
            if let Some(saved) = remembered_manifest(args.runtime_dir.as_deref()) {
                return saved;
            }
        }
        default_output_path(start)
    });
    let path = if path.is_relative() {
        start.join(path)
    } else {
        path
    };
    // An explicit output is an existing contract too, even outside this checkout.
    let base = path.parent().unwrap_or(start);
    let existing: Option<Value> = if path.is_file() {
        let text = std::fs::read_to_string(&path)?;
        Some(serde_json::from_str(text.trim_start_matches('\u{feff}')).with_context(|| format!("invalid JSON in {}; the existing manifest was left unchanged. Restore valid JSON, then regenerate it with figtether manifest", path.display()))?)
    } else {
        None
    };
    let plugin_id = resolve_plugin_id(
        args.plugin_id
            .as_deref()
            .or_else(|| existing.as_ref()?.get("id")?.as_str()),
        base,
    )?;
    let (port, automatic) = match args.port {
        PortSelection::Auto => resolve_port_from(args.port, base, || {
            running_daemon_port(args.runtime_dir.as_deref()).or_else(|| {
                let port = manifest_port(existing.as_ref()?)?;
                figtether_daemon::port::diagnose(port)
                    .is_none()
                    .then_some(port)
            })
        }),
        _ => resolve_port(args.port, base, args.runtime_dir.as_deref()),
    };
    anyhow::ensure!(port != 0, "the bridge port must be between 1 and 65535");
    anyhow::ensure!(
        !automatic || figtether_daemon::port::diagnose(port).is_none(),
        "no usable port was found; choose an available port with --port <port>"
    );
    let manifest = build_manifest(&plugin_id, port, args.editor_type);
    Ok(Generated {
        path,
        manifest,
        port,
        automatic,
    })
}

/// The plugin id of the generated manifest.
///
/// An id that the user names is used as it is. Without one, the id of the
/// manifest that already exists is kept, so a regenerated manifest stays the
/// same plugin in Figma. A machine without any manifest gets the local
/// default.
fn resolve_plugin_id(requested: Option<&str>, start: &Path) -> anyhow::Result<String> {
    let candidate = match requested {
        Some(id) => {
            let id = id.trim().to_string();
            if id.is_empty() {
                anyhow::bail!("the plugin id must not be empty");
            }
            id
        }
        None => manifest_candidates(start)
            .into_iter()
            .find(|path| path.is_file())
            .and_then(|path| plugin_id_of(&path))
            .unwrap_or_else(|| DEFAULT_PLUGIN_ID.to_string()),
    };
    if !candidate
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.')
    {
        anyhow::bail!("the plugin id must contain only letters, digits, dash, underscore, and dot");
    }
    Ok(candidate)
}

/// The path the generated manifest is written to when the user names none.
///
/// Inside a checkout the machine local manifest is used, because that is the
/// file the user imports into Figma. Everywhere else the manifest is written
/// next to the current directory.
fn default_output_path(start: &Path) -> PathBuf {
    if start.join(LOCAL_FILE_NAME).is_file() {
        return start.join(LOCAL_FILE_NAME);
    }
    match plugin_folder(start) {
        Some(folder) => folder.join(LOCAL_FILE_NAME),
        None => PathBuf::from(DEFAULT_FILE_NAME),
    }
}

/// Every manifest path the commands look at, nearest first.
///
/// The search covers the current directory and every parent directory, because
/// the commands are often run from another directory than the checkout.
pub fn manifest_candidates(start: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut directory = Some(start);
    while let Some(dir) = directory {
        candidates.push(dir.join(PLUGIN_FOLDER).join(LOCAL_FILE_NAME));
        candidates.push(dir.join(PLUGIN_FOLDER).join(DEFAULT_FILE_NAME));
        candidates.push(dir.join(LOCAL_FILE_NAME));
        candidates.push(dir.join(DEFAULT_FILE_NAME));
        directory = dir.parent();
    }
    candidates
}

/// The manifest the commands inspect.
///
/// The path that the user names wins. Otherwise the nearest manifest of the
/// current directory or of one of its parents is used.
/// Search locally first, then use the path registered for this runtime.
pub fn discover_manifest_in(
    explicit: Option<&Path>,
    runtime_dir: Option<&Path>,
) -> Option<PathBuf> {
    match explicit {
        Some(path) => Some(path.to_path_buf()),
        None => {
            let start = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            manifest_candidates(&start)
                .into_iter()
                .find(|path| path.is_file())
                .or_else(|| remembered_manifest(runtime_dir))
        }
    }
}

/// The plugin folder of the checkout, if the commands run inside one.
pub fn plugin_folder(start: &Path) -> Option<PathBuf> {
    let mut directory = Some(start);
    while let Some(dir) = directory {
        let folder = dir.join(PLUGIN_FOLDER);
        if folder.join("build.mjs").is_file() {
            return Some(folder);
        }
        directory = dir.parent();
    }
    None
}

/// The plugin id that a manifest file carries.
pub fn plugin_id_of(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let manifest: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    manifest["id"]
        .as_str()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
}

/// Whether the bytes start with a UTF-8 byte order mark.
///
/// Figma parses a development manifest with a strict JSON reader. A byte
/// order mark makes that reader fail, and the plugin shows a manifest error
/// instead of a plugin. The check exists so the doctor names the real cause.
pub fn has_byte_order_mark(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xEF, 0xBB, 0xBF])
}

/// The port that a manifest grants to the plugin.
pub fn manifest_port(manifest: &Value) -> Option<u16> {
    let domains = manifest["networkAccess"]["devAllowedDomains"].as_array()?;
    let mut selected = None;
    for digits in domains
        .iter()
        .filter_map(|domain| domain.as_str())
        .filter_map(|text| text.strip_prefix("ws://localhost:"))
        .filter(|digits| !digits.is_empty() && digits.bytes().all(|ch| ch.is_ascii_digit()))
    {
        let port = digits.parse::<u16>().ok()?;
        if port == 0 || selected.is_some_and(|previous| previous != port) {
            return None;
        }
        selected = Some(port);
    }
    selected
}

/// Parse the actual contract strictly; invalid manifests are never a reason
/// to silently fall back to another port.
pub fn read_manifest(path: &Path) -> anyhow::Result<Value> {
    use anyhow::Context;
    let bytes = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    anyhow::ensure!(!has_byte_order_mark(&bytes), "the manifest {} starts with a byte order mark; Figma cannot parse it. Regenerate it with figtether manifest", path.display());
    serde_json::from_slice(&bytes).with_context(|| format!("invalid JSON in {}", path.display()))
}

/// Save the manifest location rather than another copy of its port. This
/// survives daemon shutdown and lets a command run from an unrelated folder.
pub fn remember_manifest(path: &Path, runtime_dir: Option<&Path>) -> anyhow::Result<()> {
    let dir = crate::runtime::resolve(runtime_dir)?;
    write_manifest(
        &dir.path().join("plugin.json"),
        &json!({"manifest": std::fs::canonicalize(path)?}),
    )
}

pub fn remembered_manifest(runtime_dir: Option<&Path>) -> Option<PathBuf> {
    let dir = crate::runtime::resolve(runtime_dir).ok()?;
    let settings: Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("plugin.json")).ok()?).ok()?;
    let path = PathBuf::from(settings["manifest"].as_str()?);
    path.is_file().then_some(path)
}

/// The port of the local plugin manifest.
///
/// The manifest is the memory of the port: the plugin reads its port from
/// that file, so the daemon has to use the same one. The runtime file only
/// exists while a daemon runs, and it is removed when the daemon stops.
/// The port of the local plugin manifest, relative to a directory.
pub fn local_manifest_port_from(start: &Path) -> Option<u16> {
    manifest_candidates(start)
        .into_iter()
        .find(|path| path.is_file())
        .and_then(|path| {
            let text = std::fs::read_to_string(&path).ok()?;
            let manifest: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
            manifest_port(&manifest)
        })
}

/// The port that the machine and the plugin already agree on.
///
/// The running daemon holds the port of the bridge, and the local manifest
/// names the port the plugin was built for. A port that the operating system
/// reserved is not an agreement, so it is not kept.
pub fn agreed_port_from(start: &Path, running: impl FnOnce() -> Option<u16>) -> Option<u16> {
    if let Some(port) = running() {
        return Some(port);
    }
    let port = local_manifest_port_from(start)?;
    match figtether_daemon::port::diagnose(port) {
        None => Some(port),
        Some(_) => None,
    }
}

/// The port of the running daemon.
///
/// The runtime file is written when a daemon starts and removed when it
/// stops. A daemon that was killed with a signal leaves the file behind, so
/// the port is only used when something actually answers on it.
fn running_daemon_port(runtime_dir: Option<&Path>) -> Option<u16> {
    crate::commands::block_on(async { Ok(crate::control::running_daemon_port(runtime_dir).await) })
        .ok()
        .flatten()
}

/// Resolve the port of the manifest.
///
/// A fixed port is used as it is, because the user asked for it. The automatic
/// selection keeps the port that the machine and the plugin already agree on,
/// and picks a free port only when there is no agreement. The port is never
/// moved on its own, because the plugin was built for the port of the
/// manifest.
fn resolve_port(selection: PortSelection, start: &Path, runtime_dir: Option<&Path>) -> (u16, bool) {
    resolve_port_from(selection, start, || running_daemon_port(runtime_dir))
}

/// Resolve the port, with the lookup of the running daemon kept separate.
fn resolve_port_from(
    selection: PortSelection,
    start: &Path,
    running: impl FnOnce() -> Option<u16>,
) -> (u16, bool) {
    match selection {
        PortSelection::Fixed(port) => (port, false),
        PortSelection::Auto => match agreed_port_from(start, running) {
            Some(port) => (port, false),
            None => (
                figtether_daemon::port::suggest_free_port(DEFAULT_PORT),
                true,
            ),
        },
    }
}

/// Build the manifest document.
pub fn build_manifest(plugin_id: &str, port: u16, editor_type: EditorType) -> Value {
    json!({
        "name": PLUGIN_NAME,
        "id": plugin_id,
        "api": "1.0.0",
        "editorType": [editor_type.as_str()],
        "documentAccess": "dynamic-page",
        "main": "dist/main.js",
        "ui": "dist/ui.html",
        "networkAccess": {
            "allowedDomains": ["none"],
            "reasoning": format!(
                "FigTether contacts no remote service. The plugin talks to the local daemon on {LOOPBACK_ADDRESS}:{port} only."
            ),
            "devAllowedDomains": [
                format!("http://localhost:{port}"),
                format!("ws://localhost:{port}")
            ]
        }
    })
}

/// Write the manifest to disk.
fn write_manifest(path: &Path, manifest: &Value) -> anyhow::Result<()> {
    use std::io::Write;
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
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut stage = tempfile::NamedTempFile::new_in(parent)?;
    stage.write_all(text.as_bytes())?;
    stage.persist(path).map_err(|err| {
        anyhow::anyhow!(
            "the manifest {} could not be written: {err}",
            path.display()
        )
    })?;
    Ok(())
}

/// The command that starts the daemon on the port of the manifest.
///
/// The plugin manifest and the daemon have to agree on the port, so the
/// command is printed with the port that the manifest names.
fn daemon_command(port: u16) -> String {
    format!("figtether daemon --port {port}")
}

/// Print the result and the regeneration rule.
fn print_summary(
    path: &Path,
    manifest: &Value,
    port: u16,
    editor_type: EditorType,
    automatic: bool,
) {
    output::blank();
    output::line(&format!("Manifest written to {}", absolute(path).display()));
    if path
        .file_name()
        .is_some_and(|name| name != DEFAULT_FILE_NAME)
    {
        output::line("This file is build input. Figma only imports a file named manifest.json.");
        if path
            .parent()
            .is_some_and(|parent| parent.join("build.mjs").is_file())
        {
            output::field(
                "Figma import after build",
                &import_path(&absolute(path)).display().to_string(),
            );
        } else {
            output::line("Import the build's dist/manifest.json after building from this input.");
        }
    }
    output::blank();
    output::field("plugin id", manifest["id"].as_str().unwrap_or_default());
    output::field("editor type", editor_type.as_str());
    output::field("document access", "dynamic-page");
    output::field("bridge address", &format!("{LOOPBACK_ADDRESS}:{port}"));
    output::blank();
    output::line("The manifest grants the plugin access to the loopback daemon only.");
    output::line("No wildcard domain and no remote service is referenced.");
    if automatic {
        output::blank();
        output::line("The selected port was available when checked; startup will bind it again.");
        output::line("The plugin manifest and the daemon have to agree on the port.");
        output::line("Start the daemon with:");
        output::blank();
        output::line(&format!("  {}", daemon_command(port)));
        output::blank();
        output::line("Rebuild the plugin for that port before you reopen it in Figma.");
    }
    output::blank();
    output::line(
        "Build the plugin after generating a manifest: npm --prefix plugins/figma run build.",
    );
    output::line("Or use figtether start to prepare the source plugin and pair in one terminal.");
    output::line("If the daemon port changes, regenerate this manifest with the same port");
    output::line("then rebuild and reimport dist/manifest.json in Figma. The port is never changed silently,");
    output::line("because the plugin reads it from the manifest:");
    output::blank();
    output::line(&format!(
        "  figtether manifest --plugin-id \"<plugin-id>\" --port <new-port> --out {}",
        path.display()
    ));
    output::blank();
    output::line("The manifest is written without a byte order mark, because Figma refuses a");
    output::line("manifest that starts with one.");
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
    fn the_local_manifest_wins_from_the_plugin_folder_itself() {
        let (_temp, folder) = checkout();
        write_manifest(
            &folder.join(LOCAL_FILE_NAME),
            &build_manifest("local", 39029, EditorType::Figma),
        )
        .expect("local manifest");
        assert_eq!(local_manifest_port_from(&folder), Some(39029));
    }

    #[test]
    fn only_an_unambiguous_nonzero_local_websocket_selects_the_port() {
        for endpoints in [
            json!(["https://remote.example:1234"]),
            json!(["ws://localhost:0"]),
            json!(["ws://localhost:39029", "ws://localhost:0"]),
            json!(["ws://localhost:39029", "ws://localhost:65536"]),
            json!(["ws://localhost:1234", "ws://localhost:1235"]),
        ] {
            assert_eq!(
                manifest_port(&json!({"networkAccess":{"devAllowedDomains": endpoints}})),
                None
            );
        }
        assert_eq!(
            manifest_port(
                &json!({"networkAccess":{"devAllowedDomains":["http://localhost:1234", "ws://localhost:39029"]}})
            ),
            Some(39029)
        );
    }

    #[test]
    fn regeneration_keeps_the_identity_and_port_of_a_custom_output() {
        let temp = tempfile::tempdir().expect("temp");
        let runtime = tempfile::tempdir().expect("runtime");
        let socket = std::net::TcpListener::bind("127.0.0.1:0").expect("free port");
        let port = socket.local_addr().expect("address").port();
        drop(socket);
        let path = temp.path().join("custom.json");
        write_manifest(&path, &build_manifest("custom-id", port, EditorType::Figma))
            .expect("manifest");
        let args = ManifestArgs {
            action: "generate".into(),
            out: Some(path),
            plugin_id: None,
            port: PortSelection::Auto,
            editor_type: EditorType::Figma,
            runtime_dir: Some(runtime.path().to_owned()),
        };
        let generated = generate(&args, temp.path()).expect("generate");
        assert_eq!(generated.port, port);
        assert_eq!(generated.manifest["id"], "custom-id");
    }

    #[test]
    fn a_generated_manifest_is_bom_free_and_its_location_survives_daemon_cleanup() {
        let temp = tempfile::tempdir().expect("temp");
        let runtime = tempfile::tempdir().expect("runtime");
        let path = temp.path().join(DEFAULT_FILE_NAME);
        std::fs::write(&path, [0xef, 0xbb, 0xbf, b'{', b'}']).expect("old BOM");
        write_manifest(&path, &build_manifest("id", 39029, EditorType::Figma)).expect("replace");
        assert!(!has_byte_order_mark(&std::fs::read(&path).expect("bytes")));
        remember_manifest(&path, Some(runtime.path())).expect("remember");
        crate::runtime::resolve(Some(runtime.path()))
            .expect("runtime")
            .remove_runtime_files()
            .expect("cleanup");
        assert_eq!(
            remembered_manifest(Some(runtime.path())),
            Some(path.canonicalize().expect("path"))
        );
    }

    /// A checkout with a plugin folder and an example manifest.
    fn checkout() -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::tempdir().expect("temp dir");
        let folder = temp.path().join(PLUGIN_FOLDER);
        std::fs::create_dir_all(&folder).expect("the plugin folder");
        std::fs::write(folder.join("build.mjs"), "// source fixture").expect("source marker");
        let manifest = build_manifest("figtether-dev", 38947, EditorType::Figma);
        std::fs::write(
            folder.join(DEFAULT_FILE_NAME),
            serde_json::to_string_pretty(&manifest).expect("serializes"),
        )
        .expect("writes");
        (temp, folder)
    }

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
    fn a_named_plugin_id_is_used_as_it_is() {
        let id = resolve_plugin_id(Some("  figtether-dev  "), Path::new(".")).expect("an id");
        assert_eq!(id, "figtether-dev");
    }

    #[test]
    fn an_unnamed_plugin_id_keeps_the_id_of_the_existing_manifest() {
        let (temp, folder) = checkout();
        let id = resolve_plugin_id(None, temp.path()).expect("an id");
        assert_eq!(id, "figtether-dev");
        let _ = folder;
    }

    #[test]
    fn an_unnamed_plugin_id_without_a_manifest_uses_the_local_default() {
        let temp = tempfile::tempdir().expect("temp dir");
        let id = resolve_plugin_id(None, temp.path()).expect("an id");
        assert_eq!(id, DEFAULT_PLUGIN_ID);
    }

    #[test]
    fn a_manifest_in_a_parent_directory_is_found() {
        let (temp, folder) = checkout();
        let nested = temp.path().join("docs").join("deep");
        std::fs::create_dir_all(&nested).expect("the nested folder");
        let found = manifest_candidates(&nested)
            .into_iter()
            .find(|path| path.is_file())
            .expect("a manifest");
        assert_eq!(found, folder.join(DEFAULT_FILE_NAME));
    }

    #[test]
    fn the_local_manifest_is_the_default_output_inside_a_checkout() {
        let (temp, folder) = checkout();
        let path = default_output_path(temp.path());
        assert_eq!(path, folder.join(LOCAL_FILE_NAME));
    }

    #[test]
    fn the_default_output_outside_a_checkout_is_the_working_directory() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = default_output_path(temp.path());
        assert_eq!(path, PathBuf::from(DEFAULT_FILE_NAME));
    }

    #[test]
    fn an_unrelated_plugin_folder_without_source_is_not_a_checkout() {
        let temp = tempfile::tempdir().expect("temp");
        std::fs::create_dir_all(temp.path().join(PLUGIN_FOLDER)).expect("empty folder");
        assert_eq!(plugin_folder(temp.path()), None);
    }

    #[test]
    fn a_byte_order_mark_is_recognised() {
        assert!(has_byte_order_mark(&[0xEF, 0xBB, 0xBF, b'{']));
        assert!(!has_byte_order_mark(b"{\"a\":1}"));
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
                text.contains("localhost:38948"),
                "the domain {text} does not carry the loopback address and the port"
            );
        }
    }

    #[test]
    fn the_network_access_never_names_a_wildcard() {
        let manifest = build_manifest("123456789", 38947, EditorType::Figma);
        let text = manifest.to_string();
        assert!(!text.contains('*'), "{text}");
        assert!(manifest["networkAccess"]["reasoning"].is_string());
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
            plugin_id: Some("   ".to_string()),
            port: PortSelection::Fixed(38947),
            editor_type: EditorType::Figma,
            runtime_dir: None,
        };
        let result = run(args);
        assert!(result.is_err());
    }

    #[test]
    fn a_plugin_id_with_a_path_separator_is_refused() {
        let args = ManifestArgs {
            action: "generate".to_string(),
            out: None,
            plugin_id: Some("../etc/passwd".to_string()),
            port: PortSelection::Fixed(38947),
            editor_type: EditorType::Figma,
            runtime_dir: None,
        };
        let result = run(args);
        assert!(result.is_err());
    }

    #[test]
    fn an_unknown_action_is_refused() {
        let args = ManifestArgs {
            action: "destroy".to_string(),
            out: None,
            plugin_id: Some("123456789".to_string()),
            port: PortSelection::Fixed(38947),
            editor_type: EditorType::Figma,
            runtime_dir: None,
        };
        let result = run(args);
        assert!(result.is_err());
    }

    #[test]
    fn a_fixed_port_is_never_moved() {
        let (port, automatic) = resolve_port(PortSelection::Fixed(38948), Path::new("."), None);
        assert_eq!(port, 38948);
        assert!(!automatic, "a fixed port is not picked automatically");
    }

    #[test]
    fn the_auto_selection_picks_a_free_port() {
        let temp = tempfile::tempdir().expect("temp dir");
        let (port, automatic) = resolve_port_from(PortSelection::Auto, temp.path(), || None);
        assert!(automatic);
        assert!(
            port >= DEFAULT_PORT,
            "the picked port {port} is below the bridge port"
        );
    }

    #[test]
    fn the_auto_selection_keeps_the_port_that_is_already_agreed() {
        let (temp, folder) = checkout();
        let manifest = build_manifest("figtether-dev", 39125, EditorType::Figma);
        std::fs::write(
            folder.join(LOCAL_FILE_NAME),
            serde_json::to_string_pretty(&manifest).expect("serializes"),
        )
        .expect("writes");
        let (port, automatic) = resolve_port_from(PortSelection::Auto, temp.path(), || None);
        assert_eq!(port, 39125, "the agreed port is kept");
        assert!(!automatic, "an agreed port is not picked");
    }

    #[test]
    fn the_auto_port_is_written_into_the_manifest() {
        let temp = tempfile::tempdir().expect("temp dir");
        let runtime = tempfile::tempdir().expect("a runtime dir");
        let path = temp.path().join("manifest.json");
        let args = ManifestArgs {
            action: "generate".to_string(),
            out: Some(path.clone()),
            plugin_id: Some("123456789".to_string()),
            port: PortSelection::Auto,
            editor_type: EditorType::Figma,
            runtime_dir: Some(runtime.path().to_path_buf()),
        };
        let generated = generate(&args, temp.path()).expect("generates");
        assert!(generated.automatic, "no agreement, so the port is picked");
        assert!(generated.port >= DEFAULT_PORT, "the picked port is used");
        write_manifest(&path, &generated.manifest).expect("writes");
        let text = std::fs::read_to_string(&path).expect("reads");
        let manifest: Value = serde_json::from_str(&text).expect("parses");
        let domains = manifest["networkAccess"]["devAllowedDomains"]
            .as_array()
            .expect("an array of domains");
        for domain in domains {
            let text = domain.as_str().expect("a string");
            assert!(
                text.contains(&format!("localhost:{}", generated.port)),
                "the domain {text} does not carry the picked port {}",
                generated.port
            );
        }
    }

    #[test]
    fn a_fixed_port_reaches_the_manifest_unchanged() {
        let temp = tempfile::tempdir().expect("temp dir");
        let args = ManifestArgs {
            action: "generate".to_string(),
            out: None,
            plugin_id: Some("123456789".to_string()),
            port: PortSelection::Fixed(38948),
            editor_type: EditorType::Figma,
            runtime_dir: None,
        };
        let generated = generate(&args, temp.path()).expect("generates");
        assert_eq!(generated.port, 38948);
        assert!(!generated.automatic);
        let domains = generated.manifest["networkAccess"]["devAllowedDomains"]
            .as_array()
            .expect("an array of domains");
        for domain in domains {
            let text = domain.as_str().expect("a string");
            assert!(text.contains("localhost:38948"), "the domain is {text}");
        }
    }

    #[test]
    fn the_daemon_command_carries_the_port_of_the_manifest() {
        assert_eq!(daemon_command(39147), "figtether daemon --port 39147");
    }
}
