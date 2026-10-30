//! tests/mtp-window-rules.test.ts and send-panel-ui.test.ts's "a Kindle over
//! MTP": the window's rules for a Kindle the engine reaches over MTP, which
//! fields survive the clean rebuild of the engine's answers, the words while
//! it copies and when it is done, and the Beta badge. MTP itself stays off
//! in the engine; these hold the window ready for it.

mod common;

use common::*;
use egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{Value, json};

use sp_app::engine::{Reply, Request, default_answer};
use sp_app::model::convert::Platform;
use sp_app::model::device::copied_state;
use sp_app::model::device_watch::devices_from;
use sp_app::model::send_routes::{
    Device, copying, is_beta_device, routes_from, sent_line, with_transport,
};
use sp_device::camera_import::CAMERA_IMPORT_NOTE;

fn mtp_row(note: Option<&str>) -> Value {
    let mut device = json!({"id": "mtp:S1", "kind": "kindle", "name": "Kindle Paperwhite", "volume": null, "transport": "mtp"});
    if let Some(n) = note {
        device["copyNote"] = json!(n);
    }
    json!({"id": "device:kindle", "key": "device:kindle", "title": "Kindle Paperwhite", "detail": "copies over USB",
        "button": "Copy to Kindle Paperwhite", "available": true, "device": device})
}

fn answer(note: Option<&str>) -> Value {
    let mut routes = default_answer(&Request::Routes {
        epub: String::new(),
        quick: false,
    });
    let list = routes["routes"].as_array_mut().unwrap();
    let k = list
        .iter_mut()
        .find(|r| r["id"] == "device:kindle")
        .unwrap();
    *k = mtp_row(note);
    routes["chosen"] = json!("device:kindle");
    routes
}

fn mtp_device() -> Device {
    Device {
        id: "mtp:S1".into(),
        kind: "kindle".into(),
        name: "Kindle Paperwhite".into(),
        volume: None,
        transport: Some("mtp".into()),
        copy_note: Some(CAMERA_IMPORT_NOTE.into()),
    }
}

fn device_of(routes: &Value) -> Device {
    routes_from(routes)
        .unwrap()
        .routes
        .into_iter()
        .find_map(|r| r.device)
        .unwrap()
}

// ------------------------------------------------------------ routesFrom

#[test]
fn routes_from_keeps_transport_and_the_camera_note_on_an_mtp_kindle() {
    assert_eq!(device_of(&answer(Some(CAMERA_IMPORT_NOTE))), mtp_device());
}

#[test]
fn a_transport_it_does_not_know_or_a_note_without_mtp_is_left_behind() {
    let mut a = answer(Some(CAMERA_IMPORT_NOTE));
    let row = a["routes"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["id"] == "device:kindle")
        .unwrap();
    row["device"]["transport"] = json!("bluetooth");
    let d = device_of(&a);
    assert_eq!(d.transport, None);
    assert_eq!(d.copy_note, None);
}

// ----------------------------------------------------- devicesFrom (watcher)

#[test]
fn the_watcher_keeps_transport_and_the_note_and_takes_a_volume_of_null() {
    let list = devices_from(&json!({"ok": true, "devices": [
        {"id": "mtp:S1", "kind": "kindle", "name": "Kindle", "volume": null, "transport": "mtp", "copyNote": "n"}
    ]}))
    .unwrap();
    assert_eq!(
        list,
        vec![Device {
            id: "mtp:S1".into(),
            kind: "kindle".into(),
            name: "Kindle".into(),
            volume: None,
            transport: Some("mtp".into()),
            copy_note: Some("n".into()),
        }]
    );
    let plain = Device {
        id: "x".into(),
        ..Default::default()
    };
    assert_eq!(
        with_transport(plain.clone(), &json!({"copyNote": "n"})),
        plain
    );
}

// --------------------------------------------------------------- the words

#[test]
fn while_it_copies_the_note_follows_the_usual_line() {
    let device = mtp_device();
    assert_eq!(
        copying(&device).line,
        format!("Copying it to Kindle Paperwhite… {CAMERA_IMPORT_NOTE}")
    );
    let quiet = Device {
        copy_note: None,
        ..device
    };
    assert_eq!(copying(&quiet).line, "Copying it to Kindle Paperwhite…");
}

#[test]
fn done_is_safe_to_unplug_never_eject() {
    let device = mtp_device();
    let sent =
        json!({"ok": true, "destination": "documents/Field Station.kfx", "safeToUnplug": true});
    assert_eq!(
        sent_line(&device, &sent),
        "Sent to Kindle Paperwhite: documents/Field Station.kfx. It is safe to unplug."
    );
    let (line, eject) = copied_state("Field Station", &device, &sent);
    assert_eq!(
        line,
        "Field Station is on your Kindle Paperwhite. It is safe to unplug."
    );
    assert!(eject.is_none());
}

#[test]
fn a_mounted_kindle_still_says_eject() {
    let mounted = Device {
        id: "/Volumes/Kindle".into(),
        kind: "kindle".into(),
        name: "Kindle".into(),
        volume: Some("/Volumes/Kindle".into()),
        ..Default::default()
    };
    let (_, eject) = copied_state(
        "Field Station",
        &mounted,
        &json!({"ok": true, "destination": "/Volumes/Kindle/documents/x.kfx"}),
    );
    assert!(eject.is_some());
}

