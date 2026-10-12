//! The two JavaScript string facts the writer leans on, spelled out because
//! Rust's own answers differ at the edges.

/// JavaScript's `\s` (and what `String.prototype.trim` removes): ECMAScript
/// WhiteSpace plus LineTerminator. Rust's `char::is_whitespace` differs on
/// two code points: it counts U+0085 (NEL), which JavaScript does not, and
/// misses U+FEFF (the byte-order mark), which JavaScript counts.
pub fn is_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{B}' | '\u{C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// A JavaScript LineTerminator: where `^` matches under the `m` flag.
pub fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `String.prototype.trim`.
pub fn trim(s: &str) -> &str {
    s.trim_matches(is_space)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_follows_javascript_not_rust() {
        assert_eq!(trim("\u{FEFF} a \u{3000}"), "a");
        assert_eq!(trim("\u{85}a\u{85}"), "\u{85}a\u{85}");
    }
}
