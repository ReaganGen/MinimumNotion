//! Revision snapshots in `.forma/history/<page-id>/` (DATA_MODEL §8.2).
//! Each snapshot is a full copy of the file, readable without Forma.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, NaiveDateTime, Utc};

use crate::atomic::write_atomic_staged;
use crate::error::{CoreResult, io_at};

use super::scan::hash_rev;

const STAMP: &str = "%Y%m%dT%H%M%SZ";
/// Snapshots younger than this are never pruned.
const KEEP_ALL_DAYS: i64 = 30;

/// Writes a snapshot of `text` and prunes old ones.
pub fn snapshot(
    history_root: &Path,
    page_id: &str,
    text: &str,
    reason: &str,
    staging: &Path,
) -> CoreResult<PathBuf> {
    let dir = history_root.join(page_id);
    fs::create_dir_all(&dir).map_err(io_at(&dir))?;
    let name = format!(
        "{}_{}_{reason}.md",
        Utc::now().format(STAMP),
        hash_rev(text.as_bytes())
    );
    let path = dir.join(name);
    write_atomic_staged(&path, text.as_bytes(), Some(staging)).map_err(io_at(&path))?;
    prune(&dir, Utc::now());
    Ok(path)
}

fn taken_at(file_name: &str) -> Option<DateTime<Utc>> {
    let stamp = file_name.split('_').next()?;
    NaiveDateTime::parse_from_str(stamp, STAMP)
        .ok()
        .map(|t| t.and_utc())
}

/// Keeps every snapshot from the last 30 days, and the newest one per day
/// before that.
pub fn prune(dir: &Path, now: DateTime<Utc>) {
    let Ok(read) = fs::read_dir(dir) else { return };
    let mut names: Vec<String> = read
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".md"))
        .collect();
    // Timestamp prefixes sort chronologically; newest first.
    names.sort_unstable_by(|a, b| b.cmp(a));
    let cutoff = now - Duration::days(KEEP_ALL_DAYS);
    let mut kept_days = std::collections::HashSet::new();
    for name in names {
        let Some(when) = taken_at(&name) else {
            continue;
        };
        if when >= cutoff {
            continue;
        }
        if !kept_days.insert(when.date_naive()) {
            let path = dir.join(&name);
            if let Err(err) = fs::remove_file(&path) {
                log::warn!("couldn't prune snapshot {path:?}: {err}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_readable_snapshots() {
        let root = tempfile::tempdir().unwrap();
        let staging = root.path().join("tmp");
        fs::create_dir(&staging).unwrap();
        snapshot(
            root.path(),
            "p1",
            "---\ntitle: A\n---\nBody\n",
            "external",
            &staging,
        )
        .unwrap();
        let files: Vec<_> = fs::read_dir(root.path().join("p1"))
            .unwrap()
            .flatten()
            .collect();
        assert_eq!(files.len(), 1);
        let name = files[0].file_name().to_string_lossy().into_owned();
        assert!(name.ends_with("_external.md"), "{name}");
        assert_eq!(
            fs::read_to_string(files[0].path()).unwrap(),
            "---\ntitle: A\n---\nBody\n"
        );
    }

    #[test]
    fn prunes_to_one_per_day_after_thirty_days() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "20260101T080000Z_aaaa_autosave.md",
            "20260101T090000Z_bbbb_autosave.md", // same old day: only the newest stays
            "20260102T080000Z_cccc_autosave.md",
            "20261101T080000Z_dddd_autosave.md", // recent: kept
            "20261101T090000Z_eeee_autosave.md", // recent: kept
        ] {
            fs::write(dir.path().join(name), "x").unwrap();
        }
        let now = "2026-11-02T00:00:00Z".parse().unwrap();
        prune(dir.path(), now);
        let mut left: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(
            left,
            [
                "20260101T090000Z_bbbb_autosave.md",
                "20260102T080000Z_cccc_autosave.md",
                "20261101T080000Z_dddd_autosave.md",
                "20261101T090000Z_eeee_autosave.md",
            ]
        );
    }
}
