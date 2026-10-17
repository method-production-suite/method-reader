//! `cargo xtask make-fixture`: was tools/make-fixture.py. Generates the
//! public-safe test fixtures in tests/fixtures/.
//!
//! ```text
//! cargo xtask make-fixture screenplay tests/fixtures/screenplay.pdf
//! cargo xtask make-fixture prose      tests/fixtures/prose.pdf
//! cargo xtask make-fixture blank      tests/fixtures/blank-pages.pdf
//! cargo xtask make-fixture torture    tests/fixtures/torture.pdf
//! cargo xtask make-fixture demo       tests/fixtures/field-station.pdf
//! cargo xtask make-fixture images     tests/fixtures/images.pdf
//! cargo xtask make-fixture jpx        tests/fixtures/jpx.pdf
//! ```
//!
//! Real scripts are confidential, so the committed fixtures are invented.
//! The first three kinds cover the three paths convert.ts can take: a
//! screenplay that converts, a document that trips the not-a-screenplay
//! guard, and pages with no text layer that trip the scanned guard. torture
//! exercises every content-driven registry behavior (torture_content.rs),
//! and demo is Field Station, the invented feature the README and site
//! pictures show (field_station_content.rs). `images` carries pictures, one
//! for each rule that decides whether a picture is art or furniture, and
//! `jpx` carries a single JPEG2000 one, the format pdf.js can only decode
//! with a decoder loaded from outside its bundle.
//!
//! Geometry from docs/screenplay-format-reference.md: 12pt Courier, 10
//! chars per inch, 6 lines per inch. Indents matter: Method Reader classifies
//! elements by horizontal position, so a demo PDF with sloppy geometry would
//! parse wrong and prove nothing.
//!
//! Every committed fixture but torture.pdf is pinned byte for byte by
//! tests/fixture.rs, which is why this is a line-for-line port of the
//! Python, down to its textwrap (textwrap.rs) and its number formatting.
//! Errors print what the Python printed and exit 1, as an uncaught Python
//! exception or `sys.exit(message)` did.

pub mod field_station_content;
pub mod pictures;
pub mod py;
pub mod textwrap;
pub mod torture_content;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

pub use pictures::*;
use py::Py;
use torture_content::Item;

/// What a run printed and how it ended, so tests can read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub code: u8,
    pub stdout: String,
    pub stderr: String,
}

/// A failure, as the stderr text the Python printed for it.
type Fail = String;

pub const PT: f64 = 72.0;
pub const PAGE_W: f64 = 8.5 * PT;
pub const PAGE_H: f64 = 11.0 * PT;
/// 6 lines/inch.
pub const LINE: f64 = 12.0;
/// 1" top margin.
pub const TOP: f64 = PAGE_H - 1.0 * PT;
pub const BOTTOM: f64 = 1.0 * PT;
pub const LINES_PER_PAGE: usize = 55;

/// Base-14 Type1 fonts, so nothing is embedded and the generator stays a
/// pure string builder. Verified against pdf.js: it reports these
/// PostScript names through commonObjs, which is what src/parser/extract.ts
/// regexes for /bold|black|heavy/ and /italic|oblique/.
pub const FONTS: &[(&str, &str)] = &[
    ("F1", "Courier"),
    ("F2", "Courier-Bold"),
    ("F3", "Courier-Oblique"),
    ("F4", "Courier-BoldOblique"),
    ("F5", "Helvetica"),
];

/// 12pt Courier advance: 0.1" at 10 chars/inch.
pub const CHAR_W: f64 = 7.2;

/// Inches from the left edge, per kind.
///
/// Transitions are RIGHT-flush in a real script, and the classifier keys on
/// that: TRANSITION_MIN is indent 55, so a left-flush "CUT TO:" is action,
/// not a transition. At 1.5" neither fixture had ever produced a single
/// transition element while appearing to contain four.
pub const X: &[(&str, f64)] = &[
    ("scene", 1.5),
    ("action", 1.5),
    ("dialogue", 2.5),
    ("paren", 3.1),
    ("character", 3.7),
    ("trans", 6.0),
    ("pgnum", 7.4),
    ("mini", 1.5),
];

/// Wrap width in characters, per kind. Cues never wrap in practice.
pub const WRAP: &[(&str, usize)] = &[
    ("scene", 60),
    ("action", 60),
    ("dialogue", 35),
    ("paren", 24),
    ("trans", 60),
    ("character", 38),
    ("mini", 60),
];

fn x_of(kind: &str) -> f64 {
    X.iter()
        .find(|(k, _)| *k == kind)
        .unwrap_or_else(|| panic!("KeyError: {}", py::repr(kind)))
        .1
}

fn wrap_of(kind: &str) -> usize {
    WRAP.iter()
        .find(|(k, _)| *k == kind)
        .unwrap_or_else(|| panic!("KeyError: {}", py::repr(kind)))
        .1
}

