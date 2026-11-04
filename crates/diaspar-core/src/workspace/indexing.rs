//! Keeps the SQLite index (ADR-035) in step with the in-memory index.
//!
//! On open, the cached rows become the "previous" index, so the first scan
//! re-reads only files that changed while Diaspar wasn't running. After every
//! rescan, the rows that changed and the content of the files that were read
//! are written in one transaction.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::error::{CoreError, CoreResult};
use crate::index::{IndexDb, PageContent, PageRow};

use super::scan::{Entry, Index, Scanned, hash_rev, read_facts, scan};
use super::types::{PageKind, PageStatus};
use super::{Workspace, diff};

/// Where a workspace's index lives under the app's index folder: one
/// directory per workspace ID and location, so a moved or copied workspace
/// gets its own cache.
pub(super) fn index_path(index_dir: &Path, workspace_id: &str, root: &Path) -> PathBuf {
    let key = hash_rev(format!("{workspace_id}\n{}", root.display()).as_bytes());
    index_dir.join(key).join("index.db")
}

pub(super) fn open_db(path: Option<&Path>) -> IndexDb {
    if let Some(dir) = path.and_then(Path::parent)
        && let Err(err) = std::fs::create_dir_all(dir)
    {
        log::warn!("can't create the index folder {dir:?} ({err}); using memory");
        return IndexDb::open_or_recreate(None);
    }
    IndexDb::open_or_recreate(path)
}

fn kind_name(kind: PageKind) -> &'static str {
    match kind {
        PageKind::Leaf => "leaf",
        PageKind::Bundle => "bundle",
        PageKind::Folder => "folder",
    }
}

fn to_nanos(time: SystemTime) -> Option<i64> {
    i64::try_from(time.duration_since(UNIX_EPOCH).ok()?.as_nanos()).ok()
}

fn from_nanos(nanos: i64) -> Option<SystemTime> {
    Some(UNIX_EPOCH + Duration::from_nanos(u64::try_from(nanos).ok()?))
}

fn row_for(root: &Path, entry: &Entry) -> PageRow {
    let (status, status_detail) = match &entry.status {
        PageStatus::Ok => ("ok", None),
        PageStatus::MetadataReadOnly => ("metadata_readonly", None),
        PageStatus::InvalidFrontmatter { message } => {
            ("invalid_frontmatter", Some(message.clone()))
        }
    };
    PageRow {
        id: entry.id.clone(),
        rel_path: entry
            .location()
            .strip_prefix(root)
            .unwrap_or(entry.location())
            .to_string_lossy()
            .into_owned(),
        parent_id: entry.parent.clone(),
        kind: kind_name(entry.kind).into(),
        title: entry.title.clone(),
        icon: entry.icon.clone(),
        rev: entry.rev.clone(),
        size: entry
            .signature
            .and_then(|(size, _)| i64::try_from(size).ok()),
        mtime_ns: entry.signature.and_then(|(_, time)| to_nanos(time)),
        id_persisted: entry.id_persisted,
        status: status.into(),
        status_detail,
        is_collection: entry.is_collection,
    }
}

fn entry_from_row(root: &Path, row: PageRow) -> Option<Entry> {
    let location = root.join(&row.rel_path);
    let (kind, file, dir) = match row.kind.as_str() {
        "leaf" => (PageKind::Leaf, Some(location), None),
        "bundle" => (
            PageKind::Bundle,
            Some(location.join("index.md")),
            Some(location),
        ),
        "folder" => (PageKind::Folder, None, Some(location)),
        _ => return None,
    };
    let status = match row.status.as_str() {
        "metadata_readonly" => PageStatus::MetadataReadOnly,
        "invalid_frontmatter" => PageStatus::InvalidFrontmatter {
            message: row.status_detail.unwrap_or_default(),
        },
        _ => PageStatus::Ok,
    };
    let signature = match (row.size, row.mtime_ns.and_then(from_nanos)) {
        (Some(size), Some(time)) => Some((u64::try_from(size).ok()?, time)),
        _ => None,
    };
    Some(Entry {
        id: row.id,
        id_persisted: row.id_persisted,
        kind,
        file,
        dir,
        parent: row.parent_id,
        title: row.title,
        icon: row.icon,
        status,
        signature,
        rev: row.rev,
        is_collection: row.is_collection,
    })
}

/// The index the cache describes, used as the previous index of the first scan.
pub(super) fn cached_index(root: &Path, db: &IndexDb) -> Index {
    let mut rows = match db.rows() {
        Ok(rows) => rows,
        Err(err) => {
            log::warn!("can't read the index cache: {err}");
            return Index::default();
        }
    };
    rows.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    let mut index = Index::default();
    for row in rows {
        if let Some(entry) = entry_from_row(root, row) {
            index
                .children
                .entry(entry.parent.clone())
                .or_default()
                .push(entry.id.clone());
            index.entries.insert(entry.id.clone(), entry);
        }
    }
    index
}

