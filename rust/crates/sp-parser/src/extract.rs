//! Port of `src/parser/extract.ts`: a PDF's pages, as the PDF layer
//! describes them ([`DocumentFacts`]), into lines of text and the pictures
//! worth keeping.
//!
//! The TypeScript asked pdf.js directly; here the same answers arrive as
//! facts (see facts.rs), and every pass over them is the TypeScript's,
//! constant for constant.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use sp_model::{FamilyBucket, FontRun, RawImage, RawLine, SizeStep};

use crate::cue::is_cue_text;
use crate::facts::{DocumentFacts, Op, PageFacts, TextItemFact};
use crate::images::{
    EncodedCandidate, FetchImage, ImageStats, PageSize, collect_image_placements,
    collect_page_images_with, finish_images, new_image_run,
};
use crate::js;

/// Called as a pass over the pages advances: `page` is how many pages it
/// has finished and `pages` how many it will visit, which makes
/// `page / pages` a usable fraction from the very first tick. The text pass
/// visits every page this run reads (so `pages` honors `max_pages`), and
/// hands over each page's grouped `lines` and its width in points, so a
/// caller can look at a page as soon as it is read (the first-page preview
/// does); the picture pass visits only the pages that place a picture (see
/// `decode_images`) and passes neither.
pub type PageProgress<'a> = dyn FnMut(u32, u32, Option<&[RawLine]>, Option<f64>) + 'a;

/// What the pass over the pages knows as soon as it has read them: the
/// text, and nothing that needed a picture's pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedText {
    /// Each line's text, indent, y position and page number. Indent is a
    /// percentage of page width (0-100), which is what makes the parser work
    /// across PDF generators and page sizes.
    pub lines: Vec<RawLine>,
    /// The DOCUMENT's page count, which is not the number of pages this run
    /// visited when `max_pages` is set.
    pub page_count: u32,
    /// How many text items `repair_phantom_spaces` rewrote.
    pub spacing_repairs: u32,
}

/// What decoding the pictures produces.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractedImages {
    /// The pictures that cleared artwork.rs, in reading order.
    pub images: Vec<RawImage>,
    /// What became of the pictures, including the ones that did not make it.
    pub image_stats: ImageStats,
}

/// Everything one document read produces: the shape of the oracle's
/// `lines.json`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExtractResult {
    pub lines: Vec<RawLine>,
    pub page_count: u32,
    pub spacing_repairs: u32,
    pub images: Vec<RawImage>,
    pub image_stats: ImageStats,
}

/// The text now, and the pictures only when asked for. See [`extract_text`].
pub struct PendingExtract<'a> {
    pub text: ExtractedText,
    facts: &'a DocumentFacts,
    sizes: Vec<PageSize>,
    /// Every visited page that has an operator list: its index into
    /// `facts.pages`, and the text-line count that tells a designed title
    /// card from a watermark printed behind a full page of script.
    pending: Vec<(usize, usize)>,
}

impl PendingExtract<'_> {
    /// Encode and judge the pictures the page pass located.
    ///
    /// `on_progress` hears about the pages that place a picture, and only
    /// those: one tick at 0 before the first of them, so a caller can name
    /// the stage the moment it begins rather than after its first page, and
    /// one after each. A document that places none never calls it.
    pub fn decode_images(&self, on_progress: Option<&mut PageProgress<'_>>) -> ExtractedImages {
        self.decode_images_with(on_progress, None)
    }

    /// [`Self::decode_images`], with the pixels the facts do not carry
    /// fetched through `fetch` as each picture is reached: how the engine
    /// keeps a document's pictures undecoded until both refusals have
    /// passed, while the picture budget still charges their decoding.
    pub fn decode_images_with(
        &self,
        mut on_progress: Option<&mut PageProgress<'_>>,
        fetch: Option<&FetchImage<'_>>,
    ) -> ExtractedImages {
        // ONE budget for the whole document, threaded through every page,
        // and the same object caches encoded pixels.
        let mut run = new_image_run();
        let mut candidates: Vec<EncodedCandidate> = Vec::new();
        // Only the pages that place a picture, which changes nothing about
        // the result: collect_page_images finds nothing on any other page.
        // The filter is for the ticks, so they count the work this loop
        // actually does.
        let with_pictures: Vec<&(usize, usize)> = self
            .pending
            .iter()
            .filter(|(i, _)| {
                self.facts.pages[*i]
                    .ops
                    .as_deref()
                    .is_some_and(|ops| !collect_image_placements(ops).is_empty())
            })
            .collect();
        let total = u32::try_from(with_pictures.len()).unwrap_or(u32::MAX);
        if total > 0
            && let Some(cb) = on_progress.as_deref_mut()
        {
            cb(0, total, None, None);
        }
        // In page order, so the budget is spent and the encode cache is
        // filled in document order: the cap and the budget both rely on that.
        for (done, &&(i, text_lines)) in with_pictures.iter().enumerate() {
            let page = &self.facts.pages[i];
            candidates.extend(collect_page_images_with(
                page,
                page.ops.as_deref(),
                page.page_num,
                text_lines,
                &mut run,
                fetch,
            ));
            if let Some(cb) = on_progress.as_deref_mut() {
                cb(
                    u32::try_from(done + 1).unwrap_or(u32::MAX),
                    total,
                    None,
                    None,
                );
            }
        }
        // Which pictures are art is a DOCUMENT fact too, for the same shape
        // of reason as the font pass: repetition is only visible once every
        // sheet has been read. The page count handed over is the whole
        // document's, not this run's, which is what `keep_artwork` asks for.
        let (images, image_stats) =
            finish_images(&candidates, &self.sizes, self.facts.num_pages, &run);
        ExtractedImages {
            images,
            image_stats,
        }
    }
}

