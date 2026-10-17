//! The inventory's "model" rows, at screen level: each rule was ported and
//! tested in src/model; these drive the window to where the rule shows and
//! read it back through the AccessKit tree. Each test names its
//! rust/PARITY.md row.

mod common;

use std::sync::atomic::Ordering;

use common::*;
use egui::accesskit::Role;
use egui::{Key, Modifiers};
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::json;
use sp_app::engine::default_answer;

use sp_app::engine::{Reply, Request};
use sp_app::model::convert::Platform;

/// Some node's label or value holds `words`.
fn shows(h: &H, words: &str) -> bool {
    h.query_all_by_label_contains(words).next().is_some()
        || h.query_all(
            egui_kittest::kittest::By::new()
                .predicate(move |n| n.value().is_some_and(|v| v.contains(words))),
        )
        .next()
        .is_some()
}

#[allow(dead_code)]
fn dump(h: &H) -> Vec<String> {
    h.query_all(egui_kittest::kittest::By::new().predicate(|_| true))
        .filter_map(|n| {
            let a = n.accesskit_node();
            let l = a.label().or_else(|| a.value());
            l.map(|l| format!("{:?}: {l}", a.role()))
        })
        .collect()
}

fn convert_with(w: &World, h: &mut H) {
    w.fakes.answer_dialog(Some("/scripts/field-station.pdf"));
    h.key_press_modifiers(Modifiers::COMMAND, Key::O);
}

// ------------------------------------------------------------------ convert

#[test]
fn c12_the_read_out_names_the_stage_while_the_call_is_out() {
    let w = world();
    w.fakes.reduced_motion.store(true, Ordering::Relaxed);
    w.engine.set_progress(vec![
        json!({"stage": "parse", "percent": 30, "page": 6, "pages": 18}),
    ]);
    w.engine.hold("convert");
    let mut h = harness_threads(&w, Platform::Mac);
    convert_with(&w, &mut h);
    settle(&mut h);
    assert!(shows(&h, "reading the pages (30%)"));
    w.engine.release("convert");
    settle(&mut h);
    h.get_by_role_and_label(Role::Heading, "Field Station is ready");
}

#[test]
fn c12_a_book_held_by_another_turn_says_what_it_waits_for() {
    let w = world();
    w.fakes.reduced_motion.store(true, Ordering::Relaxed);
    let mut h = harness_threads(&w, Platform::Mac);
    convert_with(&w, &mut h);
    settle(&mut h);
    // A send holds the book (its turn is out).
    let book = h
        .state()
        .convert
        .landed
        .get("/scripts/field-station.pdf")
        .cloned()
        .expect("the book this window converted");
    w.engine.hold("kindle-check");
    h.state_mut().on_book(
        &book,
        "kindle",
        Request::KindleCheck { epub: book.clone() },
        sp_app::app::Purpose::Version,
    );
    convert_with(&w, &mut h);
    settle(&mut h);
    assert!(shows(&h, "Waiting for the Kindle file to finish building…"));
    w.engine.release("kindle-check");
    settle(&mut h);
}

#[test]
fn c26_a_refusal_leaves_the_open_book_open_and_says_so() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    w.engine.once(
        "convert",
        Reply::ok(json!({"ok": false, "error": {"code": "scanned", "message": "A scan."}})),
    );
    convert(&w, &mut h);
    assert!(shows(
        &h,
        "Field Station is still open. This changed nothing about it."
    ));
    assert!(h.state().script.is_some(), "the open book survives");
}

#[test]
fn c7_a_refused_change_says_why_and_leaves_the_line() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    w.fakes.answer_dialog(Some("/Volumes/Gone/Books"));
    w.engine.once(
        "app-settings-set",
        Reply::ok(json!({"ok": false, "error": {"code": "unwritable", "message": "That folder cannot be written."}})),
    );
    h.get_by_role_and_label(Role::Button, "Change…").click();
    steps(&mut h);
    assert!(shows(&h, "That folder cannot be written."));
    assert!(!shows(&h, sp_app::model::convert::MOVED_LINE));
    assert!(shows(&h, "Books are saved in"));

    // An answer that breaks the contract.
    w.fakes.answer_dialog(Some("/Users/reader/Books"));
    w.engine
        .once("app-settings-set", Reply::ok(json!({"ok": true})));
    h.get_by_role_and_label(Role::Button, "Change…").click();
    steps(&mut h);
    assert!(shows(&h, sp_app::model::convert::NO_LIBRARY_MESSAGE));
}

