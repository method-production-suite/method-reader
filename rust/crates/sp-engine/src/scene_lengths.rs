//! How long each scene runs, in eighths of a page, measured from the PDF at
//! convert time: what Final Draft shows as a scene's Length, and what the
//! .fountain cannot carry (registry #13b). Measured here from the parser's
//! geometry, saved beside the script in the library (library.rs,
//! `save_scene_measures`), and read by the FDX export.
//!
//! The rule:
//!
//! - A page is 8 eighths of the script's TEXT AREA: from the top of the
//!   first line of text on a typical page (the median page) down to a
//!   one-inch bottom margin, Final Draft's default, or lower when the
//!   script's fullest pages run lower (the tenth percentile of the pages'
//!   last baselines, so one stray line cannot stretch every page). A script
//!   that breaks its pages early therefore measures short of 8/8 a page,
//!   as a ruler on the printed page would. Page furniture (printed page
//!   numbers, headers and footers the parser set aside) is not text here.
//! - On each page a scene runs from the top of its first line there to the
//!   top of the next scene's heading, or, when it is the last scene on the
//!   page, to the page's last line of text (a picture counts by the room it
//!   is drawn in). Its length is the sum over its pages.
//! - Rounded to the nearest eighth, halves up, and never below 1/8 for a
//!   scene with anything in it: the common practice of assistant directors
//!   and breakdown tools ("less than an eighth still counts as an eighth").
//!   Final Draft measures its own reflowed pages, so its numbers for the
//!   same script can differ by an eighth here and there: this is a best
//!   guess at its rounding, not a port of it.
//! - An omitted scene ("OMITTED") is 0/8.
//!
//! Each scene's page is the page its heading sits on, as printed when the
//! PDF numbers its pages: the same label the `= pg N` markers carry.

use std::collections::BTreeMap;

use sp_fdx::{SceneMeasure, is_omitted_heading};
use sp_fountain::{is_body, printed_page_offset};
use sp_model::{ElementType, ParsedScreenplay, RawImage, RawLine, ScreenplayElement};

/// The pitch assumed when no two lines of a block sit one above the other.
const DEFAULT_PITCH: f64 = 12.0;
/// The widest gap that is still one line to the next inside a block
/// (group.rs breaks a block at a larger one).
const MAX_PITCH: f64 = 20.0;
/// The bottom of the text area at the latest: a one-inch bottom margin,
/// Final Draft's default, in PDF user space (y up from the sheet's foot).
const BOTTOM_MARGIN: f64 = 72.0;
/// How far a line may sit above its element's recorded baseline and still
/// be its (floating-point slack only).
const SLACK: f64 = 0.01;

/// `raw` eighths, rounded the way this module adopts: to the nearest
/// eighth, halves up, never below one.
pub fn round_eighths(raw: f64) -> u32 {
    let rounded = (raw + 0.5).floor();
    if rounded < 1.0 {
        1
    } else {
        // A scene is never billions of pages long.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = rounded as u32;
        n
    }
}

/// The most common gap between consecutive baselines inside a block, to a
/// tenth of a point: the script's line pitch.
fn line_pitch(lines: &[RawLine]) -> f64 {
    let mut counts: BTreeMap<i64, u32> = BTreeMap::new();
    for pair in lines.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        let gap = a.y - b.y;
        if a.page_num == b.page_num && gap > 0.0 && gap <= MAX_PITCH {
            #[allow(clippy::cast_possible_truncation)]
            let tenths = (gap * 10.0).round() as i64;
            *counts.entry(tenths).or_default() += 1;
        }
    }
    // The most common; the smaller gap on a tie (BTreeMap order).
    let mut best: Option<(i64, u32)> = None;
    for (tenths, count) in counts {
        if best.is_none_or(|(_, c)| count > c) {
            best = Some((tenths, count));
        }
    }
    #[allow(clippy::cast_precision_loss)]
    best.map_or(DEFAULT_PITCH, |(tenths, _)| tenths as f64 / 10.0)
}

/// The value at `fraction` of the way through `values` (sorted here),
/// rounding the index down.
fn quantile(mut values: Vec<f64>, fraction: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let at = ((values.len() - 1) as f64 * fraction).floor() as usize;
    Some(values[at])
}

/// Where one element sits on its page, as edges in PDF user space (up is
/// the high y): `top` is the top of its first line (or of the picture),
/// `bottom` the baseline of its last line (or the picture's foot).
#[derive(Clone, Copy)]
struct Extent {
    top: f64,
    bottom: f64,
}

