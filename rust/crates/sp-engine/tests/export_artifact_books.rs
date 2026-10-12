//! The checks of tests/export-artifact.test.ts that read a real KF8 or MOBI
//! back (`readKf8`, `mobiPictures`), now that `Books` is the real book
//! makers: sp-azw3 for the AZW3, the conversion for the MOBI. The ladder's
//! decisions against stand-in makers are export_artifact.rs.
//!
//! No test here reaches Calibre: the KFX builder refuses, as with no
//! Calibre on the machine, and the AZW3 and MOBI are the engine's own work.

mod common;

use std::collections::BTreeMap;

use common::books::age;
use common::epub::image_entries;
use common::kf8_reader::read_kf8;
use common::mobi_records::mobi_pictures;
use common::{Scratch, names};
use sp_engine::convert::{ConvertOptions, convert_fountain};
use sp_engine::errors::EngineError;
use sp_engine::export::artifact::{
    FreshKindleArtifactOptions, RungBuilders, azw3_sibling, fresh_artifact_of,
    fresh_kindle_artifact, kindle_artifact_plan, mobi_sibling, rung_plan, write_azw3,
};
use sp_engine::export::formats::{KindleRung, ToolchainState};
use sp_engine::export::renderers::Books;
use sp_engine::nodepath::host;
use sp_mobi::MOBI_CARRIES_PICTURES;
use sp_model::{DEFAULT_FORMAT_OPTIONS, FormatOptions};
use sp_parser::png::encode_png;

/// Builders with no Calibre: KFX refuses, the AZW3 is sp-azw3's.
struct NoCalibre;

impl RungBuilders for NoCalibre {
    fn kfx(&self, _epub: &str, _s: &mut dyn FnMut(&str)) -> Result<String, EngineError> {
        Err(EngineError::Named {
            name: "CalibreMissingError",
            message: "Calibre's ebook-convert was not found.".into(),
        })
    }

    fn azw3(&self, epub: &str) -> Result<String, EngineError> {
        write_azw3(epub, &Books)
    }
}

fn opts(epub: &str, fountain: Option<&str>, calibre: bool) -> FreshKindleArtifactOptions {
    FreshKindleArtifactOptions {
        epub: epub.into(),
        fountain_path: fountain.map(str::to_owned),
        format: DEFAULT_FORMAT_OPTIONS,
        calibre_available: calibre,
        kfx_ready: false,
    }
}

fn ladder(o: &FreshKindleArtifactOptions) -> (Result<String, EngineError>, Vec<String>) {
    let mut stages = Vec::new();
    let out = fresh_kindle_artifact(o, &NoCalibre, &Books, &mut |s| stages.push(s.to_owned()));
    (out, stages)
}

fn mobi_rung(o: &FreshKindleArtifactOptions) -> (Result<String, EngineError>, Vec<String>) {
    let mut stages = Vec::new();
    let out = fresh_artifact_of(KindleRung::Mobi, o, &NoCalibre, &Books, &mut |s| {
        stages.push(s.to_owned());
    });
    (out, stages)
}

fn temps(dir: &str) -> Vec<String> {
    names(dir)
        .into_iter()
        .filter(|n| n.ends_with(".tmp"))
        .collect()
}

/// A real EPUB, as the engine writes one, ten seconds old (`realBook`).
fn real_book(s: &Scratch) -> String {
    let epub = s.join("Script.epub");
    let book = convert_fountain(
        "Title: Test\n\nINT. ROOM - DAY\n\nA line of action.\n\nJO\nA line.\n",
        ConvertOptions::default(),
        None,
    )
    .unwrap()
    .epub;
    std::fs::write(&epub, book).unwrap();
    age(&epub, 10);
    epub
}

