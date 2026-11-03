//! Discovers pages on disk (DATA_MODEL §3.2) into an in-memory index.
//!
//! The in-memory index is the session's view of the page tree. It starts
//! from the SQLite cache (ADR-035), and a rescan re-reads only files whose
//! size or modification time changed, so reconciling after an edit costs a
//! directory walk, not a full parse. Files that were read also yield their
//! content (text, links, frontmatter) for the SQLite index.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::extract::extract;
use crate::frontmatter::{self, Parsed};
use crate::index::PageContent;

use super::naming::is_markdown_extension;
use super::types::{PageKind, PageStatus};

/// Size and modification time, used to skip unchanged files.
pub type Signature = (u64, SystemTime);

#[derive(Debug, Clone)]
pub struct Entry {
    pub id: String,
    /// False when the ID exists only in memory (the file has no `id` yet, ADR-005).
    pub id_persisted: bool,
    pub kind: PageKind,
    /// The page's Markdown file. `None` for a folder page without `index.md`.
    pub file: Option<PathBuf>,
    /// The bundle or folder directory. `None` for a leaf.
    pub dir: Option<PathBuf>,
    pub parent: Option<String>,
    pub title: String,
    pub icon: Option<String>,
    pub status: PageStatus,
    pub signature: Option<Signature>,
    pub rev: Option<String>,
    /// The frontmatter has a `collection:` schema (DATA_MODEL §6.1).
    pub is_collection: bool,
}

impl Entry {
    /// The file or directory that moves when the page moves.
    pub fn location(&self) -> &Path {
        self.dir
            .as_deref()
            .or(self.file.as_deref())
            .expect("every page has a file or a directory")
    }
}

#[derive(Debug, Default, Clone)]
pub struct Index {
    pub entries: HashMap<String, Entry>,
    pub children: HashMap<Option<String>, Vec<String>>,
}

impl Index {
    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.entries.get(id)
    }

    pub fn children_of(&self, parent: Option<&str>) -> &[String] {
        self.children
            .get(&parent.map(str::to_string))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// True if `candidate` is `id` or one of its descendants.
    pub fn is_self_or_descendant(&self, id: &str, candidate: &str) -> bool {
        let mut cursor = Some(candidate.to_string());
        while let Some(current) = cursor {
            if current == id {
                return true;
            }
            cursor = self.entries.get(&current).and_then(|e| e.parent.clone());
        }
        false
    }
}

pub fn hash_rev(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex()[..16].to_string()
}

pub fn signature(path: &Path) -> Option<Signature> {
    let meta = fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()?))
}

/// What a page file says about itself.
pub struct FileFacts {
    pub id: Option<String>,
    pub title: Option<String>,
    pub icon: Option<String>,
    pub status: PageStatus,
    pub rev: String,
    pub is_collection: bool,
    pub content: PageContent,
}

pub fn read_facts(path: &Path) -> std::io::Result<FileFacts> {
    Ok(facts_from_bytes(&fs::read(path)?))
}

/// What a page file's bytes say about the page.
pub fn facts_from_bytes(bytes: &[u8]) -> FileFacts {
    let rev = hash_rev(bytes);
    let text = String::from_utf8_lossy(bytes);
    let split = frontmatter::split(&text);
    let (fields, status) = match split.yaml.map(frontmatter::parse) {
        None => (frontmatter::Fields::default(), PageStatus::Ok),
        Some(Parsed::Editable { fields, .. }) => (fields, PageStatus::Ok),
        Some(Parsed::ReadOnly { fields }) => (fields, PageStatus::MetadataReadOnly),
        Some(Parsed::Invalid { message }) => (
            Default::default(),
            PageStatus::InvalidFrontmatter { message },
        ),
    };
    let json = match (&status, split.yaml) {
        (PageStatus::InvalidFrontmatter { .. }, _) => None,
        (_, Some(yaml)) => frontmatter::to_json(yaml),
        (_, None) => Some(serde_json::Map::new()),
    };
    let extracted = extract(split.body);
    FileFacts {
        id: fields.id,
        title: fields.title.filter(|t| !t.trim().is_empty()),
        icon: fields.icon,
        status,
        rev,
        is_collection: json.as_ref().is_some_and(|j| j.contains_key("collection")),
        content: PageContent {
            text: extracted.text,
            tags: fields.tags,
            links: extracted.links,
            frontmatter: json,
            passages: crate::passages::chunk(split.body),
        },
    }
}

