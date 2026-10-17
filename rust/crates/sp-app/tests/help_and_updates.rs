//! The suite's update behaviour and Help menu, from the window
//! (SUITE-STANDARDS.md, "Updates" and "The Help menu"): quiet checks two
//! minutes after launch and every six hours while the switch is on, an
//! update downloaded before "Update ready" is said, a previous no kept, What's
//! New once after an update, and every Help item doing what it says.

mod common;

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use common::*;
use egui::accesskit::{Role, Toggled};
use egui_kittest::kittest::{NodeT, Queryable};

use sp_app::app::App;
use sp_app::app::help::{self, Sheet};
use sp_app::app::host::FakeUpdater;
use sp_app::model::convert::Platform;
use sp_app::model::help::{HelpItem, NOTICES, RELEASES, SUITE, USER_GUIDE, WEBSITE};
use sp_app::model::update::{
    self as u, CHECK_EVERY_MS, FIRST_CHECK_AFTER_MS, Found, READY_LABEL, read_state,
};
use sp_app::runtime::Mode;

fn newer() -> Result<Option<Found>, String> {
    Ok(Some(Found {
        version: "0.99.0".into(),
        current_version: env!("CARGO_PKG_VERSION").into(),
        body: Some("Faster pages.".into()),
    }))
}

/// A Mac window whose updater finds 0.99.0, its schedule armed as the
/// shipped window arms it; and the count of downloads ahead of the click.
fn scheduled(w: &World) -> (H, Arc<std::sync::atomic::AtomicUsize>) {
    let mut h = harness(w, Platform::Mac);
    let updater = FakeUpdater {
        found: newer(),
        ..FakeUpdater::default()
    };
    let prepared = updater.prepared.clone();
    let app = h.state_mut();
    app.host.updater = Arc::new(updater);
    let now = app.now();
    app.update.start_schedule(now);
    (h, prepared)
}

fn stamp(h: &H) -> Option<f64> {
    read_state(h.state().host.storage.as_ref()).last_checked
}

fn until(h: &mut H, what: &str, done: impl Fn(&H) -> bool) {
    let start = Instant::now();
    while !done(h) {
        assert!(start.elapsed() < Duration::from_secs(5), "never: {what}");
        std::thread::sleep(Duration::from_millis(15));
        steps(h);
    }
}

#[test]
fn the_first_quiet_check_is_two_minutes_after_launch_and_then_every_six_hours() {
    let w = world();
    let (mut h, prepared) = scheduled(&w);
    w.fakes.clock.advance(FIRST_CHECK_AFTER_MS - 1.0);
    steps(&mut h);
    assert_eq!(stamp(&h), None, "not yet");
    assert!(h.query_by_label_contains("Update").is_none());
    w.fakes.clock.advance(1.0);
    steps(&mut h);
    let first = w.fakes.clock.now();
    assert_eq!(stamp(&h), Some(first));
    // Downloaded before anything was said: the label is a click from
    // installing.
    h.get_by_role_and_label(Role::Button, READY_LABEL);
    assert_eq!(prepared.load(Ordering::SeqCst), 1);
    // The window sleeps until the next one is due, and wakes for it.
    assert_eq!(h.state().update.next_due(), Some(first + CHECK_EVERY_MS));
    w.fakes.clock.advance(CHECK_EVERY_MS - 1.0);
    steps(&mut h);
    assert_eq!(stamp(&h), Some(first), "not yet");
    w.fakes.clock.advance(1.0);
    steps(&mut h);
    assert_eq!(stamp(&h), Some(first + CHECK_EVERY_MS));
    assert_eq!(
        prepared.load(Ordering::SeqCst),
        1,
        "the same version found again is not downloaded again"
    );
    h.get_by_role_and_label(Role::Button, READY_LABEL);
}

