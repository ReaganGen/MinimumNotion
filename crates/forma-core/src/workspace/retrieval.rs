//! Passage retrieval for the AI assistant (ARCHITECTURE §11, ADR-022,
//! ADR-041): BM25 over the passage index, behind the `Retriever` trait so
//! a hybrid retriever can replace it later without changing callers.
//!
//! A question becomes two queries. Its words go to the word index, any of
//! them may match, and English words are stemmed. CJK text has no word
//! breaks, so its three-character pieces go to the trigram index, where a
//! passage sharing more pieces ranks higher. The two rankings are merged
//! by reciprocal rank.

use std::collections::{HashMap, HashSet};

use super::is_cjk;
use super::read::ReadView;
use crate::index::{IndexDb, PassageRow};

/// At most this many passages come from one page, so an answer can draw
/// on several pages.
const MAX_PER_PAGE: usize = 3;
/// Reciprocal rank fusion's usual constant.
const RRF_K: f64 = 60.0;
const MAX_WORDS: usize = 32;
const MAX_TRIGRAMS: usize = 64;
/// Rows a scan for two-character CJK terms may read.
const SCAN_LIMIT: usize = 2000;
/// Pages whose passages are ranked for a CJK question.
const PAGE_POOL: usize = 30;

/// Words that say little about what a question is about.
const STOPWORDS: &[&str] = &[
    "a",
    "about",
    "after",
    "all",
    "also",
    "am",
    "an",
    "and",
    "any",
    "are",
    "as",
    "at",
    "be",
    "been",
    "before",
    "but",
    "by",
    "can",
    "could",
    "did",
    "do",
    "does",
    "doing",
    "explain",
    "for",
    "from",
    "give",
    "had",
    "has",
    "have",
    "he",
    "her",
    "here",
    "him",
    "his",
    "how",
    "i",
    "if",
    "in",
    "into",
    "is",
    "it",
    "its",
    "just",
    "list",
    "me",
    "more",
    "most",
    "my",
    "no",
    "not",
    "note",
    "notes",
    "of",
    "on",
    "or",
    "our",
    "out",
    "please",
    "say",
    "says",
    "she",
    "should",
    "show",
    "so",
    "some",
    "summarize",
    "tell",
    "than",
    "that",
    "the",
    "their",
    "them",
    "then",
    "there",
    "these",
    "they",
    "this",
    "those",
    "to",
    "up",
    "us",
    "was",
    "we",
    "were",
    "what",
    "when",
    "where",
    "which",
    "who",
    "whom",
    "why",
    "will",
    "with",
    "would",
    "write",
    "you",
    "your",
];

#[derive(Debug, Clone, PartialEq)]
pub struct RetrievedPassage {
    pub page_id: String,
    pub ordinal: u32,
    pub heading_path: Vec<String>,
    /// Byte range in the page body.
    pub start: usize,
    pub end: usize,
    pub markdown: String,
    pub text: String,
}

#[derive(Debug, Clone, Default)]
pub struct RetrievalFilter {
    /// Pages whose passages aren't wanted, such as one already in the context.
    pub exclude_pages: Vec<String>,
}

pub trait Retriever {
    /// Up to `k` passages that best match `query`, best first.
    fn retrieve(&self, query: &str, k: usize, filter: &RetrievalFilter) -> Vec<RetrievedPassage>;
}

#[derive(Debug, Default, PartialEq)]
pub(crate) struct QueryTerms {
    pub words: Vec<String>,
    pub trigrams: Vec<String>,
    /// CJK pieces of two characters, for when no trigram matches.
    pub bigrams: Vec<String>,
}

fn push_unique(list: &mut Vec<String>, item: String, max: usize) {
    if list.len() < max && !list.contains(&item) {
        list.push(item);
    }
}

pub(crate) fn query_terms(query: &str) -> QueryTerms {
    let mut terms = QueryTerms::default();
    let lower = query.to_lowercase();
    let mut word = String::new();
    let mut run: Vec<char> = Vec::new();
    let finish_word = |word: &mut String, terms: &mut QueryTerms| {
        let w = std::mem::take(word);
        let long_enough = w.chars().count() >= 2 || w.chars().all(|c| c.is_ascii_digit());
        if !w.is_empty() && long_enough && !STOPWORDS.contains(&w.as_str()) {
            push_unique(&mut terms.words, w, MAX_WORDS);
        }
    };
    let finish_run = |run: &mut Vec<char>, terms: &mut QueryTerms| {
        for piece in run.windows(3) {
            push_unique(&mut terms.trigrams, piece.iter().collect(), MAX_TRIGRAMS);
        }
        for piece in run.windows(2) {
            push_unique(&mut terms.bigrams, piece.iter().collect(), MAX_WORDS);
        }
        run.clear();
    };
    for c in lower.chars() {
        if is_cjk(c) {
            finish_word(&mut word, &mut terms);
            run.push(c);
        } else if c.is_alphanumeric() {
            finish_run(&mut run, &mut terms);
            word.push(c);
        } else {
            finish_word(&mut word, &mut terms);
            finish_run(&mut run, &mut terms);
        }
    }
    finish_word(&mut word, &mut terms);
    finish_run(&mut run, &mut terms);
    terms
}

