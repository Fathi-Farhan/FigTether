//! The owner only runtime directory.
//!
//! The runtime directory is the only place where FigTether writes to disk.
//! It holds the daemon descriptor (instance id, port, and the client
//! credential), an empty OS-lock file, and the rotating diagnostic log.
//! The CLI may also retain a plugin manifest location in `plugin.json`.
//!
//! The directory is per user and must not be readable by other accounts:
//!
//! - on POSIX the mode is set to `0700` and verified afterwards;
//! - on Windows the directory lives under `%LOCALAPPDATA%`, which is already a
//!   per user location with a per user DACL. The standard library does not
//!   expose the security descriptor, so the check is a documented best
//!   effort: the directory must exist, must not be a reparse point, and must
//!   live inside the per user root.
//!
//! When the directory cannot be secured the daemon fails closed: it refuses
//! to write credentials and it does not start the listener.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Name of the runtime directory inside the per user state root.
pub const RUNTIME_DIR_NAME: &str = "figtether";

/// Name of the daemon descriptor file.
pub const DAEMON_FILE_NAME: &str = "daemon.json";

/// Name of the rotating diagnostic log.
pub const LOG_FILE_NAME: &str = "figtether.log";

/// Windows file attribute that marks a reparse point.
#[cfg(windows)]
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