#[test]
fn only_a_reader_over_mtp_is_beta() {
    let device = mtp_device();
    assert!(is_beta_device(Some(&device)));
    assert!(!is_beta_device(Some(&Device {
        transport: None,
        ..device
    })));
    assert!(!is_beta_device(None));
}

// ------------------------------------------------- the panel (send-panel-ui)

fn calm_kindle(w: &World, routes: Value) {
    let mut a = default_answer(&Request::AppSettings);
    a["readerDevice"] = json!("kindle");
    w.engine.always("app-settings", Reply::ok(a));
    w.engine.always("routes", Reply::ok(routes));
}

fn beside(h: &H, button: &str) -> bool {
    let rect = h.get_by_role_and_label(Role::Button, button).rect();
    h.query_all_by_label("Beta").any(|b| {
        let r = b.rect();
        (r.center().y - rect.center().y).abs() < rect.height() && r.left() >= rect.right() - 1.0
    })
}

#[test]
fn its_brass_button_carries_the_badge_beside_it_and_a_mounted_kindle_none() {
    let w = world();
    calm_kindle(&w, answer(Some(CAMERA_IMPORT_NOTE)));
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    assert!(
        beside(&h, "Copy to Kindle Paperwhite (.azw3)") || beside(&h, "Copy to Kindle Paperwhite")
    );

    let w = world();
    let mut mounted = answer(None);
    let row = mounted["routes"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| r["id"] == "device:kindle")
        .unwrap();
    row["device"] =
        json!({"id": "k1", "kind": "kindle", "name": "Kindle", "volume": "/Volumes/Kindle"});
    row["title"] = json!("Kindle");
    row["button"] = json!("Copy to Kindle");
    calm_kindle(&w, mounted);
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    // A mounted Kindle's list of books wears one badge
    // (docs/device-library.md), and our own AZW3 none since 2026-10-29
    // (registry #20); MTP adds one to the brass button and has no list.
    h.get_by_role_and_label(Role::Button, "Books on your Kindle");
    let badges = h.query_all_by_label("Beta").count() - 1;
    let mut mtp = world();
    calm_kindle(&mtp, answer(None));
    let mut hm = harness(&mtp, Platform::Mac);
    convert(&mtp, &mut hm);
    assert!(hm.query_by_label("Books on your Kindle").is_none());
    assert_eq!(hm.query_all_by_label("Beta").count(), badges + 1);
    let _ = &mut mtp;
}

#[test]
fn done_says_safe_to_unplug_and_no_eject_takes_the_brass_place() {
    let w = world();
    calm_kindle(&w, answer(Some(CAMERA_IMPORT_NOTE)));
    w.engine.always(
        "send",
        Reply::ok(json!({"ok": true, "device": {"id": "mtp:S1", "kind": "kindle", "name": "Kindle Paperwhite"},
            "destination": "documents/Field Station.kfx", "safeToUnplug": true, "cameraImportPaused": true})),
    );
    let mut h = harness(&w, Platform::Mac);
    convert(&w, &mut h);
    let brass = h
        .query_all_by_role(Role::Button)
        .find(|b| {
            b.accesskit_node()
                .label()
                .is_some_and(|l| l.starts_with("Copy to Kindle Paperwhite"))
        })
        .expect("the brass");
    brass.click_accesskit();
    steps(&mut h);
    steps(&mut h);
    let send = w.engine.calls_of("send");
    assert_eq!(send.last().unwrap().argv().last().unwrap(), "mtp:S1");
    assert!(has(
        &h,
        "Field Station is on your Kindle Paperwhite. It is safe to unplug."
    ));
    assert!(
        !h.query_all_by_role(Role::Button).any(|b| b
            .accesskit_node()
            .label()
            .is_some_and(|l| l.starts_with("Eject"))),
        "no Eject"
    );
    assert!(w.engine.calls_of("eject").is_empty());
}

#[test]
fn while_it_copies_on_a_mac_the_status_says_camera_import_is_paused() {
    let w = world();
    calm_kindle(&w, answer(Some(CAMERA_IMPORT_NOTE)));
    w.engine.hold("send");
    let mut h = harness_threads(&w, Platform::Mac);
    w.fakes.answer_dialog(Some("/scripts/field-station.pdf"));
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::O);
    settle(&mut h);
    settle(&mut h);
    let brass = h
        .query_all_by_role(Role::Button)
        .find(|b| {
            b.accesskit_node()
                .label()
                .is_some_and(|l| l.starts_with("Copy to Kindle Paperwhite"))
        })
        .expect("the brass");
    brass.click_accesskit();
    settle(&mut h);
    settle(&mut h);
    let line = h.state().panel.status.line.clone();
    assert!(
        line.starts_with("Copying it to Kindle Paperwhite…"),
        "{line}"
    );
    assert!(line.contains("Photos and Image Capture cannot import from a camera or phone"));
    w.engine.release("send");
    settle(&mut h);
}