/// The screenplay kind's script: (kind, text).
pub const S: &[(&str, &str)] = &[
    ("scene", "INT. THE LAST VIDEO STORE - NIGHT"),
    (
        "action",
        "Fluorescent light hums over aisles of VHS tapes. Rain \
         streaks the front window. A hand-lettered sign reads: \
         EVERYTHING MUST GO.",
    ),
    (
        "action",
        "MARGO SHAW, 60s, cardigan with a pen through the pocket, \
         alphabetizes a shelf that nobody will browse.",
    ),
    ("action", "The bell over the door RINGS."),
    ("character", "MARGO"),
    ("dialogue", "We close at ten."),
    ("character", "DEV"),
    ("paren", "(O.S.)"),
    ("dialogue", "It's nine forty."),
    (
        "action",
        "DEV OKONKWO, 20s, soaked through, shakes off a jacket that \
         was never waterproof.",
    ),
    ("character", "MARGO"),
    ("dialogue", "And yet."),
    ("character", "DEV"),
    (
        "dialogue",
        "I need something for a fourteen-year-old who thinks \
         movies started with streaming.",
    ),
    ("action", "Margo looks up for the first time."),
    ("character", "MARGO"),
    ("paren", "(a small conversion)"),
    ("dialogue", "Sit down."),
    ("trans", "CUT TO:"),
    (
        "scene",
        "INT. THE LAST VIDEO STORE - HORROR AISLE - CONTINUOUS",
    ),
    (
        "action",
        "Margo moves like she's done this ten thousand times, \
         because she has. Dev trails her, dripping.",
    ),
    ("character", "MARGO"),
    ("dialogue", "Fourteen. Scared, or pretending not to be?"),
    ("character", "DEV"),
    ("dialogue", "Pretending. Loudly."),
    ("character", "MARGO"),
    (
        "dialogue",
        "Then not the one with the mask. Everyone starts with the \
         mask and nobody remembers it.",
    ),
    (
        "action",
        "She pulls a battered clamshell case and holds it like \
         evidence.",
    ),
    ("character", "MARGO (CONT'D)"),
    (
        "dialogue",
        "This one. Nothing happens for forty minutes and then you \
         never sleep facing a closet again.",
    ),
    ("character", "DEV"),
    ("dialogue", "That's a pitch?"),
    ("character", "MARGO"),
    ("dialogue", "That's a promise."),
    ("trans", "DISSOLVE TO:"),
    ("scene", "EXT. STRIP MALL - NIGHT"),
    (
        "action",
        "Rain. The store's neon sign flickers between LAST and AST. \
         Behind the glass, two silhouettes argue happily about \
         something that doesn't matter.",
    ),
    ("character", "DEV"),
    ("paren", "(O.S.)"),
    ("dialogue", "How long have you been here?"),
    ("character", "MARGO"),
    ("paren", "(O.S.)"),
    ("dialogue", "Since people still rewound."),
    ("scene", "INT. THE LAST VIDEO STORE - COUNTER - LATER"),
    (
        "action",
        "A shoebox of index cards. Margo writes on one in careful \
         block letters and slides it across.",
    ),
    ("character", "DEV"),
    ("dialogue", "What's this?"),
    ("character", "MARGO"),
    (
        "dialogue",
        "Your card. Nineteen years running, four hundred and six \
         members. You're four hundred and seven.",
    ),
    ("character", "DEV"),
    ("dialogue", "You're closing in a week."),
    ("character", "MARGO"),
    ("dialogue", "Then it'll be a short membership."),
    (
        "action",
        "Dev takes the card anyway. Pockets it like it matters, \
         because it does.",
    ),
    ("character", "DEV"),
    ("dialogue", "What happens to all of it?"),
    (
        "action",
        "Margo looks down the aisles. Twenty thousand tapes. A \
         lifetime of other people's Friday nights.",
    ),
    ("character", "MARGO"),
    (
        "dialogue",
        "Somebody buys the shelves. The rest goes where the rest \
         goes.",
    ),
    ("character", "DEV"),
    ("dialogue", "That's grim."),
    ("character", "MARGO"),
    ("paren", "(not grim at all)"),
    ("dialogue", "It's Tuesday."),
    ("trans", "SMASH CUT TO:"),
    ("scene", "INT. DEV'S APARTMENT - NIGHT"),
    (
        "action",
        "A television old enough to have a dial. Dev threads the \
         tape in. Static, then a countdown leader \u{2014} 5, 4, 3.",
    ),
    (
        "action",
        "His NIECE, 14, arms folded, sits as far from the screen as \
         the room allows.",
    ),
    ("character", "NIECE"),
    ("dialogue", "This is going to be so boring."),
    ("character", "DEV"),
    ("dialogue", "Forty minutes. Give it forty minutes."),
    ("action", "The countdown hits 1. The room goes dark blue."),
    (
        "action",
        "Forty minutes later, she has not moved, and she is sitting \
         considerably closer.",
    ),
    ("trans", "FADE OUT."),
];

pub const TITLE: &[(f64, &str)] = &[
    (4.0, "THE LAST VIDEO STORE"),
    (4.5, "Written by"),
    (5.0, "A. N. Placeholder"),
];

/// Courier screenplays use -- and straight quotes by convention, and
/// latin-1 (the PDF string encoding here) has no em dash anyway.
pub const ASCII_MAP: &[(char, &str)] = &[
    ('\u{2014}', "--"),
    ('\u{2013}', "-"),
    ('\u{2019}', "'"),
    ('\u{2018}', "'"),
    ('\u{201c}', "\""),
    ('\u{201d}', "\""),
    ('\u{2026}', "..."),
];

/// `s.translate(ASCII_MAP)`.
pub fn ascii(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match ASCII_MAP.iter().find(|(from, _)| *from == c) {
            Some((_, to)) => out.push_str(to),
            None => out.push(c),
        }
    }
    out
}

