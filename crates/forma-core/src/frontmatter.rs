//! YAML frontmatter: splitting, parsing, surgical editing and emission
//! (DATA_MODEL §4).
//!
//! Forma never re-serializes a whole frontmatter block. It finds the byte
//! range of each top-level key, replaces only the keys it changes, and leaves
//! every other byte (comments, user keys, order, quoting) untouched.

use saphyr::{LoadableYamlNode, ScalarOwned, YamlOwned};

/// A file split into frontmatter and body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Split<'a> {
    /// The YAML between the fences, without them. `None` when there is no frontmatter.
    pub yaml: Option<&'a str>,
    pub body: &'a str,
}

/// Splits `text` at the frontmatter fences. A file only has frontmatter if it
/// starts with a `---` line and a later line is exactly `---` or `...`;
/// otherwise the leading `---` is a Markdown thematic break.
pub fn split(text: &str) -> Split<'_> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let no_frontmatter = Split {
        yaml: None,
        body: text,
    };
    let Some(after_open) = strip_fence_line(text, "---") else {
        return no_frontmatter;
    };
    let yaml_start = text.len() - after_open.len();
    let mut offset = yaml_start;
    for line in after_open.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        if content == "---" || content == "..." {
            return Split {
                yaml: Some(&text[yaml_start..offset]),
                body: &text[offset + line.len()..],
            };
        }
        offset += line.len();
    }
    no_frontmatter
}

fn strip_fence_line<'a>(text: &'a str, fence: &str) -> Option<&'a str> {
    let rest = text.strip_prefix(fence)?;
    rest.strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))
}

/// The Forma-owned keys of a valid frontmatter block.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fields {
    pub id: Option<String>,
    pub title: Option<String>,
    pub icon: Option<String>,
    pub tags: Vec<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// One top-level key and the byte range of its entry in the YAML text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeySpan {
    pub key: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Parsed {
    /// Parsed, and every top-level key's byte range is known.
    Editable { fields: Fields, spans: Vec<KeySpan> },
    /// Parsed, but the layout prevents safe span edits (DATA_MODEL §4.6).
    ReadOnly { fields: Fields },
    /// Not valid YAML, not a mapping, or an `id` that isn't a UUID (§4.5).
    Invalid { message: String },
}

pub fn parse(yaml: &str) -> Parsed {
    let docs = match YamlOwned::load_from_str(yaml) {
        Ok(docs) => docs,
        Err(err) => {
            return Parsed::Invalid {
                message: err.to_string(),
            };
        }
    };
    let mapping = match docs.into_iter().next() {
        None => return editable_empty(yaml),
        Some(YamlOwned::Mapping(mapping)) => mapping,
        Some(YamlOwned::Value(ScalarOwned::Null)) => return editable_empty(yaml),
        Some(_) => {
            return Parsed::Invalid {
                message: "frontmatter must be a mapping of keys to values".into(),
            };
        }
    };

    let mut fields = Fields::default();
    let mut keys = Vec::new();
    for (key, value) in &mapping {
        let Some(key) = scalar_string(key) else {
            // Non-string keys can't be edited by span.
            return Parsed::ReadOnly { fields };
        };
        match key.as_str() {
            "id" => {
                let id = scalar_string(value).unwrap_or_default();
                if uuid::Uuid::parse_str(&id).is_err() {
                    return Parsed::Invalid {
                        message: format!("`id` must be a UUID, found {id:?}"),
                    };
                }
                fields.id = Some(id.to_lowercase());
            }
            "title" => fields.title = scalar_string(value).map(|t| t.replace(['\n', '\r'], " ")),
            "icon" => fields.icon = scalar_string(value),
            "tags" => fields.tags = string_list(value),
            "created_at" => fields.created_at = scalar_string(value),
            "updated_at" => fields.updated_at = scalar_string(value),
            _ => {}
        }
        keys.push(key);
    }

    match top_level_spans(yaml) {
        Some(spans) if spans.iter().map(|s| &s.key).eq(keys.iter()) => {
            Parsed::Editable { fields, spans }
        }
        _ => Parsed::ReadOnly { fields },
    }
}

