//! Port of tests/export-artifact.test.ts: the Kindle ladder's work.
//!
//! The AZW3 writer and the MOBI conversion are reached through
//! `Renderers`, and are stand-ins here ([`FakeRenderers`]) until sp-azw3
//! and the conversion land: what these tests pin is the LADDER (which rung,
//! reuse versus rebuild, the staleness rule, what is written where and
//! what is left alone, what the MOBI render is handed). The checks that
//! read a real KF8 or MOBI back (`readKf8`, `mobiPictures`) move to wave 3,
//! when the real makers are plugged in; PARITY.md says so.

mod common;

use common::books::{FAKE_EPUB, FakeRenderers, age, stamped_azw3};
use common::png::png;
use common::{Scratch, calibre_env, names};
use sp_engine::errors::EngineError;
use sp_engine::export::artifact::{
    FreshKindleArtifactOptions, RealBuilders, RungBuilders, available_formats, azw3_sibling,
    fresh_artifact_of, fresh_kindle_artifact, kindle_artifact_plan, mobi_sibling, rung_plan,
};
use sp_engine::export::formats::{ExportFormat, KindleRung, ToolchainState};
use sp_engine::export::kfx::kfx_sibling;
use sp_engine::nodepath::host;
use sp_model::{DEFAULT_FORMAT_OPTIONS, FormatOptions};

fn opts(epub: &str, fountain: Option<&str>, calibre: bool, kfx: bool) -> FreshKindleArtifactOptions {
    FreshKindleArtifactOptions {
        epub: epub.into(),
        fountain_path: fountain.map(str::to_owned),
        format: DEFAULT_FORMAT_OPTIONS,
        calibre_available: calibre,
        kfx_ready: kfx,
    }
}

/// Builders that never reach Calibre: KFX refuses (recording that it was
/// asked), and the AZW3 is the renderers' own.
struct NoCalibre<'a> {
    renderers: &'a FakeRenderers,
    kfx_asked: std::cell::Cell<usize>,
}

impl RungBuilders for NoCalibre<'_> {
    fn kfx(&self, _epub: &str, _s: &mut dyn FnMut(&str)) -> Result<String, EngineError> {
        self.kfx_asked.set(self.kfx_asked.get() + 1);
        Err(EngineError::Named {
            name: "CalibreMissingError",
            message: "Calibre's ebook-convert was not found.".into(),
        })
    }

    fn azw3(&self, epub: &str) -> Result<String, EngineError> {
        sp_engine::export::artifact::write_azw3(epub, self.renderers)
    }
}

fn no_calibre(r: &FakeRenderers) -> NoCalibre<'_> {
    NoCalibre {
        renderers: r,
        kfx_asked: std::cell::Cell::new(0),
    }
}

fn ladder(
    o: &FreshKindleArtifactOptions,
    r: &FakeRenderers,
) -> (Result<String, EngineError>, Vec<String>) {
    let b = no_calibre(r);
    let mut stages = Vec::new();
    let out = fresh_kindle_artifact(o, &b, r, &mut |s| stages.push(s.to_owned()));
    (out, stages)
}

fn rung(
    which: KindleRung,
    o: &FreshKindleArtifactOptions,
    r: &FakeRenderers,
) -> (Result<String, EngineError>, Vec<String>) {
    let b = no_calibre(r);
    let mut stages = Vec::new();
    let out = fresh_artifact_of(which, o, &b, r, &mut |s| stages.push(s.to_owned()));
    (out, stages)
}

fn temps(dir: &str) -> Vec<String> {
    names(dir).into_iter().filter(|n| n.ends_with(".tmp")).collect()
}

/// A stand-in book, ten seconds old.
fn book(s: &Scratch) -> String {
    let epub = s.join("Script.epub");
    std::fs::write(&epub, FAKE_EPUB).unwrap();
    age(&epub, 10);
    epub
}

#[cfg(unix)]
#[test]
fn the_mobi_and_azw3_siblings_share_the_epub_directory_and_stem() {
    assert_eq!(mobi_sibling("/tmp/lib/Script.epub"), "/tmp/lib/Script.mobi");
    assert_eq!(azw3_sibling("/tmp/lib/Script.epub"), "/tmp/lib/Script.azw3");
    assert_eq!(azw3_sibling("/tmp/lib/Script.EPUB"), "/tmp/lib/Script.azw3");
}

#[test]
fn epub_and_a_kindle_file_are_always_available() {
    assert_eq!(available_formats(), vec![ExportFormat::Epub, ExportFormat::Kindle]);
}