/// Read a document: its lines, its true page count, and its pictures.
/// [`extract_text`] followed straight away by its `decode_images`.
pub fn extract_document(
    facts: &DocumentFacts,
    max_pages: Option<u32>,
    on_progress: Option<&mut PageProgress<'_>>,
) -> ExtractResult {
    let pending = extract_text(facts, max_pages, on_progress);
    let ExtractedImages {
        images,
        image_stats,
    } = pending.decode_images(None);
    ExtractResult {
        lines: pending.text.lines,
        page_count: pending.text.page_count,
        spacing_repairs: pending.text.spacing_repairs,
        images,
        image_stats,
    }
}

/// Read a document's text, and leave its pictures for later: located on
/// the way past, decoded only if `decode_images` is called.
///
/// The split is for the engine, whose two refusals are both decided by the
/// text alone, so a document either one turns away never pays for its
/// pictures.
///
/// `max_pages` of `None` or `Some(0)` reads every page (the TypeScript's
/// falsy test).
pub fn extract_text<'a>(
    facts: &'a DocumentFacts,
    max_pages: Option<u32>,
    mut on_progress: Option<&mut PageProgress<'_>>,
) -> PendingExtract<'a> {
    let mut all_lines: Vec<RawLine> = Vec::new();
    // Counted, not silent. repair_phantom_spaces edits an author's words, so
    // the number rides out with the result and the app can say so. A count
    // that suddenly jumps on a script is how anyone would notice it
    // misfiring.
    let mut spacing_repairs = 0;
    let mut sizes = Vec::new();
    let mut pending = Vec::new();

    let last_page = match max_pages {
        Some(m) if m > 0 => facts.num_pages.min(m),
        _ => facts.num_pages,
    };
    for (index, page) in facts.pages.iter().enumerate() {
        if page.page_num > last_page {
            break;
        }
        sizes.push(PageSize {
            width: page.width,
            height: page.height,
        });
        let mut items: Vec<TextItem> = page.items.iter().map(TextItem::from_fact).collect();
        let ops = page.ops.as_deref();
        stamp_font_styles(page, &mut items);
        if let Some(ops) = ops {
            stamp_underlines(&mut items, ops, page.width);
        }
        // Before grouping, because every stage downstream reads `str` and a
        // phantom space fragments a word into two tokens for all of them.
        spacing_repairs += repair_phantom_spaces(&mut items, ops);

        let page_lines = group_items_into_lines(&items, page.width, page.page_num);
        // The same operator list the text pass just read, kept for the
        // pictures. A page with none gives no pictures, just as the text pass
        // above gets unresolved fonts and no underlines or repairs from it.
        if ops.is_some() {
            pending.push((index, page_lines.len()));
        }
        // After the work, not before: a tick means "page N is done", so the
        // fraction never claims progress the parser has not actually made.
        if let Some(cb) = on_progress.as_deref_mut() {
            cb(
                page.page_num,
                last_page,
                Some(&page_lines),
                Some(page.width),
            );
        }
        all_lines.extend(page_lines);
    }

    // Dominant font is a DOCUMENT fact, so this runs once, after every page:
    // a chyron page set entirely in Helvetica is still a deviation from a
    // Courier script (registry #18).
    stamp_line_fmt(&mut all_lines);

    PendingExtract {
        text: ExtractedText {
            lines: all_lines,
            page_count: facts.num_pages,
            spacing_repairs,
        },
        facts,
        sizes,
        pending,
    }
}

/// A text item as the passes below see it: pdf.js's own fields, plus what
/// the style passes stamp on it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextItem {
    pub str: String,
    pub transform: Vec<f64>,
    pub font_name: Option<String>,
    pub italic: bool,
    pub bold: bool,
    /// a rule was DRAWN under this item — never readable from font data
    pub underline: bool,
    /// coarse family bucket from the resolved PostScript name (registry #18)
    pub bucket: Option<FamilyBucket>,
    /// actual rendered width from pdf.js — much more accurate than the
    /// len*6 estimate (which false-splits names like "Marlowe" → "Mar lowe")
    pub width: Option<f64>,
}

impl TextItem {
    /// An unstamped item from its facts.
    pub fn from_fact(f: &TextItemFact) -> Self {
        Self {
            str: f.str.clone(),
            transform: f.transform.clone(),
            font_name: f.font_name.clone(),
            width: f.width,
            ..Self::default()
        }
    }

    /// `transform[i]`, or NaN where the TypeScript read `undefined`.
    pub fn t(&self, i: usize) -> f64 {
        self.transform.get(i).copied().unwrap_or(f64::NAN)
    }
}

#[allow(clippy::cast_precision_loss)]
fn end_x(item: &TextItem) -> f64 {
    item.t(4) + item.width.unwrap_or(js::len(&item.str) as f64 * 6.0)
}

static ALL_WS: LazyLock<Regex> = LazyLock::new(|| js::re(r"\s+"));

fn strip_ws(s: &str) -> String {
    ALL_WS.replace_all(s, "").into_owned()
}

