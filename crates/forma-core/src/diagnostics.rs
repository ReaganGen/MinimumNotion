//! "Copy diagnostics" (IMPLEMENTATION_PLAN §10.1): log lines made safe to
//! share. Forma's logs never contain note text or keys, but they do name
//! files and folders, and an error can quote a page title. Before lines leave
//! the Mac, every quoted string and every path is replaced.

/// Replaces quoted strings ("…" and “…”) and absolute or home paths.
pub fn sanitize_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let closing = match c {
            '"' => Some('"'),
            '“' => Some('”'),
            '‘' => Some('’'),
            _ => None,
        };
        if let Some(close) = closing
            && let Some(end) = chars[i + 1..].iter().position(|&x| x == close)
        {
            out.push(c);
            out.push_str("<redacted>");
            out.push(close);
            i += end + 2;
            continue;
        }
        let starts_token = i == 0 || matches!(chars[i - 1], ' ' | '(' | '[' | '=' | '\t');
        let path_start = c == '/' || (c == '~' && chars.get(i + 1) == Some(&'/'));
        if starts_token && path_start {
            // A path runs to ": ", ")" or the end of the line (it may hold spaces).
            let mut j = i;
            while j < chars.len() {
                if chars[j] == ')' || (chars[j] == ':' && chars.get(j + 1) == Some(&' ')) {
                    break;
                }
                j += 1;
            }
            out.push_str("<path>");
            i = j;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// The last `count` lines of `log`, sanitized.
pub fn sanitized_tail(log: &str, count: usize) -> Vec<String> {
    let lines: Vec<&str> = log.lines().collect();
    lines[lines.len().saturating_sub(count)..]
        .iter()
        .map(|l| sanitize_line(l))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_and_quoted_names_are_removed() {
        assert_eq!(
            sanitize_line(
                r#"[2026-11-03][forma::session][INFO] opened workspace "/Users/me/Private Notes""#
            ),
            r#"[2026-11-03][forma::session][INFO] opened workspace "<redacted>""#
        );
        assert_eq!(
            sanitize_line(
                "workspace operation failed: couldn't access /Users/me/My Notes/Plan.md: Permission denied"
            ),
            "workspace operation failed: couldn't access <path>: Permission denied"
        );
        assert_eq!(
            sanitize_line(
                "workspace operation failed: A folder named “Secret plan” already exists next to this page."
            ),
            "workspace operation failed: A folder named “<redacted>” already exists next to this page."
        );
        assert_eq!(
            // Conservative: a path may hold spaces, so it runs to the ")".
            sanitize_line("staging next to ~/Documents/Forma/Diary.md (other volume)"),
            "staging next to <path>)"
        );
    }

    #[test]
    fn the_rest_of_a_line_is_kept() {
        let line = "[2026-11-03][forma_ai::service][INFO] AI request to api.anthropic.com (claude-x, 3 messages, 2 sources, tools on)";
        assert_eq!(sanitize_line(line), line);
        assert_eq!(
            sanitize_line("rebuilt the search index (5000 pages)"),
            "rebuilt the search index (5000 pages)"
        );
        // An unclosed quote isn't a quoted string.
        assert_eq!(sanitize_line("a \" b"), "a \" b");
        assert_eq!(
            sanitize_line("https://example.com/x"),
            "https://example.com/x"
        );
    }

    #[test]
    fn only_the_last_lines_are_taken() {
        let log = "one /a\ntwo\nthree \"x\"";
        assert_eq!(sanitized_tail(log, 2), ["two", "three \"<redacted>\""]);
        assert_eq!(sanitized_tail(log, 10).len(), 3);
        assert!(sanitized_tail("", 5).is_empty());
    }
}