// --------------------------------------------------------------- page show

#[test]
fn p3_the_slim_line_reads_the_page_reached() {
    let w = world();
    w.engine.set_progress(vec![
        json!({"stage": "page", "page": 1, "paragraphs": ["EXT. RIDGE ROAD - DAY", "Thirty-one."]}),
        json!({"stage": "parse", "percent": 30, "page": 6, "pages": 18}),
    ]);
    w.engine.hold("convert");
    let mut h = harness_threads(&w, Platform::Mac);
    convert_with(&w, &mut h);
    settle(&mut h);
    assert!(h.state().show.is_some(), "the page line started a show");
    h.get_by_role_and_label(Role::Status, "Reading page 6 of 18");
    w.engine.release("convert");
    settle(&mut h);
}

#[test]
fn p8_typed_paragraphs_pair_with_the_books_own_blocks() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    let show = h.state().show.as_ref().expect("the show runs on");
    let sources = show.sources().expect("the book arrived").to_vec();
    let words = show.words();
    assert_eq!(sources.len(), words.len());
    // Every paragraph of the fake's first page is in the demo book.
    assert!(
        sources.iter().all(Option::is_some),
        "{words:?} -> {sources:?}"
    );
    assert!(
        sources.windows(2).all(|p| p[0] < p[1]),
        "in order: {sources:?}"
    );
}

// --------------------------------------------------------------------- read

fn read(w: &World) -> H {
    let mut h = harness(w, Platform::Mac);
    convert(w, &mut h);
    h.get_by_role_and_label(Role::Tab, "Read").click();
    steps(&mut h);
    h
}

fn offset(h: &H) -> f32 {
    h.state().read.offset
}

#[test]
fn r6_the_stages_keys_scroll_by_their_steps() {
    let w = world();
    let mut h = read(&w);
    h.get_by_role_and_label(Role::Group, "Field Station, the script")
        .focus();
    steps(&mut h);
    let frame = h.state().read.frame_height;
    h.key_press(Key::ArrowDown);
    steps(&mut h);
    assert!((offset(&h) - 60.0).abs() < 1.0, "Down: {}", offset(&h));
    h.key_press(Key::PageDown);
    steps(&mut h);
    let page = (frame * 0.9).max(120.0);
    assert!(
        (offset(&h) - (60.0 + page)).abs() < 1.0,
        "PageDown: {} (frame {frame})",
        offset(&h)
    );
    h.key_press(Key::ArrowUp);
    steps(&mut h);
    assert!((offset(&h) - page).abs() < 1.0, "Up: {}", offset(&h));
    h.key_press(Key::End);
    steps(&mut h);
    let end = h.state().read.content_height - frame;
    assert!(
        (offset(&h) - end).abs() < 2.0,
        "End: {} of {end}",
        offset(&h)
    );
    h.key_press(Key::Home);
    steps(&mut h);
    assert_eq!(offset(&h), 0.0, "Home");
}