#[test]
fn with_no_kfx_the_ladder_writes_our_azw3_and_leaves_the_book() {
    let s = Scratch::new("export-artifact");
    let epub = book(&s);
    let r = FakeRenderers::default();
    let (out, stages) = ladder(&opts(&epub, None, false, false), &r);
    let out = out.unwrap();
    assert_eq!(out, azw3_sibling(&epub));
    assert_eq!(std::fs::read(&out).unwrap(), stamped_azw3(Some(1)));
    assert_eq!(stages, vec!["building the AZW3 for Kindle…"]);
    assert_eq!(std::fs::read(&epub).unwrap(), FAKE_EPUB);
    assert!(temps(&s.str()).is_empty());
    assert!(kindle_artifact_plan(&epub, ToolchainState::default(), 1).fresh);
}

#[test]
fn an_epub_the_writer_cannot_read_is_an_honest_error_and_leaves_the_azw3() {
    let s = Scratch::new("export-artifact");
    let epub = s.join("Script.epub");
    std::fs::write(&epub, "not a zip").unwrap();
    let azw3 = azw3_sibling(&epub);
    std::fs::write(&azw3, "an older build").unwrap();
    age(&azw3, 10);
    let r = FakeRenderers::default();
    let err = ladder(&opts(&epub, None, true, false), &r).0.unwrap_err();
    assert_eq!(err.name(), "RegenerationFailedError");
    assert!(err.message().starts_with("Couldn't rebuild the Kindle file: the AZW3 could not be made from the book ("));
    assert_eq!(common::read(&azw3), "an older build");
    assert!(temps(&s.str()).is_empty());
}

#[test]
fn a_fresh_mobi_is_reused() {
    let s = Scratch::new("export-artifact");
    let epub = s.join("Script.epub");
    std::fs::write(&epub, "epub").unwrap();
    age(&epub, 10);
    let mobi = mobi_sibling(&epub);
    std::fs::write(&mobi, "existing-mobi").unwrap();
    let r = FakeRenderers::default();
    let out = rung(KindleRung::Mobi, &opts(&epub, None, false, false), &r).0.unwrap();
    assert_eq!(out, mobi);
    assert_eq!(common::read(&mobi), "existing-mobi");
    assert!(r.mobi_calls.borrow().is_empty());
}

#[test]
fn a_missing_mobi_with_no_fountain_is_an_honest_error() {
    let s = Scratch::new("export-artifact");
    let epub = s.join("Script.epub");
    std::fs::write(&epub, "epub").unwrap();
    let r = FakeRenderers::default();
    let err = rung(KindleRung::Mobi, &opts(&epub, None, false, false), &r).0.unwrap_err();
    assert_eq!(err.name(), "CannotRegenerateError");
    assert_eq!(
        err.message(),
        "Can't rebuild the Kindle file: the script's .fountain is missing."
    );
}

#[test]
fn the_mobi_rung_rebuilds_from_the_fountain_and_leaves_the_epub() {
    let s = Scratch::new("export-artifact");
    let fountain = s.join("Script.fountain");
    std::fs::write(&fountain, "Title: Test\n\nINT. ROOM - DAY\n\nA line of action.\n").unwrap();
    let epub = s.join("Script.epub");
    std::fs::write(&epub, "stale").unwrap();
    let r = FakeRenderers::default();
    let (out, stages) = rung(KindleRung::Mobi, &opts(&epub, Some(&fountain), false, false), &r);
    let out = out.unwrap();
    assert_eq!(out, mobi_sibling(&epub));
    assert!(std::path::Path::new(&out).exists());
    assert_eq!(common::read(&epub), "stale");
    assert_eq!(stages, vec!["rebuilding the Kindle file…"]);
    assert!(temps(&s.str()).is_empty());
}

// ---- the MOBI rebuild and the picture folder ----

struct Library {
    s: Scratch,
    fountain: String,
    epub: String,
    folder: String,
}

fn library() -> Library {
    let s = Scratch::new("export-artifact");
    let fountain = s.join("Script.fountain");
    std::fs::write(
        &fountain,
        "Title: Test\n\nINT. ROOM - DAY\n\n[[img: p001-01.png w=40]]\n\nA line of action.\n\n[[img: p002-01.png w=60]]\n\nAnother line.\n",
    )
    .unwrap();
    let folder = s.dir("Script.images");
    std::fs::write(host::join(&[folder.as_str(), "p001-01.png"]), png(1)).unwrap();
    std::fs::write(host::join(&[folder.as_str(), "p002-01.png"]), png(2)).unwrap();
    let epub = s.join("Script.epub");
    std::fs::write(&epub, FAKE_EPUB).unwrap();
    age(&epub, 10);
    Library {
        s,
        fountain,
        epub,
        folder,
    }
}

