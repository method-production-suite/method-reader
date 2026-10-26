//! Scene lengths in eighths, measured from the parser's geometry, on pages
//! built by hand: 55 lines a page at a 12 pt pitch, baselines from 720 down
//! to 72 (a one-inch bottom margin), so the text area is 660 pt and an
//! eighth is 82.5 pt (6.875 lines).

use serde_json::json;
use sp_engine::scene_lengths::{measure_scenes, round_eighths};
use sp_model::{ParsedScreenplay, RawImage, RawLine, ScreenplayElement};

const TOP: f64 = 720.0;
const PITCH: f64 = 12.0;
const LINES: usize = 55;

fn baseline(line: usize) -> f64 {
    TOP - PITCH * line as f64
}

fn raw(page: u32, line: usize) -> RawLine {
    serde_json::from_value(json!({
        "text": "x", "indent": 10, "y": baseline(line), "pageNum": page
    }))
    .unwrap()
}

fn el(kind: &str, text: &str, page: u32, y: f64) -> ScreenplayElement {
    serde_json::from_value(json!({
        "type": kind, "text": text, "id": "e", "pageNum": page,
        "isTitlePage": false, "isReadable": true, "y": y
    }))
    .unwrap()
}

/// A page's elements as (kind, text, first line), each owning the lines
/// down to the next one; `filled` is how many of the 55 lines carry text.
struct Page {
    elements: Vec<(&'static str, &'static str, usize)>,
    filled: usize,
}

fn script(pages: &[Page]) -> (Vec<RawLine>, ParsedScreenplay) {
    let mut lines = Vec::new();
    let mut elements = Vec::new();
    for (i, page) in pages.iter().enumerate() {
        let n = i as u32 + 1;
        for line in 0..page.filled {
            lines.push(raw(n, line));
        }
        for &(kind, text, line) in &page.elements {
            elements.push(el(kind, text, n, baseline(line)));
        }
    }
    let sp = ParsedScreenplay {
        elements,
        characters: Vec::new(),
        scenes: Vec::new(),
        page_count: pages.len() as u32,
    };
    (lines, sp)
}

fn eighths(pages: &[Page]) -> Vec<u32> {
    let (lines, sp) = script(pages);
    measure_scenes(&lines, &[], &|_| Some(792.0), &sp)
        .expect("measured")
        .into_iter()
        .map(|m| m.eighths)
        .collect()
}

fn full(elements: Vec<(&'static str, &'static str, usize)>) -> Page {
    Page {
        elements,
        filled: LINES,
    }
}

#[test]
fn a_scene_filling_two_pages_is_two_pages() {
    let got = eighths(&[
        full(vec![("scene", "INT. A - DAY", 0), ("action", "a", 2)]),
        full(vec![("action", "b", 0)]),
    ]);
    assert_eq!(got, [16]);
}

#[test]
fn two_scenes_sharing_a_page_split_it_where_the_second_heading_sits() {
    // 27 and 28 lines: half a page apiece.
    let got = eighths(&[full(vec![
        ("scene", "INT. A - DAY", 0),
        ("action", "a", 2),
        ("scene", "INT. B - DAY", 27),
        ("action", "b", 29),
    ])]);
    assert_eq!(got, [4, 4]);
}

#[test]
fn a_scene_crossing_a_page_counts_its_part_of_each() {
    // A: lines 0..40 of page 1 (40 lines); B: lines 40..55 of page 1 and
    // the top 20 lines of page 2 (15 + 20 = 35 lines); C: the other 35.
    let got = eighths(&[
        full(vec![
            ("scene", "INT. A - DAY", 0),
            ("action", "a", 2),
            ("scene", "INT. B - DAY", 40),
            ("action", "b", 42),
        ]),
        full(vec![
            ("action", "b", 0),
            ("scene", "INT. C - DAY", 20),
            ("action", "c", 22),
        ]),
    ]);
    // 40 lines = 5.82/8; 35 lines = 5.09/8; 35 lines = 5.09/8.
    assert_eq!(got, [6, 5, 5]);
}

#[test]
fn a_scene_shorter_than_an_eighth_is_still_an_eighth() {
    let got = eighths(&[full(vec![
        ("scene", "INT. A - DAY", 0),
        ("scene", "INT. B - DAY", 2),
        ("action", "b", 4),
    ])]);
    assert_eq!(got[0], 1);
}

#[test]
fn an_omitted_scene_is_none_at_all() {
    let got = eighths(&[full(vec![
        ("scene", "INT. A - DAY", 0),
        ("action", "a", 2),
        ("scene", "OMITTED", 20),
        ("scene", "INT. B - DAY", 22),
        ("action", "b", 24),
    ])]);
    assert_eq!(got[1], 0);
}

#[test]
fn the_last_scene_ends_where_its_text_does() {
    // A short last page: 13 lines of text (1.89 eighths) is two.
    let got = eighths(&[
        full(vec![("scene", "INT. A - DAY", 0), ("action", "a", 2)]),
        full(vec![("action", "a", 0)]),
        Page {
            elements: vec![("scene", "INT. B - DAY", 0), ("action", "b", 2)],
            filled: 13,
        },
    ]);
    assert_eq!(got, [16, 2]);
}

