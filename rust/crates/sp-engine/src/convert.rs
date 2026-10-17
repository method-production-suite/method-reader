//! Port of `src/convert.ts`: the orchestration. A PDF's facts (what the PDF
//! layer read) become a parsed screenplay, then Fountain text, then
//! fountain-js tokens, then the books; a `.fountain` skips the first half.
//!
//! The two refusals (a scan, and a document with no scene heading and no
//! dialogue) are decided by the text alone, before any picture is decoded,
//! so a document that is turned away never pays for its pictures and puts
//! nothing on the caller's disk.

use std::collections::{BTreeMap, HashSet};
use std::fmt;

use sp_epub::{
    BodyOptions, BookImage, BookMeta, EpubError, build_epub, content_identifier, source_date_epoch,
    tokens_to_body, tokens_to_preview_html,
};
use sp_fountain::{FountainError, TitleMeta, extract_title_meta, is_safe_image_file, to_fountain};
use sp_mobi::{
    MOBI_CARRIES_PICTURES, MobiError, MobiInput, MobiMeta, build_mobi, mobi_art,
    tokens_to_mobi_html,
};
use sp_model::{ContdMode, ElementType, FormatOptions, ParsedScreenplay, RawImage, RawLine, Token};
use sp_parser::extract::extract_text;
use sp_parser::facts::DocumentFacts;
use sp_parser::first_page::{is_first_real_page, page_paragraphs};
use sp_parser::images::{FetchImage, ImageStats};
use sp_parser::parse_lines;
use sp_parser::png::is_png;

/// Below this many text lines per page, the PDF has no usable text layer.
const MIN_LINES_PER_PAGE: usize = 3;

/// Where the pictures and the book begin on the bar, as fractions of the
/// whole job; the pages fill everything before the first.
const PICTURES_START: f64 = 0.6;
const RENDER_START: f64 = 0.85;

/// The stages a caller is told about, in the order they occur. A document
/// that places no picture never reaches `Pictures`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConvertStage {
    Parse,
    Pictures,
    Render,
}

impl ConvertStage {
    pub fn as_str(self) -> &'static str {
        match self {
            ConvertStage::Parse => "parse",
            ConvertStage::Pictures => "pictures",
            ConvertStage::Render => "render",
        }
    }
}

/// `onProgress(stage, fraction, reading?)`: `fraction` is 0..1 across the
/// whole job; `reading` is `(page, pages)` while pages are read.
pub type OnProgress<'a> = dyn FnMut(ConvertStage, f64, Option<(u32, u32)>) + 'a;
/// `onFirstPage(page, paragraphs)`: at most once, the first page that is not
/// a title page, as plain paragraphs.
pub type OnFirstPage<'a> = dyn FnMut(u32, Vec<String>) + 'a;
/// The pixels for a picture whose bytes the PDF layer did not carry (an
/// oracle dump's facts hash the pixels instead): the encoded PNG, or
/// `None` to leave it out.
pub type PictureBytes<'a> = dyn Fn(&RawImage) -> Option<Vec<u8>> + 'a;

/// `ConvertOptions`.
#[derive(Default)]
pub struct ConvertOptions<'a> {
    pub title: Option<String>,
    pub author: Option<String>,
    /// Convert even when the document does not look like a screenplay.
    pub force: bool,
    /// Also build a MOBI 6.
    pub mobi: bool,
    /// Whether that MOBI carries the pictures and the cover; default
    /// [`MOBI_CARRIES_PICTURES`], the gate.
    pub mobi_pictures: Option<bool>,
    /// The formatting knobs, resolved (the CLI resolves its layers first).
    pub format: FormatOptions,
    pub on_progress: Option<&'a mut OnProgress<'a>>,
    pub on_first_page: Option<&'a mut OnFirstPage<'a>>,
    pub picture_bytes: Option<&'a PictureBytes<'a>>,
}

