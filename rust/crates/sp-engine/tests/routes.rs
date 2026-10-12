//! Port of tests/routes.test.ts: the route catalog, in the order the window
//! draws it, and the rule that what you chose last time wins.

use serde_json::{Value, json};
use sp_engine::device_types::{ConnectedDevice, DeviceKind};
use sp_engine::export::routes::{
    Route, RouteFacts, Unavailable, device_kinds, preselected, routes,
};

fn dev(kind: DeviceKind, name: &str, volume: Option<&str>) -> ConnectedDevice {
    ConnectedDevice {
        kind,
        name: name.into(),
        volume: volume.map(str::to_owned),
    }
}

fn kindle() -> ConnectedDevice {
    dev(DeviceKind::Kindle, "Kindle", Some("/Volumes/Kindle"))
}
fn kindle_twin() -> ConnectedDevice {
    dev(DeviceKind::Kindle, "KINDLE2", Some("/Volumes/KINDLE2"))
}
fn kobo() -> ConnectedDevice {
    dev(
        DeviceKind::Kobo,
        "KOBOeReader",
        Some("/Volumes/KOBOeReader"),
    )
}
fn tolino() -> ConnectedDevice {
    dev(DeviceKind::Tolino, "tolino", Some("/Volumes/tolino"))
}
fn rm() -> ConnectedDevice {
    dev(DeviceKind::Remarkable, "reMarkable", None)
}

/// The Swift defaults: Books installed, no Amazon app, Apple Mail not the
/// default, nothing plugged in, on a Mac.
fn facts() -> RouteFacts {
    RouteFacts {
        platform: "darwin".into(),
        devices: vec![],
        remarkable_checked: None,
        books_app: true,
        send_to_kindle_app: false,
        apple_mail_default: false,
    }
}

fn on(platform: &str) -> RouteFacts {
    RouteFacts {
        platform: platform.into(),
        ..facts()
    }
}

fn with_devices(devices: Vec<ConnectedDevice>) -> RouteFacts {
    RouteFacts { devices, ..facts() }
}

fn find<'a>(list: &'a [Route], key: &str) -> Option<&'a Route> {
    list.iter().find(|r| r.key == key)
}

fn keys(list: &[Route]) -> Vec<&str> {
    list.iter().map(|r| r.key.as_str()).collect()
}

fn s(v: &str) -> Value {
    Value::String(v.into())
}

fn pre<'a>(list: &'a [Route], last: Option<&str>) -> &'a Route {
    let v = last.map(s);
    preselected(list, v.as_ref())
}

fn as_json(list: &[Route]) -> Value {
    serde_json::to_value(list).unwrap()
}

// ---- route ordering (ported) ----

#[test]
fn a_plugged_in_kindle_is_the_default_route() {
    let list = routes(&with_devices(vec![kindle()]));
    assert_eq!(list[0].key, "device:kindle");
    assert_eq!(
        list[0].device.as_ref().unwrap().volume.as_deref(),
        Some("/Volumes/Kindle")
    );
}

#[test]
fn no_device_apple_books_leads() {
    assert_eq!(routes(&facts())[0].key, "apple-books");
}

#[test]
fn no_device_and_no_books_send_to_kindle_leads() {
    let f = RouteFacts {
        books_app: false,
        ..facts()
    };
    assert_eq!(routes(&f)[0].key, "send-to-kindle");
}

#[test]
fn remarkable_never_arrives_as_a_volume_device() {
    let list = routes(&with_devices(vec![rm()]));
    assert_eq!(list[0].key, "remarkable");
    assert!(!list.iter().any(|r| r.key == "device:remarkable"));
    assert_eq!(list.iter().filter(|r| r.key == "remarkable").count(), 1);
}

#[test]
fn a_docked_remarkable_outranks_books() {
    assert_eq!(routes(&with_devices(vec![rm()]))[0].key, "remarkable");
}

#[test]
fn a_plugged_in_volume_wins_over_a_docked_remarkable() {
    assert_eq!(
        routes(&with_devices(vec![kindle(), rm()]))[0].key,
        "device:kindle"
    );
    assert_eq!(
        routes(&with_devices(vec![rm(), kindle()]))[0].key,
        "device:kindle"
    );
}

