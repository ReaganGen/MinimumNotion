//! Export, backup and restore (Settings → Data, IMPLEMENTATION_PLAN §10.1).
//!
//! - **Export** copies the workspace's files without `.forma/`: plain
//!   Markdown and attachments any other app can open.
//! - **Backup** zips the whole workspace, `.forma/` included (history, trash,
//!   operations), except temporary files and the lock.
//! - **Restore** unzips a backup into a new folder. It never writes into an
//!   existing folder, refuses entries that would land outside it (`..`,
//!   absolute paths, links), and stops at a size limit.
//!
//! Each writes into a staging name first and renames when complete, so a
//! failure leaves nothing half-made under the final name.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use ts_rs::TS;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::error::{CoreError, CoreResult, io_at};

/// A restore stops past this much unpacked data or this many entries.
pub const MAX_RESTORE_BYTES: u64 = 20 * 1024 * 1024 * 1024;
pub const MAX_RESTORE_ENTRIES: usize = 500_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveSummary {
    pub path: PathBuf,
    pub files: usize,
    pub bytes: u64,
}

/// What an export or backup made, for the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ArchiveResult {
    /// Where it went, to show the user.
    pub path: String,
    pub files: u32,
    #[ts(type = "number")]
    pub bytes: u64,
}

impl From<ArchiveSummary> for ArchiveResult {
    fn from(s: ArchiveSummary) -> Self {
        Self {
            path: s.path.display().to_string(),
            files: u32::try_from(s.files).unwrap_or(u32::MAX),
            bytes: s.bytes,
        }
    }
}

/// Today's local date, for export and backup names.
pub fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Paths under `.forma/` that a backup leaves out.
fn skipped_in_backup(rel: &Path) -> bool {
    let mut parts = rel.components();
    matches!(parts.next(), Some(Component::Normal(first)) if first == ".forma")
        && matches!(parts.next(), Some(Component::Normal(second)) if second == "tmp" || second == "lock")
}

