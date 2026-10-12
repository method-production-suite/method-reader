//! INTERIM COPY of the stage-2 discriminators the MOBI renderer imports:
//! src/fountain/notes.ts (note stripping, `[[fmt: ...]]` sizes, picture
//! notes), src/fountain/slug.ts (`isMiniSlug`) and src/fountain/cover.ts
//! (`pickCover`).
//!
//! The TypeScript keeps ONE copy of each, shared by every renderer (registry
//! #5b and #18), because two copies drift. This crate was ported while
//! sp-fountain was being ported beside it and could depend only on sp-model,
//! so it carries a private copy, line for line, with the TypeScript's own
//! names. When sp-fountain lands, delete this module and import its
//! functions instead (recorded in rust/PARITY.md's Decisions). Nothing here
//! is public API.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;
use sp_model::{FormatOptions, ImageRef, SizeStep, Token, TokenType};

use crate::js::{self, JS_SPACE, is_js_space, is_line_terminator};

/// Any note: `/\[\[[^\]]*\]\]/g`.
static NOTE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[\[[^\]]*\]\]").expect("a valid pattern"));

/// `/[^\S\n]{2,}/g`: two or more whitespace characters, newlines excepted.
static INNER_RUN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"[{}]{{2,}}", JS_SPACE.replace(r"\n", ""))).expect("a valid pattern")
});

/// A LEADING fmt note only: `/^\s*\[\[\s*fmt:([^\]]*)\]\]/`.
static LEADING_FMT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(r"^[{JS_SPACE}]*\[\[[{JS_SPACE}]*fmt:([^\]]*)\]\]"))
        .expect("a valid pattern")
});

/// `stripNotes`: remove every note, then tidy the whitespace the removal
/// left behind. Newlines survive. Text with no `[[` is returned unchanged.
pub(crate) fn strip_notes(text: &str) -> String {
    if !text.contains("[[") {
        return text.to_owned();
    }
    let removed = NOTE.replace_all(text, "");
    let collapsed = INNER_RUN.replace_all(&removed, " ");
    // `/^[^\S\n]+/gm`: at each line start (JavaScript's multiline `^` follows
    // any line terminator), the run of whitespace other than `\n`.
    let s: &str = &collapsed;
    let mut out = String::with_capacity(s.len());
    let mut prev: Option<char> = None;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        let line_start = prev.is_none_or(is_line_terminator);
        if line_start && c != '\n' && is_js_space(c) {
            let mut last = c;
            while let Some(&d) = chars.peek() {
                if d == '\n' || !is_js_space(d) {
                    break;
                }
                last = d;
                chars.next();
            }
            prev = Some(last);
            continue;
        }
        out.push(c);
        prev = Some(c);
    }
    js::trim(&out).to_owned()
}

/// `fmtSizeStep`: the size step of a leading fmt note.
pub(crate) fn fmt_size_step(text: &str) -> Option<SizeStep> {
    let m = LEADING_FMT.captures(text)?;
    for word in js::split_ws(js::trim(&m[1])) {
        match word {
            "-1" => return Some(SizeStep::Smaller),
            "+1" => return Some(SizeStep::Larger),
            "+2" => return Some(SizeStep::MuchLarger),
            _ => {}
        }
    }
    None
}

/// `SAFE_IMAGE_FILE`: the security boundary on a picture's name.
static SAFE_IMAGE_FILE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9._-]{1,64}\.png$").expect("a valid pattern"));

/// `isSafeImageFile`.
pub(crate) fn is_safe_image_file(file: &str) -> bool {
    SAFE_IMAGE_FILE.is_match(file)
}

/// `/^\s*img:\s*(\S+)(.*)$/`, with JavaScript's `.` (no line terminator).
static IMG_NOTE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"^[{JS_SPACE}]*img:[{JS_SPACE}]*([^{JS_SPACE}]+)([^\n\r\x{{2028}}\x{{2029}}]*)$"
    ))
    .expect("a valid pattern")
});

/// `/^w=(-?\d+)$/`.
static WIDTH_WORD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^w=(-?[0-9]+)$").expect("a valid pattern"));

