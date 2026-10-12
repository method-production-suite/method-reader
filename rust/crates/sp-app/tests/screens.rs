//! The window's screens, driven headless with the fake engine, asserted
//! through the AccessKit tree: what a screen shows is what a screen reader
//! is told, by role and name. Each test names the desktop/ui test it ports
//! (rust/PARITY.md, "Window test files").

use std::sync::Arc;
use std::sync::atomic::Ordering;

use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::{By, NodeT, Queryable};
use serde_json::{Value, json};

use sp_app::app::App;
use sp_app::app::host::Fakes;
use sp_app::engine::{FakeEngine, Reply, Request};
use sp_app::model::convert::Platform;
use sp_app::runtime::Mode;

struct World {
    engine: Arc<FakeEngine>,
    fakes: Fakes,
}

fn world() -> World {
    World {
        engine: Arc::new(FakeEngine::new()),
        fakes: Fakes::default(),
    }
}

fn harness(w: &World, platform: Platform) -> Harness<'static, App> {
    let app = App::new(w.engine.clone(), Mode::Inline, w.fakes.host(platform));
    let mut h = Harness::builder()
        .with_size([1280.0, 800.0])
        .build_ui_state(|ui, app: &mut App| app.frame(ui), app);
    h.run_steps(4);
    h
}

fn steps(h: &mut Harness<'static, App>) {
    h.run_steps(4);
}

fn has_value(h: &Harness<'static, App>, words: &str) -> bool {
    h.query_all_by_value(words).next().is_some() || h.query_all_by_label(words).next().is_some()
}

fn convert(w: &World, h: &mut Harness<'static, App>) {
    w.fakes.answer_dialog(Some("/scripts/field-station.pdf"));
    h.key_press_modifiers(Modifiers::COMMAND, Key::O);
    steps(h);
}

// desktop-ui.test.ts: "the Convert surface", "the drop well states one guard"
#[test]
fn the_idle_screen_names_the_app_and_asks_for_a_pdf() {
    let w = world();
    let h = harness(&w, Platform::Mac);
    h.get_by_role_and_label(Role::Heading, "Screepub");
    h.get_by_role_and_label(Role::Button, "Choose PDF…  ⌘O");
    assert!(has_value(&h, "Needs selectable text, not a scan."));
    assert!(has_value(&h, "Books are saved in ~/Screepub."));
    h.get_by_role_and_label(Role::Button, "Change…");
    assert!(
        h.query_by_role_and_label(Role::Button, "Reset").is_none(),
        "nothing chosen to reset"
    );
    h.get_by_role_and_label(Role::Button, "rev 0.11.1");
}

// desktop-ui.test.ts: "a surface with nothing behind it is absent, not dimmed"
#[test]
fn read_and_settings_are_absent_until_a_script_is_open() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    h.get_by_role_and_label(Role::Tab, "Convert");
    assert!(h.query_by_role_and_label(Role::Tab, "Read").is_none());
    assert!(h.query_by_role_and_label(Role::Tab, "Settings").is_none());
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Tab, "Read");
    h.get_by_role_and_label(Role::Tab, "Settings");
    h.get_by_role(Role::TabList);
}

// desktop-ui.test.ts: "the Convert page asks once whether to look for new versions"
#[test]
fn the_update_question_is_asked_on_a_mac_once_and_never_elsewhere() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    assert!(has_value(&h, "Check for new versions once a day?"));
    h.get_by_role_and_label(Role::Button, "No thanks").click();
    steps(&mut h);
    assert!(!has_value(&h, "Check for new versions once a day?"));
    let linux = harness(&world(), Platform::Linux);
    assert!(!has_value(&linux, "Check for new versions once a day?"));
    linux.get_by_role_and_label(Role::Button, "Choose PDF…  Ctrl+O");
}

// desktop-ui.test.ts: "what the Convert surface decides" (driven end to end)
#[test]
fn a_conversion_ends_on_the_result_with_its_counts_and_the_device_cards() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    let call = &w.engine.calls_of("convert")[0];
    assert_eq!(
        call.argv(),
        [
            "/scripts/field-station.pdf",
            "--json",
            "--progress",
            "--preview-inline",
            "--library"
        ]
    );
    h.get_by_role_and_label(Role::Heading, "FIELD STATION IS READY");
    assert!(has_value(&h, "19 pages, 11 scenes"));
    h.get_by_role_and_label(Role::Heading, "WHAT DO YOU READ ON?");
    for name in [
        "Kindle",
        "iPhone, iPad or Mac",
        "Kobo",
        "tolino",
        "reMarkable",
        "My device isn’t here",
    ] {
        h.get_by_role_and_label(Role::Button, name);
    }
    h.get_by_role_and_label(Role::Button, "About Kobo: never tried on real hardware");
    assert!(
        h.query_by_role_and_label(Role::Button, "About Kindle: never tried on real hardware")
            .is_none(),
        "a Kindle on a Mac is proven"
    );
    for link in ["Read it", "Settings", "Convert another"] {
        h.get_by_role_and_label(Role::Button, link);
    }
}

