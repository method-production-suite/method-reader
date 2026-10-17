//! Read it | Edit it at the top of the send panel: new behavior in the Rust
//! window, not parity (rust/PARITY.md rows X1 to X6). The choice shows on
//! the cards and on the calm screen, is one Tab stop with the arrows
//! between its halves, leads Edit it to two cards that save the editing
//! files through the same export Save a file uses, is remembered app-wide,
//! and slides its knob over the state motion unless Reduce Motion is on.

mod common;

use std::sync::atomic::Ordering;

use common::*;
use egui::Key;
use egui::accesskit::{Action, Role};
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::json;

use sp_app::engine::{Reply, Request, default_answer};
use sp_app::model::convert::Platform;
use sp_app::model::device::NO_FOUNTAIN;
use sp_app::model::intent::{CHOICE_LABEL, EDIT_LINE};

fn settings_with(w: &World, change: impl FnOnce(&mut serde_json::Value)) {
    let mut a = default_answer(&Request::AppSettings);
    change(&mut a);
    w.engine.always("app-settings", Reply::ok(a));
}

fn result(w: &World) -> H {
    let mut h = harness_sized(w, Platform::Mac, [1280.0, 1600.0]);
    convert(w, &mut h);
    h
}

fn selected(h: &H) -> String {
    h.query_all_by_role(Role::Tab)
        .filter(|t| t.accesskit_node().is_selected() == Some(true))
        .filter_map(|t| t.accesskit_node().label())
        .find(|l| l == "Read it" || l == "Edit it")
        .expect("one half chosen")
}

fn tab_stop(h: &H, name: &str) -> bool {
    h.get_by_role_and_label(Role::Tab, name)
        .accesskit_node()
        .data()
        .supports_action(Action::Focus)
}

#[test]
fn x1_the_choice_heads_the_cards_and_the_calm_screen() {
    let w = world();
    let h = result(&w);
    h.get_by_role_and_label(Role::TabList, CHOICE_LABEL);
    assert_eq!(selected(&h), "Read it");
    h.get_by_role_and_label(Role::Heading, "What do you read on?");
    let choice = h.get_by_role_and_label(Role::TabList, CHOICE_LABEL).rect();
    let cards = h
        .get_by_role_and_label(Role::Heading, "What do you read on?")
        .rect();
    assert!(choice.bottom() <= cards.top(), "at the top");

    let w = world();
    settings_with(&w, |a| a["readerDevice"] = json!("kobo"));
    let h = result(&w);
    h.get_by_role_and_label(Role::TabList, CHOICE_LABEL);
    h.get_by_role_and_label(Role::Button, "Other ways");
}

#[test]
fn x2_one_tab_stop_and_the_arrows_move_between_the_halves() {
    let w = world();
    let mut h = result(&w);
    assert!(tab_stop(&h, "Read it"));
    assert!(!tab_stop(&h, "Edit it"), "only the chosen half is a stop");
    h.get_by_role_and_label(Role::Tab, "Read it").focus();
    steps(&mut h);
    h.key_press(Key::ArrowRight);
    steps(&mut h);
    assert_eq!(selected(&h), "Edit it");
    assert!(
        h.get_by_role_and_label(Role::Tab, "Edit it").is_focused(),
        "the keyboard goes with the choice"
    );
    assert!(has(&h, EDIT_LINE));
    assert!(!has(&h, "What do you read on?"));
    h.key_press(Key::Home);
    steps(&mut h);
    assert_eq!(selected(&h), "Read it");
    h.key_press(Key::End);
    steps(&mut h);
    assert_eq!(selected(&h), "Edit it");
    h.key_press(Key::ArrowLeft);
    steps(&mut h);
    assert_eq!(selected(&h), "Read it");
}

#[test]
fn x3_edit_its_cards_save_through_the_same_export_with_the_types_filter() {
    for (card, kind, filter, out) in [
        (
            "Final Draft",
            "fdx",
            ("FDX", "fdx"),
            "/Users/reader/Desktop/Field Station.fdx",
        ),
        (
            "Highland, Fade In and others",
            "fountain",
            ("Fountain", "fountain"),
            "/Users/reader/Desktop/Field Station.fountain",
        ),
    ] {
        let w = world();
        settings_with(&w, |a| a["panelIntent"] = json!("edit"));
        let mut h = result(&w);
        w.fakes.answer_dialog(Some(out));
        h.get_by_role_and_label(Role::Button, card)
            .click_accesskit();
        steps(&mut h);
        let asked = w.fakes.dialogs.asked.lock().unwrap().clone();
        assert!(
            asked
                .iter()
                .any(|a| a == &format!("save Field Station.{kind}")),
            "{asked:?}"
        );
        assert_eq!(
            w.fakes.dialogs.filters.lock().unwrap().last().unwrap(),
            &(filter.0.to_string(), vec![filter.1.to_string()])
        );
        let export = w.engine.calls_of("export");
        let argv = export.last().unwrap().argv();
        assert!(argv.windows(2).any(|p| p == ["--for", kind]), "{argv:?}");
        assert!(argv.windows(2).any(|p| p == ["--out", out]));
        h.get_by_role_and_label(Role::Status, &format!("Saved Field Station.{kind}."));
    }
}