/// `parseImgNote`: the INNER text of a standalone image note.
pub(crate) fn parse_img_note(inner: &str) -> Option<ImageRef> {
    let m = IMG_NOTE.captures(inner)?;
    let file = m[1].to_owned();
    if !is_safe_image_file(&file) {
        return None;
    }
    let mut width_pct = 100.0;
    let mut full_page = false;
    for word in js::split_ws(js::trim(&m[2])) {
        if word == "full" {
            full_page = true;
        }
        if let Some(w) = WIDTH_WORD.captures(word) {
            // parseInt never fails on `-?\d+`, but runs to ±Infinity, which
            // the clamp pins exactly as it pins any large value.
            let digits = &w[1];
            let v = digits.parse::<f64>().unwrap_or(0.0);
            width_pct = v.clamp(5.0, 100.0);
        }
    }
    Some(ImageRef {
        file,
        width_pct,
        full_page,
    })
}

/// `rendersImage`: whether an `[[img: ...]]` note becomes a figure.
pub(crate) fn renders_image(
    raw: &str,
    include_images: bool,
    have: &HashSet<String>,
) -> Option<ImageRef> {
    if !include_images {
        return None;
    }
    let img = parse_img_note(raw)?;
    have.contains(&img.file).then_some(img)
}

/// `PRIMARY_SLUG`: the openers fountain-js accepts for an UNFORCED scene
/// heading. ASCII case-insensitive, as JavaScript's `/i` without `/u` is.
static PRIMARY_SLUG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:\*{0,3}_?)?(?i-u:(?:(?:int|i)\.?/(?:ext|e)|int|ext|est))[. ]")
        .expect("a valid pattern")
});

/// `isMiniSlug`.
pub(crate) fn is_mini_slug(t: &Token) -> bool {
    t.kind == TokenType::SceneHeading && !PRIMARY_SLUG.is_match(t.text_or_empty())
}

/// `pickCover`: the designed title card, if this script opens with one.
/// Answers the picture's filename.
pub(crate) fn pick_cover(
    tokens: &[Token],
    format: &FormatOptions,
    have: &HashSet<String>,
) -> Option<String> {
    if !format.promote_cover_image || !format.include_images {
        return None;
    }
    for t in tokens {
        if t.is_title() {
            continue;
        }
        if t.kind != TokenType::Note {
            return None;
        }
        let Some(img) = renders_image(t.text_or_empty(), format.include_images, have) else {
            continue;
        };
        return img.full_page.then_some(img.file);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_notes_tidies_like_the_typescript() {
        assert_eq!(
            strip_notes("He leaves [[check this]] quickly."),
            "He leaves quickly."
        );
        assert_eq!(strip_notes("no notes  here "), "no notes  here ");
        assert_eq!(strip_notes("[[a]] one\n  [[b]]two"), "one\ntwo");
        assert_eq!(strip_notes("[[fmt: +2]] Read it back."), "Read it back.");
    }

    #[test]
    fn size_steps() {
        assert_eq!(
            fmt_size_step("[[fmt: sans +2]]\nA"),
            Some(SizeStep::MuchLarger)
        );
        assert_eq!(fmt_size_step(" [[ fmt: -1]]x"), Some(SizeStep::Smaller));
        assert_eq!(fmt_size_step("x [[fmt: +1]]"), None);
        assert_eq!(fmt_size_step("[[fmt: sans]]"), None);
    }

    #[test]
    fn image_notes() {
        let img = parse_img_note("img: p001-01.png w=42").unwrap();
        assert_eq!(
            (img.file.as_str(), img.width_pct, img.full_page),
            ("p001-01.png", 42.0, false)
        );
        let img = parse_img_note("img: p001-01.png w=100 full").unwrap();
        assert!(img.full_page);
        assert_eq!(
            parse_img_note("img: p001-01.png w=-5").unwrap().width_pct,
            5.0
        );
        assert_eq!(
            parse_img_note(&format!("img: a.png w={}", "9".repeat(400)))
                .unwrap()
                .width_pct,
            100.0
        );
        assert!(parse_img_note("img: a/b.png").is_none());
        assert!(parse_img_note("IMG: a.png").is_none());
        assert!(parse_img_note("img: a.png\nmore").is_none());
    }

    #[test]
    fn mini_slugs() {
        let h = |s: &str| Token::with_text(TokenType::SceneHeading, s);
        assert!(!is_mini_slug(&h("INT. KITCHEN - DAY")));
        assert!(!is_mini_slug(&h("int/ext yard")));
        assert!(!is_mini_slug(&h("I/E CAR")));
        assert!(is_mini_slug(&h("LATER")));
        assert!(is_mini_slug(&h("BLACK")));
        // ASCII-only folding: a long s is not an s, as in JavaScript.
        assert!(is_mini_slug(&h("e\u{17f}t. ROOF")));
        assert!(!is_mini_slug(&Token::with_text(TokenType::Action, "LATER")));
    }
}