#[test]
fn routes_never_empty_save_always_offered_no_duplicates() {
    for books in [true, false] {
        for mail in [true, false] {
            let r = routes(&RouteFacts {
                books_app: books,
                apple_mail_default: mail,
                ..facts()
            });
            assert!(!r.is_empty());
            assert!(find(&r, "save-epub").is_some());
            assert!(find(&r, "save-kindle").is_some());
            let ids: std::collections::BTreeSet<&str> = r.iter().map(|x| x.id.as_str()).collect();
            assert_eq!(ids.len(), r.len());
        }
    }
}

#[test]
fn email_listed_but_unavailable_without_apple_mail_and_names_the_fix() {
    let list = routes(&facts());
    let email = find(&list, "email-to-kindle").unwrap();
    assert!(!email.available);
    assert!(email.detail.contains("Apple Mail"));
}

#[test]
fn email_sendable_when_apple_mail_is_the_default() {
    let list = routes(&RouteFacts {
        apple_mail_default: true,
        ..facts()
    });
    assert!(find(&list, "email-to-kindle").unwrap().available);
}

#[test]
fn books_hidden_when_books_is_absent() {
    let list = routes(&RouteFacts {
        books_app: false,
        ..facts()
    });
    assert!(find(&list, "apple-books").is_none());
}

// ---- a remembered choice outranks the ordering ----

#[test]
fn remembered_choices() {
    let all = routes(&with_devices(vec![kindle()]));
    assert_eq!(pre(&all, None).key, "device:kindle");
    assert_eq!(pre(&all, Some("send-to-kindle")).key, "send-to-kindle");
    assert_eq!(pre(&all, Some("apple-books")).key, "apple-books");
    let no_books = routes(&RouteFacts {
        books_app: false,
        ..facts()
    });
    assert_eq!(pre(&no_books, Some("apple-books")).key, "send-to-kindle");
    assert_eq!(all[0].key, "device:kindle");
    assert_eq!(all[0].id, "device:kindle#/Volumes/Kindle");
}

#[test]
fn a_remembered_value_that_is_not_a_string_falls_back() {
    let all = routes(&with_devices(vec![kindle()]));
    assert_eq!(preselected(&all, Some(&json!(7))).key, "device:kindle");
    assert_eq!(preselected(&all, Some(&Value::Null)).key, "device:kindle");
}

// ---- the menu is a catalog ----

#[test]
fn every_kind_is_listed_while_disconnected() {
    let bare = routes(&facts());
    for name in ["Kindle", "Kobo", "tolino", "reMarkable"] {
        assert!(
            bare.iter().any(|r| r.title == name && !r.available),
            "{name}"
        );
    }
    assert!(bare[0].available);
    let leading = bare.iter().position(|r| !r.available).unwrap();
    assert_eq!(leading, bare.iter().filter(|r| r.available).count());
    for r in bare.iter().filter(|x| !x.available) {
        assert!(r.detail.contains("USB") || r.detail.contains("Apple Mail"));
    }
}

#[test]
fn a_connected_kobo_replaces_its_placeholder() {
    let list = routes(&with_devices(vec![kobo()]));
    assert_eq!(list.iter().filter(|r| r.key == "device:kobo").count(), 1);
    assert!(find(&list, "device:kobo").unwrap().available);
}

#[test]
fn twins_stay_individually_addressable() {
    let twins = routes(&with_devices(vec![kindle(), kindle_twin()]));
    let ids: std::collections::BTreeSet<&str> = twins.iter().map(|x| x.id.as_str()).collect();
    assert_eq!(ids.len(), twins.len());
    assert_eq!(
        twins
            .iter()
            .find(|r| r.title == "KINDLE2")
            .unwrap()
            .device
            .as_ref()
            .unwrap()
            .volume
            .as_deref(),
        Some("/Volumes/KINDLE2")
    );
    assert!(pre(&twins, Some("device:kindle")).available);
}

#[test]
fn send_to_kindle_button_says_app_or_web() {
    let app = routes(&RouteFacts {
        send_to_kindle_app: true,
        ..facts()
    });
    assert_eq!(
        find(&app, "send-to-kindle").unwrap().button,
        "Send to Kindle app"
    );
    assert_eq!(
        find(&routes(&facts()), "send-to-kindle").unwrap().button,
        "Send to Kindle web"
    );
}

