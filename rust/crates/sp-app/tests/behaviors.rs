//! The inventory's "built" rows, each tested: what the window does, read
//! back through the AccessKit tree or the window's own state. Each test
//! names its rust/PARITY.md row.

mod common;

use std::sync::atomic::Ordering;

use common::*;
use egui::accesskit::{Action, Live, Role};
use egui::{Key, Modifiers};
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::json;

use sp_app::app::{App, convert, panel, show::Show};
use sp_app::engine::{Reply, Request, default_answer};
use sp_app::model::convert::Platform;
use sp_app::model::frame::Surface;
use sp_app::model::motion::Wake;

fn calm_as(w: &World, kind: &str) {
    let mut a = default_answer(&Request::AppSettings);
    a["readerDevice"] = json!(kind);
    w.engine.always("app-settings", Reply::ok(a));
}

fn open_other_ways(h: &mut H) {
    h.get_by_role_and_label(Role::Button, "Other ways")
        .click_accesskit();
    steps(h);
}

// ------------------------------------------------------------------ window

// The family's window: the system's own title bar naming the app, as
// Method Budget's and Method Schedule's do (it was hidden under the
// Screepub look, with the paper running under the traffic lights).
#[test]
fn w1_w2_w3_the_window_opens_at_1280_by_800_with_a_floor_and_a_title_bar() {
    let v = sp_app::app::viewport();
    assert_eq!(v.title.as_deref(), Some("Method Reader"));
    assert_eq!(v.inner_size, Some(egui::vec2(1280.0, 800.0)));
    assert_eq!(v.min_inner_size, Some(egui::vec2(640.0, 480.0)));
    assert_eq!(v.resizable, Some(true));
    assert_eq!(v.titlebar_shown, Some(true));
    assert_eq!(v.title_shown, Some(true));
    assert_eq!(v.fullsize_content_view, Some(false));
}

// The Dock shows the bundle's own icon while the window runs. eframe puts
// egui's logo there unless the window hands it an icon, and an EMPTY one
// is how eframe's docs say to keep the bundle's (it then sets nothing).
// Without this the 0.13.0 window showed egui's logo in the Dock.
#[test]
fn the_dock_keeps_the_bundles_own_icon() {
    let v = sp_app::app::viewport();
    let icon = v.icon.as_deref().expect("an icon is handed to eframe");
    assert_eq!(icon, &egui::IconData::default());
    assert!(icon.rgba.is_empty());
}

#[test]
fn w4_the_tab_strip_is_the_top_of_the_window() {
    let w = world();
    let h = harness(&w, Platform::Mac);
    let tab = h.get_by_role_and_label(Role::Tab, "Convert").rect();
    assert!(
        tab.top() >= 4.0 && tab.top() <= 16.0,
        "the tabs sit at the top, under the title bar: {tab:?}"
    );
}

// The title bar names the script, then the app, once there is a script.
#[test]
fn w2_the_title_bar_names_the_script_then_the_app() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    assert_eq!(h.state().title_shown.as_deref(), Some("Method Reader"));
    convert(&w, &mut h);
    assert_eq!(
        h.state().title_shown.as_deref(),
        Some("Field Station - Method Reader")
    );
}

#[test]
fn a5_decorative_furniture_has_no_node() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    assert!(h.query_all_by_label_contains("brad").next().is_none());
    // The result's first page is a picture of the book, not text to read.
    assert!(!has(&h, "Thirty-one."));
}

// -------------------------------------------------------------------- tabs

#[test]
fn n4_only_the_selected_tab_is_a_tab_stop() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    let focusable = |h: &H, name: &str| {
        h.get_by_role_and_label(Role::Tab, name)
            .accesskit_node()
            .data()
            .supports_action(Action::Focus)
    };
    assert!(focusable(&h, "Convert"));
    assert!(!focusable(&h, "Preview"));
    assert!(!focusable(&h, "Settings"));
}

#[test]
fn n5_the_arrows_move_between_tabs_and_wrap() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Tab, "Convert").focus();
    steps(&mut h);
    h.key_press(Key::ArrowLeft);
    steps(&mut h);
    assert_eq!(
        h.state().surface,
        Surface::Tune,
        "left from the first wraps to the last"
    );
    eprintln!(
        "FOCUSED {:?}",
        h.query_by(|n| n.is_focused())
            .map(|n| n.accesskit_node().label())
    );
    h.key_press(Key::Home);
    steps(&mut h);
    assert_eq!(h.state().surface, Surface::Convert);
    h.key_press(Key::ArrowRight);
    steps(&mut h);
    assert_eq!(
        h.state().surface,
        Surface::Library,
        "the Library comes next"
    );
    h.key_press(Key::ArrowRight);
    steps(&mut h);
    assert_eq!(h.state().surface, Surface::Read);
}

#[test]
fn n6_n19_closing_the_script_takes_the_tabs_away_and_returns_to_convert() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    assert!(
        h.state().watcher.running(),
        "the devices poll runs while a book is open"
    );
    h.get_by_role_and_label(Role::Tab, "Preview").click();
    steps(&mut h);
    {
        let app = h.state_mut();
        app.script = None;
        app.script_changed();
    }
    steps(&mut h);
    assert_eq!(h.state().surface, Surface::Convert);
    assert!(h.query_by_role_and_label(Role::Tab, "Preview").is_none());
    assert!(!h.state().watcher.running());
}

#[test]
fn n7_tabs_arrive_once_when_a_script_opens() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    assert!(h.state().arriving.is_empty());
    convert(&w, &mut h);
    let at = h.state().arriving.get(&Surface::Read).copied();
    assert!(at.is_some());
    convert(&w, &mut h);
    assert_eq!(
        h.state().arriving.get(&Surface::Read).copied(),
        at,
        "not replayed for a second script"
    );
}

// ----------------------------------------------------------- frames at rest

/// The clock past every animation, a step at a time, as frames would go.
fn let_everything_settle(w: &World, h: &mut H) {
    for _ in 0..80 {
        w.fakes.clock.advance(100.0);
        h.run_steps(1);
    }
    steps(h);
}

#[test]
fn a_window_with_nothing_moving_and_nothing_due_asks_for_no_frame() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    let_everything_settle(&w, &mut h);
    assert_eq!(h.state().wake, Wake::Idle, "the drop well at rest");
}

#[test]
fn a_script_at_rest_wakes_only_for_the_devices_poll() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    for surface in ["Preview", "Settings", "Convert"] {
        h.get_by_role_and_label(Role::Tab, surface).click();
        let_everything_settle(&w, &mut h);
        match h.state().wake {
            Wake::In(ms) => assert!(
                ms > 0.0 && ms <= 2000.0,
                "{surface}: the next poll, {ms} ms away"
            ),
            other => panic!("{surface}: at rest it asked for {other:?}"),
        }
    }
}

#[test]
fn an_animation_asks_for_the_very_next_frame_until_it_ends() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Tab, "Preview").click();
    let_everything_settle(&w, &mut h);
    h.get_by_role_and_label(Role::Button, "Hide scenes").click();
    h.run_steps(1);
    w.fakes.clock.advance(50.0);
    h.run_steps(1);
    assert_eq!(h.state().wake, Wake::Next, "the index sliding away");
    w.fakes.clock.advance(200.0);
    h.run_steps(2);
    assert_ne!(h.state().wake, Wake::Next, "and still once it has gone");
}

// ------------------------------------------------------------------- drops

#[test]
fn n15_a_file_dragged_over_turns_the_well_and_a_drop_converts() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    h.input_mut().hovered_files.push(egui::HoveredFile {
        path: Some("/x/a.pdf".into()),
        mime: String::new(),
    });
    h.step();
    assert!(h.state().drag_hover);
    h.input_mut().hovered_files.clear();
    h.step();
    assert!(!h.state().drag_hover);
    convert::drop_paths(h.state_mut(), &["".into(), "/x/dropped.pdf".into()]);
    steps(&mut h);
    let call = w.engine.calls_of("convert");
    assert_eq!(call[0].argv()[0], "/x/dropped.pdf");
    h.get_by_role_and_label(Role::Heading, "Field Station is ready");
}

#[test]
fn n16_a_dropped_fountain_is_the_same_convert_request_a_pdf_is() {
    // ROADMAP.md, Decisions: "Scripts as input". The window decides
    // nothing about the kind of file: the engine does.
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert::drop_paths(h.state_mut(), &["/x/Weather Log.fountain".into()]);
    steps(&mut h);
    let call = w.engine.calls_of("convert");
    assert_eq!(call.len(), 1);
    assert_eq!(
        call[0],
        Request::Convert {
            path: "/x/Weather Log.fountain".into(),
            force: false,
            options: None,
        }
    );
    h.get_by_role_and_label(Role::Heading, "Field Station is ready");
}

#[test]
fn n16_choose_a_script_converts_a_fountain_it_picks() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    w.fakes
        .answer_dialog(Some("/Users/reader/Scripts/Weather Log.fountain"));
    h.get_by_role_and_label(Role::Button, "Choose a script…")
        .click();
    steps(&mut h);
    let call = w.engine.calls_of("convert");
    assert_eq!(call.len(), 1);
    assert_eq!(
        call[0].argv()[0],
        "/Users/reader/Scripts/Weather Log.fountain"
    );
}

