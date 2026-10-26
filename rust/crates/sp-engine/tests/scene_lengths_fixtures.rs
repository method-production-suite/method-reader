//! Scene lengths measured on the committed invented scripts. The numbers
//! were checked by hand against the rendered PDF pages: an eighth of these
//! US Letter sheets is 82 to 83 pt of text area (the median first line's
//! top down to the one-inch margin), so for example field-station's
//! printed page 4, one scene from the top line to a last baseline about
//! 1.8 inches above the foot of the sheet, is 7/8, and torture's first
//! page, four scenes of three or four lines each, is 1/8 apiece.

mod common;

use sp_engine::convert::{ConvertOptions, ConvertResult, convert_pdf};

fn convert(name: &str) -> ConvertResult {
    let pdf = std::fs::read(common::repo_root().join("tests/fixtures").join(name)).unwrap();
    convert_pdf(pdf, ConvertOptions::default()).expect("converts")
}

fn measured(result: &ConvertResult) -> Vec<(&str, u32)> {
    result
        .scene_measures
        .as_ref()
        .expect("measured")
        .iter()
        .map(|m| (m.page.as_deref().unwrap_or(""), m.eighths))
        .collect()
}

/// The pages the FDX of the bare .fountain names, from its markers.
fn marker_pages(result: &ConvertResult) -> Vec<String> {
    let tokens = sp_fountain::tokenize(&result.fountain_text).unwrap();
    let fdx = sp_fdx::tokens_to_fdx(&tokens);
    fdx.split("<SceneProperties Page=\"")
        .skip(1)
        .map(|rest| rest[..rest.find('"').unwrap()].to_owned())
        .collect()
}

#[test]
fn field_station_s_scenes() {
    let r = convert("field-station.pdf");
    assert_eq!(
        measured(&r),
        [
            ("1", 7),
            ("2", 14),
            ("4", 7),
            ("5", 11),
            ("7", 7),
            ("8", 8),
            ("10", 7),
            ("11", 11),
            ("13", 5),
            ("14", 14),
            ("16", 6),
        ]
    );
    // The saved page and the .fountain's own marker agree on every scene.
    let pages: Vec<&str> = measured(&r).iter().map(|(p, _)| *p).collect();
    assert_eq!(marker_pages(&r), pages);
}

#[test]
fn torture_s_scenes() {
    let r = convert("torture.pdf");
    assert_eq!(
        measured(&r),
        [
            // Page 1: four short scenes, the last the numbered one.
            ("1", 1),
            ("1", 1),
            ("1", 1),
            ("1", 1),
            // Page 2: the scene with two shots (they are its lines, not
            // scenes), then two short ones.
            ("2", 2),
            ("2", 1),
            ("2", 1),
            ("3", 4),
            ("4", 3),
            ("5", 8),
            ("7", 5),
            // From printed page 8 to the foot of page 13.
            ("8", 36),
            ("14", 3),
            ("15", 8),
            ("16", 8),
        ]
    );
    let pages: Vec<&str> = measured(&r).iter().map(|(p, _)| *p).collect();
    assert_eq!(marker_pages(&r), pages);
}

#[test]
fn a_fountain_has_nothing_to_measure() {
    let r = sp_engine::convert::convert_fountain(
        "INT. A - DAY\n\nOne.\n",
        ConvertOptions::default(),
        None,
    )
    .unwrap();
    assert_eq!(r.scene_measures, None);
}
