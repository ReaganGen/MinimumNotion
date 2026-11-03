//! A conservative token estimate (ARCHITECTURE §11): about four characters
//! per token for Latin text and one per character for CJK. It errs high, so
//! a conversation that fits by this count fits for real.

use forma_core::workspace::is_cjk;

use crate::types::Message;

/// Per-message overhead for role markers and separators.
const PER_MESSAGE: u32 = 4;

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

pub fn estimate_messages(system: Option<&str>, messages: &[Message]) -> u32 {
    let system = system.map_or(0, |s| estimate(s) + PER_MESSAGE);
    messages
        .iter()
        .map(|m| estimate(&m.content) + PER_MESSAGE)
        .sum::<u32>()
        + system
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