// desktop-layout-measured.test.ts: "every card sits inside the panel"
#[test]
fn the_device_cards_do_not_overlap() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    let names = [
        "Kindle",
        "iPhone, iPad or Mac",
        "Kobo",
        "tolino",
        "reMarkable",
        "My device isn’t here",
    ];
    let rects: Vec<egui::Rect> = names
        .iter()
        .map(|n| h.get_by_role_and_label(Role::Button, n).rect())
        .collect();
    for (i, a) in rects.iter().enumerate() {
        for b in &rects[i + 1..] {
            assert!(
                !a.shrink(0.5).intersects(b.shrink(0.5)),
                "{a:?} overlaps {b:?}"
            );
        }
    }
}

// desktop-ui.test.ts: "what the Convert surface decides about a refusal"
#[test]
fn a_refusal_names_its_cause_and_only_not_a_screenplay_can_be_overruled() {
    let w = world();
    w.engine.once("convert", Reply::ok(json!({"ok": false, "error": {"code": "not-screenplay",
        "message": "No scene headings and no dialogue found: this does not look like a screenplay. Pass --force to convert it anyway."}})));
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Heading, "NOT A SCREENPLAY");
    assert!(has_value(
        &h,
        "No scene headings and no dialogue found: this does not look like a screenplay."
    ));
    assert!(has_value(&h, "not-screenplay"), "the error code chip");
    h.get_by_role_and_label(Role::Button, "Report a bug");
    h.get_by_role_and_label(Role::Button, "Convert anyway")
        .click();
    steps(&mut h);
    let forced = w.engine.calls_of("convert");
    assert!(
        forced
            .last()
            .unwrap()
            .argv()
            .contains(&"--force".to_string())
    );
    h.get_by_role_and_label(Role::Heading, "FIELD STATION IS READY");
}

#[test]
fn a_scan_has_no_override_and_back_to_one_returns_to_the_well() {
    let w = world();
    w.engine.once(
        "convert",
        Reply::ok(
            json!({"ok": false, "error": {"code": "scanned", "message": "This PDF is a scan."}}),
        ),
    );
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Heading, "SCANNED PDF, NO TEXT");
    assert!(
        h.query_by_role_and_label(Role::Button, "Convert anyway")
            .is_none()
    );
    h.get_by_role_and_label(Role::Button, "Back to one").click();
    steps(&mut h);
    h.get_by_role_and_label(Role::Heading, "Screepub");
}

// desktop-ui.test.ts: "a refused file is no longer a dead end" (Report a bug)
#[test]
fn report_a_bug_opens_the_issue_tracker_with_the_code_and_no_paths() {
    let w = world();
    w.engine.once("convert", Reply::ok(json!({"ok": false, "error": {"code": "unreadable", "message": "cannot read /Users/reader/Secret Title.pdf"}})));
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Button, "Report a bug")
        .click();
    steps(&mut h);
    let opened = w.fakes.opener.opened.lock().unwrap().clone();
    assert_eq!(opened.len(), 1);
    assert!(opened[0].starts_with("https://github.com/ssandweiss/screepub/issues/new?body="));
    assert!(opened[0].contains("unreadable"));
    assert!(
        !opened[0].contains("Secret"),
        "the file name never reaches a public URL"
    );
}

#[test]
fn an_engine_that_cannot_run_is_a_refusal_with_its_sentence() {
    let w = world();
    w.engine.once(
        "convert",
        Reply::failed("could not start the Screepub engine"),
    );
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Heading, "THE ENGINE DID NOT ANSWER");
    assert!(has_value(&h, "could not start the Screepub engine"));
}

// desktop-ui.test.ts: "the foot of the page names the release" (dead engine)
#[test]
fn a_dead_engine_says_so_and_keeps_the_stamp() {
    let w = world();
    w.engine.once(
        "version",
        Reply::failed("could not start the Screepub engine"),
    );
    let h = harness(&w, Platform::Mac);
    assert!(has_value(&h, "could not start the Screepub engine"));
    h.get_by_role_and_label(Role::Button, "rev 0.11.1");
}

