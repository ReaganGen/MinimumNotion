//! File and folder names derived from page titles (DATA_MODEL §3.3).
//! Names are human-friendly, never authoritative: the title lives in frontmatter.

use std::collections::HashSet;
use std::path::Path;

use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

/// Longest stem in UTF-8 bytes. Keeps deep trees under macOS's 1024-byte PATH_MAX.
pub const MAX_STEM_BYTES: usize = 100;
/// Names that collide with Diaspar's own structure.
const RESERVED: [&str; 3] = ["index", "_assets", ".diaspar"];

/// Turns a title into a safe, portable file stem.
pub fn sanitize_stem(title: &str) -> String {
    let normalized: String = title.nfc().collect();
    let mut cleaned = String::with_capacity(normalized.len());
    let mut last_was_space = false;
    for c in normalized.chars() {
        let c = match c {
            '\n' | '\r' | '\t' => ' ',
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '-',
            c if c.is_control() => '-',
            c => c,
        };
        if c == ' ' {
            if !last_was_space {
                cleaned.push(c);
            }
            last_was_space = true;
        } else {
            cleaned.push(c);
            last_was_space = false;
        }
    }
    let trimmed = cleaned.trim_matches([' ', '.']);
    let mut stem = String::new();
    for grapheme in trimmed.graphemes(true) {
        if stem.len() + grapheme.len() > MAX_STEM_BYTES {
            break;
        }
        stem.push_str(grapheme);
    }
    let stem = stem.trim_end_matches([' ', '.']).to_string();
    if stem.is_empty() {
        return "Untitled".into();
    }
    if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(&stem)) {
        return format!("{stem} 1");
    }
    stem
}

/// The comparison key APFS uses by default: case- and normalization-insensitive.
pub fn name_key(name: &str) -> String {
    name.nfc().collect::<String>().to_lowercase()
}

/// Stems already claimed in `dir`. `Name.md` and the folder `Name/` claim the same slot.
pub fn taken_stems(dir: &Path) -> HashSet<String> {
    let mut taken = HashSet::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return taken;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let stem = match name.rsplit_once('.') {
            Some((stem, ext)) if is_markdown_extension(ext) => stem.to_string(),
            _ => name,
        };
        taken.insert(name_key(&stem));
    }
    taken
}

/// Returns `stem`, or `stem 2`, `stem 3`… — the first one not in `taken`.
/// `current` (the page's own stem when renaming) doesn't count as taken.
pub fn unique_stem(stem: &str, taken: &HashSet<String>, current: Option<&str>) -> String {
    let free = |candidate: &str| {
        let key = name_key(candidate);
        current.is_some_and(|c| name_key(c) == key) || !taken.contains(&key)
    };
    if free(stem) {
        return stem.to_string();
    }
    (2..)
        .map(|n| format!("{stem} {n}"))
        .find(|candidate| free(candidate))
        .expect("an unbounded range always yields a free name")
}

pub fn is_markdown_extension(ext: &str) -> bool {
    ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_unsafe_characters_and_collapses_space() {
        assert_eq!(sanitize_stem("A/B: test?"), "A-B- test-");
        assert_eq!(
            sanitize_stem("  many   spaces\tand\nlines  "),
            "many spaces and lines"
        );
        assert_eq!(sanitize_stem(".hidden."), "hidden");
        assert_eq!(sanitize_stem("强化学习"), "强化学习");
    }

    #[test]
    fn empty_and_reserved_names() {
        assert_eq!(sanitize_stem(""), "Untitled");
        assert_eq!(sanitize_stem(" ... "), "Untitled");
        assert_eq!(sanitize_stem("Index"), "Index 1");
        assert_eq!(sanitize_stem("_assets"), "_assets 1");
    }

    #[test]
    fn truncates_on_grapheme_boundaries() {
        let long = "日本語".repeat(40); // 3 bytes per character
        let stem = sanitize_stem(&long);
        assert!(stem.len() <= MAX_STEM_BYTES);
        assert_eq!(stem.len() % 3, 0);
        let flags = "🇯🇵".repeat(30); // 8 bytes per flag grapheme
        assert_eq!(sanitize_stem(&flags).len() % 8, 0);
    }

    #[test]
    fn uniqueness_ignores_case_and_normalization() {
        let taken: HashSet<String> = ["notes", "café"].iter().map(|s| name_key(s)).collect();
        assert_eq!(unique_stem("Notes", &taken, None), "Notes 2");
        // "e" + combining acute (NFD) collides with precomposed "é".
        assert_eq!(unique_stem("cafe\u{301}", &taken, None), "cafe\u{301} 2");
        assert_eq!(unique_stem("Ideas", &taken, None), "Ideas");
        assert_eq!(unique_stem("NOTES", &taken, Some("notes")), "NOTES");
    }

    #[test]
    fn file_and_folder_claim_the_same_slot() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Plan.md"), "").unwrap();
        std::fs::create_dir(dir.path().join("Research")).unwrap();
        let taken = taken_stems(dir.path());
        assert!(taken.contains("plan"));
        assert!(taken.contains("research"));
        assert_eq!(unique_stem("research", &taken, None), "research 2");
    }
}