#[test]
fn n17_a_drop_while_converting_is_ignored() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    h.state_mut().convert.converting = true;
    convert::drop_paths(h.state_mut(), &["/x/dropped.pdf".into()]);
    steps(&mut h);
    assert!(w.engine.calls_of("convert").is_empty());
}

// ----------------------------------------------------------------- convert

#[test]
fn c3_the_well_keeps_its_820_column() {
    let w = world();
    let h = harness(&w, Platform::Mac);
    let call = h
        .query_all_by_value("A PDF with selectable text, a .fountain or an .osf.")
        .next()
        .unwrap()
        .rect();
    let heading = h
        .get_by_role_and_label(Role::Heading, "Drop a script")
        .rect();
    // The block is 50..1230 at 1280: an 820 column centred in it, so on
    // the window's centre line. The harness frames the window in a few
    // points: measure from the first tab, 32 in from the window's edge.
    let left = h.get_by_role_and_label(Role::Tab, "Convert").rect().left() - 32.0;
    let centre = left + 640.0;
    assert!((heading.center().x - centre).abs() < 2.0, "{heading:?}");
    assert!((call.center().x - centre).abs() < 2.0);
}

#[test]
fn c8_change_asks_for_a_folder_and_writes_it() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    w.fakes.answer_dialog(Some("/Users/reader/Books"));
    let mut moved = default_answer(&Request::AppSettings);
    moved["library"]["path"] = json!("/Users/reader/Books");
    moved["library"]["chosen"] = json!("/Users/reader/Books");
    w.engine.once("app-settings-set", Reply::ok(moved));
    h.get_by_role_and_label(Role::Button, "Change…").click();
    steps(&mut h);
    let set = w.engine.calls_of("app-settings-set");
    assert_eq!(
        set[0].argv().last().unwrap(),
        r#"{"libraryPath":"/Users/reader/Books"}"#
    );
    assert!(has(&h, "Books are saved in ~/Books."));
    assert!(has(
        &h,
        "New books go here. Books already converted stay where they are."
    ));
    h.get_by_role_and_label(Role::Button, "Reset");
    assert!(!h.state().convert.library_busy);
}

#[test]
fn c8_a_cancelled_change_writes_nothing() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    w.fakes.answer_dialog(None);
    h.get_by_role_and_label(Role::Button, "Change…").click();
    steps(&mut h);
    assert!(w.engine.calls_of("app-settings-set").is_empty());
    assert!(
        !h.get_by_role_and_label(Role::Button, "Change…")
            .accesskit_node()
            .is_disabled()
    );
}

#[test]
fn c9_a_probe_for_a_well_that_is_gone_is_dropped() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    let old = h.state().convert.generation;
    convert::draw_well(h.state_mut());
    h.state_mut().convert.library = None;
    let mut elsewhere = default_answer(&Request::AppSettings);
    elsewhere["library"]["path"] = json!("/Elsewhere");
    convert::answered(
        h.state_mut(),
        convert::Purpose::LibraryProbe { generation: old },
        Reply::ok(elsewhere),
    );
    assert!(h.state().convert.library.is_none());
}

#[test]
fn c10_the_probe_carries_the_page_switches() {
    let w = world();
    let mut a = default_answer(&Request::AppSettings);
    a["animatePage"] = json!(false);
    a["typewriterSounds"] = json!(true);
    w.engine.always("app-settings", Reply::ok(a));
    let h = harness(&w, Platform::Mac);
    assert_eq!(h.state().animate_page, Some(false));
    assert_eq!(h.state().typewriter_sounds, Some(true));
}

#[test]
fn c11_c15_the_working_screen_says_what_it_is_doing_and_holds_the_keyboard() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    {
        let app = h.state_mut();
        app.convert.view = convert::View::Working;
        app.convert.path = "/scripts/field-station.pdf".into();
        app.restore_focus = true;
    }
    steps(&mut h);
    h.get_by_role_and_label(Role::Heading, "Converting");
    assert!(has(
        &h,
        "The pages of field-station.pdf reflow themselves, one scene at a time."
    ));
    let status = h.get_by_role_and_label(Role::Status, "starting up");
    assert_eq!(status.accesskit_node().data().live(), Some(Live::Polite));
    let focused = h.get_by(|n| n.is_focused());
    assert_eq!(
        focused.accesskit_node().role(),
        Role::TabPanel,
        "a pane with no control takes the keyboard"
    );
}

#[test]
fn c14_converting_the_same_pdf_again_takes_its_turn_on_its_book() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    assert!(
        h.state()
            .convert
            .landed
            .contains_key("/scripts/field-station.pdf")
    );
    convert(&w, &mut h);
    assert_eq!(w.engine.calls_of("convert").len(), 2);
    assert!(
        h.state().queue.is_idle(),
        "the turn ended with the conversion"
    );
}

#[test]
fn c22_d28_the_last_send_and_its_eject_come_back_with_the_result() {
    let w = world();
    calm_as(&w, "kindle");
    plug_in_a_kindle(&w);
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Button, "Copy to Kindle (.azw3)")
        .click_accesskit();
    steps(&mut h);
    let line = "Field Station is on your Kindle. Eject it before you unplug.";
    assert!(has(&h, line));
    h.get_by_role_and_label(Role::Button, "Convert another")
        .click_accesskit();
    steps(&mut h);
    h.get_by_role_and_label(Role::Tab, "Preview").click();
    steps(&mut h);
    h.get_by_role_and_label(Role::Button, "Send to e-reader")
        .click();
    steps(&mut h);
    assert!(has(&h, line), "the line comes back with the result");
    h.get_by_role_and_label(Role::Button, "Eject Kindle");
    // Another book: the reader still waiting to be ejected comes along.
    let mut other = default_answer(&Request::Convert {
        path: String::new(),
        force: false,
        options: None,
    });
    other["epubPath"] = json!("/library/Other/Other.epub");
    other["title"] = json!("Other");
    w.engine.once("convert", Reply::ok(other));
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Button, "Eject Kindle");
}

#[test]
fn c29_convert_it_again_converts_the_open_scripts_source() {
    let w = world();
    let mut blank = default_answer(&Request::Convert {
        path: String::new(),
        force: false,
        options: None,
    });
    blank["previewHtml"] = json!("");
    w.engine.once("convert", Reply::ok(blank));
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Tab, "Preview").click();
    steps(&mut h);
    h.get_by_role_and_label(Role::Button, "Convert it again")
        .click();
    steps(&mut h);
    let calls = w.engine.calls_of("convert");
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[1].argv()[0], "/scripts/field-station.pdf");
    assert_eq!(h.state().surface, Surface::Convert);
}

// --------------------------------------------------------------- page show

#[test]
fn p2_p4_the_show_says_converting_and_carries_the_whole_page_for_a_screen_reader() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    {
        let app = h.state_mut();
        app.convert.view = convert::View::Working;
        app.show = Some(Show::begin(
            &["EXT. RIDGE ROAD - DAY".into(), "Thirty-one.".into()],
            "field-station.pdf".into(),
            0.0,
            false,
        ));
    }
    steps(&mut h);
    h.get_by_role_and_label(Role::Heading, "Converting");
    assert!(has(&h, "field-station.pdf"));
    assert!(
        has(&h, "EXT. RIDGE ROAD - DAY"),
        "the typed page's words, whole, from the first moment"
    );
    assert!(has(&h, "Thirty-one."));
    h.get_by_role_and_label(Role::Status, "Starting up");
}

fn typed_with_sounds(sounds: bool) -> Vec<sp_app::app::host::Sound> {
    let w = world();
    let mut a = default_answer(&Request::AppSettings);
    a["typewriterSounds"] = json!(sounds);
    w.engine.always("app-settings", Reply::ok(a));
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    assert!(h.state().show.is_some());
    for _ in 0..30 {
        w.fakes.clock.advance(200.0);
        steps(&mut h);
    }
    assert!(h.state().show.is_none(), "the show ran to its end");
    w.fakes.speaker.played.lock().unwrap().clone()
}

#[test]
fn p13_the_typewriter_is_heard_only_when_its_sounds_are_on() {
    use sp_app::app::host::Sound;
    let heard = typed_with_sounds(true);
    let keys = heard.iter().filter(|s| **s == Sound::Key).count();
    let bells = heard.iter().filter(|s| **s == Sound::Ding).count();
    assert!(keys > 10, "keys while typing: {keys}");
    assert!(bells >= 1, "a bell when a block finishes");
    assert!(typed_with_sounds(false).is_empty(), "off is quiet");
}

#[test]
fn p10_a_refusal_ends_the_show() {
    let w = world();
    w.engine.once(
        "convert",
        Reply::ok(json!({"ok": false, "error": {"code": "scanned", "message": "A scan."}})),
    );
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    assert!(h.state().show.is_none());
    h.get_by_role_and_label(Role::Heading, "Scanned PDF, no text");
}

// -------------------------------------------------------------------- read

#[test]
fn r3_under_the_fold_the_index_sits_above_the_script() {
    let w = world();
    let mut h = harness_sized(&w, Platform::Mac, [900.0, 800.0]);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Tab, "Preview").click();
    steps(&mut h);
    let rail = h.get_by_role_and_label(Role::Navigation, "Scenes").rect();
    let stage = h
        .get_by_role_and_label(Role::Group, "Field Station, the script")
        .rect();
    assert!(
        rail.bottom() <= stage.top() + 1.0,
        "{rail:?} above {stage:?}"
    );
}

