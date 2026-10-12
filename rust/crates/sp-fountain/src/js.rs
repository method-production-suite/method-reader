//! JavaScript's string primitives, where Rust's differ.
//!
//! `String.prototype.trim` and the regex class `\s` share one set: the
//! ECMAScript WhiteSpace and LineTerminator characters. Rust's `char::
//! is_whitespace` is Unicode `White_Space`, which adds U+0085 and leaves out
//! U+FEFF, so `str::trim` is not `trim()`. Everything here that the
//! TypeScript spells `trim()` or `\s` goes through these.

/// JavaScript's `\s`: WhiteSpace (tab, VT, FF, space, NBSP, ZWNBSP and every
/// `Zs` character) plus LineTerminator.
pub fn is_space(c: char) -> bool {
    const SPACES: &[char] = &[
        '\t', '\n', '\u{0B}', '\u{0C}', '\r', ' ', '\u{A0}', '\u{1680}', '\u{2028}', '\u{2029}',
        '\u{202F}', '\u{205F}', '\u{3000}', '\u{FEFF}',
    ];
    SPACES.contains(&c) || ('\u{2000}'..='\u{200A}').contains(&c)
}

/// JavaScript's LineTerminator: what `.` refuses and where `^`/`$` stop
/// under the `m` flag.
pub fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// JavaScript's `\w`: ASCII letters, digits and `_`, nothing else.
pub fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `s.trim()`.
pub fn trim(s: &str) -> &str {
    s.trim_matches(is_space)
}

/// `s.trimStart()`.
pub fn trim_start(s: &str) -> &str {
    s.trim_start_matches(is_space)
}

/// `s.trimEnd()`.
pub fn trim_end(s: &str) -> &str {
    s.trim_end_matches(is_space)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_is_javascripts_not_rusts() {
        // U+FEFF is JavaScript whitespace and not Unicode White_Space.
        assert_eq!(trim("\u{FEFF} a \u{FEFF}"), "a");
        // U+0085 is the reverse.
        assert_eq!(trim("\u{85}a\u{85}"), "\u{85}a\u{85}");
        assert_eq!(trim_start("  a "), "a ");
        assert_eq!(trim_end("  a "), "  a");
    }
}