/// `"a" OR "b"`: each term quoted, so punctuation and FTS5 syntax are literal.
fn any_of(terms: &[String]) -> Option<String> {
    (!terms.is_empty()).then(|| {
        terms
            .iter()
            .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" OR ")
    })
}

impl Retriever for ReadView<'_> {
    fn retrieve(&self, query: &str, k: usize, filter: &RetrievalFilter) -> Vec<RetrievedPassage> {
        if k == 0 {
            return Vec::new();
        }
        let db = &self.workspace().db;
        let terms = query_terms(query);
        let pool = (k * 4).clamp(20, 100);
        let ranked = |result: rusqlite::Result<Vec<i64>>| {
            result.unwrap_or_else(|err| {
                log::warn!("passage search failed: {err}");
                Vec::new()
            })
        };
        let mut rankings: Vec<Vec<i64>> = Vec::new();
        if let Some(q) = any_of(&terms.words) {
            rankings.push(ranked(db.search_passage_words(&q, pool)));
        }
        if !terms.trigrams.is_empty() || !terms.bigrams.is_empty() {
            rankings.push(cjk_ranking(db, &terms));
        }

        let mut scores: HashMap<i64, f64> = HashMap::new();
        for ranking in &rankings {
            for (rank, rowid) in ranking.iter().enumerate() {
                *scores.entry(*rowid).or_default() += 1.0 / (RRF_K + rank as f64 + 1.0);
            }
        }
        let mut order: Vec<(i64, f64)> = scores.into_iter().collect();
        order.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));

        let excluded: HashSet<&str> = filter.exclude_pages.iter().map(String::as_str).collect();
        let mut per_page: HashMap<String, usize> = HashMap::new();
        let mut out = Vec::new();
        for (rowid, _) in order {
            let Ok(Some(row)) = db.passage(rowid) else {
                continue;
            };
            // The index may briefly know a page the scan just removed.
            if excluded.contains(row.page_id.as_str()) || self.title(&row.page_id).is_none() {
                continue;
            }
            let count = per_page.entry(row.page_id.clone()).or_default();
            if *count >= MAX_PER_PAGE {
                continue;
            }
            *count += 1;
            out.push(retrieved(row));
            if out.len() == k {
                break;
            }
        }
        out
    }
}

