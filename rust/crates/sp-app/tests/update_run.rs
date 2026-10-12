//! The update's one run, from the window (inventory rows U6, U7, U9): the
//! click on "Update to …", sp-update's real flow over fake side effects
//! (FakeUpdater: the download and the swap succeed at once), the real
//! engine gate the window's busy count mirrors into, the wait for quiet,
//! "Restart now" when it does not come, and every engine call refused once
//! the bundle is swapped.

mod common;

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use common::*;
use egui::accesskit::Role;
use egui_kittest::kittest::Queryable;

use sp_app::app::host::FakeUpdater;
use sp_app::model::busy::UPDATED_MESSAGE;
use sp_app::model::convert::Platform;
use sp_app::model::update::{Found, Phase};
use sp_app::model::update_flow::Offer;

fn offered(h: &mut H, updater: FakeUpdater) -> Arc<std::sync::atomic::AtomicUsize> {
    let restarted = updater.restarted.clone();
    let app = h.state_mut();
    app.host.updater = Arc::new(updater);
    app.update.flow.offer_found(Offer {
        version: "0.12.0".into(),
        body: "Faster pages.".into(),
        live: true,
    });
    steps(h);
    restarted
}

fn found() -> Result<Option<Found>, String> {
    Ok(Some(Found {
        version: "0.12.0".into(),
        current_version: "0.11.1".into(),
        body: Some("Faster pages.".into()),
    }))
}

/// Step the window (in real time: the run is on its own thread) until
/// `done` holds, or fail after a few seconds.
fn until(h: &mut H, what: &str, done: impl Fn(&H) -> bool) {
    let start = Instant::now();
    while !done(h) {
        assert!(start.elapsed() < Duration::from_secs(5), "never: {what}");
        std::thread::sleep(Duration::from_millis(15));
        steps(h);
    }
}

fn phase(h: &H) -> Option<Phase> {
    h.state().update.phase().cloned()
}

fn closed(h: &H) -> bool {
    h.state().closing
}

#[test]
fn u7_a_quiet_window_installs_restarts_and_closes() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    let restarted = offered(
        &mut h,
        FakeUpdater {
            found: found(),
            quiet: Duration::from_millis(20),
            ..FakeUpdater::default()
        },
    );
    h.get_by_role_and_label(Role::Button, "Update to 0.12.0")
        .click();
    let start = Instant::now();
    while restarted.load(Ordering::SeqCst) == 0 || !closed(&h) {
        assert!(start.elapsed() < Duration::from_secs(5), "never restarted");
        std::thread::sleep(Duration::from_millis(15));
        steps(&mut h);
    }
    assert!(matches!(phase(&h), Some(Phase::Restarting { .. })));
    assert!(h.state().busy.retired(), "no engine call after the swap");
}

#[test]
fn u7_work_in_flight_waits_then_offers_restart_now() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    let restarted = offered(
        &mut h,
        FakeUpdater {
            found: found(),
            quiet: Duration::from_millis(20),
            cap: Duration::from_millis(300),
            ..FakeUpdater::default()
        },
    );
    // Somebody's work is out (a Settings change in its settle, say).
    let hold = h.state_mut().busy.hold();
    h.get_by_role_and_label(Role::Button, "Update to 0.12.0")
        .click();
    until(&mut h, "the wait is said", |h| {
        has(h, "Restarting after this finishes…") || has(h, "Restart now")
    });
    until(&mut h, "the cap runs out", |h| {
        matches!(phase(h), Some(Phase::Stalled { .. }))
    });
    h.get_by_role_and_label(Role::Button, "Restart now");
    assert_eq!(restarted.load(Ordering::SeqCst), 0);

    // Every engine call is refused now, with the window's own sentence.
    w.fakes.answer_dialog(Some("/scripts/field-station.pdf"));
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::O);
    steps(&mut h);
    assert!(has(&h, UPDATED_MESSAGE), "the refusal is shown");
    assert!(w.engine.calls_of("convert").is_empty());

    h.get_by_role_and_label(Role::Button, "Restart now").click();
    until(&mut h, "the new copy starts", |_| {
        restarted.load(Ordering::SeqCst) == 1
    });
    until(&mut h, "this window closes", closed);
    let _ = hold;
}

#[test]
fn u6_a_copy_that_cannot_restart_says_quit_and_reopen() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    offered(
        &mut h,
        FakeUpdater {
            found: found(),
            can_restart: false,
            ..FakeUpdater::default()
        },
    );
    h.get_by_role_and_label(Role::Button, "Update to 0.12.0")
        .click();
    until(&mut h, "installed", |h| {
        has(
            h,
            "Update installed. Quit and reopen Screepub to use 0.12.0.",
        )
    });
    assert!(!closed(&h));
}

#[test]
fn u9_a_remembered_offer_checks_afresh_and_a_withdrawn_release_takes_the_label_down() {
    let w = world();
    let mut h = harness(&w, Platform::Mac);
    let app = h.state_mut();
    app.host.updater = Arc::new(FakeUpdater::default());
    // Drawn from memory: no live handle.
    app.update.flow.offer_found(Offer {
        version: "0.12.0".into(),
        body: String::new(),
        live: false,
    });
    steps(&mut h);
    h.get_by_role_and_label(Role::Button, "Update to 0.12.0")
        .click();
    until(&mut h, "the label goes", |h| {
        h.query_by_role_and_label(Role::Button, "Update to 0.12.0")
            .is_none()
            && h.state().update.phase().is_none()
    });
    assert!(!h.state().busy.retired(), "nothing was installed");
}
