//! The SQLite index (ARCHITECTURE §5, ADR-035): a cache of what the page
//! files say, for fast startup, full-text search and backlinks. Deleting it
//! loses nothing; it is rebuilt from the files.
//!
//! Two full-text tables hold the same pages:
//! - `pages_fts` (unicode61): word search with prefixes and BM25 ranking.
//! - `pages_trigram` (trigram): substring search, used for Chinese, Japanese
//!   and Korean text, which unicode61 can't split into words (ADR-036).
//!
//! Both use the `pages` row's rowid, so one page's rows are replaced by key.

use std::collections::HashMap;
use std::path::Path;

use rusqlite::{Connection, OptionalExtension, Transaction, params};

use crate::extract::{LinkKind, LinkRef};

/// Bump to drop and rebuild every existing index.
pub const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = "
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE pages (
  id            TEXT PRIMARY KEY,
  rel_path      TEXT NOT NULL,
  parent_id     TEXT,
  kind          TEXT NOT NULL,
  title         TEXT NOT NULL,
  icon          TEXT,
  rev           TEXT,
  size          INTEGER,
  mtime_ns      INTEGER,
  id_persisted  INTEGER NOT NULL,
  status        TEXT NOT NULL,
  status_detail TEXT,
  is_collection INTEGER NOT NULL DEFAULT 0,
  frontmatter   TEXT
);
CREATE INDEX pages_parent ON pages(parent_id);
CREATE TABLE tags  (page_id TEXT NOT NULL, tag TEXT NOT NULL, PRIMARY KEY (page_id, tag));
CREATE TABLE links (source_id TEXT NOT NULL, ordinal INTEGER NOT NULL,
                    kind TEXT NOT NULL, target TEXT NOT NULL, label TEXT NOT NULL,
                    PRIMARY KEY (source_id, ordinal));
CREATE INDEX links_target ON links(kind, target);
CREATE VIRTUAL TABLE pages_fts USING fts5(title, body, tags, props,
  tokenize = 'unicode61 remove_diacritics 2');
CREATE VIRTUAL TABLE pages_trigram USING fts5(text, tokenize = 'trigram remove_diacritics 1');
";

/// One page as the tree and the startup scan know it.
#[derive(Debug, Clone, PartialEq)]
pub struct PageRow {
    pub id: String,
    /// The file (leaf) or directory (bundle, folder), relative to the root, with `/`.
    pub rel_path: String,
    pub parent_id: Option<String>,
    /// `leaf`, `bundle` or `folder`.
    pub kind: String,
    pub title: String,
    pub icon: Option<String>,
    pub rev: Option<String>,
    pub size: Option<i64>,
    pub mtime_ns: Option<i64>,
    pub id_persisted: bool,
    /// `ok`, `metadata_readonly` or `invalid_frontmatter`.
    pub status: String,
    pub status_detail: Option<String>,
    pub is_collection: bool,
}

/// What a page's file contains, for search, backlinks and collections.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PageContent {
    pub text: String,
    pub tags: Vec<String>,
    pub links: Vec<LinkRef>,
    pub frontmatter: Option<serde_json::Map<String, serde_json::Value>>,
}

/// A search result. `snippet` marks matches with U+0001 … U+0002.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub id: String,
    pub snippet: String,
}

pub const MARK_START: char = '\u{1}';
pub const MARK_END: char = '\u{2}';

pub struct IndexDb {
    conn: Connection,
}

impl IndexDb {
    /// Opens (or creates) the index at `path`, or an in-memory one. An index
    /// from another schema version is dropped and created again.
    pub fn open(path: Option<&Path>) -> rusqlite::Result<Self> {
        let conn = match path {
            Some(path) => Connection::open(path)?,
            None => Connection::open_in_memory()?,
        };
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
        let version: Option<i64> = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .unwrap_or(None)
            .and_then(|v| v.parse().ok());
        let db = Self { conn };
        if version != Some(SCHEMA_VERSION) {
            db.recreate()?;
        }
        Ok(db)
    }