#[test]
fn with_no_kfx_the_ladder_writes_our_own_azw3_with_no_calibre_and_the_book_untouched() {
    let s = Scratch::new("export-artifact-books");
    let epub = real_book(&s);
    let before = std::fs::read(&epub).unwrap();
    let (out, stages) = ladder(&opts(&epub, None, false));
    let out = out.unwrap();
    assert_eq!(out, azw3_sibling(&epub));
    let book = read_kf8(&std::fs::read(&out).unwrap()).expect("a well-formed KF8");
    assert_eq!(book.header.version, 8);
    assert_eq!(
        book.ncx
            .iter()
            .map(|n| n.label.as_str())
            .collect::<Vec<_>>(),
        ["INT. ROOM - DAY"]
    );
    assert_eq!(stages, ["building the AZW3 for Kindle…"]);
    assert_eq!(std::fs::read(&epub).unwrap(), before);
    assert!(temps(&s.str()).is_empty());
    assert!(
        kindle_artifact_plan(
            &epub,
            ToolchainState::default(),
            sp_azw3::AZW3_WRITER_REVISION
        )
        .fresh
    );
}

#[test]
fn an_epub_the_real_writer_cannot_read_is_an_honest_error_and_leaves_the_azw3() {
    let s = Scratch::new("export-artifact-books");
    let epub = s.join("Script.epub");
    std::fs::write(&epub, "not a zip").unwrap();
    let azw3 = azw3_sibling(&epub);
    std::fs::write(&azw3, "an older build").unwrap();
    age(&azw3, 10);
    let err = ladder(&opts(&epub, None, true)).0.unwrap_err();
    assert_eq!(err.name(), "RegenerationFailedError");
    assert!(err.message().starts_with(
        "Couldn't rebuild the Kindle file: the AZW3 could not be made from the book ("
    ));
    assert_eq!(common::read(&azw3), "an older build");
    assert!(temps(&s.str()).is_empty());
}

#[test]
fn a_stale_azw3_is_rebuilt_into_a_real_kf8() {
    let s = Scratch::new("export-artifact-books");
    let epub = real_book(&s);
    let azw3 = azw3_sibling(&epub);
    std::fs::write(&azw3, "stale-azw3").unwrap();
    age(&azw3, 20);
    let (out, stages) = ladder(&opts(&epub, None, true));
    assert_eq!(out.unwrap(), azw3);
    assert_eq!(stages, ["building the AZW3 for Kindle…"]);
    let book = read_kf8(&std::fs::read(&azw3).unwrap()).expect("a well-formed KF8");
    assert_eq!(book.header.version, 8);
}

#[test]
fn the_mobi_rebuild_writes_a_real_mobi_through_a_temp_file_and_leaves_none() {
    let s = Scratch::new("export-artifact-books");
    let fountain = s.join("Script.fountain");
    std::fs::write(
        &fountain,
        "Title: Test\n\nINT. ROOM - DAY\n\nA line of action.\n",
    )
    .unwrap();
    let epub = s.join("Script.epub");
    std::fs::write(&epub, "stale").unwrap();
    let (out, stages) = mobi_rung(&opts(&epub, Some(&fountain), false));
    let out = out.unwrap();
    assert_eq!(out, mobi_sibling(&epub));
    let mobi = std::fs::read(&out).unwrap();
    assert_eq!(&mobi[60..68], b"BOOKMOBI");
    assert!(common::mobi_records::mobi_html(&mobi).contains("INT. ROOM - DAY"));
    assert_eq!(common::read(&epub), "stale");
    assert_eq!(stages, ["rebuilding the Kindle file…"]);
    assert!(temps(&s.str()).is_empty());
}

// ---- the MOBI rebuild and the picture folder ----

/// What the handed-over .mobi carries of `pictures`: all of them once the
/// gate is open, none while it is shut.
fn in_mobi(pictures: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    if MOBI_CARRIES_PICTURES {
        pictures
    } else {
        Vec::new()
    }
}

struct LibraryBook {
    s: Scratch,
    fountain: String,
    folder: String,
    epub: String,
    plates: BTreeMap<String, Vec<u8>>,
}