#[test]
fn with_the_switch_off_the_schedule_never_asks() {
    let w = world();
    let (mut h, prepared) = scheduled(&w);
    u::set_automatic(h.state_mut().host.storage.as_mut(), false);
    w.fakes.clock.advance(FIRST_CHECK_AFTER_MS + CHECK_EVERY_MS);
    steps(&mut h);
    w.fakes.clock.advance(CHECK_EVERY_MS);
    steps(&mut h);
    assert_eq!(stamp(&h), None);
    assert_eq!(prepared.load(Ordering::SeqCst), 0);
}

#[test]
fn a_window_whose_schedule_was_never_armed_never_checks_on_its_own() {
    // Every test, capture and headless run: only main.rs arms it.
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    h.state_mut().host.updater = Arc::new(FakeUpdater {
        found: newer(),
        ..FakeUpdater::default()
    });
    w.fakes
        .clock
        .advance(FIRST_CHECK_AFTER_MS + 3.0 * CHECK_EVERY_MS);
    steps(&mut h);
    assert_eq!(stamp(&h), None);
    assert_eq!(h.state().update.next_due(), None);
}

#[test]
fn whoever_said_no_to_the_old_question_keeps_the_switch_off() {
    let w = world();
    let mut host = w.fakes.host(Platform::Mac);
    host.storage.set_item("updateAsked", "true");
    host.storage.set_item("updateOptIn", "false");
    host.updater = Arc::new(FakeUpdater {
        found: newer(),
        ..FakeUpdater::default()
    });
    let mut app = App::new(w.engine.clone(), Mode::Inline, host);
    let now = app.now();
    app.update.start_schedule(now);
    assert!(!app.update.automatic(&app.host));
    let mut h = egui_kittest::Harness::builder()
        .with_size([1280.0, 800.0])
        .build_ui_state(|ui, app: &mut App| app.frame(ui), app);
    steps(&mut h);
    w.fakes.clock.advance(FIRST_CHECK_AFTER_MS);
    steps(&mut h);
    assert_eq!(stamp(&h), None, "no quiet check");
    // The switch in the update sheet says so, and Check for Updates still
    // checks.
    help::choose(h.state_mut(), HelpItem::CheckForUpdates);
    steps(&mut h);
    h.get_by_role_and_label(Role::Dialog, "Updates");
    let switch = h.get_by_role_and_label(Role::CheckBox, u::AUTO_LABEL);
    assert_eq!(
        switch.accesskit_node().data().toggled(),
        Some(Toggled::False)
    );
    assert!(stamp(&h).is_some(), "a check a person asks for always runs");
    h.get_by_role_and_label(Role::Button, "Install 0.99.0");
}

#[test]
fn update_ready_installs_and_restarts_on_a_click() {
    let w = world();
    let (mut h, _) = scheduled(&w);
    let updater = FakeUpdater {
        found: newer(),
        ..FakeUpdater::default()
    };
    let restarted = updater.restarted.clone();
    h.state_mut().host.updater = Arc::new(updater);
    w.fakes.clock.advance(FIRST_CHECK_AFTER_MS);
    steps(&mut h);
    h.get_by_role_and_label(Role::Button, READY_LABEL).click();
    until(&mut h, "the new copy starts", |_| {
        restarted.load(Ordering::SeqCst) == 1
    });
    until(&mut h, "this one closes", |h| h.state().closing);
}

#[test]
fn a_download_that_fails_ahead_of_the_click_leaves_the_offer() {
    let w = world();
    let (mut h, _) = scheduled(&w);
    h.state_mut().host.updater = Arc::new(FakeUpdater {
        found: newer(),
        prepare_fails: Some("the download stopped".into()),
        ..FakeUpdater::default()
    });
    w.fakes.clock.advance(FIRST_CHECK_AFTER_MS);
    steps(&mut h);
    assert!(
        h.query_by_role_and_label(Role::Button, READY_LABEL)
            .is_none()
    );
    h.get_by_role_and_label(Role::Button, "Update to 0.99.0");
}