/// A PDF string literal's body: straightened, with \ ( ) escaped.
pub fn esc(s: &str) -> String {
    ascii(s)
        .replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

/// Python's `f"{v:.2f}"`.
fn f2(v: f64) -> String {
    format!("{v:.2}")
}

/// Python's `f"{v:g}"`, for the whole point sizes used here.
fn g(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

// --- inline markup, for the torture kind -----------------------------------

/// b/i are font STYLES; u/k/r/w are DRAWN rules (one real, three decoys);
/// f/t/z/g are block font SHIFTS, which registry 18 reads back out of the
/// PDF. The markup is `{x}` to open and `{/x}` to close.
pub const MARKUP_STYLES: &str = "biukrwftzg";

/// (start, end, style): a styled span, as offsets into the PLAIN text.
pub type Span = (usize, usize, char);

/// `'a {b}bold{/b} c'` -> `('a bold c', [(2, 6, 'b')])`.
///
/// Offsets index the PLAIN text (in characters), so wrapping can slice by
/// character and re-derive styles without ever seeing a marker. Unbalanced
/// markers fail: a typo buried in pages of content must fail at generation
/// rather than produce a fixture that quietly tests nothing.
pub fn parse_markup(s: &str) -> Result<(String, Vec<Span>), Fail> {
    let chars: Vec<char> = s.chars().collect();
    let mut plain = String::new();
    let mut spans = Vec::new();
    let mut open_at: BTreeMap<char, usize> = BTreeMap::new();
    let mut out_len = 0usize;
    let mut i = 0usize;
    while i < chars.len() {
        if let Some((closing, style, end)) = marker_at(&chars, i) {
            if closing {
                let Some(start) = open_at.remove(&style) else {
                    return Err(format!(
                        "ValueError: unmatched {{/{style}}} in {}",
                        py::repr(s)
                    ));
                };
                spans.push((start, out_len, style));
            } else {
                if open_at.contains_key(&style) {
                    return Err(format!("ValueError: nested {{{style}}} in {}", py::repr(s)));
                }
                open_at.insert(style, out_len);
            }
            i = end;
        } else {
            plain.push(chars[i]);
            out_len += 1;
            i += 1;
        }
    }
    if !open_at.is_empty() {
        let names: Vec<String> = open_at.keys().map(|c| py::repr(&c.to_string())).collect();
        return Err(format!(
            "ValueError: unclosed [{}] in {}",
            names.join(", "),
            py::repr(s)
        ));
    }
    Ok((plain, spans))
}

/// A marker `{x}` or `{/x}` starting at `i`: (closing, style, index after).
fn marker_at(chars: &[char], i: usize) -> Option<(bool, char, usize)> {
    if chars.get(i) != Some(&'{') {
        return None;
    }
    let closing = chars.get(i + 1) == Some(&'/');
    let at = i + 1 + usize::from(closing);
    let style = *chars.get(at)?;
    if !MARKUP_STYLES.contains(style) || chars.get(at + 1) != Some(&'}') {
        return None;
    }
    Some((closing, style, at + 2))
}

/// One styled run of a laid-out line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    /// Sorted.
    pub styles: Vec<char>,
    pub text: String,
}

impl Run {
    fn to_py(&self) -> Py {
        Py::Dict(vec![
            (
                "styles".into(),
                Py::List(self.styles.iter().map(|c| Py::Str(c.to_string())).collect()),
            ),
            ("text".into(), Py::Str(self.text.clone())),
        ])
    }
}

/// (start, end) for each space-delimited word, as offsets into plain.
fn words(plain: &[char]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let (mut i, n) = (0usize, plain.len());
    while i < n {
        while i < n && plain[i] == ' ' {
            i += 1;
        }
        if i >= n {
            break;
        }
        let mut j = i;
        while j < n && plain[j] != ' ' {
            j += 1;
        }
        out.push((i, j));
        i = j;
    }
    out
}

/// Slice [start, end) into styled runs, merging equal-styled neighbours.
fn runs(plain: &[char], spans: &[Span], start: usize, end: usize) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    for (i, &c) in plain.iter().enumerate().take(end).skip(start) {
        let mut styles: Vec<char> = spans
            .iter()
            .filter(|(a, b, _)| *a <= i && i < *b)
            .map(|s| s.2)
            .collect();
        styles.sort_unstable();
        match out.last_mut() {
            Some(last) if last.styles == styles => last.text.push(c),
            _ => out.push(Run {
                styles,
                text: c.to_string(),
            }),
        }
    }
    out
}

/// Greedy word wrap that keeps style spans intact across line breaks: a
/// list of lines, each a list of runs.
///
/// Deliberately NOT textwrap. textwrap takes a plain string, so markup has
/// to be either stripped (losing the styles) or left in (counting marker
/// characters as text width, and splitting a marker across a line). The
/// three original fixture kinds keep calling textwrap, untouched, which is
/// what lets the byte-stability guard stay meaningful.
pub fn wrap_spans(plain: &str, spans: &[Span], width: i64) -> Vec<Vec<Run>> {
    let plain: Vec<char> = plain.chars().collect();
    let words = words(&plain);
    let Some(&(mut line_start, mut line_end)) = words.first() else {
        return vec![vec![Run {
            styles: vec![],
            text: String::new(),
        }]];
    };
    let mut lines = Vec::new();
    for &(ws, we) in &words[1..] {
        if (we - line_start) as i64 <= width {
            line_end = we;
        } else {
            lines.push(runs(&plain, spans, line_start, line_end));
            (line_start, line_end) = (ws, we);
        }
    }
    lines.push(runs(&plain, spans, line_start, line_end));
    lines
}

/// A plain row: (x inches, text), or None for a blank line.
pub type Row = Option<(f64, String)>;

/// The screenplay kind: a list of pages, each a list of rows.
pub fn layout() -> Vec<Vec<Row>> {
    let mut flowed: Vec<(&str, String)> = Vec::new();
    for (kind, text) in S {
        let mut chunks = textwrap::wrap(text, wrap_of(kind));
        if chunks.is_empty() {
            chunks.push(String::new());
        }
        for chunk in chunks {
            flowed.push((kind, chunk));
        }
        flowed.push(("blank", String::new()));
    }

    let mut pages = Vec::new();
    let mut cur: Vec<Row> = Vec::new();
    let mut n = 0usize;
    for (kind, text) in flowed {
        if kind == "blank" {
            if !cur.is_empty() {
                cur.push(None);
                n += 1;
            }
        } else {
            cur.push(Some((x_of(kind), text)));
            n += 1;
        }
        if n >= LINES_PER_PAGE {
            pages.push(std::mem::take(&mut cur));
            n = 0;
        }
    }
    if !cur.is_empty() {
        pages.push(cur);
    }
    pages
}

/// Styles that are DRAWN rather than selected. They never change which font
/// a run uses, which mirrors how PDFs actually carry underline and is why
/// the parser cannot see it from font data alone (registry 9d).
pub const DRAWN: &[char] = &['u', 'k', 'r', 'w'];

/// Block font shifts (registry 18). f picks the second FACE; t/z/g pick a
/// SIZE, chosen to land one on each side of every threshold in that entry:
///   10/12 = 0.83 -> -1     15/12 = 1.25 -> +1     18/12 = 1.50 -> +2
pub const SHIFT_SIZE: &[(char, f64)] = &[('t', 10.0), ('z', 15.0), ('g', 18.0)];
pub const SHIFTS: &[char] = &['f', 't', 'z', 'g'];

pub fn size_for(styles: &[char]) -> f64 {
    for s in styles {
        if let Some((_, size)) = SHIFT_SIZE.iter().find(|(c, _)| c == s) {
            return *size;
        }
    }
    12.0
}

/// Style set -> font resource key.
pub fn font_for(styles: &[char]) -> &'static str {
    if styles.contains(&'f') {
        return "F5"; // a family shift, not a weight or slope
    }
    let s: BTreeSet<char> = styles
        .iter()
        .copied()
        .filter(|c| !DRAWN.contains(c) && !SHIFTS.contains(c))
        .collect();
    let has = |c: char| s.contains(&c);
    match (has('b'), has('i'), s.len()) {
        (true, true, 2) => "F4",
        (true, false, 1) => "F2",
        (false, true, 1) => "F3",
        _ => "F1",
    }
}