fn rebuild(l: &Library, r: &FakeRenderers, format: FormatOptions) -> (String, Vec<String>) {
    let o = FreshKindleArtifactOptions {
        format,
        ..opts(&l.epub, Some(&l.fountain), false, false)
    };
    let (out, stages) = rung(KindleRung::Mobi, &o, r);
    (out.unwrap(), stages)
}

#[test]
fn from_an_intact_folder_the_render_is_handed_every_picture() {
    let l = library();
    let r = FakeRenderers::default();
    rebuild(&l, &r, DEFAULT_FORMAT_OPTIONS);
    assert_eq!(
        *r.mobi_calls.borrow(),
        vec![Some(vec!["p001-01.png".to_owned(), "p002-01.png".to_owned()])]
    );
    assert_eq!(std::fs::read(&l.epub).unwrap(), FAKE_EPUB);
}

/// The rung against a damaged folder: the library book is left exactly as
/// it was, date included; the render gets what could be read; and the
/// .mobi it writes is current, so the next export reuses it.
fn expect_book_kept(l: &Library, carried: &[&str]) {
    let r = FakeRenderers::default();
    let date = sp_engine::fsx::mtime_ms(&l.epub).unwrap();
    let (out, mut stages) = rebuild(l, &r, DEFAULT_FORMAT_OPTIONS);
    assert_eq!(out, mobi_sibling(&l.epub));
    assert!(!std::fs::read(&out).unwrap().is_empty());
    assert_eq!(
        *r.mobi_calls.borrow(),
        vec![Some(carried.iter().map(|s| (*s).to_owned()).collect())]
    );
    assert_eq!(std::fs::read(&l.epub).unwrap(), FAKE_EPUB);
    assert_eq!(sp_engine::fsx::mtime_ms(&l.epub).unwrap(), date);
    assert!(sp_engine::fsx::mtime_ms(&out).unwrap() > date);
    assert!(rung_plan(KindleRung::Mobi, &l.epub, 1).fresh);
    let (again, more) = rebuild(l, &r, DEFAULT_FORMAT_OPTIONS);
    stages.extend(more);
    assert_eq!(again, out);
    assert_eq!(stages, vec!["rebuilding the Kindle file…"]);
}

#[test]
fn a_picture_the_folder_lost_leaves_the_book_and_the_mobi_goes_without() {
    let l = library();
    std::fs::write(host::join(&[l.folder.as_str(), "p002-01.png"]), b"").unwrap();
    expect_book_kept(&l, &["p001-01.png"]);
}

#[test]
fn a_picture_folder_that_is_gone_leaves_the_book_and_the_mobi_goes_without() {
    let l = library();
    std::fs::remove_dir_all(&l.folder).unwrap();
    expect_book_kept(&l, &[]);
}

#[test]
fn with_include_images_off_the_folder_is_not_read() {
    let on = library();
    let r = FakeRenderers::default();
    rebuild(&on, &r, DEFAULT_FORMAT_OPTIONS);
    let off = library();
    rebuild(
        &off,
        &r,
        FormatOptions {
            include_images: false,
            ..DEFAULT_FORMAT_OPTIONS
        },
    );
    let calls = r.mobi_calls.borrow();
    assert!(calls[0].is_some());
    assert_eq!(calls[1], None);
    let _ = &on.s;
}

// ---- ladder precedence ----

#[test]
fn a_fresh_kfx_is_reused_without_touching_calibre() {
    let s = Scratch::new("export-artifact");
    let epub = s.join("Script.epub");
    std::fs::write(&epub, "epub").unwrap();
    age(&epub, 10);
    let kfx = kfx_sibling(&epub);
    std::fs::write(&kfx, "existing-kfx").unwrap();
    let r = FakeRenderers::default();
    let b = no_calibre(&r);
    let out = fresh_kindle_artifact(&opts(&epub, None, true, true), &b, &r, &mut |_| {}).unwrap();
    assert_eq!(out, kfx);
    assert_eq!(common::read(&kfx), "existing-kfx");
    assert_eq!(b.kfx_asked.get(), 0);
}

