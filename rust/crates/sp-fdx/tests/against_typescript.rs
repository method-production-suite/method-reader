//! Byte parity on small inputs: every recorded inline Fountain source,
//! written by the Rust writer, is the TypeScript writer's FDX of it. The
//! fixtures cover whole scripts (tests/oracle.rs); these cover the corners a
//! script rarely reaches: emphasis that nests, overlaps or will not pair,
//! escapes, sentinels and forbidden characters arriving as text, line
//! endings, and whitespace JavaScript and Rust disagree about.

mod common;

use common::{fountain_to_fdx, recorded_sources, typescript_fdx, without_scene_properties};

/// Corners of the writer, beyond the behavior tests' own sources.
const CORNERS: &[&str] = &[
    "***a** b*\n",
    "**a *b** c*\n",
    "*a **b* c**\n",
    "_a_b_\n",
    "*a*b*\n",
    "****a****\n",
    "**x***\n",
    "***\n",
    "a * b * c\n",
    "*a *b* c*\n",
    "_*a*_ and *_b_* and __c__\n",
    "**_a_ *b* _**c**_**\n",
    "\\\\*x*\n",
    "a \\_b\\_ c \\\\_d_\n",
    "plain\u{FDD0} sentinel\u{FDD6} text \u{FDEF}*x*\n",
    "lead *\u{FEFF}x* and *x\u{FEFF}* and *x\u{85}*\n",
    "nbsp *\u{A0}x* and *y\u{3000}*\n",
    "INT. ROOM - DAY #A\u{1}\"<&>#\n",
    "MARGO\nLine one\r\nline two\n",
    "Action\rwith a bare return.\n",
    "MARGO\n\u{3000}\nafter a wide space\n",
    "Before [[a\nnote]] after.\n",
    "Before [[a]]   [[b]]  after.\n",
    "MARGO\n  [[fmt: mono]]   spaced\n",
    "> FADE OUT.\n",
    "CUT TO:\n\n===\n\nBOB\nHi.\n\nALICE ^\nHey.\n\n===\n",
    "BOB ^\nNobody on the left.\n",
    "Title: Only\nAuthor: One\n\nAction.\n",
    "Contact: Only a contact\n\nAction.\n",
    "Notes: a note\nCopyright: (c) nobody\nRevision: Blue\nDate: 2020\n\nAction.\n",
    "Title:\n    Line one\n    Line two\n    Line three\nCredit: by\nAuthors: A and B\nSource: a book\nDraft date: today\nContact:\n    one\n    two\n    three\n    four\n    five\n\nINT. X - DAY\n",
    "Title: [[only a note]]\n\nAction.\n",
    "Weird: key\n\nAction.\n",
    "~Row *row* row\n~_your_ boat\n",
    "> *THE* **END** <\n",
    ".A SHOT #3#\n\nINT./EXT. CAR - DAY #4#\n\nI/E. VAN #5#\n\nEST. CITY\n",
];

#[test]
fn every_corner_matches_the_typescript() {
    let mut differ = Vec::new();
    for &source in CORNERS {
        let got = fountain_to_fdx(source);
        if let Some(want) = typescript_fdx(source)
            && got != want
        {
            differ.push(format!("{source:?}\n--- got\n{got}\n--- want\n{want}"));
        }
    }
    assert!(differ.is_empty(), "{}", differ.join("\n\n"));
}

/// Every recorded source is the TypeScript's FDX byte for byte, except that
/// a scene heading now carries its SceneProperties (rust/PARITY.md,
/// Decisions, 2026-10-16). Only a source with a `= pg N` marker can name a
/// page, and taking the SceneProperties out again gives back the
/// TypeScript's bytes exactly, so nothing else moved.
#[test]
fn every_recorded_source_matches_the_typescript() {
    let sources = recorded_sources();
    assert!(sources.len() >= CORNERS.len());
    let mut with_pages = 0;
    for source in sources {
        let got = fountain_to_fdx(source);
        let want = typescript_fdx(source).unwrap();
        if got != want {
            assert!(
                source.contains("= pg "),
                "{source:?}: only a marker names a page"
            );
            with_pages += 1;
        }
        assert_eq!(without_scene_properties(&got), want, "{source:?}");
    }
    // The one recorded source with a page marker.
    assert_eq!(with_pages, 1);
}
