//! Rewriting hand-written relative references when a leaf page becomes a
//! bundle (DATA_MODEL §3.1). `Note.md` moves to `Note/index.md`, one level
//! deeper, so `images/a.png` must become `../images/a.png` to keep working.

use pulldown_cmark::{Event, Options, Parser, Tag};

/// True for references that resolve against the page's own folder.
fn is_relative(dest: &str) -> bool {
    let has_scheme = dest.split_once(':').is_some_and(|(scheme, _)| {
        !scheme.is_empty()
            && scheme
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
    });
    !dest.is_empty() && !has_scheme && !dest.starts_with(['/', '#', '?'])
}

/// Prefixes every relative inline link or image destination in `body` with
/// `prefix`. Returns `None` when nothing needs to change. Reference-style
/// definitions are left alone (their destinations live elsewhere in the file).
pub fn prefix_relative_refs(body: &str, prefix: &str) -> Option<String> {
    let mut inserts = Vec::new();
    for (event, range) in Parser::new_ext(body, Options::all()).into_offset_iter() {
        let dest = match &event {
            Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) => dest_url,
            _ => continue,
        };
        if !is_relative(dest) {
            continue;
        }
        let source = &body[range.clone()];
        // The destination follows the last "](" of the link, optionally inside <…>.
        let Some(open) = source.rfind("](") else {
            continue;
        };
        let after = &source[open + 2..];
        let skip = usize::from(after.starts_with('<'));
        if after[skip..].starts_with(dest.as_ref()) {
            inserts.push(range.start + open + 2 + skip);
        }
    }
    if inserts.is_empty() {
        return None;
    }
    let mut out = body.to_string();
    for at in inserts.into_iter().rev() {
        out.insert_str(at, prefix);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_relative_links_and_images_only() {
        let body = "![a](images/a.png) [b](notes/b.md \"t\") [c](https://x.com) [d](/abs) [e](#top) [p](forma://page/1) <https://auto>\n";
        let out = prefix_relative_refs(body, "../").unwrap();
        assert_eq!(
            out,
            "![a](../images/a.png) [b](../notes/b.md \"t\") [c](https://x.com) [d](/abs) [e](#top) [p](forma://page/1) <https://auto>\n"
        );
    }

    #[test]
    fn handles_angle_destinations_and_code() {
        let body = "[x](<my file.md>) and `[y](z.md)` in code\n";
        assert_eq!(
            prefix_relative_refs(body, "../").unwrap(),
            "[x](<../my file.md>) and `[y](z.md)` in code\n"
        );
    }

    #[test]
    fn nothing_to_change() {
        assert_eq!(prefix_relative_refs("[a](https://x.com)\n", "../"), None);
        assert_eq!(prefix_relative_refs("plain text\n", "../"), None);
    }
}
