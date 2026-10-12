//! TEMPORARY private copies of the two shared-discriminator helpers the FDX
//! writer imports in TypeScript: `stripNotes` (src/fountain/notes.ts,
//! registry #18) and `PRIMARY_SLUG` / `isMiniSlug` (src/fountain/slug.ts,
//! registry #5b).
//!
//! TODO(sp-fountain): these belong to sp-fountain, which is being ported in
//! parallel. When it lands, delete this module and import its copies: the
//! whole point of those modules is that there is ONE of each, and this is a
//! second. Until then this copy is held to the oracle (every committed and
//! real book.fdx byte for byte), which exercises both on real scripts.
//!
//! Public only so this crate's integration tests can work out an expected
//! paragraph sequence with the same rules the TypeScript test imports.

use sp_model::{Token, TokenType};

use crate::js;

/// `PRIMARY_SLUG.test(text)`:
/// `/^(?:\*{0,3}_?)?(?:(?:int|i)\.?\/(?:ext|e)|int|ext|est)[. ]/i`.
///
/// Hand-matched rather than through a regex crate so the case-folding is
/// JavaScript's non-Unicode `i`, which folds only ASCII here: a Unicode
/// regex would let U+017F (long s) stand for the `s` of `est`.
pub fn is_primary_slug(text: &str) -> bool {
    let c: Vec<char> = text.chars().collect();
    // The optional prefix never needs backtracking: what follows it must
    // start with a letter, so taking every `*` and `_` it allows is the only
    // way to match.
    let mut p = 0;
    while p < 3 && c.get(p) == Some(&'*') {
        p += 1;
    }
    if c.get(p) == Some(&'_') {
        p += 1;
    }
    let word_at = |at: usize, w: &str| -> Option<usize> {
        let mut i = at;
        for wc in w.chars() {
            if !c.get(i)?.eq_ignore_ascii_case(&wc) {
                return None;
            }
            i += 1;
        }
        Some(i)
    };
    let ends_ok = |at: usize| matches!(c.get(at), Some('.') | Some(' '));

    // (?:int|i)\.?\/(?:ext|e)
    for head in ["int", "i"] {
        let Some(after_head) = word_at(p, head) else {
            continue;
        };
        let mut slash_at = vec![after_head];
        if c.get(after_head) == Some(&'.') {
            slash_at.insert(0, after_head + 1);
        }
        for s in slash_at {
            if c.get(s) != Some(&'/') {
                continue;
            }
            for tail in ["ext", "e"] {
                if word_at(s + 1, tail).is_some_and(ends_ok) {
                    return true;
                }
            }
        }
    }
    ["int", "ext", "est"]
        .iter()
        .any(|w| word_at(p, w).is_some_and(ends_ok))
}

/// `isMiniSlug(t)`: a scene heading only the forced `.SLUG` form could have
/// made, which the FDX writes as a Shot.
pub fn is_mini_slug(t: &Token) -> bool {
    t.kind == TokenType::SceneHeading && !is_primary_slug(t.text_or_empty())
}

/// `stripNotes(text)`: every `[[...]]` removed, then the whitespace the
/// removal left tidied. Text with no `[[` comes back identical.
pub fn strip_notes(text: &str) -> String {
    if !text.contains("[[") {
        return text.to_owned();
    }
    let s = remove_notes(&text.chars().collect::<Vec<_>>());
    let s = collapse_spaces(&s);
    let s = strip_line_starts(&s);
    js::trim(&s.into_iter().collect::<String>()).to_owned()
}

/// `.replace(/\[\[[^\]]*\]\]/g, '')`. An unterminated `[[` matches nothing.
fn remove_notes(s: &[char]) -> Vec<char> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i] == '[' && s.get(i + 1) == Some(&'[') {
            // [^\]]* runs to the first `]`, which must be followed by another.
            if let Some(k) = s[i + 2..].iter().position(|&c| c == ']').map(|k| k + i + 2)
                && s.get(k + 1) == Some(&']')
            {
                i = k + 2;
                continue;
            }
        }
        out.push(s[i]);
        i += 1;
    }
    out
}

fn space_not_newline(c: char) -> bool {
    c != '\n' && js::is_space(c)
}

/// `.replace(/[^\S\n]{2,}/g, ' ')`.
fn collapse_spaces(s: &[char]) -> Vec<char> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if space_not_newline(s[i]) {
            let run = s[i..].iter().take_while(|&&c| space_not_newline(c)).count();
            if run >= 2 {
                out.push(' ');
            } else {
                out.push(s[i]);
            }
            i += run;
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
    out
}

/// `.replace(/^[^\S\n]+/gm, '')`: `^` under `m` is the start of the text or
/// just after any JavaScript line terminator, including `\r`.
fn strip_line_starts(s: &[char]) -> Vec<char> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let line_start = i == 0 || js::is_line_terminator(s[i - 1]);
        if line_start && space_not_newline(s[i]) {
            i += s[i..].iter().take_while(|&&c| space_not_newline(c)).count();
            continue;
        }
        out.push(s[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_slug_openers() {
        for yes in [
            "INT. ROOM",
            "ext. yard",
            "EST. CITY",
            "INT/EXT. CAR",
            "I/E CAR",
            "int./ext. car",
            "**INT. BOLD",
            "***_INT. ALL",
            "_EXT. UNDER",
            "INT ROOM",
        ] {
            assert!(is_primary_slug(yes), "{yes}");
        }
        for no in [
            "LATER",
            "INTERIOR",
            "INT",
            "****INT. FOUR",
            "__INT. TWO",
            "E\u{17F}T. LONG S",
            "I/X CAR",
            ".INT. DOT",
        ] {
            assert!(!is_primary_slug(no), "{no}");
        }
    }

    #[test]
    fn strip_notes_like_the_typescript() {
        assert_eq!(strip_notes("no notes  here "), "no notes  here ");
        assert_eq!(strip_notes("Words [[note]] more."), "Words more.");
        assert_eq!(strip_notes("[[fmt: mono]] Typed"), "Typed");
        assert_eq!(strip_notes("a\n  [[x]] b\nc"), "a\nb\nc");
        assert_eq!(strip_notes("open [[ never closed"), "open [[ never closed");
        assert_eq!(strip_notes("[[[a]] b"), "b");
        assert_eq!(strip_notes("[[a]b]] c"), "[[a]b]] c");
    }
}