/// The window sizes the layout has rules for: the floor, the short and
/// narrow breakpoints, the fold, the default and a large one.
const SIZES: [[f32; 2]; 8] = [
    [640.0, 480.0],
    [720.0, 560.0],
    [640.0, 800.0],
    [1280.0, 480.0],
    [900.0, 620.0],
    [900.0, 800.0],
    [1280.0, 800.0],
    [1800.0, 1100.0],
];

// N9: the rev stamp is drawn whole inside the window on Read,
// which fits the window rather than scrolling the page. It was clipped by
// the window's bottom edge at 1280 by 800.
#[test]
fn n9_the_rev_stamp_is_whole_inside_the_window_on_read() {
    // Every size, and the default and the floor again with the dead
    // engine's line above the stamp.
    let cases = SIZES
        .map(|s| (s, false))
        .into_iter()
        .chain([([1280.0, 800.0], true), ([640.0, 480.0], true)]);
    for ([width, height], dead) in cases {
        let w = world();
        if dead {
            w.engine.once(
                "version",
                Reply::failed("could not start the Method Reader engine"),
            );
        }
        let mut h = harness_sized(&w, Platform::Mac, [width, height]);
        convert(&w, &mut h);
        h.get_by_role_and_label(Role::Tab, "Preview").click();
        steps(&mut h);
        let stamp = h
            .get_by_role_and_label(Role::Button, concat!("rev ", env!("CARGO_PKG_VERSION")))
            .rect();
        let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
        assert!(
            stamp.height() > 5.0 && window.contains_rect(stamp),
            "at {width} by {height} (dead engine: {dead}) the stamp is at {stamp:?}"
        );
    }
}

/// Wheel the page itself (over the binding margin) as far down as it goes.
fn scroll_page_to_end(h: &mut H, height: f32) {
    h.event(egui::Event::PointerMoved(egui::pos2(20.0, height / 2.0)));
    h.run_steps(2);
    for _ in 0..5 {
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -4000.0),
            phase: egui::TouchPhase::Move,
            modifiers: Modifiers::NONE,
            source: egui::MouseWheelSource::Unknown,
        });
        h.run_steps(30);
    }
}

// N9: the surfaces that scroll the page (Convert, empty and with a result;
// Settings) end with the stamp whole inside the window.
#[test]
fn n9_at_the_end_of_a_scrolling_page_the_rev_stamp_is_whole() {
    for [width, height] in SIZES {
        let w = world();
        let mut h = harness_sized(&w, Platform::Mac, [width, height]);
        let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, height));
        let check = |h: &mut H, what: &str| {
            scroll_page_to_end(h, height);
            let stamp = h
                .get_by_role_and_label(Role::Button, concat!("rev ", env!("CARGO_PKG_VERSION")))
                .rect();
            assert!(
                stamp.height() > 5.0 && window.contains_rect(stamp),
                "{what} at {width} by {height}: the stamp is at {stamp:?}"
            );
        };
        check(&mut h, "Convert");
        convert(&w, &mut h);
        check(&mut h, "the result");
        h.get_by_role_and_label(Role::Tab, "Settings").click();
        steps(&mut h);
        check(&mut h, "Settings");
    }
}

#[test]
fn r9_r10_pressing_a_scene_scrolls_to_it_and_marks_it() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Tab, "Preview").click();
    steps(&mut h);
    h.get_by_role_and_label(Role::Button, "EXT. RIDGE – NIGHT")
        .click();
    steps(&mut h);
    // It glides there as WebKit's smooth scroll did: 200 ms at most,
    // ease-in-out, so half way through the time it is half way there.
    let start = h.state().read.offset;
    w.fakes.clock.advance(100.0);
    steps(&mut h);
    let mid = h.state().read.offset;
    w.fakes.clock.advance(150.0);
    steps(&mut h);
    let end = h.state().read.offset;
    assert!(end > 1000.0, "scrolled down the script: {end}");
    let half = (mid - start) / (end - start);
    assert!((half - 0.5).abs() < 0.05, "half way at half time: {half}");
    let current = h.get_by(|n| n.data().aria_current().is_some());
    assert_eq!(
        current.accesskit_node().label().as_deref(),
        Some("EXT. RIDGE – NIGHT")
    );
}

#[test]
fn r9_with_reduce_motion_a_scene_is_there_at_once() {
    let w = world();
    w.fakes
        .reduced_motion
        .store(true, std::sync::atomic::Ordering::Relaxed);
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Tab, "Preview").click();
    steps(&mut h);
    h.get_by_role_and_label(Role::Button, "EXT. RIDGE – NIGHT")
        .click();
    steps(&mut h);
    assert!(h.state().read.offset > 1000.0, "no clock needed");
}

#[test]
fn r15_a_settings_save_rebuilds_what_read_shows_and_keeps_the_place() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Tab, "Preview").click();
    steps(&mut h);
    h.get_by_role_and_label(Role::Button, "EXT. RIDGE – NIGHT")
        .click();
    steps(&mut h);
    h.get_by_role_and_label(Role::Tab, "Settings").click();
    steps(&mut h);
    let place = h.state().read.place.clone();
    assert!(place.is_some(), "the place was kept as the reader left");
    h.get_by_role_and_label(Role::Button, "Phone")
        .click_accesskit();
    steps(&mut h);
    w.fakes.clock.advance(301.0);
    steps(&mut h);
    steps(&mut h);
    assert_eq!(w.engine.calls_of("reconvert").len(), 1);
    h.get_by_role_and_label(Role::Tab, "Preview").click();
    steps(&mut h);
    assert_eq!(
        h.state().read.marked.as_deref(),
        place.as_ref().map(|p| p.id.as_str()),
        "back in the same scene"
    );
}

// ---------------------------------------------------------------- settings

fn settings(h: &mut H) {
    h.get_by_role_and_label(Role::Tab, "Settings").click();
    steps(h);
}

#[test]
fn s2_while_the_answer_is_out_it_says_it_is_reading_never_the_fault() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    settings(&mut h);
    {
        let app = h.state_mut();
        app.tune.settings = None;
        app.tune.fault = None;
    }
    steps(&mut h);
    assert!(has(&h, "Reading this script’s settings…"));
    h.get_by_role_and_label(Role::Button, "Send to e-reader");
    assert!(!has(&h, "Settings out of reach"));
}

#[test]
fn s7_s8_every_group_opens_and_every_knob_is_named() {
    let w = world();
    let mut h = harness_sized(&w, Platform::Mac, [1280.0, 4000.0]);
    convert(&w, &mut h);
    settings(&mut h);
    for group in [
        "Dialogue",
        "The text",
        "What the book carries",
        "Taken from the PDF",
    ] {
        h.get_by_role_and_label(Role::DisclosureTriangle, group)
            .click_accesskit();
        steps(&mut h);
    }
    assert!(has(
        &h,
        "These four are decided while the PDF is being read. Everything else re-renders from the script Method Reader already read, so changing one here saves it for the next time you convert this PDF. It will not change what you see now."
    ));
    for slider in [
        "Space between elements",
        "Dialogue margins",
        "Character name indent",
        "Parenthetical indent",
    ] {
        h.get_by_role_and_label(Role::Slider, slider);
    }
    for toggle in [
        "Start each scene on a new page",
        "Title page",
        "Pictures from the script",
        "The PDF's page numbers",
    ] {
        h.get_by_role_and_label(Role::CheckBox, toggle);
    }
    assert!(
        has(&h, "1.0 em") && has(&h, "20%"),
        "the read-outs carry their units"
    );
    assert!(
        h.query_all_by_label("Beta").count() >= 2,
        "the two picture switches wear the badge"
    );
}

#[test]
fn s14_the_engines_stored_values_redraw_the_screen() {
    let w = world();
    let mut stored = default_answer(&Request::Settings {
        fountain: String::new(),
        set: None,
    });
    stored["settings"]["dialogueSideMarginPct"] = json!(12);
    w.engine.once("settings-set", Reply::ok(stored));
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    settings(&mut h);
    h.get_by_role_and_label(Role::Button, "Phone")
        .click_accesskit();
    steps(&mut h);
    w.fakes.clock.advance(301.0);
    steps(&mut h);
    steps(&mut h);
    let shown = h.state().tune.settings.as_ref().unwrap()["dialogueSideMarginPct"].clone();
    assert_eq!(
        shown,
        json!(12),
        "the engine is the authority on what was stored"
    );
}