fn remove_db_files(path: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
    }
}

type Changes = (Vec<PageRow>, Vec<(String, PageContent)>);

impl Workspace {
    /// Writes what changed between `old` and the current index to SQLite.
    pub(super) fn sync_index(
        &mut self,
        old: &Index,
        fresh: HashMap<PathBuf, PageContent>,
        removed: &[String],
    ) {
        let (rows, contents) = self.index_changes(old, fresh);
        if rows.is_empty() && contents.is_empty() && removed.is_empty() {
            return;
        }
        if let Err(err) = self.db.apply(removed, &rows, &contents) {
            log::warn!("couldn't update the search index: {err}");
        }
    }

    /// Indexes one page that Diaspar just wrote, without a rescan.
    pub(super) fn index_page(&mut self, id: &str, content: PageContent) {
        let Some(entry) = self.index.get(id) else {
            return;
        };
        let row = row_for(&self.root, entry);
        if let Err(err) = self.db.apply(&[], &[row], &[(id.to_string(), content)]) {
            log::warn!("couldn't update the search index: {err}");
        }
    }

    /// Rebuilds the index from the files alone (Settings → Rebuild search
    /// index). A new database is filled beside the current one and replaces
    /// it only when complete. Page IDs that exist only in memory are kept.
    pub fn rebuild_index(&mut self) -> CoreResult<()> {
        let side = self
            .db_path
            .as_ref()
            .map(|p| p.with_file_name("index.rebuild.db"));
        if let Some(side) = &side {
            remove_db_files(side);
        }
        let mut previous = self.index.clone();
        for entry in previous.entries.values_mut() {
            // Forces every file to be read again.
            entry.signature = None;
        }
        let Scanned { index, fresh, .. } = scan(&self.root, &previous, None);
        let changes = diff(&self.index, &index);
        let failed = |err: rusqlite::Error| {
            CoreError::Invalid(format!("The search index couldn't be rebuilt: {err}"))
        };
        let mut built = IndexDb::open(side.as_deref()).map_err(failed)?;
        self.index = index;
        let (rows, contents) = self.index_changes(&Index::default(), fresh);
        built.apply(&[], &rows, &contents).map_err(failed)?;

        match (&self.db_path, side) {
            (Some(path), Some(side)) => {
                // Close both databases, then put the new file in place.
                drop(built);
                self.db = IndexDb::open_or_recreate(None);
                remove_db_files(path);
                if let Err(err) = std::fs::rename(&side, path) {
                    log::warn!("couldn't move the rebuilt index into place: {err}");
                }
                self.db = open_db(Some(path));
                if self.db.page_count().unwrap_or(0) != i64::try_from(rows.len()).unwrap_or(-1) {
                    // The swap failed: index into the new database the normal way.
                    let (rows, contents) = self.index_changes(&Index::default(), HashMap::new());
                    let _ = self.db.apply(&[], &rows, &contents);
                }
            }
            _ => self.db = built,
        }
        self.pending.content_changed.extend(changes.content_changed);
        self.pending.removed.extend(changes.removed);
        self.pending.tree_changed |= changes.tree_changed;
        log::info!("rebuilt the search index ({} pages)", rows.len());
        Ok(())
    }

    /// Rows that differ from `old`, and the content of files that were read
    /// (or that are new to the index).
    fn index_changes(&self, old: &Index, mut fresh: HashMap<PathBuf, PageContent>) -> Changes {
        let mut rows = Vec::new();
        let mut contents = Vec::new();
        for entry in self.index.entries.values() {
            let row = row_for(&self.root, entry);
            let before = old.get(&entry.id).map(|e| row_for(&self.root, e));
            let is_new = before.is_none();
            let title_changed = before.as_ref().is_some_and(|b| b.title != row.title);
            if before.as_ref() != Some(&row) {
                rows.push(row);
            }
            let content = match entry.file.as_ref().and_then(|f| fresh.remove(f)) {
                Some(content) => Some(content),
                // New to the index without a read (a folder page, or a page whose
                // ID changed): index what there is.
                None if is_new || title_changed => Some(
                    entry
                        .file
                        .as_deref()
                        .and_then(|f| read_facts(f).ok())
                        .map(|facts| facts.content)
                        .unwrap_or_default(),
                ),
                None => None,
            };
            if let Some(content) = content {
                contents.push((entry.id.clone(), content));
            }
        }
        (rows, contents)
    }
}