/// The result of a scan.
pub struct Scanned {
    pub index: Index,
    /// Things worth telling the user, such as copied pages.
    pub notices: Vec<String>,
    /// The content of every file that was read, by file path.
    pub fresh: HashMap<PathBuf, PageContent>,
}

struct Scanner<'a> {
    previous: &'a Index,
    /// Previous entries by location, for reusing unchanged files and memory-only IDs.
    previous_by_location: HashMap<PathBuf, &'a Entry>,
    /// A directory that just moved: (old path, new path).
    remap: Option<(PathBuf, PathBuf)>,
    found: Vec<Entry>,
    fresh: HashMap<PathBuf, PageContent>,
}

/// Walks `root` and builds a fresh index. Entries from `previous` are reused
/// for unchanged files, and memory-only IDs survive as long as the path does
/// (or moved along `remap`). Returns the index and user-facing notices.
pub fn scan(root: &Path, previous: &Index, remap: Option<(PathBuf, PathBuf)>) -> Scanned {
    let previous_by_location = previous
        .entries
        .values()
        .map(|e| (e.location().to_path_buf(), e))
        .collect();
    let mut scanner = Scanner {
        previous,
        previous_by_location,
        remap,
        found: Vec::new(),
        fresh: HashMap::new(),
    };
    scanner.walk(root, None, false);
    let notices = scanner.resolve_duplicate_ids();

    let mut index = Index::default();
    for entry in scanner.found {
        index
            .children
            .entry(entry.parent.clone())
            .or_default()
            .push(entry.id.clone());
        index.entries.insert(entry.id.clone(), entry);
    }
    Scanned {
        index,
        notices,
        fresh: scanner.fresh,
    }
}

