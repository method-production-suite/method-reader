//! The Field Station showcase (SUITE-STANDARDS.md section 8), from the
//! window: the first launch of a fresh install converts the built-in sample
//! into its own folder (never the library) and shows it in Preview; Help,
//! Open Sample does the same any time and reopens a current book rather
//! than converting again; a switch on the Settings page turns the first
//! launch's sample off. Every folder is a scratch one: the fake computer's
//! sample folder is a temp folder, and the engine is the fake.

mod common;

use common::*;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};

use sp_app::app::App;
use sp_app::app::help;
use sp_app::app::sample;
use sp_app::engine::Request;
use sp_app::model::convert::Platform;
use sp_app::model::frame::Surface;
use sp_app::model::help::HelpItem;
use sp_app::model::sample::{self as m, Paths};
use sp_app::runtime::Mode;

/// A window on a fresh computer whose storage starts with `stored`, and
/// whose sample folder is a scratch one.
fn window(w: &World, stored: &[(&str, &str)]) -> (H, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("scratch");
    let mut host = w.fakes.host(Platform::Mac);
    for (k, v) in stored {
        host.storage.set_item(k, v);
    }
    host.sample_dir = Some(dir.path().join(m::FOLDER));
    let app = App::new(w.engine.clone(), Mode::Inline, host);
    let mut h = Harness::builder()
        .with_size([1280.0, 800.0])
        .build_ui_state(|ui, app: &mut App| app.frame(ui), app);
    h.run_steps(4);
    (h, dir)
}

fn paths(dir: &tempfile::TempDir) -> Paths {
    Paths::in_folder(&dir.path().join(m::FOLDER))
}

/// Nothing that sends, exports, builds or asks Calibre anything.
fn nothing_sent(w: &World) {
    for kind in [
        "convert",
        "send",
        "export",
        "kindle-check",
        "kfx-status",
        "kfx-install",
        "route",
        "email-setup",
    ] {
        assert!(
            w.engine.calls_of(kind).is_empty(),
            "{kind}: {:?}",
            w.engine.calls_of(kind)
        );
    }
}

#[test]
fn the_first_launch_converts_the_sample_outside_the_library_and_shows_it_in_preview() {
    let w = world();
    let (mut h, dir) = window(&w, &[]);
    assert!(h.state().sample.first_launch, "a fresh install");
    sample::first_launch(h.state_mut());
    steps(&mut h);
    let p = paths(&dir);
    assert_eq!(
        w.engine.calls_of("convert-to"),
        [Request::ConvertTo {
            path: p.pdf.clone(),
            out: p.epub.clone()
        }]
    );
    // The built-in PDF, in the sample's own folder.
    assert!(std::fs::read(&p.pdf).unwrap() == m::PDF);
    assert!(
        std::path::Path::new(&p.stamp).is_file(),
        "kept for a reopen"
    );
    let app = h.state();
    assert_eq!(app.surface, Surface::Read);
    let script = app.script.as_ref().expect("the sample is open");
    assert_eq!(script.title, "Field Station");
    assert_eq!(script.epub_path.as_deref(), Some(p.epub.as_str()));
    assert!(sample::is_open(app));
    assert!(has(&h, m::NOTE), "the quiet line says it is a sample");
    nothing_sent(&w);

    // Once: a second call is nothing.
    sample::first_launch(h.state_mut());
    steps(&mut h);
    assert_eq!(w.engine.calls_of("convert-to").len(), 1);
}

#[test]
fn an_updated_copy_the_switch_off_or_an_open_script_gets_no_sample() {
    for stored in [
        &[("whatsNewSeen", "0.13.0")][..],
        &[("sampleFirstLaunch", "false")][..],
    ] {
        let w = world();
        let (mut h, _dir) = window(&w, stored);
        assert!(!h.state().sample.first_launch, "{stored:?}");
        sample::first_launch(h.state_mut());
        steps(&mut h);
        assert!(w.engine.calls_of("convert-to").is_empty(), "{stored:?}");
        assert!(h.state().script.is_none());
    }
    // A script of the person's own already open: theirs stays.
    let w = world();
    let (mut h, _dir) = window(&w, &[]);
    convert(&w, &mut h);
    sample::first_launch(h.state_mut());
    steps(&mut h);
    assert!(w.engine.calls_of("convert-to").is_empty());
    assert_eq!(
        h.state().script.as_ref().unwrap().path,
        "/scripts/field-station.pdf"
    );
}

#[test]
fn the_next_window_on_the_same_computer_has_had_its_first_launch() {
    let w = world();
    let mut host = w.fakes.host(Platform::Mac);
    let app = App::new(w.engine.clone(), Mode::Inline, w.fakes.host(Platform::Mac));
    assert!(app.sample.first_launch);
    // The storage the first window left behind.
    host.storage.set_item("sampleFirstLaunchDone", "true");
    let again = App::new(w.engine.clone(), Mode::Inline, host);
    assert!(!again.sample.first_launch);
}