/// (text operators, drawn rectangles) for one laid-out line.
///
/// Four rule shapes, one real and three decoys. Each decoy is rejected by a
/// DIFFERENT filter in collectUnderlineMarks/markUnderlinesItem, so a
/// loosened threshold shows up as a stray underscore in the emitted
/// Fountain:
///
/// ```text
///   u  real underline   0.6pt tall, run-width, 2.0pt under the baseline
///   k  strikethrough    same bar at baseline + 3.5 (mid x-height) -> the
///                       y-window rejects anything above the baseline
///   r  table border     same bar at baseline - 6.0 -> below the window, and
///                       6pt ABOVE the next row's baseline, so it is out of
///                       that row's window too
///   w  page-wide rule   0.5in margins (540pt = 88% of the page) at the real
///                       underline offset -> only the furniture-width filter
///                       saves it, over real text with 100% overlap
/// ```
pub fn styled_row_ops(x_in: f64, y: f64, runs: &[Run]) -> (Vec<String>, Vec<String>) {
    let (mut ops, mut rects) = (Vec::new(), Vec::new());
    let mut x = x_in * PT;
    for run in runs {
        let (text, styles) = (&run.text, &run.styles);
        let len = text.chars().count() as f64;
        if !text.is_empty() {
            ops.push(format!("/{} {} Tf", font_for(styles), g(size_for(styles))));
            ops.push(format!("1 0 0 1 {} {} Tm", f2(x), f2(y)));
            ops.push(format!("({}) Tj", esc(text)));
            let w = len * CHAR_W;
            // A filled rectangle, not a stroked line. Real generators
            // stroke; filling here proves the detector accepts both paint
            // ops.
            if styles.contains(&'u') {
                rects.push(format!("0 g {} {} {} 0.6 re f", f2(x), f2(y - 2.0), f2(w)));
            }
            if styles.contains(&'k') {
                rects.push(format!("0 g {} {} {} 0.6 re f", f2(x), f2(y + 3.5), f2(w)));
            }
            if styles.contains(&'r') {
                rects.push(format!("0 g {} {} {} 0.6 re f", f2(x), f2(y - 6.0), f2(w)));
            }
            if styles.contains(&'w') {
                rects.push(format!(
                    "0 g {} {} {} 0.6 re f",
                    f2(0.5 * PT),
                    f2(y - 2.0),
                    f2(PAGE_W - 1.0 * PT)
                ));
            }
        }
        x += len * CHAR_W;
    }
    (ops, rects)
}

/// Dual dialogue column geometry. The parser anchors a dual region on a
/// line holding TWO cue-shaped clusters, then partitions body lines by
/// start-x (registry 10a: the boundary starts at rightCueX - 13% and refines
/// only leftward). These indents put the right cue far enough right that the
/// learned boundary lands cleanly between the columns.
pub const DUAL_X_LEFT: &[(&str, f64)] = &[("character", 2.2), ("dialogue", 1.9), ("paren", 2.4)];
pub const DUAL_X_RIGHT: &[(&str, f64)] = &[("character", 5.4), ("dialogue", 5.1), ("paren", 5.6)];
pub const DUAL_WRAP: i64 = 26;

/// A torture row: one line, or two columns sharing a Y.
#[derive(Debug, Clone, PartialEq)]
pub enum TortureRow {
    One(f64, Vec<Run>),
    Multi(Vec<(f64, Vec<Run>)>),
}

/// A torture page: rows, None for a blank line.
pub type TorturePage = Vec<Option<TortureRow>>;

