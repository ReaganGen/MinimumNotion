//! Atomic file replacement (ARCHITECTURE §4.4).
//!
//! The new bytes go to a temporary file, are flushed to stable storage, and
//! then replace the target with one `rename(2)`. A crash at any point leaves
//! either the old file or the new one, never a torn file.
//!
//! Replacing a file this way creates a new inode, so before the rename the
//! temp file receives the target's permissions and extended attributes
//! (Finder tags, comments, quarantine). Birth time and hard links can't be
//! carried over; Forma keeps its own `created_at` in frontmatter.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const TEMP_MARKER: &str = ".forma-tmp-";

/// Atomically replaces `target` with `bytes`, staging next to the target.
pub fn write_atomic(target: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomic_staged(target, bytes, None)
}

/// Atomically replaces `target` with `bytes`, staging the temp file in
/// `staging_dir` when given (it must be on the same volume; workspace files use
/// `.forma/tmp/` so temp files never appear in the user's folders). Falls back
/// to staging next to the target if the rename crosses volumes.
pub fn write_atomic_staged(
    target: &Path,
    bytes: &[u8],
    staging_dir: Option<&Path>,
) -> io::Result<()> {
    let temp = match staging_dir {
        Some(dir) => temp_path_in(dir, target)?,
        None => temp_path_for(target)?,
    };
    match replace_with(target, bytes, &temp) {
        Err(err) if staging_dir.is_some() && err.raw_os_error() == Some(libc_exdev()) => {
            log::warn!("staging dir is on another volume; staging next to {target:?}");
            replace_with(target, bytes, &temp_path_for(target)?)
        }
        result => result,
    }
}

fn replace_with(target: &Path, bytes: &[u8], temp: &Path) -> io::Result<()> {
    let result = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).open(temp)?;
        file.write_all(bytes)?;
        // On Apple platforms, std implements sync_all with F_FULLFSYNC.
        file.sync_all()?;
        drop(file);
        if let Ok(existing) = fs::metadata(target) {
            fs::set_permissions(temp, existing.permissions())?;
            copy_xattrs(target, temp);
        }
        fs::rename(temp, target)
    })();

    if result.is_err() {
        // Best effort: the original target is untouched; only the temp file may remain.
        let _ = fs::remove_file(temp);
        return result;
    }
    // The file is already replaced. A failed directory fsync only weakens
    // durability of the rename itself, so it's logged rather than reported as
    // a failed write (which would make callers think the old content remains).
    if let Err(err) = sync_dir(parent_dir(target)?) {
        log::warn!("fsync of the directory of {target:?} failed: {err}");
    }
    Ok(())
}

/// Copies extended attributes from `from` to `to`. Attributes the system
/// refuses to copy (some are protected) are skipped and logged.
fn copy_xattrs(from: &Path, to: &Path) {
    let names = match xattr::list(from) {
        Ok(names) => names,
        Err(err) => {
            log::debug!("can't list extended attributes of {from:?}: {err}");
            return;
        }
    };
    for name in names {
        match xattr::get(from, &name) {
            Ok(Some(value)) => {
                if let Err(err) = xattr::set(to, &name, &value) {
                    log::debug!("can't copy extended attribute {name:?} to {to:?}: {err}");
                }
            }
            Ok(None) => {}
            Err(err) => log::debug!("can't read extended attribute {name:?}: {err}"),
        }
    }
}

/// `EXDEV`: the rename crossed file systems.
const fn libc_exdev() -> i32 {
    18
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
    temp_path_in(parent_dir(target)?, target)
}

fn temp_path_in(dir: &Path, target: &Path) -> io::Result<PathBuf> {
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
    Ok(dir.join(unique))
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

    #[test]
    #[cfg(unix)]
    fn rewrite_keeps_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("note.md");
        fs::write(&target, b"v1").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();

        write_atomic(&target, b"v2").unwrap();
        let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn rewrite_keeps_extended_attributes() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("note.md");
        fs::write(&target, b"v1").unwrap();
        // The attribute Finder uses for tags.
        let tags = "com.apple.metadata:_kMDItemUserTags";
        xattr::set(&target, tags, b"bplist-tags").unwrap();
        xattr::set(&target, "org.forma.test", b"kept").unwrap();

        write_atomic(&target, b"v2").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"v2");
        assert_eq!(xattr::get(&target, tags).unwrap().unwrap(), b"bplist-tags");
        assert_eq!(
            xattr::get(&target, "org.forma.test").unwrap().unwrap(),
            b"kept"
        );
    }

    #[test]
    fn staged_write_keeps_temp_files_out_of_the_target_folder() {
        let dir = tempfile::tempdir().unwrap();
        let staging = dir.path().join(".forma/tmp");
        fs::create_dir_all(&staging).unwrap();
        let notes = dir.path().join("notes");
        fs::create_dir(&notes).unwrap();
        let target = notes.join("page.md");

        write_atomic_staged(&target, b"hello", Some(&staging)).unwrap();
        write_atomic_staged(&target, b"hello again", Some(&staging)).unwrap();

        assert_eq!(fs::read(&target).unwrap(), b"hello again");
        let in_notes: Vec<_> = fs::read_dir(&notes)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(in_notes, vec![std::ffi::OsString::from("page.md")]);
        assert_eq!(fs::read_dir(&staging).unwrap().count(), 0);
    }
}