// desktop-ui.test.ts: "the window, booted whole": the stamp opens the notes
#[test]
fn the_stamp_opens_the_release_notes_and_close_closes_them() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    h.get_by_role_and_label(Role::Button, "rev 0.11.1").click();
    steps(&mut h);
    h.get_by_role_and_label(Role::Dialog, "Release notes");
    h.get_by_role_and_label(Role::Heading, "Screepub 0.11.1");
    assert!(has_value(
        &h,
        "Your Kindle file is ready almost instantly, with nothing else to install."
    ));
    h.get_by_role_and_label(Role::Button, "Check for updates");
    h.get_by_role_and_label(Role::Button, "Close").click();
    steps(&mut h);
    assert!(h.query_by_role(Role::Dialog).is_none());
}

// send-panel-ui.test.ts: "the cards", "the calm screen", "Other ways"
#[test]
fn a_card_goes_calm_at_once_stores_the_choice_and_other_ways_opens_in_place() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Button, "Kobo").click();
    steps(&mut h);
    let stored = w.engine.calls_of("app-settings-set");
    assert_eq!(
        stored.last().unwrap().argv().last().unwrap(),
        r#"{"readerDevice":"kobo"}"#
    );
    h.get_by_role_and_label(Role::Button, "Save the EPUB…");
    assert!(has_value(
        &h,
        "Plug your Kobo in over USB and this becomes a Copy button."
    ));
    let ways = h.get_by_role_and_label(Role::Button, "Other ways");
    assert_eq!(ways.accesskit_node().data().is_expanded(), Some(false));
    ways.click();
    steps(&mut h);
    h.get_by_role_and_label(Role::Heading, "SEND IT");
    h.get_by_role_and_label(Role::Heading, "SAVE A FILE");
    h.get_by_role_and_label(Role::Button, "Open in Books");
    h.get_by_role_and_label(Role::Button, "Save AZW3…");
    assert!(has_value(
        &h,
        "Kindles over USB, made by Screepub with nothing else installed."
    ));
    h.get_by_role_and_label(Role::Button, "Show in Finder");
    assert!(
        h.query_all_by_label("Beta").count() >= 2,
        "AZW3 and FDX wear the badge"
    );
    h.get_by_role_and_label(Role::Button, "Different device")
        .click();
    steps(&mut h);
    h.get_by_role_and_label(Role::Heading, "WHAT DO YOU READ ON?");
}

fn kindle_plugged_in(w: &World) {
    let mut routes = sp_app::engine::default_answer(&Request::Routes {
        epub: String::new(),
        quick: false,
    });
    let list = routes["routes"].as_array_mut().unwrap();
    let kindle = list
        .iter_mut()
        .find(|r| r["id"] == "device:kindle")
        .unwrap();
    *kindle = json!({"id": "device:kindle", "key": "device:kindle", "title": "Kindle",
        "detail": "copies over USB", "button": "Copy to Kindle", "available": true,
        "device": {"id": "k1", "kind": "kindle", "name": "Kindle", "volume": "/Volumes/Kindle"}});
    routes["chosen"] = json!("device:kindle");
    w.engine.always("routes", Reply::ok(routes));
    w.engine.always(
        "devices",
        Reply::ok(json!({"ok": true, "devices": [
        {"id": "k1", "kind": "kindle", "name": "Kindle", "volume": "/Volumes/Kindle"}]})),
    );
}

// send-panel-ui.test.ts: "after a copy over USB"
#[test]
fn a_copy_to_a_kindle_ends_with_eject_and_eject_lets_it_go() {
    let w = world();
    kindle_plugged_in(&w);
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Button, "Kindle, plugged in")
        .click();
    steps(&mut h);
    h.get_by_role_and_label(Role::Button, "Copy to Kindle (.azw3)")
        .click();
    steps(&mut h);
    steps(&mut h);
    let export = w.engine.calls_of("export");
    assert!(
        export
            .last()
            .unwrap()
            .argv()
            .windows(2)
            .any(|p| p == ["--for", "kindle"])
    );
    let send = w.engine.calls_of("send");
    assert_eq!(
        send.last().unwrap().argv()[3..],
        ["--device".to_string(), "k1".to_string()]
    );
    assert!(has_value(
        &h,
        "Field Station is on your Kindle. Eject it before you unplug."
    ));
    h.get_by_role_and_label(Role::Button, "Eject Kindle")
        .click();
    steps(&mut h);
    assert!(has_value(
        &h,
        "Your Kindle is ejected. You can unplug it now."
    ));
    assert!(
        h.query_by_role_and_label(Role::Button, "Eject Kindle")
            .is_none()
    );
}

