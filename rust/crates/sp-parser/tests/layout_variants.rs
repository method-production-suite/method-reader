//! Field Station, the invented script, re-laid out the ways real scripts
//! arrive (A4 paper, wider or narrower margins, Word's one-inch margins,
//! names centred on the page, dialogue close to the names), and read again.
//! A layout is only a different place for the same script, so every element
//! must classify as it does on the original page.

use sp_parser::extract::extract_text;
use sp_parser::facts::DocumentFacts;
use sp_parser::parse_lines;

fn field_station() -> DocumentFacts {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tests/fixtures/field-station.pdf");
    sp_pdf::read(std::fs::read(path).expect("field-station.pdf")).expect("a PDF")
}

/// Field Station's own columns, in points from the page's left edge.
#[derive(Clone, Copy)]
enum Column {
    Action,
    Dialogue,
    Parenthetical,
    Cue,
    Right,
}

fn column_of(x: f64) -> Column {
    match x {
        x if x < 150.0 => Column::Action,
        x if x < 205.0 => Column::Dialogue,
        x if x < 245.0 => Column::Parenthetical,
        x if x < 330.0 => Column::Cue,
        _ => Column::Right,
    }
}

struct Layout {
    name: &'static str,
    width: f64,
    /// Where each column starts on the new page, given its start on the old.
    place: fn(Column, f64, f64) -> f64,
}

/// A whole line keeps its own column: items after the first in a line
/// move by the same amount as the first.
fn relaid(facts: &DocumentFacts, layout: &Layout) -> DocumentFacts {
    let mut out = facts.clone();
    for page in &mut out.pages {
        let old_width = page.width;
        page.width = layout.width;
        // Group items by baseline so a line moves as one.
        let mut ys: Vec<f64> = page.items.iter().map(|i| i.transform[5]).collect();
        ys.sort_by(f64::total_cmp);
        ys.dedup_by(|a, b| (*a - *b).abs() < 0.5);
        for y in ys {
            let idx: Vec<usize> = (0..page.items.len())
                .filter(|&i| (page.items[i].transform[5] - y).abs() < 0.5)
                .collect();
            let Some(&first) = idx.iter().min_by(|a, b| {
                page.items[**a].transform[4].total_cmp(&page.items[**b].transform[4])
            }) else {
                continue;
            };
            let x0 = page.items[first].transform[4];
            let line_width: f64 = idx
                .iter()
                .map(|&i| page.items[i].transform[4] + page.items[i].width.unwrap_or(0.0))
                .fold(0.0, f64::max)
                - x0;
            let x1 = match column_of(x0) {
                Column::Right => layout.width - (old_width - x0),
                c => (layout.place)(c, x0, line_width),
            };
            for i in idx {
                page.items[i].transform[4] += x1 - x0;
            }
        }
    }
    out
}

fn kinds(facts: &DocumentFacts) -> Vec<String> {
    let lines = extract_text(facts, None, None).text.lines;
    parse_lines(&lines, &[])
        .elements
        .iter()
        .map(|e| format!("{:?} {}", e.kind, e.text))
        .collect()
}