    /// Opens the index at `path`. A broken file is deleted and created again;
    /// if that fails too, the index lives in memory for this session.
    pub fn open_or_recreate(path: Option<&Path>) -> Self {
        if let Some(path) = path {
            match Self::open(Some(path)) {
                Ok(db) => return db,
                Err(err) => {
                    log::warn!("the search index at {path:?} can't be used ({err}); recreating it");
                    for suffix in ["", "-wal", "-shm"] {
                        let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
                    }
                    match Self::open(Some(path)) {
                        Ok(db) => return db,
                        Err(err) => log::warn!("keeping the search index in memory: {err}"),
                    }
                }
            }
        }
        Self::open(None).expect("an in-memory SQLite database opens")
    }

    fn recreate(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch(
            "DROP TABLE IF EXISTS meta; DROP TABLE IF EXISTS pages; DROP TABLE IF EXISTS tags;
             DROP TABLE IF EXISTS links; DROP TABLE IF EXISTS pages_fts;
             DROP TABLE IF EXISTS pages_trigram;",
        )?;
        self.conn.execute_batch(SCHEMA)?;
        self.conn.execute(
            "INSERT INTO meta (key, value) VALUES ('schema_version', ?1)",
            [SCHEMA_VERSION.to_string()],
        )?;
        Ok(())
    }

