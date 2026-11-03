//! Collections (DATA_MODEL §6): the schema in a collection page's
//! frontmatter, and how entry values are read and written. Pure functions;
//! the Document Service does the file work.

use rand_free::random_id;
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use ts_rs::TS;

use crate::frontmatter::{Value, emit_entry, quote};
use crate::index::RESERVED_KEYS;

pub const SCHEMA_VERSION: u64 = 1;

pub const COLORS: [&str; 9] = [
    "gray", "brown", "orange", "yellow", "green", "blue", "purple", "pink", "red",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum PropertyType {
    Text,
    Number,
    Date,
    Checkbox,
    Select,
    MultiSelect,
}

impl PropertyType {
    fn name(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Number => "number",
            Self::Date => "date",
            Self::Checkbox => "checkbox",
            Self::Select => "select",
            Self::MultiSelect => "multi_select",
        }
    }

    pub fn has_options(self) -> bool {
        matches!(self, Self::Select | Self::MultiSelect)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SelectOption {
    pub id: String,
    pub name: String,
    pub color: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Property {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    #[ts(rename = "type")]
    pub kind: PropertyType,
    #[serde(default)]
    pub options: Vec<SelectOption>,
}

impl Property {
    pub fn option_named(&self, name: &str) -> Option<&SelectOption> {
        self.options.iter().find(|o| o.name == name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Schema {
    /// JSON numbers reach the webview as `number`, not `bigint`.
    #[ts(type = "number")]
    pub version: u64,
    pub properties: Vec<Property>,
}

impl Schema {
    pub fn property(&self, id: &str) -> Option<&Property> {
        self.properties.iter().find(|p| p.id == id)
    }
}

/// What a page's `collection:` key says.
#[derive(Debug, Clone, PartialEq)]
pub enum SchemaState {
    Valid(Schema),
    /// Written by a newer Forma: shown, never written (DATA_MODEL §6.2).
    Newer(Schema),
    Invalid(String),
}

mod rand_free {
    /// A random ID of `len` characters from `[a-z0-9]`, from UUID v4 bytes.
    pub fn random_id(len: usize) -> String {
        const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
        let mut out = String::with_capacity(len);
        while out.len() < len {
            for byte in uuid::Uuid::new_v4().as_bytes() {
                // Rejection sampling keeps the distribution uniform.
                if *byte < 252 && out.len() < len {
                    out.push(ALPHABET[usize::from(*byte) % 36] as char);
                }
            }
        }
        out
    }
}

pub fn new_property_id(taken: &[&str]) -> String {
    loop {
        let id = random_id(8);
        if !taken.contains(&id.as_str()) {
            return id;
        }
    }
}

pub fn new_option_id(taken: &[&str]) -> String {
    loop {
        let id = random_id(4);
        if !taken.contains(&id.as_str()) {
            return id;
        }
    }
}

fn is_id(s: &str, len: std::ops::RangeInclusive<usize>) -> bool {
    len.contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

/// Checks a property name (DATA_MODEL §6.2) and returns it trimmed.
pub fn check_property_name(name: &str, others: &[&str]) -> Result<String, String> {
    let name = name.trim();
    let length = name.chars().count();
    if !(1..=64).contains(&length) {
        return Err("A property name needs 1 to 64 characters.".into());
    }
    if name.chars().any(char::is_control) || (name.contains('"') && name.contains('\'')) {
        return Err("A property name can't contain line breaks, or both kinds of quotes.".into());
    }
    let lower = name.to_lowercase();
    if RESERVED_KEYS.contains(&lower.as_str()) {
        return Err(format!(
            "\"{name}\" is used by Forma itself. Choose another name."
        ));
    }
    if others.iter().any(|o| o.to_lowercase() == lower) {
        return Err(format!("There's already a property called \"{name}\"."));
    }
    Ok(name.to_string())
}

pub fn check_option_name(name: &str, others: &[&str]) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 100 || name.chars().any(char::is_control) {
        return Err("An option needs 1 to 100 characters, without line breaks.".into());
    }
    if others.contains(&name) {
        return Err(format!("There's already an option called \"{name}\"."));
    }
    Ok(name.to_string())
}

/// Reads and validates the `collection:` value (DATA_MODEL §6.2).
pub fn parse_schema(value: &Json) -> SchemaState {
    let Json::Object(map) = value else {
        return SchemaState::Invalid("`collection` must be a mapping".into());
    };
    let version = match map.get("version").and_then(Json::as_u64) {
        Some(v) => v,
        None => return SchemaState::Invalid("`collection.version` must be a number".into()),
    };
    let properties: Vec<Property> = match map.get("properties") {
        None | Some(Json::Null) => Vec::new(),
        Some(list) => match serde_json::from_value(list.clone()) {
            Ok(props) => props,
            Err(err) => return SchemaState::Invalid(format!("a property is malformed: {err}")),
        },
    };
    let schema = Schema {
        version,
        properties,
    };
    if version > SCHEMA_VERSION {
        return SchemaState::Newer(schema);
    }
    if version != SCHEMA_VERSION {
        return SchemaState::Invalid(format!("unknown collection version {version}"));
    }
    match validate(&schema) {
        Ok(()) => SchemaState::Valid(schema),
        Err(problem) => SchemaState::Invalid(problem),
    }
}

fn validate(schema: &Schema) -> Result<(), String> {
    let mut ids = Vec::new();
    let mut names: Vec<&str> = Vec::new();
    for property in &schema.properties {
        if !is_id(&property.id, 8..=8) || ids.contains(&property.id.as_str()) {
            return Err(format!(
                "property id {:?} isn't 8 unique letters or digits",
                property.id
            ));
        }
        ids.push(&property.id);
        check_property_name(&property.name, &names)?;
        names.push(&property.name);
        let mut option_ids = Vec::new();
        let mut option_names: Vec<&str> = Vec::new();
        for option in &property.options {
            if !is_id(&option.id, 4..=64) || option_ids.contains(&option.id.as_str()) {
                return Err(format!("option id {:?} isn't unique", option.id));
            }
            option_ids.push(&option.id);
            check_option_name(&option.name, &option_names)?;
            option_names.push(&option.name);
            if !COLORS.contains(&option.color.as_str()) {
                return Err(format!("unknown color {:?}", option.color));
            }
        }
    }
    Ok(())
}

/// The `collection:` entry as Forma writes it.
pub fn schema_entry(schema: &Schema) -> String {
    let mut out = format!("collection:\n  version: {}\n", schema.version);
    if schema.properties.is_empty() {
        out.push_str("  properties: []\n");
        return out;
    }
    out.push_str("  properties:\n");
    for p in &schema.properties {
        out.push_str(&format!("    - id: {}\n", p.id));
        out.push_str(&format!("      name: {}\n", quote(&p.name)));
        out.push_str(&format!("      type: {}\n", p.kind.name()));
        if p.kind.has_options() {
            if p.options.is_empty() {
                out.push_str("      options: []\n");
            } else {
                out.push_str("      options:\n");
                for o in &p.options {
                    out.push_str(&format!("        - id: {}\n", o.id));
                    out.push_str(&format!("          name: {}\n", quote(&o.name)));
                    out.push_str(&format!("          color: {}\n", o.color));
                }
            }
        }
    }
    out
}

/// A property name as a YAML key: plain when that reads back the same,
/// otherwise quoted so `frontmatter::top_level_spans` finds it.
pub fn yaml_key(name: &str) -> String {
    let plain = quote(name) == name && !name.contains(':') && !name.contains('#');
    if plain {
        name.to_string()
    } else if !name.contains('\'') {
        format!("'{name}'")
    } else {
        // Names never hold both kinds of quote (`check_property_name`).
        format!("\"{}\"", name.replace('\\', "\\\\"))
    }
}

/// An entry value as shown and sorted (DATA_MODEL §6.3). Invalid values are
/// shown as they are and never rewritten unless the user edits the cell.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum Cell {
    Empty,
    Text {
        value: String,
    },
    Number {
        value: f64,
        /// Written as text ("12"): used for sorting, flagged in the UI.
        from_text: bool,
    },
    Date {
        /// `YYYY-MM-DD`
        value: String,
        /// The file holds a full timestamp; it's kept until the cell is edited.
        from_timestamp: bool,
    },
    Checkbox {
        value: bool,
    },
    Select {
        value: String,
        /// False for a name that isn't one of the property's options.
        known: bool,
    },
    MultiSelect {
        values: Vec<String>,
        /// Names in `values` that aren't options.
        unknown: Vec<String>,
    },
    Invalid {
        raw: String,
        problem: String,
    },
}

fn scalar_text(value: &Json) -> Option<String> {
    match value {
        Json::String(s) => Some(s.clone()),
        Json::Number(n) => Some(n.to_string()),
        Json::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn raw(value: &Json) -> String {
    match value {
        Json::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// A valid calendar date in `YYYY-MM-DD` form.
pub fn is_date(s: &str) -> bool {
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok() && s.len() == 10
}

pub fn coerce(property: &Property, value: Option<&Json>) -> Cell {
    let value = match value {
        None | Some(Json::Null) => return Cell::Empty,
        Some(v) => v,
    };
    let invalid = |problem: &str| Cell::Invalid {
        raw: raw(value),
        problem: problem.into(),
    };
    match property.kind {
        PropertyType::Text => match scalar_text(value) {
            Some(text) if text.is_empty() => Cell::Empty,
            Some(text) => Cell::Text { value: text },
            None => invalid("not text"),
        },
        PropertyType::Number => match value {
            Json::Number(n) => n.as_f64().map_or_else(
                || invalid("not a number"),
                |value| Cell::Number {
                    value,
                    from_text: false,
                },
            ),
            Json::String(s) => match s.trim().parse::<f64>() {
                Ok(value) if value.is_finite() => Cell::Number {
                    value,
                    from_text: true,
                },
                _ => invalid("not a number"),
            },
            _ => invalid("not a number"),
        },
        PropertyType::Date => match value {
            Json::String(s) if is_date(s) => Cell::Date {
                value: s.clone(),
                from_timestamp: false,
            },
            Json::String(s) if chrono::DateTime::parse_from_rfc3339(s).is_ok() => Cell::Date {
                value: s[..10].to_string(),
                from_timestamp: true,
            },
            _ => invalid("not a date"),
        },
        PropertyType::Checkbox => match value {
            Json::Bool(value) => Cell::Checkbox { value: *value },
            _ => invalid("not true or false"),
        },
        PropertyType::Select => match scalar_text(value) {
            Some(name) if name.is_empty() => Cell::Empty,
            Some(name) => Cell::Select {
                known: property.option_named(&name).is_some(),
                value: name,
            },
            None => invalid("not a single option"),
        },
        PropertyType::MultiSelect => {
            let names: Option<Vec<String>> = match value {
                Json::Array(items) => items.iter().map(scalar_text).collect(),
                other => scalar_text(other).map(|s| vec![s]),
            };
            match names {
                Some(names) => {
                    let names: Vec<String> = names.into_iter().filter(|n| !n.is_empty()).collect();
                    if names.is_empty() {
                        return Cell::Empty;
                    }
                    let unknown = names
                        .iter()
                        .filter(|n| property.option_named(n).is_none())
                        .cloned()
                        .collect();
                    Cell::MultiSelect {
                        values: names,
                        unknown,
                    }
                }
                None => invalid("not a list of options"),
            }
        }
    }
}

/// A value the user entered in a cell.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export)]
pub enum CellInput {
    Empty,
    Text { value: String },
    Number { value: f64 },
    Date { value: String },
    Checkbox { value: bool },
    Select { value: String },
    MultiSelect { values: Vec<String> },
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        value.to_string()
    }
}

/// The frontmatter entry for `input`, or `None` to remove the key (an empty
/// value is an absent key, never `null`).
pub fn emit_value(property: &Property, input: &CellInput) -> Result<Option<String>, String> {
    let key = yaml_key(&property.name);
    let value = match (property.kind, input) {
        (_, CellInput::Empty) => return Ok(None),
        (PropertyType::Text, CellInput::Text { value }) => {
            if value.is_empty() {
                return Ok(None);
            }
            Value::Str(value.clone())
        }
        (PropertyType::Number, CellInput::Number { value }) => {
            if !value.is_finite() {
                return Err("That isn't a number.".into());
            }
            Value::Plain(format_number(*value))
        }
        (PropertyType::Date, CellInput::Date { value }) => {
            if !is_date(value) {
                return Err("Dates are written as YYYY-MM-DD.".into());
            }
            Value::Plain(value.clone())
        }
        (PropertyType::Checkbox, CellInput::Checkbox { value }) => Value::Plain(value.to_string()),
        (PropertyType::Select, CellInput::Select { value }) => {
            if property.option_named(value).is_none() {
                return Err(format!("\"{value}\" isn't one of this property's options."));
            }
            Value::Str(value.clone())
        }
        (PropertyType::MultiSelect, CellInput::MultiSelect { values }) => {
            if values.is_empty() {
                return Ok(None);
            }
            if let Some(unknown) = values.iter().find(|v| property.option_named(v).is_none()) {
                return Err(format!(
                    "\"{unknown}\" isn't one of this property's options."
                ));
            }
            Value::List(values.clone())
        }
        _ => return Err("That value doesn't fit this property's type.".into()),
    };
    Ok(Some(emit_entry(&key, &value)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn property(kind: PropertyType) -> Property {
        Property {
            id: "k7q2m9xd".into(),
            name: "Status".into(),
            kind,
            options: vec![
                SelectOption {
                    id: "r4t8".into(),
                    name: "To read".into(),
                    color: "gray".into(),
                },
                SelectOption {
                    id: "p2n6".into(),
                    name: "Reading".into(),
                    color: "blue".into(),
                },
            ],
        }
    }

    #[test]
    fn parses_the_documented_schema() {
        let value = json!({
            "version": 1,
            "properties": [
                {"id": "k7q2m9xd", "name": "Status", "type": "select",
                 "options": [{"id": "r4t8", "name": "To read", "color": "gray"}]},
                {"id": "x3v8b1cw", "name": "Year", "type": "number"},
                {"id": "m5h2j7ka", "name": "Authors", "type": "multi_select", "options": []},
                {"id": "d9f4s2le", "name": "Published", "type": "date"},
                {"id": "c1z6n3ru", "name": "Read", "type": "checkbox"}
            ]
        });
        let SchemaState::Valid(schema) = parse_schema(&value) else {
            panic!("expected a valid schema")
        };
        assert_eq!(schema.properties.len(), 5);
        assert_eq!(schema.properties[1].kind, PropertyType::Number);
    }

    #[test]
    fn rejects_broken_schemas_and_reads_newer_ones() {
        let bad = |props: Json| {
            matches!(
                parse_schema(&json!({"version": 1, "properties": props})),
                SchemaState::Invalid(_)
            )
        };
        assert!(bad(json!([{"id": "SHORT", "name": "A", "type": "text"}])));
        assert!(bad(json!([
            {"id": "aaaaaaaa", "name": "A", "type": "text"},
            {"id": "aaaaaaaa", "name": "B", "type": "text"}
        ])));
        assert!(bad(json!([
            {"id": "aaaaaaaa", "name": "Year", "type": "text"},
            {"id": "bbbbbbbb", "name": "year", "type": "number"}
        ])));
        assert!(
            bad(json!([{"id": "aaaaaaaa", "name": "Title", "type": "text"}])),
            "reserved"
        );
        assert!(bad(
            json!([{"id": "aaaaaaaa", "name": "A", "type": "formula"}])
        ));
        assert!(bad(
            json!([{"id": "aaaaaaaa", "name": "A", "type": "select",
            "options": [{"id": "abcd", "name": "x", "color": "neon"}]}])
        ));
        assert!(matches!(
            parse_schema(&json!({"version": 2})),
            SchemaState::Newer(_)
        ));
        assert!(matches!(
            parse_schema(&json!("table")),
            SchemaState::Invalid(_)
        ));
    }

    #[test]
    fn coercion_follows_the_table() {
        use PropertyType::*;
        let c = |kind, v: Json| coerce(&property(kind), Some(&v));
        assert_eq!(
            c(Text, json!(2017)),
            Cell::Text {
                value: "2017".into()
            }
        );
        assert_eq!(
            c(Text, json!(true)),
            Cell::Text {
                value: "true".into()
            }
        );
        assert!(matches!(c(Text, json!(["a"])), Cell::Invalid { .. }));
        assert_eq!(
            c(Number, json!(4.5)),
            Cell::Number {
                value: 4.5,
                from_text: false
            }
        );
        assert_eq!(
            c(Number, json!("12")),
            Cell::Number {
                value: 12.0,
                from_text: true
            }
        );
        assert!(matches!(c(Number, json!("twelve")), Cell::Invalid { .. }));
        assert_eq!(
            c(Date, json!("2017-06-12")),
            Cell::Date {
                value: "2017-06-12".into(),
                from_timestamp: false
            }
        );
        assert_eq!(
            c(Date, json!("2017-06-12T10:00:00Z")),
            Cell::Date {
                value: "2017-06-12".into(),
                from_timestamp: true
            }
        );
        assert!(matches!(c(Date, json!("2017-13-40")), Cell::Invalid { .. }));
        assert_eq!(c(Checkbox, json!(false)), Cell::Checkbox { value: false });
        assert!(matches!(c(Checkbox, json!("yes")), Cell::Invalid { .. }));
        assert_eq!(
            c(Select, json!("Reading")),
            Cell::Select {
                value: "Reading".into(),
                known: true
            }
        );
        assert_eq!(
            c(Select, json!("Done")),
            Cell::Select {
                value: "Done".into(),
                known: false
            }
        );
        assert_eq!(
            c(MultiSelect, json!("Reading")),
            Cell::MultiSelect {
                values: vec!["Reading".into()],
                unknown: vec![]
            }
        );
        assert_eq!(
            c(MultiSelect, json!(["Reading", "Later"])),
            Cell::MultiSelect {
                values: vec!["Reading".into(), "Later".into()],
                unknown: vec!["Later".into()]
            }
        );
        assert_eq!(coerce(&property(Text), None), Cell::Empty);
        assert_eq!(c(Number, Json::Null), Cell::Empty);
    }

    #[test]
    fn values_are_written_readably_and_empty_means_absent() {
        use PropertyType::*;
        let e = |kind, input: CellInput| emit_value(&property(kind), &input);
        assert_eq!(
            e(Number, CellInput::Number { value: 2017.0 })
                .unwrap()
                .unwrap(),
            "Status: 2017\n"
        );
        assert_eq!(
            e(Number, CellInput::Number { value: 4.5 })
                .unwrap()
                .unwrap(),
            "Status: 4.5\n"
        );
        assert_eq!(
            e(
                Text,
                CellInput::Text {
                    value: "yes".into()
                }
            )
            .unwrap()
            .unwrap(),
            "Status: \"yes\"\n"
        );
        assert_eq!(
            e(
                Date,
                CellInput::Date {
                    value: "2017-06-12".into()
                }
            )
            .unwrap()
            .unwrap(),
            "Status: 2017-06-12\n"
        );
        assert_eq!(
            e(Checkbox, CellInput::Checkbox { value: true })
                .unwrap()
                .unwrap(),
            "Status: true\n"
        );
        assert_eq!(
            e(
                MultiSelect,
                CellInput::MultiSelect {
                    values: vec!["Reading".into(), "To read".into()]
                }
            )
            .unwrap()
            .unwrap(),
            "Status:\n  - Reading\n  - To read\n"
        );
        assert_eq!(e(Text, CellInput::Empty).unwrap(), None);
        assert_eq!(
            e(
                Text,
                CellInput::Text {
                    value: String::new()
                }
            )
            .unwrap(),
            None
        );
        assert!(
            e(
                Select,
                CellInput::Select {
                    value: "Nope".into()
                }
            )
            .is_err()
        );
        assert!(
            e(
                Date,
                CellInput::Date {
                    value: "12/06/2017".into()
                }
            )
            .is_err()
        );
        assert!(e(Checkbox, CellInput::Text { value: "x".into() }).is_err());
    }

    #[test]
    fn written_schemas_and_keys_read_back() {
        let schema = Schema {
            version: 1,
            properties: vec![
                property(PropertyType::Select),
                Property {
                    id: "x3v8b1cw".into(),
                    name: "Due: soon #1".into(),
                    kind: PropertyType::Date,
                    options: vec![],
                },
            ],
        };
        let yaml = schema_entry(&schema);
        let json = crate::frontmatter::to_json(&yaml).unwrap();
        assert_eq!(
            parse_schema(&json["collection"]),
            SchemaState::Valid(schema)
        );

        for name in [
            "Status",
            "Due date",
            "Due: soon",
            "a#b",
            "true",
            "2017",
            "it's",
            "it's: x",
            "say \"hi\"",
        ] {
            let entry = format!("{}: 1\n", yaml_key(name));
            let spans = crate::frontmatter::top_level_spans(&entry).unwrap();
            assert_eq!(spans[0].key, name, "{entry}");
            let json = crate::frontmatter::to_json(&entry).unwrap();
            assert!(json.contains_key(name), "{entry}");
        }
    }

    #[test]
    fn names_are_checked() {
        assert_eq!(check_property_name("  Year ", &[]).unwrap(), "Year");
        assert!(check_property_name("", &[]).is_err());
        assert!(check_property_name(&"x".repeat(65), &[]).is_err());
        assert!(check_property_name("tags", &[]).is_err());
        assert!(check_property_name("YEAR", &["Year"]).is_err());
        assert!(check_property_name("a\nb", &[]).is_err());
        assert!(check_option_name("Done", &["Done"]).is_err());
        let ids: Vec<String> = (0..50).map(|_| new_property_id(&[])).collect();
        assert!(ids.iter().all(|id| is_id(id, 8..=8)));
    }
}
