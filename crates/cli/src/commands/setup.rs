//! Reversible, project-scoped host integration. Preview is the default.
use crate::{
    cli::{ClientKind, SetupArgs},
    commands::{config, CommandOutcome},
    output,
};
use anyhow::{bail, Context};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
const SKILL: &str = include_str!("../../../../integrations/figtether/SKILL.md");
const BEGIN: &str = "# BEGIN FigTether managed setup\n";
const END: &str = "# END FigTether managed setup\n";
fn hash(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

fn paths(client: ClientKind) -> anyhow::Result<(&'static str, &'static str)> {
    match client {
        ClientKind::Codex => Ok((
            ".codex/config.toml",
            ".agents/skills/figtether-design/SKILL.md",
        )),
        ClientKind::Gemini => Ok((
            ".gemini/settings.json",
            ".gemini/skills/figtether-design/SKILL.md",
        )),
        ClientKind::Claude => Ok((".mcp.json", ".claude/skills/figtether-design/SKILL.md")),
        ClientKind::Generic => {
            bail!("setup requires codex, gemini, or claude; use config print for a generic client")
        }
    }
}

/// Refuse linked config/skill paths instead of following them outside the workspace.
fn safe_path(root: &Path, relative: &str) -> anyhow::Result<PathBuf> {
    let mut current = root.to_path_buf();
    for part in Path::new(relative).components() {
        current.push(part);
        if let Ok(meta) = fs::symlink_metadata(&current) {
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                if meta.file_attributes() & 0x400 != 0 {
                    bail!("setup refuses a reparse path");
                }
            }
            if meta.file_type().is_symlink() {
                bail!("setup refuses linked path {}", current.display());
            }
        }
    }
    Ok(current)
}

fn write_atomic(path: &Path, body: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let stage = path.with_extension(format!("figtether-{}.tmp", std::process::id()));
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&stage)?;
    file.write_all(body.as_bytes())?;
    drop(file);
    fs::rename(&stage, path).with_context(|| format!("could not replace {}", path.display()))?;
    Ok(())
}

fn existing(path: &Path) -> anyhow::Result<String> {
    if path.exists() {
        Ok(fs::read_to_string(path)?.replace("\r\n", "\n"))
    } else {
        Ok(String::new())
    }
}