#[test]
fn s17_use_these_for_new_scripts_writes_the_defaults_and_says_so() {
    let w = world();
    let mut h = harness_sized(&w, Platform::Mac, [1280.0, 3000.0]);
    convert(&w, &mut h);
    settings(&mut h);
    assert!(has(
        &h,
        "New scripts start from Method Reader’s own defaults."
    ));
    assert!(
        h.query_by_role_and_label(
            Role::Button,
            "Reset new scripts to Method Reader’s defaults"
        )
        .is_none(),
        "nothing to reset"
    );
    h.get_by_role_and_label(Role::Button, "Use these for new scripts")
        .click_accesskit();
    steps(&mut h);
    let set = w.engine.calls_of("app-settings-set");
    assert!(
        set[0]
            .argv()
            .last()
            .unwrap()
            .starts_with(r#"{"formatDefaults":{"#)
    );
    h.get_by_role_and_label(Role::Status, "New scripts will start from these settings.");
}

#[test]
fn s18_the_keep_choice_is_a_radio_group_and_its_write_is_confirmed() {
    let w = world();
    let mut stored = default_answer(&Request::AppSettings);
    stored["keepScriptSettings"] = json!(false);
    w.engine.once("app-settings-set", Reply::ok(stored));
    let mut h = harness_sized(&w, Platform::Mac, [1280.0, 3000.0]);
    convert(&w, &mut h);
    settings(&mut h);
    h.get_by_role_and_label(Role::RadioGroup, "When a PDF is converted");
    h.get_by_role_and_label(Role::RadioButton, "Follow the defaults")
        .click_accesskit();
    steps(&mut h);
    assert_eq!(
        w.engine.calls_of("app-settings-set")[0]
            .argv()
            .last()
            .unwrap(),
        r#"{"keepScriptSettings":false}"#
    );
    h.get_by_role_and_label(
        Role::Status,
        "Saved. It applies to PDFs you convert from now on.",
    );
}

#[test]
fn s19_sounds_rest_while_the_page_is_still_and_reduce_motion_says_why() {
    let w = world();
    w.fakes.reduced_motion.store(true, Ordering::Relaxed);
    let mut stored = default_answer(&Request::AppSettings);
    stored["animatePage"] = json!(false);
    w.engine.once("app-settings-set", Reply::ok(stored));
    let mut h = harness_sized(&w, Platform::Mac, [1280.0, 3000.0]);
    convert(&w, &mut h);
    settings(&mut h);
    assert!(has(
        &h,
        "This computer is set to reduce motion, so the page does not animate."
    ));
    h.get_by_role_and_label(Role::CheckBox, "Animate the page")
        .click_accesskit();
    steps(&mut h);
    assert_eq!(
        w.engine.calls_of("app-settings-set")[0]
            .argv()
            .last()
            .unwrap(),
        r#"{"animatePage":false}"#
    );
    assert!(
        h.get_by_role_and_label(Role::CheckBox, "Typewriter sounds")
            .accesskit_node()
            .is_disabled()
    );
}

#[test]
fn a3_a_knobs_sentence_describes_it() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    settings(&mut h);
    let knob = h.get_by_role_and_label(Role::CheckBox, "Start each scene on a new page");
    assert!(!knob.accesskit_node().data().described_by().is_empty());
    let badge = h.query_all_by_label("Beta").next();
    let _ = badge;
}

// -------------------------------------------------------------- send panel

fn plug_in_a_kindle(w: &World) {
    let mut routes = default_answer(&Request::Routes {
        epub: String::new(),
        quick: false,
    });
    let list = routes["routes"].as_array_mut().unwrap();
    let k = list
        .iter_mut()
        .find(|r| r["id"] == "device:kindle")
        .unwrap();
    *k = json!({"id": "device:kindle", "key": "device:kindle", "title": "Kindle", "detail": "copies over USB",
        "button": "Copy to Kindle", "available": true,
        "device": {"id": "k1", "kind": "kindle", "name": "Kindle", "volume": "/Volumes/Kindle"}});
    routes["chosen"] = json!("device:kindle");
    w.engine.always("routes", Reply::ok(routes));
    w.engine.always("devices", Reply::ok(json!({"ok": true, "devices": [{"id": "k1", "kind": "kindle", "name": "Kindle", "volume": "/Volumes/Kindle"}]})));
}

#[test]
fn d1_a_reader_who_chose_never_sees_the_cards() {
    let w = world();
    calm_as(&w, "kobo");
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    assert!(!has(&h, "What do you read on?"));
    h.get_by_role_and_label(Role::Button, "Save the EPUB…");
}

#[test]
fn d13_the_calm_screen_asks_quick_then_full() {
    let w = world();
    calm_as(&w, "kobo");
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    let asks: Vec<bool> = w
        .engine
        .calls_of("routes")
        .iter()
        .map(|r| r.argv().contains(&"--quick".to_string()))
        .collect();
    assert_eq!(&asks[..2], &[true, false]);
}

#[test]
fn d15_a_remarkable_reader_is_looked_for_every_ten_seconds() {
    let w = world();
    calm_as(&w, "remarkable");
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    let before = w.engine.calls_of("routes").len();
    w.fakes.clock.advance(10_001.0);
    steps(&mut h);
    w.fakes.clock.advance(10_001.0);
    steps(&mut h);
    assert!(w.engine.calls_of("routes").len() > before);
}

#[test]
fn d19_d24_the_email_row_offers_amazons_page_and_a_route_says_how_it_went() {
    let w = world();
    calm_as(&w, "kobo");
    let mut h = harness_sized(&w, Platform::Mac, [1280.0, 3000.0]);
    convert(&w, &mut h);
    open_other_ways(&mut h);
    assert!(has(
        &h,
        "First time? Amazon needs your sender address approved, or it drops the email without a word."
    ));
    h.get_by_role_and_label(Role::Button, "Open Amazon’s page")
        .click_accesskit();
    steps(&mut h);
    assert_eq!(w.engine.calls_of("email-setup").len(), 1);
    h.get_by_role_and_label(Role::Status, "Opened Amazon’s Personal Document Settings.");
    h.get_by_role_and_label(Role::Button, "Open in Books")
        .click_accesskit();
    steps(&mut h);
    assert_eq!(w.engine.calls_of("route")[0].argv()[1], "apple-books");
    h.get_by_role_and_label(Role::Status, "Opened apple-books.");
}

#[test]
fn d22_a_save_asks_where_then_the_engine_writes_there() {
    let w = world();
    calm_as(&w, "kobo");
    let mut h = harness_sized(&w, Platform::Mac, [1280.0, 3000.0]);
    convert(&w, &mut h);
    w.fakes
        .answer_dialog(Some("/Users/reader/Desktop/Field Station.fdx"));
    open_other_ways(&mut h);
    h.get_by_role_and_label(Role::Button, "Save FDX…")
        .click_accesskit();
    steps(&mut h);
    assert!(
        w.fakes
            .dialogs
            .asked
            .lock()
            .unwrap()
            .iter()
            .any(|a| a == "save Field Station.fdx")
    );
    let export = w.engine.calls_of("export");
    let argv = export.last().unwrap().argv();
    assert!(argv.windows(2).any(|p| p == ["--for", "fdx"]));
    assert!(
        argv.windows(2)
            .any(|p| p == ["--out", "/Users/reader/Desktop/Field Station.fdx"])
    );
    h.get_by_role_and_label(Role::Status, "Saved Field Station.fdx.");
}

#[test]
fn d25_one_flow_at_a_time() {
    let w = world();
    calm_as(&w, "kobo");
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.state_mut().panel.sending = true;
    steps(&mut h);
    assert!(
        h.get_by_role_and_label(Role::Button, "Save the EPUB…")
            .accesskit_node()
            .is_disabled()
    );
}

#[test]
fn d29_the_kindle_file_is_asked_about_when_the_result_shows() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    assert!(!w.engine.calls_of("kindle-check").is_empty());
}

#[test]
fn d31_a_kindle_file_building_says_so() {
    let w = world();
    calm_as(&w, "kindle");
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    let book = h
        .state()
        .script
        .as_ref()
        .unwrap()
        .epub_path
        .clone()
        .unwrap();
    h.state_mut().panel.builds.insert(
        book,
        panel::Build {
            runs: 1,
            extension: Some("kfx".into()),
            file: None,
        },
    );
    steps(&mut h);
    h.get_by_role_and_label(
        Role::Status,
        "Building the Kindle file. This takes about half a minute.",
    );
}

#[test]
fn d32_a_send_reads_the_scripts_settings_first() {
    let w = world();
    calm_as(&w, "kindle");
    plug_in_a_kindle(&w);
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Button, "Copy to Kindle (.azw3)")
        .click_accesskit();
    steps(&mut h);
    let kinds: Vec<&str> = w
        .engine
        .calls()
        .iter()
        .map(Request::kind)
        .filter(|k| ["settings", "export", "send"].contains(k))
        .collect();
    assert_eq!(&kinds[..3], &["settings", "export", "send"]);
}

#[test]
fn d8_a_report_from_the_asterisk_is_sent_and_thanked() {
    let w = world();
    *w.fakes.poster.answer.lock().unwrap() = Some((200, Some(json!({"ok": true}))));
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Button, "About Kobo: never tried on real hardware")
        .click_accesskit();
    steps(&mut h);
    h.get_by_role_and_label(Role::Button, "It worked")
        .click_accesskit();
    for _ in 0..50 {
        steps(&mut h);
        if has(&h, "Thanks. We count every request.") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(has(&h, "Thanks. We count every request."));
    assert!(
        h.query_by_role_and_label(Role::Button, "It worked")
            .is_none(),
        "thanks takes the answers away"
    );
    let sent = w.fakes.poster.sent.lock().unwrap().clone();
    assert_eq!(sent[0].1["route"], "kobo");
    assert_eq!(sent[0].1["worked"], true);
}

// --------------------------------------------------------------------- KFX

