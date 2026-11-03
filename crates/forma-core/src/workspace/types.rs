//! Values the workspace returns over IPC. Page IDs are UUID strings.

use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PageKind {
    /// `Name.md`
    Leaf,
    /// `Name/index.md`, with children and/or attachments
    Bundle,
    /// A folder without `index.md`, created outside Forma
    Folder,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export)]
pub enum PageStatus {
    Ok,
    /// The frontmatter parses but can't be edited safely; the body still can be.
    MetadataReadOnly,
    /// The frontmatter can't be read. The page is read-only until it's fixed.
    InvalidFrontmatter {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct PageRef {
    pub id: String,
    pub title: String,
    pub icon: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct PageSummary {
    pub id: String,
    pub title: String,
    pub icon: Option<String>,
    pub kind: PageKind,
    pub has_children: bool,
    pub favorite: bool,
    pub status: PageStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct PageDocument {
    pub id: String,
    pub parent_id: Option<String>,
    pub title: String,
    pub icon: Option<String>,
    pub tags: Vec<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    /// The Markdown after the frontmatter.
    pub body: String,
    /// Revision of the whole file. `None` for a folder page that has no `index.md` yet.
    pub rev: Option<String>,
    /// Revision of the body only: what `save_page` checks (ADR-033).
    pub body_rev: String,
    pub kind: PageKind,
    pub status: PageStatus,
    pub favorite: bool,
    /// Root first, excluding this page.
    pub ancestors: Vec<PageRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "outcome", rename_all = "snake_case")]
#[ts(export)]
pub enum SaveOutcome {
    Saved {
        /// Revision of the whole file.
        rev: String,
        /// Revision of the body just written; the base for the next save.
        body_rev: String,
        updated_at: String,
    },
    /// The body changed since `base_body_rev`. Nothing was written.
    Conflict {
        current_body_rev: Option<String>,
        current_body: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct WorkspaceInfo {
    pub root: String,
    pub name: String,
    pub id: String,
    /// Set when nothing can be changed, with the reason.
    pub read_only_reason: Option<String>,
    /// One-time messages about things Forma noticed while opening.
    pub notices: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct TrashEntryInfo {
    pub entry_id: String,
    pub title: String,
    pub trashed_at: String,
    pub page_count: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct WorkspaceChanges {
    /// Pages whose file content changed (or that appeared).
    pub content_changed: Vec<String>,
    pub removed: Vec<String>,
    /// Titles, icons, kinds or positions changed.
    pub tree_changed: bool,
}

impl WorkspaceChanges {
    pub fn is_empty(&self) -> bool {
        self.content_changed.is_empty() && self.removed.is_empty() && !self.tree_changed
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct RecoveryBuffer {
    pub page_id: String,
    pub title: String,
    pub saved_at: String,
    pub reason: String,
    pub body: String,
}
