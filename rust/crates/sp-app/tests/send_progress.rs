//! Getting a script out of the window, seen from the person waiting: a
//! save or a copy says what it is doing and keeps moving while the engine
//! works, and says what came of it once it answers, with the file a save
//! wrote one press away.

mod common;

use common::*;
use egui::accesskit::Role;
use egui_kittest::kittest::Queryable;
use serde_json::json;

use sp_app::engine::{Reply, Request, default_answer};
use sp_app::model::convert::Platform;
use sp_app::model::motion::Wake;

fn calm_as(w: &World, kind: &str) {
    let mut a = default_answer(&Request::AppSettings);
    a["readerDevice"] = json!(kind);
    w.engine.always("app-settings", Reply::ok(a));
}

const SAVED_TO: &str = "/Users/reader/Desktop/Field Station.fdx";

#[test]
fn a_save_moves_while_it_runs_then_says_saved_and_shows_where() {
    let w = world();
    calm_as(&w, "kobo");
    let mut h = harness_threads_sized(&w, Platform::Mac, [1280.0, 3000.0]);
    w.fakes.answer_dialog(Some("/scripts/field-station.pdf"));
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::O);
    settle(&mut h);
    w.engine.hold("export");
    w.fakes.answer_dialog(Some(SAVED_TO));
    h.get_by_role_and_label(Role::Button, "Other ways")
        .click_accesskit();
    settle(&mut h);
    h.get_by_role_and_label(Role::Button, "Save FDX…")
        .click_accesskit();
    settle(&mut h);

    let button = h.get_by_role_and_label(Role::Button, "Save FDX…").rect();
    h.get_by_role_and_label(Role::Status, "Saving…");
    let line = h.get_by_role_and_label(Role::Label, "Saving…").rect();
    assert!(
        line.top() > button.bottom() && line.top() - button.bottom() < 60.0,
        "said under the row that started it: {button:?} then {line:?}"
    );
    assert!(h.state().panel.status.working);
    assert_eq!(h.state().wake, Wake::Next, "its bar keeps moving");
    assert!(
        h.query_by_label("Show Field Station.fdx in Finder")
            .is_none(),
        "nothing to show until it is written"
    );

    w.engine.release("export");
    settle(&mut h);
    h.get_by_role_and_label(Role::Status, "Saved Field Station.fdx.");
    assert!(h.state().panel.status.done());
    h.get_by_role_and_label(Role::Button, "Show Field Station.fdx in Finder")
        .click_accesskit();
    settle(&mut h);
    assert_eq!(
        w.engine.calls_of("reveal").last(),
        Some(&Request::Reveal {
            path: SAVED_TO.into()
        })
    );
}

#[test]
fn a_reveal_that_fails_says_why_and_takes_the_button_away() {
    let w = world();
    calm_as(&w, "kobo");
    let mut h = harness_sized(&w, Platform::Mac, [1280.0, 3000.0]);
    convert(&w, &mut h);
    w.fakes.answer_dialog(Some(SAVED_TO));
    h.get_by_role_and_label(Role::Button, "Other ways")
        .click_accesskit();
    steps(&mut h);
    h.get_by_role_and_label(Role::Button, "Save FDX…")
        .click_accesskit();
    steps(&mut h);
    w.engine.once(
        "reveal",
        Reply::ok(json!({"ok": false, "error": {"message": "That file has gone."}})),
    );
    h.get_by_role_and_label(Role::Button, "Show Field Station.fdx in Finder")
        .click_accesskit();
    steps(&mut h);
    h.get_by_role_and_label(Role::Status, "That file has gone.");
    assert!(
        h.query_by_label("Show Field Station.fdx in Finder")
            .is_none()
    );
}

#[test]
fn a_cancelled_save_leaves_nothing_moving_and_nothing_to_show() {
    let w = world();
    calm_as(&w, "kobo");
    let mut h = harness_sized(&w, Platform::Mac, [1280.0, 3000.0]);
    convert(&w, &mut h);
    w.fakes.answer_dialog(None);
    h.get_by_role_and_label(Role::Button, "Other ways")
        .click_accesskit();
    steps(&mut h);
    h.get_by_role_and_label(Role::Button, "Save FDX…")
        .click_accesskit();
    steps(&mut h);
    assert!(!h.state().panel.status.working);
    assert!(
        h.query_by_label("Show Field Station.fdx in Finder")
            .is_none()
    );
}

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
    w.engine.always(
        "devices",
        Reply::ok(
            json!({"ok": true, "devices": [{"id": "k1", "kind": "kindle", "name": "Kindle", "volume": "/Volumes/Kindle"}]}),
        ),
    );
}

#[test]
fn a_copy_to_a_reader_moves_while_it_copies_then_says_it_is_there() {
    let w = world();
    calm_as(&w, "kindle");
    plug_in_a_kindle(&w);
    let mut h = harness_threads(&w, Platform::Mac);
    w.fakes.answer_dialog(Some("/scripts/field-station.pdf"));
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::O);
    settle(&mut h);
    w.engine.hold("send");
    h.get_by_role_and_label(Role::Button, "Copy to Kindle (.azw3)")
        .click_accesskit();
    settle(&mut h);
    h.get_by_role_and_label(Role::Status, "Copying it to Kindle…");
    assert!(h.state().panel.status.working);
    assert_eq!(h.state().wake, Wake::Next, "its bar keeps moving");

    w.engine.release("send");
    settle(&mut h);
    h.get_by_role_and_label(
        Role::Status,
        "Field Station is on your Kindle. Eject it before you unplug.",
    );
    assert!(
        h.state().panel.status.done(),
        "a finished copy wears the check"
    );
}
