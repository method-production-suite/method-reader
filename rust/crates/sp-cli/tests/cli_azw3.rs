//! Port of tests/cli-azw3.test.ts: the conversion's own --azw3, Method Reader's
//! AZW3 of the EPUB the same run wrote, and `export --for azw3`, which
//! needs nothing installed. Read back with sp-azw3's structural KF8 reader
//! (independent of the writer).

mod common;

#[allow(dead_code)]
#[path = "../../sp-azw3/tests/common/kf8_reader.rs"]
mod kf8_reader;
#[allow(dead_code)]
#[path = "../../sp-azw3/tests/common/palmdoc_decode.rs"]
mod palmdoc_decode;

use common::Sandbox;
use kf8_reader::read_kf8;

const FOUNTAIN: &str = "Title: Test\n\nINT. ROOM - DAY\n\nA line of action.\n\nJO\nA line.\n\nEXT. ROAD - NIGHT\n\nRain.\n";

/// The book time both sides use, so only the PDB dates could differ.
const EPOCH: u64 = 1_767_225_600;

/// A copy with the PDB header's creation and modification dates zeroed.
fn mask_pdb_dates(b: &[u8]) -> Vec<u8> {
    let mut c = b.to_vec();
    c[36..44].fill(0);
    c
}

#[test]
fn a_pdf_conversion_writes_the_azw3_of_the_epub_it_wrote_and_names_it_in_json() {
    let mut sb = Sandbox::new();
    sb.set("SOURCE_DATE_EPOCH", EPOCH.to_string());
    let epub = sb.p("Converted.epub");
    let azw3 = sb.p("Converted.azw3");
    let pdf = common::fixture("screenplay.pdf");
    let ran = sb.run(&[
        &pdf.to_string_lossy(),
        "-o",
        &epub,
        "--azw3",
        &azw3,
        "--json",
    ]);
    assert_eq!(ran.code, 0, "{}", ran.stdout);
    assert_eq!(ran.json()["azw3Path"], azw3.as_str());
    let bytes = std::fs::read(&azw3).unwrap();
    // The same book the EPUB is: the writer on that EPUB, byte for byte but
    // for the PalmDB dates.
    let rebuilt = sp_azw3::build_azw3_at(&std::fs::read(&epub).unwrap(), EPOCH).unwrap();
    assert_eq!(mask_pdb_dates(&bytes), mask_pdb_dates(&rebuilt));
    assert_eq!(read_kf8(&bytes).unwrap().ncx.len(), 5);
}

#[test]
fn a_fountain_conversion_writes_one_too_and_says_so() {
    let sb = Sandbox::new();
    let source = sb.write("Script.fountain", FOUNTAIN);
    let azw3 = sb.p("Out.azw3");
    let ran = sb.run(&[
        &source.to_string_lossy(),
        "-o",
        &sb.p("Out.epub"),
        "--azw3",
        &azw3,
    ]);
    assert_eq!(ran.code, 0, "{}", ran.stderr);
    assert!(
        ran.stdout.contains(&format!("wrote {azw3}")),
        "{}",
        ran.stdout
    );
    let labels: Vec<String> = read_kf8(&std::fs::read(&azw3).unwrap())
        .unwrap()
        .ncx
        .into_iter()
        .map(|n| n.label)
        .collect();
    assert_eq!(labels, ["INT. ROOM - DAY", "EXT. ROAD - NIGHT"]);
}

#[test]
fn without_azw3_no_azw3_is_written_and_the_answer_has_no_azw3_path() {
    let sb = Sandbox::new();
    let source = sb.write("Script.fountain", FOUNTAIN);
    let ran = sb.run(&[&source.to_string_lossy(), "-o", &sb.p("Out.epub"), "--json"]);
    assert!(ran.json().get("azw3Path").is_none());
    assert!(
        !common::list_dir(&sb.root())
            .iter()
            .any(|f| f.ends_with(".azw3"))
    );
}

#[test]
fn export_for_azw3_makes_it_beside_the_book_with_no_calibre_and_says_it_is_beta() {
    // The sandbox points Calibre at a guard path: were the export to reach
    // for Calibre, it could not start.
    let sb = Sandbox::new();
    let source = sb.write("Script.fountain", FOUNTAIN);
    let epub = sb.p("Script.epub");
    assert_eq!(sb.run(&[&source.to_string_lossy(), "-o", &epub]).code, 0);
    let ran = sb.run(&["export", &epub, "--for", "azw3", "--json"]);
    assert_eq!(ran.code, 0, "{}", ran.stdout);
    let answer = ran.json();
    assert_eq!(answer["ok"], true);
    assert_eq!(answer["path"], sb.p("Script.azw3").as_str());
    assert_eq!(answer["extension"], "azw3");
    assert!(answer["label"].as_str().unwrap().contains("(beta)"));
    let bytes = std::fs::read(answer["path"].as_str().unwrap()).unwrap();
    assert_eq!(read_kf8(&bytes).unwrap().header.version, 8);
}
