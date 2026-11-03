//! The tools a model may call (ARCHITECTURE §10.2). Ask mode has read tools:
//! each one's `run` gets a [`ReadView`], which has no way to change a page,
//! and nothing else. Edit mode adds proposal tools: their `run` also gets a
//! [`ChangeSet`], which is plain data in memory. Applying it is a command
//! only the user's approval reaches, so no tool can change a page.
//! Arguments are page IDs, never paths; every ID is looked up in the
//! workspace index, so nothing outside it is reachable.
//!
//! Bad arguments come back to the model as a tool error it can correct.

use forma_core::collection::PropertyType;
use forma_core::passages::chunk;
use forma_core::workspace::changeset::{ChangeSet, Edit, NewProperty};
use forma_core::workspace::read::ReadView;
use forma_core::workspace::retrieval::{RetrievalFilter, Retriever};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ts_rs::TS;

use crate::knowledge::{SourceBook, SourceKind, fit, passage_block};
use crate::tokens::estimate;
use crate::types::ToolSpec;

/// Results longer than this are cut, in tokens.
pub const MAX_RESULT_TOKENS: u32 = 3000;
/// `read_page` returns a long page in parts of about this many tokens.
const PART_TOKENS: u32 = 2400;
const MAX_QUERY_CHARS: usize = 500;
const MAX_SEARCH_RESULTS: u32 = 10;
const DEFAULT_SEARCH_RESULTS: u32 = 5;
const MAX_LISTED: usize = 100;

/// What a tool did, for the panel: "Searched the notes for “x”".
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct ToolActivity {
    pub tool: String,
    pub label: String,
    pub failed: bool,
}

pub struct ToolOutput {
    /// What the model reads: JSON text.
    pub content: String,
    pub is_error: bool,
    pub activity: ToolActivity,
}

/// A tool error the model reads, and a line for the panel.
pub struct ToolError {
    pub message: String,
    pub label: String,
}

fn invalid(message: impl Into<String>) -> ToolError {
    let message = message.into();
    ToolError {
        label: "A lookup was refused".into(),
        message,
    }
}

type Run = fn(ReadView<'_>, &Value, &mut SourceBook, u32) -> Result<(String, String), ToolError>;

/// A read tool. `run` returns the result text and the panel's label; its
/// only handle on the workspace is the `ReadView`.
pub struct ReadTool {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: fn() -> Value,
    pub run: Run,
}

/// The tools of Ask mode. Every one is a [`ReadTool`].
pub const ASK_TOOLS: [ReadTool; 4] = [
    ReadTool {
        name: "search_pages",
        description: "Search the user's notes. Returns the best-matching passages as <source> blocks, each with a key to cite and the page_id of its page.",
        parameters: || {
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Words to look for, in the notes' language." },
                    "limit": { "type": "integer", "minimum": 1, "maximum": MAX_SEARCH_RESULTS, "description": "How many passages, 5 by default." }
                },
                "required": ["query"],
                "additionalProperties": false
            })
        },
        run: search_pages,
    },
    ReadTool {
        name: "read_page",
        description: "Read a page's text by its page ID. Long pages come in parts; ask for the next part to continue.",
        parameters: || {
            json!({
                "type": "object",
                "properties": {
                    "page_id": { "type": "string", "description": "The page ID from search or a list." },
                    "part": { "type": "integer", "minimum": 1, "description": "Which part of a long page, 1 by default." }
                },
                "required": ["page_id"],
                "additionalProperties": false
            })
        },
        run: read_page,
    },
    ReadTool {
        name: "list_pages",
        description: "List the pages inside a page, or the top-level pages when parent_id is left out.",
        parameters: || {
            json!({
                "type": "object",
                "properties": {
                    "parent_id": { "type": "string", "description": "The page whose subpages to list." }
                },
                "additionalProperties": false
            })
        },
        run: list_pages,
    },
    ReadTool {
        name: "get_page_metadata",
        description: "A page's title, location, tags, properties and dates, without its text.",
        parameters: || {
            json!({
                "type": "object",
                "properties": {
                    "page_id": { "type": "string" }
                },
                "required": ["page_id"],
                "additionalProperties": false
            })
        },
        run: get_page_metadata,
    },
];

pub fn specs() -> Vec<ToolSpec> {
    ASK_TOOLS
        .iter()
        .map(|t| ToolSpec {
            name: t.name.into(),
            description: t.description.into(),
            parameters: (t.parameters)(),
        })
        .collect()
}

/// Read tools and proposal tools, for Edit mode.
pub fn act_specs() -> Vec<ToolSpec> {
    let mut all = specs();
    all.extend(PROPOSE_TOOLS.iter().map(|t| ToolSpec {
        name: t.name.into(),
        description: t.description.into(),
        parameters: (t.parameters)(),
    }));
    all
}

