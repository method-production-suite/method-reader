//! Port of `src/parser/first-page.ts`.
//!
//! The first real page of a script, as plain paragraphs, for the window to
//! type out while the rest converts (spec decision 17). Pure, and read-only
//! over RawLines: nothing here feeds classification or the parse result, so
//! the parser stays format-option-free and the .fountain cannot move.

use sp_model::RawLine;

use crate::boilerplate::is_boilerplate_line;
use crate::js;

/// A page needs this many lines before it can be the first real page.
pub const FIRST_PAGE_MIN_LINES: usize = 8;

/// A gap this many times the page's usual line gap starts a paragraph.
const PARAGRAPH_GAP: f64 = 1.5;

/// Left edges this close (percent of page width) are the same indent.
const INDENT_TOLERANCE: f64 = 1.0;

/// A line is centred when its middle sits this close (percent) to the page's.
const CENTRE_BAND: f64 = 10.0;

/// Courier 12, the screenplay face: 10 characters to the inch. RawLine has no
/// width, so a line's extent is estimated from its length.
const CHAR_WIDTH_PT: f64 = 7.2;

/// The page's line gap: the smallest gap between consecutive lines that
/// occurs at least twice (gaps within half a point count as one), so a
/// wrapped line's gap, not a blank line's. Not simply the commonest gap: on
/// a page of one-line speeches the blank line between blocks outnumbers the
/// wrap, and taking it as the line gap ran every heading into its action.
/// A gap under 40% of the commonest is not a line gap (two lines set almost
/// level, as dual dialogue can be), so it is passed over.
fn common_gap(lines: &[&RawLine]) -> f64 {
    // Insertion-ordered, as a JavaScript Map iterates.
    let mut counts: Vec<(f64, usize)> = Vec::new();
    for i in 1..lines.len() {
        let gap = js::round((lines[i - 1].y - lines[i].y) * 2.0) / 2.0;
        if gap > 0.0 {
            match counts.iter_mut().find(|(g, _)| *g == gap) {
                Some((_, n)) => *n += 1,
                None => counts.push((gap, 1)),
            }
        }
    }
    let mut mode = 0.0;
    let mut mode_count = 0;
    for &(gap, count) in &counts {
        if count > mode_count || (count == mode_count && gap < mode) {
            mode = gap;
            mode_count = count;
        }
    }
    let mut best = mode;
    for &(gap, count) in &counts {
        if count >= 2 && gap < best && gap >= mode * 0.4 {
            best = gap;
        }
    }
    best
}

fn ends_with_word_hyphen(s: &str) -> bool {
    let mut it = s.chars().rev();
    it.next() == Some('-') && it.next().is_some_and(|c| c.is_ascii_alphabetic())
}

/// Joins a wrapped line onto its paragraph: with one space, or with none
/// when the line before broke at a word's hyphen ("fourteen-" "year-old").
fn join_lines(parts: &[String]) -> String {
    let mut out = parts.first().cloned().unwrap_or_default();
    for next in parts.iter().skip(1) {
        if ends_with_word_hyphen(&out) && !out.ends_with("--") {
            out.push_str(next);
        } else {
            out.push(' ');
            out.push_str(next);
        }
    }
    out
}

/// A page's words as plain paragraphs, in reading order. A paragraph breaks
/// where the gap to the next line is more than 1.5 times the page's usual
/// line gap, or where the left edge moves (a cue sits on its speech with no
/// gap at all). Wrapped lines join with one space, or none after a word's
/// hyphen. Page marks and revision slugs are dropped: they are furniture,
/// not words. Only `text` is read, never `styled`, so no emphasis marker can
/// reach the window.
pub fn page_paragraphs(lines: &[RawLine]) -> Vec<String> {
    let words: Vec<&RawLine> = lines
        .iter()
        .filter(|l| !js::trim(&l.text).is_empty() && !is_boilerplate_line(&l.text))
        .collect();
    if words.is_empty() {
        return Vec::new();
    }
    let gap = common_gap(&words);
    let mut paragraphs: Vec<Vec<String>> = vec![vec![js::trim(&words[0].text).to_owned()]];
    for i in 1..words.len() {
        let prev = words[i - 1];
        let cur = words[i];
        let wide = gap > 0.0 && prev.y - cur.y > gap * PARAGRAPH_GAP;
        let moved = (prev.indent - cur.indent).abs() > INDENT_TOLERANCE;
        let text = js::trim(&cur.text).to_owned();
        if wide || moved {
            paragraphs.push(vec![text]);
        } else if let Some(last) = paragraphs.last_mut() {
            last.push(text);
        }
    }
    paragraphs.iter().map(|p| join_lines(p)).collect()
}

/// True when a page reads as a title page: at least half its lines are
/// centred. Centred means the line's middle is within 10% of the page's AND
/// its left edge is its own. The second half matters: a speech runs 2.5in to
/// 6in on an 8.5in page, so dialogue and full action lines are centred too,
/// but they share their left edge with the lines around them. A title block
/// is centred line by line, so each line starts somewhere different.
pub fn is_title_like(lines: &[RawLine], page_width: f64) -> bool {
    if lines.is_empty() {
        return true;
    }
    let centred = lines
        .iter()
        .enumerate()
        .filter(|&(i, l)| {
            #[allow(clippy::cast_precision_loss)]
            let extent = js::len(&l.text) as f64 * CHAR_WIDTH_PT;
            let middle = l.indent + (extent / page_width) * 100.0 / 2.0;
            if (middle - 50.0).abs() > CENTRE_BAND {
                return false;
            }
            !lines
                .iter()
                .enumerate()
                .any(|(j, o)| j != i && (o.indent - l.indent).abs() <= INDENT_TOLERANCE)
        })
        .count();
    centred * 2 >= lines.len()
}

/// The first-real-page test the engine applies to each page as it is read.
pub fn is_first_real_page(lines: &[RawLine], page_width: f64) -> bool {
    lines.len() >= FIRST_PAGE_MIN_LINES && !is_title_like(lines, page_width)
}
