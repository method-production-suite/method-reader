//! The JavaScript string and number semantics the TypeScript parser leaned
//! on without saying so, spelled out once so every module agrees.
//!
//! Each of these differs from Rust's own in a way a screenplay can reach:
//!
//! - `String.prototype.length` counts UTF-16 code units, not chars or bytes,
//!   and every length threshold in the parser was tuned against it.
//! - `trim()` and `\s` use ECMAScript's WhiteSpace + LineTerminator set,
//!   which includes U+FEFF and excludes U+0085, unlike Rust's `trim()`.
//! - `Math.round` rounds a half toward +infinity (`-2.5` gives `-2`), where
//!   `f64::round` rounds it away from zero.
//! - A regex `/i` flag (no `u`) folds case with `toUpperCase` and refuses to
//!   map a non-ASCII character onto an ASCII one, so the long s (U+017F) and
//!   the Kelvin sign (U+212A) never match `s` and `k`; the regex crate's
//!   Unicode folding would match them. [`ci`] closes that gap.

use std::borrow::Cow;

/// ECMAScript `\s`, as a regex character-class body (no brackets).
pub const WS: &str = r"\t\n\x0B\x0C\r \u{A0}\u{1680}\u{2000}-\u{200A}\u{2028}\u{2029}\u{202F}\u{205F}\u{3000}\u{FEFF}";

/// Expand the JavaScript-only escapes in a pattern written the way the
/// TypeScript wrote it: `\s` and `\S` become ECMAScript's whitespace set,
/// `\d` becomes ASCII digits, and `.` (outside a class) stops at the four
/// ECMAScript line terminators rather than only at `\n`. Only the escapes
/// the parser's patterns use are handled; a backreference is not
/// expressible and is written as code where the TypeScript used one.
pub fn pattern(js: &str) -> String {
    let mut out = String::with_capacity(js.len() * 2);
    let mut chars = js.chars().peekable();
    let mut in_class = false;
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                let Some(n) = chars.next() else {
                    out.push('\\');
                    break;
                };
                match n {
                    's' if in_class => out.push_str(WS),
                    's' => {
                        out.push('[');
                        out.push_str(WS);
                        out.push(']');
                    }
                    'S' if !in_class => {
                        out.push_str("[^");
                        out.push_str(WS);
                        out.push(']');
                    }
                    'd' if in_class => out.push_str("0-9"),
                    'd' => out.push_str("[0-9]"),
                    'b' if !in_class => out.push_str(r"(?-u:\b)"),
                    _ => {
                        out.push('\\');
                        out.push(n);
                    }
                }
            }
            '[' if !in_class => {
                in_class = true;
                out.push('[');
                // A leading `]` or `^]` is literal in both dialects; carry it.
                if chars.peek() == Some(&'^') {
                    out.push(chars.next().unwrap_or('^'));
                }
            }
            ']' if in_class => {
                in_class = false;
                out.push(']');
            }
            '.' if !in_class => out.push_str(r"[^\n\r\u{2028}\u{2029}]"),
            _ => out.push(c),
        }
    }
    out
}

/// Compile a JavaScript-dialect pattern (see [`pattern`]).
///
/// # Panics
/// On a pattern that does not compile, which is a bug in this crate.
pub fn re(js: &str) -> regex::Regex {
    regex::Regex::new(&pattern(js)).unwrap_or_else(|e| panic!("pattern {js}: {e}"))
}

/// Compile a JavaScript-dialect pattern with the `/i` flag.
///
/// # Panics
/// On a pattern that does not compile, which is a bug in this crate.
pub fn re_i(js: &str) -> regex::Regex {
    regex::RegexBuilder::new(&pattern(js))
        .case_insensitive(true)
        .build()
        .unwrap_or_else(|e| panic!("pattern {js}: {e}"))
}

/// The haystack an `/i` pattern should search: the two characters whose
/// Unicode simple case folding reaches ASCII but whose JavaScript folding
/// does not (U+017F long s, U+212A Kelvin sign) are swapped for characters of
/// the same UTF-8 length that no pattern here mentions, so match offsets stay
/// valid against the original string.
pub fn ci(s: &str) -> Cow<'_, str> {
    if s.contains(['\u{17F}', '\u{212A}']) {
        Cow::Owned(
            s.chars()
                .map(|c| match c {
                    '\u{17F}' => '\u{1C2}',
                    '\u{212A}' => '\u{2FFF}',
                    other => other,
                })
                .collect(),
        )
    } else {
        Cow::Borrowed(s)
    }
}

/// ECMAScript whitespace (`\s`).
pub fn is_ws(c: char) -> bool {
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

/// `String.prototype.trim`.
pub fn trim(s: &str) -> &str {
    s.trim_matches(is_ws)
}

/// `String.prototype.length`: UTF-16 code units.
pub fn len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// `Math.round`: a half rounds toward +infinity.
pub fn round(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let f = x.floor();
    if x - f >= 0.5 { f + 1.0 } else { f }
}

/// `Number.isInteger`.
pub fn is_integer(x: f64) -> bool {
    x.is_finite() && x.fract() == 0.0
}

/// `s.split(/\s+/)` on a string already trimmed (no empty first or last
/// piece, matching what the TypeScript always split).
pub fn split_ws(s: &str) -> Vec<&str> {
    s.split(is_ws).filter(|p| !p.is_empty()).collect()
}

/// `s.replace(/\s+/g, ' ')`.
pub fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for c in s.chars() {
        if is_ws(c) {
            if !in_ws {
                out.push(' ');
            }
            in_ws = true;
        } else {
            out.push(c);
            in_ws = false;
        }
    }
    out
}

/// `s.replace(/ {2,}/g, ' ')`: runs of plain spaces only.
pub fn collapse_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for c in s.chars() {
        if c == ' ' {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    out
}

/// A `(a, b) => a - b` comparator over numbers, as `Array.prototype.sort`
/// reads it: a NaN difference counts as equal.
pub fn cmp_num(a: f64, b: f64) -> std::cmp::Ordering {
    a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_is_math_round() {
        assert!((round(2.5) - 3.0).abs() < f64::EPSILON);
        assert!((round(-2.5) - -2.0).abs() < f64::EPSILON);
        assert!(round(0.499_999_999_999_999_94).abs() < f64::EPSILON);
        assert!((round(1.4) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn length_counts_utf16_units() {
        assert_eq!(len("abc"), 3);
        assert_eq!(len("\u{1F600}"), 2);
        assert_eq!(len("é"), 1);
    }

    #[test]
    fn trim_uses_ecmascript_whitespace() {
        assert_eq!(trim("\u{FEFF} a \u{A0}"), "a");
        assert_eq!(trim("\u{85}a"), "\u{85}a");
    }

    #[test]
    fn case_insensitive_never_folds_non_ascii_to_ascii() {
        let r = re_i("^est");
        assert!(r.is_match(&ci("EST")));
        assert!(!r.is_match(&ci("e\u{17F}t")));
    }

    #[test]
    fn dot_and_space_classes_follow_ecmascript() {
        assert!(re(r"^\s$").is_match("\u{FEFF}"));
        assert!(!re(r"^\s$").is_match("\u{85}"));
        assert!(!re(r"^.$").is_match("\u{2028}"));
        assert!(!re(r"^\d$").is_match("\u{0663}"));
    }
}