type Propose = fn(ReadView<'_>, &Value, &mut ChangeSet) -> Result<String, ToolError>;

/// A proposal tool. `run` adds to the change set and returns the panel's
/// label; its only handles are the read-only view and the change set.
pub struct ProposeTool {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: fn() -> Value,
    pub run: Propose,
}

const PAGE_ID_HELP: &str = "A page ID from search_pages or list_pages, or new-N for a page proposed earlier in this answer.";

/// The tools Edit mode adds. Each only proposes: nothing changes until the
/// user reviews and applies the proposal.
pub const PROPOSE_TOOLS: [ProposeTool; 6] = [
    ProposeTool {
        name: "create_page",
        description: "Propose a new page. Returns its ID (new-N) for later proposals, such as moving pages into it.",
        parameters: || {
            json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "parent_id": { "type": "string", "description": format!("The page to put it in; leave out for the top level. {PAGE_ID_HELP}") },
                    "content": { "type": "string", "description": "The page's text, in Markdown." }
                },
                "required": ["title"],
                "additionalProperties": false
            })
        },
        run: create_page,
    },
    ProposeTool {
        name: "edit_page",
        description: "Propose changes to a page's text: exact replacements (read the page first and copy each passage you replace exactly; it must occur once), or a whole new text.",
        parameters: || {
            json!({
                "type": "object",
                "properties": {
                    "page_id": { "type": "string", "description": PAGE_ID_HELP },
                    "edits": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "old": { "type": "string", "description": "Text in the page now, exactly." },
                                "new": { "type": "string", "description": "What replaces it (empty to delete it)." }
                            },
                            "required": ["old", "new"],
                            "additionalProperties": false
                        }
                    },
                    "replace_all_body": { "type": "string", "description": "The page's whole new text, in Markdown, instead of edits." }
                },
                "required": ["page_id"],
                "additionalProperties": false
            })
        },
        run: edit_page,
    },
    ProposeTool {
        name: "move_page",
        description: "Propose moving a page, with its subpages, into another page, or to the top level.",
        parameters: || {
            json!({
                "type": "object",
                "properties": {
                    "page_id": { "type": "string", "description": PAGE_ID_HELP },
                    "new_parent_id": { "type": ["string", "null"], "description": "The page to move it into; null for the top level." }
                },
                "required": ["page_id"],
                "additionalProperties": false
            })
        },
        run: move_page,
    },
    ProposeTool {
        name: "create_collection",
        description: "Propose a new collection (a page whose subpages are entries with properties). Returns its ID (new-N); entries are created with create_page inside it.",
        parameters: || {
            json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "parent_id": { "type": "string", "description": PAGE_ID_HELP },
                    "properties": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "name": { "type": "string" },
                                "type": { "type": "string", "enum": ["text", "number", "date", "checkbox", "select", "multi_select"] },
                                "options": { "type": "array", "items": { "type": "string" }, "description": "For select and multi_select." }
                            },
                            "required": ["name", "type"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["title", "properties"],
                "additionalProperties": false
            })
        },
        run: create_collection,
    },
    ProposeTool {
        name: "update_page_properties",
        description: "Propose property values for an entry of a collection. Values: text, number, YYYY-MM-DD dates, true/false, an option name, or a list of option names.",
        parameters: || {
            json!({
                "type": "object",
                "properties": {
                    "page_id": { "type": "string", "description": PAGE_ID_HELP },
                    "set": { "type": "object", "description": "Property name → value." },
                    "unset": { "type": "array", "items": { "type": "string" }, "description": "Property names to clear." }
                },
                "required": ["page_id"],
                "additionalProperties": false
            })
        },
        run: update_page_properties,
    },
    ProposeTool {
        name: "link_pages",
        description: "Propose a link from one page to another: on `text` if given (it must occur once in the page), else as a new last line.",
        parameters: || {
            json!({
                "type": "object",
                "properties": {
                    "source_id": { "type": "string", "description": PAGE_ID_HELP },
                    "target_id": { "type": "string", "description": "The page to link to (an existing page)." },
                    "text": { "type": "string" }
                },
                "required": ["source_id", "target_id"],
                "additionalProperties": false
            })
        },
        run: link_pages,
    },
];

/// Runs a proposal tool, if `name` is one. The result tells the model what
/// the set now holds.
pub fn run_proposal(
    view: ReadView<'_>,
    name: &str,
    arguments: &str,
    set: &mut ChangeSet,
) -> Option<ToolOutput> {
    let tool = PROPOSE_TOOLS.iter().find(|t| t.name == name)?;
    let result = match serde_json::from_str::<Value>(if arguments.trim().is_empty() {
        "{}"
    } else {
        arguments
    }) {
        Ok(args) => (tool.run)(view, &args, set),
        Err(_) => Err(invalid("The arguments aren't valid JSON.")),
    };
    Some(match result {
        Ok(label) => ToolOutput {
            content: cap(
                format!(
                    "Proposed. Nothing has changed yet: the user reviews the proposal.\n\n{}",
                    set.describe(view)
                ),
                MAX_RESULT_TOKENS,
            ),
            is_error: false,
            activity: ToolActivity {
                tool: name.into(),
                label,
                failed: false,
            },
        },
        Err(error) => ToolOutput {
            content: json!({ "error": error.message }).to_string(),
            is_error: true,
            activity: ToolActivity {
                tool: name.into(),
                label: "A proposal was refused".into(),
                failed: true,
            },
        },
    })
}

