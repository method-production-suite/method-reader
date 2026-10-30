//! Port of src/epub/html.ts: fountain-js tokens to XHTML body files. Scenes
//! flow continuously as anchored `<section>`s inside one file (a spine
//! boundary forces a page break in every reader), so files split only when a
//! size budget is exceeded.
//!
//! The TypeScript's comments carry the reasons for each shape (the cue chain
//! with no wrapper, #8b; the figure as a `<p>`, #19; the marker riding the
//! next block, #13a); they are summarized here where the code needs them.

use std::collections::HashSet;
use std::sync::LazyLock;

use sp_fountain::js::trim;
use sp_fountain::jsre::{self, Regex};
use sp_fountain::{fmt_classes, is_mini_slug, pick_cover, renders_image, strip_notes};
use sp_model::{Dual, FormatOptions, ImageRef, Token, TokenType};

use crate::css::screenplay_css;
use crate::js;

/// The TypeScript's regexes, compiled once by sp-fountain's JavaScript-
/// semantics engine so they match exactly as they match there.
struct Patterns {
    /// `/\*\*\*(?!\s)([^*]+?)(?<!\s)\*\*\*/g` and its three siblings, in
    /// the order inlineEmphasis applies them, with their replacements.
    emphasis: [(Regex, &'static str, &'static str); 4],
    opening_tag: Regex,
    tag: Regex,
    transition: Regex,
    page_marker: Regex,
    stylesheet_link: Regex,
}

static P: LazyLock<Patterns> = LazyLock::new(|| Patterns {
    emphasis: [
        (
            Regex::new(r"\*\*\*(?!\s)([^*]+?)(?<!\s)\*\*\*", "g"),
            "<strong><em>",
            "</em></strong>",
        ),
        (
            Regex::new(r"\*\*(?!\s)([^*]+?)(?<!\s)\*\*", "g"),
            "<strong>",
            "</strong>",
        ),
        (
            Regex::new(r"\*(?!\s)([^*]+?)(?<!\s)\*", "g"),
            "<em>",
            "</em>",
        ),
        (
            Regex::new(r"_(?!\s)([^_]+?)(?<!\s)_", "g"),
            "<span class=\"underline\">",
            "</span>",
        ),
    ],
    opening_tag: Regex::new(r"^<(p|h[1-6]|div)\b[^>]*>", ""),
    tag: Regex::new(r"<[^>]*>", "g"),
    transition: Regex::new(r"^>\s*", ""),
    page_marker: Regex::new(r"^pg\s+(\S+)$", ""),
    stylesheet_link: Regex::new(r#"<link rel="stylesheet"[^>]*\/>"#, ""),
});

/// One XHTML body file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyFile {
    /// Filename-safe id, e.g. `body001`.
    pub id: String,
    /// The complete XHTML document.
    pub xhtml: String,
}

/// One table-of-contents entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocEntry {
    pub title: String,
    /// Relative to OEBPS/, e.g. `text/body001.xhtml#sc-001`.
    pub href: String,
}

/// One page marker, for the EPUB3 page-list nav.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageEntry {
    pub label: String,
    pub href: String,
}

/// `BookBody`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BookBody {
    pub files: Vec<BodyFile>,
    pub toc: Vec<TocEntry>,
    /// Page markers, in document order.
    pub page_list: Vec<PageEntry>,
    /// Picture filenames that reached a page, each once, in the order a page
    /// first drew it. The cover is not in here unless a page also draws it,
    /// so this is the answer to "does any page draw a figure", which decides
    /// the stylesheet's figure rules.
    pub used_images: Vec<String>,
    /// The picture `pick_cover` made the book's cover, on no page.
    pub cover_image: Option<String>,
}

/// Keep each body file comfortably under Kindle's per-flow size warnings.
pub const DEFAULT_MAX_FILE_BYTES: usize = 250_000;

/// A dual exchange whose taller column exceeds this many estimated rendered
/// lines degrades to sequential speeches (about half a device page).
const DUAL_SEQUENTIAL_LINE_THRESHOLD: usize = 12;
const EST_CHARS_PER_DUAL_LINE: usize = 30;

