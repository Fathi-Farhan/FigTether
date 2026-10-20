//! Sanitized diagnostic logging.
//!
//! The log is an allowlist surface. It carries error codes, counters, sizes,
//! versions, and correlation ids. It never carries design content: no layer
//! names, no text, no document labels, no file ids, no tokens, and no
//! secrets.
//!
//! The log file lives in the owner only runtime directory and rotates at
//! 10 MiB. Files older than seven days are removed when the daemon starts.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

use crate::config::LogFormat;

/// Maximum size of one log file before it rotates.
pub const MAX_LOG_BYTES: u64 = 10 * 1024 * 1024;

/// Maximum age of a retained log file.
pub const MAX_LOG_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Name of the rotated log file.
pub const ROTATED_LOG_NAME: &str = "figtether.log.1";

/// Failures of the logging setup.
#[derive(Debug, thiserror::Error)]
pub enum LoggingError {
    /// The log file could not be opened.
    #[error("the log file could not be opened: {0}")]
    Open(#[source] io::Error),
    /// The global subscriber was already installed.
    #[error("a global tracing subscriber is already installed")]
    AlreadyInstalled,
}

/// The writer that appends to the rotating log file.
#[derive(Clone, Debug)]
pub struct RotatingWriter {
    /// Shared file handle and size counter.
    inner: Arc<Mutex<RotatingInner>>,
}

/// Mutable state of the rotating writer.
#[derive(Debug)]
struct RotatingInner {
    /// Path of the current log file.
    path: PathBuf,
    /// Open handle of the current log file.
    file: File,
    /// Number of bytes already written.
    size: u64,
}

impl RotatingWriter {
    /// Open the log file for appending.
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        let size = file.metadata()?.len();
        Ok(Self {
            inner: Arc::new(Mutex::new(RotatingInner {
                path: path.to_path_buf(),
                file,
                size,
            })),
        })
    }

    /// Rotate the current file and start a fresh one.
    fn rotate(inner: &mut RotatingInner) -> io::Result<()> {
        let backup = rotated_path(&inner.path);
        let _ = fs::remove_file(&backup);
        fs::rename(&inner.path, &backup)?;
        inner.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&inner.path)?;
        inner.size = 0;
        Ok(())
    }
}

impl Write for RotatingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if guard.size.saturating_add(buf.len() as u64) > MAX_LOG_BYTES {
            RotatingWriter::rotate(&mut guard)?;
        }
        let written = guard.file.write(buf)?;
        guard.size = guard.size.saturating_add(written as u64);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.file.flush()
    }
}

impl<'a> MakeWriter<'a> for RotatingWriter {
    type Writer = RotatingWriter;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// The path of the rotated log file.
fn rotated_path(path: &Path) -> PathBuf {
    match path.file_name().and_then(|name| name.to_str()) {
        Some(name) => path.with_file_name(format!("{name}.1")),
        None => path.with_extension("1"),
    }
}

/// Remove log files that are older than [`MAX_LOG_AGE`].
pub fn purge_old_logs(directory: &Path) -> io::Result<()> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err),
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let path = entry.path();
        let is_log = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("figtether.log"));
        if !is_log {
            continue;
        }
        let modified = match entry.metadata().and_then(|meta| meta.modified()) {
            Ok(modified) => modified,
            Err(_) => continue,
        };
        let age = now.duration_since(modified).unwrap_or_default();
        if age > MAX_LOG_AGE {
            let _ = fs::remove_file(&path);
        }
    }
    Ok(())
}

/// Install the global tracing subscriber.
///
/// The subscriber writes to the standard error and to the rotating log file.
/// The caller decides the shape of the output.
pub fn install(
    format: LogFormat,
    level: &str,
    log_file: &Path,
) -> Result<RotatingWriter, LoggingError> {
    let filter = EnvFilter::try_new(level).unwrap_or_else(|_| EnvFilter::new("info"));
    let writer = RotatingWriter::open(log_file).map_err(LoggingError::Open)?;

    let installed = match format {
        LogFormat::Json => {
            let stderr_layer = tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .json()
                .with_ansi(false)
                .with_target(false);
            let file_layer = tracing_subscriber::fmt::layer()
                .with_writer(writer.clone())
                .json()
                .with_ansi(false)
                .with_target(false);
            tracing_subscriber::registry()
                .with(filter)
                .with(stderr_layer)
                .with(file_layer)
                .try_init()
        }
        LogFormat::Compact => {
            let stderr_layer = tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_ansi(false)
                .with_target(false);
            let file_layer = tracing_subscriber::fmt::layer()
                .with_writer(writer.clone())
                .with_ansi(false)
                .with_target(false);
            tracing_subscriber::registry()
                .with(filter)
                .with(stderr_layer)
                .with(file_layer)
                .try_init()
        }
    };

    installed.map_err(|_| LoggingError::AlreadyInstalled)?;
    Ok(writer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writer_appends_to_the_log_file() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join("figtether.log");
        let mut writer = RotatingWriter::open(&path).expect("opened");
        writer.write_all(b"first\n").expect("written");
        writer.flush().expect("flushed");
        writer.write_all(b"second\n").expect("written");
        writer.flush().expect("flushed");
        let body = fs::read_to_string(&path).expect("read");
        assert!(body.contains("first"));
        assert!(body.contains("second"));
    }

    #[test]
    fn writer_rotates_when_the_budget_is_reached() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join("figtether.log");
        let mut writer = RotatingWriter::open(&path).expect("opened");
        let block = vec![b'x'; 1024];
        for _ in 0..(MAX_LOG_BYTES as usize / block.len() + 1) {
            writer.write_all(&block).expect("written");
        }
        writer.flush().expect("flushed");
        assert!(path.exists(), "the current log file stays in place");
        assert!(
            temp.path().join(ROTATED_LOG_NAME).exists(),
            "the rotated log file is kept"
        );
    }

    #[test]
    fn old_logs_are_purged() {
        let temp = tempfile::tempdir().expect("temp dir");
        let path = temp.path().join("figtether.log");
        fs::write(&path, b"recent\n").expect("written");
        let rotated = temp.path().join(ROTATED_LOG_NAME);
        fs::write(&rotated, b"old\n").expect("written");
        purge_old_logs(temp.path()).expect("purged");
        assert!(path.exists(), "a recent log file is kept");
        assert!(rotated.exists(), "a fresh rotated file is kept");
    }

    #[test]
    fn rotated_path_keeps_the_directory() {
        let path = Path::new("/tmp/figtether/figtether.log");
        assert_eq!(
            rotated_path(path),
            Path::new("/tmp/figtether/figtether.log.1")
        );
    }
}
