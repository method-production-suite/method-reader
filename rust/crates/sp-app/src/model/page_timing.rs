//! The clock of "the page writes itself": `desktop/ui/page-timing.js`.
//! Every function takes the time as milliseconds since the typing began.

/// How long the typing takes, whatever the script.
pub const TYPE_MS: f64 = 4000.0;
/// How many blocks the wave runs behind the typing.
pub const WAVE_LAG: usize = 2;
/// The wave's pace, and the longest a sweep may take.
pub const STEP_MS: f64 = 120.0;
pub const SWEEP_MS: f64 = 1200.0;
/// How long a formed block glows.
pub const GLOW_MS: f64 = 600.0;
/// The longest whole show when the book is there before the typing ends.
pub const MAX_SHOW_MS: f64 = TYPE_MS + SWEEP_MS + GLOW_MS;
/// The gap between two key sounds, and between two bells.
pub const KEY_GAP_MS: f64 = 55.0;
pub const DING_GAP_MS: f64 = 700.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanBlock {
    pub text: String,
    pub chars: Vec<char>,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Plan {
    pub blocks: Vec<PlanBlock>,
    pub total: usize,
}

/// A page, ready to be timed. Empty paragraphs are left out. Characters are
/// Unicode scalar values (JavaScript's code points).
pub fn plan_show<S: AsRef<str>>(paragraphs: &[S]) -> Plan {
    let mut blocks = Vec::new();
    let mut total = 0;
    for p in paragraphs {
        let chars: Vec<char> = p.as_ref().trim().chars().collect();
        if chars.is_empty() {
            continue;
        }
        let start = total;
        total += chars.len();
        blocks.push(PlanBlock {
            text: chars.iter().collect(),
            chars,
            start,
            end: total,
        });
    }
    Plan { blocks, total }
}

pub fn has_page(plan: &Plan) -> bool {
    plan.total > 0
}

/// How many characters are typed `t` ms after the typing began.
pub fn typed_at(plan: &Plan, t: f64) -> usize {
    if plan.total == 0 || t.is_nan() || t <= 0.0 {
        return 0;
    }
    if t >= TYPE_MS {
        return plan.total;
    }
    plan.total
        .min((t * plan.total as f64 / TYPE_MS).floor() as usize)
}

fn typed_by(plan: &Plan, count: usize) -> f64 {
    if count == 0 {
        return 0.0;
    }
    (count as f64 * TYPE_MS / plan.total as f64).ceil()
}

/// How much of each block shows when `typed` characters are down.
pub fn shown_text(plan: &Plan, typed: usize) -> Vec<String> {
    plan.blocks
        .iter()
        .map(|b| {
            let n = typed.saturating_sub(b.start).min(b.chars.len());
            b.chars[..n].iter().collect()
        })
        .collect()
}

pub fn step_for(plan: &Plan) -> f64 {
    let n = plan.blocks.len();
    if n == 0 {
        STEP_MS
    } else {
        STEP_MS.min(SWEEP_MS / n as f64)
    }
}

/// When block `i` takes its form, given when the book arrived.
pub fn form_at(plan: &Plan, book_at: Option<f64>, i: usize) -> Option<f64> {
    let book_at = book_at?;
    let n = plan.blocks.len();
    if i >= n {
        return None;
    }
    let step = step_for(plan);
    let ahead = i + WAVE_LAG - 1;
    let behind_typing = if ahead < n {
        typed_by(plan, plan.blocks[ahead].end)
    } else {
        TYPE_MS + (ahead - n + 1) as f64 * step
    };
    Some(behind_typing.max(book_at + i as f64 * step))
}

/// How many blocks have their form `t` ms in, always a run from the top.
pub fn formed_at(plan: &Plan, book_at: Option<f64>, t: f64) -> usize {
    (0..plan.blocks.len())
        .take_while(|&i| form_at(plan, book_at, i).is_some_and(|at| at <= t))
        .count()
}

/// The blocks still glowing `t` ms in.
pub fn glowing_at(plan: &Plan, book_at: Option<f64>, t: f64) -> Vec<usize> {
    (0..plan.blocks.len())
        .filter(|&i| form_at(plan, book_at, i).is_some_and(|at| at <= t && t < at + GLOW_MS))
        .collect()
}

/// When the show is over; None until the book is there.
pub fn end_at(plan: &Plan, book_at: Option<f64>) -> Option<f64> {
    let b = book_at?;
    let n = plan.blocks.len();
    if n == 0 {
        return Some(b);
    }
    form_at(plan, book_at, n - 1).map(|at| at + GLOW_MS)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub typed: usize,
    pub shown: Vec<String>,
    pub formed: usize,
    pub glowing: Vec<usize>,
    pub done: bool,
}

/// Everything the page shows `t` ms in.
pub fn frame_at(plan: &Plan, book_at: Option<f64>, t: f64) -> Frame {
    let typed = typed_at(plan, t);
    Frame {
        typed,
        shown: shown_text(plan, typed),
        formed: formed_at(plan, book_at, t),
        glowing: glowing_at(plan, book_at, t),
        done: end_at(plan, book_at).is_some_and(|end| t >= end),
    }
}

/// When the last key and the last bell sounded (None for never).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Heard {
    pub key_at: Option<f64>,
    pub ding_at: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Due {
    pub key: bool,
    pub ding: bool,
    pub heard: Heard,
}

/// Which sounds are due between two frames.
pub fn sounds_due(plan: &Plan, before: usize, after: usize, t: f64, last: Heard) -> Due {
    let key = after > before && last.key_at.is_none_or(|k| t - k >= KEY_GAP_MS);
    let finished = plan.blocks.iter().any(|b| before < b.end && b.end <= after);
    let ding = finished && last.ding_at.is_none_or(|d| t - d >= DING_GAP_MS);
    Due {
        key,
        ding,
        heard: Heard {
            key_at: if key { Some(t) } else { last.key_at },
            ding_at: if ding { Some(t) } else { last.ding_at },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> Plan {
        plan_show(&[
            "INT. FIELD STATION - DAY",
            "",
            "Rain on the roof.",
            "MARA",
            "Again?",
        ])
    }

    #[test]
    fn empty_paragraphs_are_left_out_and_characters_are_code_points() {
        let plan = page();
        assert_eq!(plan.blocks.len(), 4);
        assert_eq!(plan.blocks[1].start, 24);
        let accents = plan_show(&["é😀"]);
        assert_eq!(accents.total, 2);
        assert!(!has_page(&plan_show(&["", "  "])));
    }

    #[test]
    fn the_typing_takes_four_seconds_whatever_the_page() {
        let plan = page();
        assert_eq!(typed_at(&plan, 0.0), 0);
        assert_eq!(typed_at(&plan, -5.0), 0);
        assert_eq!(typed_at(&plan, TYPE_MS), plan.total);
        assert_eq!(typed_at(&plan, TYPE_MS * 10.0), plan.total);
        let half = typed_at(&plan, TYPE_MS / 2.0);
        assert!(half > 0 && half < plan.total);
        let shown = shown_text(&plan, 26);
        assert_eq!(shown[0], "INT. FIELD STATION - DAY");
        assert_eq!(shown[1], "Ra");
        assert_eq!(shown[2], "");
    }

    #[test]
    fn nothing_forms_before_the_book_and_the_wave_runs_two_blocks_behind() {
        let plan = page();
        assert_eq!(formed_at(&plan, None, TYPE_MS * 2.0), 0);
        assert_eq!(end_at(&plan, None), None);
        // Block 0 forms when block 1 is finished typing.
        let at0 = form_at(&plan, Some(0.0), 0).unwrap();
        assert_eq!(
            at0,
            (plan.blocks[1].end as f64 * TYPE_MS / plan.total as f64).ceil()
        );
        // The last block follows the typing's end at the wave's pace.
        let last = form_at(&plan, Some(0.0), 3).unwrap();
        assert_eq!(last, TYPE_MS + step_for(&plan));
        // A late book is swept in at the wave's pace, not all at once.
        let late = 10_000.0;
        assert_eq!(
            form_at(&plan, Some(late), 2).unwrap(),
            late + 2.0 * step_for(&plan)
        );
    }

    #[test]
    fn a_sweep_never_takes_longer_than_its_cap() {
        let many: Vec<String> = (0..40).map(|i| format!("line {i}")).collect();
        let plan = plan_show(&many);
        assert!(step_for(&plan) * 40.0 <= SWEEP_MS + 1e-9);
        assert_eq!(step_for(&plan_show::<&str>(&[])), STEP_MS);
    }

    #[test]
    fn the_show_ends_with_the_last_glow_and_never_runs_past_its_cap() {
        let plan = page();
        let end = end_at(&plan, Some(0.0)).unwrap();
        assert_eq!(end, form_at(&plan, Some(0.0), 3).unwrap() + GLOW_MS);
        assert!(end <= MAX_SHOW_MS);
        assert!(frame_at(&plan, Some(0.0), end).done);
        assert!(!frame_at(&plan, Some(0.0), end - 1.0).done);
        assert_eq!(end_at(&plan_show::<&str>(&[]), Some(7.0)), Some(7.0));
    }

    #[test]
    fn a_formed_block_glows_for_six_hundred_milliseconds() {
        let plan = page();
        let at = form_at(&plan, Some(0.0), 0).unwrap();
        assert!(glowing_at(&plan, Some(0.0), at).contains(&0));
        assert!(!glowing_at(&plan, Some(0.0), at + GLOW_MS).contains(&0));
    }

    #[test]
    fn keys_are_spaced_and_a_bell_rings_when_a_block_finishes() {
        let plan = page();
        let first = sounds_due(&plan, 0, 3, 100.0, Heard::default());
        assert!(first.key && !first.ding);
        let soon = sounds_due(&plan, 3, 5, 120.0, first.heard);
        assert!(!soon.key, "under 55 ms after the last key");
        let later = sounds_due(&plan, 3, 25, 200.0, first.heard);
        assert!(later.key && later.ding);
        let again = sounds_due(&plan, 25, 42, 400.0, later.heard);
        assert!(!again.ding, "under 700 ms after the last bell");
        let quiet = sounds_due(&plan, 5, 5, 999.0, Heard::default());
        assert!(!quiet.key && !quiet.ding);
    }
}
