//! A read-only view of a workspace: everything the AI assistant can reach
//! (ARCHITECTURE §10.1, ADR-021). It has no method that writes, so code
//! holding only a `ReadView` can't change a page, whatever a model asks.
//! `Workspace` itself has a few `&self` methods that do write (recovery
//! buffers, snapshots); they are deliberately not here.

use super::Workspace;
use super::types::{PageDocument, PageRef, PageSummary};
use crate::error::CoreResult;
use crate::index::RESERVED_KEYS;

#[derive(Clone, Copy)]
pub struct ReadView<'a> {
    workspace: &'a Workspace,
}

impl Workspace {
    pub fn read_view(&self) -> ReadView<'_> {
        ReadView { workspace: self }
    }
}

impl<'a> ReadView<'a> {
    pub(super) fn workspace(&self) -> &'a Workspace {
        self.workspace
    }

    /// The page, read fresh from disk.
    pub fn page(&self, id: &str) -> CoreResult<PageDocument> {
        self.workspace.get_page(id)
    }

    /// Whether the page's ID is in its file (ADR-005); `None` if no such page.
    pub fn id_persisted(&self, id: &str) -> Option<bool> {
        self.workspace.index.get(id).map(|e| e.id_persisted)
    }

    pub fn title(&self, id: &str) -> Option<String> {
        self.workspace.index.get(id).map(|e| e.title.clone())
    }

    /// Children in display order; `None` for the top level.
    pub fn children(&self, parent: Option<&str>) -> CoreResult<Vec<PageSummary>> {
        self.workspace.list_children(parent)
    }

    /// Root first, excluding the page itself.
    pub fn ancestors(&self, id: &str) -> Vec<PageRef> {
        self.workspace.ancestors(id)
    }

    /// The page's frontmatter keys that aren't Diaspar's own (DATA_MODEL
    /// §4.2), such as collection properties, as the index last read them.
    pub fn properties(&self, id: &str) -> serde_json::Map<String, serde_json::Value> {
        let mut map = self
            .workspace
            .db
            .frontmatter(&[id.to_string()])
            .ok()
            .and_then(|mut all| all.remove(id))
            .unwrap_or_default();
        map.retain(|key, _| !RESERVED_KEYS.contains(&key.as_str()));
        map
    }

    /// A collection's schema, when it can be changed.
    pub fn schema(&self, collection_id: &str) -> Option<crate::collection::Schema> {
        self.workspace.editable_schema(collection_id).ok()
    }

    pub fn page_count(&self) -> usize {
        self.workspace.index.entries.len()
    }
}