/// Undo word spaces pdf.js invented, using the glyph stream as the truth.
///
/// pdf.js turns a wide inter-glyph gap into a space in the string it hands
/// us. Usually right; wrong on a PDF whose text has been re-encoded. A Final
/// Draft script re-saved through Quartz carried a uniform +17 tracking
/// adjustment on every glyph pair and, about once a page, one anomalous
/// ~-110 that widened a single gap by roughly 1.5pt. That is a fifth of a
/// character, far short of a space, and pdf.js called it one: "these"
/// arrived as "thes e", 94 times across 70 of 112 pages.
///
/// Two facts make this repairable rather than guesswork, and both matter:
///
/// A REAL space is a glyph. It appears in the run as its own entry with a
/// full cell of advance. An invented one is not in the run at all. So a
/// writer who spaced out a word deliberately keeps every space, and we never
/// have to reason about whether they meant it.
///
/// And SCREENPLAYS ARE MONOSPACED, so the ink says how many characters there
/// really are. Every clean item on a page divides to the same cell width; an
/// item carrying a phantom is a whole cell short of its own string. That is
/// the flag, and it is why this cannot fire on a proportional font: there is
/// no grid, so no item is ever flagged. (Measured on a pitch deck: 87 items
/// look short, and every one of them is a false positive. The floor on
/// `width` and the uniqueness check below are what keep that harmless, but
/// the real protection is that this only runs where a script is Courier.)
///
/// Returns how many items it repaired.
#[allow(clippy::neg_cmp_op_on_partial_ord)] // `!(cell > 0)` refuses NaN too
pub fn repair_phantom_spaces(items: &mut [TextItem], ops: Option<&[Op]>) -> u32 {
    let Some(ops) = ops else { return 0 };

    // The glyph runs, keyed by their text with ALL whitespace removed, so a
    // run and the item it produced match even though their spacing differs —
    // which is the entire point. A key claimed twice is dropped: picking one
    // would be a coin flip, and a wrong repair is worse than none.
    let mut by_key: HashMap<String, Option<String>> = HashMap::new();
    for op in ops {
        // Glyphs only. The numbers between them are positioning, and reading
        // them as anything else is the bug this function exists to undo.
        let Op::ShowText { text: Some(text) } = op else {
            continue;
        };
        if js::trim(text).is_empty() {
            continue;
        }
        let key = strip_ws(text);
        let claimed = by_key.contains_key(&key);
        by_key.insert(key, if claimed { None } else { Some(text.clone()) });
    }
    if by_key.is_empty() {
        return 0;
    }

    // The page's cell width, as the median of width-per-character over lines
    // long enough to be representative. Median, not mean: the damaged items
    // are the outliers we are hunting and must not drag the baseline toward
    // themselves.
    #[allow(clippy::cast_precision_loss)]
    let mut cells: Vec<f64> = items
        .iter()
        .filter_map(|t| match t.width {
            Some(w) if w > 0.0 && js::len(js::trim(&t.str)) > 8 => Some(w / js::len(&t.str) as f64),
            _ => None,
        })
        .collect();
    if cells.len() < 3 {
        return 0;
    }
    js::sort_by(&mut cells, |a, b| js::cmp_num(*a, *b));
    let cell = cells[cells.len() / 2];
    if !(cell > 0.0) {
        return 0;
    }

    let mut repaired = 0;
    for t in items.iter_mut() {
        let Some(width) = t.width.filter(|w| *w > 0.0) else {
            continue;
        };
        // 0.6 of a cell: comfortably past the rounding in a real line
        // (measured spread is under 0.02) and comfortably under the 1.0 a
        // genuine missing character would cost.
        #[allow(clippy::cast_precision_loss)]
        if (js::len(&t.str) as f64 - width / cell) < 0.6 {
            continue;
        }
        let Some(Some(truth)) = by_key.get(&strip_ws(&t.str)) else {
            continue;
        };
        if truth.is_empty() {
            continue;
        }
        // Compare TRIMMED. A glyph run usually ends with a trailing space that
        // the item does not carry, which makes the two the same length even
        // when the run has one less space in the middle — so a raw comparison
        // reads "no improvement" and skips. That is not hypothetical: it let
        // three of the four known cases through on the first cut of this, and
        // only the one run that happened to have no trailing space was fixed.
        // Leading and trailing space is safe to drop here regardless: indent
        // comes from the transform, and join_line trims and collapses anyway.
        let fixed = js::trim(truth);
        // Only ever REMOVE spacing. A repair that added text would mean the two
        // strings disagreed about something other than whitespace, and this
        // function has no business fixing that.
        if js::len(fixed) >= js::len(js::trim(&t.str)) {
            continue;
        }
        t.str = fixed.to_owned();
        repaired += 1;
    }
    repaired
}

static REVISION_STAR: LazyLock<Regex> = LazyLock::new(|| js::re(r"^\*+$"));

/// `groupItemsIntoLines`: one page's items as lines, top to bottom.
pub fn group_items_into_lines(items: &[TextItem], page_width: f64, page_num: u32) -> Vec<RawLine> {
    if items.is_empty() {
        return Vec::new();
    }

    // Group items by Y coordinate, clustering within a small tolerance —
    // baselines drift inside one visual line (one generator prints "(CONT'D)"
    // a point above its cue name) while real line spacing is ~12pt.
    const Y_TOLERANCE: f64 = 2.0;
    let mut sorted: Vec<&TextItem> = items.iter().collect();
    js::sort_by(&mut sorted, |a, b| js::cmp_num(b.t(5), a.t(5)));
    let mut clusters: Vec<(f64, Vec<&TextItem>)> = Vec::new();
    for item in sorted {
        match clusters.last_mut() {
            Some((y, members)) if *y - item.t(5) <= Y_TOLERANCE => members.push(item),
            _ => clusters.push((item.t(5), vec![item])),
        }
    }

    // Right-margin zone where revision stars live (Final Draft puts them at
    // ~94% of page width). Star-only items there are production markup, not
    // text — left in, a bare "*" line classifies as action and resets the
    // active-character context, fragmenting revised dialogue.
    let revision_margin_x = page_width * 0.8;

    let mut lines: Vec<LineItems> = Vec::new();
    for (y, cluster) in clusters {
        // Drop empty items (marked-content markers) and right-margin revision
        // stars — both would corrupt joining/classification below.
        let mut line_items: Vec<TextItem> = cluster
            .into_iter()
            .filter(|item| !item.str.is_empty())
            .filter(|item| !(REVISION_STAR.is_match(&item.str) && item.t(4) >= revision_margin_x))
            .cloned()
            .collect();
        if line_items.is_empty() {
            continue;
        }
        js::sort_by(&mut line_items, |a, b| js::cmp_num(a.t(4), b.t(4)));
        lines.push(LineItems {
            y: js::round(y),
            items: line_items,
        });
    }

    deinterleave_dual_dialogue(&lines, page_width, page_num)
}

