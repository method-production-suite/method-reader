//! Each scene heading's `<SceneProperties>`: the page it starts on, from the
//! .fountain's own `= pg N` markers (or the lengths saved at convert time),
//! and its length in eighths, only when those saved lengths are handed in.

mod common;

use std::path::PathBuf;

use common::{XmlElement, parse_xml};
use sp_fdx::{
    SceneMeasure, eighths_label, is_omitted_heading, scene_heading_count, tokens_to_fdx,
    tokens_to_fdx_measured,
};
use sp_model::Token;

fn tokens(source: &str) -> Vec<Token> {
    sp_fountain::tokenize(source).expect("tokenizes")
}

fn oracle_tokens(name: &str) -> Vec<Token> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../oracle")
        .join(name)
        .join("tokens.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The Content block's paragraphs (title page excluded).
fn paragraphs(fdx: &str) -> Vec<XmlElement> {
    let root = parse_xml(fdx);
    root.elements(Some("Content"))[0]
        .elements(Some("Paragraph"))
        .into_iter()
        .cloned()
        .collect()
}

/// (Number, Page, Length) for each Scene Heading paragraph.
type Props = (Option<String>, Option<String>, Option<String>);

fn scene_props(fdx: &str) -> Vec<Props> {
    paragraphs(fdx)
        .iter()
        .filter(|p| p.attr("Type") == Some("Scene Heading"))
        .map(|p| {
            let props = p.elements(Some("SceneProperties"));
            let get = |name: &str| props.first().and_then(|s| s.attr(name)).map(str::to_owned);
            (
                p.attr("Number").map(str::to_owned),
                get("Page"),
                get("Length"),
            )
        })
        .collect()
}

fn pages(fdx: &str) -> Vec<Option<String>> {
    scene_props(fdx).into_iter().map(|(_, p, _)| p).collect()
}

fn s(v: &str) -> Option<String> {
    Some(v.to_owned())
}

const TWO_PAGES: &str = "INT. A - DAY\n\nOne.\n\n= pg 2\n\nINT. B - NIGHT\n\nTwo.\n";

#[test]
fn a_scene_starts_on_the_page_its_last_marker_names() {
    let fdx = tokens_to_fdx(&tokens(TWO_PAGES));
    assert_eq!(pages(&fdx), [s("1"), s("2")]);
}

#[test]
fn a_scene_before_the_first_marker_is_on_the_page_before_it() {
    // A script whose printed numbering starts at 4 (its first marker is 5).
    let fdx = tokens_to_fdx(&tokens(
        "INT. A - DAY\n\nOne.\n\n= pg 5\n\nMore.\n\n= pg 6\n\nINT. B - DAY\n\nTwo.\n",
    ));
    assert_eq!(pages(&fdx), [s("4"), s("6")]);
}

#[test]
fn a_skipped_page_number_is_carried_as_printed() {
    let fdx = tokens_to_fdx(&tokens(
        "INT. A - DAY\n\n= pg 2\n\nx\n\n= pg 4\n\nINT. B - DAY\n\ny\n",
    ));
    assert_eq!(pages(&fdx), [s("1"), s("4")]);
}

#[test]
fn a_script_with_no_markers_names_no_page() {
    // Markers switched off, or a one-page script: the .fountain cannot say,
    // so the FDX says nothing (and stays the TypeScript writer's bytes).
    let fdx = tokens_to_fdx(&tokens("INT. A - DAY\n\nOne.\n"));
    assert!(!fdx.contains("SceneProperties"), "{fdx}");
}

#[test]
fn an_ordinary_synopsis_is_not_a_page_marker() {
    let fdx = tokens_to_fdx(&tokens("INT. A - DAY\n\n= The setup\n\nOne.\n"));
    assert!(!fdx.contains("SceneProperties"), "{fdx}");
}

#[test]
fn scene_properties_come_before_the_heading_s_text_as_final_draft_writes_them() {
    let fdx = tokens_to_fdx_measured(
        &tokens(TWO_PAGES),
        Some(&[
            SceneMeasure {
                page: s("1"),
                eighths: 10,
            },
            SceneMeasure {
                page: s("2"),
                eighths: 3,
            },
        ]),
    );
    assert!(
        fdx.contains(
            "<Paragraph Type=\"Scene Heading\"><SceneProperties Length=\"1 2/8\" Page=\"1\"/>\
             <Text>INT. A - DAY</Text></Paragraph>"
        ),
        "{fdx}"
    );
    assert_eq!(
        scene_props(&fdx),
        [(None, s("1"), s("1 2/8")), (None, s("2"), s("3/8"))]
    );
}

#[test]
fn saved_pages_win_over_the_markers() {
    let fdx = tokens_to_fdx_measured(
        &tokens(TWO_PAGES),
        Some(&[
            SceneMeasure {
                page: s("7"),
                eighths: 8,
            },
            SceneMeasure {
                page: None,
                eighths: 0,
            },
        ]),
    );
    // A saved entry with no page falls back to the markers.
    assert_eq!(
        scene_props(&fdx),
        [(None, s("7"), s("1")), (None, s("2"), s("0/8"))]
    );
}

#[test]
fn lengths_for_a_different_script_are_not_used() {
    // One measure for two headings: they were measured for another text.
    let fdx = tokens_to_fdx_measured(
        &tokens(TWO_PAGES),
        Some(&[SceneMeasure {
            page: s("9"),
            eighths: 4,
        }]),
    );
    assert_eq!(
        scene_props(&fdx),
        [(None, s("1"), None), (None, s("2"), None)]
    );
}

#[test]
fn a_shot_has_no_scene_properties() {
    let t = tokens("INT. A - DAY\n\n.THE DESK\n\nOne.\n\n= pg 2\n\nTwo.\n");
    assert_eq!(scene_heading_count(&t), 1);
    let fdx = tokens_to_fdx(&t);
    let shot = paragraphs(&fdx)
        .into_iter()
        .find(|p| p.attr("Type") == Some("Shot"))
        .expect("a shot");
    assert!(shot.elements(Some("SceneProperties")).is_empty());
}

#[test]
fn eighths_are_written_the_way_final_draft_writes_them() {
    let cases = [
        (0, "0/8"),
        (1, "1/8"),
        (6, "6/8"),
        (7, "7/8"),
        (8, "1"),
        (10, "1 2/8"),
        (16, "2"),
        (23, "2 7/8"),
    ];
    for (eighths, want) in cases {
        assert_eq!(eighths_label(eighths), want, "{eighths}");
    }
}

#[test]
fn omitted_scenes_are_recognised() {
    for text in [
        "OMITTED",
        "omitted",
        "OMIT",
        "SCENE OMITTED",
        "Scenes omitted.",
        " OMITTED ",
    ] {
        assert!(is_omitted_heading(text), "{text:?}");
    }
    for text in [
        "INT. OMITTED BAR - DAY",
        "OMITTED BY REQUEST",
        "EXT. FIELD - DAY",
    ] {
        assert!(!is_omitted_heading(text), "{text:?}");
    }
}

// The committed fixtures, from the oracle's own tokens (no saved lengths).

#[test]
fn field_station_s_scenes_start_where_its_markers_say() {
    let fdx = tokens_to_fdx(&oracle_tokens("field-station"));
    let want = ["1", "2", "4", "5", "7", "8", "10", "11", "13", "14", "16"];
    assert_eq!(
        scene_props(&fdx),
        want.iter()
            .map(|p| (None, s(p), None))
            .collect::<Vec<Props>>()
    );
}

#[test]
fn torture_s_scenes_carry_their_pages_and_its_one_printed_scene_number() {
    let t = oracle_tokens("torture");
    let fdx = tokens_to_fdx(&t);
    let props = scene_props(&fdx);
    assert_eq!(props.len(), scene_heading_count(&t));
    let want_pages = [
        "1", "1", "1", "1", "2", "2", "2", "3", "4", "5", "7", "8", "14", "15", "16",
    ];
    assert_eq!(
        props.iter().map(|(_, p, _)| p.clone()).collect::<Vec<_>>(),
        want_pages.iter().map(|p| s(p)).collect::<Vec<_>>()
    );
    // Only the reshelving scene is numbered in the PDF, and the number rides.
    let numbered: Vec<&Props> = props.iter().filter(|(n, _, _)| n.is_some()).collect();
    assert_eq!(numbered, [&(s("42"), s("1"), None)]);
}
