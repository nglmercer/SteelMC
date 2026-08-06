//! Java standard-library behavior used by vanilla parsing.

/// Returns whether Java's `Character.isWhitespace` recognizes `character`.
#[must_use]
pub const fn is_whitespace(character: char) -> bool {
    matches!(
        character,
        '\u{0009}'..='\u{000d}'
            | '\u{001c}'..='\u{0020}'
            | '\u{1680}'
            | '\u{2000}'..='\u{2006}'
            | '\u{2008}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{205f}'
            | '\u{3000}'
    )
}

/// Returns whether Java's `Character.isSpaceChar` recognizes `character`.
#[must_use]
pub const fn is_space_char(character: char) -> bool {
    matches!(
        character,
        '\u{0020}' | '\u{00a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}'
    )
}

/// Mirrors vanilla `StringUtil.isBlank`.
#[must_use]
pub fn is_blank(value: &str) -> bool {
    value
        .chars()
        .all(|character| is_whitespace(character) || is_space_char(character))
}

/// Mirrors Java's `Math.round(float)`: half-up, unlike Rust's `f32::round` which is
/// half-away-from-zero. The two differ for negative halves (`-1.5` rounds to `-1`, not `-2`).
#[must_use]
pub fn round_to_i32(value: f32) -> i32 {
    (value + 0.5).floor() as i32
}

#[cfg(test)]
mod tests {
    use super::{is_blank, is_space_char, is_whitespace, round_to_i32};

    #[test]
    fn round_to_i32_matches_java_half_up() {
        // Java rounds halves up (toward positive infinity), so negative halves differ from
        // Rust's `f32::round`, which rounds away from zero.
        assert_eq!(round_to_i32(-1.5), -1);
        assert_eq!(round_to_i32(1.5), 2);
        assert_eq!(round_to_i32(-2.5), -2);
        assert_eq!(round_to_i32(2.4), 2);
    }

    #[test]
    fn matches_java_whitespace_exclusions() {
        assert!(is_whitespace(' '));
        assert!(is_whitespace('\u{1680}'));
        for non_breaking_space in ['\u{0085}', '\u{00a0}', '\u{2007}', '\u{202f}'] {
            assert!(!is_whitespace(non_breaking_space));
        }
    }

    #[test]
    fn space_char_includes_unicode_space_separators() {
        for space in [' ', '\u{00a0}', '\u{2007}', '\u{202f}'] {
            assert!(is_space_char(space));
        }
        assert!(!is_space_char('\u{0085}'));
    }

    #[test]
    fn blank_combines_java_whitespace_and_space_char() {
        assert!(is_blank(""));
        assert!(is_blank("\u{001c}\u{00a0}\u{202f}"));
        assert!(!is_blank("\u{0085}"));
        assert!(!is_blank(" text "));
    }
}
