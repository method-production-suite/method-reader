//! The few JavaScript string rules the writers lean on, spelled out.
//!
//! The TypeScript reads text with `String.prototype.trim` and regular
//! expressions whose `\s` is ECMAScript's whitespace set, which is neither
//! Rust's `char::is_whitespace` (that one has U+0085 and lacks U+FEFF) nor
//! the regex crate's `\s`. A byte-identical port needs the JavaScript set,
//! so it lives here once and both Kindle crates use it.

/// ECMAScript's `WhiteSpace` and `LineTerminator`: what `\s` matches and
/// what `trim` removes.
pub fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// The same set as a regex character-class body, for building patterns:
/// `[{JS_SPACE}]` is JavaScript's `\s`, `[^{JS_SPACE}]` its `\S`.
pub const JS_SPACE: &str = r"\t\n\x0B\x0C\r \x{A0}\x{1680}\x{2000}-\x{200A}\x{2028}\x{2029}\x{202F}\x{205F}\x{3000}\x{FEFF}";

/// `String.prototype.trim`.
pub fn trim(s: &str) -> &str {
    s.trim_matches(is_js_space)
}

/// `s.split(/\s+/)`, JavaScript's way: an empty string, or one that starts
/// or ends with whitespace, yields an empty first or last piece.
pub fn split_ws(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut chars = s.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if is_js_space(c) {
            out.push(&s[start..i]);
            let mut end = i + c.len_utf8();
            while let Some(&(j, d)) = chars.peek() {
                if !is_js_space(d) {
                    break;
                }
                end = j + d.len_utf8();
                chars.next();
            }
            start = end;
        }
    }
    out.push(&s[start..]);
    out
}

/// A line terminator as JavaScript's multiline `^` and `.` see one.
pub fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_is_javascripts() {
        assert_eq!(trim("\u{FEFF} a \u{A0}"), "a");
        assert_eq!(trim("\u{85}a"), "\u{85}a");
    }

    #[test]
    fn split_matches_javascript() {
        assert_eq!(split_ws(""), vec![""]);
        assert_eq!(split_ws("a  b"), vec!["a", "b"]);
        assert_eq!(split_ws(" a"), vec!["", "a"]);
        assert_eq!(split_ws("a "), vec!["a", ""]);
    }
}