/// Two (kind, text) lists -> rows where both columns share a Y, and how
/// many lines they use.
///
/// The cue lines of both columns MUST land on one Y, or the parser sees two
/// ordinary consecutive cues rather than a dual region.
fn dual_rows(
    left: &[(&'static str, String)],
    right: &[(&'static str, String)],
) -> Result<(Vec<Option<TortureRow>>, usize), Fail> {
    fn col(
        items: &[(&'static str, String)],
        xs: &[(&str, f64)],
    ) -> Result<Vec<(f64, Vec<Run>)>, Fail> {
        let mut out = Vec::new();
        for (kind, text) in items {
            let (plain, spans) = parse_markup(text)?;
            let x = xs
                .iter()
                .find(|(k, _)| k == kind)
                .unwrap_or_else(|| panic!("KeyError: {}", py::repr(kind)))
                .1;
            for runs in wrap_spans(&plain, &spans, DUAL_WRAP) {
                out.push((x, runs));
            }
        }
        Ok(out)
    }
    let l = col(left, DUAL_X_LEFT)?;
    let r = col(right, DUAL_X_RIGHT)?;
    let mut rows = Vec::new();
    for i in 0..l.len().max(r.len()) {
        let mut pair = Vec::new();
        if let Some(c) = l.get(i) {
            pair.push(c.clone());
        }
        if let Some(c) = r.get(i) {
            pair.push(c.clone());
        }
        rows.push(Some(TortureRow::Multi(pair)));
    }
    rows.push(None);
    let used = rows.len();
    Ok((rows, used))
}

/// Lay torture content out into pages, honouring its directives.
pub fn flow_torture(content: &[Item]) -> Result<Vec<TorturePage>, Fail> {
    let mut pages: Vec<TorturePage> = Vec::new();
    let mut cur: TorturePage = Vec::new();
    let mut n = 0usize;

    fn flush(pages: &mut Vec<TorturePage>, cur: &mut TorturePage, n: &mut usize) {
        if !cur.is_empty() {
            pages.push(std::mem::take(cur));
        }
        cur.clear();
        *n = 0;
    }

    for item in content {
        match item {
            Item::PageBreak => flush(&mut pages, &mut cur, &mut n),
            Item::AtLine(target) => {
                if n > *target {
                    // Failing rather than overflowing is deliberate.
                    // Silently sliding past the anchor would turn the (MORE)
                    // test into a test of nothing the first time content
                    // above it grew by a sentence, and nothing would report
                    // it.
                    return Err(format!(
                        "atline: page {} is already at line {n}, cannot pad back to {target}. \
                         Content above it grew: shorten it, or move the anchor.",
                        pages.len() + 1
                    ));
                }
                while n < *target {
                    cur.push(None);
                    n += 1;
                }
            }
            Item::Dual(left, right) => {
                let (rows, used) = dual_rows(left, right)?;
                if n + used > LINES_PER_PAGE {
                    flush(&mut pages, &mut cur, &mut n);
                }
                cur.extend(rows);
                n += used;
            }
            Item::El(kind, text) => {
                let (plain, spans) = parse_markup(text)?;
                for runs in wrap_spans(&plain, &spans, wrap_of(kind) as i64) {
                    if n >= LINES_PER_PAGE {
                        flush(&mut pages, &mut cur, &mut n);
                    }
                    cur.push(Some(TortureRow::One(x_of(kind), runs)));
                    n += 1;
                }
                // A cue and a parenthetical sit DIRECTLY above what they
                // introduce: real scripts print no blank line there, and the
                // geometry is the whole point of this fixture. Everything
                // else gets its blank.
                if !matches!(*kind, "character" | "paren") && n < LINES_PER_PAGE {
                    cur.push(None);
                    n += 1;
                }
            }
        }
    }
    flush(&mut pages, &mut cur, &mut n);
    Ok(pages)
}

fn page_number_ops(page_no: usize) -> [String; 2] {
    [
        format!(
            "1 0 0 1 {} {} Tm",
            f2(x_of("pgnum") * PT),
            f2(PAGE_H - 0.5 * PT)
        ),
        format!("({page_no}.) Tj"),
    ]
}

/// Like content_stream, but rows carry styled runs and may hold two columns
/// sharing one Y.
pub fn torture_content_stream(rows: &[Option<TortureRow>], page_no: Option<usize>) -> String {
    let mut text_ops = vec!["BT".to_string(), "/F1 12 Tf".to_string()];
    let mut rects = Vec::new();
    if let Some(no) = page_no {
        text_ops.extend(page_number_ops(no));
    }
    let mut y = TOP;
    for row in rows {
        match row {
            // KNOWN GAP: these two columns are emitted at correct x
            // positions on a shared Y, but the parser still joins them into
            // "BUNNY CASSIUS" rather than anchoring a dual region.
            // tests/torture.test.ts records the current behavior.
            Some(TortureRow::Multi(pairs)) => {
                for (x, runs) in pairs {
                    let (ops, rs) = styled_row_ops(*x, y, runs);
                    text_ops.extend(ops);
                    rects.extend(rs);
                }
            }
            Some(TortureRow::One(x, runs)) => {
                let (ops, rs) = styled_row_ops(*x, y, runs);
                text_ops.extend(ops);
                rects.extend(rs);
            }
            None => {}
        }
        y -= LINE;
    }
    text_ops.push("ET".into());
    rects.extend(text_ops);
    rects.join("\n")
}

/// A page's text as drawn: page number top right when given.
pub fn content_stream(rows: &[Row], page_no: Option<usize>) -> String {
    let mut out = vec!["BT".to_string(), "/F1 12 Tf".to_string()];
    if let Some(no) = page_no {
        out.extend(page_number_ops(no));
    }
    let mut y = TOP;
    for row in rows {
        if let Some((x, text)) = row {
            out.push(format!("1 0 0 1 {} {} Tm", f2(x * PT), f2(y)));
            out.push(format!("({}) Tj", esc(text)));
        }
        y -= LINE;
    }
    out.push("ET".into());
    out.join("\n")
}

/// A title page, centered rows plus the contact block.
pub fn title_stream(table: &[(f64, &str)]) -> String {
    let mut out = vec!["BT".to_string(), "/F1 12 Tf".to_string()];
    for (inches_down, text) in table {
        let x = (8.5 - text.chars().count() as f64 / 10.0) / 2.0; // 10 chars/inch, centered
        let y = PAGE_H - inches_down * PT;
        out.push(format!("1 0 0 1 {} {} Tm", f2(x * PT), f2(y)));
        out.push(format!("({}) Tj", esc(text)));
    }
    out.push(format!("1 0 0 1 {} {} Tm", f2(1.5 * PT), f2(1.5 * PT)));
    out.push("(Placeholder Pictures) Tj".into());
    out.push(format!(
        "1 0 0 1 {} {} Tm",
        f2(1.5 * PT),
        f2(1.5 * PT - LINE)
    ));
    out.push("(hello@example.com) Tj".into());
    out.push("ET".into());
    out.join("\n")
}

/// A document that is emphatically not a screenplay: one left margin, no
/// sluglines, nothing sitting in the cue/dialogue indent bands. Parses to
/// zero scenes and zero dialogue, which is what NotAScreenplayError keys on.
/// The ALL-CAPS headings are deliberate: at the body margin they must read
/// as action, not as character cues.
pub const PROSE: &[&str] = &[
    "THE LAST VIDEO STORE",
    "",
    "A proposal for a documentary feature, prepared for financiers who \
     have never rewound anything.",
    "",
    "THE OPPORTUNITY",
    "",
    "There were once nine thousand video rental stores in this country. \
     There are now fewer than a dozen. Each one that closes takes with it \
     a set of recommendations that existed nowhere else, held by a person \
     who never wrote any of it down.",
    "",
    "We propose to film the last year of one of them.",
    "",
    "AUDIENCE",
    "",
    "The film is for the generation that grew up inside these rooms and \
     for the one that has only ever been handed an algorithm. It argues, \
     gently, that the difference between the two is a person.",
    "",
    "BUDGET",
    "",
    "A crew of four, ninety shooting days, and the patience to wait for a \
     Tuesday when nothing happens. The full schedule follows overleaf, \
     along with the letters of access we have already secured.",
    "",
    "This page exists to be rejected by the screenplay guard, and its \
     prose is padded to make sure the extractor sees a healthy text \
     layer while the parser finds nothing screenplay-shaped in it.",
];

pub fn prose_stream() -> String {
    let mut out = vec!["BT".to_string(), "/F1 12 Tf".to_string()];
    let mut y = TOP;
    for para in PROSE {
        let mut chunks = textwrap::wrap(para, 72);
        if chunks.is_empty() {
            chunks.push(String::new());
        }
        for chunk in chunks {
            if !chunk.is_empty() {
                out.push(format!("1 0 0 1 {} {} Tm", f2(1.0 * PT), f2(y)));
                out.push(format!("({}) Tj", esc(&chunk)));
            }
            y -= LINE;
        }
    }
    out.push("ET".into());
    out.join("\n")
}

/// A grey box and no text operators at all: the shape of a page scanned to
/// an image. extractDocument finds no lines, so the scanned guard fires.
pub fn blank_stream() -> String {
    format!(
        "0.85 g {} {} {} {} re f",
        f2(1.0 * PT),
        f2(1.0 * PT),
        f2(PAGE_W - 2.0 * PT),
        f2(PAGE_H - 2.0 * PT)
    )
}

// --- the PDF ---------------------------------------------------------------

/// A string as latin-1 bytes, the PDF string encoding here.
fn latin1(s: &str) -> Result<Vec<u8>, Fail> {
    s.chars()
        .map(|c| {
            u8::try_from(u32::from(c)).map_err(|_| {
                format!(
                    "UnicodeEncodeError: 'latin-1' codec can't encode character '\\u{:04x}'",
                    u32::from(c)
                )
            })
        })
        .collect()
}

/// One object's body: text (latin-1) or binary.
enum Body {
    Text(String),
    Bytes(Vec<u8>),
}

/// name -> XObject body, in object order.
pub type XObjects = Vec<(String, Vec<u8>)>;

/// The picture names each page draws, in page order.
pub type PageXObjects = Vec<Vec<&'static str>>;

/// The PDF bytes for these page streams.
///
/// Objects: 1 catalog, 2 pages, then one per font, then one per image
/// XObject, then page/content pairs. Every kind carries every font even
/// when it uses one, so the resource dict is identical across kinds and
/// there is only one object-numbering scheme to reason about. Images are
/// the exception: which sheets list a picture is exactly what the furniture
/// rule reads, so each page names only the ones it draws.
pub fn build(
    streams: &[String],
    xobjects: &XObjects,
    page_xobjects: Option<&[Vec<&str>]>,
) -> Result<Vec<u8>, Fail> {
    let n_pages = streams.len();
    // Two guards, for the two ways of leaving pages unaware of the images
    // they were handed. The failure they prevent is INVISIBLE: a page that
    // does not name an image still generates a valid PDF, every Do is a
    // no-op against a missing resource, and you get the right page count,
    // the right text, no pictures, and a byte-stability check that passes
    // because it compares against whatever was committed last.
    //
    // What they do NOT catch: a page_xobjects list of the right length that
    // simply never names some image, or a content stream whose Do names an
    // image its page's list omitted. Keeping those two consistent is the
    // caller's job, which is why every list here is built from the same
    // placement tables the content streams are drawn from.
    if !xobjects.is_empty() && page_xobjects.is_none() {
        return Err("build: xobjects given with no page_xobjects; no \
                    page would name them and every Do would be a no-op"
            .into());
    }
    if let Some(lists) = page_xobjects
        && lists.len() != n_pages
    {
        return Err(format!(
            "build: {} page_xobjects entries for {n_pages} streams. Pages are walked \
                 by stream index, so a longer list loses its tail without a word and a \
                 shorter one dies on an IndexError that names neither list.",
            lists.len()
        ));
    }

    let first_image_obj = 3 + FONTS.len();
    let image_obj = |name: &str| -> usize {
        let i = xobjects
            .iter()
            .position(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("KeyError: {}", py::repr(name)));
        first_image_obj + i
    };
    let first_page_obj = first_image_obj + xobjects.len();
    let kids: Vec<String> = (0..n_pages)
        .map(|i| format!("{} 0 R", first_page_obj + 2 * i))
        .collect();
    let mut objs: Vec<Body> = vec![
        Body::Text("<< /Type /Catalog /Pages 2 0 R >>".into()),
        Body::Text(format!(
            "<< /Type /Pages /Kids [{}] /Count {n_pages} >>",
            kids.join(" ")
        )),
    ];
    for (_, base) in FONTS {
        objs.push(Body::Text(format!(
            "<< /Type /Font /Subtype /Type1 /BaseFont /{base} >>"
        )));
    }
    for (_, body) in xobjects {
        objs.push(Body::Bytes(body.clone()));
    }
    let res: Vec<String> = FONTS
        .iter()
        .enumerate()
        .map(|(i, (key, _))| format!("/{key} {} 0 R", 3 + i))
        .collect();
    let res = res.join(" ");
    for (i, s) in streams.iter().enumerate() {
        let drawn: &[&str] = page_xobjects.map_or(&[], |lists| lists[i].as_slice());
        let xo = if drawn.is_empty() {
            String::new()
        } else {
            let names: Vec<String> = drawn
                .iter()
                .map(|d| format!("/{d} {} 0 R", image_obj(d)))
                .collect();
            format!(" /XObject << {} >>", names.join(" "))
        };
        let contents = objs.len() + 2;
        objs.push(Body::Text(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W:.0} {PAGE_H:.0}] \
             /Resources << /Font << {res} >>{xo} >> /Contents {contents} 0 R >>"
        )));
        objs.push(Body::Text(format!(
            "<< /Length {} >>\nstream\n{s}\nendstream",
            s.chars().count()
        )));
    }

    // Everything but an image body is latin-1 text, one byte per
    // character, so the offsets the xref records are character counts too.
    let mut buf: Vec<u8> = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objs.iter().enumerate() {
        offsets.push(buf.len());
        buf.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        match body {
            Body::Text(text) => buf.extend(latin1(text)?),
            Body::Bytes(bytes) => buf.extend_from_slice(bytes),
        }
        buf.extend_from_slice(b"\nendobj\n");
    }
    let xref = buf.len();
    buf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for off in offsets {
        buf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    buf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objs.len() + 1
        )
        .as_bytes(),
    );
    Ok(buf)
}

fn torture_streams() -> Result<Vec<String>, Fail> {
    let pages = flow_torture(&torture_content::content())?;
    let mut streams = vec![title_stream(torture_content::TITLE)];
    for (i, page) in pages.iter().enumerate() {
        streams.push(torture_content_stream(page, Some(i + 1)));
    }
    let placements = torture_placements()?;
    Ok(placements
        .iter()
        .zip(streams)
        .map(|(sheet, s)| {
            let mut out: String = sheet.iter().map(place).collect();
            out.push_str(&s);
            out
        })
        .collect())
}

// --- demo: the invented feature the README and site pictures show ----------
// Laid out PAGE BY PAGE, not flowed: the site's scene has to land on printed
// pages 14 to 18, where the site's own page markers say it is. So each page
// is drawn exactly as field_station_content.rs writes it, and a page that
// does not fit is an error rather than a silent reflow that would move every
// page number after it.

/// A real script puts no blank line between a character name and what that
/// character says, so neither does this. Everything else gets one above it.
const JOINED_TO_CUE: &[&str] = &["paren", "dialogue"];

/// One page of (kind, text) rows -> the rows content_stream draws. Fails if
/// the page overflows.
pub fn flow_page(rows: &[(&str, &str)]) -> Result<Vec<Row>, Fail> {
    let mut out: Vec<Row> = Vec::new();
    for (kind, text) in rows {
        if !out.is_empty() && !JOINED_TO_CUE.contains(kind) {
            out.push(None);
        }
        // Straighten BEFORE wrapping, so the wrap measures what is drawn: an
        // em dash is drawn as two characters and would otherwise count as
        // one.
        let text = ascii(text);
        let mut chunks = textwrap::wrap(&text, wrap_of(kind));
        if chunks.is_empty() {
            chunks.push(String::new());
        }
        for chunk in chunks {
            out.push(Some((x_of(kind), chunk)));
        }
    }
    if out.len() > LINES_PER_PAGE {
        let (kind, text) = rows[0];
        return Err(format!(
            "field-station page overflows: {} lines, the limit is {LINES_PER_PAGE}. \
             It starts: ({}, {})",
            out.len(),
            py::repr(kind),
            py::repr(text)
        ));
    }
    Ok(out)
}

fn demo_streams() -> Result<Vec<String>, Fail> {
    let mut streams = vec![title_stream(field_station_content::TITLE)];
    for (i, page) in field_station_content::pages().iter().enumerate() {
        let rows = flow_page(page)?;
        streams.push(content_stream(
            &rows,
            if i > 0 { Some(i + 1) } else { None },
        ));
    }
    Ok(streams)
}

/// Every kind, in the order the usage line lists them.
pub const KINDS: &[&str] = &[
    "screenplay",
    "prose",
    "blank",
    "images",
    "jpx",
    "torture",
    "demo",
];

/// A kind's page content streams.
pub fn streams(kind: &str) -> Result<Vec<String>, Fail> {
    match kind {
        "screenplay" => {
            let mut out = vec![title_stream(TITLE)];
            for (i, page) in layout().iter().enumerate() {
                out.push(content_stream(page, if i > 0 { Some(i + 1) } else { None }));
            }
            Ok(out)
        }
        "prose" => Ok(vec![prose_stream()]),
        "blank" => Ok(vec![blank_stream(), blank_stream()]),
        "images" => image_streams(),
        "jpx" => jpx_streams(),
        "torture" => torture_streams(),
        "demo" => demo_streams(),
        _ => Err(format!("KeyError: {}", py::repr(kind))),
    }
}

/// A picture is not expressible as a content stream alone: it needs an
/// object of its own and a per-page resource entry naming it. kind ->
/// (name -> XObject body, names drawn on each sheet), for the kinds that
/// have pictures.
pub fn xobjects(kind: &str) -> Result<Option<(XObjects, PageXObjects)>, Fail> {
    Ok(match kind {
        "images" => Some((xobject_bodies(), image_page_xobjects())),
        // Sheet 1 is the title page and names nothing; the picture is on
        // sheet 2.
        "jpx" => Some((
            vec![(JPX_PLACEMENT.0.to_string(), jpx_xobject())],
            vec![vec![], vec![JPX_PLACEMENT.0]],
        )),
        "torture" => {
            let drawn = torture_placements()?
                .iter()
                .map(|sheet| sheet.iter().map(|p| p.0).collect())
                .collect();
            Some((torture_xobject_bodies()?, drawn))
        }
        _ => None,
    })
}

/// A kind's PDF: (page count, bytes).
pub fn pdf(kind: &str) -> Result<(usize, Vec<u8>), Fail> {
    let pictures = xobjects(kind)?;
    let streams = streams(kind)?;
    let bytes = match &pictures {
        Some((bodies, drawn)) => build(&streams, bodies, Some(drawn))?,
        None => build(&streams, &Vec::new(), None)?,
    };
    Ok((streams.len(), bytes))
}

// --- layout as data, for tests ---------------------------------------------
// Page PLACEMENT is the thing most likely to rot silently: a speech written
// to start at line 50 so it straddles a page break still satisfies every
// output-shaped assertion at line 48, while proving nothing. These mirror
// what the content streams draw, so an assertion about a line number is an
// assertion about the actual PDF.

fn layout_row(line: usize, x: f64, runs: Vec<Py>, underline: bool) -> Py {
    Py::Dict(vec![
        ("line".into(), Py::Int(line as i64)),
        ("x".into(), Py::Float(py::round3(x))),
        ("runs".into(), Py::List(runs)),
        ("underline".into(), Py::Bool(underline)),
    ])
}

fn plain_run(text: &str) -> Py {
    Run {
        styles: vec![],
        text: text.to_string(),
    }
    .to_py()
}

fn title_rows(table: &[(f64, &str)]) -> Vec<Py> {
    table
        .iter()
        .map(|(inches_down, text)| {
            let x = (8.5 - text.chars().count() as f64 / 10.0) / 2.0;
            layout_row(
                (inches_down * 6.0) as usize,
                x,
                vec![plain_run(text)],
                false,
            )
        })
        .collect()
}

fn content_rows(rows: &[Row]) -> Vec<Py> {
    rows.iter()
        .enumerate()
        .filter_map(|(n, row)| {
            row.as_ref()
                .map(|(x, text)| layout_row(n, *x, vec![plain_run(text)], false))
        })
        .collect()
}

/// Styled rows, including two-column ones, as flat data. Both columns of a
/// dual line report the SAME line number, which is the property that makes
/// them a dual region at all.
fn torture_rows(rows: &[Option<TortureRow>]) -> Vec<Py> {
    let mut out = Vec::new();
    for (n, row) in rows.iter().enumerate() {
        let pairs: Vec<(f64, &Vec<Run>)> = match row {
            None => continue,
            Some(TortureRow::One(x, runs)) => vec![(*x, runs)],
            Some(TortureRow::Multi(pairs)) => pairs.iter().map(|(x, r)| (*x, r)).collect(),
        };
        for (x, runs) in pairs {
            let underline = runs.iter().any(|r| r.styles.contains(&'u'));
            out.push(layout_row(
                n,
                x,
                runs.iter().map(Run::to_py).collect(),
                underline,
            ));
        }
    }
    out
}

fn layout_page(page: usize, rows: Vec<Py>) -> Py {
    Py::Dict(vec![
        ("page".into(), Py::Int(page as i64)),
        ("rows".into(), Py::List(rows)),
    ])
}

/// `--emit-layout`'s answer for a kind.
pub fn layout_json(kind: &str) -> Result<Py, Fail> {
    let mut pages = Vec::new();
    match kind {
        "torture" => {
            pages.push(layout_page(1, title_rows(torture_content::TITLE)));
            for (i, rows) in flow_torture(&torture_content::content())?
                .iter()
                .enumerate()
            {
                pages.push(layout_page(i + 2, torture_rows(rows)));
            }
        }
        "demo" => {
            pages.push(layout_page(1, title_rows(field_station_content::TITLE)));
            for (i, rows) in field_station_content::pages().iter().enumerate() {
                pages.push(layout_page(i + 2, content_rows(&flow_page(rows)?)));
            }
        }
        "screenplay" => {
            pages.push(layout_page(1, title_rows(TITLE)));
            for (i, rows) in layout().iter().enumerate() {
                pages.push(layout_page(i + 2, content_rows(rows)));
            }
        }
        // prose, blank, images and jpx have no line-addressable structure
        // worth emitting: one is a wall of paragraphs, one has no text
        // operators at all, and the two picture kinds are about where
        // pictures sit and what the filters make of them, not about text
        // rows.
        _ => {}
    }
    Ok(Py::List(pages))
}

// --- the command -----------------------------------------------------------

fn usage_kinds() -> String {
    KINDS.join("|")
}

fn help() -> String {
    let kinds = usage_kinds();
    format!(
        "usage: cargo xtask make-fixture <{kinds}> <out.pdf>\n\
         \x20      cargo xtask make-fixture --emit-layout <{kinds}>\n\
         \x20      cargo xtask make-fixture --wrap <width> <text>\n\
         \x20      cargo xtask make-fixture --parse-markup <text>\n\
         \x20      cargo xtask make-fixture --atline-overflow-check\n"
    )
}

fn ok(stdout: String) -> Outcome {
    Outcome {
        code: 0,
        stdout,
        stderr: String::new(),
    }
}

fn fail(message: &str) -> Outcome {
    Outcome {
        code: 1,
        stdout: String::new(),
        stderr: format!("{message}\n"),
    }
}

const INDEX_ERROR: &str = "IndexError: list index out of range";

/// One command line, as make-fixture.py ran it: what it printed and its
/// exit code. Writes the PDF when asked for one.
pub fn run(args: &[String]) -> Outcome {
    let first = args.first().map(String::as_str);
    let arg = |i: usize| args.get(i).map(String::as_str);
    match first {
        Some("--atline-overflow-check") => {
            // Proves the guard fires: asked to pad back to line 2 having
            // already emitted well past it.
            let content = [Item::El("action", "x ".repeat(200)), Item::AtLine(2)];
            match flow_torture(&content) {
                Err(message) => fail(&message),
                Ok(_) => fail("expected atline to raise, it did not"),
            }
        }
        Some("--wrap") => {
            let Some(text) = arg(2) else {
                return fail(INDEX_ERROR);
            };
            let (plain, spans) = match parse_markup(text) {
                Ok(parsed) => parsed,
                Err(message) => return fail(&message),
            };
            let width_text = arg(1).unwrap_or_default();
            let Ok(width) = width_text.trim().parse::<i64>() else {
                return fail(&format!(
                    "ValueError: invalid literal for int() with base 10: {}",
                    py::repr(width_text)
                ));
            };
            let lines = wrap_spans(&plain, &spans, width)
                .iter()
                .map(|line| Py::List(line.iter().map(Run::to_py).collect()))
                .collect();
            ok(format!("{}\n", Py::List(lines).dumps()))
        }
        Some("--parse-markup") => {
            let Some(text) = arg(1) else {
                return fail(INDEX_ERROR);
            };
            match parse_markup(text) {
                Err(message) => fail(&message),
                Ok((plain, spans)) => {
                    let spans = spans
                        .iter()
                        .map(|(a, b, s)| {
                            Py::List(vec![
                                Py::Int(*a as i64),
                                Py::Int(*b as i64),
                                Py::Str(s.to_string()),
                            ])
                        })
                        .collect();
                    let value = Py::Dict(vec![
                        ("plain".into(), Py::Str(plain)),
                        ("spans".into(), Py::List(spans)),
                    ]);
                    ok(format!("{}\n", value.dumps()))
                }
            }
        }
        Some("--emit-layout") => {
            let kind = arg(1).unwrap_or_default();
            if args.len() != 2 || !KINDS.contains(&kind) {
                return fail(&format!(
                    "usage: cargo xtask make-fixture --emit-layout <{}>",
                    usage_kinds()
                ));
            }
            match layout_json(kind) {
                Ok(value) => ok(format!("{}\n", value.dumps())),
                Err(message) => fail(&message),
            }
        }
        Some("-h" | "--help") => ok(help()),
        _ => {
            let kind = first.unwrap_or_default();
            if args.len() != 2 || !KINDS.contains(&kind) {
                return fail(&format!(
                    "usage: cargo xtask make-fixture <{}> <out.pdf>",
                    usage_kinds()
                ));
            }
            let path = &args[1];
            let (n_pages, bytes) = match pdf(kind) {
                Ok(made) => made,
                Err(message) => return fail(&message),
            };
            if let Err(e) = std::fs::write(path, bytes) {
                return fail(&format!("{e}: {}", py::repr(path)));
            }
            ok(format!("{n_pages} pages -> {path}\n"))
        }
    }
}

/// `cargo xtask make-fixture`: its exit code.
pub fn main(args: &[String]) -> u8 {
    let outcome = run(args);
    print!("{}", outcome.stdout);
    eprint!("{}", outcome.stderr);
    let _ = std::io::stdout().flush();
    outcome.code
}
