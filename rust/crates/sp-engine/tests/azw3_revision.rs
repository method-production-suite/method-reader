//! The staleness half of tests/azw3-revision.test.ts ("the staleness rule
//! reads it"): an .azw3 with no writer revision (Calibre's, or ours from
//! before the stamp) or an older one is rebuilt, so a fixed writer reaches
//! the files already beside the books. The stamp and its reader are
//! sp-azw3/tests/azw3.rs. tests/helpers/stamped-azw3.ts is
//! common::books::stamped_azw3, checked first against sp-azw3's own reader.

mod common;

use common::Scratch;
use common::books::{age, stamped_azw3};
use sp_azw3::{AZW3_WRITER_REVISION, azw3_revision};
use sp_engine::convert::{ConvertOptions, convert_fountain};
use sp_engine::errors::EngineError;
use sp_engine::export::artifact::{
    FreshKindleArtifactOptions, RungBuilders, azw3_sibling, fresh_kindle_artifact, rung_plan,
    write_azw3,
};
use sp_engine::export::formats::KindleRung;
use sp_engine::export::renderers::Books;
use sp_model::DEFAULT_FORMAT_OPTIONS;

/// A real book, twenty seconds old, and beside it (newer) `azw3` if given.
fn book_with(s: &Scratch, azw3: Option<&[u8]>) -> String {
    let epub = s.join("Script.epub");
    let book = convert_fountain(
        "Title: Test\n\nINT. ROOM - DAY\n\nA line.\n",
        ConvertOptions::default(),
        None,
    )
    .unwrap()
    .epub;
    std::fs::write(&epub, book).unwrap();
    age(&epub, 20);
    if let Some(bytes) = azw3 {
        std::fs::write(azw3_sibling(&epub), bytes).unwrap();
    }
    epub
}

fn fresh(rung: KindleRung, epub: &str) -> bool {
    rung_plan(rung, epub, AZW3_WRITER_REVISION).fresh
}

#[test]
fn the_stand_in_reads_back_through_the_real_revision_reader() {
    assert_eq!(
        azw3_revision(&stamped_azw3(Some(AZW3_WRITER_REVISION))),
        Some(AZW3_WRITER_REVISION)
    );
    assert_eq!(azw3_revision(&stamped_azw3(Some(7))), Some(7));
    assert_eq!(azw3_revision(&stamped_azw3(None)), None);
}

#[test]
fn newer_than_the_epub_and_stamped_with_this_revision_current() {
    let s = Scratch::new("azw3-revision");
    let epub = book_with(&s, Some(&stamped_azw3(Some(AZW3_WRITER_REVISION))));
    assert!(fresh(KindleRung::Azw3, &epub));
}

#[test]
fn newer_than_the_epub_but_with_no_revision_stale() {
    let s = Scratch::new("azw3-revision");
    let epub = book_with(&s, Some(&stamped_azw3(None)));
    assert!(!fresh(KindleRung::Azw3, &epub));
}

#[test]
fn newer_than_the_epub_but_an_older_revision_stale() {
    let s = Scratch::new("azw3-revision");
    let epub = book_with(&s, Some(&stamped_azw3(Some(AZW3_WRITER_REVISION - 1))));
    assert!(!fresh(KindleRung::Azw3, &epub));
}

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

#[test]
fn the_ladder_rebuilds_an_unstamped_azw3_into_a_stamped_one() {
    let s = Scratch::new("azw3-revision");
    let epub = book_with(&s, Some(&stamped_azw3(None)));
    let mut stages = Vec::new();
    let out = fresh_kindle_artifact(
        &FreshKindleArtifactOptions {
            epub: epub.clone(),
            fountain_path: None,
            format: DEFAULT_FORMAT_OPTIONS,
            calibre_available: false,
            kfx_ready: false,
        },
        &NoCalibre,
        &Books,
        &mut |st| stages.push(st.to_owned()),
    )
    .unwrap();
    assert_eq!(stages, ["building the AZW3 for Kindle…"]);
    assert_eq!(
        azw3_revision(&std::fs::read(out).unwrap()),
        Some(AZW3_WRITER_REVISION)
    );
    assert!(fresh(KindleRung::Azw3, &epub));
}

#[test]
fn the_kfx_and_mobi_rungs_keep_the_date_rule_alone() {
    let s = Scratch::new("azw3-revision");
    let epub = book_with(&s, None);
    std::fs::write(s.join("Script.mobi"), "no revision in a mobi").unwrap();
    assert!(fresh(KindleRung::Mobi, &epub));
}