// send-panel-ui.test.ts: "My device isn’t here"
#[test]
fn my_device_isnt_here_sends_the_name_and_nothing_else() {
    let w = world();
    *w.fakes.poster.answer.lock().unwrap() = Some((200, Some(json!({"ok": true}))));
    let mut h = harness(&w, Platform::Linux);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Button, "My device isn’t here")
        .click();
    steps(&mut h);
    let send = h.get_by_role_and_label(Role::Button, "Send");
    assert!(send.accesskit_node().is_disabled(), "nothing typed yet");
    h.get_by_role(Role::TextInput).focus();
    steps(&mut h);
    h.get_by_role(Role::TextInput).type_text("Boox Note Air");
    steps(&mut h);
    h.get_by_role_and_label(Role::Button, "Send").click();
    for _ in 0..50 {
        steps(&mut h);
        if has_value(&h, "Thanks. We count every request.") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(has_value(&h, "Thanks. We count every request."));
    let sent = w.fakes.poster.sent.lock().unwrap().clone();
    assert_eq!(
        sent[0].1,
        json!({"kind": "device", "device": "Boox Note Air", "version": "0.11.1", "platform": "linux"})
    );
}

// desktop-ui.test.ts: "the Read surface", "the scene index", "the scene list is one Tab stop"
#[test]
fn read_shows_the_script_with_its_scene_index() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Tab, "Read").click();
    steps(&mut h);
    h.get_by_role_and_label(Role::Heading, "FIELD STATION");
    assert!(has_value(&h, "by the Screepub project"));
    let toggle = h.get_by_role_and_label(Role::Button, "Hide scenes");
    assert_eq!(toggle.accesskit_node().data().is_expanded(), Some(true));
    h.get_by_role_and_label(Role::Navigation, "Scenes");
    assert!(has_value(&h, "11 SCENES"));
    h.get_by_role_and_label(Role::Button, "EXT. RIDGE ROAD – DAY");
    h.get_by_role_and_label(
        Role::Button,
        "EXT. RIDGE - NIGHT".replace(" - ", " – ").as_str(),
    );
    h.get_by_role_and_label(Role::Group, "Field Station, the script");
    assert!(
        has_value(&h, "Thirty-one."),
        "the script's words are readable"
    );
    toggle.click();
    steps(&mut h);
    h.get_by_role_and_label(Role::Button, "Show scenes");
    // It fades for 0.2 s first, still there (surfaces.css: visibility
    // follows the transition), then it is gone.
    w.fakes.clock.advance(200.0);
    steps(&mut h);
    assert!(h.query_by_role(Role::Navigation).is_none());
}

#[test]
fn a_book_with_no_pages_says_so_and_offers_the_ways_on() {
    let w = world();
    w.engine.once(
        "convert",
        Reply::ok({
            let mut a = sp_app::engine::default_answer(&Request::Convert {
                path: String::new(),
                force: false,
                options: None,
            });
            a["previewHtml"] = json!("");
            a
        }),
    );
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Tab, "Read").click();
    steps(&mut h);
    h.get_by_role_and_label(Role::Heading, "No pages came back");
    h.get_by_role_and_label(Role::Button, "Convert it again");
    h.get_by_role_and_label(Role::Button, "Send").click();
    steps(&mut h);
    h.get_by_role_and_label(Role::Heading, "FIELD STATION IS READY");
}

// desktop-ui.test.ts: "the Tune surface", "the Settings page, mounted"
#[test]
fn settings_load_the_scripts_own_knobs_and_explain_the_idle_ones() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Tab, "Settings").click();
    steps(&mut h);
    h.get_by_role_and_label(Role::Heading, "This script’s settings");
    let kindle = h.get_by_role_and_label(Role::Button, "Kindle e-ink");
    assert!(
        kindle.accesskit_node().is_disabled(),
        "the preset it is on is marked and inert"
    );
    h.get_by_role_and_label(Role::Button, "Phone");
    assert!(has_value(&h, "Overwrites every setting below."));
    h.get_by_role_and_label(Role::Button, "The page");
    assert!(
        h.query_by_role_and_label(Role::Button, "Dialogue")
            .is_some()
    );
    assert!(
        !has_value(&h, "Character names"),
        "only the first group arrives open"
    );
    // Below the fold at 800 tall: pressed as a screen reader presses it.
    h.get_by_role_and_label(Role::Button, "Dialogue")
        .click_accesskit();
    steps(&mut h);
    let indent = h.get_by_role_and_label(Role::Slider, "Character name indent");
    assert!(indent.accesskit_node().is_disabled());
    assert!(has_value(&h, "Only when character names are indented."));
    h.get_by_role_and_label(Role::RadioGroup, "When a PDF is converted");
    h.get_by_role_and_label(Role::Group, "While a script converts");
}