#[test]
fn r8_the_index_is_one_tab_stop_whose_arrows_stop_at_the_ends() {
    let w = world();
    let mut h = read(&w);
    let nav = h.get_by_role_and_label(Role::Navigation, "Scenes").rect();
    let in_index: Vec<_> = h
        .query_all_by_role(Role::Button)
        .filter(|n| nav.contains_rect(n.rect()))
        .collect();
    let entries: Vec<String> = in_index
        .iter()
        .filter_map(|n| n.accesskit_node().label())
        .collect();
    assert!(entries.len() > 3, "{entries:?}");
    let focusable = in_index
        .iter()
        .filter(|n| {
            n.accesskit_node()
                .data()
                .supports_action(egui::accesskit::Action::Focus)
        })
        .count();
    assert_eq!(focusable, 1, "one Tab stop for the whole index");
    h.get_by_role_and_label(Role::Button, &entries[0]).focus();
    steps(&mut h);
    h.key_press(Key::ArrowUp);
    steps(&mut h);
    assert_eq!(h.state().read.rail_stop, 0, "stops at the top");
    h.key_press(Key::ArrowDown);
    steps(&mut h);
    assert_eq!(h.state().read.rail_stop, 1);
    h.key_press(Key::End);
    steps(&mut h);
    assert_eq!(h.state().read.rail_stop, entries.len() - 1);
    h.key_press(Key::ArrowDown);
    steps(&mut h);
    assert_eq!(
        h.state().read.rail_stop,
        entries.len() - 1,
        "and at the end"
    );
    h.key_press(Key::Home);
    steps(&mut h);
    assert_eq!(h.state().read.rail_stop, 0);
}

#[test]
fn r12_a_script_with_no_headings_says_so() {
    let w = world();
    let mut a = default_answer(&Request::Convert {
        path: String::new(),
        force: false,
        options: None,
    });
    a["previewHtml"] = json!(
        "<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>T</title></head><body><section class=\"scene\"><p class=\"action\">Only words.</p></section></body></html>"
    );
    w.engine.always("convert", Reply::ok(a));
    let h = read(&w);
    assert!(shows(&h, sp_app::model::read::NO_SCENES));
}

#[test]
fn r14_the_place_survives_a_resize_and_a_scene_that_is_gone_goes_to_the_top() {
    let w = world();
    let mut h = read(&w);
    h.get_by_role_and_label(Role::Button, "EXT. RIDGE – NIGHT")
        .click();
    steps(&mut h);
    let marked = h.state().read.marked.clone();
    assert!(marked.is_some());
    h.set_size(egui::vec2(1000.0, 700.0));
    steps(&mut h);
    steps(&mut h);
    assert_eq!(h.state().read.marked, marked, "same scene after a resize");

    // A Settings save whose book has no such scene any more.
    let mut gone = default_answer(&Request::Reconvert {
        fountain: String::new(),
        epub: String::new(),
        options: String::new(),
    });
    gone["previewHtml"] = json!(
        "<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>T</title></head><body><section class=\"scene\" id=\"scene-1\"><h2 class=\"scene-heading\">INT. ONLY ROOM - DAY</h2><p class=\"action\">Words.</p></section></body></html>"
    );
    w.engine.always("reconvert", Reply::ok(gone));
    h.get_by_role_and_label(Role::Tab, "Settings").click();
    steps(&mut h);
    h.get_by_role_and_label(Role::Button, "Phone")
        .click_accesskit();
    steps(&mut h);
    w.fakes.clock.advance(301.0);
    steps(&mut h);
    steps(&mut h);
    h.get_by_role_and_label(Role::Tab, "Read").click();
    steps(&mut h);
    steps(&mut h);
    assert_eq!(offset(&h), 0.0, "at the top");
}

// ----------------------------------------------------------------- settings

fn settings(w: &World) -> H {
    let mut h = harness(w, Platform::Mac);
    convert(w, &mut h);
    h.get_by_role_and_label(Role::Tab, "Settings").click();
    steps(&mut h);
    h
}

fn save_now(w: &World, h: &mut H) {
    steps(h);
    w.fakes.clock.advance(301.0);
    steps(h);
    steps(h);
}