#[test]
fn k3_k5_set_it_up_opens_the_checklist_whose_links_open_their_pages() {
    let w = world();
    calm_as(&w, "kindle");
    let mut h = harness_sized(&w, Platform::Mac, [1280.0, 3000.0]);
    convert(&w, &mut h);
    open_other_ways(&mut h);
    h.get_by_role_and_label(Role::Button, "Set it up")
        .click_accesskit();
    steps(&mut h);
    h.get_by_role_and_label(Role::Region, "Best Kindle quality");
    h.get_by_role_and_label(Role::Button, "Get Calibre")
        .click_accesskit();
    steps(&mut h);
    assert_eq!(
        w.fakes.opener.opened.lock().unwrap()[0],
        "https://calibre-ebook.com/download_osx"
    );
    let probes = w.engine.calls_of("kfx-status").len();
    h.input_mut().focused = false;
    h.step();
    h.input_mut().focused = true;
    steps(&mut h);
    assert!(
        w.engine.calls_of("kfx-status").len() > probes,
        "asked again on a focus return"
    );
}

// -------------------------------------------------------------- heads-up

#[test]
fn h4_no_heads_up_over_the_result() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    plug_in_a_kindle(&w);
    w.fakes.clock.advance(2001.0);
    steps(&mut h);
    assert!(!has(&h, "Kindle plugged in. Copy Field Station?"));
}

#[test]
fn h5_h6_the_heads_up_goes_by_itself_and_copy_it_copies() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Tab, "Preview").click();
    steps(&mut h);
    plug_in_a_kindle(&w);
    w.fakes.clock.advance(2001.0);
    steps(&mut h);
    assert!(has(&h, "Kindle plugged in. Copy Field Station?"));
    w.fakes.clock.advance(8001.0);
    steps(&mut h);
    assert!(
        !has(&h, "Kindle plugged in. Copy Field Station?"),
        "gone after eight seconds"
    );

    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Tab, "Preview").click();
    steps(&mut h);
    plug_in_a_kindle(&w);
    w.fakes.clock.advance(2001.0);
    steps(&mut h);
    h.get_by_role_and_label(Role::Button, "Copy it").click();
    steps(&mut h);
    steps(&mut h);
    assert_eq!(h.state().surface, Surface::Convert);
    assert_eq!(w.engine.calls_of("send").len(), 1);
}

// ------------------------------------------------------------------ notes

#[test]
fn o3_o6_the_updates_block_checks_and_says_what_it_found() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    h.get_by_role_and_label(Role::Button, concat!("rev ", env!("CARGO_PKG_VERSION")))
        .click();
    steps(&mut h);
    h.get_by_role_and_label(Role::CheckBox, sp_app::model::update::AUTO_LABEL);
    h.get_by_role_and_label(Role::Button, "Check for updates")
        .click_accesskit();
    steps(&mut h);
    h.get_by_role_and_label(
        Role::Status,
        concat!(
            "Method Reader ",
            env!("CARGO_PKG_VERSION"),
            " is the newest there is."
        ),
    );
}

#[test]
fn o4_elsewhere_the_notes_say_where_releases_are() {
    let w = world();
    let mut h = harness(&w, Platform::Linux);
    h.get_by_role_and_label(Role::Button, concat!("rev ", env!("CARGO_PKG_VERSION")))
        .click();
    steps(&mut h);
    assert!(has(&h, sp_app::model::update::NOT_HERE));
    assert!(
        h.query_by_role_and_label(Role::Button, "Check for updates")
            .is_none()
    );
}

#[test]
fn b3_the_beta_badge_is_read_out_with_its_meaning() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    let badge = h
        .query_all_by_label("Beta")
        .next()
        .expect("the missing card's badge");
    assert_eq!(
        badge.accesskit_node().description().as_deref(),
        Some(sp_app::app::widgets::BETA_NOTE)
    );
}

#[test]
fn a6_live_lines_are_polite_statuses() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    let statuses: Vec<_> = h.query_all_by_role(Role::Status).collect();
    assert!(!statuses.is_empty());
    assert!(
        statuses
            .iter()
            .all(|s| s.accesskit_node().data().live() == Some(Live::Polite))
    );
    let _ = (App::answer_or, Modifiers::NONE);
}

// ------------------------------------------------- what the window paints

/// Every shape the last frame painted, groups opened.
fn painted(h: &H) -> Vec<egui::Shape> {
    fn open(shape: &egui::Shape, out: &mut Vec<egui::Shape>) {
        match shape {
            egui::Shape::Vec(list) => list.iter().for_each(|s| open(s, out)),
            other => out.push(other.clone()),
        }
    }
    let mut out = vec![];
    for clipped in &h.output().shapes {
        open(&clipped.shape, &mut out);
    }
    out
}

/// Whether pressing the pointer at `at` asks the OS to move the window.
fn moves_the_window(h: &mut H, at: egui::Pos2) -> bool {
    h.hover_at(at);
    h.step();
    h.drag_at(at);
    h.step();
    let moved = h
        .output()
        .viewport_output
        .values()
        .any(|v| v.commands.contains(&egui::ViewportCommand::StartDrag));
    h.drop_at(at);
    steps(h);
    moved
}

// The system's title bar moves the window now; nothing in the page does.
#[test]
fn w4_w5_the_page_never_moves_the_window_and_a_tab_is_still_a_tab() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    assert!(
        !moves_the_window(&mut h, egui::pos2(900.0, 10.0)),
        "the tab strip"
    );
    let read = h.get_by_role_and_label(Role::Tab, "Preview").rect();
    let settings = h.get_by_role_and_label(Role::Tab, "Settings").rect();
    let between = egui::pos2((read.right() + settings.left()) / 2.0, read.center().y);
    assert!(!moves_the_window(&mut h, between), "a gap between tabs");
    assert!(!moves_the_window(&mut h, read.center()), "a tab");
    assert_eq!(h.state().surface, Surface::Read, "and the press chose it");
}

// The open tab joins the page (Method Budget's `.doc-tab.is-active`): the
// page's ground, a rule round it, no accent.
#[test]
fn n8_the_open_tab_takes_the_pages_ground_in_a_rule() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    let t = h.state().tokens();
    let tab = h.get_by_role_and_label(Role::Tab, "Convert").rect();
    let shapes = painted(&h);
    let rects: Vec<_> = shapes
        .iter()
        .filter_map(|s| match s {
            egui::Shape::Rect(r) if r.rect.contains(tab.center()) => Some(r.clone()),
            _ => None,
        })
        .collect();
    assert!(rects.iter().any(|r| r.fill == t.paper), "{rects:?}");
    assert!(
        rects
            .iter()
            .any(|r| r.stroke == egui::Stroke::new(1.0, t.rule)),
        "{rects:?}"
    );
    let read = h.get_by_role_and_label(Role::Tab, "Preview").rect();
    assert!(
        !shapes.iter().any(|s| matches!(s,
            egui::Shape::Rect(r) if r.rect.contains(read.center())
                && r.fill == t.paper
                && r.rect.width() < 200.0)),
        "only the open tab"
    );
}

#[test]
fn c19_the_send_panel_is_a_rule_hairline_box_with_the_wells_corners_and_the_panel_fill() {
    let w = world();
    calm_as(&w, "kobo");
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    let t = h.state().tokens();
    let ways = h
        .get_by_role_and_label(Role::Button, "Other ways")
        .rect()
        .center();
    let boxed = painted(&h).into_iter().any(|s| match s {
        egui::Shape::Rect(r) => {
            r.fill == t.panel
                && r.corner_radius == egui::CornerRadius::same(8)
                && r.stroke == egui::Stroke::new(1.0, t.rule)
                && r.rect.contains(ways)
        }
        _ => false,
    });
    assert!(boxed);
}

// The Screepub look's brads and punched hole are gone: nothing on the page
// is painted in the accent's ramp.
#[test]
fn w16_w17_there_are_no_brads_and_no_punched_hole() {
    let w = world();
    let h = harness(&w, Platform::Mac);
    let t = h.state().tokens();
    let ramp = [t.brass_specular, t.brass_edge, t.brass_rim, t.ground];
    for s in painted(&h) {
        if let egui::Shape::Mesh(mesh) = s {
            assert!(
                !mesh.vertices.iter().any(|v| ramp.contains(&v.color)),
                "a brad's colour is painted"
            );
        }
    }
}

#[test]
fn w24_a_long_path_wraps_inside_its_box() {
    let w = world();
    let mut a = default_answer(&Request::AppSettings);
    let long = format!(
        "/{}/Books",
        "a-very-long-folder-name-without-any-spaces".repeat(8)
    );
    a["library"]["path"] = json!(long);
    w.engine.always("app-settings", Reply::ok(a));
    let h = harness_sized(&w, Platform::Mac, [900.0, 2000.0]);
    let mut seen = 0;
    for node in h.query_all_by_label_contains("a-very-long-folder-name") {
        seen += 1;
        assert!(
            node.rect().right() <= 900.0,
            "inside the window: {:?}",
            node.rect()
        );
    }
    assert!(seen > 0, "the path is printed somewhere");
}

#[test]
fn o5_the_switch_is_read_again_each_time_the_sheet_opens() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    let open = |h: &mut H| {
        h.get_by_role_and_label(Role::Button, concat!("rev ", env!("CARGO_PKG_VERSION")))
            .click();
        steps(h);
    };
    let on = |h: &H| {
        h.get_by_role_and_label(Role::CheckBox, sp_app::model::update::AUTO_LABEL)
            .accesskit_node()
            .data()
            .toggled()
            == Some(egui::accesskit::Toggled::True)
    };
    open(&mut h);
    assert!(on(&h), "on by default");
    h.key_press(Key::Escape);
    steps(&mut h);
    sp_app::model::update::set_automatic(h.state_mut().host.storage.as_mut(), false);
    open(&mut h);
    assert!(!on(&h), "what was stored meanwhile");
}