/// CJK passages, best first. The page trigram index finds the pages (or,
/// for two-character words, a bounded scan does); their passages are then
/// ranked by how many of the question's pieces they contain.
fn cjk_ranking(db: &IndexDb, terms: &QueryTerms) -> Vec<i64> {
    let mut pages = any_of(&terms.trigrams)
        .map(|q| warned(db.search_page_trigrams(&q, PAGE_POOL)))
        .unwrap_or_default();
    // Two-character words ("知识") are shorter than a trigram.
    if pages.is_empty() && !terms.bigrams.is_empty() {
        pages = warned(db.scan_pages(&terms.bigrams, SCAN_LIMIT));
        pages.truncate(PAGE_POOL);
    }
    let mut scored = Vec::new();
    for (page_rank, page) in pages.iter().enumerate() {
        for (rowid, text) in warned(db.passage_texts(page)) {
            let text = text.to_lowercase();
            let count =
                |pieces: &[String]| pieces.iter().filter(|p| text.contains(p.as_str())).count();
            // A shared trigram says more than a shared pair of characters.
            let score = 3 * count(&terms.trigrams) + count(&terms.bigrams);
            if score > 0 {
                scored.push((score, page_rank, rowid));
            }
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    scored.into_iter().map(|(_, _, rowid)| rowid).collect()
}

fn warned<T>(result: rusqlite::Result<Vec<T>>) -> Vec<T> {
    result.unwrap_or_else(|err| {
        log::warn!("CJK passage search failed: {err}");
        Vec::new()
    })
}

fn retrieved(row: PassageRow) -> RetrievedPassage {
    RetrievedPassage {
        page_id: row.page_id,
        ordinal: row.ordinal,
        heading_path: row.heading_path,
        start: row.start,
        end: row.end,
        markdown: row.markdown,
        text: row.text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn questions_become_words_and_cjk_pieces() {
        let terms = query_terms("What do my notes say about Protein folding?");
        assert_eq!(terms.words, ["protein", "folding"]);
        assert!(terms.trigrams.is_empty());

        let terms = query_terms("注意力机制有什么用？GPU 2024");
        assert_eq!(terms.words, ["gpu", "2024"]);
        assert_eq!(
            terms.trigrams,
            [
                "注意力",
                "意力机",
                "力机制",
                "机制有",
                "制有什",
                "有什么",
                "什么用"
            ]
        );
        assert!(terms.bigrams.contains(&"注意".to_string()));

        let terms = query_terms("知识");
        assert!(terms.trigrams.is_empty());
        assert_eq!(terms.bigrams, ["知识"]);
        assert_eq!(query_terms("the a ? !"), QueryTerms::default());
    }

    /// `tests/fixtures/retrieval/`: pages in English and Chinese, and
    /// questions labeled with the page that answers them (or none).
    #[test]
    fn labeled_questions_find_their_pages() {
        let fixtures =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/retrieval");
        let dir = tempfile::tempdir().unwrap();
        for item in std::fs::read_dir(fixtures.join("pages")).unwrap().flatten() {
            std::fs::copy(item.path(), dir.path().join(item.file_name())).unwrap();
        }
        let ws = crate::workspace::Workspace::open(dir.path()).unwrap();
        let view = ws.read_view();
        let cases: Vec<serde_json::Value> =
            serde_json::from_str(&std::fs::read_to_string(fixtures.join("queries.json")).unwrap())
                .unwrap();
        let (mut top1, mut top3, mut answerable) = (0, 0, 0);
        for case in &cases {
            let query = case["query"].as_str().unwrap();
            let titles: Vec<String> = view
                .retrieve(query, 5, &RetrievalFilter::default())
                .iter()
                .filter_map(|p| view.title(&p.page_id))
                .collect();
            match case["expected"].as_str() {
                None => assert!(titles.is_empty(), "{query}: {titles:?}"),
                Some(expected) => {
                    answerable += 1;
                    let rank = titles.iter().position(|t| t == expected);
                    top1 += usize::from(rank == Some(0));
                    top3 += usize::from(rank.is_some_and(|r| r < 3));
                    assert!(
                        rank.is_some_and(|r| r < 3),
                        "{query}: expected {expected} in the top 3, got {titles:?}"
                    );
                }
            }
        }
        println!("retrieval: top-1 {top1}/{answerable}, top-3 {top3}/{answerable}");
        assert!(top1 * 10 >= answerable * 8, "top-1 {top1}/{answerable}");
    }

    #[test]
    fn excluded_pages_and_the_per_page_cap_are_respected() {
        let dir = tempfile::tempdir().unwrap();
        let long = (0..12)
            .map(|i| format!("## Part {i}\n\nGlaciers move slowly, part {i}."))
            .collect::<Vec<_>>()
            .join("\n\n");
        std::fs::write(dir.path().join("Ice.md"), long).unwrap();
        std::fs::write(dir.path().join("Other.md"), "Glaciers carve valleys.").unwrap();
        let ws = crate::workspace::Workspace::open(dir.path()).unwrap();
        let view = ws.read_view();
        let found = view.retrieve("glaciers", 10, &RetrievalFilter::default());
        let ice = ws
            .list_children(None)
            .unwrap()
            .into_iter()
            .find(|p| p.title == "Ice")
            .unwrap();
        assert_eq!(
            found.iter().filter(|p| p.page_id == ice.id).count(),
            MAX_PER_PAGE
        );
        assert_eq!(found.len(), MAX_PER_PAGE + 1);
        let filter = RetrievalFilter {
            exclude_pages: vec![ice.id.clone()],
        };
        let found = view.retrieve("glaciers", 10, &filter);
        assert_eq!(found.len(), 1);
        assert_eq!(view.title(&found[0].page_id).as_deref(), Some("Other"));
        assert!(found[0].markdown.contains("valleys"));
    }

    #[test]
    fn queries_quote_every_term() {
        assert_eq!(
            any_of(&["say".into(), "\"hi\"".into()]).unwrap(),
            "\"say\" OR \"\"\"hi\"\"\""
        );
        assert_eq!(any_of(&[]), None);
    }
}
