//! AI change proposals (ARCHITECTURE §10.3, ADR-021, ADR-043).
//!
//! A `ChangeSet` is built while the assistant works. Each proposal is
//! checked when it's made, against a read-only view, and nothing is written
//! then. Applying a set is a separate command that only the user's approval
//! reaches: every chosen change is checked against the page as it is now,
//! and then all of them are written in one transaction, or none are. Each
//! application is recorded in `.diaspar/operations/`, from which it can be
//! undone; an undo is recorded too, so it can be redone.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::time::Duration;

use pulldown_cmark::{Event, Options, Parser, Tag};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use similar::{ChangeTag, TextDiff};
use ts_rs::TS;

use crate::collection::{CellInput, PropertyType, check_option_name, check_property_name, is_date};
use crate::error::{CoreError, CoreResult, io_at};
use crate::extract::PAGE_LINK_PREFIX;

use super::collections::SchemaOp;
use super::read::ReadView;
use super::scan::{hash_rev, new_id};
use super::state::{FORMAT_VERSION, load_json};
use super::tx::Action;
use super::types::{PageKind, PageStatus};
use super::{Workspace, now_rfc3339, validate_uuid};

/// Changes one set may hold.
pub const MAX_CHANGES: usize = 50;
/// The longest page body a change may write.
pub const MAX_BODY_BYTES: usize = 500_000;
/// All the text one set holds, so a runaway model can't fill memory.
pub const MAX_SET_BYTES: usize = 5_000_000;
const MAX_TITLE_CHARS: usize = 200;
const MAX_PROPERTIES: usize = 30;
const MAX_OPTIONS: usize = 50;
/// Operations kept in `.diaspar/operations/` (DATA_MODEL §7.5).
pub const KEEP_OPERATIONS: usize = 100;
/// How the assistant names a page proposed in the same set: `new-3`.
pub const NEW_PAGE_PREFIX: &str = "new-";

// ------------------------------------------------------------------ model

/// A page a change refers to: one that exists, or one an earlier change in
/// the same set creates (by that change's ID).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Existing(String),
    New(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewProperty {
    pub name: String,
    pub kind: PropertyType,
    pub options: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// A page, or with `collection` a collection, with its body and, as an
    /// entry of a collection, its property values (by property name).
    CreatePage {
        parent: Option<Target>,
        title: String,
        body: String,
        collection: Option<Vec<NewProperty>>,
        values: Vec<(String, CellInput)>,
    },
    EditPage {
        page_id: String,
        base_body_rev: String,
        old_body: String,
        body: String,
    },
    SetProperties {
        page_id: String,
        base_rev: String,
        values: Vec<(String, CellInput)>,
    },
    MovePage {
        page_id: String,
        from_parent: Option<String>,
        to_parent: Option<Target>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProposedChange {
    pub id: String,
    pub change: Change,
}

/// A body edit: exact replacements (each `old` must occur exactly once), or
/// a whole new body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Edit {
    Replace(Vec<(String, String)>),
    Body(String),
}

/// Proposed changes, held in memory until the user applies or rejects them.
/// It refers to the workspace only by page IDs, so holding one can't change
/// anything.
#[derive(Debug, Clone, PartialEq)]
pub struct ChangeSet {
    pub id: String,
    /// What the user asked for.
    pub request: String,
    pub provider: String,
    pub model: String,
    pub changes: Vec<ProposedChange>,
    next: u32,
}

/// Property names and kinds a value is checked against.
struct Columns(Vec<(String, PropertyType, Vec<String>)>);

fn short(text: &str) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let cut: String = flat.chars().take(60).collect();
    if cut.len() < flat.len() {
        format!("{cut}…")
    } else {
        cut
    }
}

fn clean_title(title: &str) -> Result<String, String> {
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    if title.is_empty() {
        return Err("A page needs a title.".into());
    }
    if title.chars().count() > MAX_TITLE_CHARS {
        return Err(format!("Titles are at most {MAX_TITLE_CHARS} characters."));
    }
    Ok(title)
}

fn check_body(body: &str) -> Result<(), String> {
    if body.len() > MAX_BODY_BYTES {
        return Err(format!(
            "That's more than {} KB of text for one page.",
            MAX_BODY_BYTES / 1000
        ));
    }
    if body.contains('\0') {
        return Err("The text contains a NUL character.".into());
    }
    Ok(())
}

/// `[label](diaspar://page/<id>)` with the label's brackets escaped.
fn page_link(label: &str, id: &str) -> String {
    let label = label
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('\n', " ");
    format!("[{label}]({PAGE_LINK_PREFIX}{id})")
}

fn apply_edit(body: &str, edit: &Edit) -> Result<String, String> {
    let out = match edit {
        Edit::Body(new) => new.clone(),
        Edit::Replace(pairs) => {
            if pairs.is_empty() {
                return Err("Give at least one edit.".into());
            }
            let mut out = body.to_string();
            for (old, new) in pairs {
                if old.is_empty() {
                    return Err("Each edit needs the exact text it replaces.".into());
                }
                match out.matches(old.as_str()).count() {
                    0 => {
                        return Err(format!(
                            "“{}” isn't in the page. Copy the text to replace exactly, from read_page.",
                            short(old)
                        ));
                    }
                    1 => out = out.replacen(old.as_str(), new, 1),
                    n => {
                        return Err(format!(
                            "“{}” appears {n} times; include more of the text around it.",
                            short(old)
                        ));
                    }
                }
            }
            out
        }
    };
    check_body(&out)?;
    Ok(out)
}

/// The value the assistant gave, as the property's kind needs it.
fn cell(kind: PropertyType, options: &[String], value: &Json) -> Result<CellInput, String> {
    let option = |name: &str| {
        options
            .iter()
            .find(|o| o.eq_ignore_ascii_case(name.trim()))
            .cloned()
            .ok_or_else(|| {
                format!(
                    "“{}” isn't an option. The options are: {}.",
                    short(name),
                    options.join(", ")
                )
            })
    };
    Ok(match (kind, value) {
        (_, Json::Null) => CellInput::Empty,
        (PropertyType::Text, Json::String(s)) => CellInput::Text { value: s.clone() },
        (PropertyType::Text, Json::Number(n)) => CellInput::Text {
            value: n.to_string(),
        },
        (PropertyType::Number, Json::Number(n)) => CellInput::Number {
            value: n.as_f64().ok_or("That isn't a number.")?,
        },
        (PropertyType::Number, Json::String(s)) => CellInput::Number {
            value: s
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite())
                .ok_or("That isn't a number.")?,
        },
        (PropertyType::Date, Json::String(s)) if is_date(s.trim()) => CellInput::Date {
            value: s.trim().to_string(),
        },
        (PropertyType::Date, _) => return Err("Dates are written as YYYY-MM-DD.".into()),
        (PropertyType::Checkbox, Json::Bool(b)) => CellInput::Checkbox { value: *b },
        (PropertyType::Select, Json::String(s)) => CellInput::Select { value: option(s)? },
        (PropertyType::MultiSelect, Json::String(s)) => CellInput::MultiSelect {
            values: vec![option(s)?],
        },
        (PropertyType::MultiSelect, Json::Array(items)) => CellInput::MultiSelect {
            values: items
                .iter()
                .map(|i| {
                    i.as_str()
                        .ok_or("Options are names.".to_string())
                        .and_then(option)
                })
                .collect::<Result<_, _>>()?,
        },
        _ => return Err("That value doesn't fit the property's type.".into()),
    })
}

fn display(input: &CellInput) -> Option<String> {
    match input {
        CellInput::Empty => None,
        CellInput::Text { value } | CellInput::Date { value } | CellInput::Select { value } => {
            Some(value.clone())
        }
        CellInput::Number { value } => Some(if value.fract() == 0.0 && value.abs() < 1e15 {
            format!("{}", *value as i64)
        } else {
            value.to_string()
        }),
        CellInput::Checkbox { value } => Some(if *value { "Yes" } else { "No" }.into()),
        CellInput::MultiSelect { values } => Some(values.join(", ")),
    }
}

fn display_json(value: &Json) -> Option<String> {
    match value {
        Json::Null => None,
        Json::String(s) => Some(s.clone()),
        Json::Bool(b) => Some(if *b { "Yes" } else { "No" }.into()),
        Json::Array(items) => Some(
            items
                .iter()
                .filter_map(display_json)
                .collect::<Vec<_>>()
                .join(", "),
        ),
        other => Some(other.to_string()),
    }
}