/// A Beta badge on the same line as `rect`, just after it.
fn wears_beta(h: &H, rect: egui::Rect) -> bool {
    h.query_all_by_label("Beta").any(|b| {
        let r = b.rect();
        let y = r.center().y;
        y >= rect.top() - 2.0
            && y <= rect.bottom() + 2.0
            && r.left() >= rect.right() - 1.0
            && r.left() - rect.right() < 40.0
    })
}

fn row(h: &H, words: &str) -> egui::Rect {
    h.query_all_by_value(words)
        .chain(h.query_all_by_label(words))
        .next()
        .unwrap_or_else(|| panic!("{words}"))
        .rect()
}

#[test]
fn b4_the_badge_is_worn_where_the_inventory_says_and_nowhere_else() {
    let w = world();
    calm_as(&w, "kobo");
    let mut h = harness_sized(&w, Platform::Mac, [1280.0, 4000.0]);
    convert(&w, &mut h);
    open_other_ways(&mut h);
    for (save, beta) in [
        ("EPUB", false),
        ("AZW3", false),
        ("MOBI", false),
        ("Fountain", false),
        ("FDX", true),
    ] {
        assert_eq!(wears_beta(&h, row(&h, save)), beta, "{save}");
    }
    settings(&mut h);
    h.get_by_role_and_label(Role::DisclosureTriangle, "What the book carries")
        .click_accesskit();
    steps(&mut h);
    for knob in ["Pictures from the script", "Title card as the cover"] {
        let rect = h.get_by_role_and_label(Role::Label, knob).rect();
        assert!(wears_beta(&h, rect), "{knob}");
    }
    let plain = h
        .get_by_role_and_label(Role::Label, "Start each scene on a new page")
        .rect();
    assert!(!wears_beta(&h, plain));
}

fn focused_label(h: &H) -> Option<String> {
    h.query_all(egui_kittest::kittest::By::new().predicate(|n| n.is_focused()))
        .next()
        .and_then(|n| n.accesskit_node().label())
}

#[test]
fn d26_a_save_from_the_keyboard_gives_it_back_to_its_own_button() {
    let w = world();
    calm_as(&w, "kobo");
    let mut h = harness_sized(&w, Platform::Mac, [1280.0, 3000.0]);
    convert(&w, &mut h);
    open_other_ways(&mut h);
    w.fakes.answer_dialog(Some("/books/field-station.epub"));
    h.get_by_role_and_label(Role::Button, "Save EPUB…").focus();
    steps(&mut h);
    h.key_press(Key::Enter);
    steps(&mut h);
    steps(&mut h);
    assert!(
        w.fakes
            .dialogs
            .asked
            .lock()
            .unwrap()
            .iter()
            .any(|a| a.starts_with("save")),
        "the Save box came up"
    );
    assert_eq!(focused_label(&h).as_deref(), Some("Save EPUB…"));
}

#[test]
fn d26_a_fresh_result_owes_the_brass_the_keyboard() {
    let w = world();
    calm_as(&w, "kindle");
    plug_in_a_kindle(&w);
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    steps(&mut h);
    assert_eq!(focused_label(&h).as_deref(), Some("Copy to Kindle (.azw3)"));
}

#[test]
fn d26_a_card_pressed_from_the_keyboard_hands_it_to_the_brass() {
    let w = world();
    plug_in_a_kindle(&w);
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Button, "Kindle, plugged in")
        .focus();
    steps(&mut h);
    h.key_press(Key::Enter);
    steps(&mut h);
    steps(&mut h);
    assert_eq!(focused_label(&h).as_deref(), Some("Copy to Kindle (.azw3)"));
}

#[test]
fn u9_the_label_and_the_notes_tell_one_story() {
    use sp_app::app::host::FakeUpdater;
    use sp_app::model::update::Found;
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    h.state_mut().host.updater = std::sync::Arc::new(FakeUpdater {
        found: Ok(Some(Found {
            version: "0.12.0".into(),
            current_version: "0.11.1".into(),
            body: Some("Faster pages.".into()),
        })),
        ..FakeUpdater::default()
    });
    h.get_by_role_and_label(Role::Button, concat!("rev ", env!("CARGO_PKG_VERSION")))
        .click();
    steps(&mut h);
    h.get_by_role_and_label(Role::Button, "Check for updates")
        .click_accesskit();
    steps(&mut h);
    h.get_by_role_and_label(Role::Button, "Update to 0.12.0");
    assert!(has(&h, "Faster pages."), "the notes show the same offer");
    // Then a check that finds nothing newer takes both down.
    h.state_mut().host.updater = std::sync::Arc::new(FakeUpdater::default());
    h.get_by_role_and_label(Role::Button, "Check for updates")
        .click_accesskit();
    steps(&mut h);
    assert!(
        h.query_by_role_and_label(Role::Button, "Update to 0.12.0")
            .is_none()
    );
}

#[test]
fn n12_a_label_that_goes_while_focused_hands_the_keyboard_on() {
    use sp_app::model::update::CheckOutcome;
    use sp_app::model::update_flow::Offer;
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    h.state_mut().update.flow.offer_found(Offer {
        version: "0.12.0".into(),
        body: String::new(),
        live: true,
    });
    steps(&mut h);
    h.get_by_role_and_label(Role::Button, "Update to 0.12.0")
        .focus();
    steps(&mut h);
    assert_eq!(focused_label(&h).as_deref(), Some("Update to 0.12.0"));
    h.state_mut()
        .update
        .flow
        .check_answered(&CheckOutcome::Current {
            reason: "0.11.1".into(),
        });
    steps(&mut h);
    let now = focused_label(&h);
    assert!(
        now.is_some() && now.as_deref() != Some("Update to 0.12.0"),
        "the page's first stop: {now:?}"
    );
}

#[test]
fn r16_the_book_is_dressed_again_when_the_os_theme_changes() {
    use sp_app::app::bookview::Place;
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Tab, "Preview").click();
    steps(&mut h);
    let dressed = |h: &H| h.state().views.peek(Place::Read).map(|v| v.dark());
    assert_eq!(dressed(&h), Some(false));
    h.input_mut().system_theme = Some(egui::Theme::Dark);
    steps(&mut h);
    assert_eq!(dressed(&h), Some(true), "re-dressed without a reload");
}

/// The 2px ink rings painted this frame.
fn rings(h: &H) -> Vec<egui::Rect> {
    let t = h.state().tokens();
    painted(h)
        .into_iter()
        .filter_map(|s| match s {
            egui::Shape::Rect(r) if r.stroke == egui::Stroke::new(2.0, t.brass) => Some(r.rect),
            _ => None,
        })
        .collect()
}

fn focused(h: &H) -> Option<egui::Id> {
    h.ctx.memory(|m| m.focused())
}

// A4: rings are :focus-visible's, keyboard only. A focus the window moves
// by itself (restore_focus, after a dialog, a surface change) wears no ring
// until a key has been pressed; Tab shows it; a pointer press hides it
// again until the next key.
#[test]
fn a4_a_focus_the_window_moves_by_itself_wears_no_ring() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    // Cmd-O is a shortcut, not keyboard navigation; a fresh window had
    // no other input. Going to Read moves the focus programmatically.
    h.state_mut().go_to(Surface::Read);
    steps(&mut h);
    assert!(focused(&h).is_some(), "the window placed the focus");
    assert!(rings(&h).is_empty(), "no ring: {:?}", rings(&h));
}

#[test]
fn a4_tab_shows_the_ring_a_click_hides_it_and_the_next_tab_shows_it_again() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    h.key_press(Key::Tab);
    steps(&mut h);
    // The first Tab lands on the first tab, not on the invisible drag strip
    // or the gaps between tabs (they sense drags and take no focus).
    assert_eq!(
        focused(&h),
        Some(sp_app::app::frame::tab_id(Surface::Convert))
    );
    assert_eq!(rings(&h).len(), 1, "Tab shows the ring");
    // A pointer press on a control: focused or not, no ring.
    h.get_by_role_and_label(Role::Tab, "Convert").click();
    steps(&mut h);
    assert!(
        rings(&h).is_empty(),
        "no ring after a click: {:?}",
        rings(&h)
    );
    // A programmatic move after the click keeps the pointer's "no ring".
    h.state_mut().restore_focus = true;
    steps(&mut h);
    assert!(focused(&h).is_some());
    assert!(rings(&h).is_empty(), "no ring after restore_focus");
    h.key_press(Key::Tab);
    steps(&mut h);
    assert_eq!(rings(&h).len(), 1, "the next Tab shows it again");
    // And a programmatic move after a key keeps the keyboard's ring.
    h.state_mut().restore_focus = true;
    steps(&mut h);
    assert_eq!(rings(&h).len(), 1, "the ring follows the keyboard's mode");
}

#[test]
fn a4_the_screen_reader_still_follows_the_focus_without_a_ring() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    h.state_mut().restore_focus = true;
    steps(&mut h);
    let id = focused(&h).expect("the window placed the focus");
    assert!(rings(&h).is_empty());
    let in_tree: Vec<_> = h
        .query_all(egui_kittest::kittest::By::new().predicate(|n| n.is_focused()))
        // The consumer's id is the node's own id in its upper 64 bits.
        .map(|n| (u128::from(n.accesskit_node().id()) >> 64) as u64)
        .collect();
    assert_eq!(
        in_tree,
        vec![id.accesskit_id().0],
        "AccessKit's focus is egui's"
    );
}

