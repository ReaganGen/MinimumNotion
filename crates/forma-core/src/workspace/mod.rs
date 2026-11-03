//! The Document Service (ARCHITECTURE §4): the only code that writes
//! workspace files. Callers serialize access (one writer at a time) by
//! holding a `Workspace` behind a mutex.

pub mod collections;
mod history;
mod indexing;
mod journal;
mod links;
pub mod naming;
pub mod read;
mod refs;
pub mod retrieval;
mod scan;
mod search;
pub use search::is_cjk;
mod state;
pub mod types;
pub mod watcher;

#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::atomic::write_atomic_staged;
use crate::error::{CoreError, CoreResult, io_at};
use crate::frontmatter::{self, Change, Parsed, Value};
use crate::index::IndexDb;

use naming::{name_key, sanitize_stem, taken_stems, unique_stem};
use scan::{Entry, Index, Scanned, hash_rev, natural_key, new_id, scan, signature};
use state::{FORMAT_VERSION, TreeState, WorkspaceMeta, load_json, order_key, save_json};
use types::{
    PageDocument, PageKind, PageRef, PageStatus, PageSummary, RecoveryBuffer, SaveOutcome,
    TrashEntryInfo, WorkspaceChanges, WorkspaceInfo,
};

/// An autosave snapshot is taken at most this often per page (ARCHITECTURE §4.6).
const AUTOSAVE_SNAPSHOT_INTERVAL: Duration = Duration::from_secs(10 * 60);

pub struct Workspace {
    root: PathBuf,
    meta: WorkspaceMeta,
    tree: TreeState,
    index: Index,
    /// The SQLite cache: search, links, frontmatter (ADR-035).
    db: IndexDb,
    /// Where `db` lives on disk; `None` keeps it in memory.
    db_path: Option<PathBuf>,
    /// Held for the life of the workspace; dropping it releases the flock.
    _lock: Option<File>,
    read_only_reason: Option<String>,
    notices: Vec<String>,
    /// Changes not yet reported to the UI.
    pending: WorkspaceChanges,
    /// The last revision this session wrote, per page. Content on disk with
    /// any other revision was produced outside this session.
    written: HashMap<String, String>,
    last_autosave_snapshot: HashMap<String, SystemTime>,
    /// Tests stop a batch after this many files, as if Forma had crashed.
    #[cfg(test)]
    crash_after_steps: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TrashRecord {
    format_version: u32,
    entry_id: String,
    trashed_at: String,
    root_page_id: Option<String>,
    title: String,
    original_parent_id: Option<String>,
    original_rel_path: String,
    page_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RecoveryRecord {
    page_id: String,
    title: String,
    saved_at: String,
    reason: String,
}

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn clean_title(title: &str) -> String {
    let single_line = title.replace(['\n', '\r', '\t'], " ");
    let trimmed = single_line.trim();
    if trimmed.is_empty() {
        "Untitled".into()
    } else {
        trimmed.to_string()
    }
}

fn validate_uuid(id: &str, what: &str) -> CoreResult<()> {
    uuid::Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| CoreError::Invalid(format!("{what} {id:?} isn't a valid ID")))
}

impl Workspace {
    /// Creates `root` if needed and opens it as a workspace.
    pub fn create(root: &Path) -> CoreResult<Self> {
        fs::create_dir_all(root).map_err(io_at(root))?;
        Self::open(root)
    }

    /// Opens `root` as a workspace. Writes only inside `.forma/`.
    /// Opens a workspace with its index in memory (tests, and fallbacks).
    pub fn open(root: &Path) -> CoreResult<Self> {
        Self::open_with(root, None)
    }

    /// Opens a workspace whose index is cached under `index_dir` (the app's
    /// Application Support folder). A read-only workspace keeps it in memory,
    /// so two windows never write the same cache.
    pub fn open_with(root: &Path, index_dir: Option<&Path>) -> CoreResult<Self> {
        let root = fs::canonicalize(root).map_err(io_at(root))?;
        if !root.is_dir() {
            return Err(CoreError::Invalid(format!(
                "{} isn't a folder.",
                root.display()
            )));
        }
        refuse_unsuitable_folder(&root)?;

        let forma = root.join(".forma");
        for sub in ["tmp", "history", "trash", "recovery", "journal"] {
            let dir = forma.join(sub);
            fs::create_dir_all(&dir).map_err(io_at(&dir))?;
        }
        let mut notices = Vec::new();
        let mut read_only_reason = None;

        let meta_path = forma.join("workspace.json");
        let meta = match load_json::<WorkspaceMeta>(&meta_path) {
            Ok(Some(meta)) => meta,
            Ok(None) => {
                let meta = WorkspaceMeta {
                    format_version: FORMAT_VERSION,
                    id: new_id(),
                    name: root
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "Workspace".into()),
                    created_at: now_rfc3339(),
                    created_by: format!("Forma {}", env!("CARGO_PKG_VERSION")),
                };
                save_json(&meta_path, &meta, &forma.join("tmp"))?;
                meta
            }
            Err(err) => {
                read_only_reason = Some(format!(
                    "The workspace file .forma/workspace.json can't be read ({err}), so Forma opened this workspace read-only."
                ));
                WorkspaceMeta {
                    format_version: FORMAT_VERSION,
                    id: new_id(),
                    name: "Workspace".into(),
                    created_at: now_rfc3339(),
                    created_by: String::new(),
                }
            }
        };
        if meta.format_version > FORMAT_VERSION {
            read_only_reason = Some(
                "This workspace was created by a newer version of Forma, so it's open read-only."
                    .into(),
            );
        }

        let lock_path = forma.join("lock");
        let lock = File::options()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
            .map_err(io_at(&lock_path))?;
        let lock = match lock.try_lock() {
            Ok(()) => Some(lock),
            Err(_) => {
                read_only_reason.get_or_insert_with(|| {
                    "This workspace is open in another Forma window, so it's read-only here.".into()
                });
                None
            }
        };

        let tree = match load_json::<TreeState>(&forma.join("tree.json")) {
            Ok(tree) => tree.unwrap_or_default(),
            Err(err) => {
                notices.push(format!(
                    "The page order and favorites file couldn't be read ({err}). Pages are sorted by title until you reorder them."
                ));
                TreeState::default()
            }
        };

