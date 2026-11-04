//! Passages: a page body cut at headings and paragraphs into pieces of
//! about 200–400 tokens (ARCHITECTURE §11, DATA_MODEL §9). Each keeps the
//! headings above it and its byte range in the body, so the assistant can
//! cite it and Diaspar can show where it is.

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag};

use crate::extract::extract;
use crate::tokens::estimate;

/// A passage with at least this many tokens takes no more blocks…
pub const TARGET_TOKENS: u32 = 250;
/// …and one never grows past this. Longer blocks are cut at lines, then
/// sentences, then characters.
pub const MAX_TOKENS: u32 = 400;

#[derive(Debug, Clone, PartialEq)]
pub struct Passage {
    /// The headings above the passage, outermost first, including the one
    /// it starts with.
    pub heading_path: Vec<String>,
    /// Byte range in the body.
    pub start: usize,
    pub end: usize,
    /// `body[start..end]`.
    pub markdown: String,
    /// The readable text, without markup: what search sees.
    pub text: String,
}

/// A top-level block, or a piece of one that was too long.
struct Block {
    start: usize,
    end: usize,
    heading: Option<(HeadingLevel, String)>,
}

fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_MATH
        | Options::ENABLE_WIKILINKS
}

fn blocks(body: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut open: Option<Block> = None;
    for (event, range) in Parser::new_ext(body, options()).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                if depth == 0 {
                    let heading = match tag {
                        Tag::Heading { level, .. } => Some((level, String::new())),
                        _ => None,
                    };
                    open = Some(Block {
                        start: range.start,
                        end: range.end,
                        heading,
                    });
                }
                depth += 1;
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                if depth == 0
                    && let Some(block) = open.take()
                {
                    out.push(block);
                }
            }
            Event::Text(text) | Event::Code(text) | Event::InlineMath(text) if depth > 0 => {
                if let Some(Block {
                    heading: Some((_, title)),
                    ..
                }) = &mut open
                {
                    title.push_str(&text);
                }
            }
            // A block that is a single event, such as a rule.
            _ if depth == 0 => out.push(Block {
                start: range.start,
                end: range.end,
                heading: None,
            }),
            _ => {}
        }
    }
    out
}

/// Cuts `body[start..end]` into ranges of at most `MAX_TOKENS`: at line
/// breaks, then after sentences, then between characters.
fn pieces(body: &str, start: usize, end: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for (line_start, line) in split_after(&body[start..end], |c, _| c == '\n') {
        let line_start = start + line_start;
        if estimate(line) <= MAX_TOKENS {
            out.push((line_start, line_start + line.len()));
            continue;
        }
        let sentence_end = |c: char, next: Option<char>| match c {
            '。' | '！' | '？' | '；' => true,
            '.' | '!' | '?' | ';' => next.is_none_or(char::is_whitespace),
            _ => false,
        };
        for (sentence_start, sentence) in split_after(line, sentence_end) {
            let sentence_start = line_start + sentence_start;
            if estimate(sentence) <= MAX_TOKENS {
                out.push((sentence_start, sentence_start + sentence.len()));
            } else {
                out.extend(
                    by_size(sentence)
                        .into_iter()
                        .map(|(a, b)| (sentence_start + a, sentence_start + b)),
                );
            }
        }
    }
    out
}

/// Splits after every character where `cut(char, next char)` holds.
fn split_after(text: &str, cut: impl Fn(char, Option<char>) -> bool) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut from = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let next = chars.peek().map(|(_, n)| *n);
        if cut(c, next) {
            let to = i + c.len_utf8();
            out.push((from, &text[from..to]));
            from = to;
        }
    }
    if from < text.len() {
        out.push((from, &text[from..]));
    }
    out
}

/// Ranges of `text` that each estimate to at most `MAX_TOKENS`.
fn by_size(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut from = 0;
    // Counted as `estimate` counts: one per CJK character, four others per token.
    let (mut cjk, mut other) = (0u32, 0u32);
    for (i, c) in text.char_indices() {
        if cjk + other.div_ceil(4) >= MAX_TOKENS {
            out.push((from, i));
            from = i;
            (cjk, other) = (0, 0);
        }
        if crate::workspace::is_cjk(c) {
            cjk += 1;
        } else {
            other += 1;
        }
    }
    if from < text.len() {
        out.push((from, text.len()));
    }
    out
}

struct Open {
    start: usize,
    end: usize,
    tokens: u32,
    only_headings: bool,
    path: Vec<String>,
}