#[test]
fn a4_a_focused_control_wears_a_2px_accent_ring_outside_it() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    // The keyboard is in use (rings are keyboard only).
    h.key_press(Key::Tab);
    steps(&mut h);
    let change = h.get_by_role_and_label(Role::Button, "Change…");
    let rect = change.rect();
    change.focus();
    steps(&mut h);
    let t = h.state().tokens();
    let ring = painted(&h).into_iter().find_map(|s| match s {
        egui::Shape::Rect(r) if r.stroke == egui::Stroke::new(2.0, t.brass) => Some(r.rect),
        _ => None,
    });
    let ring = ring.expect("a ring");
    let gap = rect.left() - ring.left();
    assert!((2.0..=3.0).contains(&gap), "offset {gap}");
    assert!(ring.contains_rect(rect));
}

#[test]
fn copying_our_own_azw3_says_a_name_can_land_at_a_pages_foot() {
    // The engine's note rides on its Kindle-file answer; the window says it
    // under the copy, in words the engine chose.
    let note = sp_engine::export::formats::AZW3_LAYOUT_NOTE;
    let w = world();
    calm_as(&w, "kindle");
    plug_in_a_kindle(&w);
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Button, "Copy to Kindle (.azw3)");
    assert!(h.query_by_label(note).is_some(), "the note is shown");
}

#[test]
fn b4_eject_off_a_mac_wears_it_and_our_own_azw3_no_longer_does() {
    for (platform, eject_beta) in [(Platform::Mac, false), (Platform::Linux, true)] {
        let w = world();
        calm_as(&w, "kindle");
        plug_in_a_kindle(&w);
        let mut h = harness(&w, platform);
        convert(&w, &mut h);
        let copy = h
            .get_by_role_and_label(Role::Button, "Copy to Kindle (.azw3)")
            .rect();
        // A Kindle opened one on 2026-10-29 (registry #20).
        assert!(!wears_beta(&h, copy), "our own AZW3");
        h.get_by_role_and_label(Role::Button, "Copy to Kindle (.azw3)")
            .click_accesskit();
        steps(&mut h);
        let eject = h.get_by_role_and_label(Role::Button, "Eject Kindle").rect();
        assert_eq!(wears_beta(&h, eject), eject_beta, "{platform:?}");
    }
}

// ---------------------------------------------------------------- library
// docs/superpowers/specs/2026-11-02-library-view-design.md, part 2.

fn library_row(name: &str, epub: bool) -> serde_json::Value {
    let folder = format!("/Users/reader/Method Reader/{name}");
    json!({"folder": folder, "name": name, "title": name.to_uppercase(),
        "source": format!("/scripts/{name}.pdf"), "sourceExists": true,
        "fountain": format!("{folder}/{name}.fountain"),
        "epub": if epub { json!(format!("{folder}/{name}.epub")) } else { json!(null) },
        "pages": 18, "scenes": 11, "converted": "2026-11-02T18:00:00Z",
        "others": [], "settings": null})
}

fn library_of(rows: Vec<serde_json::Value>) -> Reply {
    Reply::ok(json!({"ok": true, "root": "/Users/reader/Method Reader", "scripts": rows}))
}

#[test]
fn library_open_remakes_the_book_from_its_fountain_and_lands_on_preview() {
    let w = world();
    w.engine.always(
        "library",
        library_of(vec![library_row("Field Station", true)]),
    );
    let mut h = harness(&w, Platform::Mac);
    h.get_by_role_and_label(Role::Tab, "Library").click();
    steps(&mut h);
    h.get_by_role_and_label(Role::Button, "Open FIELD STATION")
        .click_accesskit();
    steps(&mut h);
    let folder = "/Users/reader/Method Reader/Field Station";
    assert_eq!(
        w.engine.calls_of("convert-to"),
        [Request::ConvertTo {
            path: format!("{folder}/Field Station.fountain"),
            out: format!("{folder}/Field Station.epub"),
        }],
        "the book is remade from the library .fountain"
    );
    assert!(w.engine.calls_of("convert").is_empty(), "never the PDF");
    let app = h.state();
    assert_eq!(app.surface, Surface::Read);
    assert!(
        h.get_by_role_and_label(Role::Tab, "Preview")
            .accesskit_node()
            .is_selected()
            == Some(true)
    );
    let script = app.script.as_ref().expect("the script is open");
    assert_eq!(script.path, "/scripts/Field Station.pdf", "its own PDF");
    assert_eq!(script.title, "FIELD STATION");
    assert_eq!((script.pages, script.scenes), (Some(18), Some(11)));
    assert_eq!(
        script.fountain_path.as_deref(),
        Some(format!("{folder}/Field Station.fountain").as_str())
    );
    assert_eq!(
        script.epub_path.as_deref(),
        Some(format!("{folder}/Field Station.epub").as_str())
    );
}

#[test]
fn library_open_makes_the_book_a_row_does_not_have_yet() {
    let w = world();
    w.engine.always(
        "library",
        library_of(vec![library_row("Weather Log", false)]),
    );
    let mut h = harness(&w, Platform::Mac);
    h.get_by_role_and_label(Role::Tab, "Library").click();
    steps(&mut h);
    h.get_by_role_and_label(Role::Button, "Open WEATHER LOG")
        .click_accesskit();
    steps(&mut h);
    let folder = std::path::Path::new("/Users/reader/Method Reader/Weather Log");
    assert_eq!(
        w.engine.calls_of("convert-to"),
        [Request::ConvertTo {
            path: format!("{}/Weather Log.fountain", folder.display()),
            out: folder
                .join("Weather Log.epub")
                .to_string_lossy()
                .into_owned(),
        }]
    );
    assert_eq!(h.state().surface, Surface::Read);
}

#[test]
fn library_asks_each_time_it_is_shown_and_drops_a_stale_answer() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    h.get_by_role_and_label(Role::Tab, "Library").click();
    steps(&mut h);
    h.get_by_role_and_label(Role::Tab, "Convert").click();
    steps(&mut h);
    h.get_by_role_and_label(Role::Tab, "Library").click();
    steps(&mut h);
    assert_eq!(w.engine.calls_of("library").len(), 2, "once per showing");

    // Two asks out at once: the first one's answer is old news.
    w.engine
        .once("library", library_of(vec![library_row("Old News", true)]));
    w.engine.once(
        "library",
        library_of(vec![library_row("Field Station", true)]),
    );
    sp_app::app::library::look(h.state_mut());
    sp_app::app::library::look(h.state_mut());
    steps(&mut h);
    assert!(has(&h, "Open FIELD STATION"));
    assert!(!has(&h, "Open OLD NEWS"), "the stale answer is dropped");
}

#[test]
fn library_open_waits_while_a_conversion_runs() {
    let w = world();
    w.engine.hold("convert");
    let mut h = harness_threads(&w, Platform::Mac);
    h.get_by_role_and_label(Role::Tab, "Library").click();
    settle(&mut h);
    w.fakes.answer_dialog(Some("/scripts/field-station.pdf"));
    h.key_press_modifiers(Modifiers::COMMAND, Key::O);
    settle(&mut h);
    assert!(h.state().convert.converting);
    sp_app::app::library::look(h.state_mut());
    settle(&mut h);
    h.state_mut().go_to(Surface::Library);
    settle(&mut h);
    let open = h.get_by_role_and_label(Role::Button, "Open FIELD STATION");
    assert!(
        open.accesskit_node().is_disabled(),
        "one conversion at a time"
    );
    w.engine.release("convert");
    settle(&mut h);
}

// docs/superpowers/specs/2026-11-03-library-row-settings-design.md.

#[test]
fn library_settings_reopens_from_the_fountain_and_lands_on_settings_for_that_script() {
    let w = world();
    let mut own = library_row("Weather Log", true);
    own["settings"] =
        json!("/Users/reader/Method Reader/Weather Log/Weather Log.method-reader.json");
    w.engine.always(
        "library",
        library_of(vec![library_row("Field Station", true), own]),
    );
    let mut h = harness(&w, Platform::Mac);
    // Another script open first, its settings already read.
    convert(&w, &mut h);
    let before = h
        .state()
        .script
        .as_ref()
        .and_then(|s| s.fountain_path.clone())
        .expect("a script open");
    h.get_by_role_and_label(Role::Tab, "Settings").click();
    steps(&mut h);
    assert_eq!(
        w.engine.calls_of("settings"),
        [Request::Settings {
            fountain: before.clone(),
            set: None,
        }]
    );
    h.get_by_role_and_label(Role::Tab, "Library").click();
    steps(&mut h);
    // Only the row with settings of its own says so.
    assert!(has(
        &h,
        "18 pages, 11 scenes, converted 2 November 2026, its own settings"
    ));
    assert!(has(&h, "18 pages, 11 scenes, converted 2 November 2026"));
    h.get_by_role_and_label(Role::Button, "Settings for FIELD STATION");
    let asked = w.engine.calls_of("settings").len();
    h.get_by_role_and_label(Role::Button, "Settings for WEATHER LOG")
        .click_accesskit();
    steps(&mut h);
    let folder = "/Users/reader/Method Reader/Weather Log";
    let fountain = format!("{folder}/Weather Log.fountain");
    assert_eq!(
        w.engine.calls_of("convert-to"),
        [Request::ConvertTo {
            path: fountain.clone(),
            out: format!("{folder}/Weather Log.epub"),
        }],
        "the book is remade from the library .fountain, once"
    );
    assert_eq!(w.engine.calls_of("convert").len(), 1, "never its PDF");
    let app = h.state();
    assert_eq!(app.surface, Surface::Tune);
    assert!(
        h.get_by_role_and_label(Role::Tab, "Settings")
            .accesskit_node()
            .is_selected()
            == Some(true)
    );
    let script = app.script.as_ref().expect("the script is open");
    assert_eq!(script.path, "/scripts/Weather Log.pdf", "its own PDF");
    assert_eq!(script.fountain_path.as_deref(), Some(fountain.as_str()));
    // The tab asks for the reopened script's settings, and only for them.
    assert_eq!(
        w.engine.calls_of("settings")[asked..],
        [Request::Settings {
            fountain,
            set: None,
        }]
    );
}