fn editable_empty(yaml: &str) -> Parsed {
    // Comments only, or nothing at all.
    let only_comments = yaml
        .lines()
        .all(|l| l.trim().is_empty() || l.trim_start().starts_with('#'));
    if only_comments {
        Parsed::Editable {
            fields: Fields::default(),
            spans: Vec::new(),
        }
    } else {
        Parsed::Invalid {
            message: "frontmatter must be a mapping of keys to values".into(),
        }
    }
}

fn scalar_string(value: &YamlOwned) -> Option<String> {
    match value {
        YamlOwned::Value(ScalarOwned::String(s)) => Some(s.clone()),
        YamlOwned::Value(ScalarOwned::Integer(i)) => Some(i.to_string()),
        YamlOwned::Value(ScalarOwned::FloatingPoint(f)) => Some(f.to_string()),
        YamlOwned::Value(ScalarOwned::Boolean(b)) => Some(b.to_string()),
        YamlOwned::Representation(s, _, _) => Some(s.clone()),
        YamlOwned::Tagged(_, inner) => scalar_string(inner),
        _ => None,
    }
}

fn string_list(value: &YamlOwned) -> Vec<String> {
    match value {
        YamlOwned::Sequence(items) => items.iter().filter_map(scalar_string).collect(),
        other => scalar_string(other).into_iter().collect(),
    }
}

/// The top-level mapping as JSON, for the index cache and collection values.
/// YAML 1.2 core types map directly; dates stay strings. Returns `None` when
/// the frontmatter isn't a valid mapping.
pub fn to_json(yaml: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
    let docs = YamlOwned::load_from_str(yaml).ok()?;
    match docs.into_iter().next() {
        None | Some(YamlOwned::Value(ScalarOwned::Null)) => Some(serde_json::Map::new()),
        Some(YamlOwned::Mapping(mapping)) => {
            let mut out = serde_json::Map::new();
            for (key, value) in &mapping {
                if let Some(key) = scalar_string(key) {
                    out.insert(key, yaml_to_json(value));
                }
            }
            Some(out)
        }
        Some(_) => None,
    }
}

fn yaml_to_json(value: &YamlOwned) -> serde_json::Value {
    use serde_json::Value as J;
    match value {
        YamlOwned::Value(ScalarOwned::Null) => J::Null,
        YamlOwned::Value(ScalarOwned::Boolean(b)) => J::Bool(*b),
        YamlOwned::Value(ScalarOwned::Integer(i)) => J::from(*i),
        YamlOwned::Value(ScalarOwned::FloatingPoint(f)) => serde_json::Number::from_f64(f.0)
            .map(J::Number)
            .unwrap_or_else(|| J::String(f.0.to_string())),
        YamlOwned::Value(ScalarOwned::String(s)) => J::String(s.clone()),
        YamlOwned::Sequence(items) => J::Array(items.iter().map(yaml_to_json).collect()),
        YamlOwned::Mapping(mapping) => J::Object(
            mapping
                .iter()
                .filter_map(|(k, v)| Some((scalar_string(k)?, yaml_to_json(v))))
                .collect(),
        ),
        YamlOwned::Tagged(_, inner) => yaml_to_json(inner),
        other => scalar_string(other).map_or(J::Null, J::String),
    }
}

/// Finds the byte range of every top-level key. A key's range covers its line
/// and the lines of its value, but not trailing blank lines or column-0
/// comments, which stay where they are. Returns `None` for layouts it can't
/// map safely (flow mappings, complex keys, text before the first key).
pub fn top_level_spans(yaml: &str) -> Option<Vec<KeySpan>> {
    let mut spans: Vec<KeySpan> = Vec::new();
    let mut offset = 0;
    // End of the last line that belongs to the current key's value.
    let mut value_end = 0;
    for line in yaml.split_inclusive('\n') {
        let content = line.trim_end_matches(['\n', '\r']);
        let first = content.chars().next();
        let is_blank = content.trim().is_empty();
        let is_comment = first == Some('#');
        let continues_value =
            matches!(first, Some(' ' | '\t')) || content == "-" || content.starts_with("- ");
        if is_blank || is_comment {
            // Neither ends nor extends the current value.
        } else if continues_value {
            spans.last()?;
            value_end = offset + line.len();
        } else {
            let key = top_level_key(content)?;
            if let Some(previous) = spans.last_mut() {
                previous.end = value_end;
            }
            spans.push(KeySpan {
                key,
                start: offset,
                end: offset + line.len(),
            });
            value_end = offset + line.len();
        }
        offset += line.len();
    }
    if let Some(last) = spans.last_mut() {
        last.end = value_end;
    }
    Some(spans)
}