/// The real KFX builder, against a placeholder book, with Calibre pointed at
/// the scratch COPY of its settings folder. Reaching toKfx throws
/// CalibreMissingError (no Calibre), KfxToolchainNotReadyError (no
/// Previewer or plugin) or CalibreFailedError (the placeholder is no zip);
/// falling through to the AZW3 would throw RegenerationFailedError.
/// CALIBRE SPAWNS: calibre-customize --list-plugins, and ebook-convert when
/// the whole toolchain is here, both with the scratch copy.
#[test]
fn kfx_ready_with_no_kfx_attempts_the_kfx_rebuild() {
    let s = Scratch::new("export-artifact");
    let epub = s.join("Script.epub");
    std::fs::write(&epub, "epub").unwrap();
    let env = calibre_env();
    let r = FakeRenderers::default();
    let b = RealBuilders {
        env: &env,
        renderers: &r,
    };
    let err = fresh_kindle_artifact(&opts(&epub, None, true, true), &b, &r, &mut |_| {})
        .unwrap_err();
    assert!(
        [
            "CalibreMissingError",
            "CalibreFailedError",
            "KfxToolchainNotReadyError"
        ]
        .contains(&err.name()),
        "{err:?}"
    );
    assert_eq!(*r.azw3_calls.borrow(), 0);
}

#[test]
fn a_fresh_azw3_is_reused_not_rebuilt() {
    let s = Scratch::new("export-artifact");
    let epub = s.join("Script.epub");
    std::fs::write(&epub, "epub").unwrap();
    age(&epub, 10);
    let azw3 = azw3_sibling(&epub);
    std::fs::write(&azw3, stamped_azw3(Some(1))).unwrap();
    let r = FakeRenderers::default();
    let (out, stages) = ladder(&opts(&epub, None, false, false), &r);
    assert_eq!(out.unwrap(), azw3);
    assert!(stages.is_empty());
    assert_eq!(*r.azw3_calls.borrow(), 0);
}

#[test]
fn a_stale_azw3_is_rebuilt() {
    let s = Scratch::new("export-artifact");
    let epub = book(&s);
    let azw3 = azw3_sibling(&epub);
    std::fs::write(&azw3, "stale-azw3").unwrap();
    age(&azw3, 20);
    let r = FakeRenderers::default();
    let (out, stages) = ladder(&opts(&epub, None, true, false), &r);
    assert_eq!(out.unwrap(), azw3);
    assert_eq!(stages, vec!["building the AZW3 for Kindle…"]);
    assert_eq!(std::fs::read(&azw3).unwrap(), stamped_azw3(Some(1)));
}

#[test]
fn an_azw3_from_an_older_writer_or_none_is_stale() {
    for stamp in [None, Some(0)] {
        let s = Scratch::new("export-artifact");
        let epub = book(&s);
        let azw3 = azw3_sibling(&epub);
        std::fs::write(&azw3, stamped_azw3(stamp)).unwrap();
        assert!(!rung_plan(KindleRung::Azw3, &epub, 1).fresh, "{stamp:?}");
        // And a later revision is current.
        std::fs::write(&azw3, stamped_azw3(Some(2))).unwrap();
        assert!(rung_plan(KindleRung::Azw3, &epub, 1).fresh);
    }
}

#[test]
fn a_fresh_mobi_is_never_the_ladders_answer() {
    let s = Scratch::new("export-artifact");
    let epub = book(&s);
    let mobi = mobi_sibling(&epub);
    std::fs::write(&mobi, "existing-mobi").unwrap();
    let r = FakeRenderers::default();
    assert_eq!(
        ladder(&opts(&epub, None, false, false), &r).0.unwrap(),
        azw3_sibling(&epub)
    );
    assert_eq!(common::read(&mobi), "existing-mobi");
}

// ---- the plan ----

fn with_sibling(s: &Scratch, ext: Option<&str>, fresh: bool) -> String {
    let epub = s.join("Script.epub");
    std::fs::write(&epub, "epub").unwrap();
    age(&epub, 5);
    if let Some(ext) = ext {
        let sibling = s.join(&format!("Script.{ext}"));
        if ext == "azw3" {
            std::fs::write(&sibling, stamped_azw3(Some(1))).unwrap();
        } else {
            std::fs::write(&sibling, ext).unwrap();
        }
        age(&sibling, if fresh { 0 } else { 10 });
    }
    epub
}