#[test]
fn remarkable_detail_names_the_epub() {
    let list = routes(&with_devices(vec![rm()]));
    let detail = &find(&list, "remarkable").unwrap().detail;
    assert!(detail.contains("EPUB"));
    assert!(!detail.contains("PDF"));
}

#[test]
fn a_remembered_kobo_stays_chosen_while_unplugged() {
    let bare = routes(&facts());
    assert_eq!(pre(&bare, Some("device:kobo")).key, "device:kobo");
    assert!(!pre(&bare, Some("device:kobo")).available);
    assert!(pre(&bare, None).available);
}

// ---- the button reads its own verb ----

#[test]
fn button_verbs() {
    assert_eq!(
        routes(&with_devices(vec![kindle()]))[0].button,
        "Copy to Kindle"
    );
    assert_eq!(
        routes(&with_devices(vec![rm()]))[0].button,
        "Upload to reMarkable"
    );
    assert_eq!(routes(&facts())[0].button, "Add to Apple Books");
    let no_books = routes(&RouteFacts {
        books_app: false,
        ..facts()
    });
    assert_eq!(no_books[0].button, "Send to Kindle web");
    let mail = routes(&RouteFacts {
        apple_mail_default: true,
        ..facts()
    });
    assert_eq!(
        find(&mail, "email-to-kindle").unwrap().button,
        "Send to Kindle email"
    );
    assert_eq!(
        find(&mail, "email-to-kindle").unwrap().title,
        "Send to Kindle email"
    );
    let list = routes(&facts());
    assert_eq!(
        find(&list, "send-to-kindle").unwrap().title,
        "Send to Kindle web"
    );
    assert_eq!(find(&list, "save-epub").unwrap().button, "Save the EPUB…");
    assert_eq!(
        find(&list, "save-kindle").unwrap().button,
        "Save a Kindle file…"
    );
}

// ---- the whole list, row by row ----

const DEVICE_DETAIL: &str = "over USB, offline, nothing leaves this computer";

fn save_epub() -> Value {
    json!({ "id": "save-epub", "key": "save-epub", "title": "Save the EPUB",
        "detail": "for email, Apple Books and most e-readers", "button": "Save the EPUB…", "available": true })
}

fn save_kindle() -> Value {
    json!({ "id": "save-kindle", "key": "save-kindle", "title": "Save a Kindle file",
        "detail": "for copying to a Kindle by hand", "button": "Save a Kindle file…", "available": true })
}

fn placeholder(kind: &str, name: &str) -> Value {
    json!({ "id": format!("device:{kind}"), "key": format!("device:{kind}"), "title": name,
        "detail": "plug in over USB to send", "button": format!("Copy to {name}"),
        "available": false, "unavailable": "connect" })
}

fn remarkable_dim(detail: &str) -> Value {
    json!({ "id": "remarkable", "key": "remarkable", "title": "reMarkable", "detail": detail,
        "button": "Upload to reMarkable", "available": false, "unavailable": "connect" })
}

fn web() -> Value {
    json!({ "id": "send-to-kindle", "key": "send-to-kindle", "title": "Send to Kindle web",
        "detail": "via Amazon, the best-looking Kindle result", "button": "Send to Kindle web", "available": true })
}