#[test]
fn the_switch_in_the_help_menu_turns_quiet_checks_off_and_on() {
    let w = world();
    let (mut h, _) = scheduled(&w);
    assert!(h.state().update.automatic(&h.state().host), "on by default");
    help::choose(h.state_mut(), HelpItem::AutomaticUpdates);
    assert!(!h.state().update.automatic(&h.state().host));
    w.fakes.clock.advance(FIRST_CHECK_AFTER_MS);
    steps(&mut h);
    assert_eq!(stamp(&h), None);
    // On again: checks at once.
    help::choose(h.state_mut(), HelpItem::AutomaticUpdates);
    steps(&mut h);
    assert!(h.state().update.automatic(&h.state().host));
    assert!(stamp(&h).is_some());
    h.get_by_role_and_label(Role::Button, READY_LABEL);
}

// ------------------------------------------------------------ What's New

fn opened_after(w: &World, seen: Option<&str>, earlier: bool) -> H {
    let mut host = w.fakes.host(Platform::Mac);
    if let Some(seen) = seen {
        host.storage.set_item("whatsNewSeen", seen);
    }
    if earlier {
        host.storage.set_item("updateFound", "");
        host.storage.set_item("updateLastChecked", "1767225600000");
    }
    let app = App::new(w.engine.clone(), Mode::Inline, host);
    let mut h = egui_kittest::Harness::builder()
        .with_size([1280.0, 800.0])
        .build_ui_state(|ui, app: &mut App| app.frame(ui), app);
    steps(&mut h);
    h
}

#[test]
fn whats_new_opens_once_after_an_update_and_not_on_a_fresh_install() {
    let w = world();
    let fresh = opened_after(&w, None, false);
    assert!(
        fresh
            .query_by_role_and_label(Role::Dialog, "Release notes")
            .is_none()
    );
    let updated = opened_after(&w, Some("0.0.1"), false);
    updated.get_by_role_and_label(Role::Dialog, "Release notes");
    // The first 0.13 on a computer an earlier copy used.
    let first = opened_after(&w, None, true);
    first.get_by_role_and_label(Role::Dialog, "Release notes");
    let seen = first.state().host.storage.get_item("whatsNewSeen");
    assert_eq!(seen.as_deref(), Some(env!("CARGO_PKG_VERSION")));
    let again = opened_after(&w, seen.as_deref(), true);
    assert!(
        again
            .query_by_role_and_label(Role::Dialog, "Release notes")
            .is_none()
    );
}

// ------------------------------------------------------------ the menu

#[test]
fn on_windows_and_linux_the_help_menu_is_at_the_foot_of_the_page() {
    for platform in [Platform::Linux, Platform::Windows] {
        let w = world();
        let mut h = harness(&w, platform);
        h.get_by_role_and_label(Role::Button, "Help").click();
        steps(&mut h);
        for item in [
            HelpItem::About,
            HelpItem::UserGuide,
            HelpItem::Shortcuts,
            HelpItem::WhatsNew,
            HelpItem::CheckForUpdates,
            HelpItem::ReportBug,
            HelpItem::Licenses,
            HelpItem::Suite,
        ] {
            h.get_by_role_and_label(Role::MenuItem, &item.label());
        }
        // No self-update here: the switch is there, and off.
        let label = HelpItem::AutomaticUpdates.label();
        let switch = h.get_by_role_and_label(Role::MenuItemCheckBox, &label);
        assert!(switch.accesskit_node().is_disabled());
        h.get_by_role_and_label(Role::MenuItem, "Method Production Suite")
            .click();
        steps(&mut h);
        assert_eq!(w.fakes.opener.opened.lock().unwrap().as_slice(), [SUITE]);
    }
}

#[test]
fn a_mac_has_no_help_menu_in_the_window() {
    let w = world();
    let h = harness(&w, Platform::Mac);
    assert!(h.query_by_role_and_label(Role::Button, "Help").is_none());
}