        let db_path = index_dir
            .filter(|_| read_only_reason.is_none())
            .map(|dir| indexing::index_path(dir, &meta.id, &root));
        let db = indexing::open_db(db_path.as_deref());
        let index = indexing::cached_index(&root, &db);
        let mut workspace = Self {
            root,
            meta,
            tree,
            index,
            db,
            db_path,
            _lock: lock,
            read_only_reason,
            notices,
            pending: WorkspaceChanges::default(),
            written: HashMap::new(),
            last_autosave_snapshot: HashMap::new(),
            #[cfg(test)]
            crash_after_steps: None,
        };
        if workspace.read_only_reason.is_none() {
            workspace.recover_after_crash();
        }
        workspace.rescan(None);
        workspace.pending = WorkspaceChanges::default();
        Ok(workspace)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn forma(&self) -> PathBuf {
        self.root.join(".forma")
    }

    fn staging(&self) -> PathBuf {
        self.forma().join("tmp")
    }

    /// Cleans up after an interrupted session (ARCHITECTURE §4.10).
    fn recover_after_crash(&mut self) {
        // Batches that stopped halfway are undone first (DATA_MODEL §7.6).
        self.roll_back_journals();
        // Leftover temp files from interrupted atomic writes.
        if let Ok(read) = fs::read_dir(self.staging()) {
            for item in read.flatten() {
                let path = item.path();
                let removed = if path.is_dir() {
                    fs::remove_dir_all(&path)
                } else {
                    fs::remove_file(&path)
                };
                if let Err(err) = removed {
                    log::warn!("couldn't remove leftover temp file {path:?}: {err}");
                }
            }
        }
        // Trash entries whose page never arrived (crash between the two steps).
        if let Ok(read) = fs::read_dir(self.forma().join("trash")) {
            for item in read.flatten() {
                let dir = item.path();
                let only_record = fs::read_dir(&dir)
                    .map(|r| r.flatten().all(|e| e.file_name() == "entry.json"))
                    .unwrap_or(false);
                if only_record {
                    let _ = fs::remove_dir_all(&dir);
                }
            }
        }
        // An interrupted leaf-to-bundle conversion leaves an empty `Name/` beside `Name.md`.
        remove_empty_conversion_dirs(&self.root);
    }

    // ---------------------------------------------------------------- reads

    pub fn info(&self) -> WorkspaceInfo {
        WorkspaceInfo {
            root: self.root.display().to_string(),
            name: self.meta.name.clone(),
            id: self.meta.id.clone(),
            read_only_reason: self.read_only_reason.clone(),
            notices: self.notices.clone(),
        }
    }

    pub fn take_notices(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notices)
    }

    /// Changes since the last call, for the UI.
    pub fn take_changes(&mut self) -> WorkspaceChanges {
        std::mem::take(&mut self.pending)
    }

    fn entry(&self, id: &str) -> CoreResult<&Entry> {
        self.index
            .get(id)
            .ok_or_else(|| CoreError::NotFound("This page".into()))
    }

    fn summary(&self, entry: &Entry) -> PageSummary {
        PageSummary {
            id: entry.id.clone(),
            title: entry.title.clone(),
            icon: entry.icon.clone(),
            kind: entry.kind,
            has_children: !self.index.children_of(Some(&entry.id)).is_empty(),
            favorite: self.tree.favorites.contains(&entry.id),
            status: entry.status.clone(),
            is_collection: entry.is_collection,
        }
    }