struct LineItems {
    y: f64,
    items: Vec<TextItem>,
}

/// A drawn horizontal rule that might be an underline, in PDF user space —
/// the same space text baselines live in (origin bottom-left).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnderlineMark {
    pub x0: f64,
    pub x1: f64,
    pub y: f64,
}

// Geometry bounds, calibrated 2026-08-04 against the local generator set.
// Real underlines measured 0.0-0.6pt tall, 36-168pt wide, and sat 1.5-3.0pt
// below their baseline. Every constant has a decoy in the torture fixture.
const MARK_MAX_HEIGHT: f64 = 2.5; // taller is a box or a filled bar
const MARK_MIN_WIDTH: f64 = 4.0; // narrower is a bullet or a tick
const MARK_MAX_WIDTH_FRAC: f64 = 0.85; // wider is a header/footer rule
const MARK_BELOW: f64 = 3.5; // deepest a mark may sit under a baseline
const MARK_ABOVE: f64 = 0.5; // a strikethrough sits far higher than this
const MARK_MIN_OVERLAP: f64 = 0.6; // fraction of the item a mark must cover

/// `m ∘ t` — PDF's `cm` post-multiplies onto the current matrix.
fn concat(m: &[f64; 6], t: &[f64]) -> [f64; 6] {
    let t = |i: usize| t.get(i).copied().unwrap_or(f64::NAN);
    [
        m[0] * t(0) + m[2] * t(1),
        m[1] * t(0) + m[3] * t(1),
        m[0] * t(2) + m[2] * t(3),
        m[1] * t(2) + m[3] * t(3),
        m[0] * t(4) + m[2] * t(5) + m[4],
        m[1] * t(4) + m[3] * t(5) + m[5],
    ]
}

/// `Math.min` of two: NaN if either is.
fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}

fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}

/// Walk a page's operator list for drawn rules that could be underlines.
///
/// pdf.js folds all path painting into `constructPath`, whose args carry the
/// paint op and a bounding box. The box alone separates an underline (flat
/// and short) from a box (tall) or a page rule (wide), so the packed path
/// data is never decoded. If a generator ever batches several underlines
/// into one path with several subpaths, decoding that into per-subpath boxes
/// is the refinement — not built until a real script needs it.
///
/// Best-effort by design, like `stamp_font_styles`: anything unexpected
/// yields no marks.
pub fn collect_underline_marks(ops: &[Op], page_width: f64) -> Vec<UnderlineMark> {
    let mut marks = Vec::new();
    let identity = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut m = identity;
    let mut stack: Vec<[f64; 6]> = Vec::new();

    for op in ops {
        match op {
            Op::Save => {
                stack.push(m);
                continue;
            }
            Op::Restore => {
                m = stack.pop().unwrap_or(identity);
                continue;
            }
            Op::Transform { matrix } => {
                m = concat(&m, matrix);
                continue;
            }
            _ => {}
        }
        let Op::ConstructPath { paint, bbox } = op else {
            continue;
        };

        // endPath is the clip-path spelling (`W n`): it builds geometry and
        // paints nothing. Every full-page box the browser-print generator
        // draws is one. Every OTHER paint op is accepted — real underlines are
        // STROKED, not filled, so requiring a fill would detect nothing real.
        if paint
            .as_ref()
            .is_some_and(crate::facts::PaintOp::is_end_path)
        {
            continue;
        }
        let Some(bb) = bbox.as_deref().filter(|b| b.len() >= 4) else {
            continue;
        };

        // Skew or rotation: the bbox stops describing an axis-aligned rule,
        // and screenplays do not underline on a slant. Skip rather than guess.
        if m[1].abs() > 1e-6 || m[2].abs() > 1e-6 {
            continue;
        }

        // BOTH corners, then min/max. Final Draft draws its underlines under a
        // y-flip ([1,0,0,-1,0,pageHeight]); transforming one corner, or
        // treating a flip as "rotated, skip it", puts the mark hundreds of
        // points off and no baseline ever matches.
        let ax = m[0] * bb[0] + m[2] * bb[1] + m[4];
        let ay = m[1] * bb[0] + m[3] * bb[1] + m[5];
        let bx = m[0] * bb[2] + m[2] * bb[3] + m[4];
        let by = m[1] * bb[2] + m[3] * bb[3] + m[5];
        let x0 = js_min(ax, bx);
        let x1 = js_max(ax, bx);
        let y0 = js_min(ay, by);
        let y1 = js_max(ay, by);

        let width = x1 - x0;
        if y1 - y0 > MARK_MAX_HEIGHT {
            continue;
        }
        if width < MARK_MIN_WIDTH {
            continue;
        }
        if width >= page_width * MARK_MAX_WIDTH_FRAC {
            continue;
        }

        marks.push(UnderlineMark {
            x0,
            x1,
            y: (y0 + y1) / 2.0,
        });
    }
    marks
}

/// True when `mark` underlines the text item spanning [x0, x1) on
/// `baseline`. Pure geometry.
///
/// The band excludes a strikethrough (which sits mid x-height, ABOVE the
/// baseline) and the row below's own underline (12pt away at screenplay
/// spacing). The overlap floor keeps a rule under part of a run from marking
/// the whole run: item granularity means the alternative to dropping it is
/// wrapping characters that are not underlined.
pub fn mark_underlines_item(mark: &UnderlineMark, x0: f64, x1: f64, baseline: f64) -> bool {
    if mark.y > baseline + MARK_ABOVE {
        return false;
    }
    if mark.y < baseline - MARK_BELOW {
        return false;
    }
    let width = x1 - x0;
    if width <= 0.0 {
        return false;
    }
    let overlap = js_min(x1, mark.x1) - js_max(x0, mark.x0);
    overlap / width >= MARK_MIN_OVERLAP
}