#[test]
fn each_page_the_help_menu_opens_is_one_the_window_may_open() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    for item in [HelpItem::UserGuide, HelpItem::ReportBug, HelpItem::Suite] {
        help::choose(h.state_mut(), item);
    }
    let opened = w.fakes.opener.opened.lock().unwrap().clone();
    assert_eq!(opened.len(), 3, "{opened:?}");
    assert_eq!(opened[0], USER_GUIDE);
    assert!(opened[1].starts_with(sp_app::model::feedback::NEW_ISSUE_BASE));
    assert!(opened[1].contains(env!("CARGO_PKG_VERSION")));
    assert_eq!(opened[2], SUITE);
}

#[test]
fn about_says_the_version_the_website_and_the_licence() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    help::choose(h.state_mut(), HelpItem::About);
    steps(&mut h);
    h.get_by_role_and_label(Role::Dialog, "About Method Reader");
    assert!(has(
        &h,
        concat!("Method Reader ", env!("CARGO_PKG_VERSION"))
    ));
    assert!(has(&h, sp_app::model::help::COPYRIGHT));
    assert!(has(&h, sp_app::model::help::LICENCE_LINE));
    h.get_by_role_and_label(Role::Button, "methodreader.com")
        .click();
    steps(&mut h);
    assert_eq!(w.fakes.opener.opened.lock().unwrap().as_slice(), [WEBSITE]);
    h.get_by_role_and_label(Role::Button, "Licenses and Notices")
        .click();
    steps(&mut h);
    assert_eq!(h.state().help.sheet, Some(Sheet::Licenses));
}

#[test]
fn licenses_and_notices_shows_each_text_built_into_the_app() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    help::choose(h.state_mut(), HelpItem::Licenses);
    steps(&mut h);
    h.get_by_role_and_label(Role::Dialog, "Licenses and Notices");
    for notice in NOTICES {
        h.get_by_role_and_label(Role::Tab, notice.title).click();
        steps(&mut h);
        let shown = h.get_by_role_and_label(Role::MultilineTextInput, notice.title);
        let value = shown.accesskit_node().value().unwrap_or_default();
        assert!(
            value.starts_with(&notice.text[..40]),
            "{}: {:?}",
            notice.title,
            &value[..value.len().min(60)]
        );
    }
    h.key_press(egui::Key::Escape);
    steps(&mut h);
    assert_eq!(h.state().help.sheet, None);
}

#[test]
fn keyboard_shortcuts_lists_the_window_s_own_keys() {
    let w = world();
    let mut h = harness(&w, Platform::Linux);
    help::choose(h.state_mut(), HelpItem::Shortcuts);
    steps(&mut h);
    h.get_by_role_and_label(Role::Dialog, "Keyboard Shortcuts");
    assert!(has(&h, "Ctrl+O"));
    assert!(has(&h, "Choose a screenplay to convert"));
}

#[test]
fn check_for_updates_answers_on_a_mac_and_says_where_releases_are_elsewhere() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    help::choose(h.state_mut(), HelpItem::CheckForUpdates);
    steps(&mut h);
    h.get_by_role_and_label(Role::Dialog, "Updates");
    h.get_by_role_and_label(
        Role::Status,
        concat!(
            "Method Reader ",
            env!("CARGO_PKG_VERSION"),
            " is the newest there is."
        ),
    );

    let w = world();
    let mut linux = harness(&w, Platform::Linux);
    help::choose(linux.state_mut(), HelpItem::CheckForUpdates);
    steps(&mut linux);
    linux.get_by_role_and_label(Role::Dialog, "Updates");
    assert!(has(&linux, u::NOT_HERE));
    linux
        .get_by_role_and_label(Role::Button, "Open the releases page")
        .click();
    steps(&mut linux);
    assert_eq!(w.fakes.opener.opened.lock().unwrap().as_slice(), [RELEASES]);
}

#[test]
fn whats_new_from_the_menu_opens_the_release_notes() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    help::choose(h.state_mut(), HelpItem::About);
    help::choose(h.state_mut(), HelpItem::WhatsNew);
    steps(&mut h);
    h.get_by_role_and_label(Role::Dialog, "Release notes");
    assert!(
        h.query_by_role_and_label(Role::Dialog, "About Method Reader")
            .is_none()
    );
}
