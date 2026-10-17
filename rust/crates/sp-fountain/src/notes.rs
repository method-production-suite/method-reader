//! Port of `src/fountain/notes.ts`: Fountain notes, shared by every
//! renderer (registry #18, #19).
//!
//! The format says `[[...]]` is invisible in output. Method Reader writes two
//! kinds of its own, `[[fmt: ...]]` font shifts and `[[img: ...]]` pictures,
//! so stripping is load-bearing and lives here as the one copy every
//! renderer uses, rather than one regex per renderer that drift apart.

use std::collections::{BTreeSet, HashSet};
use std::sync::LazyLock;

use sp_model::{ImageRef, SizeStep, TokenType};

use crate::js;
use crate::jsre::{Regex, group};
use crate::tokenizer::{FountainError, tokenize};

struct Patterns {
    note: Regex,
    leading_fmt: Regex,
    spaces_run: Regex,
    line_indent: Regex,
    own_note_line: Regex,
    leading_fmt_prefix: Regex,
    blank: Regex,
    whitespace_run: Regex,
    safe_image_file: Regex,
    width_word: Regex,
    img_note: Regex,
}

static P: LazyLock<Patterns> = LazyLock::new(|| Patterns {
    // Bracket-free inside, so an unterminated `[[` matches nothing and is
    // left in the text rather than eating the rest of the line.
    note: Regex::new(r"\[\[[^\]]*\]\]", "g"),
    // A LEADING fmt note only: a note further in is about the text, not a
    // declaration about the block.
    leading_fmt: Regex::new(r"^\s*\[\[\s*fmt:([^\]]*)\]\]", ""),
    spaces_run: Regex::new(r"[^\S\n]{2,}", "g"),
    line_indent: Regex::new(r"^[^\S\n]+", "gm"),
    // Case-sensitive keywords: we write exactly `fmt:` and `img:`, and a
    // note we did not write is the writer's.
    own_note_line: Regex::new(r"^\s*\[\[\s*(?:fmt|img):[^\]]*\]\]\s*$", ""),
    leading_fmt_prefix: Regex::new(r"^(\s*)\[\[\s*fmt:[^\]]*\]\] ?", ""),
    blank: Regex::new(r"^\s*$", ""),
    whitespace_run: Regex::new(r"\s+", ""),
    // SECURITY BOUNDARY: see `is_safe_image_file`.
    safe_image_file: Regex::new(r"^[A-Za-z0-9._-]{1,64}\.png$", ""),
    width_word: Regex::new(r"^w=(-?\d+)$", ""),
    img_note: Regex::new(r"^\s*img:\s*(\S+)(.*)$", ""),
});

/// `s.split(/\s+/)`.
fn split_words(s: &str) -> Vec<String> {
    P.whitespace_run.split(s, None)
}

/// `stripNotes`: remove every note, then tidy the whitespace the removal
/// left behind. Newlines survive (a lyrics or verse token carries its own
/// line breaks); text with no `[[` is returned identical.
pub fn strip_notes(text: &str) -> String {
    if !text.contains("[[") {
        return text.to_owned();
    }
    let s = P.note.replace_all(text, "");
    let s = P.spaces_run.replace_all(&s, " ");
    let s = P.line_indent.replace_all(&s, "");
    js::trim(&s).to_owned()
}

/// `stripScreepubNotes`: the script for a screenwriting app, Method Reader's own
/// `[[fmt: ...]]` and `[[img: ...]]` notes taken out and every other line
/// kept byte for byte. Only the shapes serialize writes are touched: a note
/// alone on its line, and an fmt note leading a dialogue line. A note that
/// was a paragraph of its own takes one blank line with it.
///
/// For an EXPORTED copy only: the cached `.fountain` keeps every note.
pub fn strip_screepub_notes(fountain_text: &str) -> String {
    if !fountain_text.contains("[[") {
        return fountain_text.to_owned();
    }
    let lines: Vec<&str> = fountain_text.split('\n').collect();
    let is_blank = |line: &str| P.blank.test(line);
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let mut own = P.own_note_line.test(line);
        let mut kept = line.to_owned();
        if !own {
            let chars: Vec<char> = line.chars().collect();
            if let Some(m) = P.leading_fmt_prefix.find_from(&chars, 0) {
                let (_, end) = m[0].unwrap();
                kept = group(&chars, &m, 1).unwrap_or_default();
                kept.extend(&chars[end..]);
                // A dialogue line that was only the note would leave a blank
                // line, which ends the speech: it goes whole instead.
                own = is_blank(&kept);
            }
        }
        if !own {
            out.push(kept);
            i += 1;
            continue;
        }
        let prev_blank = out.last().is_none_or(|l| is_blank(l));
        if prev_blank && i + 1 < lines.len() && is_blank(lines[i + 1]) {
            i += 1;
        }
        i += 1;
    }
    out.join("\n")
}