static SUBSET_PREFIX: LazyLock<Regex> = LazyLock::new(|| js::re(r"^[A-Z]{6}\+"));
static NON_ALNUM: LazyLock<Regex> = LazyLock::new(|| js::re(r"[^a-z0-9]"));

/// PostScript base name → coarse family bucket.
///
/// Matched against the name lowercased with its subset prefix ("AAAAAB+")
/// and every non-alphanumeric stripped, so "Letter Gothic" and
/// "LetterGothic" both read as "lettergothic". Two traps this shape avoids:
///   - bare "gothic" would swallow Century Gothic, which is a sans;
///   - bare "roman" would swallow Helvetica-Roman and AvenirNext-Roman, where
///     "-Roman" is the REGULAR weight, not the family. Times matches "times".
///
/// Weight and slope tokens (bold, black, heavy, italic, oblique) match
/// nothing here on purpose: they are style, and style is registry 9d's
/// business.
pub fn family_bucket(name: &str) -> Option<FamilyBucket> {
    let unprefixed = SUBSET_PREFIX.replace(name, "");
    let n = NON_ALNUM
        .replace_all(&unprefixed.to_lowercase(), "")
        .into_owned();
    if n.is_empty() {
        return None;
    }
    const FAMILY_PATTERNS: [(&[&str], FamilyBucket); 3] = [
        (
            &["courier", "mono", "lettergothic", "prestige"],
            FamilyBucket::Mono,
        ),
        (
            &[
                "times",
                "georgia",
                "garamond",
                "palatino",
                "caslon",
                "baskerville",
                "minion",
            ],
            FamilyBucket::Serif,
        ),
        (&["script", "hand", "brush", "comic"], FamilyBucket::Cursive),
    ];
    for (words, bucket) in FAMILY_PATTERNS {
        if words.iter().any(|w| n.contains(w)) {
            return Some(bucket);
        }
    }
    Some(FamilyBucket::Sans)
}

// Deviation thresholds (registry #18). A step needs BOTH a ratio outside the
// band and >= 1.5pt of absolute change, so float jitter in a text matrix can
// never mint one.
const SIZE_SMALLER_RATIO: f64 = 0.85;
const SIZE_BIGGER_RATIO: f64 = 1.15;
const SIZE_BIGGEST_RATIO: f64 = 1.4;
const SIZE_MIN_DELTA: f64 = 1.5;
/// Share of a line's resolved characters that must agree on one deviation.
const FMT_AGREEMENT: f64 = 0.8;

fn size_step(size: f64, dominant: f64) -> Option<SizeStep> {
    if (size - dominant).abs() < SIZE_MIN_DELTA {
        return None;
    }
    let ratio = size / dominant;
    if ratio <= SIZE_SMALLER_RATIO {
        return Some(SizeStep::Smaller);
    }
    if ratio > SIZE_BIGGEST_RATIO {
        return Some(SizeStep::MuchLarger);
    }
    if ratio >= SIZE_BIGGER_RATIO {
        return Some(SizeStep::Larger);
    }
    None
}

/// An insertion-ordered tally, as a JavaScript `Map<K, number>` iterates.
struct Tally<K>(Vec<(K, u32)>);

impl<K: PartialEq + Copy> Tally<K> {
    fn new() -> Self {
        Self(Vec::new())
    }

    fn add(&mut self, k: K, n: u32) {
        match self.0.iter_mut().find(|(key, _)| *key == k) {
            Some((_, v)) => *v += n,
            None => self.0.push((k, n)),
        }
    }

    /// The key with the largest tally (the first seen on a tie), or `None`
    /// for an empty tally.
    fn argmax(&self) -> Option<K> {
        let mut best = None;
        let mut best_n: i64 = -1;
        for &(k, n) in &self.0 {
            if i64::from(n) > best_n {
                best = Some(k);
                best_n = i64::from(n);
            }
        }
        best
    }

    /// The first key (in insertion order) carrying at least `FMT_AGREEMENT`
    /// of `total`.
    fn agreed(&self, total: u32) -> Option<K> {
        self.0
            .iter()
            .find(|(_, n)| f64::from(*n) / f64::from(total) >= FMT_AGREEMENT)
            .map(|(k, _)| *k)
    }
}

/// Stamp `fmt` on every line that deviates from the document's dominant
/// font.
///
/// Runs over the WHOLE document, after every page, because "dominant" is a
/// document-level fact: a chyron page set entirely in Helvetica is still a
/// deviation from a Courier script. Weighted by character count so a page of
/// one-word sans lines cannot outvote the body.
///
/// A uniform Courier-12 screenplay produces zero fmt anywhere, by
/// construction: every run agrees with the dominant, so neither tally below
/// is ever non-empty.
pub fn stamp_line_fmt(lines: &mut [RawLine]) {
    let mut doc_family: Tally<FamilyBucket> = Tally::new();
    // Sizes keyed by value, as a Map of numbers is (SameValueZero).
    let mut doc_size: Tally<u64> = Tally::new();
    for line in lines.iter() {
        for run in line.fonts.iter().flatten() {
            doc_family.add(run.bucket, run.chars);
            doc_size.add((run.size + 0.0).to_bits(), run.chars);
        }
    }
    let (Some(dom_family), Some(dom_size)) = (doc_family.argmax(), doc_size.argmax()) else {
        return;
    };
    let dom_size = f64::from_bits(dom_size);
    if dom_size <= 0.0 {
        return;
    }

    for line in lines.iter_mut() {
        let Some(runs) = line.fonts.as_ref().filter(|r| !r.is_empty()) else {
            continue;
        };
        let total: u32 = runs.iter().map(|r| r.chars).sum();
        if total == 0 {
            continue;
        }

        let mut by_family: Tally<FamilyBucket> = Tally::new();
        let mut by_step: Tally<SizeStep> = Tally::new();
        for run in runs {
            if run.bucket != dom_family {
                by_family.add(run.bucket, run.chars);
            }
            if let Some(step) = size_step(run.size, dom_size) {
                by_step.add(step, run.chars);
            }
        }
        let family = by_family.agreed(total);
        let size = by_step.agreed(total);
        if family.is_some() || size.is_some() {
            line.fmt = Some(sp_model::Fmt { family, size });
        }
    }
}