    pub fn rows(&self) -> rusqlite::Result<Vec<PageRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, rel_path, parent_id, kind, title, icon, rev, size, mtime_ns,
                    id_persisted, status, status_detail, is_collection FROM pages",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(PageRow {
                id: r.get(0)?,
                rel_path: r.get(1)?,
                parent_id: r.get(2)?,
                kind: r.get(3)?,
                title: r.get(4)?,
                icon: r.get(5)?,
                rev: r.get(6)?,
                size: r.get(7)?,
                mtime_ns: r.get(8)?,
                id_persisted: r.get(9)?,
                status: r.get(10)?,
                status_detail: r.get(11)?,
                is_collection: r.get(12)?,
            })
        })?;
        rows.collect()
    }

    /// Applies one rescan: removes pages, updates rows, and replaces the
    /// content of pages whose files were read. One transaction.
    pub fn apply(
        &mut self,
        removed: &[String],
        rows: &[PageRow],
        contents: &[(String, PageContent)],
    ) -> rusqlite::Result<()> {
        let tx = self.conn.transaction()?;
        for id in removed {
            delete_page(&tx, id)?;
        }
        for row in rows {
            upsert_row(&tx, row)?;
        }
        for (id, content) in contents {
            replace_content(&tx, id, content)?;
        }
        tx.commit()
    }

    /// Cached frontmatter of the given pages (collection entries).
    pub fn frontmatter(
        &self,
        ids: &[String],
    ) -> rusqlite::Result<HashMap<String, serde_json::Map<String, serde_json::Value>>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT frontmatter FROM pages WHERE id = ?1")?;
        let mut out = HashMap::new();
        for id in ids {
            let json: Option<Option<String>> = stmt.query_row([id], |r| r.get(0)).optional()?;
            if let Some(map) = json
                .flatten()
                .and_then(|j| serde_json::from_str::<serde_json::Value>(&j).ok())
                .and_then(|v| match v {
                    serde_json::Value::Object(map) => Some(map),
                    _ => None,
                })
            {
                out.insert(id.clone(), map);
            }
        }
        Ok(out)
    }

    pub fn tags(&self, id: &str) -> rusqlite::Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT tag FROM tags WHERE page_id = ?1 ORDER BY tag")?;
        let tags = stmt.query_map([id], |r| r.get(0))?;
        tags.collect()
    }

    /// Word search (unicode61) with BM25; the title counts most.
    pub fn search_words(&self, fts_query: &str, limit: usize) -> rusqlite::Result<Vec<Hit>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT p.id, snippet(pages_fts, 1, char(1), char(2), '…', 16)
             FROM pages_fts JOIN pages p ON p.rowid = pages_fts.rowid
             WHERE pages_fts MATCH ?1
             ORDER BY bm25(pages_fts, 10.0, 1.0, 4.0, 2.0)
             LIMIT ?2",
        )?;
        let hits = stmt.query_map(params![fts_query, limit as i64], |r| {
            Ok(Hit {
                id: r.get(0)?,
                snippet: r.get(1)?,
            })
        })?;
        hits.collect()
    }

    /// Substring search (trigram): every term must occur. Terms of three or
    /// more characters use the index; shorter ones scan, bounded by `limit`.
    pub fn search_substrings(&self, terms: &[String], limit: usize) -> rusqlite::Result<Vec<Hit>> {
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let conditions = vec!["pages_trigram.text LIKE ? ESCAPE '\\'"; terms.len()].join(" AND ");
        let sql = format!(
            "SELECT p.id, pages_trigram.text FROM pages_trigram
             JOIN pages p ON p.rowid = pages_trigram.rowid
             WHERE {conditions} LIMIT {limit}"
        );
        let patterns: Vec<String> = terms
            .iter()
            .map(|t| format!("%{}%", like_escape(t)))
            .collect();
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(patterns.iter()), |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut hits = Vec::new();
        for row in rows {
            let (id, text) = row?;
            // The first line of `text` is the title (see `replace_content`).
            let body = text.split_once('\n').map_or("", |(_, body)| body);
            hits.push(Hit {
                id,
                snippet: substring_snippet(body, terms),
            });
        }
        Ok(hits)
    }

    /// Pages whose body links to `target` with a page link.
    pub fn page_links_to(&self, target: &str) -> rusqlite::Result<Vec<String>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT DISTINCT source_id FROM links WHERE kind = 'page' AND target = ?1",
        )?;
        let ids = stmt.query_map([target], |r| r.get(0))?;
        ids.collect()
    }

    /// Every wikilink: (source page, target text).
    pub fn wiki_links(&self) -> rusqlite::Result<Vec<(String, String)>> {
        let mut stmt = self
            .conn
            .prepare_cached("SELECT source_id, target FROM links WHERE kind = 'wiki'")?;
        let links = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        links.collect()
    }

    /// The links a page makes, in order.
    pub fn links_from(&self, source: &str) -> rusqlite::Result<Vec<LinkRef>> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT kind, target, label FROM links WHERE source_id = ?1 ORDER BY ordinal",
        )?;
        let links = stmt.query_map([source], |r| {
            let kind: String = r.get(0)?;
            Ok(LinkRef {
                kind: if kind == "page" {
                    LinkKind::Page
                } else {
                    LinkKind::Wiki
                },
                target: r.get(1)?,
                label: r.get(2)?,
            })
        })?;
        links.collect()
    }

    /// Everything the index holds, in a stable order, for comparing two
    /// indexes in tests (incremental versus rebuilt).
    #[cfg(test)]
    pub fn dump(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (sql, width) in [
            (
                "SELECT id, rel_path, parent_id, kind, title, icon, rev, id_persisted, status,
                     is_collection, frontmatter FROM pages ORDER BY id",
                11,
            ),
            ("SELECT page_id, tag FROM tags ORDER BY 1, 2", 2),
            (
                "SELECT source_id, ordinal, kind, target, label FROM links ORDER BY 1, 2",
                5,
            ),
            (
                "SELECT p.id, f.title, f.body, f.tags, f.props FROM pages_fts f
              JOIN pages p ON p.rowid = f.rowid ORDER BY 1",
                5,
            ),
            (
                "SELECT p.id, t.text FROM pages_trigram t JOIN pages p ON p.rowid = t.rowid ORDER BY 1",
                2,
            ),
        ] {
            let mut stmt = self.conn.prepare(sql).unwrap();
            let rows = stmt
                .query_map([], |r| {
                    (0..width)
                        .map(|i| {
                            r.get::<_, rusqlite::types::Value>(i)
                                .map(|v| format!("{v:?}"))
                        })
                        .collect::<rusqlite::Result<Vec<_>>>()
                })
                .unwrap();
            out.extend(rows.map(|r| r.unwrap().join(" | ")));
        }
        out
    }

    pub fn page_count(&self) -> rusqlite::Result<i64> {
        self.conn
            .query_row("SELECT count(*) FROM pages", [], |r| r.get(0))
    }
}

fn rowid(tx: &Transaction<'_>, id: &str) -> rusqlite::Result<Option<i64>> {
    tx.query_row("SELECT rowid FROM pages WHERE id = ?1", [id], |r| r.get(0))
        .optional()
}