/// `ConvertResult`.
#[derive(Debug, Clone)]
pub struct ConvertResult {
    pub epub: Vec<u8>,
    pub mobi: Option<Vec<u8>>,
    pub preview_html: String,
    pub fountain_text: String,
    pub screenplay: Option<ParsedScreenplay>,
    pub title: String,
    pub author: Option<String>,
    pub warnings: Vec<String>,
    /// Word spaces the PDF layer invented that were removed; 0 for Fountain.
    pub spacing_repairs: u32,
    /// The pictures a PDF conversion carried, for the caller's picture
    /// folder. Always empty on Fountain input.
    pub images: Vec<BookImage>,
}

/// What a conversion refuses or fails on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConvertError {
    /// `ScannedPdfError`.
    Scanned { pages: u32, lines: usize },
    /// `NotAScreenplayError`.
    NotAScreenplay,
    /// pdf.js's `PasswordException`: the PDF is encrypted.
    Password,
    /// pdf.js's `InvalidPDFException`: not a PDF the reader can open.
    InvalidPdf,
    /// fountain-js's one throw.
    Fountain(String),
    /// A book maker refused (a bug of ours: the callers keep these out of
    /// reach).
    Book(String),
}

impl fmt::Display for ConvertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConvertError::Scanned { pages, lines } => write!(
                f,
                "No usable text layer ({lines} text lines across {pages} pages). \
                 This looks like a scanned/image-only PDF. Run OCR on it first."
            ),
            ConvertError::NotAScreenplay => f.write_str(
                "No scene headings and no dialogue found: this does not look like a \
                 screenplay. Pass --force to convert it anyway.",
            ),
            ConvertError::Password => f.write_str("No password given"),
            ConvertError::InvalidPdf => f.write_str("Invalid PDF structure."),
            ConvertError::Fountain(m) | ConvertError::Book(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for ConvertError {}

impl From<FountainError> for ConvertError {
    fn from(e: FountainError) -> Self {
        ConvertError::Fountain(e.to_string())
    }
}

impl From<EpubError> for ConvertError {
    fn from(e: EpubError) -> Self {
        ConvertError::Book(e.to_string())
    }
}

impl From<MobiError> for ConvertError {
    fn from(e: MobiError) -> Self {
        ConvertError::Book(e.to_string())
    }
}

/// `imageWarnings(stats, carried)`: what to tell someone about the pictures
/// that did not make it (only the two exceptional losses; see convert.ts).
pub fn image_warnings(stats: &ImageStats, carried: &[BookImage]) -> Vec<String> {
    let mut out = Vec::new();
    let kept = carried.len();
    if stats.over_cap > 0 {
        out.push(format!(
            "Method Reader found {} pictures in this script. Only the first {kept} were kept.",
            kept + stats.over_cap as usize
        ));
    }
    if stats.failed > 0 {
        out.push(if stats.failed == 1 {
            "1 picture could not be read out of the PDF, so it was left out.".to_owned()
        } else {
            format!(
                "{} pictures could not be read out of the PDF, so they were left out.",
                stats.failed
            )
        });
        if stats.decoders_unavailable == Some(true) {
            out.push(
                "Method Reader could not unpack its picture decoders, so pictures stored as JPEG2000 or JBIG2 could not be read."
                    .to_owned(),
            );
        }
    }
    out
}

fn resolve_meta(detected: &TitleMeta, opts: &ConvertOptions) -> (String, Option<String>) {
    let title = opts
        .title
        .clone()
        .or_else(|| detected.title.clone())
        .unwrap_or_else(|| "Untitled Screenplay".to_owned());
    let author = opts.author.clone().or_else(|| detected.author.clone());
    (title, author)
}

struct Books {
    epub: Vec<u8>,
    mobi: Option<Vec<u8>>,
    preview_html: String,
}

/// `renderBooks`: the books for one `.fountain`, with the pictures its
/// notes may show (see convert.ts for why one array feeds every renderer).
fn render_books(
    fountain_text: &str,
    title: &str,
    author: Option<&str>,
    want_mobi: bool,
    format: &FormatOptions,
    images: &[BookImage],
    mobi_pictures: bool,
) -> Result<Books, ConvertError> {
    let tokens = sp_fountain::tokenize(fountain_text)?;
    // The book's identity is its CONTENT, not the moment of rendering.
    let meta = BookMeta {
        title: title.to_owned(),
        author: author.map(str::to_owned),
        language: None,
        identifier: Some(content_identifier(fountain_text)),
    };
    let names: HashSet<String> = images.iter().map(|i| i.name.clone()).collect();
    let body = tokens_to_body(
        &tokens,
        &BodyOptions {
            format: *format,
            images: names,
            ..BodyOptions::default()
        },
    );
    let epub = build_epub(&meta, &body, format, images, source_date_epoch())?;
    // First bytes win for a name passed twice, as they do in build_epub.
    let mut by_name: Vec<(String, Vec<u8>)> = Vec::new();
    for img in images {
        if !by_name.iter().any(|(n, _)| *n == img.name) {
            by_name.push((img.name.clone(), img.bytes.clone()));
        }
    }
    let preview_html = tokens_to_preview_html(&tokens, format, &by_name);
    let mobi = if want_mobi {
        let have: HashSet<String> = if mobi_pictures {
            by_name.iter().map(|(n, _)| n.clone()).collect()
        } else {
            HashSet::new()
        };
        let art = mobi_art(&tokens, format, &have);
        let mobi_meta = MobiMeta {
            title: title.to_owned(),
            author: author.map(str::to_owned),
        };
        let html = tokens_to_mobi_html(&tokens, &mobi_meta, format, &art);
        let pictures = art
            .order
            .iter()
            .map(|name| {
                by_name
                    .iter()
                    .find(|(n, _)| n == name)
                    .map(|(_, b)| b.clone())
                    .unwrap_or_default()
            })
            .collect();
        Some(build_mobi(&MobiInput {
            title: title.to_owned(),
            author: author.map(str::to_owned),
            html,
            images: pictures,
            cover_index: art.cover_index,
            time: None,
        })?)
    } else {
        None
    };
    Ok(Books {
        epub,
        mobi,
        preview_html,
    })
}

fn tick(
    opts: &mut ConvertOptions,
    stage: ConvertStage,
    fraction: f64,
    reading: Option<(u32, u32)>,
) {
    if let Some(cb) = opts.on_progress.as_deref_mut() {
        cb(stage, fraction, reading);
    }
}

/// `convertPdf(pdfBytes, opts)`: the full pipeline for a screenplay PDF.
///
/// The pages are read first, text and operators only; a picture's pixels
/// are decoded only once both refusals have passed, as each picture is
/// reached, inside the picture budget.
pub fn convert_pdf(bytes: Vec<u8>, opts: ConvertOptions) -> Result<ConvertResult, ConvertError> {
    let doc = std::panic::catch_unwind(|| sp_pdf::Document::load(bytes))
        .map_err(|_| ConvertError::InvalidPdf)?
        .map_err(|e| match e {
            sp_pdf::LoadError::Encrypted => ConvertError::Password,
            sp_pdf::LoadError::Invalid => ConvertError::InvalidPdf,
        })?;
    let mut reader = doc.reader();
    let facts = sp_pdf::read_pages(&doc, &mut reader);
    let fetch = |id: &str| sp_pdf::image_fact(&reader, id);
    convert_read(&facts, opts, Some(&fetch))
}

/// `convertPdf`, from facts already read (an oracle dump's, or a test's).
pub fn convert_facts(
    facts: &DocumentFacts,
    opts: ConvertOptions,
) -> Result<ConvertResult, ConvertError> {
    convert_read(facts, opts, None)
}

/// [`convert_facts`], with the pixels the facts do not carry fetched
/// through `fetch` as each picture is reached: what [`convert_pdf`] does
/// with sp-pdf's reader, open to a caller that wants to watch the fetches
/// (a refused document must make none).
pub fn convert_facts_with(
    facts: &DocumentFacts,
    opts: ConvertOptions,
    fetch: Option<&FetchImage<'_>>,
) -> Result<ConvertResult, ConvertError> {
    convert_read(facts, opts, fetch)
}

fn convert_read(
    facts: &DocumentFacts,
    mut opts: ConvertOptions,
    fetch: Option<&FetchImage<'_>>,
) -> Result<ConvertResult, ConvertError> {
    let mut warnings = Vec::new();

    // The text only; the pictures are decoded below, once neither refusal
    // can fire.
    let mut first_page_sent = false;
    let pending = {
        let on_first = &mut opts.on_first_page;
        let on_progress = &mut opts.on_progress;
        let mut cb = |page: u32, pages: u32, lines: Option<&[RawLine]>, width: Option<f64>| {
            if let Some(first) = on_first.as_deref_mut()
                && !first_page_sent
                && let (Some(lines), Some(width)) = (lines, width)
                && width != 0.0
                && is_first_real_page(lines, width)
            {
                first_page_sent = true;
                first(page, page_paragraphs(lines));
            }
            if let Some(p) = on_progress.as_deref_mut() {
                p(
                    ConvertStage::Parse,
                    f64::from(page) / f64::from(pages) * PICTURES_START,
                    Some((page, pages)),
                );
            }
        };
        extract_text(facts, None, Some(&mut cb))
    };
    let lines = &pending.text.lines;
    let page_count = pending.text.page_count;
    if lines.len() < page_count as usize * MIN_LINES_PER_PAGE {
        return Err(ConvertError::Scanned {
            pages: page_count,
            lines: lines.len(),
        });
    }

    let text_only = parse_lines(lines, &[]);
    let scene_count = text_only.scenes.len();
    let dialogue_count = text_only
        .elements
        .iter()
        .filter(|e| e.kind == ElementType::Dialogue)
        .count();
    if scene_count == 0 && dialogue_count == 0 && !opts.force {
        return Err(ConvertError::NotAScreenplay);
    }

    let decoded = {
        let on_progress = &mut opts.on_progress;
        let mut cb = |page: u32, pages: u32, _: Option<&[RawLine]>, _: Option<f64>| {
            if let Some(p) = on_progress.as_deref_mut() {
                p(
                    ConvertStage::Pictures,
                    PICTURES_START
                        + f64::from(page) / f64::from(pages) * (RENDER_START - PICTURES_START),
                    None,
                );
            }
        };
        pending.decode_images_with(Some(&mut cb), fetch)
    };
    let mut raw_images = decoded.images;
    if let Some(fill) = opts.picture_bytes {
        for img in &mut raw_images {
            if img.bytes.is_empty()
                && let Some(bytes) = fill(img)
            {
                img.bytes = bytes;
            }
        }
    }
    let screenplay = if raw_images.is_empty() {
        text_only
    } else {
        parse_lines(lines, &raw_images)
    };
    let images: Vec<BookImage> = raw_images
        .iter()
        .map(|img| BookImage {
            name: img.file.clone(),
            bytes: img.bytes.clone(),
        })
        .collect();

    if scene_count == 0 {
        warnings
            .push("No scene headings detected: the EPUB will have no scene navigation.".to_owned());
    }
    warnings.extend(image_warnings(&decoded.image_stats, &images));

    let format = opts.format;
    let (title, author) = resolve_meta(&extract_title_meta(&screenplay.elements), &opts);
    let fountain_text = to_fountain(
        &screenplay,
        Some(&TitleMeta {
            title: Some(title.clone()),
            author: author.clone(),
        }),
        &format,
    );
    tick(&mut opts, ConvertStage::Render, RENDER_START, None);
    let books = render_books(
        &fountain_text,
        &title,
        author.as_deref(),
        opts.mobi,
        &format,
        &images,
        opts.mobi_pictures.unwrap_or(MOBI_CARRIES_PICTURES),
    )?;
    tick(&mut opts, ConvertStage::Render, 1.0, None);

    Ok(ConvertResult {
        epub: books.epub,
        mobi: books.mobi,
        preview_html: books.preview_html,
        fountain_text,
        screenplay: Some(screenplay),
        title,
        author,
        warnings,
        spacing_repairs: pending.text.spacing_repairs,
        images,
    })
}

/// JavaScript's line terminators, which `^` in a multiline regex follows.
fn js_lines(text: &str) -> impl Iterator<Item = &str> {
    text.split(['\n', '\r', '\u{2028}', '\u{2029}'])
}

/// `CONTD_CUE`: `/^@.*\(\s*CONT['’]?D\.?\s*\)/im`, line by line.
fn has_contd_cue(text: &str) -> bool {
    static CUE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"(?i)^@.*\([\s\x{feff}]*CONT['’]?D\.?[\s\x{feff}]*\)")
            .expect("CONTD_CUE")
    });
    js_lines(text).any(|line| CUE.is_match(line))
}