/// Every regular file under `root`, relative, sorted. Links are never followed.
fn files_under(root: &Path, skip: &dyn Fn(&Path) -> bool) -> CoreResult<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        let dir = root.join(&rel);
        for item in fs::read_dir(&dir).map_err(io_at(&dir))? {
            let item = item.map_err(io_at(&dir))?;
            let child = rel.join(item.file_name());
            if skip(&child) {
                continue;
            }
            let kind = item.file_type().map_err(io_at(&item.path()))?;
            if kind.is_dir() {
                stack.push(child);
            } else if kind.is_file() {
                out.push(child);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// `parent/name`, or `parent/name 2`, `name 3`… if taken.
fn unused(parent: &Path, name: &str, extension: &str) -> PathBuf {
    let candidate = |n: u32| {
        let stem = if n == 1 {
            name.to_string()
        } else {
            format!("{name} {n}")
        };
        parent.join(format!("{stem}{extension}"))
    };
    (1..)
        .map(candidate)
        .find(|p| fs::symlink_metadata(p).is_err())
        .expect("some name is free")
}

/// Whether `path` is `root` or inside it. Both are compared as real paths, so
/// a link or another spelling can't hide it; a path that doesn't exist yet
/// is judged by its nearest existing ancestor.
pub fn is_inside(path: &Path, root: &Path) -> bool {
    fn real(path: &Path) -> PathBuf {
        let mut missing = Vec::new();
        let mut existing = path;
        loop {
            if let Ok(mut found) = fs::canonicalize(existing) {
                found.extend(missing.iter().rev());
                return found;
            }
            match (existing.parent(), existing.file_name()) {
                (Some(parent), Some(name)) => {
                    missing.push(name.to_owned());
                    existing = parent;
                }
                _ => return path.to_path_buf(),
            }
        }
    }
    real(path).starts_with(real(root))
}

/// A name safe as one path component.
fn clean_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c == '/' || c == ':' || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_start_matches('.').trim();
    if trimmed.is_empty() {
        "Workspace".into()
    } else {
        trimmed.to_string()
    }
}

/// Copies the workspace's files, without `.forma/`, into a new folder
/// named `name` inside `parent`.
pub fn export_markdown(root: &Path, parent: &Path, name: &str) -> CoreResult<ArchiveSummary> {
    let destination = unused(parent, &clean_name(name), "");
    if is_inside(&destination, root) {
        return Err(CoreError::Invalid(
            "Choose a folder outside the workspace for the export.".into(),
        ));
    }
    let staging = parent.join(format!(".forma-export-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        let files = files_under(root, &|rel| rel == Path::new(".forma"))?;
        let mut bytes = 0;
        for rel in &files {
            let to = staging.join(rel);
            if let Some(dir) = to.parent() {
                fs::create_dir_all(dir).map_err(io_at(dir))?;
            }
            bytes += fs::copy(root.join(rel), &to).map_err(io_at(&to))?;
        }
        fs::create_dir_all(&staging).map_err(io_at(&staging))?;
        fs::rename(&staging, &destination).map_err(io_at(&destination))?;
        Ok(ArchiveSummary {
            path: destination.clone(),
            files: files.len(),
            bytes,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

/// Zips the workspace into `zip_path`, `.forma/` included. Entries sit
/// under one folder named after the workspace.
pub fn backup_zip(root: &Path, zip_path: &Path, folder: &str) -> CoreResult<ArchiveSummary> {
    if is_inside(zip_path, root) {
        return Err(CoreError::Invalid(
            "Save the backup outside the workspace.".into(),
        ));
    }
    let folder = clean_name(folder);
    let partial = zip_path.with_extension("zip.partial");
    let result = (|| {
        let files = files_under(root, &skipped_in_backup)?;
        let file = File::create(&partial).map_err(io_at(&partial))?;
        let mut zip = ZipWriter::new(io::BufWriter::new(file));
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .large_file(true)
            .unix_permissions(0o644);
        let mut bytes = 0;
        let fail = |e: zip::result::ZipError| {
            CoreError::Invalid(format!("The backup couldn't be written: {e}"))
        };
        for rel in &files {
            let name = format!("{folder}/{}", rel.to_string_lossy());
            zip.start_file(name, options).map_err(fail)?;
            let mut source = File::open(root.join(rel)).map_err(io_at(&root.join(rel)))?;
            bytes += io::copy(&mut source, &mut zip).map_err(io_at(&partial))?;
        }
        let mut out = zip.finish().map_err(fail)?;
        out.flush().map_err(io_at(&partial))?;
        out.into_inner()
            .map_err(|e| io_at(&partial)(e.into_error()))?
            .sync_all()
            .map_err(io_at(&partial))?;
        fs::rename(&partial, zip_path).map_err(io_at(zip_path))?;
        Ok(ArchiveSummary {
            path: zip_path.to_path_buf(),
            files: files.len(),
            bytes,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&partial);
    }
    result
}

/// Unzips a backup into a new folder inside `parent` and returns it. The
/// folder is named after the backup's top folder (or the zip file).
pub fn restore_zip(zip_path: &Path, parent: &Path) -> CoreResult<ArchiveSummary> {
    let bad = |why: &str| CoreError::Invalid(format!("This file can't be restored: {why}."));
    let file = File::open(zip_path).map_err(io_at(zip_path))?;
    let mut archive = ZipArchive::new(file).map_err(|_| bad("it isn't a zip file"))?;
    if archive.len() > MAX_RESTORE_ENTRIES {
        return Err(bad("it holds too many files"));
    }

    // Check every entry before writing anything.
    let mut names = Vec::with_capacity(archive.len());
    let mut total = 0u64;
    for i in 0..archive.len() {
        let entry = archive.by_index(i).map_err(|_| bad("it's damaged"))?;
        if entry.is_symlink() {
            return Err(bad("it contains a link"));
        }
        // Forma's backups hold relative names only; anything else is refused,
        // even where the zip library would make it safe.
        let raw = entry
            .name()
            .map_err(|_| bad("an entry's name can't be read"))?;
        if raw.starts_with('/') || raw.starts_with('\\') || raw.contains('\0') {
            return Err(bad("an entry would land outside the folder"));
        }
        let Some(name) = entry.enclosed_name() else {
            return Err(bad("an entry would land outside the folder"));
        };
        if name
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(bad("an entry would land outside the folder"));
        }
        total = total.saturating_add(entry.size());
        if total > MAX_RESTORE_BYTES {
            return Err(bad("it unpacks to more than 20 GB"));
        }
        names.push((name, entry.is_dir()));
    }
    // A Forma backup holds one folder; take its name and unpack its contents.
    // A zip whose only top-level entry is a file has no such folder.
    let tops: std::collections::BTreeSet<_> = names
        .iter()
        .filter_map(|(n, _)| n.components().next().map(|c| c.as_os_str().to_owned()))
        .collect();
    let top_is_folder = names
        .iter()
        .all(|(n, is_dir)| *is_dir || n.components().count() > 1);
    let single_top = (tops.len() == 1 && top_is_folder)
        .then(|| tops.into_iter().next())
        .flatten();
    let strip = |name: &Path| -> PathBuf {
        match &single_top {
            Some(top) => name
                .strip_prefix(top)
                .map(Path::to_path_buf)
                .unwrap_or_default(),
            None => name.to_path_buf(),
        }
    };
    let looks_like_forma = names.iter().any(|(n, _)| {
        let rel = strip(n);
        rel == Path::new(".forma/workspace.json")
            || rel
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("md"))
    });
    if !looks_like_forma {
        return Err(bad("it holds no Markdown pages"));
    }
    let base = match &single_top {
        Some(top) => top.to_string_lossy().into_owned(),
        None => zip_path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
    };
    let destination = unused(parent, &clean_name(&base), "");
    let staging = parent.join(format!(".forma-restore-{}", uuid::Uuid::new_v4()));

    let result = (|| {
        fs::create_dir_all(&staging).map_err(io_at(&staging))?;
        let mut files = 0;
        let mut bytes = 0u64;
        for (i, (name, is_dir)) in names.iter().enumerate() {
            let rel = strip(name);
            if rel.as_os_str().is_empty() {
                continue;
            }
            let to = staging.join(&rel);
            if *is_dir {
                fs::create_dir_all(&to).map_err(io_at(&to))?;
                continue;
            }
            if let Some(dir) = to.parent() {
                fs::create_dir_all(dir).map_err(io_at(dir))?;
            }
            let mut entry = archive.by_index(i).map_err(|_| bad("it's damaged"))?;
            let mut out = File::create(&to).map_err(io_at(&to))?;
            // Never more than the entry claims: a lying header can't fill the disk.
            let size = entry.size();
            let copied = io::copy(&mut (&mut entry).take(size), &mut out)
                .map_err(|_| bad("it's damaged"))?;
            // The zip library checks an entry's CRC when a read reaches its
            // end, so read once more: a damaged backup fails instead of
            // restoring altered files.
            if entry.read(&mut [0u8; 1]).map_err(|_| bad("it's damaged"))? != 0 {
                return Err(bad("an entry is larger than it says"));
            }
            bytes += copied;
            files += 1;
        }
        // A lock from the backed-up session would make the copy read-only.
        let _ = fs::remove_file(staging.join(".forma/lock"));
        fs::rename(&staging, &destination).map_err(io_at(&destination))?;
        Ok(ArchiveSummary {
            path: destination.clone(),
            files,
            bytes,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(dir: &Path) -> Vec<(String, Vec<u8>)> {
        files_under(dir, &|_| false)
            .unwrap()
            .into_iter()
            .map(|rel| (rel.display().to_string(), fs::read(dir.join(&rel)).unwrap()))
            .collect()
    }

    fn sample() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Notes");
        for (path, text) in [
            ("Plan.md", "---\ntitle: Plan\n---\nShip it.\n"),
            ("Projects/index.md", "Projects.\n"),
            ("Projects/_assets/chart.png", "PNG"),
            ("Projects/注意力.md", "中文页面\n"),
            (".forma/workspace.json", "{\"format_version\": 1}\n"),
            (".forma/history/p1/x_external.md", "old\n"),
            (".forma/tmp/staged", "half"),
            (".forma/lock", ""),
        ] {
            let file = root.join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, text).unwrap();
        }
        dir
    }

    #[test]
    fn export_copies_pages_without_forma() {
        let dir = sample();
        let root = dir.path().join("Notes");
        let out = dir.path().join("out");
        fs::create_dir(&out).unwrap();
        let summary = export_markdown(&root, &out, "Notes export").unwrap();
        assert_eq!(summary.path, out.join("Notes export"));
        assert_eq!(summary.files, 4);
        let copied: Vec<String> = tree(&summary.path).into_iter().map(|(p, _)| p).collect();
        assert_eq!(
            copied,
            [
                "Plan.md",
                "Projects/_assets/chart.png",
                "Projects/index.md",
                "Projects/注意力.md"
            ]
        );
        assert_eq!(
            fs::read(summary.path.join("Plan.md")).unwrap(),
            fs::read(root.join("Plan.md")).unwrap()
        );
        // A second export doesn't touch the first.
        let again = export_markdown(&root, &out, "Notes export").unwrap();
        assert_eq!(again.path, out.join("Notes export 2"));
        assert!(export_markdown(&root, &root, "inside").is_err());
        assert!(
            !fs::read_dir(&out).unwrap().any(|e| e
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with('.'))
        );
    }

    #[test]
    fn backup_then_restore_gives_back_every_file() {
        let dir = sample();
        let root = dir.path().join("Notes");
        let zip = dir.path().join("Notes backup.zip");
        let summary = backup_zip(&root, &zip, "Notes").unwrap();
        assert_eq!(summary.files, 6, "everything but .forma/tmp and the lock");
        assert!(!dir.path().join("Notes backup.zip.partial").exists());

        let restore_to = dir.path().join("restored");
        fs::create_dir(&restore_to).unwrap();
        let restored = restore_zip(&zip, &restore_to).unwrap();
        assert_eq!(restored.path, restore_to.join("Notes"));
        let expected: Vec<_> = tree(&root)
            .into_iter()
            .filter(|(p, _)| !p.starts_with(".forma/tmp") && p != ".forma/lock")
            .collect();
        assert_eq!(tree(&restored.path), expected);
        // Restoring again never overwrites: a new folder.
        let second = restore_zip(&zip, &restore_to).unwrap();
        assert_eq!(second.path, restore_to.join("Notes 2"));
        assert!(backup_zip(&root, &root.join("inside.zip"), "Notes").is_err());
    }

    fn zip_with(entries: &[(&str, &[u8])]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("evil.zip");
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        for (name, data) in entries {
            zip.start_file(*name, SimpleFileOptions::default()).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
        (dir, path)
    }

    #[test]
    fn restore_refuses_entries_outside_its_folder() {
        for name in [
            "../escape.md",
            "Notes/../../escape.md",
            "/etc/escape.md",
            "Notes/\0x.md",
        ] {
            let (dir, zip) = zip_with(&[("Notes/ok.md", b"ok"), (name, b"bad")]);
            let target = dir.path().join("target");
            fs::create_dir(&target).unwrap();
            assert!(restore_zip(&zip, &target).is_err(), "{name}");
            assert_eq!(
                fs::read_dir(&target).unwrap().count(),
                0,
                "{name}: nothing written"
            );
            assert!(!dir.path().join("escape.md").exists());
        }
    }

    #[test]
    fn restore_refuses_links_and_other_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("link.zip");
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        zip.start_file("Notes/a.md", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"a").unwrap();
        zip.add_symlink("Notes/link", "/etc/passwd", SimpleFileOptions::default())
            .unwrap();
        zip.finish().unwrap();
        assert!(
            restore_zip(&path, dir.path())
                .unwrap_err()
                .to_string()
                .contains("link")
        );

        let (dir, not_notes) = zip_with(&[("photos/a.jpg", b"x")]);
        assert!(
            restore_zip(&not_notes, dir.path())
                .unwrap_err()
                .to_string()
                .contains("Markdown")
        );
        let garbage = dir.path().join("garbage.zip");
        fs::write(&garbage, "not a zip").unwrap();
        assert!(
            restore_zip(&garbage, dir.path())
                .unwrap_err()
                .to_string()
                .contains("zip")
        );
    }

    #[test]
    fn a_zip_without_one_top_folder_restores_under_its_own_name() {
        let (dir, zip) = zip_with(&[("a.md", b"a"), ("b/c.md", b"c"), (".forma/lock", b"")]);
        let target = dir.path().join("t");
        fs::create_dir(&target).unwrap();
        let restored = restore_zip(&zip, &target).unwrap();
        assert_eq!(restored.path, target.join("evil"));
        assert_eq!(restored.files, 3);
        assert!(
            !restored.path.join(".forma/lock").exists(),
            "the old lock is dropped"
        );

        // One page at the top is a page, not the folder to unpack.
        let (dir, zip) = zip_with(&[("Page.md", b"just one")]);
        let restored = restore_zip(&zip, dir.path()).unwrap();
        assert_eq!(restored.path, dir.path().join("evil"));
        assert_eq!(
            fs::read(restored.path.join("Page.md")).unwrap(),
            b"just one"
        );
    }

    #[test]
    fn a_damaged_backup_restores_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("damaged.zip");
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        zip.start_file("Notes/a.md", stored).unwrap();
        zip.write_all(b"the original text").unwrap();
        zip.finish().unwrap();
        // Flip one byte of the stored text: the sizes still match, the CRC doesn't.
        let mut bytes = fs::read(&path).unwrap();
        let at = bytes
            .windows(8)
            .position(|w| w == b"original")
            .expect("stored text");
        bytes[at] ^= 0x20;
        fs::write(&path, bytes).unwrap();

        let target = dir.path().join("target");
        fs::create_dir(&target).unwrap();
        let err = restore_zip(&path, &target).unwrap_err().to_string();
        assert!(err.contains("damaged"), "{err}");
        assert_eq!(fs::read_dir(&target).unwrap().count(), 0, "nothing left");
    }

    #[test]
    fn a_link_into_the_workspace_counts_as_inside() {
        let dir = sample();
        let root = dir.path().join("Notes");
        let link = dir.path().join("shortcut");
        std::os::unix::fs::symlink(root.join("Projects"), &link).unwrap();
        assert!(is_inside(&link.join("new folder"), &root));
        assert!(is_inside(&root, &root));
        assert!(!is_inside(dir.path(), &root));
        assert!(!is_inside(&dir.path().join("Notes 2"), &root));
        assert!(export_markdown(&root, &link, "Notes export").is_err());
        assert!(backup_zip(&root, &link.join("b.zip"), "Notes").is_err());
        assert!(!root.join("Projects/Notes export").exists());
    }
}