fn proposal(message: String) -> ToolError {
    ToolError {
        label: "A proposal was refused".into(),
        message,
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateArgs {
    title: String,
    parent_id: Option<String>,
    content: Option<String>,
}

fn create_page(
    view: ReadView<'_>,
    value: &Value,
    set: &mut ChangeSet,
) -> Result<String, ToolError> {
    let a: CreateArgs = args(value)?;
    let id = set
        .propose_page(
            view,
            a.parent_id.as_deref(),
            &a.title,
            a.content.as_deref().unwrap_or(""),
        )
        .map_err(proposal)?;
    Ok(format!("Proposed a new page “{}” ({id})", a.title.trim()))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TextEdit {
    old: String,
    new: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditArgs {
    page_id: String,
    edits: Option<Vec<TextEdit>>,
    replace_all_body: Option<String>,
}

fn edit_page(view: ReadView<'_>, value: &Value, set: &mut ChangeSet) -> Result<String, ToolError> {
    let a: EditArgs = args(value)?;
    let edit = match (a.edits, a.replace_all_body) {
        (Some(edits), None) => Edit::Replace(edits.into_iter().map(|e| (e.old, e.new)).collect()),
        (None, Some(body)) => Edit::Body(body),
        _ => return Err(invalid("Give either edits or replace_all_body.")),
    };
    set.propose_edit(view, &a.page_id, &edit)
        .map_err(proposal)?;
    Ok(format!(
        "Proposed an edit to {}",
        quoted(&page_title(view, set, &a.page_id))
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MoveArgs {
    page_id: String,
    #[serde(default)]
    new_parent_id: Option<String>,
}

fn move_page(view: ReadView<'_>, value: &Value, set: &mut ChangeSet) -> Result<String, ToolError> {
    let a: MoveArgs = args(value)?;
    set.propose_move(view, &a.page_id, a.new_parent_id.as_deref())
        .map_err(proposal)?;
    Ok(format!(
        "Proposed moving {}",
        quoted(&page_title(view, set, &a.page_id))
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PropertyArg {
    name: String,
    #[serde(rename = "type")]
    kind: PropertyType,
    #[serde(default)]
    options: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CollectionArgs {
    title: String,
    parent_id: Option<String>,
    properties: Vec<PropertyArg>,
}

fn create_collection(
    view: ReadView<'_>,
    value: &Value,
    set: &mut ChangeSet,
) -> Result<String, ToolError> {
    let a: CollectionArgs = args(value)?;
    let properties: Vec<NewProperty> = a
        .properties
        .into_iter()
        .map(|p| NewProperty {
            name: p.name,
            kind: p.kind,
            options: p.options,
        })
        .collect();
    let id = set
        .propose_collection(view, a.parent_id.as_deref(), &a.title, &properties)
        .map_err(proposal)?;
    Ok(format!(
        "Proposed a new collection “{}” ({id})",
        a.title.trim()
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PropertiesArgs {
    page_id: String,
    #[serde(default)]
    set: serde_json::Map<String, Value>,
    #[serde(default)]
    unset: Vec<String>,
}

fn update_page_properties(
    view: ReadView<'_>,
    value: &Value,
    set: &mut ChangeSet,
) -> Result<String, ToolError> {
    let a: PropertiesArgs = args(value)?;
    set.propose_properties(view, &a.page_id, &a.set, &a.unset)
        .map_err(proposal)?;
    Ok(format!(
        "Proposed property values for {}",
        quoted(&page_title(view, set, &a.page_id))
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LinkArgs {
    source_id: String,
    target_id: String,
    text: Option<String>,
}

fn link_pages(view: ReadView<'_>, value: &Value, set: &mut ChangeSet) -> Result<String, ToolError> {
    let a: LinkArgs = args(value)?;
    set.propose_link(view, &a.source_id, &a.target_id, a.text.as_deref())
        .map_err(proposal)?;
    Ok(format!(
        "Proposed a link from {} to {}",
        quoted(&page_title(view, set, &a.source_id)),
        quoted(&page_title(view, set, &a.target_id))
    ))
}

/// A page's title, for proposals: existing, or proposed in `set`.
fn page_title(view: ReadView<'_>, set: &ChangeSet, id: &str) -> String {
    match set.target(view, id) {
        Ok(forma_core::workspace::changeset::Target::Existing(id)) => {
            view.title(&id).unwrap_or_default()
        }
        Ok(forma_core::workspace::changeset::Target::New(n)) => {
            set.preview_title(&n).unwrap_or_else(|| format!("new-{n}"))
        }
        Err(_) => id.to_string(),
    }
}

/// Runs the tool `name` with `arguments` (JSON text), adding any notes it
/// returns to `book`. The result is at most `budget` tokens.
pub fn run(
    view: ReadView<'_>,
    name: &str,
    arguments: &str,
    book: &mut SourceBook,
    budget: u32,
) -> ToolOutput {
    let budget = budget.min(MAX_RESULT_TOKENS);
    let result = match ASK_TOOLS.iter().find(|t| t.name == name) {
        None => Err(invalid(format!("There is no tool called {name:?}."))),
        Some(tool) => match serde_json::from_str::<Value>(if arguments.trim().is_empty() {
            "{}"
        } else {
            arguments
        }) {
            Ok(args) => (tool.run)(view, &args, book, budget),
            Err(_) => Err(invalid("The arguments aren't valid JSON.")),
        },
    };
    match result {
        Ok((content, label)) => ToolOutput {
            content: cap(content, budget),
            is_error: false,
            activity: ToolActivity {
                tool: name.into(),
                label,
                failed: false,
            },
        },
        Err(error) => ToolOutput {
            content: json!({ "error": error.message }).to_string(),
            is_error: true,
            activity: ToolActivity {
                tool: name.into(),
                label: error.label,
                failed: true,
            },
        },
    }
}

fn cap(content: String, budget: u32) -> String {
    let (kept, complete) = fit(&content, budget.saturating_sub(20));
    if complete {
        content
    } else {
        format!("{kept}\n[The rest was cut to fit the context window.]")
    }
}

fn args<T: DeserializeOwned>(value: &Value) -> Result<T, ToolError> {
    serde_json::from_value(value.clone()).map_err(|e| invalid(format!("Invalid arguments: {e}.")))
}

/// A page ID that names a page in this workspace.
fn page_id(view: ReadView<'_>, id: &str) -> Result<(String, String), ToolError> {
    let id = id.trim().to_ascii_lowercase();
    if uuid::Uuid::parse_str(&id).is_err() {
        return Err(invalid(format!(
            "{id:?} isn't a page ID. Page IDs look like 0c6a5f3e-6c1d-4b8e-9a63-2f1d1b0a9e77; get one from search_pages or list_pages."
        )));
    }
    match view.title(&id) {
        Some(title) => Ok((id, title)),
        None => Err(invalid("No page has that ID.")),
    }
}

fn quoted(text: &str) -> String {
    let short: String = text.chars().take(60).collect();
    if short.len() < text.len() {
        format!("“{short}…”")
    } else {
        format!("“{short}”")
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchArgs {
    query: String,
    limit: Option<u32>,
}

fn search_pages(
    view: ReadView<'_>,
    value: &Value,
    book: &mut SourceBook,
    budget: u32,
) -> Result<(String, String), ToolError> {
    let a: SearchArgs = args(value)?;
    let query = a.query.trim();
    if query.is_empty() {
        return Err(invalid("The query is empty."));
    }
    if query.chars().count() > MAX_QUERY_CHARS {
        return Err(invalid(format!(
            "The query is longer than {MAX_QUERY_CHARS} characters."
        )));
    }
    let limit = a.limit.unwrap_or(DEFAULT_SEARCH_RESULTS);
    if !(1..=MAX_SEARCH_RESULTS).contains(&limit) {
        return Err(invalid(format!(
            "limit must be between 1 and {MAX_SEARCH_RESULTS}."
        )));
    }
    let found = view.retrieve(query, limit as usize, &RetrievalFilter::default());
    let label = format!("Searched the notes for {}", quoted(query));
    if found.is_empty() {
        return Ok(("Nothing in the notes matched.".into(), label));
    }
    let mut blocks = Vec::new();
    let mut used = 0;
    for passage in &found {
        let title = view.title(&passage.page_id).unwrap_or_default();
        let room = budget.saturating_sub(used + 40);
        let Some(block) = passage_block(book, &title, passage, room) else {
            break;
        };
        used += estimate(&block) + 2;
        blocks.push(block);
    }
    if blocks.is_empty() {
        return Ok((
            "The passages found are too long for the room left in the context window.".into(),
            label,
        ));
    }
    Ok((blocks.join("\n\n"), label))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadArgs {
    page_id: String,
    part: Option<u32>,
}

fn read_page(
    view: ReadView<'_>,
    value: &Value,
    book: &mut SourceBook,
    budget: u32,
) -> Result<(String, String), ToolError> {
    let a: ReadArgs = args(value)?;
    let (id, title) = page_id(view, &a.page_id)?;
    let page = view
        .page(&id)
        .map_err(|e| invalid(format!("The page can't be read: {e}.")))?;
    // Parts are runs of whole passages, so no part starts mid-sentence.
    let size = PART_TOKENS.min(budget.saturating_sub(200)).max(200);
    let mut parts: Vec<(usize, usize)> = Vec::new();
    for p in chunk(&page.body) {
        match parts.last_mut() {
            Some((start, end)) if estimate(&page.body[*start..p.end]) <= size => *end = p.end,
            _ => parts.push((p.start, p.end)),
        }
    }
    let total = parts.len().max(1) as u32;
    let part = a.part.unwrap_or(1);
    if part == 0 || part > total {
        return Err(invalid(format!("This page has {total} part(s).")));
    }
    let (start, end) = parts
        .get(part as usize - 1)
        .copied()
        .unwrap_or((0, page.body.len()));
    let markdown = &page.body[start..end];
    let text = forma_core::extract::extract(markdown).text;
    let (key, _) = book.add(SourceKind::Page, &id, &title, &[], start, &text);
    let block = crate::knowledge::source_block(
        &key,
        SourceKind::Page,
        &id,
        &title,
        &[],
        if markdown.trim().is_empty() {
            "(This page is empty.)"
        } else {
            markdown
        },
    );
    let label = if total > 1 {
        format!("Read {} (part {part} of {total})", quoted(&title))
    } else {
        format!("Read {}", quoted(&title))
    };
    Ok((format!("Part {part} of {total}.\n\n{block}"), label))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListArgs {
    parent_id: Option<String>,
}

fn list_pages(
    view: ReadView<'_>,
    value: &Value,
    _book: &mut SourceBook,
    _budget: u32,
) -> Result<(String, String), ToolError> {
    let a: ListArgs = args(value)?;
    let parent = match a.parent_id.as_deref().filter(|p| !p.trim().is_empty()) {
        Some(id) => Some(page_id(view, id)?),
        None => None,
    };
    let children = view
        .children(parent.as_ref().map(|(id, _)| id.as_str()))
        .map_err(|e| invalid(format!("The pages can't be listed: {e}.")))?;
    let truncated = children.len() > MAX_LISTED;
    let pages: Vec<Value> = children
        .iter()
        .take(MAX_LISTED)
        .map(|p| {
            json!({ "page_id": p.id, "title": p.title, "has_subpages": p.has_children,
                    "is_collection": p.is_collection })
        })
        .collect();
    let label = match &parent {
        Some((_, title)) => format!("Listed the pages in {}", quoted(title)),
        None => "Listed the top-level pages".to_string(),
    };
    Ok((
        json!({ "pages": pages, "truncated": truncated }).to_string(),
        label,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MetadataArgs {
    page_id: String,
}

fn get_page_metadata(
    view: ReadView<'_>,
    value: &Value,
    _book: &mut SourceBook,
    _budget: u32,
) -> Result<(String, String), ToolError> {
    let a: MetadataArgs = args(value)?;
    let (id, title) = page_id(view, &a.page_id)?;
    let page = view
        .page(&id)
        .map_err(|e| invalid(format!("The page can't be read: {e}.")))?;
    let path: Vec<&str> = page.ancestors.iter().map(|a| a.title.as_str()).collect();
    Ok((
        json!({
            "page_id": page.id,
            "title": page.title,
            "path": path,
            "parent_id": page.parent_id,
            "tags": page.tags,
            "properties": view.properties(&id),
            "created_at": page.created_at,
            "updated_at": page.updated_at,
            "is_collection": page.is_collection,
            "collection_id": page.collection_id,
            "has_subpages": !view.children(Some(&id)).unwrap_or_default().is_empty(),
            "revision": page.body_rev,
        })
        .to_string(),
        format!("Looked up {}", quoted(&title)),
    ))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use forma_core::workspace::Workspace;

    use super::*;

    fn workspace() -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Glaciers.md"),
            "---\ntitle: Glaciers\ntags: [earth]\nStatus: Draft\n---\n# Ice\n\nGlaciers carve valleys.\n",
        )
        .unwrap();
        std::fs::create_dir(dir.path().join("Trips")).unwrap();
        std::fs::write(dir.path().join("Trips/index.md"), "Travel notes.").unwrap();
        std::fs::write(
            dir.path().join("Trips/Kyoto.md"),
            format!(
                "Temples.\n\n{}",
                "Long walk through the city. ".repeat(2000)
            ),
        )
        .unwrap();
        let ws = Workspace::open(dir.path()).unwrap();
        (dir, ws)
    }

    fn id(ws: &Workspace, title: &str) -> String {
        let mut stack: Vec<Option<String>> = vec![None];
        while let Some(parent) = stack.pop() {
            for p in ws.list_children(parent.as_deref()).unwrap() {
                if p.title == title {
                    return p.id;
                }
                stack.push(Some(p.id));
            }
        }
        panic!("no page {title}")
    }

    fn call(ws: &Workspace, name: &str, args: Value) -> (ToolOutput, SourceBook) {
        let mut book = SourceBook::new(1);
        let out = run(ws.read_view(), name, &args.to_string(), &mut book, 3000);
        (out, book)
    }

    #[test]
    fn ask_tools_only_get_a_read_view() {
        // The registry's type: every Ask tool is a ReadTool, and a ReadTool's
        // only way to the workspace is the ReadView its `run` receives. A
        // tool with a mutation handle wouldn't fit this type.
        let runs: [Run; 4] = ASK_TOOLS.map(|t| t.run);
        assert_eq!(runs.len(), 4);
        let names: Vec<_> = ASK_TOOLS.iter().map(|t| t.name).collect();
        assert_eq!(
            names,
            [
                "search_pages",
                "read_page",
                "list_pages",
                "get_page_metadata"
            ]
        );
        for spec in specs() {
            assert_eq!(spec.parameters["type"], "object");
            assert_eq!(spec.parameters["additionalProperties"], false);
        }
    }

    #[test]
    fn search_returns_cited_passages() {
        let (_dir, ws) = workspace();
        let (out, book) = call(&ws, "search_pages", json!({"query": "glaciers valleys"}));
        assert!(!out.is_error, "{}", out.content);
        assert!(
            out.content
                .contains("<source key=\"S1\" kind=\"passage\" page=\"Glaciers\"")
        );
        assert!(out.content.contains("Glaciers carve valleys."));
        assert_eq!(
            out.activity.label,
            "Searched the notes for “glaciers valleys”"
        );
        assert_eq!(book.sources.len(), 1);
        let (out, _) = call(&ws, "search_pages", json!({"query": "volcano"}));
        assert!(out.content.contains("Nothing in the notes matched"));
    }

    #[test]
    fn long_pages_are_read_in_parts() {
        let (_dir, ws) = workspace();
        let kyoto = id(&ws, "Kyoto");
        let (out, book) = call(&ws, "read_page", json!({"page_id": kyoto}));
        let parts: u32 = out.content["Part 1 of ".len()..]
            .split('.')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(parts >= 3, "{parts}");
        assert!(out.content.contains("Temples."));
        assert!(estimate(&out.content) <= MAX_RESULT_TOKENS);
        assert!(out.activity.label.starts_with("Read “Kyoto” (part 1 of"));
        assert_eq!(book.sources[0].kind, SourceKind::Page);

        let (out, _) = call(&ws, "read_page", json!({"page_id": kyoto, "part": 2}));
        assert!(!out.content.contains("Temples."));
        let (out, _) = call(
            &ws,
            "read_page",
            json!({"page_id": kyoto, "part": parts + 1}),
        );
        assert!(out.is_error);
    }

    #[test]
    fn lists_and_metadata() {
        let (_dir, ws) = workspace();
        let (out, _) = call(&ws, "list_pages", json!({}));
        assert!(out.content.contains("\"title\":\"Glaciers\""));
        assert!(out.content.contains("\"has_subpages\":true"));
        let trips = id(&ws, "Trips");
        let (out, _) = call(&ws, "list_pages", json!({"parent_id": trips}));
        assert!(out.content.contains("Kyoto"));
        assert_eq!(out.activity.label, "Listed the pages in “Trips”");

        let (out, _) = call(
            &ws,
            "get_page_metadata",
            json!({"page_id": id(&ws, "Glaciers")}),
        );
        let meta: Value = serde_json::from_str(&out.content).unwrap();
        assert_eq!(meta["tags"], json!(["earth"]));
        assert_eq!(meta["properties"], json!({"Status": "Draft"}));
        assert!(meta["revision"].is_string());
    }

    #[test]
    fn bad_arguments_are_tool_errors() {
        let (_dir, ws) = workspace();
        for (name, args) in [
            ("search_pages", json!({})),
            ("search_pages", json!({"query": "  "})),
            ("search_pages", json!({"query": "x", "limit": 0})),
            ("search_pages", json!({"query": "x", "limit": 11})),
            ("search_pages", json!({"query": "x".repeat(501)})),
            ("search_pages", json!({"query": "x", "path": "/etc"})),
            ("read_page", json!({"page_id": "../../etc/passwd"})),
            ("read_page", json!({"page_id": "Glaciers.md"})),
            (
                "read_page",
                json!({"page_id": "0c6a5f3e-6c1d-4b8e-9a63-2f1d1b0a9e77"}),
            ),
            ("read_page", json!({"page_id": 7})),
            ("list_pages", json!({"parent_id": "/Users"})),
            ("get_page_metadata", json!({"page_id": null})),
            ("delete_page", json!({"page_id": "x"})),
        ] {
            let (out, book) = call(&ws, name, args.clone());
            assert!(out.is_error, "{name} {args}: {}", out.content);
            assert!(out.activity.failed);
            assert!(serde_json::from_str::<Value>(&out.content).unwrap()["error"].is_string());
            assert!(book.sources.is_empty());
        }
        let mut book = SourceBook::new(1);
        let out = run(ws.read_view(), "search_pages", "{not json", &mut book, 3000);
        assert!(out.is_error);
    }

    /// Every file under `dir`, with a hash of its contents.
    fn snapshot(dir: &Path) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for item in std::fs::read_dir(&d).unwrap().flatten() {
                let path = item.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let bytes = std::fs::read(&path).unwrap();
                    out.push((path.display().to_string(), format!("{:x}", md5ish(&bytes))));
                }
            }
        }
        out.sort();
        out
    }

    fn md5ish(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf29ce484222325, |h, b| {
            (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
        })
    }

    /// A small deterministic generator, so a failure can be replayed.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
            &items[(self.next() % items.len() as u64) as usize]
        }
        fn value(&mut self, depth: u32, ids: &[String]) -> Value {
            let keys = [
                "query",
                "limit",
                "page_id",
                "part",
                "parent_id",
                "path",
                "extra",
            ];
            self.value_with(depth, ids, &keys)
        }
        fn value_with(&mut self, depth: u32, ids: &[String], keys: &[&str]) -> Value {
            let strings = [
                "",
                " ",
                "glaciers",
                "../",
                "../../.forma/workspace.json",
                "/etc/passwd",
                "~/.ssh/id_rsa",
                "C:\\Windows",
                "forma://page/x",
                "\u{0}",
                "%2e%2e/",
                "Trips/Kyoto.md",
                "注意力",
                "\"; DROP TABLE pages; --",
                "*",
                "NEAR(a b)",
                "a OR b AND",
                "\u{202e}rtl",
            ];
            match self.next() % if depth > 2 { 6 } else { 8 } {
                0 => Value::Null,
                1 => json!(self.next().is_multiple_of(2)),
                2 => json!(self.next() as i64 - (u32::MAX as i64)),
                3 => json!(f64::from(self.next() as u32) / 7.0),
                4 => json!(self.pick(&strings).repeat((self.next() % 3 + 1) as usize)),
                5 => json!(self.pick(ids)),
                6 => Value::Array(
                    (0..self.next() % 4)
                        .map(|_| self.value_with(depth + 1, ids, keys))
                        .collect(),
                ),
                _ => {
                    let mut map = serde_json::Map::new();
                    for _ in 0..self.next() % 4 {
                        map.insert(
                            (*self.pick(keys)).into(),
                            self.value_with(depth + 1, ids, keys),
                        );
                    }
                    Value::Object(map)
                }
            }
        }
    }

    #[test]
    fn fuzzed_arguments_never_panic_or_write() {
        let (dir, ws) = workspace();
        let before = snapshot(dir.path());
        let ids = vec![id(&ws, "Glaciers"), id(&ws, "Kyoto"), id(&ws, "Trips")];
        let mut rng = Rng(0x5eed_f0f0_1234_5678);
        let mut errors = 0;
        for _ in 0..3000 {
            let name = *rng.pick(&[
                "search_pages",
                "read_page",
                "list_pages",
                "get_page_metadata",
                "nope",
            ]);
            let args = rng.value(0, &ids);
            let text = if rng.next().is_multiple_of(10) {
                "{\"query\": ".to_string()
            } else {
                args.to_string()
            };
            let mut book = SourceBook::new(1);
            let budget = (rng.next() % 4000) as u32;
            let out = run(ws.read_view(), name, &text, &mut book, budget);
            errors += usize::from(out.is_error);
            assert!(
                estimate(&out.content) <= MAX_RESULT_TOKENS + 20,
                "{name} {text}"
            );
            assert!(!out.is_error || serde_json::from_str::<Value>(&out.content).is_ok());
            // A source always names a page in this workspace, and its key
            // was in what the model got.
            for source in &book.sources {
                assert!(
                    out.content.contains(&format!("key=\"{}\"", source.key)),
                    "{name} {text}: {} not sent",
                    source.key
                );
                assert!(
                    ids.contains(&source.page_id)
                        || ws.read_view().title(&source.page_id).is_some()
                );
            }
        }
        assert!(errors > 1000, "{errors}");
        assert_eq!(snapshot(dir.path()), before, "a read tool changed a file");
    }

    #[test]
    fn edit_tools_only_get_a_read_view_and_a_change_set() {
        // Every proposal tool's `run` has this type: the read-only view, its
        // arguments, and the change set, which is data in memory. Applying a
        // set needs `&mut Workspace`, which no tool is given.
        let runs: [Propose; 6] = PROPOSE_TOOLS.map(|t| t.run);
        assert_eq!(runs.len(), 6);
        let names: Vec<_> = PROPOSE_TOOLS.iter().map(|t| t.name).collect();
        assert_eq!(
            names,
            [
                "create_page",
                "edit_page",
                "move_page",
                "create_collection",
                "update_page_properties",
                "link_pages"
            ]
        );
        // Ask mode offers none of them, and none deletes anything.
        assert!(specs().iter().all(|s| !names.contains(&s.name.as_str())));
        assert_eq!(act_specs().len(), 10);
        assert!(
            act_specs()
                .iter()
                .all(|s| !s.name.contains("delete") && !s.name.contains("trash"))
        );
    }

    #[test]
    fn proposals_are_checked_as_they_are_made_and_write_nothing() {
        let (dir, ws) = workspace();
        let before = snapshot(dir.path());
        let view = ws.read_view();
        let mut set = ChangeSet::new("Organize", "test", "m");
        let (glaciers, trips) = (id(&ws, "Glaciers"), id(&ws, "Trips"));
        let propose = |set: &mut ChangeSet, name: &str, args: Value| {
            run_proposal(view, name, &args.to_string(), set).expect("a proposal tool")
        };
        let out = propose(
            &mut set,
            "create_page",
            json!({"title": "Ice ages", "parent_id": trips, "content": "Long ago."}),
        );
        assert!(!out.is_error, "{}", out.content);
        assert_eq!(out.activity.label, "Proposed a new page “Ice ages” (new-1)");
        assert!(out.content.contains("Nothing has changed yet"));
        assert!(
            out.content
                .contains("1. Create the page “Ice ages” in “Trips”")
        );
        let out = propose(
            &mut set,
            "edit_page",
            json!({"page_id": glaciers, "edits": [{"old": "carve valleys", "new": "shape valleys"}]}),
        );
        assert!(!out.is_error, "{}", out.content);
        let out = propose(
            &mut set,
            "move_page",
            json!({"page_id": glaciers, "new_parent_id": "new-1"}),
        );
        assert!(!out.is_error, "{}", out.content);
        assert_eq!(out.activity.label, "Proposed moving “Glaciers”");
        let out = propose(
            &mut set,
            "link_pages",
            json!({"source_id": "new-1", "target_id": glaciers}),
        );
        assert!(!out.is_error, "{}", out.content);
        assert!(run_proposal(view, "search_pages", "{}", &mut set).is_none());

        for (name, args) in [
            (
                "edit_page",
                json!({"page_id": "../Glaciers.md", "replace_all_body": ""}),
            ),
            (
                "edit_page",
                json!({"page_id": glaciers, "edits": [{"old": "nope", "new": "x"}]}),
            ),
            ("edit_page", json!({"page_id": glaciers})),
            (
                "edit_page",
                json!({"page_id": glaciers, "edits": [], "replace_all_body": "x"}),
            ),
            (
                "move_page",
                json!({"page_id": glaciers, "new_parent_id": "/tmp"}),
            ),
            (
                "move_page",
                json!({"page_id": trips, "new_parent_id": "new-1"}),
            ),
            ("create_page", json!({"title": ""})),
            ("create_page", json!({"title": "x", "path": "/etc"})),
            (
                "create_collection",
                json!({"title": "C", "properties": [{"name": "S", "type": "colour"}]}),
            ),
            (
                "update_page_properties",
                json!({"page_id": glaciers, "set": {"Status": "x"}}),
            ),
            (
                "link_pages",
                json!({"source_id": glaciers, "target_id": glaciers}),
            ),
            (
                "link_pages",
                json!({"source_id": glaciers, "target_id": "new-1"}),
            ),
        ] {
            let out = propose(&mut set, name, args.clone());
            assert!(out.is_error, "{name} {args}: {}", out.content);
            assert!(out.activity.failed);
        }
        assert_eq!(set.changes.len(), 3, "the create took the link");
        assert_eq!(snapshot(dir.path()), before, "proposing wrote a file");
    }

    #[test]
    fn fuzzed_proposals_never_panic_or_write() {
        let (dir, ws) = workspace();
        let before = snapshot(dir.path());
        let ids = vec![
            id(&ws, "Glaciers"),
            id(&ws, "Kyoto"),
            id(&ws, "Trips"),
            "new-1".into(),
            "new-2".into(),
        ];
        let keys = [
            "title",
            "content",
            "parent_id",
            "page_id",
            "edits",
            "old",
            "new",
            "replace_all_body",
            "new_parent_id",
            "properties",
            "name",
            "type",
            "options",
            "set",
            "unset",
            "source_id",
            "target_id",
            "text",
            "path",
        ];
        let mut rng = Rng(0x0ddb_a11c_afef_00d5);
        let mut set = ChangeSet::new("fuzz", "t", "m");
        let (mut errors, mut accepted) = (0, 0);
        for _ in 0..3000 {
            let name = *rng.pick(&[
                "create_page",
                "edit_page",
                "move_page",
                "create_collection",
                "update_page_properties",
                "link_pages",
            ]);
            // Half the calls have the tool's own fields, with random values.
            let args = if rng.next().is_multiple_of(2) {
                rng.value_with(0, &ids, &keys)
            } else {
                let fields: &[&str] = match name {
                    "create_page" => &["title", "parent_id", "content"],
                    "edit_page" => &["page_id", "replace_all_body"],
                    "move_page" => &["page_id", "new_parent_id"],
                    "create_collection" => &["title", "properties"],
                    "update_page_properties" => &["page_id", "set"],
                    _ => &["source_id", "target_id", "text"],
                };
                let mut map = serde_json::Map::new();
                for field in fields {
                    let value = match rng.next() % 3 {
                        0 => json!(rng.pick(&ids)),
                        1 => json!(*rng.pick(&["Ice", "glaciers", "../x", "注意力", "", "new-1"])),
                        _ => rng.value_with(1, &ids, &keys),
                    };
                    map.insert((*field).into(), value);
                }
                Value::Object(map)
            };
            let out = run_proposal(ws.read_view(), name, &args.to_string(), &mut set).unwrap();
            if out.is_error {
                errors += 1;
                assert!(serde_json::from_str::<Value>(&out.content).unwrap()["error"].is_string());
            } else {
                accepted += 1;
            }
            assert!(set.changes.len() <= forma_core::workspace::changeset::MAX_CHANGES);
            assert!(estimate(&out.content) <= MAX_RESULT_TOKENS + 20);
        }
        assert!(errors > 1000, "{errors}");
        assert!(accepted > 20, "{accepted}");
        // Whatever got through still previews, and nothing was written.
        let preview = set.preview(ws.read_view());
        assert_eq!(preview.changes.len(), set.changes.len());
        println!("fuzz: {accepted} proposals accepted, {errors} refused");
        assert_eq!(
            snapshot(dir.path()),
            before,
            "a proposal tool changed a file"
        );
    }
}