impl ChangeSet {
    pub fn new(request: &str, provider: &str, model: &str) -> Self {
        Self {
            id: new_id(),
            request: request.to_string(),
            provider: provider.to_string(),
            model: model.to_string(),
            changes: Vec::new(),
            next: 1,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    /// The title of the page change `change_id` creates.
    pub fn preview_title(&self, change_id: &str) -> Option<String> {
        self.changes.iter().find_map(|c| match &c.change {
            Change::CreatePage { title, .. } if c.id == change_id => Some(title.clone()),
            _ => None,
        })
    }

    /// The text the set holds, old bodies included.
    fn bytes(&self) -> usize {
        self.changes
            .iter()
            .map(|c| match &c.change {
                Change::CreatePage { body, title, .. } => body.len() + title.len(),
                Change::EditPage { old_body, body, .. } => old_body.len() + body.len(),
                _ => 0,
            })
            .sum()
    }

    fn check_size(&self, adding: usize) -> Result<(), String> {
        if self.bytes() + adding > MAX_SET_BYTES {
            return Err(format!(
                "This proposal is already {} MB of text; propose the rest in another request.",
                MAX_SET_BYTES / 1_000_000
            ));
        }
        Ok(())
    }

    fn push(&mut self, change: Change) -> Result<String, String> {
        if self.changes.len() >= MAX_CHANGES {
            return Err(format!("One proposal holds at most {MAX_CHANGES} changes."));
        }
        let id = self.next.to_string();
        self.next += 1;
        self.changes.push(ProposedChange {
            id: id.clone(),
            change,
        });
        Ok(id)
    }

    fn change_mut(&mut self, id: &str) -> Option<&mut Change> {
        self.changes
            .iter_mut()
            .find(|c| c.id == id)
            .map(|c| &mut c.change)
    }

    /// A page ID from the assistant: a UUID in this workspace, or `new-N`
    /// for a page created by change N of this set.
    pub fn target(&self, view: ReadView<'_>, id: &str) -> Result<Target, String> {
        let id = id.trim();
        if let Some(n) = id.strip_prefix(NEW_PAGE_PREFIX) {
            return self
                .changes
                .iter()
                .any(|c| c.id == n && matches!(c.change, Change::CreatePage { .. }))
                .then(|| Target::New(n.to_string()))
                .ok_or_else(|| format!("{id:?} isn't a page proposed in this set."));
        }
        let lower = id.to_ascii_lowercase();
        if uuid::Uuid::parse_str(&lower).is_err() {
            return Err(format!(
                "{id:?} isn't a page ID. Use an ID from search_pages or list_pages, or new-N for a page proposed here."
            ));
        }
        view.title(&lower)
            .map(|_| Target::Existing(lower))
            .ok_or_else(|| "No page has that ID.".into())
    }

    fn target_opt(&self, view: ReadView<'_>, id: Option<&str>) -> Result<Option<Target>, String> {
        id.filter(|p| !p.trim().is_empty())
            .map(|p| self.target(view, p))
            .transpose()
    }

    /// The body as the set would leave it.
    fn current_body(&self, view: ReadView<'_>, target: &Target) -> Result<String, String> {
        for c in &self.changes {
            match (&c.change, target) {
                (Change::CreatePage { body, .. }, Target::New(id)) if c.id == *id => {
                    return Ok(body.clone());
                }
                (Change::EditPage { page_id, body, .. }, Target::Existing(id)) if page_id == id => {
                    return Ok(body.clone());
                }
                _ => {}
            }
        }
        match target {
            Target::Existing(id) => view.page(id).map(|p| p.body).map_err(|e| e.to_string()),
            Target::New(_) => Err("That page isn't proposed in this set.".into()),
        }
    }

    pub fn propose_page(
        &mut self,
        view: ReadView<'_>,
        parent: Option<&str>,
        title: &str,
        body: &str,
    ) -> Result<String, String> {
        let parent = self.target_opt(view, parent)?;
        let title = clean_title(title)?;
        check_body(body)?;
        self.check_size(body.len())?;
        let id = self.push(Change::CreatePage {
            parent,
            title,
            body: body.to_string(),
            collection: None,
            values: Vec::new(),
        })?;
        Ok(format!("{NEW_PAGE_PREFIX}{id}"))
    }

    pub fn propose_collection(
        &mut self,
        view: ReadView<'_>,
        parent: Option<&str>,
        title: &str,
        properties: &[NewProperty],
    ) -> Result<String, String> {
        let parent = self.target_opt(view, parent)?;
        let title = clean_title(title)?;
        if properties.len() > MAX_PROPERTIES {
            return Err(format!(
                "A collection has at most {MAX_PROPERTIES} properties."
            ));
        }
        let mut checked: Vec<NewProperty> = Vec::new();
        for p in properties {
            let names: Vec<&str> = checked.iter().map(|c| c.name.as_str()).collect();
            let name = check_property_name(&p.name, &names)?;
            if !p.kind.has_options() && !p.options.is_empty() {
                return Err(format!(
                    "“{name}” isn't a select property, so it has no options."
                ));
            }
            if p.options.len() > MAX_OPTIONS {
                return Err(format!("A property has at most {MAX_OPTIONS} options."));
            }
            let mut options: Vec<String> = Vec::new();
            for o in &p.options {
                let taken: Vec<&str> = options.iter().map(String::as_str).collect();
                options.push(check_option_name(o, &taken)?);
            }
            checked.push(NewProperty {
                name,
                kind: p.kind,
                options,
            });
        }
        let id = self.push(Change::CreatePage {
            parent,
            title,
            body: String::new(),
            collection: Some(checked),
            values: Vec::new(),
        })?;
        Ok(format!("{NEW_PAGE_PREFIX}{id}"))
    }

    pub fn propose_edit(
        &mut self,
        view: ReadView<'_>,
        page: &str,
        edit: &Edit,
    ) -> Result<(), String> {
        let target = self.target(view, page)?;
        let current = self.current_body(view, &target)?;
        let body = apply_edit(&current, edit)?;
        if body == current {
            return Err("That leaves the page as it is.".into());
        }
        // An edit of a page already in the set replaces its proposed text;
        // a first edit adds the old text too.
        let first_edit = matches!(&target, Target::Existing(id) if !self.changes.iter().any(
            |c| matches!(&c.change, Change::EditPage { page_id, .. } if page_id == id)
        ));
        let adding = body.len().saturating_sub(current.len())
            + if first_edit {
                body.len() + current.len()
            } else {
                0
            };
        self.check_size(adding)?;
        match target {
            Target::New(id) => {
                if let Some(Change::CreatePage { body: b, .. }) = self.change_mut(&id) {
                    *b = body;
                }
            }
            Target::Existing(id) => {
                let existing = self.changes.iter_mut().find_map(|c| match &mut c.change {
                    Change::EditPage { page_id, body, .. } if *page_id == id => Some(body),
                    _ => None,
                });
                if let Some(b) = existing {
                    *b = body;
                    return Ok(());
                }
                let page = view.page(&id).map_err(|e| e.to_string())?;
                if let PageStatus::InvalidFrontmatter { .. } = page.status {
                    return Err(
                        "This page's frontmatter can't be read, so Diaspar won't edit it.".into(),
                    );
                }
                if page.kind == PageKind::Folder {
                    return Err(
                        "That's a folder; it has no text. The user can turn it into a page first."
                            .into(),
                    );
                }
                self.push(Change::EditPage {
                    page_id: id,
                    base_body_rev: page.body_rev,
                    old_body: page.body,
                    body,
                })?;
            }
        }
        Ok(())
    }

    /// Inserts a page link to `target` in `source`: around `text` if given
    /// (it must occur exactly once), else as a new last paragraph.
    pub fn propose_link(
        &mut self,
        view: ReadView<'_>,
        source: &str,
        target: &str,
        text: Option<&str>,
    ) -> Result<(), String> {
        let Target::Existing(target_id) = self.target(view, target)? else {
            return Err("Link to a page proposed here once it exists, in a later request.".into());
        };
        let source_target = self.target(view, source)?;
        if source_target == Target::Existing(target_id.clone()) {
            return Err("A page can't link to itself.".into());
        }
        if view
            .page(&target_id)
            .is_ok_and(|p| p.kind == PageKind::Folder)
        {
            return Err("That's a folder; folders can't be linked to (ADR-047).".into());
        }
        let title = view.title(&target_id).unwrap_or_default();
        let edit = match text.map(str::trim).filter(|t| !t.is_empty()) {
            Some(text) => Edit::Replace(vec![(text.to_string(), page_link(text, &target_id))]),
            None => {
                let body = self.current_body(view, &source_target)?;
                Edit::Body(format!(
                    "{}\n\n{}\n",
                    body.trim_end(),
                    page_link(&title, &target_id)
                ))
            }
        };
        self.propose_edit(view, source, &edit)
    }

    /// The columns a page's values are checked against: its collection's
    /// schema, or the proposed one.
    fn columns(&self, view: ReadView<'_>, parent: Option<&Target>) -> Result<Columns, String> {
        let not_entry = || "This page isn't in a collection.".to_string();
        match parent.ok_or_else(not_entry)? {
            Target::Existing(id) => {
                let schema = view.schema(id).ok_or_else(not_entry)?;
                Ok(Columns(
                    schema
                        .properties
                        .into_iter()
                        .map(|p| {
                            (
                                p.name,
                                p.kind,
                                p.options.into_iter().map(|o| o.name).collect(),
                            )
                        })
                        .collect(),
                ))
            }
            Target::New(id) => self
                .changes
                .iter()
                .find_map(|c| match &c.change {
                    Change::CreatePage {
                        collection: Some(props),
                        ..
                    } if c.id == *id => Some(Columns(
                        props
                            .iter()
                            .map(|p| (p.name.clone(), p.kind, p.options.clone()))
                            .collect(),
                    )),
                    _ => None,
                })
                .ok_or_else(not_entry),
        }
    }

    pub fn propose_properties(
        &mut self,
        view: ReadView<'_>,
        page: &str,
        set: &serde_json::Map<String, Json>,
        unset: &[String],
    ) -> Result<(), String> {
        if set.is_empty() && unset.is_empty() {
            return Err("Give at least one property to set or clear.".into());
        }
        let target = self.target(view, page)?;
        let parent = match &target {
            Target::Existing(id) => view
                .page(id)
                .map_err(|e| e.to_string())?
                .collection_id
                .map(Target::Existing),
            Target::New(id) => self.changes.iter().find_map(|c| match &c.change {
                Change::CreatePage { parent, .. } if c.id == *id => parent.clone(),
                _ => None,
            }),
        };
        let columns = self.columns(view, parent.as_ref())?;
        let mut values = Vec::new();
        let unset_values = unset.iter().map(|name| (name, &Json::Null));
        for (name, value) in set.iter().chain(unset_values) {
            let Some((canonical, kind, options)) = columns
                .0
                .iter()
                .find(|(n, _, _)| n.eq_ignore_ascii_case(name.trim()))
            else {
                let names: Vec<&str> = columns.0.iter().map(|(n, _, _)| n.as_str()).collect();
                return Err(format!(
                    "There's no property “{}”. The properties are: {}.",
                    short(name),
                    names.join(", ")
                ));
            };
            let input = cell(*kind, options, value).map_err(|e| format!("{canonical}: {e}"))?;
            values.push((canonical.clone(), input));
        }
        let merge = |into: &mut Vec<(String, CellInput)>| {
            for (name, input) in values.clone() {
                into.retain(|(n, _)| *n != name);
                into.push((name, input));
            }
        };
        match target {
            Target::New(id) => {
                if let Some(Change::CreatePage { values: v, .. }) = self.change_mut(&id) {
                    merge(v);
                }
            }
            Target::Existing(id) => {
                let existing = self.changes.iter_mut().find_map(|c| match &mut c.change {
                    Change::SetProperties {
                        page_id, values, ..
                    } if *page_id == id => Some(values),
                    _ => None,
                });
                if let Some(v) = existing {
                    merge(v);
                    return Ok(());
                }
                let page = view.page(&id).map_err(|e| e.to_string())?;
                let base_rev = page.rev.ok_or("This page has no file to hold values.")?;
                self.push(Change::SetProperties {
                    page_id: id,
                    base_rev,
                    values,
                })?;
            }
        }
        Ok(())
    }

    /// Whether `inner` is `page` or lies inside it, counting pages this set creates.
    fn inside(&self, view: ReadView<'_>, inner: &Target, page: &str) -> bool {
        match inner {
            Target::Existing(id) => id == page || view.ancestors(id).iter().any(|a| a.id == page),
            Target::New(id) => self
                .changes
                .iter()
                .find_map(|c| match &c.change {
                    Change::CreatePage { parent, .. } if c.id == *id => Some(parent.clone()),
                    _ => None,
                })
                .flatten()
                .is_some_and(|p| self.inside(view, &p, page)),
        }
    }

    pub fn propose_move(
        &mut self,
        view: ReadView<'_>,
        page: &str,
        new_parent: Option<&str>,
    ) -> Result<(), String> {
        let target = self.target(view, page)?;
        let to = self.target_opt(view, new_parent)?;
        match target {
            Target::New(id) => {
                if to.as_ref() == Some(&Target::New(id.clone()))
                    || to.as_ref().is_some_and(|t| self.new_page_inside(t, &id))
                {
                    return Err("A page can't go inside itself.".into());
                }
                if let Some(Change::CreatePage { parent, .. }) = self.change_mut(&id) {
                    *parent = to;
                }
            }
            Target::Existing(id) => {
                if to.as_ref().is_some_and(|t| self.inside(view, t, &id)) {
                    return Err(
                        "A page can't be moved into itself or into one of its sub-pages.".into(),
                    );
                }
                let from = view.page(&id).map_err(|e| e.to_string())?.parent_id;
                if let Some(i) = self.changes.iter().position(
                    |c| matches!(&c.change, Change::MovePage { page_id, .. } if *page_id == id),
                ) {
                    if to == from.clone().map(Target::Existing) {
                        self.changes.remove(i);
                    } else if let Change::MovePage { to_parent, .. } = &mut self.changes[i].change {
                        *to_parent = to;
                    }
                    return Ok(());
                }
                if to == from.clone().map(Target::Existing) {
                    return Err("The page is already there.".into());
                }
                self.push(Change::MovePage {
                    page_id: id,
                    from_parent: from,
                    to_parent: to,
                })?;
            }
        }
        Ok(())
    }

    /// Whether the new page `target` lies inside the new page `id`.
    fn new_page_inside(&self, target: &Target, id: &str) -> bool {
        let mut current = target.clone();
        for _ in 0..=self.changes.len() {
            let Target::New(n) = &current else {
                return false;
            };
            if n == id {
                return true;
            }
            let parent = self.changes.iter().find_map(|c| match &c.change {
                Change::CreatePage { parent, .. } if c.id == *n => Some(parent.clone()),
                _ => None,
            });
            match parent.flatten() {
                Some(p) => current = p,
                None => return false,
            }
        }
        false
    }

    /// The set in a few lines, for the assistant to check what it proposed.
    pub fn describe(&self, view: ReadView<'_>) -> String {
        if self.changes.is_empty() {
            return "No changes proposed yet.".into();
        }
        let mut lines =
            vec!["Proposed so far (the user reviews these before anything changes):".to_string()];
        for c in &self.changes {
            lines.push(format!("{}. {}", c.id, self.summary_of(view, &c.change)));
        }
        lines.join("\n")
    }

    fn title_of(&self, view: ReadView<'_>, target: &Target) -> String {
        match target {
            Target::Existing(id) => view.title(id).unwrap_or_else(|| "a removed page".into()),
            Target::New(id) => self
                .changes
                .iter()
                .find_map(|c| match &c.change {
                    Change::CreatePage { title, .. } if c.id == *id => Some(title.clone()),
                    _ => None,
                })
                .unwrap_or_default(),
        }
    }

    fn summary_of(&self, view: ReadView<'_>, change: &Change) -> String {
        let place = |parent: &Option<Target>| match parent {
            Some(p) => format!("in “{}”", self.title_of(view, p)),
            None => "at the top level".into(),
        };
        match change {
            Change::CreatePage {
                parent,
                title,
                collection,
                ..
            } => format!(
                "Create {} “{title}” {}",
                if collection.is_some() {
                    "the collection"
                } else {
                    "the page"
                },
                place(parent)
            ),
            Change::EditPage { page_id, .. } => format!(
                "Edit “{}”",
                self.title_of(view, &Target::Existing(page_id.clone()))
            ),
            Change::SetProperties {
                page_id, values, ..
            } => format!(
                "Set {} on “{}”",
                values
                    .iter()
                    .map(|(n, _)| n.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
                self.title_of(view, &Target::Existing(page_id.clone()))
            ),
            Change::MovePage {
                page_id, to_parent, ..
            } => format!(
                "Move “{}” {}",
                self.title_of(view, &Target::Existing(page_id.clone())),
                match to_parent {
                    Some(p) => format!("into “{}”", self.title_of(view, p)),
                    None => "to the top level".into(),
                }
            ),
        }
    }
}

fn dependencies(change: &Change) -> Vec<String> {
    let parent = match change {
        Change::CreatePage { parent, .. } => parent,
        Change::MovePage { to_parent, .. } => to_parent,
        _ => &None,
    };
    match parent {
        Some(Target::New(id)) => vec![id.clone()],
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------- preview

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ChangeKind {
    CreatePage,
    CreateCollection,
    EditPage,
    SetProperties,
    MovePage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DiffKind {
    Equal,
    Insert,
    Delete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct DiffSpan {
    pub kind: DiffKind,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct PropertyChange {
    pub name: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ChangePreview {
    pub change_id: String,
    pub kind: ChangeKind,
    /// The page changed, or `None` for a page this set creates.
    pub page_id: Option<String>,
    pub title: String,
    pub summary: String,
    /// Where the page is, or will be: titles from the top.
    pub location: Vec<String>,
    /// For moves: where it goes.
    pub new_location: Option<Vec<String>>,
    /// The body, word by word.
    pub diff: Vec<DiffSpan>,
    pub properties: Vec<PropertyChange>,
    /// A new collection's properties, described.
    pub details: Vec<String>,
    /// Addresses of external images the change adds. Never loaded in the preview.
    pub external_images: Vec<String>,
    /// Changes this one needs, such as the one creating its parent.
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ChangeSetPreview {
    pub id: String,
    pub request: String,
    pub changes: Vec<ChangePreview>,
}

/// Word-level differences; CJK text is split per character.
pub fn word_diff(old: &str, new: &str) -> Vec<DiffSpan> {
    let diff = TextDiff::configure()
        .timeout(Duration::from_millis(500))
        .diff_unicode_words(old, new);
    let mut spans: Vec<DiffSpan> = Vec::new();
    for change in diff.iter_all_changes() {
        let kind = match change.tag() {
            ChangeTag::Equal => DiffKind::Equal,
            ChangeTag::Insert => DiffKind::Insert,
            ChangeTag::Delete => DiffKind::Delete,
        };
        match spans.last_mut() {
            Some(last) if last.kind == kind => last.text.push_str(change.value()),
            _ => spans.push(DiffSpan {
                kind,
                text: change.value().to_string(),
            }),
        }
    }
    spans
}

/// External (`http:`/`https:`) image addresses in a body.
pub fn external_images(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    for event in Parser::new_ext(body, Options::all()) {
        if let Event::Start(Tag::Image { dest_url, .. }) = event {
            let url = dest_url.trim();
            let lower = url.to_ascii_lowercase();
            if (lower.starts_with("https://")
                || lower.starts_with("http://")
                || lower.starts_with("//"))
                && !out.iter().any(|u| u == url)
            {
                out.push(url.to_string());
            }
        }
    }
    out
}

/// Pages `new` links to (and `old` didn't) whose IDs aren't in their files
/// yet: applying writes them, so the links keep working.
fn id_notes(view: ReadView<'_>, old: &str, new: &str) -> Vec<String> {
    let targets = |body: &str| {
        crate::extract::extract(body)
            .links
            .into_iter()
            .filter(|l| l.kind == crate::extract::LinkKind::Page)
            .map(|l| l.target)
            .collect::<Vec<_>>()
    };
    let before = targets(old);
    let mut notes = Vec::new();
    for target in targets(new) {
        if before.contains(&target) || view.id_persisted(&target) != Some(false) {
            continue;
        }
        let note = format!(
            "Also saves the page ID of “{}” in its file, so the link keeps working.",
            view.title(&target).unwrap_or_default()
        );
        if !notes.contains(&note) {
            notes.push(note);
        }
    }
    notes
}

fn added_images(old: &str, new: &str) -> Vec<String> {
    let before = external_images(old);
    external_images(new)
        .into_iter()
        .filter(|u| !before.contains(u))
        .collect()
}

impl ChangeSet {
    fn location(&self, view: ReadView<'_>, parent: &Option<Target>) -> Vec<String> {
        self.location_at(view, parent, 0)
    }

    fn location_at(
        &self,
        view: ReadView<'_>,
        parent: &Option<Target>,
        depth: usize,
    ) -> Vec<String> {
        if depth > self.changes.len() {
            return Vec::new();
        }
        match parent {
            None => Vec::new(),
            Some(Target::Existing(id)) => {
                let mut path: Vec<String> =
                    view.ancestors(id).into_iter().map(|a| a.title).collect();
                path.push(view.title(id).unwrap_or_default());
                path
            }
            Some(Target::New(id)) => {
                let parent = self.changes.iter().find_map(|c| match &c.change {
                    Change::CreatePage { parent, .. } if c.id == *id => Some(parent.clone()),
                    _ => None,
                });
                let mut path = match parent {
                    Some(p) => self.location_at(view, &p, depth + 1),
                    None => Vec::new(),
                };
                path.push(self.title_of(view, &Target::New(id.clone())));
                path
            }
        }
    }

    /// What the user reviews: each change with its word-level diff.
    pub fn preview(&self, view: ReadView<'_>) -> ChangeSetPreview {
        let changes = self
            .changes
            .iter()
            .map(|c| {
                let summary = self.summary_of(view, &c.change);
                let depends_on = dependencies(&c.change);
                let mut p = ChangePreview {
                    change_id: c.id.clone(),
                    kind: ChangeKind::EditPage,
                    page_id: None,
                    title: String::new(),
                    summary,
                    location: Vec::new(),
                    new_location: None,
                    diff: Vec::new(),
                    properties: Vec::new(),
                    details: Vec::new(),
                    external_images: Vec::new(),
                    depends_on,
                };
                match &c.change {
                    Change::CreatePage {
                        parent,
                        title,
                        body,
                        collection,
                        values,
                    } => {
                        p.kind = if collection.is_some() {
                            ChangeKind::CreateCollection
                        } else {
                            ChangeKind::CreatePage
                        };
                        p.title = title.clone();
                        p.location = self.location(view, parent);
                        if !body.is_empty() {
                            p.diff = vec![DiffSpan {
                                kind: DiffKind::Insert,
                                text: body.clone(),
                            }];
                        }
                        p.external_images = external_images(body);
                        p.details = id_notes(view, "", body);
                        p.properties = values
                            .iter()
                            .map(|(name, input)| PropertyChange {
                                name: name.clone(),
                                before: None,
                                after: display(input),
                            })
                            .collect();
                        for prop in collection.iter().flatten() {
                            let kind = serde_json::to_value(prop.kind)
                                .ok()
                                .and_then(|v| v.as_str().map(str::to_owned))
                                .unwrap_or_default()
                                .replace('_', " ");
                            p.details.push(if prop.options.is_empty() {
                                format!("{} ({kind})", prop.name)
                            } else {
                                format!("{} ({kind}: {})", prop.name, prop.options.join(", "))
                            });
                        }
                    }
                    Change::EditPage {
                        page_id,
                        old_body,
                        body,
                        ..
                    } => {
                        p.kind = ChangeKind::EditPage;
                        p.page_id = Some(page_id.clone());
                        p.title = view.title(page_id).unwrap_or_default();
                        p.location = view
                            .ancestors(page_id)
                            .into_iter()
                            .map(|a| a.title)
                            .collect();
                        p.diff = word_diff(old_body, body);
                        p.external_images = added_images(old_body, body);
                        p.details = id_notes(view, old_body, body);
                    }
                    Change::SetProperties {
                        page_id, values, ..
                    } => {
                        p.kind = ChangeKind::SetProperties;
                        p.page_id = Some(page_id.clone());
                        p.title = view.title(page_id).unwrap_or_default();
                        p.location = view
                            .ancestors(page_id)
                            .into_iter()
                            .map(|a| a.title)
                            .collect();
                        let current = view.properties(page_id);
                        p.properties = values
                            .iter()
                            .map(|(name, input)| PropertyChange {
                                name: name.clone(),
                                before: current.get(name).and_then(display_json),
                                after: display(input),
                            })
                            .collect();
                    }
                    Change::MovePage {
                        page_id, to_parent, ..
                    } => {
                        p.kind = ChangeKind::MovePage;
                        p.page_id = Some(page_id.clone());
                        p.title = view.title(page_id).unwrap_or_default();
                        p.location = view
                            .ancestors(page_id)
                            .into_iter()
                            .map(|a| a.title)
                            .collect();
                        p.new_location = Some(self.location(view, to_parent));
                    }
                }
                p
            })
            .collect();
        ChangeSetPreview {
            id: self.id.clone(),
            request: self.request.clone(),
            changes,
        }
    }
}

// ------------------------------------------------------- operation records

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OpEntry {
    EditPage {
        page_id: String,
        before_rev: String,
        after_rev: String,
        /// The page before, relative to `.diaspar/`. Protected while the record is kept.
        snapshot: String,
    },
    SetProperties {
        page_id: String,
        before_rev: String,
        after_rev: String,
        snapshot: String,
    },
    CreatePage {
        page_id: String,
        after_rev: String,
    },
    MovePage {
        page_id: String,
        from_parent: Option<String>,
        to_parent: Option<String>,
    },
    TrashPage {
        page_id: String,
        entry_id: String,
    },
    RestorePage {
        page_id: String,
        rev: String,
    },
}

impl OpEntry {
    fn page_id(&self) -> &str {
        match self {
            OpEntry::EditPage { page_id, .. }
            | OpEntry::SetProperties { page_id, .. }
            | OpEntry::CreatePage { page_id, .. }
            | OpEntry::MovePage { page_id, .. }
            | OpEntry::TrashPage { page_id, .. }
            | OpEntry::RestorePage { page_id, .. } => page_id,
        }
    }

    fn snapshot(&self) -> Option<&str> {
        match self {
            OpEntry::EditPage { snapshot, .. } | OpEntry::SetProperties { snapshot, .. } => {
                Some(snapshot)
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OperationRecord {
    format_version: u32,
    op_id: String,
    /// Increases with every record: timestamps can tie or go backwards.
    #[serde(default)]
    seq: u64,
    applied_at: String,
    /// `ai`, or `undo` for an undo (of an AI change, or of another undo).
    source: String,
    request: String,
    summary: String,
    #[serde(default)]
    provider: String,
    #[serde(default)]
    model: String,
    ops: Vec<OpEntry>,
    /// The operation this one undid.
    #[serde(default)]
    undoes: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct OperationPage {
    pub id: String,
    /// `None` when the page isn't in the workspace anymore.
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct OperationInfo {
    pub op_id: String,
    pub applied_at: String,
    /// `ai`, or `undo`.
    pub source: String,
    pub request: String,
    pub summary: String,
    pub model: String,
    /// The operation this one undid.
    pub undoes: Option<String>,
    /// Undone since (and not redone).
    pub undone: bool,
    pub pages: Vec<OperationPage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct ChangeConflict {
    /// The change (or, for an undo, the page) it concerns.
    pub change_id: String,
    pub title: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "outcome", rename_all = "snake_case")]
#[ts(export)]
pub enum ChangeOutcome {
    Applied {
        operation: OperationInfo,
    },
    /// Something changed since; nothing was written.
    Conflicts {
        conflicts: Vec<ChangeConflict>,
    },
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn summarize(ops: &[OpEntry]) -> String {
    let mut parts = Vec::new();
    let tally = |f: fn(&OpEntry) -> bool| ops.iter().filter(|o| f(o)).count();
    let created = tally(|o| matches!(o, OpEntry::CreatePage { .. }));
    let edited = tally(|o| matches!(o, OpEntry::EditPage { .. }));
    let set = tally(|o| matches!(o, OpEntry::SetProperties { .. }));
    let moved = tally(|o| matches!(o, OpEntry::MovePage { .. }));
    let trashed = tally(|o| matches!(o, OpEntry::TrashPage { .. }));
    let restored = tally(|o| matches!(o, OpEntry::RestorePage { .. }));
    if created > 0 {
        parts.push(format!("created {}", count(created, "page", "pages")));
    }
    if edited > 0 {
        parts.push(format!("edited {}", count(edited, "page", "pages")));
    }
    if set > 0 {
        parts.push(format!("set properties on {}", count(set, "page", "pages")));
    }
    if moved > 0 {
        parts.push(format!("moved {}", count(moved, "page", "pages")));
    }
    if trashed > 0 {
        parts.push(format!(
            "moved {} to the trash",
            count(trashed, "page", "pages")
        ));
    }
    if restored > 0 {
        parts.push(format!(
            "restored {} from the trash",
            count(restored, "page", "pages")
        ));
    }
    let text = parts.join(", ");
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => "No changes".into(),
    }
}

impl Workspace {
    fn operations_dir(&self) -> std::path::PathBuf {
        self.diaspar().join("operations")
    }

    fn records(&self) -> Vec<OperationRecord> {
        let Ok(read) = fs::read_dir(self.operations_dir()) else {
            return Vec::new();
        };
        let mut records: Vec<OperationRecord> = read
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
            .filter_map(|e| match load_json::<OperationRecord>(&e.path()) {
                Ok(Some(record)) => Some(record),
                other => {
                    log::warn!(
                        "unreadable operation record {:?}: {:?}",
                        e.path(),
                        other.err()
                    );
                    None
                }
            })
            .collect();
        // Newest first.
        records.sort_by(|a, b| {
            b.seq
                .cmp(&a.seq)
                .then(b.applied_at.cmp(&a.applied_at))
                .then(b.op_id.cmp(&a.op_id))
        });
        records
    }

    /// Remembers which snapshots recorded operations need.
    pub(super) fn load_protected(&mut self) {
        self.protected = self
            .records()
            .iter()
            .flat_map(|r| {
                r.ops
                    .iter()
                    .filter_map(OpEntry::snapshot)
                    .map(str::to_owned)
            })
            .collect();
    }

    /// Keeps the newest `KEEP_OPERATIONS` records; older ones stop protecting
    /// their snapshots.
    fn prune_operations(&mut self) {
        for old in self.records().into_iter().skip(KEEP_OPERATIONS) {
            let path = self.operations_dir().join(format!("{}.json", old.op_id));
            if let Err(err) = fs::remove_file(&path) {
                log::warn!("couldn't remove the old operation {path:?}: {err}");
            }
        }
        self.load_protected();
    }

    fn info_of(&self, record: &OperationRecord, all: &[OperationRecord]) -> OperationInfo {
        let mut seen = HashSet::new();
        let pages = record
            .ops
            .iter()
            .map(OpEntry::page_id)
            .filter(|id| seen.insert(id.to_string()))
            .map(|id| OperationPage {
                id: id.to_string(),
                title: self.index.get(id).map(|e| e.title.clone()),
            })
            .collect();
        OperationInfo {
            op_id: record.op_id.clone(),
            applied_at: record.applied_at.clone(),
            source: record.source.clone(),
            request: record.request.clone(),
            summary: record.summary.clone(),
            model: record.model.clone(),
            undoes: record.undoes.clone(),
            undone: undone(&record.op_id, all, 0),
            pages,
        }
    }

    /// Recorded operations, newest first.
    pub fn list_operations(&self) -> Vec<OperationInfo> {
        let all = self.records();
        all.iter().map(|r| self.info_of(r, &all)).collect()
    }

    fn file_rev(&self, id: &str) -> CoreResult<String> {
        let entry = self.entry(id)?;
        let file = entry
            .file
            .as_ref()
            .ok_or_else(|| CoreError::Invalid("This page has no file.".into()))?;
        fs::read(file).map(|b| hash_rev(&b)).map_err(io_at(file))
    }

    /// The snapshot taken for `page_id` by the first write recorded after
    /// `mark`, relative to `.diaspar/`.
    fn snapshot_since(&self, mark: usize, page_id: &str) -> Option<String> {
        self.tx_writes()
            .into_iter()
            .skip(mark)
            .find_map(|a| match a {
                Action::Write {
                    page_id: Some(id),
                    snapshot: Some(snapshot),
                    ..
                } if id == page_id => Some(
                    snapshot
                        .strip_prefix(".diaspar/")
                        .unwrap_or(&snapshot)
                        .to_string(),
                ),
                _ => None,
            })
    }

    /// Makes sure every page `new` links to (and `old` didn't) has its ID
    /// on disk, as any durable reference needs (architecture rule 6).
    fn persist_link_targets(&mut self, new: &str, old: &str) -> CoreResult<()> {
        let targets = |body: &str| {
            crate::extract::extract(body)
                .links
                .into_iter()
                .filter(|l| l.kind == crate::extract::LinkKind::Page)
                .map(|l| l.target)
                .collect::<HashSet<_>>()
        };
        let before = targets(old);
        for target in targets(new) {
            if !before.contains(&target) && self.index.get(&target).is_some() {
                self.ensure_persisted_id(&target)?;
            }
        }
        Ok(())
    }

    fn save_body(&mut self, id: &str, base: &str, body: &str) -> CoreResult<()> {
        match self.save_page(id, Some(base), body)? {
            super::types::SaveOutcome::Saved { .. } => Ok(()),
            _ => Err(CoreError::Invalid(format!(
                "“{}” changed while the changes were being applied.",
                self.index
                    .get(id)
                    .map(|e| e.title.as_str())
                    .unwrap_or("A page")
            ))),
        }
    }

    fn set_values(&mut self, id: &str, values: &[(String, CellInput)]) -> CoreResult<()> {
        if values.is_empty() {
            return Ok(());
        }
        let collection = self
            .entry(id)?
            .parent
            .clone()
            .ok_or_else(|| CoreError::Invalid("This page isn't in a collection.".into()))?;
        let schema = self.editable_schema(&collection)?;
        for (name, input) in values {
            let property = schema
                .properties
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| CoreError::Invalid(format!("The property “{name}” is gone.")))?;
            self.set_property(id, &property.id, input)?;
        }
        Ok(())
    }

    fn conflict(&self, change_id: &str, id: &str, message: &str) -> ChangeConflict {
        ChangeConflict {
            change_id: change_id.to_string(),
            title: self
                .index
                .get(id)
                .map(|e| e.title.clone())
                .unwrap_or_else(|| "A removed page".into()),
            message: message.to_string(),
        }
    }

    /// What stands in the way of applying `chosen` now.
    fn check_changes(&self, chosen: &[&ProposedChange]) -> Vec<ChangeConflict> {
        let mut conflicts = Vec::new();
        for c in chosen {
            let problem: Option<(&str, &str)> = match &c.change {
                Change::EditPage {
                    page_id,
                    base_body_rev,
                    ..
                } => match self.get_page(page_id) {
                    Err(_) => Some((page_id, "It was deleted or moved to the trash.")),
                    Ok(p) if p.body_rev != *base_body_rev => {
                        Some((page_id, "Its text changed after this was proposed."))
                    }
                    Ok(_) => None,
                },
                Change::SetProperties {
                    page_id, base_rev, ..
                } => match self.get_page(page_id) {
                    Err(_) => Some((page_id, "It was deleted or moved to the trash.")),
                    Ok(p) if p.rev.as_deref() != Some(base_rev.as_str()) => {
                        Some((page_id, "It changed after this was proposed."))
                    }
                    Ok(_) => None,
                },
                Change::MovePage {
                    page_id,
                    from_parent,
                    to_parent,
                } => match self.index.get(page_id) {
                    None => Some((page_id, "It was deleted or moved to the trash.")),
                    Some(e) if e.parent != *from_parent => {
                        Some((page_id, "It was moved after this was proposed."))
                    }
                    Some(_) => match to_parent {
                        Some(Target::Existing(t)) if self.index.get(t).is_none() => {
                            Some((page_id, "The page it would go into is gone."))
                        }
                        Some(Target::Existing(t))
                            if self.index.is_self_or_descendant(page_id, t) =>
                        {
                            Some((page_id, "It would end up inside itself."))
                        }
                        _ => None,
                    },
                },
                Change::CreatePage {
                    parent: Some(Target::Existing(p)),
                    title,
                    ..
                } if self.index.get(p).is_none() => {
                    conflicts.push(ChangeConflict {
                        change_id: c.id.clone(),
                        title: title.clone(),
                        message: "The page it would go into is gone.".into(),
                    });
                    None
                }
                Change::CreatePage { .. } => None,
            };
            if let Some((id, message)) = problem {
                conflicts.push(self.conflict(&c.id, id, message));
            }
        }
        conflicts
    }

    /// Applies the chosen changes of `set`, all or none (ARCHITECTURE §10.3).
    /// Only the user's approval calls this: the assistant can't reach it.
    pub fn apply_changeset(
        &mut self,
        set: &ChangeSet,
        chosen: &[String],
    ) -> CoreResult<ChangeOutcome> {
        self.writable()?;
        let selected: Vec<&ProposedChange> = set
            .changes
            .iter()
            .filter(|c| chosen.contains(&c.id))
            .collect();
        if selected.is_empty() {
            return Err(CoreError::Invalid(
                "Choose at least one change to apply.".into(),
            ));
        }
        for c in &selected {
            for needed in dependencies(&c.change) {
                if !chosen.contains(&needed) {
                    return Err(CoreError::Invalid(format!(
                        "Change {} goes into the page change {needed} creates; choose both.",
                        c.id
                    )));
                }
            }
        }
        let conflicts = self.check_changes(&selected);
        if !conflicts.is_empty() {
            return Ok(ChangeOutcome::Conflicts { conflicts });
        }

        self.begin_tx("changeset_apply", "ai")?;
        match self.apply_selected(set, &selected) {
            Ok(record) => self.finish_operation(record),
            Err(err) => {
                self.abort_tx();
                Err(err)
            }
        }
    }

    fn apply_selected(
        &mut self,
        set: &ChangeSet,
        selected: &[&ProposedChange],
    ) -> CoreResult<OperationRecord> {
        let mut created: HashMap<String, String> = HashMap::new();
        let resolve = |target: &Option<Target>, created: &HashMap<String, String>| match target {
            None => Ok(None),
            Some(Target::Existing(id)) => Ok(Some(id.clone())),
            Some(Target::New(change)) => created.get(change).cloned().map(Some).ok_or_else(|| {
                CoreError::Invalid(format!("Change {change} wasn't applied first."))
            }),
        };
        let mut ops = Vec::new();
        // A page goes in after the page it goes into: the assistant may have
        // proposed the container later.
        let mut pending: Vec<&ProposedChange> = selected.to_vec();
        let mut ordered: Vec<&ProposedChange> = Vec::new();
        while !pending.is_empty() {
            let ready = pending.iter().position(|c| {
                dependencies(&c.change).iter().all(|d| {
                    ordered.iter().any(|o| o.id == *d) || !pending.iter().any(|p| p.id == *d)
                })
            });
            match ready {
                Some(i) => ordered.push(pending.remove(i)),
                None => {
                    return Err(CoreError::Invalid(
                        "These changes depend on each other in a circle.".into(),
                    ));
                }
            }
        }
        for c in ordered {
            match &c.change {
                Change::CreatePage {
                    parent,
                    title,
                    body,
                    collection,
                    values,
                } => {
                    let parent = resolve(parent, &created)?;
                    let id = match collection {
                        Some(_) => self.create_collection(parent.as_deref(), title)?.id,
                        None => self.create_page(parent.as_deref(), title)?.id,
                    };
                    for property in collection.iter().flatten() {
                        let schema = self.update_schema(
                            &id,
                            &SchemaOp::AddProperty {
                                name: property.name.clone(),
                                kind: property.kind,
                            },
                        )?;
                        let pid = schema
                            .properties
                            .last()
                            .map(|p| p.id.clone())
                            .unwrap_or_default();
                        for option in &property.options {
                            self.update_schema(
                                &id,
                                &SchemaOp::AddOption {
                                    property_id: pid.clone(),
                                    name: option.clone(),
                                },
                            )?;
                        }
                    }
                    if !body.trim().is_empty() {
                        self.persist_link_targets(body, "")?;
                        let base = self.get_page(&id)?.body_rev;
                        self.save_body(&id, &base, body)?;
                    }
                    self.set_values(&id, values)?;
                    created.insert(c.id.clone(), id.clone());
                    let after_rev = self.file_rev(&id)?;
                    ops.push(OpEntry::CreatePage {
                        page_id: id,
                        after_rev,
                    });
                }
                Change::EditPage {
                    page_id,
                    base_body_rev,
                    old_body,
                    body,
                } => {
                    self.persist_link_targets(body, old_body)?;
                    let before_rev = self.file_rev(page_id)?;
                    let mark = self.tx_writes().len();
                    self.save_body(page_id, base_body_rev, body)?;
                    ops.push(OpEntry::EditPage {
                        page_id: page_id.clone(),
                        before_rev,
                        after_rev: self.file_rev(page_id)?,
                        snapshot: self.snapshot_since(mark, page_id).unwrap_or_default(),
                    });
                }
                Change::SetProperties {
                    page_id, values, ..
                } => {
                    let before_rev = self.file_rev(page_id)?;
                    let mark = self.tx_writes().len();
                    self.set_values(page_id, values)?;
                    ops.push(OpEntry::SetProperties {
                        page_id: page_id.clone(),
                        before_rev,
                        after_rev: self.file_rev(page_id)?,
                        snapshot: self.snapshot_since(mark, page_id).unwrap_or_default(),
                    });
                }
                Change::MovePage {
                    page_id,
                    from_parent,
                    to_parent,
                } => {
                    let to = resolve(to_parent, &created)?;
                    self.move_page(page_id, to.as_deref(), None)?;
                    ops.push(OpEntry::MovePage {
                        page_id: page_id.clone(),
                        from_parent: from_parent.clone(),
                        to_parent: to,
                    });
                }
            }
        }
        Ok(OperationRecord {
            format_version: FORMAT_VERSION,
            op_id: new_id(),
            seq: 0,
            applied_at: now_rfc3339(),
            source: "ai".into(),
            request: set.request.clone(),
            summary: summarize(&ops),
            provider: set.provider.clone(),
            model: set.model.clone(),
            ops,
            undoes: None,
        })
    }

    /// Writes the record inside the transaction, commits it, and tidies up.
    fn finish_operation(&mut self, mut record: OperationRecord) -> CoreResult<ChangeOutcome> {
        record.seq = self.records().first().map_or(1, |r| r.seq + 1);
        let path = self.operations_dir().join(format!("{}.json", record.op_id));
        if let Err(err) = self.save_record(&path, &record) {
            self.abort_tx();
            return Err(err);
        }
        self.commit_tx()?;
        self.prune_operations();
        self.rescan(None);
        let all = self.records();
        Ok(ChangeOutcome::Applied {
            operation: self.info_of(&record, &all),
        })
    }

    /// Undoes an operation (or, for an undo, redoes what it undid), all or
    /// nothing: if any page isn't as the operation left it, nothing changes.
    pub fn undo_operation(&mut self, op_id: &str) -> CoreResult<ChangeOutcome> {
        self.writable()?;
        validate_uuid(op_id, "Operation")?;
        let all = self.records();
        let record = all
            .iter()
            .find(|r| r.op_id == op_id)
            .cloned()
            .ok_or_else(|| CoreError::NotFound("This change".into()))?;
        if undone(op_id, &all, 0) {
            return Err(CoreError::Invalid("This change was already undone.".into()));
        }
        let conflicts = self.check_undo(&record);
        if !conflicts.is_empty() {
            return Ok(ChangeOutcome::Conflicts { conflicts });
        }
        self.begin_tx("operation_undo", "undo")?;
        match self.undo_entries(&record) {
            Ok(ops) => {
                let summary = match record.source.as_str() {
                    "undo" => format!(
                        "Redid: {}",
                        record
                            .summary
                            .strip_prefix("Undid: ")
                            .unwrap_or(&record.summary)
                    ),
                    _ => format!("Undid: {}", record.summary),
                };
                let undo = OperationRecord {
                    format_version: FORMAT_VERSION,
                    op_id: new_id(),
                    seq: 0,
                    applied_at: now_rfc3339(),
                    source: "undo".into(),
                    request: record.request.clone(),
                    summary,
                    provider: record.provider.clone(),
                    model: record.model.clone(),
                    ops,
                    undoes: Some(record.op_id.clone()),
                };
                self.finish_operation(undo)
            }
            Err(err) => {
                self.abort_tx();
                Err(err)
            }
        }
    }

    fn check_undo(&self, record: &OperationRecord) -> Vec<ChangeConflict> {
        let created: HashSet<&str> = record
            .ops
            .iter()
            .filter_map(|o| match o {
                OpEntry::CreatePage { page_id, .. } => Some(page_id.as_str()),
                _ => None,
            })
            .collect();
        let mut conflicts = Vec::new();
        for op in &record.ops {
            let id = op.page_id();
            let exists = self.index.get(id).is_some();
            let rev = || self.file_rev(id).ok();
            let problem: Option<&str> = match op {
                OpEntry::EditPage {
                    after_rev,
                    snapshot,
                    ..
                }
                | OpEntry::SetProperties {
                    after_rev,
                    snapshot,
                    ..
                } => {
                    if !exists {
                        Some("It was deleted or moved to the trash.")
                    } else if rev().as_deref() != Some(after_rev.as_str()) {
                        Some("It changed after the change was applied.")
                    } else if !self.diaspar().join(snapshot).is_file() {
                        Some("Its earlier version isn't in the history anymore.")
                    } else {
                        None
                    }
                }
                OpEntry::CreatePage { after_rev, .. } => {
                    if !exists {
                        Some("It was deleted or moved to the trash.")
                    } else if rev().as_deref() != Some(after_rev.as_str()) {
                        Some("It was edited after it was created.")
                    } else if self
                        .index
                        .children_of(Some(id))
                        .iter()
                        .any(|c| !created.contains(c.as_str()))
                    {
                        Some("Pages were added inside it since.")
                    } else {
                        None
                    }
                }
                OpEntry::TrashPage { entry_id, .. } => (!self
                    .diaspar()
                    .join("trash")
                    .join(entry_id)
                    .join("entry.json")
                    .is_file())
                .then_some("It isn't in the trash anymore."),
                OpEntry::RestorePage { rev: expected, .. } => {
                    if !exists {
                        Some("It was deleted or moved to the trash.")
                    } else if rev().as_deref() != Some(expected.as_str()) {
                        Some("It was edited after it was restored.")
                    } else {
                        None
                    }
                }
                OpEntry::MovePage {
                    from_parent,
                    to_parent,
                    ..
                } => match self.index.get(id) {
                    None => Some("It was deleted or moved to the trash."),
                    Some(e) if e.parent != *to_parent => Some("It was moved again since."),
                    Some(_) => match from_parent {
                        Some(p) if self.index.get(p).is_none() => {
                            Some("The page it was in is gone.")
                        }
                        Some(p) if self.index.is_self_or_descendant(id, p) => {
                            Some("It would end up inside itself.")
                        }
                        _ => None,
                    },
                },
            };
            if let Some(message) = problem {
                conflicts.push(ChangeConflict {
                    change_id: id.to_string(),
                    title: self
                        .index
                        .get(id)
                        .map(|e| e.title.clone())
                        .unwrap_or_else(|| "A removed page".into()),
                    message: message.into(),
                });
            }
        }
        conflicts
    }

    fn undo_entries(&mut self, record: &OperationRecord) -> CoreResult<Vec<OpEntry>> {
        let mut ops = Vec::new();
        for op in record.ops.iter().rev() {
            match op {
                OpEntry::EditPage {
                    page_id,
                    after_rev,
                    snapshot,
                    ..
                }
                | OpEntry::SetProperties {
                    page_id,
                    after_rev,
                    snapshot,
                    ..
                } => {
                    let path = self.diaspar().join(snapshot);
                    let earlier = fs::read(&path).map_err(io_at(&path))?;
                    let earlier = String::from_utf8_lossy(&earlier).into_owned();
                    let mark = self.tx_writes().len();
                    self.write_page_text(page_id, &earlier)?;
                    let snapshot = self.snapshot_since(mark, page_id).unwrap_or_default();
                    let (before_rev, after_rev) = (after_rev.clone(), hash_rev(earlier.as_bytes()));
                    ops.push(match op {
                        OpEntry::EditPage { .. } => OpEntry::EditPage {
                            page_id: page_id.clone(),
                            before_rev,
                            after_rev,
                            snapshot,
                        },
                        _ => OpEntry::SetProperties {
                            page_id: page_id.clone(),
                            before_rev,
                            after_rev,
                            snapshot,
                        },
                    });
                }
                OpEntry::CreatePage { page_id, .. } | OpEntry::RestorePage { page_id, .. } => {
                    let entry = self.trash_page(page_id)?;
                    ops.push(OpEntry::TrashPage {
                        page_id: page_id.clone(),
                        entry_id: entry.entry_id,
                    });
                }
                OpEntry::TrashPage { page_id, entry_id } => {
                    let restored = self.restore_trash(entry_id)?;
                    if restored.as_deref() != Some(page_id.as_str()) {
                        return Err(CoreError::Invalid(
                            "The page came back from the trash with another ID.".into(),
                        ));
                    }
                    ops.push(OpEntry::RestorePage {
                        page_id: page_id.clone(),
                        rev: self.file_rev(page_id)?,
                    });
                }
                OpEntry::MovePage {
                    page_id,
                    from_parent,
                    to_parent,
                } => {
                    self.move_page(page_id, from_parent.as_deref(), None)?;
                    ops.push(OpEntry::MovePage {
                        page_id: page_id.clone(),
                        from_parent: to_parent.clone(),
                        to_parent: from_parent.clone(),
                    });
                }
            }
        }
        Ok(ops)
    }

    /// Writes `text` to the page's file exactly, as an undo does.
    fn write_page_text(&mut self, id: &str, text: &str) -> CoreResult<()> {
        let file = self
            .entry(id)?
            .file
            .clone()
            .ok_or_else(|| CoreError::Invalid("This page has no file.".into()))?;
        let bytes = fs::read(&file).map_err(io_at(&file))?;
        let current = String::from_utf8_lossy(&bytes).into_owned();
        self.write_page_file(id, &file, text, Some((&current, &hash_rev(&bytes))), None)?;
        self.rescan(None);
        Ok(())
    }
}

/// Whether `op_id` is undone: an undo of it exists that isn't itself undone.
fn undone(op_id: &str, all: &[OperationRecord], depth: usize) -> bool {
    depth < all.len()
        && all
            .iter()
            .filter(|r| r.undoes.as_deref() == Some(op_id))
            .any(|u| !undone(&u.op_id, all, depth + 1))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::workspace::types::SaveOutcome;

    fn workspace(pages: &[(&str, &str)]) -> (tempfile::TempDir, Workspace) {
        let dir = tempfile::tempdir().unwrap();
        for (path, text) in pages {
            let file = dir.path().join(path);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, text).unwrap();
        }
        let ws = Workspace::open(dir.path()).unwrap();
        (dir, ws)
    }

    fn id(ws: &Workspace, title: &str) -> String {
        ws.index
            .entries
            .values()
            .find(|e| e.title == title)
            .unwrap_or_else(|| panic!("no page {title}"))
            .id
            .clone()
    }

    /// Every file under `dir` with its hash; `.diaspar/` too when `all`.
    fn files(dir: &Path, all: bool) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for item in fs::read_dir(&d).unwrap().flatten() {
                let path = item.path();
                if !all && item.file_name() == ".diaspar" {
                    continue;
                }
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let rel = path.strip_prefix(dir).unwrap().display().to_string();
                    out.push((rel, hash_rev(&fs::read(&path).unwrap())));
                }
            }
        }
        out.sort();
        out
    }

    fn body(ws: &Workspace, id: &str) -> String {
        ws.get_page(id).unwrap().body
    }

    fn applied(outcome: ChangeOutcome) -> OperationInfo {
        match outcome {
            ChangeOutcome::Applied { operation } => operation,
            other => panic!("expected the changes to apply, got {other:?}"),
        }
    }

    fn all_ids(set: &ChangeSet) -> Vec<String> {
        set.changes.iter().map(|c| c.id.clone()).collect()
    }

    const PAGES: &[(&str, &str)] = &[
        (
            "Plan.md",
            "---\ntitle: Plan\nid: 3f1d2c4b-5a6e-4f70-8a9b-0c1d2e3f4a5b\n---\nShip the beta in March. Old text here.\n",
        ),
        ("Notes.md", "Loose notes, no frontmatter.\n"),
        (
            "Projects/index.md",
            "---\ntitle: Projects\n---\nAll projects.\n",
        ),
    ];

    /// A set that creates, edits, moves and links.
    fn proposals(ws: &Workspace) -> ChangeSet {
        let view = ws.read_view();
        let mut set = ChangeSet::new("Tidy up", "test", "model");
        let (plan, notes, projects) = (id(ws, "Plan"), id(ws, "Notes"), id(ws, "Projects"));
        let new = set
            .propose_page(view, Some(&projects), "Ideas", "First idea.\n")
            .unwrap();
        assert_eq!(new, "new-1");
        set.propose_edit(
            view,
            &plan,
            &Edit::Replace(vec![("Old text".into(), "New text".into())]),
        )
        .unwrap();
        set.propose_move(view, &notes, Some(&projects)).unwrap();
        set.propose_link(view, "new-1", &plan, None).unwrap();
        set
    }

    #[test]
    fn proposing_and_previewing_write_nothing() {
        let (dir, ws) = workspace(PAGES);
        let before = files(dir.path(), true);
        let set = proposals(&ws);
        let preview = set.preview(ws.read_view());
        assert_eq!(preview.changes.len(), 3);
        assert_eq!(preview.changes[0].kind, ChangeKind::CreatePage);
        assert_eq!(preview.changes[0].location, ["Projects"]);
        assert!(preview.changes[0].diff[0].text.contains("diaspar://page/"));
        let edit = &preview.changes[1];
        assert_eq!(edit.kind, ChangeKind::EditPage);
        assert!(
            edit.diff
                .iter()
                .any(|d| d.kind == DiffKind::Delete && d.text == "Old")
        );
        assert!(
            edit.diff
                .iter()
                .any(|d| d.kind == DiffKind::Insert && d.text == "New")
        );
        assert_eq!(
            preview.changes[2].new_location.as_deref(),
            Some(&["Projects".to_string()][..])
        );
        assert!(set.describe(ws.read_view()).contains("2. Edit “Plan”"));
        // Rejecting is dropping the set: nothing on disk changed, not even in .diaspar.
        drop(set);
        assert_eq!(files(dir.path(), true), before);
    }

    #[test]
    fn applying_writes_every_change_and_undo_restores_the_exact_bytes() {
        let (dir, mut ws) = workspace(PAGES);
        let plan_file = dir.path().join("Plan.md");
        let original_plan = fs::read(&plan_file).unwrap();
        let before = files(dir.path(), false);
        let set = proposals(&ws);
        let (plan, notes, projects) = (id(&ws, "Plan"), id(&ws, "Notes"), id(&ws, "Projects"));

        let op = applied(ws.apply_changeset(&set, &all_ids(&set)).unwrap());
        assert_eq!(op.source, "ai");
        assert_eq!(op.summary, "Created 1 page, edited 1 page, moved 1 page");
        assert!(body(&ws, &plan).contains("New text here."));
        assert_eq!(
            ws.index.get(&notes).unwrap().parent.as_deref(),
            Some(projects.as_str())
        );
        let ideas = id(&ws, "Ideas");
        assert_eq!(
            ws.index.get(&ideas).unwrap().parent.as_deref(),
            Some(projects.as_str())
        );
        assert!(body(&ws, &ideas).contains(&format!("diaspar://page/{plan}")));
        assert!(
            !dir.path()
                .join(".diaspar/journal")
                .read_dir()
                .unwrap()
                .any(|_| true)
        );
        assert_eq!(ws.list_operations().len(), 1);

        let undo = applied(ws.undo_operation(&op.op_id).unwrap());
        assert_eq!(undo.source, "undo");
        assert_eq!(undo.undoes.as_deref(), Some(op.op_id.as_str()));
        assert_eq!(fs::read(&plan_file).unwrap(), original_plan, "exact bytes");
        assert_eq!(ws.index.get(&notes).unwrap().parent, None);
        assert!(
            ws.index.get(&ideas).is_none(),
            "the created page went to the trash"
        );
        assert_eq!(ws.list_trash().len(), 1);
        // Notes got its ID written to move it; everything else is as it was.
        let after: Vec<_> = files(dir.path(), false)
            .into_iter()
            .filter(|(p, _)| p != "Notes.md")
            .collect();
        let expected: Vec<_> = before
            .into_iter()
            .filter(|(p, _)| p != "Notes.md")
            .collect();
        assert_eq!(after, expected);
        assert!(
            ws.list_operations()
                .iter()
                .any(|o| o.op_id == op.op_id && o.undone)
        );
        assert!(ws.undo_operation(&op.op_id).is_err(), "already undone");

        // Redo: undoing the undo.
        let redo = applied(ws.undo_operation(&undo.op_id).unwrap());
        assert!(redo.summary.starts_with("Redid: "));
        assert!(body(&ws, &plan).contains("New text here."));
        assert_eq!(
            ws.index.get(&notes).unwrap().parent.as_deref(),
            Some(projects.as_str())
        );
        assert!(ws.index.get(&ideas).is_some(), "restored from the trash");
        let listed = ws.list_operations();
        assert!(!listed.iter().find(|o| o.op_id == op.op_id).unwrap().undone);
    }

    #[test]
    fn an_edit_made_meanwhile_is_a_conflict_and_nothing_is_written() {
        let (dir, mut ws) = workspace(PAGES);
        let set = proposals(&ws);
        let plan = id(&ws, "Plan");
        let current = ws.get_page(&plan).unwrap().body_rev;
        assert!(matches!(
            ws.save_page(&plan, Some(&current), "I changed it myself.\n")
                .unwrap(),
            SaveOutcome::Saved { .. }
        ));
        let before = files(dir.path(), false);
        match ws.apply_changeset(&set, &all_ids(&set)).unwrap() {
            ChangeOutcome::Conflicts { conflicts } => {
                assert_eq!(conflicts.len(), 1);
                assert_eq!(conflicts[0].change_id, "2");
                assert_eq!(conflicts[0].title, "Plan");
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(files(dir.path(), false), before);
        assert!(ws.list_operations().is_empty());
    }

    #[test]
    fn a_crash_during_apply_is_rolled_back_on_the_next_start() {
        // Notes is a leaf, so a page created inside it turns it into a
        // folder first: renames, a new folder, then writes.
        let pages: &[(&str, &str)] = &[
            ("Plan.md", "---\ntitle: Plan\n---\nOld text here.\n"),
            (
                "Notes.md",
                "---\ntitle: Notes\nid: 6a1c1a7e-8f4b-4c55-9b0a-1c2d3e4f5a6b\n---\nNotes.\n",
            ),
            (
                "Projects/index.md",
                "---\ntitle: Projects\nid: 0b3f6d0e-1a2b-4c3d-8e9f-0a1b2c3d4e5f\n---\nAll projects.\n",
            ),
        ];
        let mut crashed_somewhere = 0;
        for crash_at in 0..30 {
            let (dir, mut ws) = workspace(pages);
            let before = files(dir.path(), false);
            let view = ws.read_view();
            let mut set = ChangeSet::new("Tidy", "test", "m");
            let (plan, notes, projects) = (id(&ws, "Plan"), id(&ws, "Notes"), id(&ws, "Projects"));
            set.propose_page(view, Some(&notes), "Inside notes", "Body.")
                .unwrap();
            set.propose_edit(view, &plan, &Edit::Body("Rewritten.\n".into()))
                .unwrap();
            set.propose_move(view, &plan, Some(&projects)).unwrap();
            ws.crash_after_steps = Some(crash_at);
            let result = ws.apply_changeset(&set, &all_ids(&set));
            if result.is_ok() {
                // The whole set fits in fewer actions than `crash_at`.
                assert!(crashed_somewhere > 3, "{crashed_somewhere}");
                break;
            }
            crashed_somewhere += 1;
            drop(ws);
            let mut reopened = Workspace::open(dir.path()).unwrap();
            assert_eq!(
                files(dir.path(), false),
                before,
                "crash after {crash_at} actions"
            );
            assert!(
                reopened
                    .take_notices()
                    .iter()
                    .any(|n| n.contains("interrupted AI change was undone")),
                "crash after {crash_at}"
            );
            assert!(reopened.list_operations().is_empty());
            assert!(
                !dir.path()
                    .join(".diaspar/journal")
                    .read_dir()
                    .unwrap()
                    .any(|_| true)
            );
        }
        assert!(crashed_somewhere > 3);
    }

    #[test]
    fn a_failure_halfway_undoes_what_was_done() {
        let (dir, mut ws) = workspace(PAGES);
        let before = files(dir.path(), false);
        let set = proposals(&ws);
        // The move's destination disappears after the check, as if the
        // filesystem refused: make the folder unwritable.
        let projects_dir = dir.path().join("Projects");
        let mut perms = fs::metadata(&projects_dir).unwrap().permissions();
        std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o555);
        fs::set_permissions(&projects_dir, perms.clone()).unwrap();
        let result = ws.apply_changeset(&set, &all_ids(&set));
        std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
        fs::set_permissions(&projects_dir, perms).unwrap();
        assert!(result.is_err());
        assert_eq!(files(dir.path(), false), before);
        assert!(ws.list_operations().is_empty());
        assert!(!ws.in_tx());
    }

    #[test]
    fn chosen_changes_need_the_changes_they_depend_on() {
        let (dir, mut ws) = workspace(PAGES);
        let view = ws.read_view();
        let mut set = ChangeSet::new("Organize", "test", "m");
        let notes = id(&ws, "Notes");
        let plan = id(&ws, "Plan");
        let folder = set.propose_page(view, None, "Archive", "").unwrap();
        set.propose_move(view, &notes, Some(&folder)).unwrap();
        set.propose_edit(view, &plan, &Edit::Body("Only this.\n".into()))
            .unwrap();
        let before = files(dir.path(), false);
        assert!(ws.apply_changeset(&set, &["2".into()]).is_err());
        assert_eq!(files(dir.path(), false), before);

        let op = applied(ws.apply_changeset(&set, &["3".into()]).unwrap());
        assert_eq!(op.summary, "Edited 1 page");
        assert_eq!(body(&ws, &plan), "Only this.\n");
        assert!(ws.index.get(&notes).unwrap().parent.is_none());
    }

    #[test]
    fn a_container_proposed_later_is_created_first() {
        let (_dir, mut ws) = workspace(PAGES);
        let view = ws.read_view();
        let mut set = ChangeSet::new("Organize", "test", "m");
        let a = set.propose_page(view, None, "Child", "").unwrap();
        let b = set.propose_page(view, None, "Parent", "").unwrap();
        set.propose_move(view, &a, Some(&b)).unwrap();
        assert!(set.propose_move(view, &b, Some(&a)).is_err(), "no circles");
        applied(ws.apply_changeset(&set, &all_ids(&set)).unwrap());
        let parent = id(&ws, "Parent");
        assert_eq!(
            ws.index.get(&id(&ws, "Child")).unwrap().parent.as_deref(),
            Some(parent.as_str())
        );
    }

    #[test]
    fn collections_and_their_values() {
        let (_dir, mut ws) = workspace(&[]);
        let reading = ws.create_collection(None, "Reading").unwrap().id;
        let schema = ws
            .update_schema(
                &reading,
                &SchemaOp::AddProperty {
                    name: "Status".into(),
                    kind: PropertyType::Select,
                },
            )
            .unwrap();
        let status = schema.properties[0].id.clone();
        for name in ["To read", "Done"] {
            ws.update_schema(
                &reading,
                &SchemaOp::AddOption {
                    property_id: status.clone(),
                    name: name.into(),
                },
            )
            .unwrap();
        }
        let entry = ws.create_page(Some(&reading), "Paper").unwrap().id;
        let view = ws.read_view();
        let mut set = ChangeSet::new("Mark it done", "test", "m");
        let mut values = serde_json::Map::new();
        values.insert("status".into(), "done".into());
        set.propose_properties(view, &entry, &values, &[]).unwrap();
        let mut bad = serde_json::Map::new();
        bad.insert("Status".into(), "Maybe".into());
        assert!(
            set.propose_properties(view, &entry, &bad, &[])
                .unwrap_err()
                .contains("To read, Done")
        );
        assert!(
            set.propose_properties(view, &reading, &values, &[])
                .is_err(),
            "not an entry"
        );

        let new_list = set
            .propose_collection(
                view,
                None,
                "Films",
                &[
                    NewProperty {
                        name: "Year".into(),
                        kind: PropertyType::Number,
                        options: vec![],
                    },
                    NewProperty {
                        name: "Seen".into(),
                        kind: PropertyType::Checkbox,
                        options: vec![],
                    },
                ],
            )
            .unwrap();
        let film = set
            .propose_page(view, Some(&new_list), "Alien", "")
            .unwrap();
        let mut film_values = serde_json::Map::new();
        film_values.insert("Year".into(), 1979.into());
        film_values.insert("Seen".into(), true.into());
        set.propose_properties(view, &film, &film_values, &[])
            .unwrap();
        let preview = set.preview(view);
        assert_eq!(preview.changes[0].properties[0].before.as_deref(), None);
        assert_eq!(
            preview.changes[0].properties[0].after.as_deref(),
            Some("Done")
        );
        assert_eq!(
            preview.changes[1].details,
            ["Year (number)", "Seen (checkbox)"]
        );

        applied(ws.apply_changeset(&set, &all_ids(&set)).unwrap());
        let cell = ws.get_entry(&entry).unwrap().unwrap().row.cells[&status].clone();
        assert!(
            matches!(&cell, crate::collection::Cell::Select { value, .. } if value == "Done"),
            "{cell:?}"
        );
        let films = id(&ws, "Films");
        let data = ws.get_collection(&films).unwrap();
        assert_eq!(data.schema.unwrap().properties.len(), 2);
        assert_eq!(data.rows.len(), 1);
    }

    #[test]
    fn one_set_holds_a_bounded_amount_of_text() {
        let (_dir, ws) = workspace(PAGES);
        let view = ws.read_view();
        let mut set = ChangeSet::new("x", "t", "m");
        let big = "x".repeat(MAX_BODY_BYTES - 10);
        let mut refused = None;
        for i in 0..MAX_CHANGES {
            if let Err(e) = set.propose_page(view, None, &format!("Page {i}"), &big) {
                refused = Some((i, e));
                break;
            }
        }
        let (at, message) = refused.expect("the size limit is reached first");
        assert_eq!(at, MAX_SET_BYTES / big.len());
        assert!(message.contains("MB of text"), "{message}");
        assert!(set.bytes() <= MAX_SET_BYTES);
    }

    #[test]
    fn folders_are_neither_edited_nor_linked_to() {
        // "Archive" has no index.md, so it's a folder (ADR-047).
        let (_dir, ws) = workspace(&[("Plan.md", "plan\n"), ("Archive/Old.md", "old\n")]);
        let view = ws.read_view();
        let mut set = ChangeSet::new("x", "t", "m");
        let (plan, folder) = (id(&ws, "Plan"), id(&ws, "Archive"));
        assert!(
            set.propose_edit(view, &folder, &Edit::Body("x".into()))
                .is_err()
        );
        assert!(set.propose_link(view, &plan, &folder, None).is_err());
        // Pages can still go into it.
        set.propose_move(view, &plan, Some(&folder)).unwrap();
    }

    #[test]
    fn page_references_must_name_pages_in_this_workspace() {
        let (dir, ws) = workspace(PAGES);
        let view = ws.read_view();
        let mut set = ChangeSet::new("x", "t", "m");
        let plan = id(&ws, "Plan");
        for bad in [
            "../Plan.md",
            "/etc/passwd",
            "Plan.md",
            ".diaspar/workspace.json",
            "0c6a5f3e-6c1d-4b8e-9a63-2f1d1b0a9e77",
            "new-9",
            "",
        ] {
            assert!(set.target(view, bad).is_err(), "{bad:?}");
            assert!(
                set.propose_edit(view, bad, &Edit::Body("x".into()))
                    .is_err(),
                "{bad:?}"
            );
            assert!(
                set.propose_move(view, &plan, Some(bad)).is_err() || bad.is_empty(),
                "{bad:?}"
            );
            assert!(
                set.propose_page(view, Some(bad), "T", "").is_err() || bad.is_empty(),
                "{bad:?}"
            );
        }
        assert!(
            set.propose_edit(
                view,
                &plan,
                &Edit::Replace(vec![("missing".into(), "x".into())])
            )
            .is_err()
        );
        let notes = id(&ws, "Notes");
        assert!(
            set.propose_edit(view, &notes, &Edit::Replace(vec![("o".into(), "0".into())]))
                .unwrap_err()
                .contains("times")
        );
        assert!(set.propose_page(view, None, "  ", "").is_err());
        assert!(
            set.propose_page(view, None, "T", &"x".repeat(MAX_BODY_BYTES + 1))
                .is_err()
        );
        assert!(
            set.propose_move(view, &id(&ws, "Projects"), Some(&id(&ws, "Projects")))
                .is_err()
        );
        assert!(set.propose_link(view, &plan, &plan, None).is_err());
        // Only the top-level page proposed above landed in the set: the empty-parent cases.
        assert!(
            files(dir.path(), true)
                .iter()
                .all(|(p, _)| !p.contains("operations/"))
        );
    }

    #[test]
    fn undo_refuses_when_a_page_changed_since() {
        let (dir, mut ws) = workspace(PAGES);
        let view = ws.read_view();
        let plan = id(&ws, "Plan");
        let mut set = ChangeSet::new("x", "t", "m");
        set.propose_edit(view, &plan, &Edit::Body("AI text.\n".into()))
            .unwrap();
        let op = applied(ws.apply_changeset(&set, &all_ids(&set)).unwrap());
        let base = ws.get_page(&plan).unwrap().body_rev;
        ws.save_page(&plan, Some(&base), "Then I edited it.\n")
            .unwrap();
        let before = files(dir.path(), false);
        match ws.undo_operation(&op.op_id).unwrap() {
            ChangeOutcome::Conflicts { conflicts } => assert_eq!(conflicts[0].title, "Plan"),
            other => panic!("{other:?}"),
        }
        assert_eq!(files(dir.path(), false), before);
        assert!(ws.undo_operation("not-an-id").is_err());
    }

    #[test]
    fn linking_writes_the_target_id_first() {
        let (dir, mut ws) = workspace(PAGES);
        let view = ws.read_view();
        let (plan, notes) = (id(&ws, "Plan"), id(&ws, "Notes"));
        let mut set = ChangeSet::new("link", "t", "m");
        set.propose_link(view, &plan, &notes, Some("beta")).unwrap();
        assert_eq!(
            set.preview(view).changes[0].details,
            ["Also saves the page ID of “Notes” in its file, so the link keeps working."]
        );
        applied(ws.apply_changeset(&set, &all_ids(&set)).unwrap());
        assert!(
            body(&ws, &plan).contains(&format!("Ship the [beta](diaspar://page/{notes}) in March"))
        );
        let notes_text = fs::read_to_string(dir.path().join("Notes.md")).unwrap();
        assert!(notes_text.contains(&format!("id: {notes}")), "{notes_text}");
    }

    #[test]
    fn previews_flag_new_external_images_and_diff_cjk_by_character() {
        let (_dir, ws) = workspace(&[(
            "中文.md",
            "注意力机制很重要。\n![a](https://old.example/a.png)\n",
        )]);
        let view = ws.read_view();
        let page = id(&ws, "中文");
        let mut set = ChangeSet::new("x", "t", "m");
        set.propose_edit(
            view,
            &page,
            &Edit::Body("注意力机制非常重要。\n![a](https://old.example/a.png) ![b](https://tracker.example/b.png?u=1)\n".into()),
        )
        .unwrap();
        let preview = set.preview(view);
        assert_eq!(
            preview.changes[0].external_images,
            ["https://tracker.example/b.png?u=1"]
        );
        let inserted: Vec<&str> = preview.changes[0]
            .diff
            .iter()
            .filter(|d| d.kind == DiffKind::Insert)
            .map(|d| d.text.as_str())
            .collect();
        assert!(inserted.contains(&"非常"), "{inserted:?}");
    }

    #[test]
    fn only_the_last_hundred_operations_are_kept_with_their_snapshots() {
        let (_dir, mut ws) = workspace(PAGES);
        let plan = id(&ws, "Plan");
        let mut first = None;
        for i in 0..(KEEP_OPERATIONS + 2) {
            let view = ws.read_view();
            let mut set = ChangeSet::new("x", "t", "m");
            set.propose_edit(view, &plan, &Edit::Body(format!("Version {i}.\n")))
                .unwrap();
            let op = applied(ws.apply_changeset(&set, &all_ids(&set)).unwrap());
            first.get_or_insert(op.op_id);
        }
        let ops = ws.list_operations();
        assert_eq!(ops.len(), KEEP_OPERATIONS);
        assert!(!ops.iter().any(|o| Some(&o.op_id) == first.as_ref()));
        assert_eq!(ws.protected.len(), KEEP_OPERATIONS);
        assert!(ws.protected.iter().all(|s| ws.diaspar().join(s).is_file()));
    }
}