/// Returns the key of a `key: value` line at column 0.
fn top_level_key(line: &str) -> Option<String> {
    let first = line.chars().next()?;
    if first == '"' || first == '\'' {
        let close = line[1..].find(first)? + 1;
        let rest = line[close + 1..].trim_start();
        if !(rest == ":" || rest.starts_with(": ") || rest.starts_with(":\t")) {
            return None;
        }
        let inner = &line[1..close];
        return Some(if first == '"' {
            inner.replace("\\\"", "\"").replace("\\\\", "\\")
        } else {
            inner.replace("''", "'")
        });
    }
    if "?[]{}&*!|>%@`-".contains(first) {
        return None;
    }
    let colon = line.find(':')?;
    let after = &line[colon + 1..];
    if !(after.is_empty() || after.starts_with(' ') || after.starts_with('\t')) {
        return None;
    }
    Some(line[..colon].trim_end().to_string())
}

/// A value Forma writes into frontmatter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Str(String),
    /// Written unquoted (UUIDs and RFC 3339 timestamps).
    Plain(String),
    List(Vec<String>),
}

/// Emits `key: value` lines, ending with a newline (DATA_MODEL §4.7).
pub fn emit_entry(key: &str, value: &Value) -> String {
    match value {
        Value::Str(s) => format!("{key}: {}\n", quote(s)),
        Value::Plain(s) => format!("{key}: {s}\n"),
        Value::List(items) => {
            let mut out = format!("{key}:\n");
            for item in items {
                out.push_str(&format!("  - {}\n", quote(item)));
            }
            out
        }
    }
}

/// Quotes `s` unless plain style reads back as the same string under both
/// YAML 1.2 core and YAML 1.1 (the "Norway problem": `no` is false in 1.1).
pub fn quote(s: &str) -> String {
    if !needs_quotes(s) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn needs_quotes(s: &str) -> bool {
    let Some(first) = s.chars().next() else {
        return true;
    };
    let lower = s.to_ascii_lowercase();
    s.trim() != s
        || "-?:,[]{}#&*!|>'\"%@`".contains(first)
        || s.contains(": ")
        || s.contains(" #")
        || s.ends_with(':')
        || s.chars()
            .any(|c| c.is_control() || u32::from(c) > 0xFFFF || is_emoji_like(c))
        || matches!(
            lower.as_str(),
            "true" | "false" | "yes" | "no" | "on" | "off" | "y" | "n" | "null" | "~"
        )
        || looks_numeric(s)
        || looks_like_date(s)
}

fn is_emoji_like(c: char) -> bool {
    matches!(u32::from(c), 0x2600..=0x27BF | 0xFE0F | 0x200D)
}

fn looks_numeric(s: &str) -> bool {
    let body = s.strip_prefix(['-', '+']).unwrap_or(s);
    let lower = body.to_ascii_lowercase();
    if matches!(lower.as_str(), ".inf" | ".nan") {
        return true;
    }
    if let Some(rest) = lower
        .strip_prefix("0x")
        .or_else(|| lower.strip_prefix("0o"))
        .or_else(|| lower.strip_prefix("0b"))
    {
        return !rest.is_empty() && rest.chars().all(|c| c.is_ascii_hexdigit() || c == '_');
    }
    // Decimal, float, exponent, or 1.1 sexagesimal (e.g. 1:30).
    let digits = lower.chars().filter(|c| c.is_ascii_digit()).count();
    digits > 0
        && lower
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | '_' | ':' | 'e' | '-' | '+'))
        && lower
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit() || c == '.')
}

fn looks_like_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 8 && b[..4].iter().all(u8::is_ascii_digit) && b[4] == b'-' && b[5].is_ascii_digit()
}

/// A change to one top-level key: the new entry text, or `None` to remove it.
pub struct Change<'a> {
    pub key: &'a str,
    pub entry: Option<String>,
}

