//! Layout learning: read each script against its own columns.
//!
//! Classification is geometric, by fixed bands of page width tuned on the
//! common layouts (action under 20%, dialogue 25-35%, cues 35-50%). Every
//! real script measured so far sits inside them, but a script typed with a
//! word processor's one-inch margins puts its dialogue at 24% and its cues
//! at 35%, and every speech in it read as action.
//!
//! A script is consistent with itself, though: each column is used hundreds
//! of times. So before reading, this measures the script's own action,
//! dialogue and cue columns from the speeches themselves (a cue-shaped line
//! with a shallower-set line straight under it), and only when a column
//! falls outside the band it belongs to, moves every line
//! proportionally between the script's columns and the standard ones. A
//! script that already fits is read exactly as before.

use std::borrow::Cow;
use std::collections::BTreeMap;

use sp_model::{RawLine, indent_ranges as R};

use crate::cue::is_cue_text;
use crate::js;

/// A script's three main columns, as a percentage of page width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Columns {
    pub action: f64,
    pub dialogue: f64,
    pub cue: f64,
}

/// Where the bands expect each column: the common layout (action 1.5",
/// dialogue 2.5", cue 3.5" to 3.7" on US Letter).
pub const STANDARD: Columns = Columns {
    action: 18.0,
    dialogue: 29.0,
    cue: 42.0,
};

/// Fewer speeches than this and the measure is not trusted.
pub const MIN_SPEECHES: usize = 20;

/// The most common value, and the share of samples within one point of it.
fn mode(values: &[f64]) -> Option<(f64, f64)> {
    let mut counts: BTreeMap<i64, usize> = BTreeMap::new();
    for v in values {
        #[allow(clippy::cast_possible_truncation)]
        let k = v.round() as i64;
        *counts.entry(k).or_default() += 1;
    }
    let (&k, _) = counts.iter().max_by_key(|(k, c)| (**c, -**k))?;
    let near: usize = (k - 1..=k + 1).filter_map(|j| counts.get(&j)).sum();
    #[allow(clippy::cast_precision_loss)]
    Some((k as f64, near as f64 / values.len() as f64))
}

/// The script's own columns, or `None` when it has too few speeches or no
/// clear column to measure.
pub fn measure(lines: &[RawLine]) -> Option<Columns> {
    let (mut cues, mut dialogue) = (Vec::new(), Vec::new());
    for pair in lines.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        let gap = a.y - b.y;
        if a.placed || b.placed || a.page_num != b.page_num || !(6.0..=20.0).contains(&gap) {
            continue;
        }
        // A cue is a cue-shaped line with a speech set shallower under it.
        if is_cue_text(&a.text) && !is_cue_text(&b.text) && b.indent < a.indent - 4.0 {
            cues.push(a.indent);
            if !js::trim(&b.text).starts_with('(') {
                dialogue.push(b.indent);
            }
        }
    }
    if cues.len() < MIN_SPEECHES || dialogue.len() < MIN_SPEECHES {
        return None;
    }
    let (cue, cue_share) = mode(&cues)?;
    let (dialogue, dialogue_share) = mode(&dialogue)?;
    if cue_share < 0.6 || dialogue_share < 0.6 {
        return None;
    }
    let left: Vec<f64> = lines
        .iter()
        .filter(|l| !l.placed && l.indent < dialogue - 5.0)
        .map(|l| l.indent)
        .collect();
    let (action, _) = mode(&left)?;
    (0.0 < action && action < dialogue && dialogue < cue && cue < 100.0).then_some(Columns {
        action,
        dialogue,
        cue,
    })
}

/// Every column sits comfortably inside the band that reads it.
pub fn fits(c: &Columns) -> bool {
    c.action < R::ACTION_MAX - 1.0
        && (R::DIALOGUE_MIN + 1.0..=R::DIALOGUE_MAX - 2.0).contains(&c.dialogue)
        && (R::CHARACTER_MIN + 2.0..=R::CHARACTER_MAX - 2.0).contains(&c.cue)
}

/// `indent` moved from the script's columns to the standard ones, linearly
/// between neighbouring columns (and the page's edges).
pub fn translate(c: &Columns, indent: f64) -> f64 {
    let from = [0.0, c.action, c.dialogue, c.cue, 100.0];
    let to = [0.0, STANDARD.action, STANDARD.dialogue, STANDARD.cue, 100.0];
    let i = (1..from.len())
        .find(|&i| indent <= from[i])
        .unwrap_or(from.len() - 1);
    let (x0, x1, y0, y1) = (from[i - 1], from[i], to[i - 1], to[i]);
    let moved = y0 + (indent - x0) * (y1 - y0) / (x1 - x0);
    js::round(moved)
}

/// The lines as the bands should see them: unchanged when the script fits
/// the bands or cannot be measured, else every line not placed by
/// extraction translated to the standard columns.
pub fn calibrate(lines: &[RawLine]) -> Cow<'_, [RawLine]> {
    match measure(lines) {
        Some(c) if !fits(&c) => Cow::Owned(
            lines
                .iter()
                .map(|l| {
                    let mut l = l.clone();
                    if !l.placed {
                        l.indent = translate(&c, l.indent);
                    }
                    l
                })
                .collect(),
        ),
        _ => Cow::Borrowed(lines),
    }
}