impl Scanner<'_> {
    fn walk(&mut self, dir: &Path, parent: Option<String>, is_page_dir: bool) {
        let Ok(read) = fs::read_dir(dir) else {
            log::warn!("can't read directory {dir:?}");
            return;
        };
        let mut names: Vec<_> = read.flatten().collect();
        names.sort_by_key(|e| e.file_name());
        for item in names {
            let name = item.file_name().to_string_lossy().into_owned();
            let Ok(file_type) = item.file_type() else {
                continue;
            };
            if name.starts_with('.') || file_type.is_symlink() {
                continue;
            }
            let path = item.path();
            if file_type.is_dir() {
                if is_page_dir && name == "_assets" {
                    continue;
                }
                let index_file = path.join("index.md");
                let is_bundle = fs::symlink_metadata(&index_file).is_ok_and(|m| m.is_file());
                let id = if is_bundle {
                    self.add_file_page(&index_file, Some(&path), &name, parent.clone())
                } else {
                    self.add_folder_page(&path, &name, parent.clone())
                };
                self.walk(&path, Some(id), true);
            } else if file_type.is_file() {
                let Some((stem, ext)) = name.rsplit_once('.') else {
                    continue;
                };
                if !is_markdown_extension(ext) || (is_page_dir && name == "index.md") {
                    continue;
                }
                self.add_file_page(&path, None, stem, parent.clone());
            }
        }
    }

    /// The previous entry at this location, following a pending directory move.
    fn previous_at(&self, location: &Path) -> Option<&Entry> {
        let mut found = self.previous_by_location.get(location).copied();
        if found.is_none()
            && let Some((old, new)) = &self.remap
            && let Ok(rest) = location.strip_prefix(new)
        {
            found = self.previous_by_location.get(&old.join(rest)).copied();
        }
        found
    }

    fn add_file_page(
        &mut self,
        file: &Path,
        dir: Option<&Path>,
        fallback_title: &str,
        parent: Option<String>,
    ) -> String {
        let location = dir.unwrap_or(file);
        let kind = if dir.is_some() {
            PageKind::Bundle
        } else {
            PageKind::Leaf
        };
        let signature = signature(file);
        let previous = self.previous_at(location).cloned();

        // Unchanged file in the same place: reuse what we know without reading it.
        if let Some(prev) = &previous
            && prev.signature.is_some()
            && prev.signature == signature
            && prev.file.as_deref() == Some(file)
        {
            let mut entry = prev.clone();
            entry.parent = parent;
            entry.kind = kind;
            let id = entry.id.clone();
            self.found.push(entry);
            return id;
        }

        let facts = match read_facts(file) {
            Ok(facts) => facts,
            Err(err) => {
                log::warn!("can't read page {file:?}: {err}");
                FileFacts {
                    id: None,
                    title: None,
                    icon: None,
                    status: PageStatus::InvalidFrontmatter {
                        message: format!("the file couldn't be read: {err}"),
                    },
                    rev: String::new(),
                    is_collection: false,
                    content: PageContent::default(),
                }
            }
        };
        let (id, id_persisted) = match facts.id {
            Some(id) => (id, true),
            None => match previous.filter(|p| !p.id_persisted) {
                Some(prev) => (prev.id, false),
                None => (new_id(), false),
            },
        };
        self.fresh.insert(file.to_path_buf(), facts.content);
        self.found.push(Entry {
            id: id.clone(),
            id_persisted,
            kind,
            file: Some(file.to_path_buf()),
            dir: dir.map(Path::to_path_buf),
            parent,
            title: facts.title.unwrap_or_else(|| fallback_title.to_string()),
            icon: facts.icon,
            status: facts.status,
            signature,
            rev: Some(facts.rev).filter(|r| !r.is_empty()),
            is_collection: facts.is_collection,
        });
        id
    }

    fn add_folder_page(&mut self, dir: &Path, name: &str, parent: Option<String>) -> String {
        let id = self
            .previous_at(dir)
            .filter(|p| p.kind == PageKind::Folder)
            .map(|p| p.id.clone())
            .unwrap_or_else(new_id);
        self.found.push(Entry {
            id: id.clone(),
            id_persisted: false,
            kind: PageKind::Folder,
            file: None,
            dir: Some(dir.to_path_buf()),
            parent,
            title: name.to_string(),
            icon: None,
            status: PageStatus::Ok,
            signature: None,
            rev: None,
            is_collection: false,
        });
        id
    }

    /// Two files with the same `id` (a Finder copy, a sync conflict copy): the
    /// file the index already knew keeps it, else the oldest file, else the
    /// first path. The other gets a memory-only ID, which is written to its
    /// file the next time Forma writes it (DATA_MODEL §4.4).
    fn resolve_duplicate_ids(&mut self) -> Vec<String> {
        let mut by_id: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, entry) in self.found.iter().enumerate() {
            if entry.id_persisted {
                by_id.entry(entry.id.clone()).or_default().push(i);
            }
        }
        let mut notices = Vec::new();
        for (id, mut indices) in by_id.into_iter().filter(|(_, v)| v.len() > 1) {
            let known = self.previous.get(&id).and_then(|e| e.file.clone());
            indices.sort_by_key(|&i| {
                let entry = &self.found[i];
                let is_known = entry.file.is_some() && entry.file == known;
                let born = entry
                    .file
                    .as_deref()
                    .and_then(|f| fs::metadata(f).ok()?.created().ok())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                (!is_known, born, entry.location().to_path_buf())
            });
            for &i in &indices[1..] {
                let old_location = self.found[i].location().to_path_buf();
                // A copy that was already told apart keeps the ID it got then,
                // so an edit made outside Forma doesn't make it a new page.
                if let Some(earlier) = self
                    .previous_at(&old_location)
                    .filter(|p| !p.id_persisted && p.id != id)
                    .map(|p| p.id.clone())
                {
                    self.relabel(i, &id, earlier);
                    continue;
                }
                self.relabel(i, &id, new_id());
                notices.push(format!(
                    "\"{}\" had the same ID as another page (probably a copy). Forma will give it its own ID the next time it saves it.",
                    self.found[i].title
                ));
            }
        }
        notices
    }

    /// Gives `found[i]` a memory-only ID, and its children the new parent.
    fn relabel(&mut self, i: usize, old_id: &str, new: String) {
        let location = self.found[i].location().to_path_buf();
        self.found[i].id = new.clone();
        self.found[i].id_persisted = false;
        for other in &mut self.found {
            if other.parent.as_deref() == Some(old_id)
                && other.location().parent() == Some(location.as_path())
            {
                other.parent = Some(new.clone());
            }
        }
    }
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Natural sort key: case-insensitive, with digit runs compared as numbers.
pub fn natural_key(title: &str) -> Vec<(u8, u128, String)> {
    let mut key = Vec::new();
    let mut chars = title.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_ascii_digit() {
            let mut digits = String::new();
            while let Some(&d) = chars.peek().filter(|d| d.is_ascii_digit()) {
                digits.push(d);
                chars.next();
            }
            key.push((0, digits.parse().unwrap_or(u128::MAX), String::new()));
        } else {
            let mut text = String::new();
            while let Some(&t) = chars.peek().filter(|t| !t.is_ascii_digit()) {
                text.extend(t.to_lowercase());
                chars.next();
            }
            key.push((1, 0, text));
        }
    }
    key
}
