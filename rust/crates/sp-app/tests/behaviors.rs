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

#[test]
fn w1_w2_w3_the_window_opens_at_1280_by_800_with_a_floor_and_no_title_bar() {
    let v = sp_app::app::viewport();
    assert_eq!(v.title.as_deref(), Some("Screepub"));
    assert_eq!(v.inner_size, Some(egui::vec2(1280.0, 800.0)));
    assert_eq!(v.min_inner_size, Some(egui::vec2(640.0, 480.0)));
    assert_eq!(v.resizable, Some(true));
    assert_eq!(v.titlebar_shown, Some(false));
    assert_eq!(v.title_shown, Some(false));
    assert_eq!(v.fullsize_content_view, Some(true));
}

#[test]
fn w4_the_drag_strip_covers_no_control() {
    let w = world();
    let h = harness(&w, Platform::Mac);
    let tab = h.get_by_role_and_label(Role::Tab, "Convert").rect();
    assert!(
        tab.top() >= 32.0,
        "the strip is the sheet's top padding: {tab:?}"
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
    assert!(!focusable(&h, "Read"));
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
    h.get_by_role_and_label(Role::Tab, "Read").click();
    steps(&mut h);
    {
        let app = h.state_mut();
        app.script = None;
        app.script_changed();
    }
    steps(&mut h);
    assert_eq!(h.state().surface, Surface::Convert);
    assert!(h.query_by_role_and_label(Role::Tab, "Read").is_none());
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
    h.get_by_role_and_label(Role::Heading, "FIELD STATION IS READY");
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
        .query_all_by_value("Needs selectable text, not a scan.")
        .next()
        .unwrap()
        .rect();
    let heading = h.get_by_role_and_label(Role::Heading, "Screepub").rect();
    // The block is 100..1232 at 1280: an 820 column centred in it.
    // The harness frames the window in 8 points: measure from the block.
    let left = h.get_by_role_and_label(Role::Tab, "Convert").rect().left();
    let centre = left + 1132.0 / 2.0;
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
    h.get_by_role_and_label(Role::Tab, "Read").click();
    steps(&mut h);
    h.get_by_role_and_label(Role::Button, "Send").click();
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
    h.get_by_role_and_label(Role::Tab, "Read").click();
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
    h.get_by_role_and_label(Role::Heading, "CONVERTING");
    assert!(has(&h, "field-station.pdf"));
    assert!(
        has(&h, "EXT. RIDGE ROAD - DAY"),
        "the typed page's words, whole, from the first moment"
    );
    assert!(has(&h, "Thirty-one."));
    h.get_by_role_and_label(Role::Status, "Starting up");
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
    h.get_by_role_and_label(Role::Heading, "SCANNED PDF, NO TEXT");
}

// -------------------------------------------------------------------- read

#[test]
fn r3_under_the_fold_the_index_sits_above_the_script() {
    let w = world();
    let mut h = harness_sized(&w, Platform::Mac, [900.0, 800.0]);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Tab, "Read").click();
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

#[test]
fn r9_r10_pressing_a_scene_scrolls_to_it_and_marks_it() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Tab, "Read").click();
    steps(&mut h);
    h.get_by_role_and_label(Role::Button, "EXT. RIDGE – NIGHT")
        .click();
    steps(&mut h);
    let marks = h.state().read.marks.clone();
    let ridge = marks
        .iter()
        .find(|m| m.id.ends_with("6") || m.id.ends_with("06"))
        .cloned();
    assert!(
        h.state().read.offset > 1000.0,
        "scrolled down the script: {}",
        h.state().read.offset
    );
    let current = h.get_by(|n| n.data().aria_current().is_some());
    assert_eq!(
        current.accesskit_node().label().as_deref(),
        Some("EXT. RIDGE – NIGHT")
    );
    let _ = ridge;
}

#[test]
fn r15_a_settings_save_rebuilds_what_read_shows_and_keeps_the_place() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Tab, "Read").click();
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
    h.get_by_role_and_label(Role::Tab, "Read").click();
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
    h.get_by_role_and_label(Role::Button, "Send");
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
        "Read from the PDF",
    ] {
        h.get_by_role_and_label(Role::Button, group)
            .click_accesskit();
        steps(&mut h);
    }
    assert!(has(
        &h,
        "These four are decided while the PDF is being read. Everything else re-renders from the script Screepub already read, so changing one here saves it for the next time you convert this PDF. It will not change what you see now."
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
    assert!(has(&h, "New scripts start from Screepub’s own defaults."));
    assert!(
        h.query_by_role_and_label(Role::Button, "Reset new scripts to Screepub’s defaults")
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
    assert!(!has(&h, "WHAT DO YOU READ ON?"));
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
    h.get_by_role_and_label(Role::Tab, "Read").click();
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
    h.get_by_role_and_label(Role::Tab, "Read").click();
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
    h.get_by_role_and_label(Role::Button, "rev 0.11.1").click();
    steps(&mut h);
    h.get_by_role_and_label(Role::CheckBox, sp_app::model::update::AUTO_LABEL);
    h.get_by_role_and_label(Role::Button, "Check for updates")
        .click_accesskit();
    steps(&mut h);
    h.get_by_role_and_label(Role::Status, "Screepub 0.11.1 is the newest there is.");
}

#[test]
fn o4_elsewhere_the_notes_say_where_releases_are() {
    let w = world();
    let mut h = harness(&w, Platform::Linux);
    h.get_by_role_and_label(Role::Button, "rev 0.11.1").click();
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