#[test]
fn s13_a_failed_save_owes_its_changes_again() {
    let w = world();
    let mut h = settings(&w);
    w.engine.once(
        "settings-set",
        Reply::ok(json!({"ok": false, "error": {"code": "unwritable", "message": "The library is read-only."}})),
    );
    h.get_by_role_and_label(Role::Button, "Phone")
        .click_accesskit();
    save_now(&w, &mut h);
    // tune.js says its own sentence for a save the engine refused.
    assert!(shows(&h, sp_app::model::tune::NO_MESSAGE));
    let first = w.engine.calls_of("settings-set");
    assert_eq!(first.len(), 1);
    // The next change carries the failed one with it.
    h.get_by_role_and_label(Role::CheckBox, "Start each scene on a new page")
        .click_accesskit();
    save_now(&w, &mut h);
    let calls = w.engine.calls_of("settings-set");
    assert_eq!(calls.len(), 2);
    let was: serde_json::Value = serde_json::from_str(first[0].argv().last().unwrap()).unwrap();
    let now: serde_json::Value = serde_json::from_str(calls[1].argv().last().unwrap()).unwrap();
    for (key, value) in was.as_object().unwrap() {
        if key != "sceneBreaks" && key != "pageBreakBeforeScene" {
            assert_eq!(&now[key], value, "{key} is owed again");
        }
    }
    assert!(now.as_object().unwrap().len() > was.as_object().unwrap().len());
}

#[test]
fn s15_a_rebuilds_warnings_sit_under_the_status_until_the_next_good_one() {
    let w = world();
    let mut warned = default_answer(&Request::Reconvert {
        fountain: String::new(),
        epub: String::new(),
        options: String::new(),
    });
    warned["warnings"] = json!(["Two pictures were too small to keep."]);
    w.engine.once("reconvert", Reply::ok(warned));
    let mut h = settings(&w);
    h.get_by_role_and_label(Role::Button, "Phone")
        .click_accesskit();
    save_now(&w, &mut h);
    h.get_by_role_and_label(Role::Status, "(Two pictures were too small to keep.)");
    h.get_by_role_and_label(Role::CheckBox, "Start each scene on a new page")
        .click_accesskit();
    save_now(&w, &mut h);
    assert!(!shows(&h, "Two pictures were too small"), "replaced");
}

// --------------------------------------------------------------- send panel

fn calm_as(w: &World, kind: &str) {
    let mut a = default_answer(&Request::AppSettings);
    a["readerDevice"] = json!(kind);
    w.engine.always("app-settings", Reply::ok(a));
}

fn routes_with(w: &World, change: impl FnOnce(&mut Vec<serde_json::Value>)) {
    let mut routes = default_answer(&Request::Routes {
        epub: String::new(),
        quick: false,
    });
    change(routes["routes"].as_array_mut().unwrap());
    w.engine.always("routes", Reply::ok(routes));
}

fn plug_in(w: &World, kind: &str, name: &str) {
    let key = format!("device:{kind}");
    let device =
        json!({"id": "d1", "kind": kind, "name": name, "volume": format!("/Volumes/{kind}")});
    let d2 = device.clone();
    routes_with(w, |list| {
        let row = list.iter_mut().find(|r| r["id"] == key).unwrap();
        row["available"] = json!(true);
        row["detail"] = json!("copies over USB");
        row["device"] = d2;
        row.as_object_mut().unwrap().remove("unavailable");
    });
    let mut routes = default_answer(&Request::Routes {
        epub: String::new(),
        quick: false,
    });
    let _ = &mut routes;
    w.engine.always(
        "devices",
        Reply::ok(json!({"ok": true, "devices": [device]})),
    );
}

fn panel(w: &World, platform: Platform) -> H {
    let mut h = harness_sized(w, platform, [1280.0, 3000.0]);
    convert(w, &mut h);
    h
}

fn open_other_ways(h: &mut H) {
    h.get_by_role_and_label(Role::Button, "Other ways")
        .click_accesskit();
    steps(h);
}

#[test]
fn d2_no_book_on_disk_says_so_and_offers_to_convert_again() {
    let w = world();
    let mut a = default_answer(&Request::Convert {
        path: String::new(),
        force: false,
        options: None,
    });
    a.as_object_mut().unwrap().remove("epubPath");
    w.engine.always("convert", Reply::ok(a));
    let h = panel(&w, Platform::Mac);
    h.get_by_role_and_label(Role::Heading, "No book to send");
    assert!(shows(
        &h,
        "Method Reader has no book on disk for this script"
    ));
    h.get_by_role_and_label(Role::Button, "Convert it again");
}