/// `escapeXml(s)`.
pub fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Fountain inline emphasis to XHTML, applied AFTER escaping.
fn inline_emphasis(escaped: &str) -> String {
    let mut s = escaped.to_owned();
    for (re, open, close) in &P.emphasis {
        s = re.replace_with(&s, true, |chars, caps| {
            format!(
                "{open}{}{close}",
                jsre::group(chars, caps, 1).unwrap_or_default()
            )
        });
    }
    s
}

fn xhtml_doc(title: &str, body: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<!DOCTYPE html>\n\
<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\">\n\
<head>\n\
<title>{}</title>\n\
<link rel=\"stylesheet\" type=\"text/css\" href=\"../style.css\"/>\n\
</head>\n\
<body>\n\
{body}</body>\n\
</html>\n",
        escape_xml(title)
    )
}

/// A page marker waiting for a block to ride inside, carried ACROSS scene
/// sections so a marker at a scene seam attaches to the next heading.
struct MarkerState {
    pending: Option<(String, String)>,
    /// Labels that landed in the blocks of the current section.
    landed: Vec<String>,
}

/// The pictures a book may show, where their markup points, and the ones it
/// did show. `in_flow` is the one question asked of a note, by the renderer
/// and by the section opener alike.
struct Art<'a> {
    cover: Option<&'a Token>,
    include_images: bool,
    have: &'a HashSet<String>,
    src: &'a dyn Fn(&str) -> String,
    used: Vec<String>,
}

impl Art<'_> {
    fn in_flow(&self, note: &Token) -> Option<ImageRef> {
        if self.cover.is_some_and(|c| std::ptr::eq(c, note)) {
            return None;
        }
        renders_image(note.text_or_empty(), self.include_images, self.have)
    }
}

/// `/^<(p|h[1-6]|div)\b[^>]*>/`: the length of a block's opening tag, in
/// bytes.
fn opening_tag_len(s: &str) -> Option<usize> {
    let chars: Vec<char> = s.chars().collect();
    let caps = P.opening_tag.exec_at(&chars, 0)?;
    let (_, end) = caps[0]?;
    Some(chars[..end].iter().map(|c| c.len_utf8()).sum())
}

/// Where the blocks of one section go: the open speech, if any, else the
/// section itself.
struct Blocks<'m> {
    blocks: Vec<String>,
    speech: Option<Vec<String>>,
    markers: &'m mut MarkerState,
}

impl Blocks<'_> {
    /// A block into whichever stream is open, declining any pending marker
    /// (only the figure does).
    fn place(&mut self, s: String) {
        match &mut self.speech {
            Some(cells) => cells.push(s),
            None => self.blocks.push(s),
        }
    }

    /// A block, with the pending page marker slipped inside its opening tag
    /// when the block is description. Any other block leaves it waiting: on
    /// a Kindle's KFX book a number in or beside a speech let its cue strand
    /// at a page's foot, and one on a heading sits on a keep chain too
    /// (registry #13a, 2026-10-29). A newer page number replaces a waiting
    /// one, which is then never shown or listed.
    fn emit(&mut self, mut s: String) {
        if !s.starts_with("<p class=\"action") {
            self.place(s);
            return;
        }
        if let Some((label, html)) = self.markers.pending.take() {
            if let Some(open) = opening_tag_len(&s) {
                s.insert_str(open, &html);
                self.markers.landed.push(label);
            } else {
                self.markers.pending = Some((label, html));
            }
        }
        self.place(s);
    }
}

fn speech_block(cells: &[String]) -> String {
    format!("<div class=\"dialogue-block\">\n{}</div>\n", cells.concat())
}

/// The rendered lines a dual column is estimated to take.
fn est_lines(col: &[String]) -> usize {
    col.iter()
        .map(|html| {
            let text = P.tag.replace_all(html, "");
            1.max(js::length(&text).div_ceil(EST_CHARS_PER_DUAL_LINE))
        })
        .sum()
}

