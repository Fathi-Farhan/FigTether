//! Prepare a source installation and pair in a single foreground terminal.
//! This does not import a plugin or change agent-host settings on the user's behalf.
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{ensure, Context};

use crate::cli::{
    DaemonArgs, EditorType, ManifestArgs, PairArgs, PortSelection, StartArgs, DEFAULT_PORT,
};
use crate::commands::{block_on, daemon, manifest, pair, CommandOutcome};
use crate::{control, output};

pub fn run(args: StartArgs) -> anyhow::Result<CommandOutcome> {
    let folder = source_folder(args.plugin_dir.as_deref(), args.runtime_dir.as_deref())?;
    let path = if folder.join(manifest::LOCAL_FILE_NAME).is_file() {
        folder.join(manifest::LOCAL_FILE_NAME)
    } else {
        folder.join(manifest::DEFAULT_FILE_NAME)
    };
    let existing = path
        .is_file()
        .then(|| manifest::read_manifest(&path))
        .transpose()?;
    let existing_port = existing.as_ref().and_then(manifest::manifest_port);
    let running =
        block_on(async { Ok(control::running_daemon_port(args.runtime_dir.as_deref()).await) })?;
    let port = args
        .port
        .or(existing_port)
        .or(running)
        .unwrap_or_else(|| figtether_daemon::port::suggest_free_port(DEFAULT_PORT));

    if let Some(active) = running {
        ensure!(
            active == port,
            "a daemon already runs on port {active}; run figtether stop before changing to {port}"
        );
        ensure!(existing_port == Some(port), "the source manifest does not yet agree with port {port}; run figtether stop, then figtether start --port {port} to prepare both sides before pairing");
        ensure!(
            args.plugin_id
                .as_deref()
                .is_none_or(|id| existing.as_ref().and_then(|m| m["id"].as_str()) == Some(id)),
            "stop the running daemon before changing the plugin id"
        );
        let bundle = super::doctor::bundle_check(&path, port);
        ensure!(
            bundle.status != "fail",
            "{}; run figtether stop, then figtether start to rebuild before pairing",
            bundle.detail
        );
        if path.is_file() {
            manifest::remember_manifest(&path, args.runtime_dir.as_deref())?;
        }
        output::line(&format!("FigTether is already running on port {port}."));
        output::line(&format!(
            "Figma import: {} (the file must be named manifest.json).",
            manifest::import_path(&path).display()
        ));
        output::line("Open the imported FigTether Bridge plugin in Figma.");
        return pair::run(PairArgs {
            port: Some(port),
            runtime_dir: args.runtime_dir,
            json: false,
        });
    }
    ensure!(figtether_daemon::port::diagnose(port).is_none(),
        "port {port} from the plugin contract cannot be bound. Run figtether doctor, or explicitly move both sides with figtether start --port <free-port>");
    // Check dependencies before writing a new contract. No install or network
    // operation is hidden in startup; npm ci is the source-install prerequisite.
    ensure!(folder.join("node_modules/esbuild").is_dir(),
        "plugin build dependencies are missing. Run npm --prefix \"{}\" ci, then repeat figtether start", folder.display());
    let target = folder.join(manifest::LOCAL_FILE_NAME);
    let import = manifest::import_path(&target);
    manifest::run(ManifestArgs {
        action: "generate".into(),
        out: Some(target.clone()),
        plugin_id: args.plugin_id.or_else(|| {
            existing
                .as_ref()
                .and_then(|m| m["id"].as_str())
                .map(str::to_owned)
        }),
        port: PortSelection::Fixed(port),
        editor_type: EditorType::Figma,
        runtime_dir: args.runtime_dir.clone(),
    })?;
    output::line("Building the Figma plugin for this manifest...");
    let status = Command::new("node")
        .arg(folder.join("build.mjs"))
        .arg("--manifest")
        .arg(&target)
        .env_remove("FIGTETHER_PORT")
        .current_dir(&folder)
        .status()
        .context("Node.js 22.14+ is required to build the source plugin")?;
    ensure!(status.success(), "plugin build failed; the daemon was not started. Fix the build error and repeat figtether start");
    output::blank();
    output::line("In Figma Desktop: Plugins > Development > Import plugin from manifest...");
    output::line(&format!(
        "Select {} and open FigTether Bridge.",
        import.display()
    ));
    output::line("Import once; reimport after changing the port. Keep this terminal open.");
    daemon::run_with_pairing(
        DaemonArgs {
            port: Some(port),
            runtime_dir: args.runtime_dir,
            ..Default::default()
        },
        true,
    )
}

fn source_folder(explicit: Option<&Path>, runtime_dir: Option<&Path>) -> anyhow::Result<PathBuf> {
    let folder = if let Some(path) = explicit {
        path.to_path_buf()
    } else {
        manifest::discover_manifest_in(None, runtime_dir)
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .filter(|p| p.join("build.mjs").is_file())
            .or_else(|| manifest::plugin_folder(&std::env::current_dir().ok()?))
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugins/figma"))
    };
    let folder = folder.canonicalize().context("plugin source folder not found; use figtether start --plugin-dir <source/plugins/figma>. The executable alone does not contain the plugin")?;
    let package: serde_json::Value =
        serde_json::from_slice(&std::fs::read(folder.join("package.json")).context("plugin package.json is missing or unreadable; choose the plugins/figma source folder containing build.mjs")?)
            .context("plugin package.json is not valid JSON; restore the source package before running figtether start")?;
    ensure!(
        package["name"] == "@figtether/figma-plugin" && folder.join("build.mjs").is_file(),
        "choose the FigTether plugin source folder containing build.mjs"
    );
    Ok(folder)
}