/// `stageOneWarnings`: the one provable case of a stage-1 option a
/// `.fountain` cannot honor.
fn stage_one_warnings(fountain_text: &str, format: &FormatOptions) -> Vec<String> {
    if format.contd_mode != ContdMode::Strip || !has_contd_cue(fountain_text) {
        return Vec::new();
    }
    vec![
        "This book still shows (CONT'D): it is written into the script the book was made from. \
         Removing it takes effect when you convert the script's PDF."
            .to_owned(),
    ]
}

/// `text.replace(/<[^>]+>/g, '')`.
fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(lt) = rest.find('<') {
        let after = &rest[lt + 1..];
        match after.find('>') {
            Some(gt) if gt > 0 => {
                out.push_str(&rest[..lt]);
                rest = &after[gt + 1..];
            }
            _ => {
                out.push_str(&rest[..=lt]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn first_text(tokens: &[Token], kinds: &[&str]) -> Option<Option<String>> {
    tokens
        .iter()
        .find(|t| kinds.contains(&t.kind.as_str()))
        .map(|t| t.text.clone())
}

/// `convertFountain(text, { ..., images })`: stage 1 skipped. `images` is
/// the picture folder beside the `.fountain` (filename to bytes); absent,
/// no note shows a picture.
pub fn convert_fountain(
    fountain_text: &str,
    opts: ConvertOptions,
    images: Option<&BTreeMap<String, Vec<u8>>>,
) -> Result<ConvertResult, ConvertError> {
    let tokens = sp_fountain::tokenize(fountain_text)?;
    let detected = TitleMeta {
        title: first_text(&tokens, &["title"])
            .flatten()
            .map(|t| strip_tags(&t)),
        author: first_text(&tokens, &["author", "authors"]).flatten(),
    };
    let format = opts.format;
    let (title, author) = resolve_meta(&detected, &opts);
    let images: Vec<BookImage> = images
        .into_iter()
        .flatten()
        .filter(|(name, bytes)| is_safe_image_file(name) && is_png(bytes))
        .map(|(name, bytes)| BookImage {
            name: name.clone(),
            bytes: bytes.clone(),
        })
        .collect();
    let books = render_books(
        fountain_text,
        &title,
        author.as_deref(),
        opts.mobi,
        &format,
        &images,
        opts.mobi_pictures.unwrap_or(MOBI_CARRIES_PICTURES),
    )?;
    Ok(ConvertResult {
        epub: books.epub,
        mobi: books.mobi,
        preview_html: books.preview_html,
        fountain_text: fountain_text.to_owned(),
        screenplay: None,
        title,
        author,
        warnings: stage_one_warnings(fountain_text, &format),
        spacing_repairs: 0,
        images: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_tags_matches_the_regex() {
        assert_eq!(strip_tags("<b>A</b> B"), "A B");
        assert_eq!(strip_tags("a <> b"), "a <> b");
        assert_eq!(strip_tags("a < b"), "a < b");
        assert_eq!(strip_tags("x<<y>z"), "xz");
    }

    #[test]
    fn contd_cue_is_found_on_any_line() {
        assert!(has_contd_cue("INT. A\n\n@BOB (CONT'D)\nHi"));
        assert!(has_contd_cue("@bob (cont’d.)"));
        assert!(!has_contd_cue("BOB (CONT'D)"));
    }
}