    /// Children in display order: the saved order first, then by title.
    fn sorted_children(&self, parent: Option<&str>) -> Vec<String> {
        let mut ids: Vec<String> = self.index.children_of(parent).to_vec();
        let order = self.tree.order.get(&order_key(parent));
        let position = |id: &String| order.and_then(|o| o.iter().position(|x| x == id));
        ids.sort_by(|a, b| {
            let (pa, pb) = (position(a), position(b));
            match (pa, pb) {
                (Some(x), Some(y)) => x.cmp(&y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => {
                    let (ea, eb) = (&self.index.entries[a], &self.index.entries[b]);
                    natural_key(&ea.title)
                        .cmp(&natural_key(&eb.title))
                        .then(a.cmp(b))
                }
            }
        });
        ids
    }

    pub fn list_children(&self, parent: Option<&str>) -> CoreResult<Vec<PageSummary>> {
        if let Some(parent) = parent {
            self.entry(parent)?;
        }
        Ok(self
            .sorted_children(parent)
            .iter()
            .map(|id| self.summary(&self.index.entries[id]))
            .collect())
    }

    pub fn list_favorites(&self) -> Vec<PageSummary> {
        self.tree
            .favorites
            .iter()
            .filter_map(|id| self.index.get(id))
            .map(|e| self.summary(e))
            .collect()
    }

    pub fn ancestors(&self, id: &str) -> Vec<PageRef> {
        let mut chain = Vec::new();
        let mut cursor = self.index.get(id).and_then(|e| e.parent.clone());
        while let Some(current) = cursor {
            let Some(entry) = self.index.get(&current) else {
                break;
            };
            chain.push(PageRef {
                id: entry.id.clone(),
                title: entry.title.clone(),
                icon: entry.icon.clone(),
            });
            cursor = entry.parent.clone();
        }
        chain.reverse();
        chain
    }

    /// Reads a page fresh from disk. Never writes.
    pub fn get_page(&self, id: &str) -> CoreResult<PageDocument> {
        let entry = self.entry(id)?;
        let (fields, body, rev) = match &entry.file {
            Some(file) => {
                let bytes = fs::read(file).map_err(io_at(file))?;
                let rev = hash_rev(&bytes);
                let text = String::from_utf8_lossy(&bytes).into_owned();
                let split = frontmatter::split(&text);
                let fields = match split.yaml.map(frontmatter::parse) {
                    Some(Parsed::Editable { fields, .. } | Parsed::ReadOnly { fields }) => fields,
                    _ => Default::default(),
                };
                (fields, split.body.to_string(), Some(rev))
            }
            None => (Default::default(), String::new(), None),
        };
        let body_rev = hash_rev(body.as_bytes());
        Ok(PageDocument {
            id: entry.id.clone(),
            parent_id: entry.parent.clone(),
            title: entry.title.clone(),
            icon: entry.icon.clone(),
            tags: fields.tags,
            created_at: fields.created_at,
            updated_at: fields.updated_at,
            body,
            rev,
            body_rev,
            kind: entry.kind,
            status: entry.status.clone(),
            favorite: self.tree.favorites.contains(&entry.id),
            ancestors: self.ancestors(id),
            is_collection: entry.is_collection,
            collection_id: entry
                .parent
                .clone()
                .filter(|p| self.index.get(p).is_some_and(|e| e.is_collection)),
        })
    }

    // -------------------------------------------------------- write helpers

    fn writable(&self) -> CoreResult<()> {
        match &self.read_only_reason {
            Some(reason) => Err(CoreError::ReadOnly(reason.clone())),
            None => Ok(()),
        }
    }

    fn rescan(&mut self, remap: Option<(PathBuf, PathBuf)>) {
        let Scanned {
            index,
            notices,
            fresh,
        } = scan(&self.root, &self.index, remap);
        let changes = diff(&self.index, &index);
        let old = std::mem::replace(&mut self.index, index);
        self.sync_index(&old, fresh, &changes.removed);
        for notice in notices {
            if !self.notices.contains(&notice) {
                self.notices.push(notice);
            }
        }
        self.pending.content_changed.extend(changes.content_changed);
        self.pending.removed.extend(changes.removed);
        self.pending.tree_changed |= changes.tree_changed;
    }

    /// Re-reads the workspace after changes made outside Forma.
    pub fn reconcile(&mut self) {
        self.rescan(None);
    }

    /// Writes `text` to a page's file atomically, snapshotting the previous
    /// content first when it came from outside this session, when the last
    /// autosave snapshot is old, or when `force_reason` is given.
    fn write_page_file(
        &mut self,
        id: &str,
        file: &Path,
        text: &str,
        previous: Option<(&str, &str)>,
        force_reason: Option<&str>,
    ) -> CoreResult<String> {
        if let Some((old_text, old_rev)) = previous {
            let now = SystemTime::now();
            let reason = if force_reason.is_some() {
                force_reason
            } else if self.written.get(id).map(String::as_str) != Some(old_rev) {
                Some("external")
            } else if self.last_autosave_snapshot.get(id).is_none_or(|t| {
                now.duration_since(*t).unwrap_or_default() >= AUTOSAVE_SNAPSHOT_INTERVAL
            }) {
                Some("autosave")
            } else {
                None
            };
            if let Some(reason) = reason {
                history::snapshot(
                    &self.forma().join("history"),
                    id,
                    old_text,
                    reason,
                    &self.staging(),
                )?;
                if reason == "autosave" {
                    self.last_autosave_snapshot.insert(id.to_string(), now);
                }
            }
        }
        write_atomic_staged(file, text.as_bytes(), Some(&self.staging())).map_err(io_at(file))?;
        let rev = hash_rev(text.as_bytes());
        self.written.insert(id.to_string(), rev.clone());
        if let Some(entry) = self.index.entries.get_mut(id) {
            let facts = scan::facts_from_bytes(text.as_bytes());
            entry.rev = Some(rev.clone());
            entry.signature = signature(file);
            entry.is_collection = facts.is_collection;
            entry.status = facts.status;
            self.index_page(id, facts.content);
        }
        Ok(rev)
    }

    /// Applies frontmatter `changes` (in canonical key order) to `text`,
    /// replacing the body when given. With metadata that can't be edited
    /// safely, only the body changes; `need_edit` makes that an error instead.
    fn rewrite(
        text: &str,
        changes: &[(&'static str, Value)],
        body: Option<&str>,
        need_edit: bool,
    ) -> CoreResult<String> {
        let split = frontmatter::split(text);
        let body = body.unwrap_or(split.body);
        let entries: Vec<Change<'_>> = changes
            .iter()
            .map(|(key, value)| Change {
                key,
                entry: Some(frontmatter::emit_entry(key, value)),
            })
            .collect();
        match split.yaml {
            None => {
                let yaml: String = entries.iter().filter_map(|c| c.entry.clone()).collect();
                Ok(frontmatter::assemble(&yaml, body))
            }
            Some(yaml) => match frontmatter::parse(yaml) {
                Parsed::Editable { spans, .. } => {
                    Ok(frontmatter::assemble(&frontmatter::edit(yaml, &spans, &entries), body))
                }
                Parsed::ReadOnly { .. } if !need_edit => Ok(frontmatter::assemble(yaml, body)),
                Parsed::ReadOnly { .. } => Err(CoreError::ReadOnly(
                    "This page's frontmatter is written in a way Forma can't edit safely. Edit it in a text editor first.".into(),
                )),
                Parsed::Invalid { message } => Err(CoreError::ReadOnly(format!(
                    "This page's frontmatter can't be read ({message}). Fix it in a text editor to edit the page in Forma."
                ))),
            },
        }
    }

    /// Frontmatter keys every Forma write fills in when missing.
    fn standard_changes(
        entry: &Entry,
        file: &Path,
        text: &str,
        title: Option<String>,
    ) -> Vec<(&'static str, Value)> {
        let fields = match frontmatter::split(text).yaml.map(frontmatter::parse) {
            Some(Parsed::Editable { fields, .. } | Parsed::ReadOnly { fields }) => fields,
            _ => Default::default(),
        };
        let mut changes = Vec::new();
        if !entry.id_persisted || fields.id.as_deref() != Some(entry.id.as_str()) {
            changes.push(("id", Value::Plain(entry.id.clone())));
        }
        match title {
            Some(title) => changes.push(("title", Value::Str(title))),
            None if fields.title.is_none() => {
                changes.push(("title", Value::Str(entry.title.clone())))
            }
            None => {}
        }
        if fields.created_at.is_none() {
            let born = fs::metadata(file)
                .and_then(|m| m.created())
                .map(|t| DateTime::<Utc>::from(t).to_rfc3339_opts(SecondsFormat::Secs, true))
                .unwrap_or_else(|_| now_rfc3339());
            changes.push(("created_at", Value::Plain(born)));
        }
        changes.push(("updated_at", Value::Plain(now_rfc3339())));
        changes
    }

    fn new_page_text(id: &str, title: &str, body: &str) -> String {
        let now = now_rfc3339();
        let yaml = [
            frontmatter::emit_entry("id", &Value::Plain(id.into())),
            frontmatter::emit_entry("title", &Value::Str(title.into())),
            frontmatter::emit_entry("created_at", &Value::Plain(now.clone())),
            frontmatter::emit_entry("updated_at", &Value::Plain(now)),
        ]
        .concat();
        frontmatter::assemble(&yaml, body)
    }

    /// Writes the page's `id` into its file if it only exists in memory
    /// (ADR-005). Called before anything durable refers to the page.
    pub fn ensure_persisted_id(&mut self, id: &str) -> CoreResult<()> {
        self.writable()?;
        let entry = self.entry(id)?.clone();
        if entry.id_persisted {
            return Ok(());
        }
        match &entry.file {
            None => {
                // A folder page gets an index.md, becoming a bundle.
                let dir = entry.dir.clone().expect("folder pages have a directory");
                let file = dir.join("index.md");
                self.write_page_file(
                    id,
                    &file,
                    &Self::new_page_text(id, &entry.title, ""),
                    None,
                    None,
                )?;
            }
            Some(file) => {
                let bytes = fs::read(file).map_err(io_at(file))?;
                let text = String::from_utf8_lossy(&bytes).into_owned();
                let new_text =
                    Self::rewrite(&text, &[("id", Value::Plain(id.into()))], None, true)?;
                self.write_page_file(id, file, &new_text, Some((&text, &hash_rev(&bytes))), None)?;
            }
        }
        self.rescan(None);
        Ok(())
    }

    // ------------------------------------------------------------ mutations

    /// Saves a page's body. `base_body_rev` is the body revision the editor
    /// started from (ADR-033): if the body on disk changed since, nothing is
    /// written. Frontmatter changed meanwhile (title, tags, other keys) is
    /// kept, because the save edits the frontmatter that's on disk now.
    pub fn save_page(
        &mut self,
        id: &str,
        base_body_rev: Option<&str>,
        body: &str,
    ) -> CoreResult<SaveOutcome> {
        self.save_page_inner(id, base_body_rev, body, None)
    }

    /// "Keep mine" after a conflict: snapshots what's on disk, then saves.
    pub fn save_page_over_conflict(&mut self, id: &str, body: &str) -> CoreResult<SaveOutcome> {
        let current = self.get_page(id)?.body_rev;
        self.save_page_inner(id, Some(&current), body, Some("conflict"))
    }

    fn save_page_inner(
        &mut self,
        id: &str,
        base_body_rev: Option<&str>,
        body: &str,
        force_reason: Option<&str>,
    ) -> CoreResult<SaveOutcome> {
        self.writable()?;
        let entry = self.entry(id)?.clone();
        if let PageStatus::InvalidFrontmatter { message } = &entry.status {
            return Err(CoreError::ReadOnly(format!(
                "This page's frontmatter can't be read ({message}). Fix it in a text editor to edit the page in Forma."
            )));
        }
        let Some(file) = entry.file.clone() else {
            // A folder page has no file yet; its body is empty.
            let empty = hash_rev(b"");
            if base_body_rev.is_some_and(|base| base != empty) {
                return Ok(SaveOutcome::Conflict {
                    current_body_rev: Some(empty),
                    current_body: Some(String::new()),
                });
            }
            let dir = entry.dir.clone().expect("folder pages have a directory");
            let file = dir.join("index.md");
            let text = Self::new_page_text(id, &entry.title, body);
            let rev = self.write_page_file(id, &file, &text, None, None)?;
            self.rescan(None);
            return Ok(SaveOutcome::Saved {
                rev,
                body_rev: hash_rev(body.as_bytes()),
                updated_at: now_rfc3339(),
            });
        };

        let bytes = match fs::read(&file) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(SaveOutcome::Conflict {
                    current_body_rev: None,
                    current_body: None,
                });
            }
            Err(e) => return Err(io_at(&file)(e)),
        };
        let current_rev = hash_rev(&bytes);
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let current_body = frontmatter::split(&text).body;
        let current_body_rev = hash_rev(current_body.as_bytes());
        if base_body_rev != Some(current_body_rev.as_str()) {
            return Ok(SaveOutcome::Conflict {
                current_body_rev: Some(current_body_rev),
                current_body: Some(current_body.to_string()),
            });
        }
        let changes = Self::standard_changes(&entry, &file, &text, None);
        let updated_at = match changes.last() {
            Some((_, Value::Plain(at))) => at.clone(),
            _ => now_rfc3339(),
        };
        let new_text = Self::rewrite(&text, &changes, Some(body), false)?;
        let rev = self.write_page_file(
            id,
            &file,
            &new_text,
            Some((&text, &current_rev)),
            force_reason,
        )?;
        if !entry.id_persisted
            && entry.status == PageStatus::Ok
            && let Some(e) = self.index.entries.get_mut(id)
        {
            e.id_persisted = true;
        }
        Ok(SaveOutcome::Saved {
            rev,
            body_rev: hash_rev(body.as_bytes()),
            updated_at,
        })
    }

    /// Changes a page's title and renames its file or folder to match. Only
    /// frontmatter changes, so this never conflicts with body edits (ADR-033).
    pub fn set_title(&mut self, id: &str, title: &str) -> CoreResult<()> {
        self.writable()?;
        let title = clean_title(title);
        if self.entry(id)?.file.is_none() {
            self.ensure_persisted_id(id)?;
        }
        let entry = self.entry(id)?.clone();
        let file = entry.file.clone().expect("ensured above");
        let bytes = fs::read(&file).map_err(io_at(&file))?;
        let current_rev = hash_rev(&bytes);
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let changes = Self::standard_changes(&entry, &file, &text, Some(title.clone()));
        let new_text = Self::rewrite(&text, &changes, None, true)?;
        self.write_page_file(id, &file, &new_text, Some((&text, &current_rev)), None)?;

        // Rename the file (leaf) or folder (bundle) to match the new title.
        let location = entry.location().to_path_buf();
        let parent_dir = location
            .parent()
            .expect("pages live inside the workspace")
            .to_path_buf();
        let old_stem = match entry.kind {
            PageKind::Leaf => location.file_stem(),
            _ => location.file_name(),
        }
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
        let new_stem = unique_stem(
            &sanitize_stem(&title),
            &taken_stems(&parent_dir),
            Some(&old_stem),
        );
        if new_stem != old_stem {
            let new_location = match entry.kind {
                PageKind::Leaf => parent_dir.join(format!("{new_stem}.md")),
                _ => parent_dir.join(&new_stem),
            };
            fs::rename(&location, &new_location).map_err(io_at(&location))?;
            self.rescan(Some((location, new_location)));
        } else {
            self.rescan(None);
        }
        Ok(())
    }

    /// Sets or clears a page's emoji icon.
    pub fn set_icon(&mut self, id: &str, icon: Option<&str>) -> CoreResult<()> {
        self.writable()?;
        self.ensure_persisted_id(id)?;
        let entry = self.entry(id)?.clone();
        let file = entry.file.clone().expect("persisted pages have a file");
        let bytes = fs::read(&file).map_err(io_at(&file))?;
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let split = frontmatter::split(&text);
        let Some(Parsed::Editable { spans, .. }) = split.yaml.map(frontmatter::parse) else {
            return Err(CoreError::ReadOnly(
                "This page's frontmatter can't be edited safely.".into(),
            ));
        };
        let yaml = split.yaml.unwrap_or_default();
        let icon_entry = icon
            .filter(|i| !i.trim().is_empty())
            .map(|i| frontmatter::emit_entry("icon", &Value::Str(i.trim().into())));
        let changes = [
            Change {
                key: "icon",
                entry: icon_entry,
            },
            Change {
                key: "updated_at",
                entry: Some(frontmatter::emit_entry(
                    "updated_at",
                    &Value::Plain(now_rfc3339()),
                )),
            },
        ];
        let new_text =
            frontmatter::assemble(&frontmatter::edit(yaml, &spans, &changes), split.body);
        self.write_page_file(id, &file, &new_text, Some((&text, &hash_rev(&bytes))), None)?;
        self.rescan(None);
        Ok(())
    }

    /// The folder a page's children and attachments live in, converting a
    /// leaf into a bundle when needed.
    fn dir_for_children(&mut self, id: &str) -> CoreResult<PathBuf> {
        let entry = self.entry(id)?.clone();
        match (&entry.kind, &entry.dir) {
            (PageKind::Bundle | PageKind::Folder, Some(dir)) => Ok(dir.clone()),
            _ => self.convert_to_bundle(&entry),
        }
    }

    /// `Name.md` → `Name/index.md` (DATA_MODEL §3.1).
    fn convert_to_bundle(&mut self, entry: &Entry) -> CoreResult<PathBuf> {
        let file = entry.file.clone().expect("leaves have a file");
        let parent = file.parent().expect("pages live inside the workspace");
        let stem = file
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let dir = parent.join(&stem);
        if fs::symlink_metadata(&dir).is_ok() {
            return Err(CoreError::Invalid(format!(
                "A folder named “{stem}” already exists next to this page. Rename one of them first."
            )));
        }
        fs::create_dir(&dir).map_err(io_at(&dir))?;
        let index_file = dir.join("index.md");
        fs::rename(&file, &index_file).map_err(io_at(&file))?;
        // Hand-written relative references now need one more "../".
        let bytes = fs::read(&index_file).map_err(io_at(&index_file))?;
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let split = frontmatter::split(&text);
        if let Some(body) = refs::prefix_relative_refs(split.body, "../") {
            let new_text = match split.yaml {
                Some(yaml) => frontmatter::assemble(yaml, &body),
                None => body,
            };
            self.write_page_file(
                &entry.id,
                &index_file,
                &new_text,
                Some((&text, &hash_rev(&bytes))),
                Some("convert"),
            )?;
        }
        self.rescan(Some((file, dir.clone())));
        Ok(dir)
    }

    /// Creates a new empty page under `parent` (or at the root).
    pub fn create_page(&mut self, parent: Option<&str>, title: &str) -> CoreResult<PageSummary> {
        self.writable()?;
        let title = clean_title(title);
        let dir = match parent {
            Some(parent) => self.dir_for_children(parent)?,
            None => self.root.clone(),
        };
        let stem = unique_stem(&sanitize_stem(&title), &taken_stems(&dir), None);
        let file = dir.join(format!("{stem}.md"));
        let id = new_id();
        self.write_page_file(
            &id,
            &file,
            &Self::new_page_text(&id, &title, ""),
            None,
            None,
        )?;
        let key = order_key(parent);
        if let Some(order) = self.tree.order.get_mut(&key) {
            order.push(id.clone());
            self.save_tree()?;
        }
        self.rescan(None);
        Ok(self.summary(self.entry(&id)?))
    }

    /// Moves a page (with its sub-pages) under `new_parent`, or to the root.
    pub fn move_page(
        &mut self,
        id: &str,
        new_parent: Option<&str>,
        position: Option<usize>,
    ) -> CoreResult<()> {
        self.writable()?;
        let old_parent = self.entry(id)?.parent.clone();
        if let Some(target) = new_parent {
            self.entry(target)?;
            if self.index.is_self_or_descendant(id, target) {
                return Err(CoreError::Invalid(
                    "A page can't be moved into itself or into one of its sub-pages.".into(),
                ));
            }
        }
        if old_parent.as_deref() == new_parent {
            return match position {
                Some(position) => self.reorder(id, position),
                None => Ok(()),
            };
        }
        self.ensure_persisted_id(id)?;
        let target_dir = match new_parent {
            Some(parent) => self.dir_for_children(parent)?,
            None => self.root.clone(),
        };
        let entry = self.entry(id)?.clone();
        let location = entry.location().to_path_buf();
        let name = location
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (stem, extension) = match entry.kind {
            PageKind::Leaf => match name.rsplit_once('.') {
                Some((stem, ext)) => (stem.to_string(), format!(".{ext}")),
                None => (name.clone(), String::new()),
            },
            _ => (name.clone(), String::new()),
        };
        let stem = unique_stem(&stem, &taken_stems(&target_dir), None);
        let new_location = target_dir.join(format!("{stem}{extension}"));
        fs::rename(&location, &new_location).map_err(io_at(&location))?;
        self.rescan(Some((location, new_location)));

        let old_key = order_key(old_parent.as_deref());
        let mut tree_dirty = false;
        if let Some(order) = self.tree.order.get_mut(&old_key) {
            order.retain(|x| x != id);
            tree_dirty = true;
        }
        if tree_dirty {
            self.save_tree()?;
        }
        if let Some(position) = position {
            self.reorder(id, position)?;
        } else if let Some(order) = self.tree.order.get_mut(&order_key(new_parent)) {
            order.push(id.to_string());
            self.save_tree()?;
        }
        Ok(())
    }

    /// Moves a page to `position` among its siblings and saves the order.
    pub fn reorder(&mut self, id: &str, position: usize) -> CoreResult<()> {
        self.writable()?;
        let parent = self.entry(id)?.parent.clone();
        let siblings = self.sorted_children(parent.as_deref());
        // An order list refers to pages by ID, so their IDs must be on disk.
        for sibling in &siblings {
            if let Err(err) = self.ensure_persisted_id(sibling) {
                log::warn!("page {sibling} keeps its title order: {err}");
            }
        }
        let mut order: Vec<String> = self
            .sorted_children(parent.as_deref())
            .into_iter()
            .filter(|s| s != id && self.index.get(s).is_some_and(|e| e.id_persisted))
            .collect();
        order.insert(position.min(order.len()), id.to_string());
        self.tree.order.insert(order_key(parent.as_deref()), order);
        self.save_tree()?;
        self.pending.tree_changed = true;
        Ok(())
    }

    /// Copies a page and its sub-pages next to it, with new IDs.
    pub fn duplicate_page(&mut self, id: &str) -> CoreResult<PageSummary> {
        self.writable()?;
        self.ensure_persisted_id(id)?;
        let entry = self.entry(id)?.clone();
        let location = entry.location().to_path_buf();
        let parent_dir = location
            .parent()
            .expect("pages live inside the workspace")
            .to_path_buf();
        let title = format!("{} (copy)", entry.title);
        let stem = unique_stem(&sanitize_stem(&title), &taken_stems(&parent_dir), None);
        let name = match entry.kind {
            PageKind::Leaf => format!("{stem}.md"),
            _ => stem,
        };
        let staging = self.staging().join(format!("dup-{}", new_id()));
        fs::create_dir_all(&staging).map_err(io_at(&staging))?;
        let staged = staging.join(&name);
        copy_tree(&location, &staged)?;

        let new_root_id = new_id();
        let root_file = match entry.kind {
            PageKind::Leaf => staged.clone(),
            _ => staged.join("index.md"),
        };
        for file in markdown_files(&staged) {
            let Ok(text) = fs::read_to_string(&file) else {
                continue;
            };
            let is_root = file == root_file;
            let has_id = matches!(
                frontmatter::split(&text).yaml.map(frontmatter::parse),
                Some(Parsed::Editable {
                    fields: frontmatter::Fields { id: Some(_), .. },
                    ..
                })
            );
            if !is_root && !has_id {
                continue;
            }
            let mut changes = vec![(
                "id",
                Value::Plain(if is_root {
                    new_root_id.clone()
                } else {
                    new_id()
                }),
            )];
            if is_root {
                changes.push(("title", Value::Str(title.clone())));
            }
            if let Ok(new_text) = Self::rewrite(&text, &changes, None, true) {
                write_atomic_staged(&file, new_text.as_bytes(), None).map_err(io_at(&file))?;
            }
        }
        let destination = parent_dir.join(&name);
        fs::rename(&staged, &destination).map_err(io_at(&staged))?;
        let _ = fs::remove_dir_all(&staging);
        self.rescan(None);
        Ok(self.summary(self.entry(&new_root_id)?))
    }

    /// Moves a page and its sub-pages to `.forma/trash/` (ARCHITECTURE §4.7).
    pub fn trash_page(&mut self, id: &str) -> CoreResult<TrashEntryInfo> {
        self.writable()?;
        let entry = self.entry(id)?.clone();
        let location = entry.location().to_path_buf();
        let mut page_ids = Vec::new();
        let mut stack = vec![entry.id.clone()];
        while let Some(current) = stack.pop() {
            if let Some(e) = self.index.get(&current)
                && e.id_persisted
            {
                page_ids.push(current.clone());
            }
            stack.extend(self.index.children_of(Some(&current)).iter().cloned());
        }
        let count = u32::try_from(self.subtree_size(id)).unwrap_or(u32::MAX);
        let record = TrashRecord {
            format_version: FORMAT_VERSION,
            entry_id: new_id(),
            trashed_at: now_rfc3339(),
            root_page_id: entry.id_persisted.then(|| entry.id.clone()),
            title: entry.title.clone(),
            original_parent_id: entry.parent.clone(),
            original_rel_path: location
                .strip_prefix(&self.root)
                .unwrap_or(&location)
                .display()
                .to_string(),
            page_ids,
        };
        let trash_dir = self.forma().join("trash").join(&record.entry_id);
        fs::create_dir_all(&trash_dir).map_err(io_at(&trash_dir))?;
        save_json(&trash_dir.join("entry.json"), &record, &self.staging())?;
        let name = location.file_name().expect("pages have names");
        fs::rename(&location, trash_dir.join(name)).map_err(io_at(&location))?;
        self.rescan(None);
        Ok(TrashEntryInfo {
            entry_id: record.entry_id,
            title: record.title,
            trashed_at: record.trashed_at,
            page_count: count,
        })
    }

    fn subtree_size(&self, id: &str) -> usize {
        1 + self
            .index
            .children_of(Some(id))
            .iter()
            .map(|c| self.subtree_size(c))
            .sum::<usize>()
    }

    pub fn list_trash(&self) -> Vec<TrashEntryInfo> {
        let mut entries: Vec<TrashEntryInfo> = fs::read_dir(self.forma().join("trash"))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|item| {
                let record: TrashRecord = load_json(&item.path().join("entry.json")).ok()??;
                let page_count = markdown_files(&item.path()).len().max(1);
                Some(TrashEntryInfo {
                    entry_id: record.entry_id,
                    title: record.title,
                    trashed_at: record.trashed_at,
                    page_count: u32::try_from(page_count).unwrap_or(u32::MAX),
                })
            })
            .collect();
        entries.sort_by(|a, b| b.trashed_at.cmp(&a.trashed_at));
        entries
    }

