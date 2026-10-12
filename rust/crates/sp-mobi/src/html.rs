//! fountain-js tokens to MOBI 6 HTML. The old MOBI renderer speaks a small
//! HTML-3.2-ish dialect: no CSS, indents via `<blockquote>`, alignment via
//! attributes. Screenplay mapping keeps structure readable on e-ink: bold
//! sluglines, blockquoted speeches with bold cues, right-flush transitions.
//! (The EPUB path stays the high-fidelity rendering; this is the
//! dependency-free USB sideload format.)
//!
//! This dialect has exactly one fragmentation primitive: `<mbp:pagebreak/>`.
//! It is wired to scenePageBreaks (registry #1), before PRIMARY scene
//! headings only: a mini-slug is a micro-heading inside the current scene
//! (registry #5b). The other knob it reads is includeTitlePage (#11). It
//! ships no stylesheet (#8c), so every keep, margin and alignment knob is
//! EPUB territory. It reads preserveFontShifts (#18), but only the SIZE arm,
//! includeImages (#19a) through [`mobi_art`], and promoteCoverImage (#19b)
//! only through `mobi_art`'s cover index, which the writer turns into a
//! header record.
//!
//! Ports src/mobi/html.ts.

use std::collections::HashSet;
use std::sync::LazyLock;

use sp_fountain::jsre::{Regex, group};
use sp_fountain::{fmt_size_step, is_mini_slug, js, pick_cover, renders_image, strip_notes};
use sp_model::{FormatOptions, SizeStep, Token, TokenType};

/// The title and author the title page shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MobiMeta {
    pub title: String,
    pub author: Option<String>,
}

/// The pictures one MOBI carries, in the order its image records hold them.
///
/// A MOBI addresses a picture by RECORD, not by name: `recindex="00002"` is
/// the second image record, and nothing in the file says which picture that
/// was. So the numbering exists once, in this list: [`tokens_to_mobi_html`]
/// numbers its figures from it and the caller hands
/// [`crate::writer::build_mobi`] the bytes in this same order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MobiArt {
    /// `order[i]` is the picture `recindex` i+1 names.
    pub order: Vec<String>,
    /// Which of `order` is the book's cover (registry #19b), when the cover
    /// picker chose one. The card stays in `order` and on its page.
    pub cover_index: Option<usize>,
}

/// THE SAFETY GATE on pictures in the engine's own MOBI (registry #19a,
/// #19b).
///
/// The .mobi is the file for the oldest Kindles, and no device has yet
/// opened one carrying picture records. Older firmware is reported to cap a
/// record near 127 KB, and a failure there could leave the whole book
/// unopenable. So until a device probe passes, every route builds the .mobi
/// text-only: no image records, no EXTH 201 cover, the book from before
/// pictures, byte for byte. The EPUB, and the AZW3 and KFX made from it,
/// keep both.
///
/// The probe is [`crate::probe`] (today `bun tools/mobi-picture-probe.ts`).
/// When it passes, this is the one line to change. A caller's own override
/// for one build (the probe itself, and the tests that keep the records
/// proven while the gate is shut) passes the picture set to [`mobi_art`]
/// regardless of this constant.
pub const MOBI_CARRIES_PICTURES: bool = false;

/// Which pictures this token stream puts in a MOBI, in document order, each
/// once. Decided by the EPUB's own predicate, so the EPUB and the MOBI of
/// one script show the same pictures. `have` is what the caller can supply
/// bytes for. The cover is the EPUB's own cover pick for the same tokens.
pub fn mobi_art(tokens: &[Token], format: &FormatOptions, have: &HashSet<String>) -> MobiArt {
    let mut order: Vec<String> = Vec::new();
    for t in tokens {
        if t.is_title() || t.kind != TokenType::Note {
            continue;
        }
        if let Some(img) = renders_image(t.text_or_empty(), format.include_images, have)
            && !order.contains(&img.file)
        {
            order.push(img.file);
        }
    }
    let cover_index =
        pick_cover(tokens, format, have).and_then(|c| order.iter().position(|f| *f == c.file));
    MobiArt { order, cover_index }
}

/// The record number a figure's `recindex` names: 1-based, five digits.
fn recindex(i: usize) -> String {
    format!("{:05}", i + 1)
}

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// One literal JavaScript pattern, compiled once.
macro_rules! js_regex {
    ($name:ident, $pattern:expr, $flags:expr) => {
        static $name: LazyLock<Regex> = LazyLock::new(|| Regex::new($pattern, $flags));
    };
}