const LAYOUTS: &[Layout] = &[
    Layout {
        name: "a4 paper",
        width: 595.0,
        place: |_, x, _| x,
    },
    Layout {
        name: "margins quarter-inch wider",
        width: 612.0,
        place: |_, x, _| x + 18.0,
    },
    Layout {
        name: "margins quarter-inch narrower",
        width: 612.0,
        place: |_, x, _| x - 18.0,
    },
    Layout {
        name: "word, one-inch margins",
        width: 612.0,
        place: |c, x, _| match c {
            Column::Action => x - 36.0,
            Column::Dialogue => 144.0,
            Column::Parenthetical => 180.0,
            Column::Cue => 216.0,
            Column::Right => x,
        },
    },
    Layout {
        name: "word, one-inch margins, A4",
        width: 595.0,
        place: |c, x, _| match c {
            Column::Action => x - 36.0,
            Column::Dialogue => 144.0,
            Column::Parenthetical => 180.0,
            Column::Cue => 216.0,
            Column::Right => x,
        },
    },
    Layout {
        name: "word, names close in (2.75in)",
        width: 612.0,
        place: |c, x, _| match c {
            Column::Action => x - 36.0,
            Column::Dialogue => 126.0,
            Column::Parenthetical => 162.0,
            Column::Cue => 198.0,
            Column::Right => x,
        },
    },
    Layout {
        name: "deep columns (dialogue 3in, names 4.5in)",
        width: 612.0,
        place: |c, x, _| match c {
            Column::Action => x + 18.0,
            Column::Dialogue => 216.0,
            Column::Parenthetical => 252.0,
            Column::Cue => 324.0,
            Column::Right => x,
        },
    },
    Layout {
        name: "names centred on the page",
        width: 612.0,
        place: |c, x, w| match c {
            Column::Cue => 306.0 - w / 2.0,
            _ => x,
        },
    },
    Layout {
        name: "dialogue close to the names",
        width: 612.0,
        place: |c, x, _| match c {
            Column::Dialogue => x + 22.0,
            Column::Parenthetical => x + 18.0,
            _ => x,
        },
    },
];

#[test]
fn a_script_already_on_the_standard_columns_is_left_exactly_as_it_was() {
    use sp_parser::layout::{calibrate, fits, measure};
    let lines = extract_text(&field_station(), None, None).text.lines;
    let c = measure(&lines).expect("Field Station has plenty of speeches");
    assert!(fits(&c), "{c:?}");
    assert!(matches!(calibrate(&lines), std::borrow::Cow::Borrowed(_)));
}

#[test]
fn a_script_too_short_to_measure_is_left_alone() {
    use sp_parser::layout::{MIN_SPEECHES, calibrate, measure};
    let word = &LAYOUTS[3];
    assert_eq!(word.name, "word, one-inch margins");
    let lines = extract_text(&relaid(&field_station(), word), None, None)
        .text
        .lines;
    // Cut the script off before its MIN_SPEECHES-th speech.
    let cues = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| sp_parser::cue::is_cue_text(&l.text))
        .map(|(i, _)| i)
        .nth(MIN_SPEECHES / 2)
        .unwrap();
    let short = &lines[..cues];
    assert_eq!(measure(short), None);
    assert!(matches!(calibrate(short), std::borrow::Cow::Borrowed(_)));
}

#[test]
fn lines_extraction_placed_are_never_moved() {
    use sp_parser::layout::calibrate;
    let word = &LAYOUTS[3];
    let mut lines = extract_text(&relaid(&field_station(), word), None, None)
        .text
        .lines;
    lines[40].placed = true;
    let before = lines[40].indent;
    let after = calibrate(&lines);
    assert!(
        matches!(after, std::borrow::Cow::Owned(_)),
        "the Word layout is translated"
    );
    assert_eq!(after[40].indent, before);
}

#[test]
fn every_layout_reads_as_the_original_page() {
    let original = field_station();
    let want = kinds(&original);
    let mut report = Vec::new();
    for layout in LAYOUTS {
        let got = kinds(&relaid(&original, layout));
        let differ =
            want.iter().zip(&got).filter(|(a, b)| a != b).count() + want.len().abs_diff(got.len());
        report.push(format!(
            "{:<32} {differ} of {} differ",
            layout.name,
            want.len()
        ));
        if std::env::var("SHOW").is_ok() {
            for (a, b) in want.iter().zip(&got).filter(|(a, b)| a != b).take(8) {
                report.push(format!("      was {a}\n      now {b}"));
            }
        }
    }
    println!("{}", report.join("\n"));
    assert!(
        report
            .iter()
            .all(|r| !r.starts_with(' ') && r.contains(" 0 of ")),
        "{}",
        report.join("\n")
    );
}