/// Build a concrete edit without writing; existing unrelated settings are preserved.
fn plan(
    root: &Path,
    client: ClientKind,
    binary: &Path,
    args: &[String],
    remove: bool,
) -> anyhow::Result<Value> {
    let (config_rel, skill_rel) = paths(client)?;
    let config_path = safe_path(root, config_rel)?;
    let skill_path = safe_path(root, skill_rel)?;
    let record_rel = format!(".figtether/setup-{}.json", client.as_str());
    let record_path = safe_path(root, &record_rel)?;
    let old_record = existing(&record_path)?;
    let record: Value = if old_record.is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&old_record)?
    };
    if remove && record.is_null() {
        bail!("no managed setup for this client");
    }
    let old_skill = existing(&skill_path)?;
    if !old_skill.is_empty() && old_skill != SKILL && record["skill_sha256"] != hash(&old_skill) {
        bail!("the existing design skill was edited or belongs to another installation; preserve it before setup");
    }
    let old_config = existing(&config_path)?;
    if client == ClientKind::Codex && !old_config.is_empty() {
        let _: toml::Value = toml::from_str(&old_config)
            .context("existing Codex project settings are invalid TOML; left untouched")?;
    }
    let rendered = config::render(client, binary, args);
    let (next, entry) = if client == ClientKind::Codex {
        let mut preserved = old_config.clone();
        if let Some(start) = preserved.find(BEGIN) {
            let end = start
                + preserved[start..]
                    .find(END)
                    .context("unterminated FigTether setup block")?
                + END.len();
            let owned = &preserved[start..end];
            if record["config_sha256"] != hash(owned) {
                bail!("managed MCP block was edited; setup will not overwrite it");
            }
            preserved.replace_range(start..end, "");
        } else if !record.is_null() {
            bail!("the managed MCP entry is missing; setup will not guess how to restore edited settings");
        }
        let parsed: toml::Value = toml::from_str(&preserved)?;
        if parsed
            .get("mcp_servers")
            .and_then(|v| v.get("figtether"))
            .is_some()
        {
            bail!("an unmanaged FigTether entry exists; reconcile it before setup");
        }
        let entry = format!("{BEGIN}{rendered}{END}");
        let next = if remove {
            preserved
        } else {
            format!(
                "{}{}{}",
                preserved,
                if preserved.is_empty() || preserved.ends_with('\n') {
                    ""
                } else {
                    "\n"
                },
                entry
            )
        };
        let _: toml::Value =
            toml::from_str(&next).context("proposed Codex settings must be valid TOML")?;
        (next, entry)
    } else {
        let mut settings: Value = if old_config.is_empty() {
            json!({})
        } else {
            serde_json::from_str(&old_config)
                .context("client settings must be valid JSON; existing data was left untouched")?
        };
        let object = settings
            .as_object_mut()
            .context("client settings must be a JSON object")?;
        let servers = object
            .entry("mcpServers")
            .or_insert(json!({}))
            .as_object_mut()
            .context("mcpServers must be an object")?;
        if let Some(old) = servers.get("figtether") {
            if record["config_sha256"] != hash(&serde_json::to_string(old)?) {
                bail!("an unmanaged or edited FigTether entry exists; setup will not overwrite it");
            }
        } else if !record.is_null() {
            bail!("the managed MCP entry is missing");
        }
        let entry: Value =
            serde_json::from_str::<Value>(&rendered)?["mcpServers"]["figtether"].clone();
        if remove {
            servers.remove("figtether");
        } else {
            servers.insert("figtether".into(), entry.clone());
        }
        (
            serde_json::to_string_pretty(&settings)? + "\n",
            serde_json::to_string(&entry)?,
        )
    };
    Ok(
        json!({"client":client.as_str(),"config":config_rel,"skill":skill_rel,"record":record_rel,"configuration":next,"skill_content":SKILL,"managed_record":{"version":1,"config_sha256":hash(&entry),"skill_sha256":hash(SKILL)},"action":if remove{"remove"}else{"install"}}),
    )
}

pub fn run(args: SetupArgs) -> anyhow::Result<CommandOutcome> {
    let root = fs::canonicalize(&args.workspace).context("choose an existing design workspace")?;
    let _lock = if args.apply || args.remove {
        Some(SetupLock::acquire(&root)?)
    } else {
        None
    };
    let binary = config::current_binary();
    let mut command_args = vec!["mcp".to_owned(), "--stdio".to_owned()];
    if let Some(runtime) = &args.runtime_dir {
        command_args.extend([
            "--runtime-dir".into(),
            fs::canonicalize(runtime)?.display().to_string(),
        ]);
    }
    if let Some(port) = args.port {
        command_args.extend(["--port".into(), port.to_string()]);
    }
    if args.check {
        let (registered, registered_args) = managed_entry(&root, args.client)?;
        return super::agent_check::run(&args, &root, &registered, &registered_args);
    }
    let proposed = plan(&root, args.client, &binary, &command_args, args.remove)?;
    if args.apply || args.remove {
        let config_path = safe_path(&root, proposed["config"].as_str().unwrap())?;
        let skill_path = safe_path(&root, proposed["skill"].as_str().unwrap())?;
        let record_path = safe_path(&root, proposed["record"].as_str().unwrap())?;
        write_atomic(&config_path, proposed["configuration"].as_str().unwrap())?;
        if args.remove {
            if skill_path.exists() {
                fs::remove_file(skill_path)?;
            }
            fs::remove_file(record_path)?;
        } else {
            write_atomic(&skill_path, SKILL)?;
            write_atomic(
                &record_path,
                &serde_json::to_string_pretty(&proposed["managed_record"])?,
            )?;
        }
    }
    // Do not print unrelated user settings, which may contain their credentials.
    let summary = json!({"client":args.client.as_str(),"action":proposed["action"],"written":args.apply||args.remove,"config":proposed["config"],"skill":proposed["skill"],"entry":config::render(args.client,&binary,&command_args),"next_action":"Restart/refresh MCP in your host, then ask it to call open_design for your frame. Run setup --check for adapter readiness. Host tools require host-side verification."});
    if args.json {
        output::json(&summary)?;
    } else {
        output::line(&serde_json::to_string_pretty(&summary)?);
        if !args.apply && !args.remove {
            output::line("Preview only. Add --apply to install these project-local files.");
        }
    }
    Ok(CommandOutcome::ok())
}