js_regex!(BOLD_ITALIC, r"\*\*\*(?!\s)([^*]+?)(?<!\s)\*\*\*", "g");
js_regex!(BOLD, r"\*\*(?!\s)([^*]+?)(?<!\s)\*\*", "g");
js_regex!(ITALIC, r"\*(?!\s)([^*]+?)(?<!\s)\*", "g");
js_regex!(UNDERLINE, r"_(?!\s)([^_]+?)(?<!\s)_", "g");
js_regex!(
    BLOCK_OPEN,
    r"^<(p|blockquote|center|table|div|h[1-6])\b[^>]*>",
    ""
);
js_regex!(PAGE_MARKER, r"^pg\s+(\S+)$", "");
js_regex!(TRANSITION_PREFIX, r"^>\s*", "");

/// `s.replace(re, open + '$1' + close)`, globally.
fn wrap(re: &Regex, s: &str, open: &str, close: &str) -> String {
    re.replace_with(s, true, |chars, caps| {
        format!("{open}{}{close}", group(chars, caps, 1).unwrap_or_default())
    })
}

/// Fountain inline emphasis to old-MOBI tags, applied AFTER escaping.
fn inline(escaped: &str) -> String {
    let s = wrap(&BOLD_ITALIC, escaped, "<b><i>", "</i></b>");
    let s = wrap(&BOLD, &s, "<b>", "</b>");
    let s = wrap(&ITALIC, &s, "<i>", "</i>");
    wrap(&UNDERLINE, &s, "<u>", "</u>")
}

fn size_word(step: SizeStep) -> &'static str {
    match step {
        SizeStep::Smaller => "-1",
        SizeStep::Larger => "+1",
        SizeStep::MuchLarger => "+2",
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
}

struct DualBlock {
    left: Vec<String>,
    right: Vec<String>,
    side: Side,
}

/// The renderer's running state: the output so far and the break rules.
struct Out {
    out: Vec<String>,
    /// Body content emitted since the last page break: a break is only
    /// wanted where there is something to break AWAY from.
    since_break: bool,
    /// A break owed AFTER a full-page plate, paid only by the next block.
    break_owed: bool,
    /// A page marker waiting to ride inside the next block.
    pending_marker: Option<String>,
}

impl Out {
    fn break_page(&mut self) {
        self.out.push("<mbp:pagebreak/>".to_owned());
        self.since_break = false;
        self.break_owed = false;
    }

    /// A block, with no claim on the pending page marker, paying first any
    /// break a full-page plate left owing.
    fn place(&mut self, s: String) {
        if self.break_owed {
            self.break_page();
        }
        self.out.push(s);
        self.since_break = true;
    }

    /// A block that takes the pending page marker, if it can hold one.
    fn push(&mut self, mut s: String) {
        if let Some(marker) = &self.pending_marker
            && let Some(open) = BLOCK_OPEN.captures(&s)
        {
            let at = open[0].as_deref().map_or(0, str::len);
            s.insert_str(at, marker);
            self.pending_marker = None;
        }
        self.place(s);
    }

    fn close_speech(&mut self, speech: &mut Option<Vec<String>>) {
        if let Some(lines) = speech.take() {
            self.push(format!("<blockquote>{}</blockquote>", lines.join("<br/>")));
        }
    }
}