fn delete_page(tx: &Transaction<'_>, id: &str) -> rusqlite::Result<()> {
    if let Some(rowid) = rowid(tx, id)? {
        tx.execute("DELETE FROM pages_fts WHERE rowid = ?1", [rowid])?;
        tx.execute("DELETE FROM pages_trigram WHERE rowid = ?1", [rowid])?;
    }
    tx.execute("DELETE FROM pages WHERE id = ?1", [id])?;
    tx.execute("DELETE FROM tags WHERE page_id = ?1", [id])?;
    tx.execute("DELETE FROM links WHERE source_id = ?1", [id])?;
    Ok(())
}

fn upsert_row(tx: &Transaction<'_>, row: &PageRow) -> rusqlite::Result<()> {
    // ON CONFLICT keeps the rowid, which the full-text rows are keyed by.
    tx.execute(
        "INSERT INTO pages (id, rel_path, parent_id, kind, title, icon, rev, size, mtime_ns,
                            id_persisted, status, status_detail, is_collection)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         ON CONFLICT(id) DO UPDATE SET
           rel_path = excluded.rel_path, parent_id = excluded.parent_id, kind = excluded.kind,
           title = excluded.title, icon = excluded.icon, rev = excluded.rev,
           size = excluded.size, mtime_ns = excluded.mtime_ns,
           id_persisted = excluded.id_persisted, status = excluded.status,
           status_detail = excluded.status_detail, is_collection = excluded.is_collection",
        params![
            row.id,
            row.rel_path,
            row.parent_id,
            row.kind,
            row.title,
            row.icon,
            row.rev,
            row.size,
            row.mtime_ns,
            row.id_persisted,
            row.status,
            row.status_detail,
            row.is_collection,
        ],
    )?;
    Ok(())
}

fn replace_content(tx: &Transaction<'_>, id: &str, content: &PageContent) -> rusqlite::Result<()> {
    let Some(rowid) = rowid(tx, id)? else {
        return Ok(());
    };
    let title: String = tx.query_row("SELECT title FROM pages WHERE rowid = ?1", [rowid], |r| {
        r.get(0)
    })?;
    tx.execute(
        "UPDATE pages SET frontmatter = ?1 WHERE rowid = ?2",
        params![
            content
                .frontmatter
                .as_ref()
                .map(|m| serde_json::Value::Object(m.clone()).to_string()),
            rowid
        ],
    )?;

    tx.execute("DELETE FROM tags WHERE page_id = ?1", [id])?;
    for tag in &content.tags {
        tx.execute(
            "INSERT OR IGNORE INTO tags (page_id, tag) VALUES (?1, ?2)",
            [id, tag.as_str()],
        )?;
    }
    tx.execute("DELETE FROM links WHERE source_id = ?1", [id])?;
    for (ordinal, link) in content.links.iter().enumerate() {
        let kind = match link.kind {
            LinkKind::Page => "page",
            LinkKind::Wiki => "wiki",
        };
        tx.execute(
            "INSERT INTO links (source_id, ordinal, kind, target, label) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, ordinal as i64, kind, link.target, link.label],
        )?;
    }

    let props = content
        .frontmatter
        .as_ref()
        .map(user_value_text)
        .unwrap_or_default();
    tx.execute("DELETE FROM pages_fts WHERE rowid = ?1", [rowid])?;
    tx.execute(
        "INSERT INTO pages_fts (rowid, title, body, tags, props) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![rowid, title, content.text, content.tags.join(" "), props],
    )?;
    tx.execute("DELETE FROM pages_trigram WHERE rowid = ?1", [rowid])?;
    tx.execute(
        "INSERT INTO pages_trigram (rowid, text) VALUES (?1, ?2)",
        params![
            rowid,
            format!("{}\n{}\n{}", title.replace('\n', " "), content.text, props)
        ],
    )?;
    Ok(())
}

/// The keys Forma owns (DATA_MODEL §4.2); every other key is a user value.
pub const RESERVED_KEYS: [&str; 8] = [
    "id",
    "title",
    "icon",
    "tags",
    "created_at",
    "updated_at",
    "collection",
    "forma",
];