/// Failures of the runtime directory handling.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeDirError {
    /// The per user state root could not be determined.
    #[error("the per user state directory could not be determined")]
    Unresolved,
    /// The directory could not be created.
    #[error("the runtime directory could not be created: {0}")]
    Create(#[source] io::Error),
    /// The directory is not owner only.
    #[error("the runtime directory is not owner only: {0}")]
    NotOwnerOnly(String),
    /// A runtime file could not be written.
    #[error("the runtime file could not be written: {0}")]
    Write(#[source] io::Error),
    /// A runtime file could not be read.
    #[error("the runtime file could not be read: {0}")]
    Read(#[source] io::Error),
    /// A runtime file is malformed.
    #[error("the runtime file is malformed: {0}")]
    Malformed(String),
    /// Another daemon owns this directory, even if it uses another port.
    #[error("another daemon owns this runtime directory; stop it first or use a separate --runtime-dir ({0})")]
    Locked(#[source] io::Error),
}

/// The descriptor the daemon writes into the runtime directory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonFile {
    /// Identity of this daemon instance.
    pub instance_id: Uuid,
    /// Identity of the local installation that owns the sessions.
    ///
    /// The credential in this file identifies the installation, and the
    /// installation owns every session that was granted on this machine. The
    /// field is additive: a reader that does not know it ignores it, and a
    /// file of an older build still parses.
    #[serde(default)]
    pub installation_id: Uuid,
    /// Port the daemon listens on.
    pub port: u16,
    /// Credential the local clients present on their first frame.
    pub client_credential: String,
    /// Bridge protocol version the daemon speaks.
    pub protocol_version: String,
    /// When the daemon started.
    pub created_at: DateTime<Utc>,
}

/// The owner only runtime directory of the daemon.
#[derive(Debug, Clone)]
pub struct RuntimeDir {
    /// Absolute path of the directory.
    path: PathBuf,
    /// Whether the path was chosen by the caller instead of the platform.
    explicit: bool,
}

impl RuntimeDir {
    /// Resolve the runtime directory and make sure it is owner only.
    ///
    /// An explicit path wins over the platform default. The default is
    /// `%LOCALAPPDATA%\figtether` on Windows and
    /// `$XDG_STATE_HOME/figtether`, falling back to
    /// `~/.local/state/figtether`, on POSIX.
    pub fn resolve(explicit: Option<&Path>) -> Result<Self, RuntimeDirError> {
        let (path, explicit) = match explicit {
            Some(path) => (path.to_path_buf(), true),
            None => (default_path()?, false),
        };
        let dir = Self { path, explicit };
        dir.create()?;
        dir.ensure_owner_only()?;
        Ok(dir)
    }

    /// The path of the runtime directory.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether the path is the platform default instead of a caller choice.
    pub fn is_default_path(&self) -> bool {
        !self.explicit
    }

    /// The path of the daemon descriptor.
    pub fn daemon_file(&self) -> PathBuf {
        self.path.join(DAEMON_FILE_NAME)
    }

    /// The path of the rotating diagnostic log.
    pub fn log_file(&self) -> PathBuf {
        self.path.join(LOG_FILE_NAME)
    }

    /// Hold one OS lock for the daemon's entire lifetime. Keep the lock file:
    /// removing it would let a second process lock a different inode.
    /// The OS releases the lock when the process exits, including after a crash.
    pub fn lock(&self) -> Result<File, RuntimeDirError> {
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options
            .open(self.path.join("daemon.lock"))
            .map_err(RuntimeDirError::Write)?;
        fs2::FileExt::try_lock_exclusive(&file).map_err(RuntimeDirError::Locked)?;
        Ok(file)
    }

    /// Write the daemon descriptor with owner only permissions.
    pub fn write_daemon_file(&self, file: &DaemonFile) -> Result<(), RuntimeDirError> {
        let body = serde_json::to_vec_pretty(file).map_err(|err| {
            RuntimeDirError::Write(io::Error::new(io::ErrorKind::InvalidData, err))
        })?;
        let path = self.daemon_file();
        let mut handle = open_private(&path)?;
        handle
            .write_all(&body)
            .and_then(|()| handle.flush())
            .map_err(RuntimeDirError::Write)?;
        Ok(())
    }

    /// Read the daemon descriptor.
    pub fn read_daemon_file(&self) -> Result<DaemonFile, RuntimeDirError> {
        let body = fs::read(self.daemon_file()).map_err(RuntimeDirError::Read)?;
        serde_json::from_slice(&body).map_err(|err| RuntimeDirError::Malformed(err.to_string()))
    }

    /// Remove the credential file of this daemon.
    ///
    /// The diagnostic log is kept: the retention policy is a rotation policy,
    /// not a deletion on exit.
    pub fn remove_runtime_files(&self) -> Result<(), RuntimeDirError> {
        let path = self.daemon_file();
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(RuntimeDirError::Write(err)),
        }
    }

    /// Create the directory when it does not exist yet.
    fn create(&self) -> Result<(), RuntimeDirError> {
        fs::create_dir_all(&self.path).map_err(RuntimeDirError::Create)
    }

    /// Verify that only the owner may enter the directory.
    fn ensure_owner_only(&self) -> Result<(), RuntimeDirError> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let mut permissions = fs::metadata(&self.path)
                .map_err(RuntimeDirError::Create)?
                .permissions();
            permissions.set_mode(0o700);
            fs::set_permissions(&self.path, permissions).map_err(RuntimeDirError::Create)?;

            let mode = fs::metadata(&self.path)
                .map_err(RuntimeDirError::Create)?
                .permissions()
                .mode();
            if mode & 0o077 != 0 {
                return Err(RuntimeDirError::NotOwnerOnly(format!(
                    "mode {:o} is readable by other accounts",
                    mode & 0o777
                )));
            }
            Ok(())
        }

        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;

            let metadata = fs::metadata(&self.path).map_err(RuntimeDirError::Create)?;
            if !metadata.is_dir() {
                return Err(RuntimeDirError::NotOwnerOnly(
                    "the runtime path is not a directory".to_string(),
                ));
            }
            if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err(RuntimeDirError::NotOwnerOnly(
                    "the runtime path is a reparse point".to_string(),
                ));
            }
            if !self.explicit {
                if let Ok(root) = per_user_root() {
                    if !self.path.starts_with(&root) {
                        return Err(RuntimeDirError::NotOwnerOnly(
                            "the runtime path is outside the per user directory".to_string(),
                        ));
                    }
                }
            }
            Ok(())
        }

        #[cfg(not(any(unix, windows)))]
        {
            let _ = &self.path;
            Err(RuntimeDirError::NotOwnerOnly(
                "the platform does not expose a permission model".to_string(),
            ))
        }
    }
}

/// The per user state root of the platform.
fn per_user_root() -> Result<PathBuf, RuntimeDirError> {
    #[cfg(windows)]
    {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            if !local.is_empty() {
                return Ok(PathBuf::from(local));
            }
        }
        if let Some(profile) = std::env::var_os("USERPROFILE") {
            if !profile.is_empty() {
                return Ok(PathBuf::from(profile).join("AppData").join("Local"));
            }
        }
        Err(RuntimeDirError::Unresolved)
    }

    #[cfg(unix)]
    {
        if let Some(state) = std::env::var_os("XDG_STATE_HOME") {
            if !state.is_empty() {
                return Ok(PathBuf::from(state));
            }
        }
        if let Some(home) = std::env::var_os("HOME") {
            if !home.is_empty() {
                return Ok(PathBuf::from(home).join(".local").join("state"));
            }
        }
        Err(RuntimeDirError::Unresolved)
    }

    #[cfg(not(any(unix, windows)))]
    {
        Err(RuntimeDirError::Unresolved)
    }
}

