//! Plain text and links from a page body, for search and backlinks
//! (ARCHITECTURE §5.2). The editor never sees this; it reads the same
//! Markdown with its own parser, and `tests/fixtures/links/` keeps the two
//! in agreement.

use pulldown_cmark::{Event, LinkType, Options, Parser, Tag, TagEnd};
use serde::Deserialize;

/// The URL prefix of a page link (DATA_MODEL §5.4).
pub const PAGE_LINK_PREFIX: &str = "forma://page/";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
    /// `[Label](forma://page/<uuid>)`: the target is a page ID.
    Page,
    /// `[[Target]]` or `[[Target|alias]]`: the target is text, resolved by title.
    Wiki,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LinkRef {
    pub kind: LinkKind,
    pub target: String,
    pub label: String,
}

#[derive(Debug, Default)]
pub struct Extracted {
    /// The readable text: blocks separated by newlines, no markup or URLs.
    pub text: String,
    pub links: Vec<LinkRef>,
}

/// The page ID in a `forma://page/<uuid>[#anchor]` URL, lowercased.
pub fn page_link_target(url: &str) -> Option<String> {
    let rest = url.strip_prefix(PAGE_LINK_PREFIX)?;
    let id = rest.split(['#', '?']).next()?;
    uuid::Uuid::parse_str(id).ok()?;
    Some(id.to_ascii_lowercase())
}

pub fn extract(body: &str) -> Extracted {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_MATH
        | Options::ENABLE_WIKILINKS;
    let mut out = Extracted::default();
    // The link being read: its index in `out.links`, or None for other links.
    let mut open_link: Option<Option<usize>> = None;

    for event in Parser::new_ext(body, options) {
        match event {
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                ..
            }) => {
                let found = if matches!(link_type, LinkType::WikiLink { .. }) {
                    Some(LinkRef {
                        kind: LinkKind::Wiki,
                        target: dest_url.trim().to_string(),
                        label: String::new(),
                    })
                } else {
                    page_link_target(&dest_url).map(|target| LinkRef {
                        kind: LinkKind::Page,
                        target,
                        label: String::new(),
                    })
                };
                open_link = Some(found.map(|link| {
                    out.links.push(link);
                    out.links.len() - 1
                }));
            }
            Event::End(TagEnd::Link) => open_link = None,
            Event::Text(text) | Event::Code(text) | Event::InlineMath(text) => {
                if let Some(Some(i)) = open_link {
                    out.links[i].label.push_str(&text);
                }
                out.text.push_str(&text);
            }
            Event::DisplayMath(text) => {
                out.text.push_str(&text);
                out.text.push('\n');
            }
            Event::SoftBreak | Event::HardBreak => out.text.push(' '),
            Event::End(
                TagEnd::Paragraph
                | TagEnd::Heading(_)
                | TagEnd::Item
                | TagEnd::CodeBlock
                | TagEnd::TableCell
                | TagEnd::TableRow
                | TagEnd::TableHead,
            ) if !out.text.ends_with('\n') => out.text.push('\n'),
            // Raw HTML is never shown (ARCHITECTURE §2.1), so it isn't searched either.
            _ => {}
        }
    }
    out.text = out.text.trim().to_string();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0c6a5f3e-6c1d-4b8e-9a63-2f1d1b0a9e77";

    #[test]
    fn finds_page_links_and_wikilinks() {
        let body = format!(
            "See [Attention](forma://page/{ID}) and [[Reading List]], [[Papers/Old|old one]].\n\n\
             A [web link](https://example.com) and [bad](forma://page/not-a-uuid)."
        );
        let got = extract(&body);
        assert_eq!(
            got.links,
            vec![
                LinkRef {
                    kind: LinkKind::Page,
                    target: ID.into(),
                    label: "Attention".into()
                },
                LinkRef {
                    kind: LinkKind::Wiki,
                    target: "Reading List".into(),
                    label: "Reading List".into()
                },
                LinkRef {
                    kind: LinkKind::Wiki,
                    target: "Papers/Old".into(),
                    label: "old one".into()
                },
            ]
        );
    }

    #[test]
    fn text_has_no_markup_urls_or_html() {
        let got = extract(
            "# Title\n\nSome **bold** and `code` with <span>tags</span>.\n\n\
             | a | b |\n| - | - |\n| 1 | 2 |\n\n![alt text](_assets/x.png) $x^2$",
        );
        assert!(got.text.contains("Title\nSome bold and code with tags."));
        assert!(got.text.contains("alt text"));
        assert!(got.text.contains("x^2"));
        assert!(!got.text.contains("<span>"));
        assert!(!got.text.contains("_assets"));
    }

    /// `tests/fixtures/links/`: the editor's test reads the same files, so
    /// backlinks and the editor agree on what is a link.
    #[test]
    fn link_fixtures_match_the_shared_expectations() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/links");
        let mut checked = 0;
        for item in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = item.path();
            if path.extension().is_none_or(|e| e != "md") {
                continue;
            }
            let body = std::fs::read_to_string(&path).unwrap();
            let expected: Vec<LinkRef> = serde_json::from_str(
                &std::fs::read_to_string(path.with_extension("links.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(extract(&body).links, expected, "{path:?}");
            checked += 1;
        }
        assert!(checked >= 2);
    }

    #[test]
    fn page_link_targets_are_validated_and_lowercased() {
        let upper = ID.to_uppercase();
        assert_eq!(
            page_link_target(&format!("forma://page/{upper}#intro")),
            Some(ID.to_string())
        );
        assert_eq!(page_link_target("forma://page/123"), None);
        assert_eq!(page_link_target("https://example.com"), None);
    }
}
