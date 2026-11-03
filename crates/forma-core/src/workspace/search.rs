//! Full-text search and quick open (ARCHITECTURE §5.4, ADR-036).

use super::Workspace;
use super::naming::name_key;
use super::types::PageHit;
use crate::index::Hit;

/// True for Chinese, Japanese and Korean characters, which unicode61 can't
/// split into words.
pub(super) fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x3040..=0x30FF      // Hiragana, Katakana
        | 0x3400..=0x4DBF    // CJK Extension A
        | 0x4E00..=0x9FFF    // CJK Unified Ideographs
        | 0xAC00..=0xD7AF    // Hangul syllables
        | 0x1100..=0x11FF    // Hangul Jamo
        | 0xF900..=0xFAFF    // CJK Compatibility Ideographs
        | 0x20000..=0x2FA1F) // Extensions B–F and supplements
}

/// An FTS5 query from what the user typed: every term must match, each one
/// quoted so punctuation is literal, and the last term is a prefix.
pub(super) fn fts_query(terms: &[String]) -> Option<String> {
    let usable: Vec<&String> = terms
        .iter()
        .filter(|t| t.chars().any(char::is_alphanumeric))
        .collect();
    let last = usable.len().checked_sub(1)?;
    Some(
        usable
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let quoted = format!("\"{}\"", t.replace('"', "\"\""));
                if i == last {
                    format!("{quoted}*")
                } else {
                    quoted
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    )
}

impl Workspace {
    /// Searches titles, text, tags and property values. Results are ranked,
    /// with title matches first.
    pub fn search(&self, query: &str, limit: usize) -> Vec<PageHit> {
        let terms: Vec<String> = query.split_whitespace().map(str::to_string).collect();
        if terms.is_empty() {
            return Vec::new();
        }
        let substring = |db: &crate::index::IndexDb| {
            db.search_substrings(&terms, limit).unwrap_or_else(|err| {
                log::warn!("substring search failed: {err}");
                Vec::new()
            })
        };
        let hits: Vec<Hit> = if query.chars().any(is_cjk) {
            substring(&self.db)
        } else {
            let words = fts_query(&terms)
                .map(|q| {
                    self.db.search_words(&q, limit).unwrap_or_else(|err| {
                        log::warn!("word search failed: {err}");
                        Vec::new()
                    })
                })
                .unwrap_or_default();
            // Nothing at word starts: look inside words ("tention" finds "attention").
            if words.is_empty() && terms.iter().any(|t| t.chars().count() >= 3) {
                substring(&self.db)
            } else {
                words
            }
        };
        hits.into_iter()
            .filter_map(|hit| self.hit(&hit.id, Some(hit.snippet)))
            .collect()
    }

    /// Pages whose title contains `query`: titles that start with it first,
    /// then titles with a word that starts with it, then the rest.
    pub fn quick_open(&self, query: &str, limit: usize) -> Vec<PageHit> {
        let needle = name_key(query.trim());
        if needle.is_empty() {
            return Vec::new();
        }
        let mut found: Vec<(u8, usize, String, &str)> = self
            .index
            .entries
            .values()
            .filter_map(|entry| {
                let title = name_key(&entry.title);
                let at = title.find(&needle)?;
                let rank = if at == 0 {
                    0
                } else if title[..at].ends_with(|c: char| !c.is_alphanumeric()) {
                    1
                } else {
                    2
                };
                Some((rank, title.chars().count(), title, entry.id.as_str()))
            })
            .collect();
        found.sort();
        found
            .into_iter()
            .take(limit)
            .filter_map(|(_, _, _, id)| self.hit(id, None))
            .collect()
    }

    pub(super) fn hit(&self, id: &str, snippet: Option<String>) -> Option<PageHit> {
        let entry = self.index.get(id)?;
        Some(PageHit {
            id: entry.id.clone(),
            title: entry.title.clone(),
            icon: entry.icon.clone(),
            path: self.ancestors(id).into_iter().map(|a| a.title).collect(),
            snippet: snippet.filter(|s| !s.trim().is_empty()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_safe_prefix_queries() {
        let terms = |s: &str| s.split_whitespace().map(str::to_string).collect::<Vec<_>>();
        assert_eq!(
            fts_query(&terms("attention prot")).unwrap(),
            "\"attention\" \"prot\"*"
        );
        assert_eq!(
            fts_query(&terms("say \"hi\"")).unwrap(),
            "\"say\" \"\"\"hi\"\"\"*"
        );
        assert_eq!(fts_query(&terms("- ***")), None);
    }

    #[test]
    fn recognizes_cjk() {
        assert!("知识".chars().all(is_cjk));
        assert!("カタカナ".chars().all(is_cjk));
        assert!("한국어".chars().all(is_cjk));
        assert!(!"Notes, café".chars().any(is_cjk));
    }
}