/// The default runtime directory path.
fn default_path() -> Result<PathBuf, RuntimeDirError> {
    Ok(per_user_root()?.join(RUNTIME_DIR_NAME))
}

/// Open a file with owner only permissions.
fn open_private(path: &Path) -> Result<File, RuntimeDirError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;

        let handle = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(RuntimeDirError::Write)?;
        Ok(handle)
    }

    #[cfg(not(unix))]
    {
        let handle = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .map_err(RuntimeDirError::Write)?;
        Ok(handle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_runtime_dir_is_created_and_secured() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join("nested").join("figtether");
        let dir = RuntimeDir::resolve(Some(&path)).expect("resolved");
        assert!(dir.path().is_dir());
        assert_eq!(
            dir.daemon_file().file_name().and_then(|n| n.to_str()),
            Some(DAEMON_FILE_NAME)
        );
        assert_eq!(
            dir.log_file().file_name().and_then(|n| n.to_str()),
            Some(LOG_FILE_NAME)
        );
    }

    #[test]
    fn daemon_file_round_trips() {
        let temp = tempfile::tempdir().expect("temp dir");
        let dir = RuntimeDir::resolve(Some(&temp.path().join("figtether"))).expect("resolved");
        let file = DaemonFile {
            instance_id: Uuid::new_v4(),
            installation_id: Uuid::new_v4(),
            port: 38947,
            client_credential: "credential-value".to_string(),
            protocol_version: "1.0".to_string(),
            created_at: Utc::now(),
        };
        dir.write_daemon_file(&file).expect("written");
        let read = dir.read_daemon_file().expect("read");
        assert_eq!(read.instance_id, file.instance_id);
        assert_eq!(read.installation_id, file.installation_id);
        assert_eq!(read.port, file.port);
        assert_eq!(read.client_credential, file.client_credential);
    }

    #[test]
    fn a_file_of_an_older_build_still_parses() {
        let temp = tempfile::tempdir().expect("temp dir");
        let dir = RuntimeDir::resolve(Some(&temp.path().join("figtether"))).expect("resolved");
        let body = serde_json::json!({
            "instance_id": Uuid::new_v4(),
            "port": 38947,
            "client_credential": "credential-value",
            "protocol_version": "1.0",
            "created_at": Utc::now(),
        });
        std::fs::write(dir.daemon_file(), body.to_string()).expect("written");
        let read = dir.read_daemon_file().expect("read");
        assert_eq!(read.port, 38947);
        assert_eq!(read.installation_id, Uuid::nil());
    }

    #[test]
    fn runtime_files_are_removed_on_request() {
        let temp = tempfile::tempdir().expect("temp dir");
        let dir = RuntimeDir::resolve(Some(&temp.path().join("figtether"))).expect("resolved");
        let file = DaemonFile {
            instance_id: Uuid::new_v4(),
            installation_id: Uuid::new_v4(),
            port: 38947,
            client_credential: "credential-value".to_string(),
            protocol_version: "1.0".to_string(),
            created_at: Utc::now(),
        };
        dir.write_daemon_file(&file).expect("written");
        dir.remove_runtime_files().expect("removed");
        assert!(!dir.daemon_file().exists());
        dir.remove_runtime_files().expect("removal is idempotent");
    }

    #[cfg(unix)]
    #[test]
    fn runtime_dir_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("temp dir");
        let dir = RuntimeDir::resolve(Some(&temp.path().join("figtether"))).expect("resolved");
        let mode = fs::metadata(dir.path())
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    #[cfg(windows)]
    #[test]
    fn runtime_dir_accepts_an_explicit_local_directory() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join("figtether");
        let dir = RuntimeDir::resolve(Some(&path)).expect("resolved");
        assert!(dir.path().is_dir());
        assert!(!dir.is_default_path());
    }

    #[test]
    fn default_runtime_dir_lives_in_the_per_user_root() {
        let root = per_user_root().expect("per user root");
        let default = default_path().expect("default path");
        assert!(default.starts_with(&root));
        assert_eq!(
            default.file_name().and_then(|n| n.to_str()),
            Some(RUNTIME_DIR_NAME)
        );
    }
}