    /// Puts a trashed page back under its old parent, or at the root if that
    /// parent is gone. Returns the restored page's ID.
    pub fn restore_trash(&mut self, entry_id: &str) -> CoreResult<Option<String>> {
        self.writable()?;
        validate_uuid(entry_id, "Trash entry")?;
        let trash_dir = self.forma().join("trash").join(entry_id);
        let record: TrashRecord = load_json(&trash_dir.join("entry.json"))
            .map_err(CoreError::Invalid)?
            .ok_or_else(|| CoreError::NotFound("This item in the trash".into()))?;
        let item = fs::read_dir(&trash_dir)
            .map_err(io_at(&trash_dir))?
            .flatten()
            .find(|e| e.file_name() != "entry.json")
            .ok_or_else(|| CoreError::NotFound("The trashed page".into()))?
            .path();
        let target_dir = match record
            .original_parent_id
            .as_deref()
            .filter(|p| self.index.get(p).is_some())
        {
            Some(parent) => self.dir_for_children(parent)?,
            None => self.root.clone(),
        };
        let name = item
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (stem, extension) = if item.is_dir() {
            (name.clone(), String::new())
        } else {
            match name.rsplit_once('.') {
                Some((stem, ext)) => (stem.to_string(), format!(".{ext}")),
                None => (name.clone(), String::new()),
            }
        };
        let stem = unique_stem(&stem, &taken_stems(&target_dir), None);
        let destination = target_dir.join(format!("{stem}{extension}"));
        fs::rename(&item, &destination).map_err(io_at(&item))?;
        fs::remove_dir_all(&trash_dir).map_err(io_at(&trash_dir))?;
        self.rescan(None);
        let restored = record
            .root_page_id
            .filter(|id| self.index.get(id).is_some())
            .or_else(|| {
                self.index
                    .entries
                    .values()
                    .find(|e| e.location() == destination)
                    .map(|e| e.id.clone())
            });
        Ok(restored)
    }