/// The MOBI 6 HTML for `tokens`. `art` is what [`mobi_art`] decided for
/// these tokens and this format; [`MobiArt::default`] is no pictures, the
/// honest answer for a caller with no records to address.
pub fn tokens_to_mobi_html(
    tokens: &[Token],
    meta: &MobiMeta,
    format: &FormatOptions,
    art: &MobiArt,
) -> String {
    let mut o = Out {
        out: vec!["<html><head></head><body>".to_owned()],
        since_break: false,
        break_owed: false,
        pending_marker: None,
    };

    // Title page (registry #11's MOBI arm).
    if format.include_title_page {
        o.out.push("<center>".to_owned());
        o.out.push(format!(
            "<br/><br/><br/><b><font size=\"+2\">{}</font></b>",
            esc(&meta.title)
        ));
        if let Some(author) = meta.author.as_deref().filter(|a| !a.is_empty()) {
            o.out
                .push(format!("<br/><br/>Written by<br/>{}", esc(author)));
        }
        o.out.push("</center>".to_owned());
        o.break_page();
    }

    let mut speech: Option<Vec<String>> = None;
    let mut dual: Option<DualBlock> = None;
    // The pictures this book has records for.
    let in_book: HashSet<String> = art.order.iter().cloned().collect();

    for t in tokens.iter().filter(|t| !t.is_title()) {
        let raw = t.text_or_empty();
        // Read the shift before stripping: notes are invisible by spec.
        let step = if format.preserve_font_shifts {
            fmt_size_step(raw)
        } else {
            None
        };
        let sized = |s: String| match step {
            Some(step) => format!("<font size=\"{}\">{s}</font>", size_word(step)),
            None => s,
        };
        let text = strip_notes(raw);
        match t.kind {
            TokenType::SceneHeading => {
                o.close_speech(&mut speech);
                // The only break primitive this dialect has, and only before
                // a PRIMARY scene heading.
                if format.scene_page_breaks && !is_mini_slug(t) && o.since_break {
                    o.break_page();
                }
                o.push(format!("<p><b>{}</b></p>", esc(&text)));
            }
            TokenType::Action | TokenType::Lyrics => {
                o.close_speech(&mut speech);
                o.push(format!("<p>{}</p>", sized(inline(&esc(&text)))));
            }
            TokenType::DualDialogueBegin => {
                dual = Some(DualBlock {
                    left: Vec::new(),
                    right: Vec::new(),
                    side: Side::Left,
                });
            }
            TokenType::DualDialogueEnd => {
                if let Some(d) = dual.take() {
                    o.push(format!(
                        "<table width=\"100%\"><tr><td width=\"50%\">{}</td><td width=\"50%\">{}</td></tr></table>",
                        d.left.join("<br/>"),
                        d.right.join("<br/>")
                    ));
                }
            }
            TokenType::DialogueBegin => {
                if let Some(d) = dual.as_mut() {
                    d.side = if t.dual == Some(sp_model::Dual::Right) {
                        Side::Right
                    } else {
                        Side::Left
                    };
                }
                speech = Some(Vec::new());
            }
            TokenType::Character => {
                if let Some(s) = speech.as_mut() {
                    s.push(format!("<b>{}</b>", esc(&text)));
                }
            }
            TokenType::Parenthetical => {
                if let Some(s) = speech.as_mut() {
                    s.push(format!("<i>{}</i>", esc(&text)));
                }
            }
            TokenType::Dialogue => {
                if let Some(s) = speech.as_mut() {
                    s.push(sized(inline(&esc(&text)).replace('\n', "<br/>")));
                }
            }
            TokenType::DialogueEnd => {
                if let (Some(d), Some(_)) = (dual.as_mut(), speech.as_ref()) {
                    let lines = speech.take().unwrap_or_default();
                    match d.side {
                        Side::Left => d.left.extend(lines),
                        Side::Right => d.right.extend(lines),
                    }
                } else {
                    o.close_speech(&mut speech);
                }
            }
            TokenType::Transition => {
                o.close_speech(&mut speech);
                o.push(format!(
                    "<p align=\"right\">{}</p>",
                    esc(&TRANSITION_PREFIX.replace_with(&text, false, |_, _| String::new()))
                ));
            }
            TokenType::Centered => {
                o.close_speech(&mut speech);
                o.push(format!("<center>{}</center>", esc(&text)));
            }
            TokenType::Synopsis => {
                // Emits no block of its own: it waits for the next one.
                if let Some(m) = PAGE_MARKER.captures(js::trim(&text)) {
                    let page = m[1].clone().unwrap_or_default();
                    o.pending_marker = Some(format!(
                        "<span class=\"page-marker\"><font size=\"-2\">{}.</font></span>",
                        esc(&page)
                    ));
                }
            }
            TokenType::Note => {
                // A standalone `[[img: ...]]` is a picture carried out of the
                // PDF (registry #19); every other note renders nothing. A
                // title card that is the book's cover is drawn here too,
                // where the EPUB leaves it off its pages (#19b).
                let Some(img) = renders_image(raw, format.include_images, &in_book) else {
                    continue;
                };
                o.close_speech(&mut speech);
                // The EPUB's p.figure-full in this dialect's one break
                // primitive: a break before the plate and one after, neither
                // taken where there is nothing to break from or away from.
                if img.full_page && o.since_break {
                    o.break_page();
                }
                // `place`, not `push`: a figure declines a pending page
                // marker. No width: a share of the column has no spelling
                // in this dialect.
                let n = art
                    .order
                    .iter()
                    .position(|f| *f == img.file)
                    .expect("every picture in the book is in its order");
                o.place(format!(
                    "<p align=\"center\"><img recindex=\"{}\" alt=\"Illustration\"/></p>",
                    recindex(n)
                ));
                if img.full_page {
                    o.break_owed = true;
                }
            }
            _ => {}
        }
    }
    o.close_speech(&mut speech);

    o.out.push("</body></html>".to_owned());
    o.out.join("\n")
}