#[test]
fn a_mac_with_everything() {
    let list = routes(&RouteFacts {
        devices: vec![kindle(), rm()],
        books_app: true,
        send_to_kindle_app: true,
        apple_mail_default: true,
        ..facts()
    });
    assert_eq!(
        as_json(&list),
        json!([
            { "id": "device:kindle#/Volumes/Kindle", "key": "device:kindle", "title": "Kindle",
              "detail": DEVICE_DETAIL, "button": "Copy to Kindle", "available": true,
              "device": { "id": "/Volumes/Kindle", "kind": "kindle", "name": "Kindle", "volume": "/Volumes/Kindle" } },
            { "id": "remarkable", "key": "remarkable", "title": "reMarkable",
              "detail": "the EPUB, over its USB connection", "button": "Upload to reMarkable", "available": true,
              "device": { "id": "remarkable", "kind": "remarkable", "name": "reMarkable", "volume": null } },
            { "id": "apple-books", "key": "apple-books", "title": "Apple Books",
              "detail": "syncs to your iPhone and iPad", "button": "Add to Apple Books", "available": true },
            { "id": "send-to-kindle", "key": "send-to-kindle", "title": "Send to Kindle app",
              "detail": "via Amazon, the best-looking Kindle result", "button": "Send to Kindle app", "available": true },
            { "id": "email-to-kindle", "key": "email-to-kindle", "title": "Send to Kindle email",
              "detail": "a Mail message with the book attached", "button": "Send to Kindle email", "available": true },
            save_epub(),
            save_kindle(),
            placeholder("kobo", "Kobo"),
            placeholder("tolino", "tolino"),
        ])
    );
    // And in the TypeScript's key order, as JSON.stringify writes it.
    assert!(sp_engine::jsjson::to_string(&list[0]).starts_with(
        r#"{"id":"device:kindle#/Volumes/Kindle","key":"device:kindle","title":"Kindle","detail":"#
    ));
}

#[test]
fn a_bare_mac_without_apple_mail() {
    let list = routes(&RouteFacts {
        books_app: false,
        ..facts()
    });
    assert_eq!(
        as_json(&list),
        json!([
            web(), save_epub(), save_kindle(),
            placeholder("kindle", "Kindle"), placeholder("kobo", "Kobo"), placeholder("tolino", "tolino"),
            remarkable_dim("dock over USB to send"),
            { "id": "email-to-kindle", "key": "email-to-kindle", "title": "Send to Kindle email",
              "detail": "needs Apple Mail as the default mail app, the one mail app the attachment survives",
              "button": "Send to Kindle email", "available": false, "unavailable": "setup" },
        ])
    );
}

fn apple_books_off_mac() -> Value {
    json!({ "id": "apple-books", "key": "apple-books", "title": "Apple Books", "detail": "on a Mac only",
        "button": "Add to Apple Books", "available": false, "unavailable": "platform" })
}

fn email_off_mac() -> Value {
    json!({ "id": "email-to-kindle", "key": "email-to-kindle", "title": "Send to Kindle email",
        "detail": "on a Mac only; save the EPUB and attach it yourself", "button": "Send to Kindle email",
        "available": false, "unavailable": "platform" })
}

#[test]
fn windows_with_nothing_plugged_in() {
    let list = routes(&RouteFacts {
        books_app: false,
        ..on("win32")
    });
    assert_eq!(
        as_json(&list),
        json!([
            web(), save_epub(), save_kindle(),
            placeholder("kindle", "Kindle"), placeholder("kobo", "Kobo"),
            { "id": "device:tolino", "key": "device:tolino", "title": "tolino",
              "detail": "cannot be found on Windows: a Windows drive carries no volume name",
              "button": "Copy to tolino", "available": false, "unavailable": "platform" },
            remarkable_dim("dock over USB to send"),
            apple_books_off_mac(),
            email_off_mac(),
        ])
    );
}

#[test]
fn linux_with_a_kobo_plugged_in() {
    let list = routes(&RouteFacts {
        devices: vec![kobo()],
        ..on("linux")
    });
    assert_eq!(
        keys(&list),
        [
            "device:kobo",
            "send-to-kindle",
            "save-epub",
            "save-kindle",
            "device:kindle",
            "device:tolino",
            "remarkable",
            "apple-books",
            "email-to-kindle"
        ]
    );
}

// ---- three kinds of unavailable ----

#[test]
fn a_mac_without_apple_mail_email_is_setup() {
    assert_eq!(
        find(&routes(&facts()), "email-to-kindle")
            .unwrap()
            .unavailable,
        Some(Unavailable::Setup)
    );
}

#[test]
fn off_a_mac_books_and_email_are_platform_whatever_the_probes_said() {
    for platform in ["linux", "win32"] {
        for v in [false, true] {
            let list = routes(&RouteFacts {
                books_app: v,
                apple_mail_default: v,
                send_to_kindle_app: v,
                ..on(platform)
            });
            let books = find(&list, "apple-books").unwrap();
            let email = find(&list, "email-to-kindle").unwrap();
            assert!(!books.available);
            assert_eq!(books.unavailable, Some(Unavailable::Platform));
            assert_eq!(books.detail, "on a Mac only");
            assert!(!email.available);
            assert_eq!(email.unavailable, Some(Unavailable::Platform));
            assert_eq!(
                email.detail,
                "on a Mac only; save the EPUB and attach it yourself"
            );
            assert_eq!(
                find(&list, "send-to-kindle").unwrap().title,
                "Send to Kindle web"
            );
        }
        assert_eq!(routes(&on(platform))[0].key, "send-to-kindle");
    }
}

