//! TODO(sp-fountain): a PRIVATE, TEMPORARY copy of the parts of
//! src/fountain/notes.ts, slug.ts and cover.ts the EPUB renderer asks. Those
//! modules exist so every renderer asks ONE definition (registry #5b, #18,
//! #19b); sp-fountain owns the Rust port, and when it lands this file is
//! deleted and its callers import sp-fountain's instead. Kept to exactly
//! what html.rs needs, and written to the TypeScript's regexes character
//! for character (JavaScript `\s`, non-multiline `$`, ASCII-only `/i`).

use sp_model::{FormatOptions, Token, TokenType};

use crate::js;

/// `ImageRef`: what a picture note asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageRef {
    pub file: String,
    pub width_pct: u32,
    pub full_page: bool,
}

/// `NOTE = /\[\[[^\]]*\]\]/g` applied with `replace(NOTE, '')`.
fn remove_notes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("[[") {
        let after = &rest[at + 2..];
        match after.find(']') {
            Some(close) if after[close..].starts_with("]]") => {
                out.push_str(&rest[..at]);
                rest = &after[close + 2..];
            }
            _ => {
                // No note starts here; the next attempt is one character on.
                out.push_str(&rest[..at + 1]);
                rest = &rest[at + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// `[^\S\n]`: JavaScript whitespace other than a line feed.
fn is_inline_space(c: char) -> bool {
    c != '\n' && js::is_space(c)
}

/// `stripNotes(text)`.
pub fn strip_notes(text: &str) -> String {
    if !text.contains("[[") {
        return text.to_owned();
    }
    let removed = remove_notes(text);
    // .replace(/[^\S\n]{2,}/g, ' ')
    let chars: Vec<char> = removed.chars().collect();
    let mut collapsed: Vec<char> = Vec::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        if is_inline_space(chars[i]) {
            let mut j = i;
            while j < chars.len() && is_inline_space(chars[j]) {
                j += 1;
            }
            if j - i >= 2 {
                collapsed.push(' ');
            } else {
                collapsed.push(chars[i]);
            }
            i = j;
        } else {
            collapsed.push(chars[i]);
            i += 1;
        }
    }
    // .replace(/^[^\S\n]+/gm, ''): a run at the start of the text or after
    // a LineTerminator (\n, \r, U+2028, U+2029) in the multiline sense.
    let mut lined = String::with_capacity(collapsed.len());
    let mut i = 0;
    while i < collapsed.len() {
        let at_line_start = i == 0 || js::is_line_terminator(collapsed[i - 1]);
        if at_line_start && is_inline_space(collapsed[i]) {
            while i < collapsed.len() && is_inline_space(collapsed[i]) {
                i += 1;
            }
            continue;
        }
        lined.push(collapsed[i]);
        i += 1;
    }
    js::trim(&lined).to_owned()
}

/// `str.trim().split(/\s+/)`. An empty string splits into one empty word,
/// which no lookup holds.
fn words(s: &str) -> Vec<&str> {
    let t = js::trim(s);
    if t.is_empty() {
        return vec![""];
    }
    t.split(js::is_space).filter(|w| !w.is_empty()).collect()
}

/// `LEADING_FMT = /^\s*\[\[\s*fmt:([^\]]*)\]\]/`: the inside of a leading
/// fmt note.
fn leading_fmt(text: &str) -> Option<&str> {
    let rest = text.trim_start_matches(js::is_space);
    let rest = rest.strip_prefix("[[")?;
    let rest = rest.trim_start_matches(js::is_space);
    let rest = rest.strip_prefix("fmt:")?;
    let close = rest.find(']')?;
    rest[close..].starts_with("]]").then(|| &rest[..close])
}

/// `fmtClasses(text)`.
///
/// One deliberate difference: the TypeScript looks each word up in a plain
/// object, so a word naming an `Object.prototype` member (`constructor`,
/// `toString`, `__proto__` ...) is truthy there and stringifies a function
/// into the class attribute. Only the table's own keys count here (see
/// rust/PARITY.md, Decisions).
pub fn fmt_classes(text: &str) -> String {
    let Some(inner) = leading_fmt(text) else {
        return String::new();
    };
    let mut out = String::new();
    for word in words(inner) {
        let class = match word {
            "mono" => "fmt-mono",
            "serif" => "fmt-serif",
            "sans" => "fmt-sans",
            "cursive" => "fmt-cursive",
            "-1" => "fmt-minus1",
            "+1" => "fmt-plus1",
            "+2" => "fmt-plus2",
            _ => continue,
        };
        out.push(' ');
        out.push_str(class);
    }
    out
}

/// `isSafeImageFile(file)`: `/^[A-Za-z0-9._-]{1,64}\.png$/`.
pub fn is_safe_image_file(file: &str) -> bool {
    let Some(stem) = file.strip_suffix(".png") else {
        return false;
    };
    (1..=64).contains(&stem.len())
        && stem
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// `WIDTH_WORD = /^w=(-?\d+)$/`, then `parseInt` and the 5..100 clamp.
fn width_word(word: &str) -> Option<u32> {
    let digits = word.strip_prefix("w=")?;
    let unsigned = digits.strip_prefix('-').unwrap_or(digits);
    if unsigned.is_empty() || !unsigned.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // parseInt's value can be ±Infinity for a long run of digits; the clamp
    // pins either end, as Math.min/Math.max do.
    let v: f64 = digits.parse().ok()?;
    let clamped = 100f64.min(5f64.max(v));
    // In 5..=100 and integral, so the cast is exact.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some(clamped as u32)
}

/// `parseImgNote(inner)`: `/^\s*img:\s*(\S+)(.*)$/` (no `s` flag, so the
/// tail may hold no line terminator).
pub fn parse_img_note(inner: &str) -> Option<ImageRef> {
    let rest = inner.trim_start_matches(js::is_space);
    let rest = rest.strip_prefix("img:")?;
    let rest = rest.trim_start_matches(js::is_space);
    let end = rest.find(js::is_space).unwrap_or(rest.len());
    let file = &rest[..end];
    let tail = &rest[end..];
    if file.is_empty() || tail.contains(js::is_line_terminator) {
        return None;
    }
    if !is_safe_image_file(file) {
        return None;
    }
    let mut width_pct = 100;
    let mut full_page = false;
    for word in words(tail) {
        if word == "full" {
            full_page = true;
        }
        if let Some(w) = width_word(word) {
            width_pct = w;
        }
    }
    Some(ImageRef {
        file: file.to_owned(),
        width_pct,
        full_page,
    })
}

/// `rendersImage(raw, includeImages, have)`.
pub fn renders_image(
    raw: &str,
    include_images: bool,
    have: &dyn Fn(&str) -> bool,
) -> Option<ImageRef> {
    if !include_images {
        return None;
    }
    let img = parse_img_note(raw)?;
    have(&img.file).then_some(img)
}

/// `PRIMARY_SLUG = /^(?:\*{0,3}_?)?(?:(?:int|i)\.?\/(?:ext|e)|int|ext|est)[. ]/i`.
pub fn is_primary_slug(text: &str) -> bool {
    let b = text.as_bytes();
    let mut i = 0;
    while i < b.len() && i < 3 && b[i] == b'*' {
        i += 1;
    }
    if b.get(i) == Some(&b'_') {
        i += 1;
    }
    let at = &b[i..];
    let word =
        |s: &[u8], w: &str| s.len() >= w.len() && s[..w.len()].eq_ignore_ascii_case(w.as_bytes());
    let ends = |s: &[u8]| matches!(s.first(), Some(b'.' | b' '));
    for first in ["int", "i"] {
        if !word(at, first) {
            continue;
        }
        let mut s = &at[first.len()..];
        if s.first() == Some(&b'.') {
            // `\.?` is greedy, but taking it or not cannot both lead to `/`.
            s = &s[1..];
        }
        if s.first() != Some(&b'/') {
            continue;
        }
        let s = &s[1..];
        for second in ["ext", "e"] {
            if word(s, second) && ends(&s[second.len()..]) {
                return true;
            }
        }
    }
    ["int", "ext", "est"]
        .iter()
        .any(|w| word(at, w) && ends(&at[w.len()..]))
}

/// `isMiniSlug(t)`.
pub fn is_mini_slug(t: &Token) -> bool {
    t.kind == TokenType::SceneHeading && !is_primary_slug(t.text_or_empty())
}

/// `Cover`: the picture a book wears and the note that placed it.
pub struct Cover<'a> {
    pub file: String,
    pub token: &'a Token,
}

/// `pickCover(tokens, format, have)`.
pub fn pick_cover<'a>(
    tokens: &[&'a Token],
    format: &FormatOptions,
    have: &dyn Fn(&str) -> bool,
) -> Option<Cover<'a>> {
    if !format.promote_cover_image || !format.include_images {
        return None;
    }
    for &t in tokens {
        if t.is_title() {
            continue;
        }
        if t.kind != TokenType::Note {
            return None;
        }
        let Some(img) = renders_image(t.text_or_empty(), format.include_images, have) else {
            continue;
        };
        return img.full_page.then_some(Cover {
            file: img.file,
            token: t,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_strip_and_tidy() {
        assert_eq!(
            strip_notes("He leaves [[check this]] quickly."),
            "He leaves quickly."
        );
        assert_eq!(strip_notes("no notes  here "), "no notes  here ");
        assert_eq!(strip_notes("[[fmt: mono]] one\ntwo"), "one\ntwo");
        assert_eq!(strip_notes("a [[x\n"), "a [[x");
        assert_eq!(strip_notes("[[[a]] b"), "b");
    }

    #[test]
    fn fmt_words() {
        assert_eq!(
            fmt_classes("[[fmt: sans +2]]\nA SIGN."),
            " fmt-sans fmt-plus2"
        );
        assert_eq!(fmt_classes("  [[ fmt:]]x"), "");
        assert_eq!(fmt_classes("x [[fmt: sans]]"), "");
    }

    #[test]
    fn img_notes() {
        let r = parse_img_note("img: p001-01.png w=100 full").unwrap();
        assert_eq!(
            (r.file.as_str(), r.width_pct, r.full_page),
            ("p001-01.png", 100, true)
        );
        assert_eq!(
            parse_img_note("img: p001-01.png w=-9").unwrap().width_pct,
            5
        );
        assert_eq!(
            parse_img_note(&format!("img: a.png w={}", "9".repeat(400)))
                .unwrap()
                .width_pct,
            100
        );
        assert!(parse_img_note("img: ../a.png").is_none());
        assert!(parse_img_note("img: a.png\nfull").is_none());
        assert!(parse_img_note("Img: a.png").is_none());
    }

    #[test]
    fn primary_slugs() {
        for (t, yes) in [
            ("INT. HOUSE", true),
            ("int house", true),
            ("I/E. CAR", true),
            ("I/E CAR", true),
            ("INT./EXT. CAR", true),
            ("INT/EXT CAR", true),
            ("**_EXT. X", true),
            ("****INT. X", false),
            ("EST. X", true),
            ("ESTABLISHING", false),
            ("INTO", false),
            ("LATER", false),
        ] {
            assert_eq!(is_primary_slug(t), yes, "{t}");
        }
    }
}