#[test]
fn x3_final_draft_wears_the_beta_badge_and_says_what_it_is() {
    let w = world();
    settings_with(&w, |a| a["panelIntent"] = json!("edit"));
    let h = result(&w);
    let fdx = h.get_by_role_and_label(Role::Button, "Final Draft");
    assert_eq!(
        fdx.accesskit_node().description().as_deref(),
        Some("FDX, also for most screenwriting apps")
    );
    let rect = fdx.rect();
    assert!(
        h.query_all_by_label("Beta")
            .any(|b| rect.contains_rect(b.rect())),
        "the badge sits on the card"
    );
    let fountain = h
        .get_by_role_and_label(Role::Button, "Highland, Fade In and others")
        .rect();
    assert!(
        !h.query_all_by_label("Beta")
            .any(|b| fountain.contains_rect(b.rect()))
    );
}

#[test]
fn x3_a_book_without_a_fountain_refuses_with_the_save_rows_words() {
    let w = world();
    settings_with(&w, |a| a["panelIntent"] = json!("edit"));
    let mut a = default_answer(&Request::Convert {
        path: String::new(),
        force: false,
        options: None,
    });
    a.as_object_mut().unwrap().remove("fountainPath");
    w.engine.always("convert", Reply::ok(a));
    let mut h = result(&w);
    h.get_by_role_and_label(Role::Button, "Final Draft")
        .click_accesskit();
    steps(&mut h);
    h.get_by_role_and_label(Role::Status, NO_FOUNTAIN);
    assert!(
        !w.fakes
            .dialogs
            .asked
            .lock()
            .unwrap()
            .iter()
            .any(|a| a.starts_with("save")),
        "no Save box"
    );
    assert!(w.engine.calls_of("export").is_empty());
}

#[test]
fn x4_the_choice_is_remembered_app_wide() {
    let w = world();
    let mut h = result(&w);
    h.get_by_role_and_label(Role::Tab, "Edit it").click();
    steps(&mut h);
    let set = w.engine.calls_of("app-settings-set");
    assert_eq!(
        set.last().unwrap().argv().last().unwrap(),
        r#"{"panelIntent":"edit"}"#
    );
    // The next book keeps it, without asking again.
    h.get_by_role_and_label(Role::Button, "Convert another")
        .click_accesskit();
    steps(&mut h);
    convert(&w, &mut h);
    assert_eq!(selected(&h), "Edit it");

    // And a new window reads it back.
    let w = world();
    settings_with(&w, |a| a["panelIntent"] = json!("edit"));
    let h = result(&w);
    assert_eq!(selected(&h), "Edit it");
    assert!(has(&h, EDIT_LINE));
}

#[test]
fn x5_the_knob_slides_over_the_state_motion_unless_motion_is_reduced() {
    let w = world();
    let mut h = result(&w);
    h.get_by_role_and_label(Role::Tab, "Edit it").click();
    steps(&mut h);
    let start = h.state().panel.knob;
    assert!(start < 0.5, "it has only just left: {start}");
    w.fakes.clock.advance(100.0);
    steps(&mut h);
    let mid = h.state().panel.knob;
    assert!(mid > start && mid < 1.0, "on its way: {mid}");
    w.fakes.clock.advance(150.0);
    steps(&mut h);
    assert_eq!(h.state().panel.knob, 1.0);

    let w = world();
    w.fakes.reduced_motion.store(true, Ordering::Relaxed);
    let mut h = result(&w);
    h.get_by_role_and_label(Role::Tab, "Edit it").click();
    steps(&mut h);
    assert_eq!(h.state().panel.knob, 1.0, "at once");
}

#[test]
fn x6_save_a_file_keeps_its_fdx_and_fountain_rows() {
    let w = world();
    settings_with(&w, |a| a["readerDevice"] = json!("kobo"));
    let mut h = result(&w);
    h.get_by_role_and_label(Role::Button, "Other ways")
        .click_accesskit();
    steps(&mut h);
    h.get_by_role_and_label(Role::Button, "Save FDX…");
    h.get_by_role_and_label(Role::Button, "Save Fountain…");
}
