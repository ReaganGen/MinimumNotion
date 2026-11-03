//! Collections in the Document Service (DATA_MODEL §6): the schema lives in
//! the collection page's frontmatter, entry values in each entry's, and view
//! settings in `.forma/views.json`.

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::collection::{
    COLORS, Cell, CellInput, Property, PropertyType, Schema, SchemaState, SelectOption,
    check_option_name, check_property_name, coerce, emit_value, new_option_id, new_property_id,
    parse_schema, schema_entry, yaml_key,
};
use crate::error::{CoreError, CoreResult, io_at};
use crate::frontmatter::{self, Change, Parsed, Value};

use super::journal::BatchWrite;
use super::scan::hash_rev;
use super::state::{FORMAT_VERSION, load_json, save_json};
use super::types::{PageStatus, PageSummary};
use super::{Workspace, now_rfc3339};

/// A collection and its entries, as the table and list views show them.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct CollectionData {
    pub id: String,
    pub title: String,
    pub schema: Option<Schema>,
    /// Why the schema can't be edited (written by a newer Forma, unreadable,
    /// or a read-only workspace). `None` when it can.
    pub problem: Option<String>,
    pub rows: Vec<CollectionRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct CollectionRow {
    pub id: String,
    pub title: String,
    pub icon: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    /// By property ID. Every schema property has a cell.
    pub cells: HashMap<String, Cell>,
    /// False when the entry's frontmatter can't be edited safely.
    pub editable: bool,
}

/// One entry with its collection's schema, for the properties shown under
/// an entry page's title.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export)]
pub struct EntryData {
    pub collection_id: String,
    pub collection_title: String,
    pub schema: Option<Schema>,
    pub problem: Option<String>,
    pub row: CollectionRow,
}

/// A change to a collection's schema (DATA_MODEL §6.4).
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(tag = "op", rename_all = "snake_case")]
#[ts(export)]
pub enum SchemaOp {
    AddProperty {
        name: String,
        #[serde(rename = "type")]
        kind: PropertyType,
    },
    RenameProperty {
        property_id: String,
        name: String,
    },
    SetPropertyType {
        property_id: String,
        #[serde(rename = "type")]
        kind: PropertyType,
    },
    RemoveProperty {
        property_id: String,
    },
    AddOption {
        property_id: String,
        name: String,
    },
    RenameOption {
        property_id: String,
        option_id: String,
        name: String,
    },
    SetOptionColor {
        property_id: String,
        option_id: String,
        color: String,
    },
    RemoveOption {
        property_id: String,
        option_id: String,
    },
}