#[test]
fn an_unplugged_kindle_is_connect() {
    for platform in ["darwin", "linux", "win32"] {
        let list = routes(&on(platform));
        let row = find(&list, "device:kindle").unwrap();
        assert!(!row.available);
        assert_eq!(row.unavailable, Some(Unavailable::Connect));
    }
}

#[test]
fn tolino_is_platform_on_win32_and_connect_elsewhere() {
    let t = |p: &str| find(&routes(&on(p)), "device:tolino").unwrap().unavailable;
    assert_eq!(t("win32"), Some(Unavailable::Platform));
    assert_eq!(t("darwin"), Some(Unavailable::Connect));
    assert_eq!(t("linux"), Some(Unavailable::Connect));
}

#[test]
fn a_tolino_connected_on_win32_is_a_device_row() {
    let list = routes(&RouteFacts {
        devices: vec![tolino()],
        ..on("win32")
    });
    assert_eq!(list.iter().filter(|r| r.key == "device:tolino").count(), 1);
    assert!(find(&list, "device:tolino").unwrap().available);
}

#[test]
fn an_undocked_remarkable_is_connect() {
    let list = routes(&facts());
    let row = find(&list, "remarkable").unwrap();
    assert!(!row.available);
    assert_eq!(row.unavailable, Some(Unavailable::Connect));
}

#[test]
fn a_remarkable_not_looked_for_yet_says_so() {
    let quick = routes(&RouteFacts {
        remarkable_checked: Some(false),
        ..facts()
    });
    assert_eq!(
        serde_json::to_value(find(&quick, "remarkable").unwrap()).unwrap(),
        remarkable_dim("checking whether one is docked…")
    );
    let full = routes(&RouteFacts {
        remarkable_checked: Some(true),
        ..facts()
    });
    assert_eq!(
        find(&full, "remarkable").unwrap().detail,
        "dock over USB to send"
    );
    assert_eq!(
        find(&routes(&facts()), "remarkable").unwrap().detail,
        "dock over USB to send"
    );
}

// ---- row identity ----

#[test]
fn a_connected_device_row_carries_the_id_send_accepts() {
    let row = routes(&with_devices(vec![kobo()])).remove(0);
    assert_eq!(
        serde_json::to_value(row.device.as_ref().unwrap()).unwrap(),
        json!({ "id": "/Volumes/KOBOeReader", "kind": "kobo", "name": "KOBOeReader", "volume": "/Volumes/KOBOeReader" })
    );
    assert_eq!(row.title, "KOBOeReader");
    assert_eq!(row.button, "Copy to KOBOeReader");
}

#[test]
fn a_blank_name_is_named_by_its_kind() {
    let cases = [
        (dev(DeviceKind::Kindle, "", Some("E:\\")), "Kindle"),
        (dev(DeviceKind::Kobo, "   ", Some("/Volumes/   ")), "Kobo"),
        (dev(DeviceKind::Tolino, "\t\n", Some("/media/x")), "tolino"),
    ];
    for (device, name) in cases {
        let row = routes(&with_devices(vec![device.clone()])).remove(0);
        assert_eq!(row.title, name);
        assert_eq!(row.button, format!("Copy to {name}"));
        assert_eq!(row.device.as_ref().unwrap().name, name);
        assert_eq!(
            &row.device.as_ref().unwrap().id,
            device.volume.as_ref().unwrap()
        );
        assert_eq!(
            row.id,
            format!("device:{}#{}", device.kind.as_str(), device.volume.unwrap())
        );
    }
}

#[test]
fn a_docked_remarkable_with_a_blank_name_carries_its_kind_name() {
    let list = routes(&with_devices(vec![dev(DeviceKind::Remarkable, " ", None)]));
    let row = find(&list, "remarkable").unwrap();
    assert_eq!(row.title, "reMarkable");
    assert_eq!(row.device.as_ref().unwrap().name, "reMarkable");
}

