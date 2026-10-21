//! Source installation updates. No remote release endpoint is assumed.
//!
//! Build into a staging installation before touching the running executable.
//! Renaming the old executable first also works around Windows file locking.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::Context;

use crate::cli::UpdateArgs;
use crate::commands::CommandOutcome;
use crate::output;

/// Update the source checkout, plugin, and this executable.
pub fn run(args: UpdateArgs) -> anyhow::Result<CommandOutcome> {
    let source = source_root(args.source.as_deref())?;
    let executable = std::env::current_exe()?.canonicalize()?;
    output::section("Update FigTether from source");
    output::field("source", &source.display().to_string());
    output::field("executable", &executable.display().to_string());
    output::line("This builds the CLI and plugin; running daemons and agents are not stopped.");
    if args.dry_run {
        if !args.local {
            output::line(
                "1. Require a clean Git checkout and configured upstream; pull --ff-only.",
            );
        }
        output::line("2. Build a staged CLI with cargo install --locked --path crates/cli.");
        output::line("3. Run npm ci and npm run build in plugins/figma.");
        output::line(
            "4. Replace this executable, retaining a rollback copy until replacement succeeds.",
        );
        output::line("Dry run: no commands executed or files changed.");
        return Ok(CommandOutcome::ok());
    }
    let directory = executable
        .parent()
        .context("executable has no parent directory")?;
    let _lock = UpdateLock::acquire(directory)?;
    if !args.local {
        ensure_clean_upstream(&source)?;
        execute(&source, "git", &["pull", "--ff-only"], "source update")?;
    }

    // Keep staging beside the executable: the final rename stays on one volume.
    let directory = executable
        .parent()
        .context("executable has no parent directory")?;
    let staging = directory.join(format!(".figtether-update-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&staging)
        .context("cannot create update staging directory beside executable")?;
    let result = build_and_replace(&source, &executable, &staging);
    // This directory is created by this invocation, never a user-supplied target.
    let _ = std::fs::remove_dir_all(&staging);
    result?;
    output::line(
        "Update complete. Restart the daemon and your MCP entry when all agents are ready.",
    );
    output::line(
        "Close and reopen the Figma plugin to load its rebuilt bundle; pair/grant again if needed.",
    );
    Ok(CommandOutcome::ok())
}

fn source_root(explicit: Option<&Path>) -> anyhow::Result<PathBuf> {
    let compiled = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = explicit.unwrap_or(&compiled).canonicalize().context(
        "source checkout is missing; pass --source PATH to the extracted/cloned FigTether source",
    )?;
    for file in [
        "Cargo.toml",
        "Cargo.lock",
        "crates/cli/Cargo.toml",
        "plugins/figma/package-lock.json",
    ] {
        anyhow::ensure!(
            source.join(file).is_file(),
            "not a FigTether source checkout: missing {file}"
        );
    }
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(source.join("plugins/figma/package.json"))?)?;
    anyhow::ensure!(
        manifest["name"] == "@figtether/figma-plugin",
        "unexpected plugin package in source directory"
    );
    Ok(source)
}

fn ensure_clean_upstream(source: &Path) -> anyhow::Result<()> {
    let top = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(source)
        .output()
        .context("Git is required; ZIP users can use --local")?;
    anyhow::ensure!(
        top.status.success(),
        "source is not a Git checkout; ZIP users can use --local after refreshing their source"
    );
    let top = PathBuf::from(String::from_utf8(top.stdout)?.trim()).canonicalize()?;
    anyhow::ensure!(
        top == source.canonicalize()?,
        "source must be the Git repository root; refusing to update a parent repository"
    );
    let status = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=normal"])
        .current_dir(source)
        .output()
        .context("Git is required for source updates; ZIP users can use --local after refreshing their source")?;
    anyhow::ensure!(
        status.status.success(),
        "cannot inspect Git checkout; ZIP users can use --local after refreshing their source"
    );
    anyhow::ensure!(status.stdout.is_empty(), "source checkout has local changes; commit or move them before updating, or use --local to rebuild them without pulling");
    let upstream = Command::new("git")
        .args([
            "rev-parse",
            "--abbrev-ref",
            "--symbolic-full-name",
            "@{upstream}",
        ])
        .current_dir(source)
        .output()?;
    anyhow::ensure!(upstream.status.success(), "no Git upstream configured; clone the project with its upstream, or use --local to rebuild existing source");
    Ok(())
}

fn execute(source: &Path, tool: &str, args: &[&str], stage: &str) -> anyhow::Result<()> {
    output::section(stage);
    let status = Command::new(tool)
        .args(args)
        .current_dir(source)
        .status()
        .with_context(|| format!("cannot run {tool}; install the source-build prerequisites"))?;
    anyhow::ensure!(
        status.success(),
        "{stage} failed; the CLI executable was not replaced"
    );
    Ok(())
}

fn build_and_replace(source: &Path, executable: &Path, staging: &Path) -> anyhow::Result<()> {
    let stage = staging
        .to_str()
        .context("staging path is not valid Unicode")?;
    execute(
        source,
        "cargo",
        &[
            "install",
            "--locked",
            "--path",
            "crates/cli",
            "--root",
            stage,
        ],
        "Build CLI",
    )?;
    let candidate = staging
        .join("bin")
        .join(format!("figtether{}", std::env::consts::EXE_SUFFIX));
    anyhow::ensure!(
        candidate.is_file(),
        "staged executable is missing; the CLI was not replaced"
    );
    let version = Command::new(&candidate)
        .arg("--version")
        .output()
        .context("cannot verify staged CLI")?;
    anyhow::ensure!(
        version.status.success() && version.stdout.starts_with(b"figtether "),
        "staged CLI version check failed"
    );
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    execute(
        source,
        npm,
        &["--prefix", "plugins/figma", "ci"],
        "Install plugin dependencies",
    )?;
    execute(
        source,
        npm,
        &["--prefix", "plugins/figma", "run", "build"],
        "Build Figma plugin",
    )?;
    let backup = executable.with_file_name(format!(
        "figtether-previous-{}.exe.bak",
        uuid::Uuid::new_v4()
    ));
    replace_with_rollback(executable, &candidate, &backup)?;
    if std::fs::remove_file(&backup).is_err() {
        // On Windows the original process may keep this file locked until exit.
        output::note(&format!(
            "Previous executable retained at {}; it can be removed after old processes exit.",
            backup.display()
        ));
    }
    Ok(())
}

fn replace_with_rollback(executable: &Path, candidate: &Path, backup: &Path) -> anyhow::Result<()> {
    std::fs::rename(executable, backup)
        .context("cannot move the old executable; close processes using it and retry")?;
    if let Err(error) = std::fs::rename(candidate, executable) {
        if let Err(rollback) = std::fs::rename(backup, executable) {
            anyhow::bail!(
                "replacement failed ({error}); rollback failed ({rollback}); restore {} to {}",
                backup.display(),
                executable.display()
            );
        }
        return Err(error).context("replacement failed; previous executable restored");
    }
    Ok(())
}

struct UpdateLock {
    path: PathBuf,
    handle: Option<std::fs::File>,
}

impl UpdateLock {
    fn acquire(directory: &Path) -> anyhow::Result<Self> {
        let path = directory.join(".figtether-update.lock");
        let handle = std::fs::OpenOptions::new().write(true).create_new(true).open(&path)
            .context("cannot acquire update lock; another update may be running. If an update crashed, remove .figtether-update.lock beside the executable after confirming it stopped")?;
        Ok(Self {
            path,
            handle: Some(handle),
        })
    }
}

impl Drop for UpdateLock {
    fn drop(&mut self) {
        self.handle.take();
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn update_flags_parse() {
        let cli = crate::cli::Cli::try_parse_from([
            "figtether",
            "update",
            "--source",
            "path with spaces",
            "--local",
            "--dry-run",
        ])
        .unwrap();
        let crate::cli::Command::Update(args) = cli.into_command() else {
            panic!("wrong command")
        };
        assert_eq!(args.source.unwrap(), PathBuf::from("path with spaces"));
        assert!(args.local && args.dry_run);
    }

    #[test]
    fn missing_source_is_actionable() {
        let temp = tempfile::tempdir().unwrap();
        let error = source_root(Some(temp.path())).unwrap_err().to_string();
        assert!(error.contains("missing Cargo.toml"));
    }

    #[test]
    fn failed_replacement_restores_original() {
        let temp = tempfile::tempdir().unwrap();
        let executable = temp.path().join("figtether");
        let backup = temp.path().join("backup");
        std::fs::write(&executable, b"original").unwrap();
        assert!(replace_with_rollback(&executable, &temp.path().join("missing"), &backup).is_err());
        assert_eq!(std::fs::read(&executable).unwrap(), b"original");
        assert!(!backup.exists());
    }

    #[test]
    fn successful_replacement_retains_rollback_copy() {
        let temp = tempfile::tempdir().unwrap();
        let executable = temp.path().join("figtether");
        let candidate = temp.path().join("candidate");
        let backup = temp.path().join("backup");
        std::fs::write(&executable, b"original").unwrap();
        std::fs::write(&candidate, b"new").unwrap();
        replace_with_rollback(&executable, &candidate, &backup).unwrap();
        assert_eq!(std::fs::read(executable).unwrap(), b"new");
        assert_eq!(std::fs::read(backup).unwrap(), b"original");
    }

    #[test]
    fn concurrent_update_is_refused_and_lock_is_released() {
        let temp = tempfile::tempdir().unwrap();
        let lock = UpdateLock::acquire(temp.path()).unwrap();
        assert!(UpdateLock::acquire(temp.path()).is_err());
        drop(lock);
        assert!(UpdateLock::acquire(temp.path()).is_ok());
    }

    fn git(source: &Path, args: &[&str]) {
        let result = Command::new("git")
            .args(args)
            .current_dir(source)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }

    #[test]
    fn source_update_refuses_dirty_worktree_and_missing_upstream() {
        let temp = tempfile::tempdir().unwrap();
        git(temp.path(), &["init"]);
        std::fs::write(temp.path().join("fixture"), "initial").unwrap();
        git(temp.path(), &["add", "fixture"]);
        git(
            temp.path(),
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "fixture",
            ],
        );
        assert!(ensure_clean_upstream(temp.path())
            .unwrap_err()
            .to_string()
            .contains("upstream"));
        std::fs::write(temp.path().join("fixture"), "local edits").unwrap();
        assert!(ensure_clean_upstream(temp.path())
            .unwrap_err()
            .to_string()
            .contains("local changes"));
        assert_eq!(
            std::fs::read_to_string(temp.path().join("fixture")).unwrap(),
            "local edits"
        );
    }

    #[test]
    fn source_update_refuses_parent_repository() {
        let temp = tempfile::tempdir().unwrap();
        git(temp.path(), &["init"]);
        let child = temp.path().join("child");
        std::fs::create_dir(&child).unwrap();
        assert!(ensure_clean_upstream(&child)
            .unwrap_err()
            .to_string()
            .contains("parent repository"));
    }

    #[test]
    fn upstream_fast_forward_works_and_divergence_is_refused() {
        let temp = tempfile::tempdir().unwrap();
        let origin = temp.path().join("origin");
        let checkout = temp.path().join("checkout");
        std::fs::create_dir(&origin).unwrap();
        git(&origin, &["init"]);
        std::fs::write(origin.join("fixture"), "one").unwrap();
        git(&origin, &["add", "fixture"]);
        git(
            &origin,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "initial",
            ],
        );
        git(
            temp.path(),
            &[
                "clone",
                origin.to_str().unwrap(),
                checkout.to_str().unwrap(),
            ],
        );
        ensure_clean_upstream(&checkout).unwrap();
        std::fs::write(origin.join("fixture"), "two").unwrap();
        git(&origin, &["add", "fixture"]);
        git(
            &origin,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "upstream",
            ],
        );
        execute(
            &checkout,
            "git",
            &["pull", "--ff-only"],
            "test fast-forward",
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(checkout.join("fixture")).unwrap(),
            "two"
        );
        std::fs::write(checkout.join("local"), "keep me").unwrap();
        git(&checkout, &["add", "local"]);
        git(
            &checkout,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "local",
            ],
        );
        std::fs::write(origin.join("fixture"), "three").unwrap();
        git(&origin, &["add", "fixture"]);
        git(
            &origin,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-m",
                "divergent",
            ],
        );
        assert!(execute(&checkout, "git", &["pull", "--ff-only"], "test divergence").is_err());
        assert_eq!(
            std::fs::read_to_string(checkout.join("local")).unwrap(),
            "keep me"
        );
        assert_eq!(
            std::fs::read_to_string(checkout.join("fixture")).unwrap(),
            "two"
        );
    }
}