#[test]
fn d7_the_asterisk_names_what_was_never_tried_and_asks_how_it_went() {
    let w = world();
    calm_as(&w, "kobo");
    let mut h = panel(&w, Platform::Mac);
    open_other_ways(&mut h);
    // The calm screen's asterisk and Other ways' fold each wear one.
    let both: Vec<_> = h
        .query_all_by_role_and_label(Role::Button, "About Kobo: never tried on real hardware")
        .collect();
    assert_eq!(both.len(), 2);
    both[0].click_accesskit();
    steps(&mut h);
    assert!(shows(
        &h,
        "Built and tested in code, never tried on a real Kobo yet. If you try it, tell us how it went."
    ));
    // The note opens under each asterisk of that kind.
    assert!(
        h.query_all_by_role_and_label(Role::Button, "It worked")
            .count()
            > 0
    );
    assert!(
        h.query_all_by_role_and_label(Role::Button, "It did not work")
            .next()
            .is_some()
    );

    let w = world();
    calm_as(&w, "kindle");
    plug_in(&w, "kindle", "Kindle");
    let mut h = panel(&w, Platform::Windows);
    h.get_by_role_and_label(Role::Button, "About Kindle: never tried on real hardware")
        .click_accesskit();
    steps(&mut h);
    assert!(shows(
        &h,
        "Built and tested in code, never tried with a real Kindle on Windows yet."
    ));
}

#[test]
fn d10_a_kindle_copies_when_plugged_and_goes_by_the_web_when_not() {
    let w = world();
    calm_as(&w, "kindle");
    let h = panel(&w, Platform::Mac);
    h.get_by_role_and_label(Role::Button, "Send to Kindle");
    assert!(shows(&h, sp_app::model::device::PLUG_KINDLE));

    let w = world();
    calm_as(&w, "kindle");
    plug_in(&w, "kindle", "Kindle");
    let mut h = panel(&w, Platform::Mac);
    h.get_by_role_and_label(Role::Button, "Copy to Kindle (.azw3)");
    open_other_ways(&mut h);
    let send_to = h
        .get_by_role_and_label(Role::Button, "Send to Kindle")
        .rect();
    let email = h.get_by_role_and_label(Role::Button, "Email it").rect();
    let books = h
        .get_by_role_and_label(Role::Button, "Open in Books")
        .rect();
    assert!(
        send_to.top() < books.top() && email.top() < books.top(),
        "web and email lead Other ways"
    );
}

#[test]
fn d11_apple_is_books_on_a_mac_and_a_saved_epub_elsewhere() {
    let w = world();
    calm_as(&w, "apple");
    let h = panel(&w, Platform::Mac);
    h.get_by_role_and_label(Role::Button, "Open in Books");

    let w = world();
    calm_as(&w, "apple");
    let mut routes = default_answer(&Request::Routes {
        epub: String::new(),
        quick: false,
    });
    routes["routes"]
        .as_array_mut()
        .unwrap()
        .retain(|r| r["id"] != "apple-books");
    w.engine.always("routes", Reply::ok(routes));
    let h = panel(&w, Platform::Windows);
    h.get_by_role_and_label(Role::Button, "Save the EPUB…");
    assert!(shows(&h, sp_app::model::device::APPLE_ELSEWHERE));
}

#[test]
fn d12_while_the_list_is_out_it_says_it_is_looking() {
    let w = world();
    calm_as(&w, "kobo");
    w.engine.hold("routes");
    let mut h = harness_threads(&w, Platform::Mac);
    convert_with(&w, &mut h);
    settle(&mut h);
    settle(&mut h);
    assert!(shows(&h, sp_app::model::device::LOOKING));
    w.engine.release("routes");
    settle(&mut h);
    assert!(!shows(&h, sp_app::model::device::LOOKING));
}

#[test]
fn d14_a_list_that_cannot_be_read_says_the_engines_sentence_and_no_brass() {
    let w = world();
    calm_as(&w, "kindle");
    w.engine.always(
        "routes",
        Reply::ok(
            json!({"ok": false, "error": {"code": "internal", "message": "The route list broke."}}),
        ),
    );
    let h = panel(&w, Platform::Mac);
    assert!(shows(&h, "The route list broke."));
    assert!(
        h.query_by_role_and_label(Role::Button, sp_app::model::device::WEB_BUTTON)
            .is_none()
    );
}