#[test]
fn a_name_that_is_not_blank_is_kept_exactly() {
    let row = routes(&with_devices(vec![dev(
        DeviceKind::Kindle,
        " My Kindle ",
        Some("/Volumes/ My Kindle "),
    )]))
    .remove(0);
    assert_eq!(row.title, " My Kindle ");
    assert_eq!(row.button, "Copy to  My Kindle ");
    assert_eq!(row.device.unwrap().name, " My Kindle ");
}

#[test]
fn a_device_with_no_volume_falls_back_to_its_id() {
    let row = routes(&with_devices(vec![dev(DeviceKind::Kobo, "Kobo", None)])).remove(0);
    assert_eq!(row.id, "device:kobo#kobo");
}

#[test]
fn placeholders_carry_no_device() {
    for r in routes(&facts()).iter().filter(|x| !x.available) {
        assert!(r.device.is_none());
    }
}

#[test]
fn two_kindles_are_two_rows_in_arrival_order() {
    let list = routes(&with_devices(vec![kindle(), kindle_twin()]));
    assert_eq!(list[0].id, "device:kindle#/Volumes/Kindle");
    assert_eq!(list[1].id, "device:kindle#/Volumes/KINDLE2");
    assert_eq!(list.iter().filter(|r| r.key == "device:kindle").count(), 2);
}

#[test]
fn device_kinds_is_every_volume_mounted_kind_in_order() {
    assert_eq!(
        device_kinds(),
        vec![DeviceKind::Kindle, DeviceKind::Kobo, DeviceKind::Tolino]
    );
}

// ---- preselected ----

#[test]
fn a_remembered_kind_picks_the_first_row_of_that_kind() {
    let twins = routes(&with_devices(vec![kindle(), kindle_twin()]));
    assert_eq!(
        pre(&twins, Some("device:kindle")).id,
        "device:kindle#/Volumes/Kindle"
    );
    assert_eq!(
        pre(&twins, Some("device:kindle#/Volumes/KINDLE2")).id,
        "device:kindle#/Volumes/Kindle"
    );
}

#[test]
fn an_unknown_remembered_key_falls_back() {
    let list = routes(&RouteFacts {
        books_app: false,
        ..facts()
    });
    assert_eq!(pre(&list, Some("saveCopy")).key, "send-to-kindle");
}

#[test]
fn a_remembered_save_row_beats_a_connected_device() {
    let list = routes(&with_devices(vec![kindle()]));
    assert_eq!(pre(&list, Some("save-kindle")).key, "save-kindle");
    assert_eq!(pre(&list, Some("save-epub")).key, "save-epub");
}

#[test]
fn a_remembered_mac_only_route_off_a_mac_falls_back() {
    for platform in ["linux", "win32"] {
        let list = routes(&on(platform));
        let chosen = pre(&list, Some("apple-books"));
        assert_eq!(chosen.key, "send-to-kindle");
        assert!(chosen.available);
        assert_eq!(pre(&list, Some("email-to-kindle")).key, "send-to-kindle");
    }
    let list = routes(&RouteFacts {
        devices: vec![kindle()],
        ..on("linux")
    });
    assert_eq!(
        pre(&list, Some("apple-books")).id,
        "device:kindle#/Volumes/Kindle"
    );
}

#[test]
fn a_remembered_tolino_on_win32_falls_back_and_on_darwin_stays() {
    let w = routes(&on("win32"));
    let chosen = pre(&w, Some("device:tolino"));
    assert_eq!(chosen.key, "send-to-kindle");
    assert!(chosen.available);
    let d = routes(&on("darwin"));
    let chosen = pre(&d, Some("device:tolino"));
    assert_eq!(chosen.key, "device:tolino");
    assert_eq!(chosen.unavailable, Some(Unavailable::Connect));
}

#[test]
fn a_remembered_email_on_darwin_without_mail_stays_chosen() {
    let list = routes(&facts());
    let chosen = pre(&list, Some("email-to-kindle"));
    assert_eq!(chosen.key, "email-to-kindle");
    assert!(!chosen.available);
    assert_eq!(chosen.unavailable, Some(Unavailable::Setup));
}