#[test]
fn library_settings_waits_while_a_conversion_runs() {
    let w = world();
    w.engine.hold("convert");
    let mut h = harness_threads(&w, Platform::Mac);
    h.get_by_role_and_label(Role::Tab, "Library").click();
    settle(&mut h);
    w.fakes.answer_dialog(Some("/scripts/field-station.pdf"));
    h.key_press_modifiers(Modifiers::COMMAND, Key::O);
    settle(&mut h);
    assert!(h.state().convert.converting);
    sp_app::app::library::look(h.state_mut());
    settle(&mut h);
    h.state_mut().go_to(Surface::Library);
    settle(&mut h);
    let tune = h.get_by_role_and_label(Role::Button, "Settings for FIELD STATION");
    assert!(
        tune.accesskit_node().is_disabled(),
        "one conversion at a time"
    );
    w.engine.release("convert");
    settle(&mut h);
    let tune = h.get_by_role_and_label(Role::Button, "Settings for FIELD STATION");
    assert!(!tune.accesskit_node().is_disabled());
}

// docs/superpowers/specs/2026-11-03-library-remove-design.md, part 2.

fn library_tab_with(w: &World, rows: Vec<serde_json::Value>) -> H {
    w.engine.always("library", library_of(rows));
    let mut h = harness(w, Platform::Mac);
    h.get_by_role_and_label(Role::Tab, "Library").click();
    steps(&mut h);
    h
}

fn ask_to_remove(h: &mut H, title: &str) {
    h.get_by_role_and_label(Role::Button, &format!("Remove {title} from the library"))
        .click_accesskit();
    steps(h);
}

#[test]
fn library_remove_asks_first_with_cancel_focused_and_cancel_sends_nothing() {
    let w = world();
    let mut h = library_tab_with(
        &w,
        vec![
            library_row("Field Station", true),
            library_row("Weather Log", false),
        ],
    );
    // Each row has its Remove from library beside its Open.
    let remove = h.get_by_role_and_label(Role::Button, "Remove FIELD STATION from the library");
    assert!(!remove.accesskit_node().is_disabled());
    h.get_by_role_and_label(Role::Button, "Remove WEATHER LOG from the library");
    ask_to_remove(&mut h, "FIELD STATION");
    assert!(has(&h, "Move FIELD STATION to the Trash?"));
    assert!(has(&h, &sp_app::model::library::question_note(false)));
    h.get_by_role_and_label(Role::Button, "Move FIELD STATION to the Trash");
    assert_eq!(
        focused_label(&h).as_deref(),
        Some("Cancel"),
        "the keyboard lands on Cancel"
    );
    assert!(w.engine.calls_of("library-remove").is_empty(), "not yet");
    // The question replaces the row's own Remove until it is answered.
    assert!(
        h.query_by_role_and_label(Role::Button, "Remove FIELD STATION from the library")
            .is_none()
    );
    h.get_by_role_and_label(Role::Button, "Cancel")
        .click_accesskit();
    steps(&mut h);
    assert!(!has(&h, "Move FIELD STATION to the Trash?"));
    assert!(w.engine.calls_of("library-remove").is_empty());
    h.get_by_role_and_label(Role::Button, "Remove FIELD STATION from the library");
}

#[test]
fn library_move_to_trash_sends_one_removal_then_lists_again_and_says_so() {
    let w = world();
    let mut h = library_tab_with(
        &w,
        vec![
            library_row("Field Station", true),
            library_row("Weather Log", false),
        ],
    );
    assert_eq!(w.engine.calls_of("library").len(), 1);
    ask_to_remove(&mut h, "WEATHER LOG");
    // The library now lists only the script that stays.
    w.engine.always(
        "library",
        library_of(vec![library_row("Field Station", true)]),
    );
    h.get_by_role_and_label(Role::Button, "Move WEATHER LOG to the Trash")
        .click_accesskit();
    steps(&mut h);
    assert_eq!(
        w.engine.calls_of("library-remove"),
        [Request::LibraryRemove {
            folder: "/Users/reader/Method Reader/Weather Log".into(),
        }]
    );
    assert_eq!(w.engine.calls_of("library").len(), 2, "listed again");
    h.get_by_role_and_label(Role::Status, "WEATHER LOG is in the Trash.");
    assert!(
        h.query_by_role_and_label(Role::Button, "Open WEATHER LOG")
            .is_none()
    );
    h.get_by_role_and_label(Role::Button, "Open FIELD STATION");
}

#[test]
fn library_a_refused_removal_says_the_engines_sentence_and_keeps_the_row() {
    let w = world();
    w.engine.always(
        "library-remove",
        Reply::ok(json!({"ok": false, "error": {"code": "not-a-script",
            "message": "not a script in the library (it is a link): /Users/reader/Method Reader/Field Station"}})),
    );
    let mut h = library_tab_with(&w, vec![library_row("Field Station", true)]);
    ask_to_remove(&mut h, "FIELD STATION");
    h.get_by_role_and_label(Role::Button, "Move FIELD STATION to the Trash")
        .click_accesskit();
    steps(&mut h);
    assert_eq!(w.engine.calls_of("library-remove").len(), 1);
    h.get_by_role_and_label(
        Role::Status,
        "not a script in the library (it is a link): /Users/reader/Method Reader/Field Station",
    );
    h.get_by_role_and_label(Role::Button, "Open FIELD STATION");
    h.get_by_role_and_label(Role::Button, "Remove FIELD STATION from the library");
}

#[test]
fn library_remove_waits_while_a_conversion_or_another_removal_runs() {
    let w = world();
    w.engine.always(
        "library",
        library_of(vec![
            library_row("Field Station", true),
            library_row("Weather Log", false),
        ]),
    );
    w.engine.hold("library-remove");
    let mut h = harness_threads(&w, Platform::Mac);
    h.get_by_role_and_label(Role::Tab, "Library").click();
    settle(&mut h);
    h.get_by_role_and_label(Role::Button, "Remove FIELD STATION from the library")
        .click_accesskit();
    settle(&mut h);
    h.get_by_role_and_label(Role::Button, "Move FIELD STATION to the Trash")
        .click_accesskit();
    settle(&mut h);
    assert!(has(&h, "Moving FIELD STATION to the Trash…"));
    assert!(!h.state().quiet(), "the removal is out");
    let other = h.get_by_role_and_label(Role::Button, "Remove WEATHER LOG from the library");
    assert!(
        other.accesskit_node().is_disabled(),
        "one removal at a time"
    );
    let going = h.get_by_role_and_label(Role::Button, "Open FIELD STATION");
    assert!(
        going.accesskit_node().is_disabled(),
        "not opened on its way out"
    );
    w.engine.release("library-remove");
    settle(&mut h);
    let other = h.get_by_role_and_label(Role::Button, "Remove WEATHER LOG from the library");
    assert!(!other.accesskit_node().is_disabled());

    // While a conversion runs, nothing goes.
    w.engine.hold("convert");
    w.fakes.answer_dialog(Some("/scripts/field-station.pdf"));
    h.key_press_modifiers(Modifiers::COMMAND, Key::O);
    settle(&mut h);
    assert!(h.state().convert.converting);
    h.state_mut().go_to(Surface::Library);
    settle(&mut h);
    let remove = h.get_by_role_and_label(Role::Button, "Remove WEATHER LOG from the library");
    assert!(
        remove.accesskit_node().is_disabled(),
        "not during a conversion"
    );
    w.engine.release("convert");
    settle(&mut h);
}

#[test]
fn library_remove_says_recycle_bin_on_windows() {
    let w = world();
    w.engine.always(
        "library",
        library_of(vec![library_row("Field Station", true)]),
    );
    let mut h = harness(&w, Platform::Windows);
    h.get_by_role_and_label(Role::Tab, "Library").click();
    steps(&mut h);
    ask_to_remove(&mut h, "FIELD STATION");
    assert!(has(&h, "Move FIELD STATION to the Recycle Bin?"));
    assert!(has(&h, &sp_app::model::library::question_note(true)));
    assert!(!has(&h, "Move FIELD STATION to the Trash?"));
    h.get_by_role_and_label(Role::Button, "Move FIELD STATION to the Recycle Bin")
        .click_accesskit();
    steps(&mut h);
    h.get_by_role_and_label(Role::Status, "FIELD STATION is in the Recycle Bin.");
}