#[test]
fn pages_that_stop_short_of_the_bottom_margin_are_short_of_a_page() {
    // Every page stops at 48 lines: the text area still runs to the
    // one-inch margin, so each page is 576 / 660 of a page, 7/8.
    let short = |elements| Page {
        elements,
        filled: 48,
    };
    let got = eighths(&[
        short(vec![("scene", "INT. A - DAY", 0), ("action", "a", 2)]),
        short(vec![("scene", "INT. B - DAY", 0), ("action", "b", 2)]),
    ]);
    assert_eq!(got, [7, 7]);
}

#[test]
fn text_that_runs_below_an_inch_widens_the_text_area() {
    // 60 lines a page, the last baseline at 12 pt: a full page is 8/8.
    let long = |elements| Page {
        elements,
        filled: 60,
    };
    let got = eighths(&[
        long(vec![("scene", "INT. A - DAY", 0), ("action", "a", 2)]),
        long(vec![("scene", "INT. B - DAY", 0), ("action", "b", 2)]),
    ]);
    assert_eq!(got, [8, 8]);
}

#[test]
fn page_furniture_takes_no_room() {
    // A printed page number above the text and a footer line below it:
    // neither is the scene's, and neither moves the text area.
    let pages = [
        full(vec![("scene", "INT. A - DAY", 0), ("action", "a", 2)]),
        full(vec![("scene", "INT. B - DAY", 0), ("action", "b", 2)]),
    ];
    let (mut lines, mut sp) = script(&pages);
    for page in [1u32, 2] {
        lines.push(
            serde_json::from_value(json!({
                "text": "2.", "indent": 80, "y": 750.0, "pageNum": page
            }))
            .unwrap(),
        );
        lines.push(
            serde_json::from_value(json!({
                "text": "FOOTER", "indent": 40, "y": 40.0, "pageNum": page
            }))
            .unwrap(),
        );
        sp.elements.push(el("page-number", "2.", page, 750.0));
        sp.elements.push(el("page-number", "FOOTER", page, 40.0));
    }
    lines.sort_by(|a, b| a.page_num.cmp(&b.page_num).then(b.y.total_cmp(&a.y)));
    sp.elements.sort_by(|a, b| {
        a.page_num
            .cmp(&b.page_num)
            .then(b.y.unwrap().total_cmp(&a.y.unwrap()))
    });
    let got: Vec<u32> = measure_scenes(&lines, &[], &|_| Some(792.0), &sp)
        .unwrap()
        .into_iter()
        .map(|m| m.eighths)
        .collect();
    assert_eq!(got, [8, 8]);
}

#[test]
fn a_scene_s_page_is_the_printed_page_number() {
    // Sheet 1 is unnumbered, sheet 2 prints "2.": the offset is 0 here,
    // and a script whose sheet 2 prints "5." starts its scenes there.
    let (mut lines, mut sp) = script(&[
        full(vec![("scene", "INT. A - DAY", 0), ("action", "a", 2)]),
        full(vec![("scene", "INT. B - DAY", 0), ("action", "b", 2)]),
    ]);
    lines.insert(
        LINES,
        serde_json::from_value(json!({"text": "5.", "indent": 80, "y": 750.0, "pageNum": 2}))
            .unwrap(),
    );
    sp.elements.insert(2, el("page-number", "5.", 2, 750.0));
    let pages: Vec<Option<String>> = measure_scenes(&lines, &[], &|_| Some(792.0), &sp)
        .unwrap()
        .into_iter()
        .map(|m| m.page)
        .collect();
    assert_eq!(pages, [Some("4".to_owned()), Some("5".to_owned())]);
}

#[test]
fn a_picture_takes_the_room_it_is_drawn_in() {
    // Page 2 is a scene heading and a picture half the page tall (396 pt
    // of a 792 pt sheet), then nothing.
    let (lines_a, mut sp) = script(&[
        full(vec![("scene", "INT. A - DAY", 0), ("action", "a", 2)]),
        Page {
            elements: vec![("scene", "INT. B - DAY", 0)],
            filled: 1,
        },
    ]);
    let mut picture = el("image", "", 2, baseline(2) + PITCH);
    picture.img = Some(
        serde_json::from_value(json!({"file": "p002-01.png", "widthPct": 50, "fullPage": false}))
            .unwrap(),
    );
    sp.elements.push(picture);
    let images: Vec<RawImage> = vec![
        serde_json::from_value(json!({
            "file": "p002-01.png", "pageNum": 2, "y": baseline(2) + PITCH,
            "pageWidthPct": 50, "pageHeightPct": 50
        }))
        .unwrap(),
    ];
    let got: Vec<u32> = measure_scenes(&lines_a, &images, &|_| Some(792.0), &sp)
        .unwrap()
        .into_iter()
        .map(|m| m.eighths)
        .collect();
    // (the heading's 24 pt above the picture's top + 396 pt of picture)
    // / 82.5 = 5.09: five eighths.
    assert_eq!(got, [8, 5]);
}

#[test]
fn elements_without_a_position_cannot_be_measured() {
    let (lines, mut sp) = script(&[full(vec![("scene", "INT. A - DAY", 0)])]);
    sp.elements[0].y = None;
    assert_eq!(measure_scenes(&lines, &[], &|_| Some(792.0), &sp), None);
}

#[test]
fn rounding_is_to_the_nearest_eighth_halves_up_never_below_one() {
    let cases = [
        (0.0, 1),
        (0.2, 1),
        (1.49, 1),
        (1.5, 2),
        (2.5, 3),
        (7.6, 8),
        (12.4, 12),
    ];
    for (raw, want) in cases {
        assert_eq!(round_eighths(raw), want, "{raw}");
    }
}