#[test]
fn d18_readers_that_only_need_plugging_in_fold_into_one_line() {
    let w = world();
    calm_as(&w, "apple");
    let mut h = panel(&w, Platform::Mac);
    open_other_ways(&mut h);
    assert!(shows(&h, "plug in or dock over USB to send."));
    for name in ["Kobo", "tolino", "reMarkable"] {
        h.get_by_role_and_label(
            Role::Button,
            &format!("About {name}: never tried on real hardware"),
        );
    }
    assert!(
        h.query_by_role_and_label(Role::Button, "Copy to Kobo")
            .is_none(),
        "folded, not rows"
    );
}

#[test]
fn d30_a_stale_calibre_kfx_is_built_ahead_once_with_its_line() {
    let w = world();
    calm_as(&w, "kindle");
    plug_in(&w, "kindle", "Kindle");
    w.engine.always(
        "kindle-check",
        Reply::ok(json!({"ok": true, "extension": "kfx", "fresh": false, "builtBy": "calibre", "bookDate": 7})),
    );
    w.engine.hold("export");
    let mut h = harness_threads(&w, Platform::Mac);
    convert_with(&w, &mut h);
    settle(&mut h);
    settle(&mut h);
    assert_eq!(w.engine.calls_of("export").len(), 1, "built ahead");
    assert!(shows(
        &h,
        "Building the Kindle file. This takes about half a minute."
    ));
    w.engine.release("export");
    settle(&mut h);
    settle(&mut h);
    assert_eq!(
        w.engine.calls_of("export").len(),
        1,
        "once per book version and kind"
    );
}

// --------------------------------------------------------------------- KFX

fn kfx_missing_plugin() -> serde_json::Value {
    json!({"ok": true, "ready": false, "possible": true,
    "summary": "Kindles get AZW3 today. Add the KFX plugin for the best layout.",
    "steps": [
        {"id": "calibre", "name": "Calibre", "installed": true, "fix": null},
        {"id": "previewer", "name": "Kindle Previewer", "installed": true, "fix": null},
        {"id": "plugin", "name": "KFX plugin", "installed": false,
         "fix": {"kind": "install", "label": "Install"}}
    ]})
}

#[test]
fn k1_k2_set_it_up_shows_only_where_kfx_is_possible_and_not_ready() {
    let w = world();
    calm_as(&w, "kindle");
    let mut h = panel(&w, Platform::Mac);
    open_other_ways(&mut h);
    h.get_by_role_and_label(Role::Button, "Set it up")
        .click_accesskit();
    steps(&mut h);
    h.get_by_role_and_label(Role::Region, "Best Kindle quality");
    for step in ["Calibre", "Kindle Previewer", "KFX plugin"] {
        assert!(shows(&h, step), "{step}");
    }

    for status in [
        json!({"ok": true, "ready": true, "possible": true, "summary": "", "steps": []}),
        json!({"ok": true, "ready": false, "possible": false, "summary": "", "steps": []}),
    ] {
        let w = world();
        calm_as(&w, "kindle");
        w.engine.always("kfx-status", Reply::ok(status));
        let mut h = panel(&w, Platform::Mac);
        open_other_ways(&mut h);
        if let Some(b) = h.query_by_role_and_label(Role::Button, "Set it up") {
            b.click_accesskit();
            steps(&mut h);
        }
        assert!(
            h.query_by_role_and_label(Role::Region, "Best Kindle quality")
                .is_none(),
            "no checklist where KFX is ready or impossible"
        );
    }

    // A checklist that is not whole draws nothing.
    let w = world();
    calm_as(&w, "kindle");
    let mut half = kfx_missing_plugin();
    half["steps"].as_array_mut().unwrap().pop();
    w.engine.always("kfx-status", Reply::ok(half));
    let mut h = panel(&w, Platform::Mac);
    open_other_ways(&mut h);
    if let Some(b) = h.query_by_role_and_label(Role::Button, "Set it up") {
        b.click_accesskit();
        steps(&mut h);
    }
    assert!(
        h.query_by_role_and_label(Role::Region, "Best Kindle quality")
            .is_none()
    );
}