// ------------------------------------------------------------------ views

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ViewKind {
    #[default]
    Table,
    List,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SortDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SortRule {
    /// A property ID, or `title`, `created_at`, `updated_at`.
    pub property: String,
    pub direction: SortDirection,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct FilterRule {
    pub property: String,
    /// DATA_MODEL §6.5: `contains`, `is`, `lt`, `empty`, …
    pub op: String,
    /// Always text: numbers, dates and option names as written.
    #[serde(default)]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct ViewSettings {
    pub sort: Vec<SortRule>,
    pub filter: Vec<FilterRule>,
    pub hidden: Vec<String>,
    pub column_widths: BTreeMap<String, u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export)]
pub struct CollectionViews {
    pub active_view: ViewKind,
    pub table: ViewSettings,
    pub list: ViewSettings,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct ViewsFile {
    format_version: u32,
    collections: BTreeMap<String, StoredViews>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct StoredViews {
    active_view: ViewKind,
    views: StoredViewSet,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct StoredViewSet {
    table: ViewSettings,
    list: ViewSettings,
}

/// A page file's text and its editable frontmatter, read fresh.
struct Loaded {
    file: PathBuf,
    text: String,
    rev: String,
}

impl Workspace {
    fn load(&mut self, id: &str) -> CoreResult<Loaded> {
        self.ensure_persisted_id(id)?;
        let file = self
            .entry(id)?
            .file
            .clone()
            .expect("persisted pages have a file");
        let bytes = fs::read(&file).map_err(io_at(&file))?;
        Ok(Loaded {
            rev: hash_rev(&bytes),
            text: String::from_utf8_lossy(&bytes).into_owned(),
            file,
        })
    }

    /// `text` with frontmatter `changes` applied and `updated_at` set.
    fn with_changes(text: &str, changes: &[(String, Option<String>)]) -> CoreResult<String> {
        let split = frontmatter::split(text);
        let yaml = split.yaml.unwrap_or_default();
        let spans = match frontmatter::parse(yaml) {
            Parsed::Editable { spans, .. } => spans,
            _ => {
                return Err(CoreError::ReadOnly(
                    "This page's frontmatter can't be edited safely. Edit it in a text editor first."
                        .into(),
                ));
            }
        };
        let updated = frontmatter::emit_entry("updated_at", &Value::Plain(now_rfc3339()));
        let mut list: Vec<Change<'_>> = changes
            .iter()
            .map(|(key, entry)| Change {
                key,
                entry: entry.clone(),
            })
            .collect();
        list.push(Change {
            key: "updated_at",
            entry: Some(updated),
        });
        Ok(frontmatter::assemble(
            &frontmatter::edit(yaml, &spans, &list),
            split.body,
        ))
    }

    /// The collection's schema as its file says now.
    fn read_schema(&self, id: &str) -> CoreResult<SchemaState> {
        let entry = self.entry(id)?;
        let Some(file) = &entry.file else {
            return Err(CoreError::Invalid("This page isn't a collection.".into()));
        };
        let text = fs::read_to_string(file).map_err(io_at(file))?;
        let json = frontmatter::split(&text)
            .yaml
            .and_then(frontmatter::to_json)
            .unwrap_or_default();
        match json.get("collection") {
            Some(value) => Ok(parse_schema(value)),
            None => Err(CoreError::Invalid("This page isn't a collection.".into())),
        }
    }

    fn editable_schema(&self, id: &str) -> CoreResult<Schema> {
        self.writable()?;
        match self.read_schema(id)? {
            SchemaState::Valid(schema) => Ok(schema),
            SchemaState::Newer(_) => Err(CoreError::ReadOnly(
                "This collection was made by a newer version of Forma, so it can't be changed here."
                    .into(),
            )),
            SchemaState::Invalid(problem) => Err(CoreError::ReadOnly(format!(
                "This collection's settings can't be read ({problem}). Fix them in a text editor."
            ))),
        }
    }

    /// The schema, and why it can't be edited (if it can't).
    fn schema_for_display(&self, id: &str) -> CoreResult<(Option<Schema>, Option<String>)> {
        Ok(match self.read_schema(id)? {
            SchemaState::Valid(schema) => (Some(schema), self.read_only_reason.clone()),
            SchemaState::Newer(schema) => (
                Some(schema),
                Some(
                    "This collection was made by a newer version of Forma. It's shown read-only."
                        .into(),
                ),
            ),
            SchemaState::Invalid(problem) => (
                None,
                Some(format!(
                    "This collection's settings can't be read ({problem}). Fix them in a text editor."
                )),
            ),
        })
    }

    /// Rows for `children`, with values read from the index cache.
    fn rows(&self, children: Vec<PageSummary>, schema: Option<&Schema>) -> Vec<CollectionRow> {
        let ids: Vec<String> = children.iter().map(|c| c.id.clone()).collect();
        let frontmatter = self.db.frontmatter(&ids).unwrap_or_else(|err| {
            log::warn!("couldn't read entry values from the index: {err}");
            HashMap::new()
        });
        children
            .into_iter()
            .map(|child| {
                let values = frontmatter.get(&child.id);
                let text = |key: &str| {
                    values
                        .and_then(|v| v.get(key))
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                };
                let cells = schema
                    .iter()
                    .flat_map(|s| &s.properties)
                    .map(|p| (p.id.clone(), coerce(p, values.and_then(|v| v.get(&p.name)))))
                    .collect();
                CollectionRow {
                    created_at: text("created_at"),
                    updated_at: text("updated_at"),
                    editable: child.status == PageStatus::Ok && self.read_only_reason.is_none(),
                    id: child.id,
                    title: child.title,
                    icon: child.icon,
                    cells,
                }
            })
            .collect()
    }

    pub fn get_collection(&self, id: &str) -> CoreResult<CollectionData> {
        let title = self.entry(id)?.title.clone();
        let (schema, problem) = self.schema_for_display(id)?;
        let rows = self.rows(self.list_children(Some(id))?, schema.as_ref());
        Ok(CollectionData {
            id: id.to_string(),
            title,
            schema,
            problem,
            rows,
        })
    }

    /// The entry `page_id` with its collection's schema, or `None` when the
    /// page isn't in a collection.
    pub fn get_entry(&self, page_id: &str) -> CoreResult<Option<EntryData>> {
        let entry = self.entry(page_id)?;
        let Some(collection) = entry
            .parent
            .as_deref()
            .and_then(|p| self.index.get(p))
            .filter(|p| p.is_collection)
        else {
            return Ok(None);
        };
        let (schema, problem) = self.schema_for_display(&collection.id)?;
        let summary = self.summary(entry);
        let row = self
            .rows(vec![summary], schema.as_ref())
            .pop()
            .expect("one summary gives one row");
        Ok(Some(EntryData {
            collection_id: collection.id.clone(),
            collection_title: collection.title.clone(),
            schema,
            problem,
            row,
        }))
    }

    /// Makes `id` a collection (it becomes a bundle, ready for entries).
    pub fn convert_to_collection(&mut self, id: &str) -> CoreResult<()> {
        self.writable()?;
        if self.entry(id)?.is_collection {
            return Ok(());
        }
        self.dir_for_children(id)?;
        let page = self.load(id)?;
        let schema = Schema {
            version: crate::collection::SCHEMA_VERSION,
            properties: Vec::new(),
        };
        let text = Self::with_changes(
            &page.text,
            &[("collection".into(), Some(schema_entry(&schema)))],
        )?;
        self.write_page_file(id, &page.file, &text, Some((&page.text, &page.rev)), None)?;
        self.rescan(None);
        Ok(())
    }

    pub fn create_collection(
        &mut self,
        parent: Option<&str>,
        title: &str,
    ) -> CoreResult<PageSummary> {
        let page = self.create_page(parent, title)?;
        self.convert_to_collection(&page.id)?;
        Ok(self.summary(self.entry(&page.id)?))
    }

    /// "Turn into a regular page": removes the schema. Entries stay as child
    /// pages, and their values stay in their files as plain keys.
    pub fn remove_collection(&mut self, id: &str) -> CoreResult<()> {
        self.writable()?;
        let page = self.load(id)?;
        let text = Self::with_changes(&page.text, &[("collection".into(), None)])?;
        self.write_page_file(id, &page.file, &text, Some((&page.text, &page.rev)), None)?;
        self.rescan(None);
        Ok(())
    }

    /// Sets one entry's value for one property (an absent key when empty).
    pub fn set_property(
        &mut self,
        page_id: &str,
        property_id: &str,
        input: &CellInput,
    ) -> CoreResult<()> {
        self.writable()?;
        let collection = self
            .entry(page_id)?
            .parent
            .clone()
            .filter(|p| self.index.get(p).is_some_and(|e| e.is_collection))
            .ok_or_else(|| CoreError::Invalid("This page isn't in a collection.".into()))?;
        let schema = self.editable_schema(&collection)?;
        let property = schema
            .property(property_id)
            .ok_or_else(|| CoreError::NotFound("This property".into()))?;
        let entry = emit_value(property, input).map_err(CoreError::Invalid)?;
        let page = self.load(page_id)?;
        let text = Self::with_changes(&page.text, &[(property.name.clone(), entry)])?;
        self.write_page_file(
            page_id,
            &page.file,
            &text,
            Some((&page.text, &page.rev)),
            None,
        )?;
        Ok(())
    }

    /// Applies a schema change. Renames rewrite every entry that uses the old
    /// name, as one journaled batch; nothing else touches the entries.
    pub fn update_schema(&mut self, id: &str, op: &SchemaOp) -> CoreResult<Schema> {
        let mut schema = self.editable_schema(id)?;
        let mut renames: Vec<EntryRewrite> = Vec::new();
        let property_index = |schema: &Schema, pid: &str| {
            schema
                .properties
                .iter()
                .position(|p| p.id == pid)
                .ok_or_else(|| CoreError::NotFound("This property".into()))
        };
        match op {
            SchemaOp::AddProperty { name, kind } => {
                let names: Vec<&str> = schema.properties.iter().map(|p| p.name.as_str()).collect();
                let name = check_property_name(name, &names).map_err(CoreError::Invalid)?;
                let ids: Vec<&str> = schema.properties.iter().map(|p| p.id.as_str()).collect();
                let id = new_property_id(&ids);
                schema.properties.push(Property {
                    id,
                    name,
                    kind: *kind,
                    options: Vec::new(),
                });
            }
            SchemaOp::RenameProperty { property_id, name } => {
                let at = property_index(&schema, property_id)?;
                let others: Vec<&str> = schema
                    .properties
                    .iter()
                    .filter(|p| p.id != *property_id)
                    .map(|p| p.name.as_str())
                    .collect();
                let name = check_property_name(name, &others).map_err(CoreError::Invalid)?;
                let old = schema.properties[at].name.clone();
                if old != name {
                    renames.push(EntryRewrite::Key {
                        from: old,
                        to: name.clone(),
                    });
                    schema.properties[at].name = name;
                }
            }
            SchemaOp::SetPropertyType { property_id, kind } => {
                let at = property_index(&schema, property_id)?;
                let property = &mut schema.properties[at];
                property.kind = *kind;
                if !kind.has_options() {
                    property.options.clear();
                }
            }
            SchemaOp::RemoveProperty { property_id } => {
                let at = property_index(&schema, property_id)?;
                schema.properties.remove(at);
            }
            SchemaOp::AddOption { property_id, name } => {
                let at = property_index(&schema, property_id)?;
                let property = &mut schema.properties[at];
                if !property.kind.has_options() {
                    return Err(CoreError::Invalid(
                        "Only select properties have options.".into(),
                    ));
                }
                let names: Vec<&str> = property.options.iter().map(|o| o.name.as_str()).collect();
                let name = check_option_name(name, &names).map_err(CoreError::Invalid)?;
                let ids: Vec<&str> = property.options.iter().map(|o| o.id.as_str()).collect();
                let color = COLORS[property.options.len() % COLORS.len()].to_string();
                property.options.push(SelectOption {
                    id: new_option_id(&ids),
                    name,
                    color,
                });
            }
            SchemaOp::RenameOption {
                property_id,
                option_id,
                name,
            } => {
                let at = property_index(&schema, property_id)?;
                let property = &mut schema.properties[at];
                let names: Vec<&str> = property
                    .options
                    .iter()
                    .filter(|o| o.id != *option_id)
                    .map(|o| o.name.as_str())
                    .collect();
                let name = check_option_name(name, &names).map_err(CoreError::Invalid)?;
                let option = property
                    .options
                    .iter_mut()
                    .find(|o| o.id == *option_id)
                    .ok_or_else(|| CoreError::NotFound("This option".into()))?;
                if option.name != name {
                    renames.push(EntryRewrite::Option {
                        key: property.name.clone(),
                        multi: property.kind == PropertyType::MultiSelect,
                        from: option.name.clone(),
                        to: name.clone(),
                    });
                    option.name = name;
                }
            }
            SchemaOp::SetOptionColor {
                property_id,
                option_id,
                color,
            } => {
                if !COLORS.contains(&color.as_str()) {
                    return Err(CoreError::Invalid(format!("Unknown color {color:?}.")));
                }
                let at = property_index(&schema, property_id)?;
                let option = schema.properties[at]
                    .options
                    .iter_mut()
                    .find(|o| o.id == *option_id)
                    .ok_or_else(|| CoreError::NotFound("This option".into()))?;
                option.color = color.clone();
            }
            SchemaOp::RemoveOption {
                property_id,
                option_id,
            } => {
                let at = property_index(&schema, property_id)?;
                schema.properties[at].options.retain(|o| o.id != *option_id);
            }
        }

        let collection = self.load(id)?;
        let schema_text = Self::with_changes(
            &collection.text,
            &[("collection".into(), Some(schema_entry(&schema)))],
        )?;
        if renames.is_empty() {
            self.write_page_file(
                id,
                &collection.file,
                &schema_text,
                Some((&collection.text, &collection.rev)),
                None,
            )?;
        } else {
            let mut writes = self.entry_rewrites(id, &renames)?;
            writes.push(BatchWrite {
                id: id.to_string(),
                file: collection.file,
                old_text: collection.text,
                new_text: schema_text,
            });
            self.apply_batch("collection_rename", writes)?;
        }
        self.rescan(None);
        Ok(schema)
    }

    /// The new text of every entry a rename affects. Refuses when an entry
    /// can't be edited safely, or already has a key with the new name, so a
    /// rename never leaves the collection half done.
    fn entry_rewrites(
        &mut self,
        id: &str,
        renames: &[EntryRewrite],
    ) -> CoreResult<Vec<BatchWrite>> {
        let children: Vec<String> = self
            .list_children(Some(id))?
            .into_iter()
            .map(|c| c.id)
            .collect();
        let mut writes = Vec::new();
        for child in children {
            let Some(file) = self.index.get(&child).and_then(|e| e.file.clone()) else {
                continue;
            };
            let text = fs::read_to_string(&file).map_err(io_at(&file))?;
            let split = frontmatter::split(&text);
            let Some(yaml) = split.yaml else { continue };
            let json = frontmatter::to_json(yaml).unwrap_or_default();
            let mut changes: Vec<(String, Option<String>)> = Vec::new();
            for rename in renames {
                match rename {
                    EntryRewrite::Key { from, to } => {
                        let Some(value) = json.get(from) else {
                            continue;
                        };
                        if json
                            .keys()
                            .any(|k| k.to_lowercase() == to.to_lowercase() && k != from)
                        {
                            return Err(CoreError::Invalid(format!(
                                "\"{}\" already has a value called \"{to}\". Choose another name.",
                                self.index
                                    .get(&child)
                                    .map_or("An entry", |e| e.title.as_str())
                            )));
                        }
                        changes.push((from.clone(), None));
                        changes.push((to.clone(), Some(reemit(to, value))));
                    }
                    EntryRewrite::Option {
                        key,
                        multi,
                        from,
                        to,
                    } => {
                        let Some(value) = json.get(key) else { continue };
                        let renamed = match (multi, value) {
                            (false, serde_json::Value::String(s)) if s == from => {
                                Some(Value::Str(to.clone()))
                            }
                            (true, serde_json::Value::Array(items))
                                if items.iter().any(|i| i.as_str() == Some(from)) =>
                            {
                                Some(Value::List(
                                    items
                                        .iter()
                                        .map(|i| match i.as_str() {
                                            Some(s) if s == from => to.clone(),
                                            Some(s) => s.to_string(),
                                            None => i.to_string(),
                                        })
                                        .collect(),
                                ))
                            }
                            (true, serde_json::Value::String(s)) if s == from => {
                                Some(Value::Str(to.clone()))
                            }
                            _ => None,
                        };
                        if let Some(value) = renamed {
                            changes.push((
                                key.clone(),
                                Some(frontmatter::emit_entry(&yaml_key(key), &value)),
                            ));
                        }
                    }
                }
            }
            if changes.is_empty() {
                continue;
            }
            let new_text = Self::with_changes(&text, &changes).map_err(|_| {
                CoreError::ReadOnly(format!(
                    "\"{}\" has frontmatter Forma can't edit safely, so nothing was renamed. Fix it in a text editor first.",
                    self.index.get(&child).map_or("An entry", |e| e.title.as_str())
                ))
            })?;
            writes.push(BatchWrite {
                id: child,
                file,
                old_text: text,
                new_text,
            });
        }
        Ok(writes)
    }

    pub fn get_views(&self, id: &str) -> CollectionViews {
        let file: ViewsFile = load_json(&self.views_path())
            .ok()
            .flatten()
            .unwrap_or_default();
        file.collections
            .get(id)
            .map(|v| CollectionViews {
                active_view: v.active_view,
                table: v.views.table.clone(),
                list: v.views.list.clone(),
            })
            .unwrap_or_default()
    }

    /// Saves a collection's view settings. They're advisory: an unreadable
    /// file is replaced, and nothing else depends on it.
    pub fn save_views(&mut self, id: &str, views: &CollectionViews) -> CoreResult<()> {
        self.writable()?;
        self.entry(id)?;
        let path = self.views_path();
        let mut file: ViewsFile = load_json(&path).ok().flatten().unwrap_or_default();
        file.format_version = FORMAT_VERSION;
        file.collections.insert(
            id.to_string(),
            StoredViews {
                active_view: views.active_view,
                views: StoredViewSet {
                    table: views.table.clone(),
                    list: views.list.clone(),
                },
            },
        );
        save_json(&path, &file, &self.staging())
    }

    fn views_path(&self) -> PathBuf {
        self.forma().join("views.json")
    }
}

enum EntryRewrite {
    Key {
        from: String,
        to: String,
    },
    Option {
        key: String,
        multi: bool,
        from: String,
        to: String,
    },
}

/// `value` written again under `key`, the way Forma writes values.
fn reemit(key: &str, value: &serde_json::Value) -> String {
    use serde_json::Value as J;
    let key = yaml_key(key);
    let plain = |s: String| frontmatter::emit_entry(&key, &Value::Plain(s));
    match value {
        J::String(s) => frontmatter::emit_entry(&key, &Value::Str(s.clone())),
        J::Number(n) => plain(n.to_string()),
        J::Bool(b) => plain(b.to_string()),
        J::Array(items) => frontmatter::emit_entry(
            &key,
            &Value::List(
                items
                    .iter()
                    .map(|i| i.as_str().map_or_else(|| i.to_string(), str::to_string))
                    .collect(),
            ),
        ),
        J::Null => plain(String::new()),
        J::Object(_) => plain(value.to_string()),
    }
}