#[test]
fn open_sample_reopens_a_current_book_and_converts_a_stale_one() {
    let w = world();
    let (mut h, dir) = window(&w, &[("whatsNewSeen", "0.13.0")]);
    let p = paths(&dir);
    help::choose(h.state_mut(), HelpItem::OpenSample);
    steps(&mut h);
    assert_eq!(w.engine.calls_of("convert-to").len(), 1);
    assert_eq!(h.state().surface, Surface::Read);
    // What the real engine would have written beside the PDF.
    std::fs::write(&p.epub, b"book").unwrap();
    std::fs::write(&p.fountain, b"Title: Field Station\n").unwrap();

    help::choose(h.state_mut(), HelpItem::OpenSample);
    steps(&mut h);
    let calls = w.engine.calls_of("convert-to");
    assert_eq!(
        calls[1],
        Request::ConvertTo {
            path: p.fountain.clone(),
            out: p.epub.clone()
        },
        "the book is remade from the .fountain, not the PDF"
    );
    let script = h.state().script.clone().unwrap();
    assert_eq!(script.path, p.pdf, "still the sample's PDF");
    assert_eq!(script.pages, Some(19), "the stamp's facts");
    assert_eq!(script.fountain_path.as_deref(), Some(p.fountain.as_str()));

    // A PDF that is not the built-in one: converted afresh, and put back.
    std::fs::write(&p.pdf, b"%PDF-1.4 something else").unwrap();
    help::choose(h.state_mut(), HelpItem::OpenSample);
    steps(&mut h);
    assert_eq!(
        w.engine.calls_of("convert-to")[2],
        Request::ConvertTo {
            path: p.pdf.clone(),
            out: p.epub.clone()
        }
    );
    assert!(std::fs::read(&p.pdf).unwrap() == m::PDF);
    nothing_sent(&w);
}

#[test]
fn converting_the_sample_again_keeps_it_out_of_the_library() {
    let w = world();
    let (mut h, dir) = window(&w, &[]);
    sample::first_launch(h.state_mut());
    steps(&mut h);
    sp_app::app::convert::convert_again(h.state_mut());
    steps(&mut h);
    let p = paths(&dir);
    assert_eq!(
        w.engine.calls_of("convert-to"),
        vec![
            Request::ConvertTo {
                path: p.pdf.clone(),
                out: p.epub.clone()
            };
            2
        ]
    );
    nothing_sent(&w);
}

#[test]
fn no_sample_folder_means_no_sample() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    assert!(h.state().host.sample_dir.is_none());
    help::choose(h.state_mut(), HelpItem::OpenSample);
    sample::first_launch(h.state_mut());
    steps(&mut h);
    assert!(w.engine.calls_of("convert-to").is_empty());
}

#[test]
fn the_settings_page_turns_the_first_launch_sample_off() {
    let w = world();
    let (mut h, _dir) = window(&w, &[]);
    sample::first_launch(h.state_mut());
    steps(&mut h);
    h.state_mut().go_to(Surface::Tune);
    steps(&mut h);
    let switch = h.get_by_role_and_label(Role::CheckBox, m::SWITCH_LABEL);
    assert_eq!(
        switch.accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::True),
        "on by default"
    );
    // Through the screen reader's press, as the other Settings switches.
    switch.click_accesskit();
    steps(&mut h);
    assert!(!m::first_launch_on(h.state().host.storage.as_ref()));
    assert_eq!(
        h.state()
            .host
            .storage
            .get_item("sampleFirstLaunch")
            .as_deref(),
        Some("false")
    );
    assert!(has(&h, m::SWITCH_LEGEND));
}

#[test]
fn the_open_sample_flag_is_the_menu_item_at_launch() {
    // Not the first launch, and the switch off: the flag opens it anyway.
    let w = world();
    let (mut h, dir) = window(
        &w,
        &[("whatsNewSeen", "0.13.0"), ("sampleFirstLaunch", "false")],
    );
    h.state_mut().launched_with(&["--open-sample".to_string()]);
    steps(&mut h);
    let p = paths(&dir);
    assert_eq!(
        w.engine.calls_of("convert-to"),
        [Request::ConvertTo {
            path: p.pdf.clone(),
            out: p.epub.clone()
        }]
    );
    assert_eq!(h.state().surface, Surface::Read);

    // On a first launch it stands in for the first launch's sample: once.
    let w = world();
    let (mut h, _dir) = window(&w, &[]);
    h.state_mut().launched_with(&["--open-sample".to_string()]);
    sample::first_launch(h.state_mut());
    steps(&mut h);
    steps(&mut h);
    assert_eq!(w.engine.calls_of("convert-to").len(), 1);
}

#[test]
fn the_help_menu_opens_the_sample_on_windows_and_linux_too() {
    let w = world();
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness(&w, Platform::Linux);
    h.state_mut().host.sample_dir = Some(dir.path().join(m::FOLDER));
    h.get_by_role_and_label(Role::Button, "Help").click();
    steps(&mut h);
    h.get_by_role_and_label(Role::MenuItem, m::MENU_LABEL)
        .click();
    steps(&mut h);
    assert_eq!(w.engine.calls_of("convert-to").len(), 1);
    assert_eq!(h.state().surface, Surface::Read);
}