static ITALIC_NAME: LazyLock<Regex> = LazyLock::new(|| js::re_i("italic|oblique"));
static BOLD_NAME: LazyLock<Regex> = LazyLock::new(|| js::re_i("bold|black|heavy"));

/// Mark each item bold/italic from its font's PostScript name (e.g.
/// "CourierPrime-Italic"). Best-effort: an unresolved font leaves its items
/// plain.
fn stamp_font_styles(page: &PageFacts, items: &mut [TextItem]) {
    for item in items.iter_mut() {
        let Some(id) = item.font_name.as_deref().filter(|id| !id.is_empty()) else {
            continue;
        };
        let name = page.fonts.get(id).cloned().flatten().unwrap_or_default();
        let folded = js::ci(&name);
        item.italic = ITALIC_NAME.is_match(&folded);
        item.bold = BOLD_NAME.is_match(&folded);
        item.bucket = family_bucket(&name);
    }
}

/// Per-run font tallies for a line's items, merging equal neighbours.
///
/// Size comes from the text matrix's vertical scale, which is `transform[3]`
/// for the unrotated text screenplays are made of; the absolute value keeps
/// a flipped matrix from reporting a negative size. Items whose font never
/// resolved are skipped entirely, so the shares in `stamp_line_fmt` are over
/// resolved characters rather than being diluted by unknowns.
fn font_runs(items: &[TextItem]) -> Option<Vec<FontRun>> {
    let mut out: Vec<FontRun> = Vec::new();
    for item in items {
        let chars = u32::try_from(js::len(js::trim(&item.str))).unwrap_or(u32::MAX);
        let Some(bucket) = item.bucket else { continue };
        if chars == 0 {
            continue;
        }
        let size = js::round(item.t(3).abs() * 10.0) / 10.0;
        match out.last_mut() {
            Some(last) if last.bucket == bucket && last.size == size => last.chars += chars,
            _ => out.push(FontRun {
                bucket,
                size,
                chars,
            }),
        }
    }
    (!out.is_empty()).then_some(out)
}

/// Mark each item underlined when a drawn rule sits in the band below its
/// baseline. Underline is DRAWN, not selected, so unlike bold/italic it can
/// never be read from font data (registry 9d).
fn stamp_underlines(items: &mut [TextItem], ops: &[Op], page_width: f64) {
    let marks = collect_underline_marks(ops, page_width);
    if marks.is_empty() {
        return;
    }
    for item in items.iter_mut() {
        if item.str.is_empty() || js::trim(&item.str).is_empty() {
            continue;
        }
        let x0 = item.t(4);
        let baseline = item.t(5);
        let x1 = end_x(item);
        if marks
            .iter()
            .any(|m| mark_underlines_item(m, x0, x1, baseline))
        {
            item.underline = true;
        }
    }
}

/// Join sorted items into text: spaces at >5pt gaps, exact-overlap dedup
/// (Final Draft double-prints (MORE)/(CONT'D) furniture).
fn join_items(items: &[&TextItem]) -> String {
    let mut text = String::new();
    let mut prev_end_x = -1.0;
    let mut prev_x = f64::NAN;
    let mut prev_str: &str = "";

    for item in items {
        let x = item.t(4);
        if item.str == prev_str && (x - prev_x).abs() < 2.0 {
            continue;
        }
        if prev_end_x >= 0.0 && x - prev_end_x > 5.0 {
            text.push(' ');
        }
        text.push_str(&item.str);
        prev_end_x = end_x(item);
        prev_x = x;
        prev_str = &item.str;
    }
    // Items carry their own padding (italic spans especially) — collapse
    // runs so the fountain matches what HTML whitespace-collapsing renders.
    js::trim(&js::collapse_spaces(&text)).to_owned()
}

/// Fountain emphasis as OPEN/CLOSE pairs. A mixed mark is not a palindrome
/// — `**_x_**` closes in the mirror order — so one string per style no
/// longer works. Canonical nesting puts the underscore innermost and the
/// stars outside, which is the order both renderers' regexes already unwrap
/// (triple stars, then double, then single, then underscore).
///
/// Keys are built as b→i→u, so every subset of {b,i,u} appears exactly once.
fn emphasis_mark(style: &str) -> (&'static str, &'static str) {
    match style {
        "b" => ("**", "**"),
        "u" => ("_", "_"),
        "bi" => ("***", "***"),
        "bu" => ("**_", "_**"),
        "iu" => ("*_", "_*"),
        "biu" => ("***_", "_***"),
        _ => ("*", "*"),
    }
}

static HAS_WORD: LazyLock<Regex> = LazyLock::new(|| js::re(r"[\p{L}\p{N}]{2}"));

/// A line's plain text, and its styled form when fonts vary.
struct Joined {
    text: String,
    styled: Option<String>,
}