pub fn chunk(body: &str) -> Vec<Passage> {
    let mut units = Vec::new();
    for block in blocks(body) {
        if block.heading.is_none() && estimate(&body[block.start..block.end]) > MAX_TOKENS {
            units.extend(
                pieces(body, block.start, block.end)
                    .into_iter()
                    .map(|(start, end)| Block {
                        start,
                        end,
                        heading: None,
                    }),
            );
        } else {
            units.push(block);
        }
    }

    let mut passages = Vec::new();
    let mut headings: Vec<(HeadingLevel, String)> = Vec::new();
    let mut open: Option<Open> = None;
    let close = |open: &mut Option<Open>, passages: &mut Vec<Passage>| {
        if let Some(o) = open.take() {
            let markdown = body[o.start..o.end].to_string();
            let text = extract(&markdown).text;
            if !text.trim().is_empty() {
                passages.push(Passage {
                    heading_path: o.path,
                    start: o.start,
                    end: o.end,
                    markdown,
                    text,
                });
            }
        }
    };
    for unit in units {
        let tokens = estimate(&body[unit.start..unit.end]);
        if let Some((level, title)) = unit.heading {
            headings.retain(|(l, _)| *l < level);
            headings.push((level, title.trim().to_string()));
            let path = headings.iter().map(|(_, t)| t.clone()).collect();
            // Headings with nothing under them yet stay with what follows.
            if let Some(o) = open.as_mut().filter(|o| o.only_headings) {
                o.end = unit.end;
                o.tokens += tokens;
                o.path = path;
                continue;
            }
            close(&mut open, &mut passages);
            open = Some(Open {
                start: unit.start,
                end: unit.end,
                tokens,
                only_headings: true,
                path,
            });
            continue;
        }
        if open.as_ref().is_some_and(|o| {
            !o.only_headings && (o.tokens >= TARGET_TOKENS || o.tokens + tokens > MAX_TOKENS)
        }) {
            close(&mut open, &mut passages);
        }
        match open.as_mut() {
            Some(o) => {
                o.end = unit.end;
                o.tokens += tokens;
                o.only_headings = false;
            }
            None => {
                open = Some(Open {
                    start: unit.start,
                    end: unit.end,
                    tokens,
                    only_headings: false,
                    path: headings.iter().map(|(_, t)| t.clone()).collect(),
                })
            }
        }
    }
    close(&mut open, &mut passages);
    passages
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paragraph(words: usize, word: &str) -> String {
        vec![word; words].join(" ")
    }

    #[test]
    fn short_pages_are_one_passage_and_empty_ones_none() {
        assert!(chunk("").is_empty());
        assert!(chunk("\n\n  \n").is_empty());
        let body = "Some text.\n\nMore text.\n";
        let got = chunk(body);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].text, "Some text.\nMore text.");
        assert!(got[0].heading_path.is_empty());
        assert_eq!(&body[got[0].start..got[0].end], got[0].markdown);
    }

    #[test]
    fn headings_start_passages_and_give_their_path() {
        let body = "Intro.\n\n# Methods\n\n## Data\n\nWe used **cells**.\n\n## Models\n\nA transformer.\n\n# Results\n\nIt worked.\n";
        let got = chunk(body);
        let summary: Vec<(Vec<&str>, &str)> = got
            .iter()
            .map(|p| {
                (
                    p.heading_path.iter().map(String::as_str).collect(),
                    p.text.as_str(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                (vec![], "Intro."),
                (vec!["Methods", "Data"], "Methods\nData\nWe used cells."),
                (vec!["Methods", "Models"], "Models\nA transformer."),
                (vec!["Results"], "Results\nIt worked."),
            ]
        );
        for p in &got {
            assert_eq!(&body[p.start..p.end], p.markdown);
        }
    }

    #[test]
    fn passages_stay_within_the_size_limits() {
        // 20 paragraphs of 50 words (about 60 tokens each).
        let body = (0..20)
            .map(|_| paragraph(50, "word"))
            .collect::<Vec<_>>()
            .join("\n\n");
        let got = chunk(&body);
        assert!(got.len() >= 3, "{}", got.len());
        for p in &got {
            let tokens = estimate(&p.markdown);
            assert!(tokens <= MAX_TOKENS, "{tokens}");
        }
        // All but the last reach the target.
        for p in &got[..got.len() - 1] {
            assert!(estimate(&p.markdown) >= TARGET_TOKENS - 70);
        }
    }

    #[test]
    fn long_blocks_are_cut_at_lines_sentences_then_characters() {
        let code = format!(
            "```\n{}\n```",
            (0..300)
                .map(|i| format!("let x{i} = {i};"))
                .collect::<Vec<_>>()
                .join("\n")
        );
        let sentences = (0..200)
            .map(|_| "This is one sentence.")
            .collect::<Vec<_>>()
            .join(" ");
        let chinese = "注意力机制".repeat(200);
        for body in [code, sentences, chinese] {
            let got = chunk(&body);
            assert!(got.len() >= 2, "{body:.40}");
            let mut covered = 0;
            for p in &got {
                assert!(estimate(&p.markdown) <= MAX_TOKENS);
                assert!(p.start >= covered);
                covered = p.end;
            }
            assert_eq!(covered, body.len());
        }
    }

    #[test]
    fn a_heading_before_a_long_block_joins_its_first_piece() {
        let body = format!("# Notes\n\n{}", "长".repeat(1000));
        let got = chunk(&body);
        assert!(got[0].markdown.starts_with("# Notes"));
        assert!(got.iter().all(|p| p.heading_path == ["Notes"]));
    }

    #[test]
    fn markup_only_passages_are_left_out() {
        let got = chunk("<div>raw</div>\n\n# Title\n\nText");
        assert_eq!(got.len(), 1, "{got:?}");
        assert_eq!(got[0].heading_path, ["Title"]);
    }
}