#[test]
fn k4_install_says_it_is_working_then_what_it_installed() {
    let w = world();
    calm_as(&w, "kindle");
    w.engine
        .always("kfx-status", Reply::ok(kfx_missing_plugin()));
    let mut ready = kfx_missing_plugin();
    ready["ready"] = json!(true);
    ready["steps"][2]["installed"] = json!(true);
    ready["steps"][2]["fix"] = serde_json::Value::Null;
    w.engine.once(
        "kfx-install",
        Reply::ok(json!({"ok": true, "version": "2.17.1", "removed": ["KFX Output 2.10"], "setup": ready})),
    );
    w.engine.hold("kfx-install");
    let mut h = harness_threads(&w, Platform::Mac);
    h.set_size(egui::vec2(1280.0, 3000.0));
    convert_with(&w, &mut h);
    settle(&mut h);
    open_other_ways(&mut h);
    h.get_by_role_and_label(Role::Button, "Set it up")
        .click_accesskit();
    settle(&mut h);
    h.get_by_role_and_label(Role::Button, "Install")
        .click_accesskit();
    settle(&mut h);
    assert!(shows(&h, sp_app::model::kfx::INSTALLING));
    assert!(
        h.get_by_role_and_label(Role::Button, "Other ways")
            .accesskit_node()
            .is_disabled()
            || h.query_all_by_role(Role::Button)
                .filter(|b| b
                    .accesskit_node()
                    .label()
                    .is_some_and(|l| l.starts_with("Save")))
                .all(|b| b.accesskit_node().is_disabled()),
        "every other panel button dead while it runs"
    );
    w.engine.release("kfx-install");
    settle(&mut h);
    assert!(shows(
        &h,
        "Installed the KFX plugin 2.17.1. Kindles now get KFX. Removed an older copy: KFX Output 2.10."
    ));
}

// ---------------------------------------------------------------- devices

#[test]
fn h2_a_reader_with_no_name_is_named_for_its_kind() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    h.get_by_role_and_label(Role::Tab, "Read").click();
    steps(&mut h);
    plug_in(&w, "kindle", "");
    w.fakes.clock.advance(2001.0);
    steps(&mut h);
    assert_eq!(
        h.state().heads_up.offered.as_ref().map(|d| d.name.as_str()),
        Some("Kindle"),
        "the poll's nameless reader is named for its kind"
    );
    h.get_by_role_and_label(Role::Button, "Copy it").click();
    steps(&mut h);
    steps(&mut h);
    assert!(shows(&h, "Field Station is on your Kindle."));
}

// ---------------------------------------------------------------- updates

fn offer(h: &mut H, version: &str) {
    h.state_mut()
        .update
        .flow
        .offer_found(sp_app::model::update_flow::Offer {
            version: version.into(),
            body: String::new(),
            live: true,
        });
    steps(h);
}

#[test]
fn n11_the_label_is_hidden_until_there_are_words_and_inert_moments_are_disabled() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    assert!(h.query_by_label_contains("Update to").is_none());
    offer(&mut h, "0.12.0");
    let label = h.get_by_role_and_label(Role::Button, "Update to 0.12.0");
    assert!(!label.accesskit_node().is_disabled());
    h.state_mut()
        .update
        .flow
        .during(sp_app::model::update::Phase::Installing {
            version: "0.12.0".into(),
        });
    steps(&mut h);
    assert!(
        h.get_by_role_and_label(Role::Button, "Installing…")
            .accesskit_node()
            .is_disabled()
    );
}