/// Join a line producing both plain text and (when fonts vary) a styled
/// variant with fountain emphasis markers. Punctuation-only items never
/// carry style — a lone italic comma must not become "*,*".
fn join_line(items: &[TextItem]) -> Joined {
    struct Run<'a> {
        str: &'a str,
        gap_before: bool,
        style: String,
    }
    let mut runs: Vec<Run> = Vec::new();
    let mut prev_end_x = -1.0;
    let mut prev_x = f64::NAN;
    let mut prev_str: &str = "";
    for item in items {
        let x = item.t(4);
        if item.str == prev_str && (x - prev_x).abs() < 2.0 {
            continue;
        }
        let has_word = HAS_WORD.is_match(&item.str);
        let mut style = String::new();
        if has_word {
            if item.bold {
                style.push('b');
            }
            if item.italic {
                style.push('i');
            }
            if item.underline {
                style.push('u');
            }
        }
        runs.push(Run {
            str: &item.str,
            gap_before: prev_end_x >= 0.0 && x - prev_end_x > 5.0,
            style,
        });
        prev_end_x = end_x(item);
        prev_x = x;
        prev_str = &item.str;
    }

    let mut plain = String::new();
    for r in &runs {
        if r.gap_before && !plain.is_empty() {
            plain.push(' ');
        }
        plain.push_str(r.str);
    }
    let plain = js::trim(&js::collapse_spaces(&plain)).to_owned();
    if runs.iter().all(|r| r.style.is_empty()) {
        return Joined {
            text: plain,
            styled: None,
        };
    }

    let mut groups: Vec<(String, String)> = Vec::new();
    for r in &runs {
        let piece = format!("{}{}", if r.gap_before { " " } else { "" }, r.str);
        match groups.last_mut() {
            Some((style, text)) if *style == r.style => text.push_str(&piece),
            _ => groups.push((r.style.clone(), piece)),
        }
    }
    let mut styled = String::new();
    for (style, text) in &groups {
        let clean = js::collapse_spaces(text);
        let core = js::trim(&clean);
        if style.is_empty() || core.is_empty() {
            styled.push_str(&clean);
            continue;
        }
        let (open, close) = emphasis_mark(style);
        let lead_end = clean.len() - clean.trim_start_matches(js::is_ws).len();
        let trail_start = clean.trim_end_matches(js::is_ws).len();
        styled.push_str(&clean[..lead_end]);
        styled.push_str(open);
        styled.push_str(core);
        styled.push_str(close);
        styled.push_str(&clean[trail_start..]);
    }
    let styled = js::trim(&js::collapse_spaces(&styled)).to_owned();
    let styled = (styled != plain).then_some(styled);
    Joined {
        text: plain,
        styled,
    }
}

struct ClusterSplit<'a> {
    left_text: String,
    right_text: String,
    right_items: Vec<&'a TextItem>,
}

/// Split a line's items at the widest internal gap into two columns, when
/// the geometry looks like side-by-side text. Reliable only on short lines
/// (cue pairs) — long body lines close the gap, see the boundary pass.
fn cluster_split<'a>(items: &[&'a TextItem], page_width: f64) -> Option<ClusterSplit<'a>> {
    // Drop whitespace-only items before measuring. pdf.js synthesizes a space
    // item to BRIDGE a wide horizontal jump, and that filler is exactly as
    // wide as the gap it spans: for a dual cue line it arrives as
    //   ["BUNNY" x=158 w=36]  [" " x=194 w=194]  ["CASSIUS" x=389 w=50]
    // so every adjacent gap measures zero, the largest-gap scan finds nothing,
    // and a perfectly good two-column line never anchors a dual region.
    // A whitespace item carries no text and contributes nothing to left_text
    // or right_text, so removing it can only reveal the real gap, never
    // invent one.
    let items: Vec<&TextItem> = items
        .iter()
        .copied()
        .filter(|it| !js::trim(&it.str).is_empty())
        .collect();
    if items.len() < 2 {
        return None;
    }
    let mut gap_idx: Option<usize> = None;
    let mut gap_size = 0.0;
    for i in 1..items.len() {
        let gap = items[i].t(4) - end_x(items[i - 1]);
        if gap > gap_size {
            gap_size = gap;
            gap_idx = Some(i);
        }
    }
    let gap_idx = gap_idx?;
    if gap_size < page_width * 0.12 {
        return None;
    }
    let (left, right) = items.split_at(gap_idx);
    if left[0].t(4) > page_width * 0.42 {
        return None;
    }
    if right[0].t(4) < page_width * 0.48 {
        return None;
    }
    let left_text = join_items(left);
    let right_text = join_items(right);
    if left_text.is_empty() || right_text.is_empty() {
        return None;
    }
    Some(ClusterSplit {
        left_text,
        right_text,
        right_items: right.to_vec(),
    })
}

// Synthetic indents for de-interleaved dual-dialogue lines: standard cue and
// dialogue zones so the classifier treats each column as an ordinary
// sequential speech.
const DUAL_CUE_INDENT: f64 = 40.0;
const DUAL_BODY_INDENT: f64 = 30.0;

/// `text.replace(/^(\d+[A-Z]?\.?)\s+\1$/, '$1')`: a shooting script's scene
/// number printed in both margins of one row, collapsed to one token. The
/// number must be the whole first token (whitespace follows it at once), and
/// the rest of the line exactly that token again after whitespace.
fn collapse_margin_number(text: &str) -> &str {
    static NUMBER: LazyLock<Regex> = LazyLock::new(|| js::re(r"^\d+[A-Z]?\.?$"));
    let Some(ws) = text.find(js::is_ws) else {
        return text;
    };
    let number = &text[..ws];
    if !NUMBER.is_match(number) {
        return text;
    }
    if text[ws..].trim_start_matches(js::is_ws) == number {
        number
    } else {
        text
    }
}