/// Render body tokens (no title-page tokens) as discrete blocks: one string
/// per paragraph, a complete dialogue block as one string.
fn render_blocks(
    tokens: &[&Token],
    format: &FormatOptions,
    markers: &mut MarkerState,
    art: &mut Art<'_>,
) -> Vec<String> {
    let mut out = Blocks {
        blocks: Vec::new(),
        speech: None,
        markers,
    };
    // Simultaneous speech: (left, right, writing to the right column).
    let mut dual: Option<(Vec<String>, Vec<String>, bool)> = None;

    for &t in tokens {
        let raw = t.text_or_empty();
        // The shift is read BEFORE stripping; every note is stripped from
        // every token type in this one place.
        let fmt = if format.preserve_font_shifts {
            fmt_classes(raw)
        } else {
            String::new()
        };
        let text = strip_notes(raw);
        match t.kind {
            TokenType::SceneHeading => {
                let num = match t.scene_number.as_deref() {
                    Some(n) if format.show_scene_numbers && !n.is_empty() => {
                        format!("<span class=\"scene-number\">{}</span> ", escape_xml(n))
                    }
                    _ => String::new(),
                };
                out.emit(if is_mini_slug(t) {
                    format!("<p class=\"mini-slug\">{num}{}</p>\n", escape_xml(&text))
                } else {
                    format!(
                        "<h2 class=\"scene-heading\">{num}{}</h2>\n",
                        escape_xml(&text)
                    )
                });
            }
            TokenType::Action | TokenType::Lyrics => {
                out.emit(format!(
                    "<p class=\"action{fmt}\">{}</p>\n",
                    inline_emphasis(&escape_xml(&text))
                ));
            }
            TokenType::DualDialogueBegin => dual = Some((Vec::new(), Vec::new(), false)),
            TokenType::DualDialogueEnd => {
                if let Some((left, right, _)) = dual.take() {
                    if est_lines(&left).max(est_lines(&right)) > DUAL_SEQUENTIAL_LINE_THRESHOLD {
                        if !left.is_empty() {
                            out.blocks.push(speech_block(&left));
                        }
                        if !right.is_empty() {
                            out.blocks.push(speech_block(&right));
                        }
                    } else {
                        out.blocks.push(format!(
                            "<table class=\"dual-dialogue\">\n<tr>\n<td>\n{}</td>\n<td>\n{}</td>\n</tr>\n</table>\n",
                            left.concat(),
                            right.concat()
                        ));
                    }
                }
            }
            TokenType::DialogueBegin => {
                if let Some(d) = &mut dual {
                    d.2 = t.dual == Some(Dual::Right);
                }
                out.speech = Some(Vec::new());
            }
            TokenType::DialogueEnd => match (&mut dual, out.speech.take()) {
                (Some((left, right, on_right)), Some(cells)) => {
                    if *on_right {
                        right.extend(cells);
                    } else {
                        left.extend(cells);
                    }
                }
                (_, Some(cells)) => out.blocks.push(speech_block(&cells)),
                (_, None) => {}
            },
            TokenType::Character => {
                out.emit(format!(
                    "<p class=\"character\">{}</p>\n",
                    escape_xml(&text)
                ));
            }
            TokenType::Parenthetical => {
                out.emit(format!(
                    "<p class=\"parenthetical\">{}</p>\n",
                    escape_xml(&text)
                ));
            }
            TokenType::Dialogue => {
                // Multi-line speech arrives as one token with embedded
                // newlines (lyrics, verse): each line its own paragraph.
                for line in text.split('\n') {
                    if !trim(line).is_empty() {
                        out.emit(format!(
                            "<p class=\"dialogue{fmt}\">{}</p>\n",
                            inline_emphasis(&escape_xml(line))
                        ));
                    }
                }
            }
            TokenType::Transition => {
                // .replace(/^>\s*/, '')
                let bare = P
                    .transition
                    .replace_with(&text, false, |_, _| String::new());
                out.emit(format!(
                    "<p class=\"transition\">{}</p>\n",
                    escape_xml(&bare)
                ));
            }
            TokenType::Centered => {
                out.emit(format!(
                    "<p class=\"centered\">{}</p>\n",
                    inline_emphasis(&escape_xml(&text))
                ));
            }
            TokenType::Synopsis => {
                // "= pg N" lines are page markers; other synopses stay
                // invisible. /^pg\s+(\S+)$/ on the trimmed text.
                if let Some(label) = P
                    .page_marker
                    .captures(trim(&text))
                    .and_then(|m| m[1].clone())
                {
                    let esc = escape_xml(&label);
                    out.markers.pending = Some((
                        label,
                        format!(
                            "<span epub:type=\"pagebreak\" role=\"doc-pagebreak\" id=\"pg{esc}\" \
                             title=\"{esc}\" class=\"page-marker\">{esc}.</span>"
                        ),
                    ));
                }
            }
            TokenType::Note => {
                // A standalone `[[img: ...]]` is a picture (registry #19);
                // every other note renders nothing.
                let Some(img) = art.in_flow(t) else { continue };
                // The width as a class in steps of five; the clamp repeats
                // parseImgNote's on purpose.
                let step = 100f64.min(5f64.max(js::round(img.width_pct / 5.0) * 5.0));
                let full = if img.full_page { " figure-full" } else { "" };
                // `place`, not `emit`: a figure declines a pending marker.
                out.place(format!(
                    "<p class=\"figure w{}{full}\"><img src=\"{}\" alt=\"Illustration\"/></p>\n",
                    js::number(step),
                    escape_xml(&(art.src)(&img.file))
                ));
                if !art.used.contains(&img.file) {
                    art.used.push(img.file);
                }
            }
            // structural/no-render tokens: title page, page breaks, etc.
            _ => {}
        }
    }
    if let Some(cells) = out.speech.take() {
        out.blocks.push(speech_block(&cells));
    }
    out.blocks
}