/// Searchable text of the user keys' values (collection properties and others).
fn user_value_text(frontmatter: &serde_json::Map<String, serde_json::Value>) -> String {
    fn push(value: &serde_json::Value, out: &mut Vec<String>) {
        match value {
            serde_json::Value::String(s) => out.push(s.clone()),
            serde_json::Value::Number(n) => out.push(n.to_string()),
            serde_json::Value::Array(items) => items.iter().for_each(|i| push(i, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for (key, value) in frontmatter {
        if !RESERVED_KEYS.contains(&key.as_str()) {
            push(value, &mut out);
        }
    }
    out.join(" ")
}

fn like_escape(term: &str) -> String {
    term.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// About 80 characters of `body` around the first match, with every match marked.
pub fn substring_snippet(body: &str, terms: &[String]) -> String {
    let lower = body.to_lowercase();
    // Lowercasing can change byte lengths (rarely); fall back to the start then.
    let same_shape = lower.len() == body.len();
    let first = terms
        .iter()
        .filter_map(|t| lower.find(&t.to_lowercase()))
        .min()
        .filter(|_| same_shape)
        .unwrap_or(0);
    let chars: Vec<(usize, char)> = body.char_indices().collect();
    let at = chars.iter().position(|(i, _)| *i >= first).unwrap_or(0);
    let start = at.saturating_sub(24);
    let end = (at + 64).min(chars.len());
    let mut snippet: String = chars[start..end].iter().map(|(_, c)| *c).collect();
    snippet = snippet.replace('\n', " ");
    if same_shape {
        for term in terms {
            snippet = mark_all(&snippet, term);
        }
    }
    let prefix = if start > 0 { "…" } else { "" };
    let suffix = if end < chars.len() { "…" } else { "" };
    format!("{prefix}{}{suffix}", snippet.trim())
}

fn mark_all(text: &str, term: &str) -> String {
    let needle = term.to_lowercase();
    if needle.is_empty() || text.to_lowercase().len() != text.len() {
        return text.to_string();
    }
    let lower = text.to_lowercase();
    let mut out = String::new();
    let mut last = 0;
    for (i, _) in lower.match_indices(&needle) {
        if i < last {
            continue;
        }
        out.push_str(&text[last..i]);
        out.push(MARK_START);
        out.push_str(&text[i..i + needle.len()]);
        out.push(MARK_END);
        last = i + needle.len();
    }
    out.push_str(&text[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, title: &str) -> PageRow {
        PageRow {
            id: id.into(),
            rel_path: format!("{title}.md"),
            parent_id: None,
            kind: "leaf".into(),
            title: title.into(),
            icon: None,
            rev: Some("r".into()),
            size: Some(1),
            mtime_ns: Some(1),
            id_persisted: true,
            status: "ok".into(),
            status_detail: None,
            is_collection: false,
        }
    }

    fn content(text: &str) -> PageContent {
        PageContent {
            text: text.into(),
            ..Default::default()
        }
    }

    #[test]
    fn fts5_and_trigram_are_available() {
        let db = IndexDb::open(None).unwrap();
        let version: String = db
            .conn
            .query_row("SELECT sqlite_version()", [], |r| r.get(0))
            .unwrap();
        assert!(
            version.as_str() >= "3.34",
            "trigram needs SQLite 3.34+: {version}"
        );
        assert_eq!(db.page_count().unwrap(), 0);
    }

    #[test]
    fn word_and_substring_search() {
        let mut db = IndexDb::open(None).unwrap();
        db.apply(
            &[],
            &[row("a", "Attention"), row("b", "知识管理")],
            &[
                ("a".into(), content("Transformer models for protein design")),
                (
                    "b".into(),
                    content("今天我们讨论注意力机制在蛋白质结构预测中的应用"),
                ),
            ],
        )
        .unwrap();
        let hits = db.search_words("\"prot\"*", 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].snippet.contains("\u{1}protein\u{2}"));
        // Title matches rank first.
        let hits = db.search_words("\"attention\"", 10).unwrap();
        assert_eq!(hits[0].id, "a");

        let three = db.search_substrings(&["蛋白质".into()], 10).unwrap();
        assert_eq!(three.len(), 1);
        assert!(three[0].snippet.contains("\u{1}蛋白质\u{2}"));
        let two = db.search_substrings(&["知识".into()], 10).unwrap();
        assert_eq!(
            two.len(),
            1,
            "a two-character query matches the title by scanning"
        );
        let both = db
            .search_substrings(&["注意力".into(), "预测".into()], 10)
            .unwrap();
        assert_eq!(both.len(), 1);
        assert!(
            db.search_substrings(&["50%".into()], 10)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn replacing_and_removing_keeps_tables_in_step() {
        let mut db = IndexDb::open(None).unwrap();
        db.apply(
            &[],
            &[row("a", "One")],
            &[("a".into(), content("old words"))],
        )
        .unwrap();
        let mut renamed = row("a", "Two");
        renamed.rev = Some("r2".into());
        db.apply(&[], &[renamed], &[("a".into(), content("new words"))])
            .unwrap();
        assert!(db.search_words("\"old\"", 10).unwrap().is_empty());
        assert_eq!(db.search_words("\"two\"", 10).unwrap().len(), 1);
        db.apply(&["a".into()], &[], &[]).unwrap();
        assert!(db.search_words("\"new\"", 10).unwrap().is_empty());
        assert!(
            db.search_substrings(&["new".into()], 10)
                .unwrap()
                .is_empty()
        );
        assert_eq!(db.page_count().unwrap(), 0);
    }

    #[test]
    fn links_tags_and_frontmatter_are_cached() {
        let mut db = IndexDb::open(None).unwrap();
        let mut frontmatter = serde_json::Map::new();
        frontmatter.insert("Status".into(), "Reading".into());
        db.apply(
            &[],
            &[row("a", "A")],
            &[(
                "a".into(),
                PageContent {
                    text: "body".into(),
                    tags: vec!["ml".into()],
                    links: vec![
                        LinkRef {
                            kind: LinkKind::Page,
                            target: "b".into(),
                            label: "B".into(),
                        },
                        LinkRef {
                            kind: LinkKind::Wiki,
                            target: "Some Title".into(),
                            label: "Some Title".into(),
                        },
                    ],
                    frontmatter: Some(frontmatter),
                },
            )],
        )
        .unwrap();
        assert_eq!(db.page_links_to("b").unwrap(), vec!["a".to_string()]);
        assert_eq!(
            db.wiki_links().unwrap(),
            vec![("a".to_string(), "Some Title".to_string())]
        );
        assert_eq!(db.tags("a").unwrap(), vec!["ml".to_string()]);
        assert_eq!(
            db.frontmatter(&["a".into()]).unwrap()["a"]["Status"],
            "Reading"
        );
        // Properties are searchable.
        assert_eq!(db.search_words("\"reading\"", 10).unwrap().len(), 1);
    }

    #[test]
    fn a_file_index_survives_reopening_and_a_new_schema_rebuilds_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.db");
        {
            let mut db = IndexDb::open(Some(&path)).unwrap();
            db.apply(&[], &[row("a", "A")], &[("a".into(), content("x"))])
                .unwrap();
        }
        let db = IndexDb::open(Some(&path)).unwrap();
        assert_eq!(db.rows().unwrap(), vec![row("a", "A")]);
        db.conn
            .execute(
                "UPDATE meta SET value = '0' WHERE key = 'schema_version'",
                [],
            )
            .unwrap();
        drop(db);
        let db = IndexDb::open(Some(&path)).unwrap();
        assert_eq!(db.page_count().unwrap(), 0);

        std::fs::write(&path, b"not a database").unwrap();
        let db = IndexDb::open_or_recreate(Some(&path));
        assert_eq!(db.page_count().unwrap(), 0);
    }

    #[test]
    fn snippets_mark_matches_and_stay_short() {
        let body = "a".repeat(200) + "注意力机制" + &"b".repeat(200);
        let snippet = substring_snippet(&body, &["注意力".into()]);
        assert!(snippet.starts_with('…') && snippet.ends_with('…'));
        assert!(snippet.contains("\u{1}注意力\u{2}"));
        assert!(snippet.chars().count() < 100);
    }
}