/// Dual dialogue prints two speeches in side-by-side columns. Y-joining
/// would interleave them into garbage, so: detect regions anchored by a
/// dual-cue line (two cue-shaped clusters), collect each column, and emit
/// the left speech followed by the right speech as normal-looking lines.
fn deinterleave_dual_dialogue(lines: &[LineItems], page_width: f64, page_num: u32) -> Vec<RawLine> {
    let mut out = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let all: Vec<&TextItem> = lines[i].items.iter().collect();
        let split = cluster_split(&all, page_width)
            .filter(|s| is_cue_text(&s.left_text) && is_cue_text(&s.right_text));

        let Some(split) = split else {
            // Shooting scripts print the scene number in BOTH margins on the
            // same row — collapse "2   2" / "12A.  12A." to a single token so
            // it classifies and attaches as one scene number.
            let joined = join_line(&lines[i].items);
            let text = collapse_margin_number(&joined.text);
            if !text.is_empty() {
                let indent = js::round((lines[i].items[0].t(4) / page_width) * 100.0);
                let styled = if text == joined.text {
                    joined.styled.clone()
                } else {
                    None
                };
                // Dual-column lines below deliberately carry no `fonts`, and
                // so no fmt: that pass joins its columns to strings without
                // retaining the per-column items (registry #10a, #18's bounds).
                out.push(RawLine {
                    text: text.to_owned(),
                    indent,
                    y: lines[i].y,
                    page_num,
                    styled,
                    dual_right: None,
                    fonts: font_runs(&lines[i].items),
                    fmt: None,
                });
            }
            i += 1;
            continue;
        };

        // Dual region. Body lines are partitioned against a column boundary
        // rather than per-line gaps (long lines close the gap between the
        // columns). Geometry facts that shape this: cues sit ~1.2" deeper than
        // their column's text on BOTH sides, and the left column can start at
        // the action margin — so the boundary starts at rightCueX minus that
        // cue offset and refines to the right column's actual text edge as
        // soon as a body line splits cleanly on its own gap.
        let mut boundary = split.right_items[0].t(4) - page_width * 0.13;
        let mut left_body_min_x: Option<f64> = None;

        let mut ys: Vec<f64> = vec![lines[i].y];
        let mut left: Vec<String> = vec![split.left_text];
        let mut right: Vec<String> = vec![split.right_text];
        i += 1;
        while i < lines.len() {
            // Whitespace-only items dropped for the same reason as in
            // cluster_split: pdf.js's bridging space spans the WHOLE column
            // gap, so it looks like a single item straddling the boundary and
            // would end the region on the first body line. It carries no text,
            // so nothing downstream loses anything (join_items re-inserts
            // spaces from the surviving gaps).
            let items: Vec<&TextItem> = lines[i]
                .items
                .iter()
                .filter(|it| !js::trim(&it.str).is_empty())
                .collect();

            // Refine toward the right column's true text edge — only ever move
            // LEFT: indented lines (parentheticals) start deeper in the column
            // and must not drag the boundary onto normal dialogue lines.
            //
            // THE ORDER HERE IS LOAD-BEARING, and it used to be the other way
            // round. The opening boundary only assumes the cue-to-body offset
            // (13% of the page). A right column whose body starts LEFT of that
            // guess while still running past it looks exactly like a
            // full-width line, so the straddle test below ended the region on
            // its own first body line: both cues fell back to action, and the
            // two simultaneous speeches were concatenated into one sentence. A
            // line that splits cleanly into two columns is by definition not
            // full-width, so the split gets to speak first.
            if let Some(gap_split) = cluster_split(&items, page_width) {
                boundary = boundary.min(gap_split.right_items[0].t(4) - 6.0);
            }

            let straddles = items
                .iter()
                .any(|it| it.t(4) < boundary - 6.0 && end_x(it) > boundary + 6.0);
            if straddles {
                break; // full-width line — region over
            }

            let left_items: Vec<&TextItem> = items
                .iter()
                .copied()
                .filter(|it| it.t(4) < boundary)
                .collect();
            let right_items: Vec<&TextItem> = items
                .iter()
                .copied()
                .filter(|it| it.t(4) >= boundary)
                .collect();
            let left_text = join_items(&left_items);
            let right_text = join_items(&right_items);

            if !left_text.is_empty()
                && !right_text.is_empty()
                && is_cue_text(&left_text)
                && is_cue_text(&right_text)
            {
                break; // next simultaneous exchange — new region anchors here
            }
            if !left_text.is_empty() && right_text.is_empty() && is_cue_text(&left_text) {
                break; // a normal cue or slugline — back to single-column flow
            }
            if !left_text.is_empty()
                && let Some(min_x) = left_body_min_x
                && left_items[0].t(4) < min_x - 24.0
            {
                break; // text left of the established column edge — not column text
            }

            if !left_text.is_empty() {
                let x = left_items[0].t(4);
                left_body_min_x = Some(left_body_min_x.map_or(x, |m| m.min(x)));
                left.push(left_text);
            }
            if !right_text.is_empty() {
                right.push(right_text);
            }
            ys.push(lines[i].y);
            i += 1;
        }

        for (is_right, column) in [(false, &left), (true, &right)] {
            for (k, text) in column.iter().enumerate() {
                if text.is_empty() {
                    continue;
                }
                out.push(RawLine {
                    text: text.clone(),
                    indent: if k == 0 {
                        DUAL_CUE_INDENT
                    } else {
                        DUAL_BODY_INDENT
                    },
                    y: ys[k.min(ys.len() - 1)],
                    page_num,
                    styled: None,
                    // the right column's cue marks the pair for fountain's ^
                    // syntax
                    dual_right: (is_right && k == 0).then_some(true),
                    fonts: None,
                    fmt: None,
                });
            }
        }
    }

    out
}

/// `lines.json`'s `imageStats` and the rest, written as the oracle writes
/// them: for comparing an [`ExtractResult`] with the TypeScript's answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinesDump {
    pub page_count: u32,
    pub spacing_repairs: u32,
    pub lines: Vec<RawLine>,
    pub images: Vec<serde_json::Value>,
    pub image_stats: ImageStats,
}
