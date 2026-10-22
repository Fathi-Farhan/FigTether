//! Best-effort, read-only inspection of Figma Desktop's development imports.
//! The settings format is private: missing/unknown data is a warning, never
//! proof that the running editor has loaded a particular CSP. Only relevant
//! manifest paths and ports reach the sanitized doctor output.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{doctor::CheckEntry, manifest};

pub fn check(source: &Path, port: u16) -> CheckEntry {
    let Some(settings) = settings_path() else {
        return entry("warn", "import could not be verified on this platform; import the built dist/manifest.json in Figma Desktop".into());
    };
    let Ok(bytes) = std::fs::read(settings) else {
        return entry("warn", "Figma Desktop import could not be verified; import the built dist/manifest.json, then reopen the plugin".into());
    };
    let Ok(settings) = serde_json::from_slice(&bytes) else {
        return entry(
            "warn",
            "Figma Desktop settings could not be read; verify the manifest import in Figma".into(),
        );
    };
    evaluate(source, port, &settings)
}

fn settings_path() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("Figma/settings.json"))
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME")
            .map(|p| PathBuf::from(p).join("Library/Application Support/Figma/settings.json"))
    } else {
        None
    }
}

fn entry(status: &str, detail: String) -> CheckEntry {
    CheckEntry {
        name: "Figma development import".into(),
        status: status.into(),
        detail,
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    let a = a.canonicalize().unwrap_or_else(|_| a.to_path_buf());
    let b = b.canonicalize().unwrap_or_else(|_| b.to_path_buf());
    if cfg!(target_os = "windows") {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    } else {
        a == b
    }
}

fn evaluate(source: &Path, port: u16, settings: &Value) -> CheckEntry {
    let Ok(contract) = manifest::read_manifest(source) else {
        return entry(
            "warn",
            "fix the plugin manifest before checking the Figma import".into(),
        );
    };
    let base = source.parent().unwrap_or(Path::new("."));
    let main = base.join(contract["main"].as_str().unwrap_or("dist/main.js"));
    let expected = main.parent().unwrap_or(base).join("manifest.json");
    let recovery = format!("import {} through Plugins > Development > Import plugin from manifest, then reopen FigTether Bridge", expected.display());
    let records: Vec<&Value> = match &settings["localFileExtensions"] {
        Value::Array(values) => values.iter().collect(),
        Value::Object(values) => values.values().collect(),
        _ => {
            return entry(
                "warn",
                format!("Figma's import records are unavailable; {recovery}"),
            )
        }
    };
    let plugin_id = contract["id"].as_str();
    let relevant: Vec<&Value> = records
        .into_iter()
        .filter(|record| {
            // Figma stores code/UI file watchers in the same collection. They
            // are dependencies of an import, not invalid plugin imports.
            if record["fileMetadata"]["type"]
                .as_str()
                .is_some_and(|kind| kind != "manifest")
            {
                return false;
            }
            let Some(path) = record["manifestPath"].as_str().map(Path::new) else {
                return false;
            };
            same_path(path, source)
                || same_path(path, &expected)
                || path
                    .parent()
                    .is_some_and(|parent| same_path(parent, main.parent().unwrap_or(base)))
                || plugin_id.is_some_and(|id| record["lastKnownPluginId"].as_str() == Some(id))
        })
        .collect();
    let correct = relevant.iter().any(|record| {
        record["manifestPath"]
            .as_str()
            .is_some_and(|path| same_path(Path::new(path), &expected))
    });
    if correct {
        let duplicates = relevant
            .iter()
            .filter(|record| {
                record["manifestPath"]
                    .as_str()
                    .is_some_and(|path| !same_path(Path::new(path), &expected))
            })
            .count();
        return if duplicates == 0 {
            entry("ok", format!("{} is registered; reopen the plugin after a CSP change (the active editor policy is checked inside the plugin)", expected.display()))
        } else {
            entry("warn", format!("{} is registered, with {duplicates} older/invalid import(s); remove those entries through Manage plugins in development, keeping the built manifest entry and source files", expected.display()))
        };
    }
    for record in &relevant {
        let Some(path) = record["manifestPath"].as_str().map(Path::new) else {
            continue;
        };
        if path.file_name().is_none_or(|name| name != "manifest.json") {
            return entry("fail", format!("Figma requires the filename manifest.json; a bundle or differently named file was selected; {recovery}"));
        }
        if let Ok(imported) = manifest::read_manifest(path) {
            if let Some(old_port) = manifest::manifest_port(&imported).filter(|p| *p != port) {
                return entry("fail", format!("the registered manifest allows port {old_port}, but this build uses {port}; {recovery}"));
            }
        }
    }
    entry(
        "warn",
        format!("the built manifest is not registered in these Figma settings; {recovery}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn installation() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("manifest.local.json");
        let built = temp.path().join("dist/manifest.json");
        std::fs::create_dir_all(built.parent().unwrap()).unwrap();
        std::fs::write(&source, serde_json::to_vec(&json!({"id":"test-id", "main":"dist/main.js", "networkAccess":{"devAllowedDomains":["ws://localhost:39029"]}})).unwrap()).unwrap();
        std::fs::write(&built, "{}").unwrap();
        (temp, source, built)
    }

    #[test]
    fn detects_an_example_import_still_on_the_old_port() {
        let (temp, source, _) = installation();
        let old = temp.path().join("manifest.json");
        std::fs::write(
            &old,
            r#"{"networkAccess":{"devAllowedDomains":["ws://localhost:38947"]}}"#,
        )
        .unwrap();
        let check = evaluate(
            &source,
            39029,
            &json!({"localFileExtensions":[{"manifestPath":old,"lastKnownPluginId":"test-id"}]}),
        );
        assert_eq!(check.status, "fail");
        assert!(check.detail.contains("38947"));
        assert!(check.detail.contains("39029"));
        assert!(check.detail.contains("dist"));
    }

    #[test]
    fn canonical_import_wins_over_duplicate_and_invalid_entries() {
        let (_temp, source, built) = installation();
        let check = evaluate(
            &source,
            39029,
            &json!({"localFileExtensions":[{"manifestPath":built},{"manifestPath":source,"lastKnownPluginId":"test-id"}]}),
        );
        assert_eq!(check.status, "warn");
        assert!(check.detail.contains("1 older/invalid"));
    }

    #[test]
    fn a_bundle_is_not_an_importable_manifest() {
        let (_temp, source, built) = installation();
        let check = evaluate(
            &source,
            39029,
            &json!({"localFileExtensions":[{"manifestPath":built.with_file_name("main.js")}]}),
        );
        assert_eq!(check.status, "fail");
        assert!(check.detail.contains("requires the filename manifest.json"));
    }

    #[test]
    fn code_and_ui_watchers_are_not_duplicate_imports() {
        let (_temp, source, built) = installation();
        let settings = json!({"localFileExtensions":[
            {"manifestPath":built,"fileMetadata":{"type":"manifest"}},
            {"manifestPath":built.with_file_name("main.js"),"fileMetadata":{"type":"code"}},
            {"manifestPath":built.with_file_name("ui.html"),"fileMetadata":{"type":"ui"}}
        ]});
        assert_eq!(evaluate(&source, 39029, &settings).status, "ok");
    }

    #[test]
    fn unrelated_plugins_and_private_settings_do_not_reach_output() {
        let (_temp, source, built) = installation();
        let settings = json!({"private":"DO_NOT_PRINT", "localFileExtensions":{"0":{"manifestPath":built},"1":{"manifestPath":"private-account-path","lastKnownPluginId":"other-id"}}});
        let check = evaluate(&source, 39029, &settings);
        assert_eq!(check.status, "ok");
        assert!(!check.detail.contains("private"));
        assert!(!check.detail.contains("DO_NOT_PRINT"));
    }

    #[test]
    fn unknown_format_is_unverified_rather_than_success() {
        let (_temp, source, _) = installation();
        assert_eq!(evaluate(&source, 39029, &json!({})).status, "warn");
    }
}
