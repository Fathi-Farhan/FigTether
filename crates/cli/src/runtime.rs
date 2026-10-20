//! The runtime directory of FigTether.
//!
//! The runtime directory is the only on-disk secret surface of the product. It
//! is per user, per host, and owner only. The daemon and the CLI share one
//! convention, which is implemented once in the daemon crate:
//!
//! - Windows: `%LOCALAPPDATA%\figtether`;
//! - unix: `$XDG_STATE_HOME/figtether`, otherwise `~/.local/state/figtether`.
//!
//! The directory holds `daemon.json`, the descriptor that carries the port of
//! the daemon and the credential of the local clients, plus the rotating
//! diagnostic log. The CLI only reads that file; the daemon writes it.

use std::path::Path;

use figtether_daemon::{DaemonFile, RuntimeDir, RuntimeDirError};

/// Resolve the runtime directory and make sure it is owner only.
///
/// The call fails closed: when the directory cannot be secured, the caller
/// must not write a credential into it and must not start a listener.
pub fn resolve(explicit: Option<&Path>) -> Result<RuntimeDir, RuntimeDirError> {
    RuntimeDir::resolve(explicit)
}

/// The descriptor of the running daemon, when there is one.
pub fn daemon_file(runtime: &RuntimeDir) -> Option<DaemonFile> {
    runtime.read_daemon_file().ok()
}

/// The state of the runtime directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// The directory exists and only the owner may reach it.
    OwnerOnly,
    /// The directory cannot be used.
    Unusable,
}

impl State {
    /// Stable name of the state.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OwnerOnly => "owner_only",
            Self::Unusable => "unusable",
        }
    }
}

/// The result of the runtime directory check.
#[derive(Debug, Clone)]
pub struct Report {
    /// The resolved path, when the directory could be resolved.
    pub path: String,
    /// The state of the directory.
    pub state: State,
    /// Human readable explanation.
    pub detail: String,
}

impl Report {
    /// Whether the report blocks the daemon from starting.
    pub fn is_blocking(&self) -> bool {
        self.state == State::Unusable
    }
}

/// Check the runtime directory.
///
/// The check is the check of the daemon: the same resolution, the same
/// permission model, and the same fail closed verdict.
pub fn check(explicit: Option<&Path>) -> Report {
    match resolve(explicit) {
        Ok(dir) => Report {
            path: dir.path().display().to_string(),
            state: State::OwnerOnly,
            detail: detail(),
        },
        Err(err) => Report {
            path: explicit
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "the platform runtime directory".to_string()),
            state: State::Unusable,
            detail: err.to_string(),
        },
    }
}

/// The explanation of an owner only runtime directory.
fn detail() -> String {
    #[cfg(unix)]
    {
        "the directory mode is 0700, so only the owner may reach it".to_string()
    }
    #[cfg(not(unix))]
    {
        "the directory lives in the per user root and is not a reparse point".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_explicit_path_wins() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join("custom-runtime");
        let dir = resolve(Some(&path)).expect("resolves");
        assert_eq!(dir.path(), path);
    }

    #[test]
    fn the_platform_directory_is_resolved() {
        let dir = resolve(None).expect("resolves");
        let text = dir.path().display().to_string();
        assert!(text.contains("figtether"), "{text}");
    }

    #[test]
    fn a_fresh_runtime_directory_is_owner_only() {
        let temp = tempfile::tempdir().expect("temp dir");
        let report = check(Some(&temp.path().join("figtether")));
        assert_eq!(report.state, State::OwnerOnly);
        assert!(!report.is_blocking());
    }

    #[test]
    fn a_file_is_not_a_usable_runtime_directory() {
        let temp = tempfile::tempdir().expect("temp dir");
        let file = temp.path().join("not-a-directory");
        std::fs::write(&file, b"x").expect("writes");
        let report = check(Some(&file));
        assert_eq!(report.state, State::Unusable);
        assert!(report.is_blocking());
    }

    #[test]
    fn a_missing_descriptor_means_no_daemon_runs() {
        let temp = tempfile::tempdir().expect("temp dir");
        let dir = resolve(Some(&temp.path().join("figtether"))).expect("resolves");
        assert!(daemon_file(&dir).is_none());
    }
}
