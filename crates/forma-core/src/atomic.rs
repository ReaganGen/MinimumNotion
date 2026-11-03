//! Atomic file replacement (ARCHITECTURE §4.4).
//!
//! The new bytes go to a temporary file in the target's directory, are flushed
//! to stable storage, and then replace the target with one `rename(2)`. A crash
//! at any point leaves either the old file or the new one, never a torn file.
//!
//! Phase 2 adds a variant that stages in `.forma/tmp/` for workspace files.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const TEMP_MARKER: &str = ".forma-tmp-";

/// Atomically replaces `target` with `bytes`.
pub fn write_atomic(target: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = parent_dir(target)?;
    let temp = temp_path_for(target)?;

    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        // On Apple platforms, std implements sync_all with F_FULLFSYNC.
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, target)?;
        sync_dir(dir)
    })();

    if result.is_err() {
        // Best effort: the original target is untouched; only the temp file may remain.
        let _ = fs::remove_file(&temp);
    }
    result
}

/// Removes temp files left behind by an interrupted `write_atomic` of `target`.
/// Returns how many were removed.
pub fn remove_stale_temps(target: &Path) -> io::Result<usize> {
    let dir = parent_dir(target)?;
    let prefix = temp_prefix(target)?;
    let mut removed = 0;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with(&prefix) && entry.file_type()?.is_file()
        {
            fs::remove_file(entry.path())?;
            removed += 1;
        }
    }
    Ok(removed)
}

fn parent_dir(target: &Path) -> io::Result<&Path> {
    target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "target has no parent directory",
            )
        })
}

fn temp_prefix(target: &Path) -> io::Result<String> {
    let name = target
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "target has no file name"))?;
    Ok(format!(".{}{TEMP_MARKER}", name.to_string_lossy()))
}

fn temp_path_for(target: &Path) -> io::Result<PathBuf> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let unique = format!(
        "{}{}-{nanos}-{}",
        temp_prefix(target)?,
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    Ok(parent_dir(target)?.join(unique))
}

fn sync_dir(dir: &Path) -> io::Result<()> {
    File::open(dir)?.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_and_replaces_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("settings.json");

        write_atomic(&target, b"first").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"first");

        write_atomic(&target, b"second").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"second");
    }

    #[test]
    fn leaves_no_temp_files_behind() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("settings.json");
        write_atomic(&target, b"data").unwrap();

        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from("settings.json")]);
    }

    #[test]
    #[cfg(unix)]
    fn failed_write_keeps_original_intact() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("settings.json");
        write_atomic(&target, b"original").unwrap();

        // A read-only directory makes creating the temp file fail.
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o500)).unwrap();
        let result = write_atomic(&target, b"replacement");
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();

        assert!(result.is_err());
        assert_eq!(fs::read(&target).unwrap(), b"original");
    }

    #[test]
    fn removes_stale_temps_from_interrupted_writes() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("settings.json");
        write_atomic(&target, b"current").unwrap();
        // Simulate a crash after the temp write but before the rename.
        let stale = temp_path_for(&target).unwrap();
        fs::write(&stale, b"half-written").unwrap();
        let unrelated = dir.path().join("other.json");
        fs::write(&unrelated, b"keep").unwrap();

        assert_eq!(remove_stale_temps(&target).unwrap(), 1);
        assert!(!stale.exists());
        assert!(unrelated.exists());
        assert_eq!(fs::read(&target).unwrap(), b"current");
    }
}
