//! The window's identity from 0.13: the bundle identifier
//! com.methodreader.desktop, the app-data folder named after it, and where
//! the window still looks for what the 0.12.x copies (com.darkwell.screepub)
//! kept, so a person's update answer survives the change.

use std::path::Path;

use sp_app::app::host::{FileStorage, webkit_identifiers};
use sp_app::model::update::{Storage, read_state};

#[test]
fn the_app_data_folder_is_named_after_the_bundle_identifier() {
    assert_eq!(sp_update::IDENTIFIER, "com.methodreader.desktop");
    assert_eq!(sp_cli::APP_DATA_FOLDER, sp_update::IDENTIFIER);
    assert_eq!(sp_update::LEGACY_IDENTIFIER, "com.darkwell.screepub");
}

#[test]
fn the_tauri_windows_webkit_storage_is_looked_for_under_their_identifiers() {
    assert_eq!(
        webkit_identifiers(sp_update::IDENTIFIER),
        [
            "com.methodreader.desktop",
            "com.darkwell.screepub",
            "com.darkwell.screepub.desktop",
        ]
    );
    // A rehearsal's identifier extends the shipped one, and so does the
    // Tauri window it updates from.
    assert_eq!(
        webkit_identifiers("com.methodreader.desktop.rehearsal"),
        [
            "com.methodreader.desktop.rehearsal",
            "com.darkwell.screepub.rehearsal",
        ]
    );
    assert_eq!(
        webkit_identifiers("com.example.other"),
        ["com.example.other"]
    );
}

/// The 0.12 window's file, as the one-time move carries it (byte for byte,
/// sp-engine's tests/legacy_names.rs) or leaves it in place.
fn said_no(at: &Path) {
    std::fs::write(
        at,
        r#"{"updateAsked":"true","updateLastChecked":"1767225600000","updateOptIn":"false"}"#,
    )
    .unwrap();
}

#[test]
fn someone_who_said_no_in_0_12_still_has_said_no() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("window-storage.json");
    said_no(&path);
    // A file is there, so the Tauri window's WebKit answer (a yes, here) is
    // not read over it.
    let tauri_said_yes =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../sp-update/tests/data/webkit-legacy");
    let s = FileStorage::open_or_import(path, Some(&tauri_said_yes));
    let state = read_state(&s);
    assert!(state.asked);
    assert!(!state.opted_in);
    assert_eq!(s.get_item("updateOptIn").as_deref(), Some("false"));
}