#[test]
fn a_remembered_row_is_kept_unless_platform_unavailable() {
    for platform in ["darwin", "linux", "win32"] {
        let list = routes(&on(platform));
        let first_available = list.iter().find(|x| x.available).unwrap().key.clone();
        for r in &list {
            let chosen = pre(&list, Some(&r.key));
            if r.unavailable == Some(Unavailable::Platform) {
                assert_eq!(chosen.key, first_available);
                assert!(chosen.available);
            } else {
                assert_eq!(chosen.key, r.key);
            }
        }
    }
}

#[test]
fn with_no_available_row_the_first_row() {
    let all_dim: Vec<Route> = routes(&facts())
        .into_iter()
        .map(|r| Route {
            available: false,
            ..r
        })
        .collect();
    assert_eq!(pre(&all_dim, None), &all_dim[0]);
}

#[test]
fn the_first_available_row_when_nothing_is_remembered() {
    let list = routes(&facts());
    let mut reordered: Vec<Route> = list.iter().filter(|r| !r.available).cloned().collect();
    reordered.extend(list.iter().filter(|r| r.available).cloned());
    assert_eq!(pre(&reordered, None).key, "apple-books");
}

// ---- the whole matrix ----

fn matrix() -> Vec<(String, RouteFacts)> {
    let kindle_sets = [vec![], vec![kindle()], vec![kindle(), kindle_twin()]];
    let other_sets = [vec![], vec![kobo(), tolino()]];
    let mut cases = Vec::new();
    for platform in ["darwin", "linux", "win32"] {
        for books_app in [false, true] {
            for send_to_kindle_app in [false, true] {
                for apple_mail_default in [false, true] {
                    for kindles in &kindle_sets {
                        for others in &other_sets {
                            for docked in [false, true] {
                                let mut devices = kindles.clone();
                                devices.extend(others.iter().cloned());
                                if docked {
                                    devices.push(rm());
                                }
                                cases.push((
                                    format!("{platform} {books_app} {send_to_kindle_app} {apple_mail_default} {}", devices.len()),
                                    RouteFacts {
                                        platform: platform.into(),
                                        devices,
                                        remarkable_checked: None,
                                        books_app,
                                        send_to_kindle_app,
                                        apple_mail_default,
                                    },
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    cases
}

#[test]
fn the_matrix_covers_every_combination() {
    assert_eq!(matrix().len(), 3 * 2 * 2 * 2 * 3 * 2 * 2);
}

#[test]
fn the_matrix_holds_every_rule() {
    for (label, f) in matrix() {
        let list = routes(&f);
        let ids: std::collections::BTreeSet<&str> = list.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids.len(), list.len(), "{label}");
        for r in &list {
            for text in [&r.title, &r.detail, &r.button] {
                assert!(!text.contains('\u{2014}'), "{label} {text}");
            }
            if r.available {
                assert!(r.unavailable.is_none());
                assert!(
                    serde_json::to_value(r)
                        .unwrap()
                        .get("unavailable")
                        .is_none()
                );
            } else {
                assert!(r.unavailable.is_some());
                assert!(!r.detail.trim().is_empty());
            }
            assert!(!r.title.is_empty() && !r.button.is_empty());
        }
        assert!(find(&list, "save-epub").unwrap().available);
        assert!(find(&list, "save-kindle").unwrap().available);
        assert!(list[0].available, "{label}");
        let first_dim = list.iter().position(|r| !r.available).unwrap_or(list.len());
        assert!(list[first_dim..].iter().all(|r| !r.available), "{label}");
        assert!(pre(&list, None).available);
        for kind in device_kinds() {
            let connected = f.devices.iter().filter(|d| d.kind == kind).count();
            let rows: Vec<&Route> = list
                .iter()
                .filter(|r| r.key == format!("device:{}", kind.as_str()))
                .collect();
            assert_eq!(rows.len(), connected.max(1), "{label}");
            assert!(rows.iter().all(|r| r.available == (connected > 0)));
        }
        assert_eq!(list.iter().filter(|r| r.key == "remarkable").count(), 1);
        assert!(!list.iter().any(|r| r.key == "device:remarkable"));
    }
}
