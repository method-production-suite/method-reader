//! Port of `src/parser/boilerplate.ts`.
//!
//! Production-draft page furniture — watermarks, revision slugs, draft
//! stamps, page marks — routed into the "page-number" element type, which
//! every consumer (playback batching, RSVP, emotion inference, reader
//! display) already skips. Elements are re-typed, never removed, so
//! indices/resume positions/audio hashes are untouched. Spec: 2026-07-21
//! design doc.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use sp_model::{ElementType, ScreenplayElement};

use crate::js;

const COLOR: &str = r"(?:\d+(?:st|nd|rd|th)\s+)?(?:white|blue|pink|yellow|green|goldenrod|buff|salmon|cherry|tan|gray|grey)";
const DATE: &str = r"\(?\s*(?:\d{1,2}[\/.-]\d{1,2}[\/.-]\d{2,4}|mm\/dd\/yy)\s*\)?";
const PAGE_MARK: &str = r"[A-Z]?\d{1,3}\.(?:\d{1,3})?";
const DRAFT_STAMP: &str = r"(?:(?:first|second|third|fourth|fifth|final|revised|production|network|studio)\s+draft|shooting\s+script|production\s+draft)";

// A color word alone is never a slug (color-named character cues like
// "CHERRY" must survive) — it needs a Rev./Revision suffix or a date.
fn rev_slug() -> String {
    format!(r"{COLOR}\s*(?:rev(?:ision)?\.?\s*(?:{DATE})?|{DATE})")
}

// A line is boilerplate only when slugs/stamps/dates/page-marks consume the
// WHOLE line (any repetition, any order) — a sentence containing these
// words never matches.
static BOILERPLATE_LINE: LazyLock<Regex> = LazyLock::new(|| {
    js::re_i(&format!(
        r"^(?:\s*(?:{}|{DRAFT_STAMP}|{DATE}|{PAGE_MARK})[\s—–-]*)+$",
        rev_slug()
    ))
});

/// Pattern layer: revision slugs, draft stamps, header dates, page marks.
pub fn is_boilerplate_line(text: &str) -> bool {
    let t = js::trim(text);
    if t.is_empty() || js::len(t) > 120 {
        return false;
    }
    BOILERPLATE_LINE.is_match(&js::ci(t))
}

/// Candidates for recurrence suppression: short standalone non-dialogue
/// lines.
const RECUR_MAX_LEN: usize = 60;
const RECUR_MIN_PAGES: f64 = 3.0;
const RECUR_PAGE_FRACTION: f64 = 0.4;

fn is_candidate(e: &ScreenplayElement) -> bool {
    let n = js::len(js::trim(&e.text));
    matches!(
        e.kind,
        ElementType::Action | ElementType::PageNumber | ElementType::MiniSlug
    ) && n > 0
        && n <= RECUR_MAX_LEN
}

fn norm(s: &str) -> String {
    js::collapse_ws(js::trim(s)).to_lowercase()
}

/// Recurrence layer (the watermark killer): any candidate whose normalized
/// text appears on >= max(3, 40% of pages) distinct pages is re-typed to
/// "page-number". Pure and idempotent; never mutates its input.
///
/// Mini-slugs go through the SAME rule as action, and must. Classification
/// runs first, so a left-flush all-caps watermark ("CONFIDENTIAL")
/// classifies as a mini-slug at priority 9 — outside this layer it would
/// render as a bold micro-heading on every page. Counting them keeps the
/// pool exactly what it was before classify.ts learned to mint mini-slugs,
/// and hiding them on the same terms keeps ONE watermark verdict per
/// document: a mark that happens to type action on one page and mini-slug
/// on the next must not come out hidden in one place and visible in the
/// other. Verbatim recurrence on 40%+ of pages is watermark-shaped, not
/// slug-shaped — real slug families vary by location — so the threshold is
/// doing the job it was built for. #5b.
pub fn suppress_boilerplate(
    elements: &[ScreenplayElement],
    page_count: u32,
) -> Vec<ScreenplayElement> {
    let threshold = RECUR_MIN_PAGES.max((f64::from(page_count) * RECUR_PAGE_FRACTION).ceil());
    let mut pages_by_text: HashMap<String, HashSet<u32>> = HashMap::new();

    for e in elements {
        if !is_candidate(e) {
            continue;
        }
        pages_by_text
            .entry(norm(&e.text))
            .or_default()
            .insert(e.page_num);
    }
    #[allow(clippy::cast_precision_loss)]
    let recurs = |e: &ScreenplayElement| {
        pages_by_text.get(&norm(&e.text)).map_or(0, HashSet::len) as f64 >= threshold
    };

    elements
        .iter()
        .map(|e| {
            if e.kind != ElementType::Action && e.kind != ElementType::MiniSlug {
                return e.clone();
            }
            let furniture = ScreenplayElement {
                kind: ElementType::PageNumber,
                ..e.clone()
            };
            // Pattern layer: slugs embedding per-page-varying page marks never
            // recur verbatim, so the recurrence count alone misses them —
            // catch directly.
            if is_boilerplate_line(&e.text) {
                return furniture;
            }
            if !is_candidate(e) {
                return e.clone();
            }
            if recurs(e) { furniture } else { e.clone() }
        })
        .collect()
}