    /// Permanently deletes a trash entry. The UI confirms this first.
    pub fn delete_trash_entry(&mut self, entry_id: &str) -> CoreResult<()> {
        self.writable()?;
        validate_uuid(entry_id, "Trash entry")?;
        let trash_dir = self.forma().join("trash").join(entry_id);
        fs::remove_dir_all(&trash_dir).map_err(io_at(&trash_dir))
    }

    pub fn set_favorite(&mut self, id: &str, favorite: bool) -> CoreResult<()> {
        self.writable()?;
        if favorite {
            self.ensure_persisted_id(id)?;
            if !self.tree.favorites.iter().any(|f| f == id) {
                self.tree.favorites.push(id.to_string());
            }
        } else {
            self.tree.favorites.retain(|f| f != id);
        }
        self.save_tree()?;
        self.pending.tree_changed = true;
        Ok(())
    }

    fn save_tree(&self) -> CoreResult<()> {
        save_json(&self.forma().join("tree.json"), &self.tree, &self.staging())
    }

    // ---------------------------------------------------------- attachments

    /// Stores an attachment in the page's `_assets/` folder and returns the
    /// reference to put in the Markdown (relative to the page file).
    pub fn import_attachment(
        &mut self,
        page_id: &str,
        file_name: &str,
        bytes: &[u8],
    ) -> CoreResult<String> {
        self.writable()?;
        let dir = self.dir_for_children(page_id)?;
        let assets = dir.join("_assets");
        fs::create_dir_all(&assets).map_err(io_at(&assets))?;
        let (stem, extension) = match file_name.rsplit_once('.') {
            Some((stem, ext))
                if !ext.is_empty()
                    && ext.len() <= 10
                    && ext.chars().all(|c| c.is_ascii_alphanumeric()) =>
            {
                (stem, format!(".{}", ext.to_ascii_lowercase()))
            }
            _ => (file_name, String::new()),
        };
        if extension.eq_ignore_ascii_case(".md") || extension.eq_ignore_ascii_case(".markdown") {
            return Err(CoreError::Invalid(
                "Markdown files can't be attached.".into(),
            ));
        }
        let taken: HashSet<String> = fs::read_dir(&assets)
            .map_err(io_at(&assets))?
            .flatten()
            .map(|e| name_key(&e.file_name().to_string_lossy()))
            .collect();
        let base = sanitize_stem(stem);
        let name = (1..)
            .map(|n| {
                if n == 1 {
                    format!("{base}{extension}")
                } else {
                    format!("{base} {n}{extension}")
                }
            })
            .find(|candidate| !taken.contains(&name_key(candidate)))
            .expect("an unbounded range always yields a free name");
        let path = assets.join(&name);
        write_atomic_staged(&path, bytes, Some(&self.staging())).map_err(io_at(&path))?;
        Ok(format!("_assets/{}", percent_encode_path(&name)))
    }