/// Options for [`tokens_to_body`].
pub struct BodyOptions<'a> {
    pub max_file_bytes: usize,
    pub format: FormatOptions,
    /// Picture filenames the caller can ship. Empty renders no figures.
    pub images: HashSet<String>,
    /// Where a figure's `<img>` points; the book's own path by default.
    pub image_src: Option<&'a dyn Fn(&str) -> String>,
}

impl Default for BodyOptions<'_> {
    fn default() -> Self {
        BodyOptions {
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            format: FormatOptions::default(),
            images: HashSet::new(),
            image_src: None,
        }
    }
}

/// Where a figure points inside the EPUB: OEBPS/images/, one up from the
/// body files in OEBPS/text/.
fn book_image_src(file: &str) -> String {
    format!("../images/{file}")
}

struct SceneSection {
    anchor: String,
    title: String,
    html: String,
    page_labels: Vec<String>,
}

/// `tokensToBody(tokens, opts)`: split body tokens into anchored scene
/// sections, then pack sections into as few files as the budget allows.
/// Content before the first scene heading becomes an "Opening" section; a
/// title card that became the cover opens nothing.
pub fn tokens_to_body(tokens: &[Token], opts: &BodyOptions<'_>) -> BookBody {
    let format = &opts.format;
    let body: Vec<&Token> = tokens.iter().filter(|t| !t.is_title()).collect();
    // The cover is found among ALL the tokens (pick_cover passes over the
    // title page itself), and compared by identity like the TypeScript's.
    let cover = pick_cover(tokens, format, &opts.images);
    let mut art = Art {
        cover: cover.as_ref().map(|c| &tokens[c.index]),
        include_images: format.include_images,
        have: &opts.images,
        src: opts.image_src.unwrap_or(&book_image_src),
        used: Vec::new(),
    };

    let mut groups: Vec<(String, Vec<&Token>)> = Vec::new();
    for &t in &body {
        if t.kind == TokenType::SceneHeading && !is_mini_slug(t) {
            groups.push((
                t.text.clone().unwrap_or_else(|| "Scene".to_owned()),
                vec![t],
            ));
        } else {
            // A note that renders nothing may not open the Opening: a group
            // of nothing but such notes is an empty section.
            if groups.is_empty() && t.kind == TokenType::Note && art.in_flow(t).is_none() {
                continue;
            }
            if groups.is_empty() {
                groups.push(("Opening".to_owned(), Vec::new()));
            }
            if let Some(g) = groups.last_mut() {
                g.1.push(t);
            }
        }
    }

    // One marker state for the whole script: a marker at the tail of one
    // scene rides into the first block of the next.
    let mut markers = MarkerState {
        pending: None,
        landed: Vec::new(),
    };
    let sections: Vec<SceneSection> = groups
        .iter()
        .enumerate()
        .map(|(i, (title, toks))| {
            let anchor = format!("sc-{:03}", i + 1);
            markers.landed = Vec::new();
            let html = format!(
                "<section class=\"scene\" id=\"{anchor}\">\n{}</section>\n",
                render_blocks(toks, format, &mut markers, &mut art).concat()
            );
            SceneSection {
                anchor,
                title: title.clone(),
                html,
                page_labels: std::mem::take(&mut markers.landed),
            }
        })
        .collect();

    // Pack sections into files, starting a new file only when the budget
    // would be exceeded (a file always holds at least one section).
    let mut out = BookBody {
        used_images: Vec::new(),
        cover_image: cover.map(|c| c.file),
        ..BookBody::default()
    };
    let mut pending: Vec<&SceneSection> = Vec::new();
    let mut pending_bytes = 0usize;
    let flush = |pending: &mut Vec<&SceneSection>, out: &mut BookBody| {
        if pending.is_empty() {
            return;
        }
        let id = format!("body{:03}", out.files.len() + 1);
        for s in pending.iter() {
            out.toc.push(TocEntry {
                title: s.title.clone(),
                href: format!("text/{id}.xhtml#{}", s.anchor),
            });
            for label in &s.page_labels {
                out.page_list.push(PageEntry {
                    label: label.clone(),
                    href: format!("text/{id}.xhtml#pg{}", escape_xml(label)),
                });
            }
        }
        let html: String = pending.iter().map(|s| s.html.as_str()).collect();
        out.files.push(BodyFile {
            xhtml: xhtml_doc(&pending[0].title, &html),
            id,
        });
        pending.clear();
    };
    for s in &sections {
        let size = s.html.len();
        if !pending.is_empty() && pending_bytes.saturating_add(size) > opts.max_file_bytes {
            flush(&mut pending, &mut out);
            pending_bytes = 0;
        }
        pending.push(s);
        pending_bytes = pending_bytes.saturating_add(size);
    }
    flush(&mut pending, &mut out);
    out.used_images = art.used;
    out
}