/// The words of a leading `[[fmt: ...]]` note, or `None` without one.
fn leading_fmt_words(text: &str) -> Option<Vec<String>> {
    let m = P.leading_fmt.captures(text)?;
    Some(split_words(js::trim(m[1].as_deref().unwrap_or(""))))
}

/// `fmtClasses`: a leading `[[fmt: ...]]` as a class suffix, e.g.
/// `" fmt-sans fmt-plus2"`. Unknown words are ignored one by one.
pub fn fmt_classes(text: &str) -> String {
    let Some(words) = leading_fmt_words(text) else {
        return String::new();
    };
    let mut out = String::new();
    for word in words {
        let class = match word.as_str() {
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

/// `fmtSizeStep`: the size step of a leading fmt note, for renderers with
/// no stylesheet.
pub fn fmt_size_step(text: &str) -> Option<SizeStep> {
    leading_fmt_words(text)?
        .iter()
        .find_map(|word| match word.as_str() {
            "-1" => Some(SizeStep::Smaller),
            "+1" => Some(SizeStep::Larger),
            "+2" => Some(SizeStep::MuchLarger),
            _ => None,
        })
}

/// `isSafeImageFile`: a filename we are willing to put inside a zip.
///
/// SECURITY BOUNDARY, not tidiness. The `.fountain` is a text a user can
/// hand-edit or receive from someone else, and this string becomes an entry
/// path in the EPUB and a filename read off disk. The allowlist excludes
/// both separators, absolute paths, `%`-escapes and anything that is not a
/// lowercase `.png`; a name that fails is dropped, never repaired. With no
/// separator in the class, `..png` is a filename, not a path. The 64 counts
/// the STEM. The whole name must match on one line.
pub fn is_safe_image_file(file: &str) -> bool {
    P.safe_image_file.test(file)
}

/// `parseImgNote`: the INNER text of a standalone image note (fountain-js
/// hands a `note` token its contents with the brackets stripped).
/// Case-sensitive keyword. Width 100 when absent or unreadable, clamped to
/// 5..100; `full` marks a full-page picture; other words are ignored and the
/// last width wins.
pub fn parse_img_note(inner: &str) -> Option<ImageRef> {
    let m = P.img_note.captures(inner)?;
    let file = m[1].clone()?;
    if !is_safe_image_file(&file) {
        return None;
    }
    let mut width_pct = 100.0;
    let mut full_page = false;
    let rest = m[2].clone().unwrap_or_default();
    for word in split_words(js::trim(&rest)) {
        if word == "full" {
            full_page = true;
        }
        if let Some(w) = P.width_word.captures(&word) {
            // `parseInt` on `-?\d+` never fails but can overflow to
            // ±Infinity; the clamp pins both, as it does in the TypeScript.
            let v: f64 = w[1].as_deref().unwrap().parse().unwrap_or(f64::NAN);
            width_pct = 100f64.min(5f64.max(v));
        }
    }
    Some(ImageRef {
        file,
        width_pct,
        full_page,
    })
}

/// The pictures a caller holds bytes for, by filename (`have` in the
/// TypeScript, a `ReadonlySet<string>`).
pub trait ImageSet {
    fn has(&self, file: &str) -> bool;
}

impl ImageSet for HashSet<String> {
    fn has(&self, file: &str) -> bool {
        self.contains(file)
    }
}

impl ImageSet for BTreeSet<String> {
    fn has(&self, file: &str) -> bool {
        self.contains(file)
    }
}

impl ImageSet for [&str] {
    fn has(&self, file: &str) -> bool {
        self.contains(&file)
    }
}

impl ImageSet for [String] {
    fn has(&self, file: &str) -> bool {
        self.iter().any(|f| f == file)
    }
}

/// `rendersImage`: whether an `[[img: ...]]` note becomes a figure. The
/// option is on, the note parses (the allowlist is inside
/// [`parse_img_note`]), and the picture is one the caller can ship. The ONE
/// place this is decided, so every renderer and the cover agree.
pub fn renders_image<S: ImageSet + ?Sized>(
    raw: &str,
    include_images: bool,
    have: &S,
) -> Option<ImageRef> {
    if !include_images {
        return None;
    }
    let img = parse_img_note(raw)?;
    have.has(&img.file).then_some(img)
}

/// `imageNoteFiles`: every picture a whole Fountain text's notes ask for, in
/// first-asked order, once each. Read the way the renderers read it
/// (tokenized, `note` tokens through [`parse_img_note`]), so a note no render
/// would turn into a figure asks for nothing. Errs where fountain-js throws.
pub fn image_note_files(fountain_text: &str) -> Result<Vec<String>, FountainError> {
    let mut files: Vec<String> = Vec::new();
    for t in tokenize(fountain_text)? {
        if t.kind != TokenType::Note {
            continue;
        }
        if let Some(img) = parse_img_note(t.text_or_empty())
            && !files.contains(&img.file)
        {
            files.push(img.file);
        }
    }
    Ok(files)
}