#[test]
fn u3_u5_a_check_is_stamped_before_it_asks_and_a_version_found_is_remembered() {
    use sp_app::app::host::FakeUpdater;
    use sp_app::model::update::{Found, remembered_offer};
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    let updater = FakeUpdater {
        found: Ok(Some(Found {
            version: "0.12.0".into(),
            current_version: "0.11.1".into(),
            body: None,
        })),
        ..FakeUpdater::default()
    };
    let prepared = updater.prepared.clone();
    h.state_mut().host.updater = std::sync::Arc::new(updater);
    w.fakes.clock.advance(5_000.0);
    // The switch off: a quiet check never asks.
    {
        let app = h.state_mut();
        sp_app::model::update::set_automatic(app.host.storage.as_mut(), false);
        app.update.check(&mut app.host, false);
    }
    steps(&mut h);
    assert!(
        sp_app::model::update::read_state(h.state().host.storage.as_ref())
            .last_checked
            .is_none(),
        "no request, so no stamp"
    );
    assert!(h.query_by_label_contains("Update to").is_none());
    // On again: the switch is the consent, and the check runs now, quietly,
    // so the update is downloaded before anything is said.
    h.get_by_role_and_label(Role::Button, concat!("rev ", env!("CARGO_PKG_VERSION")))
        .click();
    steps(&mut h);
    h.get_by_role_and_label(Role::CheckBox, sp_app::model::update::AUTO_LABEL)
        .click_accesskit();
    steps(&mut h);
    h.get_by_role_and_label(Role::Button, sp_app::model::update::READY_LABEL);
    assert_eq!(prepared.load(std::sync::atomic::Ordering::SeqCst), 1);
    let app = h.state();
    assert_eq!(
        remembered_offer(app.host.storage.as_ref(), "0.11.1").as_deref(),
        Some("0.12.0")
    );
    assert_eq!(
        sp_app::model::update::read_state(app.host.storage.as_ref()).last_checked,
        Some(5_000.0),
        "stamped with the clock of the request"
    );
}

#[test]
fn u4_a_failed_check_is_a_message_and_an_intel_mac_is_told_why() {
    use sp_app::app::host::FakeUpdater;
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    h.state_mut().host.updater = std::sync::Arc::new(FakeUpdater {
        found: Err("The network is down.".into()),
        ..FakeUpdater::default()
    });
    h.get_by_role_and_label(Role::Button, concat!("rev ", env!("CARGO_PKG_VERSION")))
        .click();
    steps(&mut h);
    h.get_by_role_and_label(Role::Button, "Check for updates")
        .click_accesskit();
    steps(&mut h);
    assert!(shows(&h, "The network is down."));
    assert!(!shows(&h, "is the newest there is"));
    h.state_mut().host.updater = std::sync::Arc::new(FakeUpdater {
        found: Err("None of the fallback platforms `[\"darwin-x86_64\"]` were found in the response `platforms` object".into()),
        ..FakeUpdater::default()
    });
    h.get_by_role_and_label(Role::Button, "Check for updates")
        .click_accesskit();
    steps(&mut h);
    assert!(shows(&h, sp_app::model::update::INTEL_MAC_RETIRED));
}

#[test]
fn u6_each_moment_has_its_words() {
    use sp_app::model::update::Phase;
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    offer(&mut h, "0.8.0");
    let v = || "0.8.0".to_string();
    for (phase, words) in [
        (
            Phase::Downloading {
                version: v(),
                received: 40.0,
                total: Some(100.0),
                body: String::new(),
            },
            "Downloading 0.8.0… 40%",
        ),
        (Phase::Installing { version: v() }, "Installing…"),
        (
            Phase::Waiting { version: v() },
            "Restarting after this finishes…",
        ),
        (Phase::Stalled { version: v() }, "Restart now"),
        (Phase::Restarting { version: v() }, "Restarting…"),
        (
            Phase::Failed {
                version: Some(v()),
                message: "no".into(),
            },
            "Update failed. Try again",
        ),
        (
            Phase::Installed { version: v() },
            "Update installed. Quit and reopen Method Reader to use 0.8.0.",
        ),
    ] {
        h.state_mut().update.flow.during(phase);
        steps(&mut h);
        h.get_by_role_and_label(Role::Button, words);
    }
}