/// One picture by name, for the preview: the bytes a figure's data: URI
/// carries.
pub type PreviewImages = [(String, Vec<u8>)];

/// `tokensToPreviewHtml(tokens, format, images)`: the whole script as ONE
/// self-contained HTML document, the same section markup as the EPUB body
/// with the stylesheet inlined and each figure's picture inside its `<img>`
/// as a data: URI. `images` is first-bytes-wins for a name given twice.
pub fn tokens_to_preview_html(
    tokens: &[Token],
    format: &FormatOptions,
    images: &PreviewImages,
) -> String {
    let lookup = |file: &str| {
        images
            .iter()
            .find(|(name, _)| name == file)
            .map(|(_, bytes)| bytes.as_slice())
            .unwrap_or_default()
    };
    let src = |file: &str| format!("data:image/png;base64,{}", base64(lookup(file)));
    let body = tokens_to_body(
        tokens,
        &BodyOptions {
            max_file_bytes: usize::MAX,
            format: *format,
            images: images.iter().map(|(name, _)| name.clone()).collect(),
            image_src: Some(&src),
        },
    );
    let doc = body
        .files
        .first()
        .map_or_else(|| xhtml_doc("Script", ""), |f| f.xhtml.clone());
    let css = screenplay_css(format, !body.used_images.is_empty());
    // .replace(/<link rel="stylesheet"[^>]*\/>/, () => ...): the first one.
    P.stylesheet_link
        .replace_with(&doc, false, |_, _| format!("<style>\n{css}</style>"))
}

/// Standard base64 with padding, as `Buffer.toString('base64')` writes it.
fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for k in 0..4 {
            if k <= chunk.len() {
                out.push(char::from(ABC[((n >> (18 - 6 * k)) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_node() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
    }

    #[test]
    fn emphasis_matches_the_regexes() {
        assert_eq!(
            inline_emphasis("***A*** **B** *c* _d_"),
            "<strong><em>A</em></strong> <strong>B</strong> <em>c</em> <span class=\"underline\">d</span>"
        );
        assert_eq!(inline_emphasis("2 * 3 math"), "2 * 3 math");
        assert_eq!(
            inline_emphasis("**_VOID_**"),
            "<strong><span class=\"underline\">VOID</span></strong>"
        );
        assert_eq!(inline_emphasis("* a*"), "* a*");
        // The answers bun gives for the TypeScript's regexes.
        for (input, js) in [
            ("****x****", "<em><strong><em>x</em></strong></em>"),
            ("* a*", "* a*"),
            ("**a* b**", "*<em>a</em> b**"),
            ("*a**b*", "<em>a</em><em>b</em>"),
            ("_a_b_", "<span class=\"underline\">a</span>b_"),
            ("***a** b*", "<em><strong>a</strong> b</em>"),
        ] {
            assert_eq!(inline_emphasis(input), js, "{input}");
        }
    }
}