// desktop-ui.test.ts: "the Tune surface" (the save and the rebuild)
#[test]
fn a_preset_saves_after_the_settle_and_rebuilds_the_book() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Tab, "Settings").click();
    steps(&mut h);
    h.get_by_role_and_label(Role::Button, "Phone").click();
    steps(&mut h);
    assert!(has_value(&h, "Changed. Saving in a moment…"));
    assert!(h.state().busy.engine_busy(), "a settle holds a restart off");
    assert!(
        w.engine.calls_of("settings-set").is_empty(),
        "not before the settle"
    );
    w.fakes.clock.advance(301.0);
    steps(&mut h);
    steps(&mut h);
    let set = w.engine.calls_of("settings-set");
    assert_eq!(set.len(), 1);
    assert!(
        set[0]
            .argv()
            .last()
            .unwrap()
            .contains("dialogueSideMarginPct")
    );
    assert_eq!(w.engine.calls_of("reconvert").len(), 1);
    assert!(has_value(
        &h,
        "Saved. The book on disk matches what you see."
    ));
}

#[test]
fn settings_out_of_reach_offers_to_convert_again() {
    let w = world();
    w.engine
        .always("settings", Reply::ok(json!({"ok": true, "settings": {}})));
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Tab, "Settings").click();
    steps(&mut h);
    h.get_by_role_and_label(Role::Heading, "Settings out of reach");
    assert!(has_value(
        &h,
        "The engine refused the change without saying why."
    ));
    h.get_by_role_and_label(Role::Button, "Convert it again");
}

// page-show.test.ts: "the page writes itself"
#[test]
fn the_page_writes_itself_unless_motion_is_reduced() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    assert!(h.state().show.is_some(), "the page line started a show");
    w.fakes.clock.advance(10_000.0);
    steps(&mut h);
    w.fakes.clock.advance(300.0);
    steps(&mut h);
    assert!(h.state().show.is_none(), "it ended by itself");

    let still = world();
    still.fakes.reduced_motion.store(true, Ordering::Relaxed);
    let mut h = harness(&still, Platform::Mac);
    convert(&still, &mut h);
    assert!(h.state().show.is_none());
    h.get_by_role_and_label(Role::Heading, "FIELD STATION IS READY");
}

// desktop-ui.test.ts: "the Kindle heads-up, mounted on its own"
#[test]
fn a_kindle_plugged_in_while_reading_gets_a_heads_up() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Tab, "Read").click();
    steps(&mut h);
    kindle_plugged_in(&w);
    w.fakes.clock.advance(2001.0);
    steps(&mut h);
    let line = "Kindle plugged in. Copy Field Station?";
    assert!(h.query_by_role_and_label(Role::Status, line).is_some());
    h.get_by_role_and_label(Role::Button, "Not now").click();
    steps(&mut h);
    assert!(h.query_by_role_and_label(Role::Status, line).is_none());
}

// desktop-ui.test.ts: "where the keyboard stands when a dialog closes"
#[test]
fn after_a_cancelled_dialog_the_keyboard_lands_on_the_first_control() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    w.fakes.answer_dialog(None);
    h.key_press_modifiers(Modifiers::COMMAND, Key::O);
    steps(&mut h);
    let focused = h.get_by(|n| n.is_focused());
    assert_eq!(
        focused.accesskit_node().label().as_deref(),
        Some("Choose PDF…  ⌘O")
    );
    assert!(w.engine.calls_of("convert").is_empty());
}

#[test]
fn every_value_the_tree_carries_is_a_string_not_null() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    let all: Vec<Value> = h
        .query_all(By::new())
        .filter_map(|n| n.accesskit_node().value().map(|v| json!(v)))
        .collect();
    assert!(
        all.iter().all(|v| v != "null"),
        "no screen prints a stray null"
    );
}