/// Each element's extent, by index: the lines each owns are those below
/// its own first baseline and above the next element's on the same page.
fn extents(
    elements: &[ScreenplayElement],
    lines: &[RawLine],
    images: &[RawImage],
    page_height: &dyn Fn(u32) -> Option<f64>,
    pitch: f64,
) -> Vec<Option<Extent>> {
    let mut by_page: BTreeMap<u32, Vec<(f64, usize)>> = BTreeMap::new();
    for (i, el) in elements.iter().enumerate() {
        if let Some(y) = el.y {
            by_page.entry(el.page_num).or_default().push((y, i));
        }
    }
    let mut out: Vec<Option<Extent>> = elements
        .iter()
        .map(|el| {
            let y = el.y?;
            if el.kind == ElementType::Image {
                let drawn = el.img.as_ref().and_then(|img| {
                    let raw = images.iter().find(|r| r.file == img.file)?;
                    Some(raw.page_height_pct / 100.0 * page_height(el.page_num)?)
                });
                Some(Extent {
                    top: y,
                    bottom: y - drawn.unwrap_or(0.0),
                })
            } else {
                Some(Extent {
                    top: y + pitch,
                    bottom: y,
                })
            }
        })
        .collect();
    for line in lines {
        let Some(on_page) = by_page.get(&line.page_num) else {
            continue;
        };
        // The nearest element at or above the line.
        let owner = on_page
            .iter()
            .filter(|(y, _)| *y >= line.y - SLACK)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some(&(_, i)) = owner
            && let Some(extent) = out[i].as_mut()
        {
            extent.bottom = extent.bottom.min(line.y);
        }
    }
    out
}

/// Each scene heading's page and length, in the order the scenes come, or
/// `None` when the script cannot be measured (an element with no position,
/// or no text area to measure against).
pub fn measure_scenes(
    lines: &[RawLine],
    images: &[RawImage],
    page_height: &dyn Fn(u32) -> Option<f64>,
    screenplay: &ParsedScreenplay,
) -> Option<Vec<SceneMeasure>> {
    let elements = &screenplay.elements;
    let body: Vec<usize> = (0..elements.len())
        .filter(|&i| is_body(&elements[i]))
        .collect();
    if body.iter().any(|&i| elements[i].y.is_none()) {
        return None;
    }
    let pitch = line_pitch(lines);
    let extent = extents(elements, lines, images, page_height, pitch);

    // Each page's text, top and bottom, over its body elements.
    let mut pages: BTreeMap<u32, Extent> = BTreeMap::new();
    for &i in &body {
        let e = extent[i]?;
        pages
            .entry(elements[i].page_num)
            .and_modify(|p| {
                p.top = p.top.max(e.top);
                p.bottom = p.bottom.min(e.bottom);
            })
            .or_insert(e);
    }
    let top = quantile(pages.values().map(|p| p.top).collect(), 0.5)?;
    let bottom = quantile(pages.values().map(|p| p.bottom).collect(), 0.1)?.min(BOTTOM_MARGIN);
    let area = top - bottom;
    if area < pitch * 3.0 {
        return None;
    }

    // Each scene's stretch of each page: where it starts there, in the
    // order the stretches come down the page.
    let mut heading_of: Vec<usize> = Vec::new();
    // page -> [(scene, top of its first element there)]
    let mut stretches: BTreeMap<u32, Vec<(usize, f64)>> = BTreeMap::new();
    for &i in &body {
        let el = &elements[i];
        if el.kind == ElementType::Scene {
            heading_of.push(i);
        }
        let Some(scene) = heading_of.len().checked_sub(1) else {
            continue; // before the first scene: no scene's
        };
        let top = extent[i]?.top;
        let on_page = stretches.entry(el.page_num).or_default();
        match on_page.last_mut() {
            Some((s, t)) if *s == scene => *t = t.max(top),
            _ => on_page.push((scene, top)),
        }
    }
    let mut raw = vec![0.0f64; heading_of.len()];
    for (page, on_page) in &stretches {
        let page_bottom = pages[page].bottom;
        for (k, &(scene, start)) in on_page.iter().enumerate() {
            let end = on_page.get(k + 1).map_or(page_bottom, |&(_, next)| next);
            raw[scene] += (start - end).max(0.0) / area * 8.0;
        }
    }

    let offset = printed_page_offset(elements).unwrap_or(0);
    Some(
        heading_of
            .iter()
            .zip(raw)
            .map(|(&i, raw)| {
                let el = &elements[i];
                let label = i64::from(el.page_num) + offset;
                SceneMeasure {
                    page: (label >= 1).then(|| label.to_string()),
                    eighths: if is_omitted_heading(&el.text) {
                        0
                    } else {
                        round_eighths(raw)
                    },
                }
            })
            .collect(),
    )
}