pub(super) fn installed(root: &Path, client: ClientKind) -> bool {
    paths(client).is_ok_and(|(config, skill)| {
        safe_path(root, config).is_ok_and(|p| p.is_file())
            && safe_path(root, skill).is_ok_and(|p| p.is_file())
            && safe_path(root, &format!(".figtether/setup-{}.json", client.as_str()))
                .is_ok_and(|p| p.is_file())
    }) && plan(root, client, Path::new("figtether"), &[], true).is_ok()
}

struct SetupLock(PathBuf);
impl SetupLock {
    fn acquire(root: &Path) -> anyhow::Result<Self> {
        let path = safe_path(root, ".figtether/setup.lock")?;
        fs::create_dir_all(path.parent().unwrap())?;
        let _file=fs::OpenOptions::new().write(true).create_new(true).open(&path).context("another setup is running; remove a stale .figtether/setup.lock only after verifying it stopped")?;
        Ok(Self(path))
    }
}
impl Drop for SetupLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn managed_entry(root: &Path, client: ClientKind) -> anyhow::Result<(PathBuf, Vec<String>)> {
    if !installed(root, client) {
        bail!("run setup --apply for this workspace first; edited or unmanaged entries need reconciliation");
    }
    let (relative, _) = paths(client)?;
    let text = existing(&safe_path(root, relative)?)?;
    let value: Value = if client == ClientKind::Codex {
        serde_json::to_value(toml::from_str::<toml::Value>(&text)?)?
    } else {
        serde_json::from_str(&text)?
    };
    let entry = &value[if client == ClientKind::Codex {
        "mcp_servers"
    } else {
        "mcpServers"
    }]["figtether"];
    let binary = PathBuf::from(
        entry["command"]
            .as_str()
            .context("missing adapter executable")?,
    );
    let args: Vec<String> = serde_json::from_value(entry["args"].clone())?;
    if args.len() < 2 || args[0] != "mcp" || args[1] != "--stdio" {
        bail!("unexpected adapter arguments");
    }
    Ok((binary, args))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn toml_inline_entries_and_edited_skills_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join(".codex")).unwrap();
        let config = dir.path().join(".codex/config.toml");
        fs::write(
            &config,
            "mcp_servers = { figtether = { command = 'mine' } }\n",
        )
        .unwrap();
        assert!(plan(
            dir.path(),
            ClientKind::Codex,
            Path::new("other"),
            &[],
            false
        )
        .is_err());
        assert_eq!(
            fs::read_to_string(config).unwrap(),
            "mcp_servers = { figtether = { command = 'mine' } }\n"
        );
        let lock = SetupLock::acquire(dir.path()).unwrap();
        assert!(SetupLock::acquire(dir.path()).is_err());
        drop(lock);
        assert!(SetupLock::acquire(dir.path()).is_ok());
    }
    #[test]
    fn preview_preserves_settings_and_refuses_existing_ownership() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join(".mcp.json"),
            "{\"mcpServers\":{\"other\":{\"command\":\"other\"}},\"theme\":\"dark\"}",
        )
        .unwrap();
        let p = plan(
            dir.path(),
            ClientKind::Claude,
            Path::new("C:/tools/figtether.exe"),
            &["mcp".into()],
            false,
        )
        .unwrap();
        assert!(p["configuration"].as_str().unwrap().contains("other"));
        assert!(!dir.path().join(".claude").exists());
        fs::write(
            dir.path().join(".mcp.json"),
            "{\"mcpServers\":{\"figtether\":{\"command\":\"mine\"}}}",
        )
        .unwrap();
        assert!(plan(
            dir.path(),
            ClientKind::Claude,
            Path::new("tool"),
            &[],
            false
        )
        .is_err());
    }
}