    /// Resolves an image or attachment reference from a page to a file inside
    /// the workspace. Anything that leaves the workspace (`..`, symlinks) or
    /// points into `.forma/` is refused.
    pub fn resolve_asset(&self, page_id: &str, reference: &str) -> CoreResult<PathBuf> {
        let entry = self.entry(page_id)?;
        let base = match (&entry.dir, &entry.file) {
            (Some(dir), _) => dir.clone(),
            (None, Some(file)) => file
                .parent()
                .expect("pages live inside the workspace")
                .to_path_buf(),
            _ => unreachable!("every page has a file or a directory"),
        };
        let decoded = percent_decode(reference);
        if decoded.contains('\0') || Path::new(&decoded).is_absolute() {
            return Err(CoreError::Invalid(
                "That file reference isn't allowed.".into(),
            ));
        }
        let candidate = base.join(&decoded);
        let resolved =
            fs::canonicalize(&candidate).map_err(|_| CoreError::NotFound("This file".into()))?;
        if !resolved.starts_with(&self.root)
            || resolved.starts_with(self.forma())
            || !resolved.is_file()
        {
            return Err(CoreError::Invalid(
                "That file is outside the workspace.".into(),
            ));
        }
        Ok(resolved)
    }

    // ------------------------------------------------------------- recovery