/// Applies `changes` to `yaml` by replacing, removing or appending whole key
/// entries. Bytes outside the changed entries are kept as they were.
pub fn edit(yaml: &str, spans: &[KeySpan], changes: &[Change<'_>]) -> String {
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();
    let mut appended = String::new();
    for change in changes {
        match spans.iter().find(|s| s.key == change.key) {
            Some(span) => replacements.push((
                span.start,
                span.end,
                change.entry.clone().unwrap_or_default(),
            )),
            None => {
                if let Some(entry) = &change.entry {
                    appended.push_str(entry);
                }
            }
        }
    }
    replacements.sort_by_key(|r| r.0);
    let mut out = String::with_capacity(yaml.len() + appended.len());
    let mut cursor = 0;
    for (start, end, text) in replacements {
        out.push_str(&yaml[cursor..start]);
        out.push_str(&text);
        cursor = end;
    }
    out.push_str(&yaml[cursor..]);
    if !appended.is_empty() {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&appended);
    }
    out
}

/// Builds a file from frontmatter YAML and a body.
pub fn assemble(yaml: &str, body: &str) -> String {
    let mut out = String::with_capacity(yaml.len() + body.len() + 8);
    out.push_str("---\n");
    out.push_str(yaml);
    if !yaml.is_empty() && !yaml.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("---\n");
    out.push_str(body);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "550e8400-e29b-41d4-a716-446655440000";

    fn editable(yaml: &str) -> (Fields, Vec<KeySpan>) {
        match parse(yaml) {
            Parsed::Editable { fields, spans } => (fields, spans),
            other => panic!("expected editable, got {other:?}"),
        }
    }

    #[test]
    fn splits_frontmatter_and_body() {
        let text = format!("---\nid: {ID}\ntitle: Hi\n---\nBody\n");
        let split = split(&text);
        assert_eq!(split.yaml, Some(format!("id: {ID}\ntitle: Hi\n").as_str()));
        assert_eq!(split.body, "Body\n");
    }

    #[test]
    fn leading_rule_without_closing_fence_is_body() {
        let text = "---\nJust a thematic break\n";
        assert_eq!(split(text).yaml, None);
        assert_eq!(split(text).body, text);
    }

    #[test]
    fn handles_crlf_bom_and_dot_terminator() {
        let text = "\u{feff}---\r\ntitle: Hi\r\n...\r\nBody";
        let split = split(text);
        assert_eq!(split.yaml, Some("title: Hi\r\n"));
        assert_eq!(split.body, "Body");
    }

    #[test]
    fn parses_forma_fields_and_keeps_order() {
        let yaml = format!(
            "id: {ID}\ntitle: Reinforcement Learning\nicon: \"🧠\"\ntags:\n  - ai\n  - learning\ncreated_at: 2026-11-02T10:00:00Z\nStatus: Reading\n"
        );
        let (fields, spans) = editable(&yaml);
        assert_eq!(fields.id.as_deref(), Some(ID));
        assert_eq!(fields.title.as_deref(), Some("Reinforcement Learning"));
        assert_eq!(fields.icon.as_deref(), Some("🧠"));
        assert_eq!(fields.tags, vec!["ai", "learning"]);
        assert_eq!(fields.created_at.as_deref(), Some("2026-11-02T10:00:00Z"));
        let keys: Vec<_> = spans.iter().map(|s| s.key.as_str()).collect();
        assert_eq!(
            keys,
            ["id", "title", "icon", "tags", "created_at", "Status"]
        );
    }

    #[test]
    fn invalid_yaml_duplicates_and_bad_ids_are_rejected() {
        assert!(matches!(
            parse("title: [unclosed\n"),
            Parsed::Invalid { .. }
        ));
        assert!(matches!(parse("a: 1\na: 2\n"), Parsed::Invalid { .. }));
        assert!(matches!(parse("- a\n- b\n"), Parsed::Invalid { .. }));
        assert!(matches!(parse("id: not-a-uuid\n"), Parsed::Invalid { .. }));
    }

    #[test]
    fn flow_mapping_is_read_only() {
        assert!(matches!(parse("{title: Hi}\n"), Parsed::ReadOnly { .. }));
    }

    #[test]
    fn surgical_edit_keeps_comments_unknown_keys_and_block_scalars() {
        let yaml = "# my notes\ntitle: Old\nnote: |\n  line one\n\n  line three\n# about tags\ntags: [a, b]\ncustom: {x: 1}\n";
        let (_, spans) = editable(yaml);
        let out = edit(
            yaml,
            &spans,
            &[
                Change {
                    key: "title",
                    entry: Some(emit_entry("title", &Value::Str("New".into()))),
                },
                Change {
                    key: "updated_at",
                    entry: Some(emit_entry(
                        "updated_at",
                        &Value::Plain("2026-11-02T10:00:00Z".into()),
                    )),
                },
            ],
        );
        assert_eq!(
            out,
            "# my notes\ntitle: New\nnote: |\n  line one\n\n  line three\n# about tags\ntags: [a, b]\ncustom: {x: 1}\nupdated_at: 2026-11-02T10:00:00Z\n"
        );
        // The result still parses, with the same unknown keys.
        let (fields, _) = editable(&out);
        assert_eq!(fields.title.as_deref(), Some("New"));
    }

    #[test]
    fn removing_and_replacing_multi_line_values() {
        let yaml = "tags:\n- a\n- b\ntitle: T\n";
        let (_, spans) = editable(yaml);
        let out = edit(
            yaml,
            &spans,
            &[Change {
                key: "tags",
                entry: Some(emit_entry("tags", &Value::List(vec!["x".into()]))),
            }],
        );
        assert_eq!(out, "tags:\n  - x\ntitle: T\n");
        let out = edit(
            yaml,
            &spans,
            &[Change {
                key: "tags",
                entry: None,
            }],
        );
        assert_eq!(out, "title: T\n");
    }

    #[test]
    fn quotes_ambiguous_strings() {
        for (raw, quoted) in [
            ("no", "\"no\""),
            ("Yes", "\"Yes\""),
            ("null", "\"null\""),
            ("2017", "\"2017\""),
            ("1.5", "\"1.5\""),
            ("0x1F", "\"0x1F\""),
            ("1:30", "\"1:30\""),
            ("2026-11-02", "\"2026-11-02\""),
            ("- dash", "\"- dash\""),
            ("a: b", "\"a: b\""),
            ("say \"hi\"", "say \"hi\""),
            ("#tag", "\"#tag\""),
            ("", "\"\""),
            (" padded", "\" padded\""),
            ("line\nbreak", "\"line\\nbreak\""),
            ("🧠", "\"🧠\""),
        ] {
            assert_eq!(quote(raw), quoted, "quote({raw:?})");
        }
        for plain in [
            "Reinforcement Learning",
            "强化学习",
            "C# tips",
            "v1.2 notes",
            "100% done",
        ] {
            assert_eq!(quote(plain), plain, "quote({plain:?})");
        }
    }

    #[test]
    fn emitted_values_read_back_identically() {
        for raw in [
            "no",
            "2017",
            "a: b",
            "say \"hi\"\\n",
            "🧠",
            "-x",
            "x #y",
            "日本語",
        ] {
            let yaml = emit_entry("title", &Value::Str(raw.into()));
            let (fields, _) = editable(&yaml);
            assert_eq!(fields.title.as_deref(), Some(raw), "round trip of {raw:?}");
        }
    }

    #[test]
    fn assemble_round_trips_through_split() {
        let file = assemble("title: T\n", "Body\n");
        assert_eq!(file, "---\ntitle: T\n---\nBody\n");
        let split = split(&file);
        assert_eq!(split.yaml, Some("title: T\n"));
        assert_eq!(split.body, "Body\n");
    }
}

#[cfg(test)]
mod json_tests {
    use super::*;

    #[test]
    fn converts_the_top_level_mapping() {
        let json = to_json(
            "title: Paper\nYear: 2017\nScore: 4.5\nRead: false\nPublished: 2017-06-12\n\
             Authors:\n  - Vaswani\n  - Shazeer\nEmpty:\n",
        )
        .unwrap();
        assert_eq!(json["Year"], 2017);
        assert_eq!(json["Score"], 4.5);
        assert_eq!(json["Read"], false);
        assert_eq!(json["Published"], "2017-06-12");
        assert_eq!(json["Authors"], serde_json::json!(["Vaswani", "Shazeer"]));
        assert!(json["Empty"].is_null());
        assert!(to_json("- a list\n").is_none());
        assert!(to_json("").unwrap().is_empty());
    }
}
