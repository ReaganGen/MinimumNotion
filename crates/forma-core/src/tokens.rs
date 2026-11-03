//! A conservative token estimate (ARCHITECTURE §11): about four characters
//! per token for Latin text and one per character for CJK. It errs high, so
//! text that fits by this count fits for real. Passages are sized with it,
//! and the AI layer packs context with it.

use crate::workspace::is_cjk;

pub fn estimate(text: &str) -> u32 {
    let (cjk, other) = text.chars().fold((0u32, 0u32), |(cjk, other), c| {
        if is_cjk(c) {
            (cjk + 1, other)
        } else {
            (cjk, other + 1)
        }
    });
    cjk + other.div_ceil(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin_and_cjk_are_counted_differently() {
        assert_eq!(estimate(""), 0);
        assert_eq!(estimate("abcd"), 1);
        assert_eq!(estimate("abcde"), 2);
        assert_eq!(estimate("注意力机制"), 5);
        assert_eq!(estimate("注意 attention"), 2 + 3);
    }
}