#[test]
fn the_plan_names_the_rung_its_file_what_builds_it_and_the_books_date() {
    let s = Scratch::new("export-artifact");
    let epub = with_sibling(&s, None, false);
    let date = sp_engine::fsx::mtime_ms(&epub);
    let plan = |state| serde_json::to_value(kindle_artifact_plan(&epub, state, 1)).unwrap();
    let want = |path: String, by: &str| {
        serde_json::json!({ "path": path, "fresh": false, "builtBy": by, "bookDate": date })
    };
    assert_eq!(
        plan(ToolchainState {
            calibre_available: true,
            kfx_ready: true
        }),
        want(kfx_sibling(&epub), "calibre")
    );
    assert_eq!(
        plan(ToolchainState {
            calibre_available: true,
            kfx_ready: false
        }),
        want(azw3_sibling(&epub), "screepub")
    );
    assert_eq!(
        plan(ToolchainState::default()),
        want(azw3_sibling(&epub), "screepub")
    );
    assert_eq!(
        serde_json::to_value(rung_plan(KindleRung::Mobi, &epub, 1)).unwrap(),
        want(mobi_sibling(&epub), "screepub")
    );
}

#[test]
fn the_books_date_moves_with_the_book_and_is_null_when_unreadable() {
    let s = Scratch::new("export-artifact");
    let epub = with_sibling(&s, None, false);
    let state = ToolchainState {
        calibre_available: true,
        kfx_ready: true,
    };
    let before = kindle_artifact_plan(&epub, state, 1).book_date;
    age(&epub, -60);
    let after = kindle_artifact_plan(&epub, state, 1).book_date;
    assert_ne!(after, before);
    assert_eq!(after, sp_engine::fsx::mtime_ms(&epub));
    assert_eq!(
        kindle_artifact_plan(&s.join("gone.epub"), state, 1).book_date,
        None
    );
}

#[test]
fn the_plan_is_fresh_exactly_when_the_ladder_would_reuse() {
    let cases = [
        ("kfx", ToolchainState { calibre_available: true, kfx_ready: true }),
        ("azw3", ToolchainState { calibre_available: true, kfx_ready: false }),
        ("azw3", ToolchainState::default()),
    ];
    for (ext, state) in cases {
        let a = Scratch::new("export-artifact");
        assert!(kindle_artifact_plan(&with_sibling(&a, Some(ext), true), state, 1).fresh, "{ext}");
        let b = Scratch::new("export-artifact");
        assert!(!kindle_artifact_plan(&with_sibling(&b, Some(ext), false), state, 1).fresh, "{ext}");
    }
    let c = Scratch::new("export-artifact");
    assert!(
        !kindle_artifact_plan(
            &with_sibling(&c, Some("azw3"), true),
            ToolchainState {
                calibre_available: true,
                kfx_ready: true
            },
            1
        )
        .fresh
    );
}

#[test]
fn a_fresh_plan_is_what_the_ladder_hands_back_untouched() {
    let s = Scratch::new("export-artifact");
    let epub = with_sibling(&s, Some("azw3"), true);
    let plan = kindle_artifact_plan(
        &epub,
        ToolchainState {
            calibre_available: true,
            kfx_ready: false,
        },
        1,
    );
    assert!(plan.fresh);
    let r = FakeRenderers::default();
    let out = ladder(&opts(&epub, None, true, false), &r).0.unwrap();
    assert_eq!(out, plan.path);
    assert_eq!(std::fs::read(&out).unwrap(), stamped_azw3(Some(1)));
}

#[test]
fn an_explicit_mobi_rung_leaves_the_epub_and_its_siblings_alone() {
    let s = Scratch::new("export-artifact");
    let fountain = s.join("Script.fountain");
    std::fs::write(&fountain, "Title: Test\n\nINT. ROOM - DAY\n\nA line of action.\n").unwrap();
    let epub = s.join("Script.epub");
    std::fs::write(&epub, "the book as it is").unwrap();
    let azw3 = azw3_sibling(&epub);
    std::fs::write(&azw3, stamped_azw3(Some(1))).unwrap();
    age(&epub, 60);
    age(&azw3, 30);
    let before = sp_engine::fsx::mtime_ms(&epub);
    let r = FakeRenderers::default();
    let out = rung(KindleRung::Mobi, &opts(&epub, Some(&fountain), true, false), &r)
        .0
        .unwrap();
    assert_eq!(out, mobi_sibling(&epub));
    assert!(!std::fs::read(&out).unwrap().is_empty());
    assert_eq!(common::read(&epub), "the book as it is");
    assert_eq!(sp_engine::fsx::mtime_ms(&epub), before);
    assert!(
        kindle_artifact_plan(
            &epub,
            ToolchainState {
                calibre_available: true,
                kfx_ready: false
            },
            1
        )
        .fresh
    );
    let _ = png(0);
}