    /// Keeps an editor buffer that couldn't be saved normally (ARCHITECTURE §4.9).
    pub fn write_recovery(&self, page_id: &str, body: &str, reason: &str) -> CoreResult<()> {
        validate_uuid(page_id, "Page")?;
        let dir = self.forma().join("recovery");
        let title = self
            .index
            .get(page_id)
            .map(|e| e.title.clone())
            .unwrap_or_default();
        let record = RecoveryRecord {
            page_id: page_id.into(),
            title,
            saved_at: now_rfc3339(),
            reason: reason.into(),
        };
        let body_path = dir.join(format!("{page_id}.md"));
        write_atomic_staged(&body_path, body.as_bytes(), Some(&self.staging()))
            .map_err(io_at(&body_path))?;
        save_json(
            &dir.join(format!("{page_id}.json")),
            &record,
            &self.staging(),
        )
    }

    pub fn list_recovery(&self) -> Vec<RecoveryBuffer> {
        let dir = self.forma().join("recovery");
        let mut buffers: Vec<RecoveryBuffer> = fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".json"))
            .filter_map(|e| {
                let record: RecoveryRecord = load_json(&e.path()).ok()??;
                let body = fs::read_to_string(dir.join(format!("{}.md", record.page_id))).ok()?;
                Some(RecoveryBuffer {
                    page_id: record.page_id,
                    title: record.title,
                    saved_at: record.saved_at,
                    reason: record.reason,
                    body,
                })
            })
            .collect();
        buffers.sort_by(|a, b| b.saved_at.cmp(&a.saved_at));
        buffers
    }

    pub fn discard_recovery(&self, page_id: &str) -> CoreResult<()> {
        validate_uuid(page_id, "Page")?;
        let dir = self.forma().join("recovery");
        for ext in ["md", "json"] {
            let path = dir.join(format!("{page_id}.{ext}"));
            match fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(io_at(&path)(e)),
                _ => {}
            }
        }
        Ok(())
    }

    /// Saves `text` as a history snapshot ("Use theirs" keeps the discarded buffer).
    pub fn snapshot_text(&self, page_id: &str, text: &str, reason: &str) -> CoreResult<()> {
        validate_uuid(page_id, "Page")?;
        history::snapshot(
            &self.forma().join("history"),
            page_id,
            text,
            reason,
            &self.staging(),
        )
        .map(|_| ())
    }
}