/// A library script with two figures, and the book an earlier conversion
/// built from its folder while both pictures were there, ten seconds old.
fn library_book() -> LibraryBook {
    let s = Scratch::new("export-artifact-books");
    let fountain = s.join("Script.fountain");
    let text = "Title: Test\n\nINT. ROOM - DAY\n\n[[img: p001-01.png w=40]]\n\nA line of action.\n\n\
                [[img: p002-01.png w=60]]\n\nAnother line.\n";
    std::fs::write(&fountain, text).unwrap();
    let folder = s.dir("Script.images");
    let mut plates = BTreeMap::new();
    plates.insert(
        "p001-01.png".to_owned(),
        encode_png(&[0, 128, 255], 1, 1).unwrap(),
    );
    plates.insert(
        "p002-01.png".to_owned(),
        encode_png(&[255, 128, 0], 1, 1).unwrap(),
    );
    for (name, bytes) in &plates {
        std::fs::write(host::join(&[folder.as_str(), name.as_str()]), bytes).unwrap();
    }
    let epub = s.join("Script.epub");
    let book = convert_fountain(text, ConvertOptions::default(), Some(&plates))
        .unwrap()
        .epub;
    std::fs::write(&epub, book).unwrap();
    age(&epub, 10);
    // The control: the book starts out carrying both pictures.
    assert_eq!(
        image_entries(&std::fs::read(&epub).unwrap()),
        ["OEBPS/images/p001-01.png", "OEBPS/images/p002-01.png"]
    );
    LibraryBook {
        s,
        fountain,
        folder,
        epub,
        plates,
    }
}

fn rebuild(l: &LibraryBook, format: FormatOptions) -> (String, Vec<String>) {
    let o = FreshKindleArtifactOptions {
        format,
        ..opts(&l.epub, Some(&l.fountain), false)
    };
    let (out, stages) = mobi_rung(&o);
    (out.unwrap(), stages)
}

/// The rung against a damaged folder: the library book is left exactly as
/// it was, date included; the .mobi carries what could be read (`carried`,
/// in the script's order); and that .mobi is current.
fn expect_book_kept(l: &LibraryBook, carried: Vec<Vec<u8>>) {
    let bytes = std::fs::read(&l.epub).unwrap();
    let date = sp_engine::fsx::mtime_ms(&l.epub).unwrap();
    let (out, mut stages) = rebuild(l, DEFAULT_FORMAT_OPTIONS);
    assert_eq!(out, mobi_sibling(&l.epub));
    let mobi = std::fs::read(&out).unwrap();
    assert!(!mobi.is_empty());
    assert_eq!(mobi_pictures(&mobi), in_mobi(carried));
    assert_eq!(std::fs::read(&l.epub).unwrap(), bytes);
    assert_eq!(sp_engine::fsx::mtime_ms(&l.epub).unwrap(), date);
    assert!(sp_engine::fsx::mtime_ms(&out).unwrap() > date);
    assert!(rung_plan(KindleRung::Mobi, &l.epub, sp_azw3::AZW3_WRITER_REVISION).fresh);
    let (again, more) = rebuild(l, DEFAULT_FORMAT_OPTIONS);
    stages.extend(more);
    assert_eq!(again, out);
    assert_eq!(stages, ["rebuilding the Kindle file…"]);
}

#[test]
fn from_an_intact_folder_the_mobi_carries_every_picture_the_gate_lets_it() {
    let l = library_book();
    let (out, _) = rebuild(&l, DEFAULT_FORMAT_OPTIONS);
    assert_eq!(
        mobi_pictures(&std::fs::read(&out).unwrap()),
        in_mobi(vec![
            l.plates["p001-01.png"].clone(),
            l.plates["p002-01.png"].clone()
        ])
    );
    assert_eq!(
        image_entries(&std::fs::read(&l.epub).unwrap()),
        ["OEBPS/images/p001-01.png", "OEBPS/images/p002-01.png"]
    );
    let _ = &l.s;
}

#[test]
fn a_picture_the_folder_lost_leaves_the_library_book_and_the_mobi_without_it() {
    let l = library_book();
    std::fs::write(host::join(&[l.folder.as_str(), "p002-01.png"]), b"").unwrap();
    expect_book_kept(&l, vec![l.plates["p001-01.png"].clone()]);
}

#[test]
fn a_picture_folder_that_is_gone_leaves_the_library_book_and_the_mobi_without_pictures() {
    let l = library_book();
    std::fs::remove_dir_all(&l.folder).unwrap();
    expect_book_kept(&l, Vec::new());
}