/// Folders a workspace must not be: scanning them would read far more than notes.
/// Counts the files and folders under `root` (hidden ones and symlinks are not
/// followed), stopping once the count passes `limit`. Used to warn before a
/// very large folder becomes a workspace.
pub fn count_entries(root: &Path, limit: usize) -> usize {
    let mut count = 0;
    let mut pending = vec![root.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let Ok(read) = fs::read_dir(&dir) else {
            continue;
        };
        for item in read.flatten() {
            if item.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            count += 1;
            if count > limit {
                return count;
            }
            if item.file_type().is_ok_and(|t| t.is_dir()) {
                pending.push(item.path());
            }
        }
    }
    count
}

/// True if `root` already holds a Forma workspace.
pub fn is_workspace(root: &Path) -> bool {
    root.join(".forma").join("workspace.json").is_file()
}

fn refuse_unsuitable_folder(root: &Path) -> CoreResult<()> {
    let home = std::env::home_dir();
    let system = [
        "/",
        "/System",
        "/Library",
        "/Applications",
        "/Users",
        "/Volumes",
        "/private",
    ];
    if system.iter().any(|s| root == Path::new(s)) || home.as_deref() == Some(root) {
        return Err(CoreError::Invalid(
            "Choose a folder just for your notes, not your home folder or a system folder.".into(),
        ));
    }
    Ok(())
}

fn diff(old: &Index, new: &Index) -> WorkspaceChanges {
    let mut changes = WorkspaceChanges::default();
    for (id, entry) in &new.entries {
        match old.get(id) {
            None => {
                changes.content_changed.push(id.clone());
                changes.tree_changed = true;
            }
            Some(before) => {
                if before.rev != entry.rev {
                    changes.content_changed.push(id.clone());
                }
                if before.parent != entry.parent
                    || before.title != entry.title
                    || before.icon != entry.icon
                    || before.kind != entry.kind
                    || before.location() != entry.location()
                    || before.status != entry.status
                {
                    changes.tree_changed = true;
                }
            }
        }
    }
    for id in old
        .entries
        .keys()
        .filter(|id| !new.entries.contains_key(*id))
    {
        changes.removed.push(id.clone());
        changes.tree_changed = true;
    }
    changes
}

fn remove_empty_conversion_dirs(dir: &Path) {
    let Ok(read) = fs::read_dir(dir) else { return };
    for item in read.flatten() {
        let name = item.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || !item.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let path = item.path();
        let is_empty = fs::read_dir(&path)
            .map(|mut r| r.next().is_none())
            .unwrap_or(false);
        if is_empty && dir.join(format!("{name}.md")).is_file() {
            if let Err(err) = fs::remove_dir(&path) {
                log::warn!("couldn't remove empty folder {path:?}: {err}");
            }
        } else {
            remove_empty_conversion_dirs(&path);
        }
    }
}

fn copy_tree(from: &Path, to: &Path) -> CoreResult<()> {
    let meta = fs::symlink_metadata(from).map_err(io_at(from))?;
    if meta.is_symlink() {
        return Ok(());
    }
    if meta.is_dir() {
        fs::create_dir_all(to).map_err(io_at(to))?;
        for item in fs::read_dir(from).map_err(io_at(from))?.flatten() {
            copy_tree(&item.path(), &to.join(item.file_name()))?;
        }
    } else {
        fs::copy(from, to).map_err(io_at(from))?;
    }
    Ok(())
}

fn markdown_files(path: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(meta) = fs::symlink_metadata(path) else {
        return found;
    };
    if meta.is_file() {
        if path
            .extension()
            .is_some_and(|e| naming::is_markdown_extension(&e.to_string_lossy()))
        {
            found.push(path.to_path_buf());
        }
    } else if meta.is_dir() {
        for item in fs::read_dir(path).into_iter().flatten().flatten() {
            if item.file_name() != "_assets" && !item.file_name().to_string_lossy().starts_with('.')
            {
                found.extend(markdown_files(&item.path()));
            }
        }
    }
    found
}

/// Encodes characters that would break a Markdown link destination.
fn percent_encode_path(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            ' ' => out.push_str("%20"),
            '(' => out.push_str("%28"),
            ')' => out.push_str("%29"),
            '<' => out.push_str("%3C"),
            '>' => out.push_str("%3E"),
            '%' => out.push_str("%25"),
            c => out.push(c),
        }
    }
    out
}

/// Decodes `%XX` escapes (as used in Markdown link destinations and headers).
pub fn percent_decode(input: &str) -> String {
    let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(high), Some(low)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push(high * 16 + low);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
