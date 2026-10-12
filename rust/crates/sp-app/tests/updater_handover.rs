//! The window's half of the handover: the update consent read once from
//! the Tauri window's WebKit storage, and an updater that only claims to
//! work inside a Mac app bundle (rows U1, U2).

use std::path::Path;

use sp_app::app::host::{FileStorage, ManifestUpdater, Updater};
use sp_app::model::update::{Storage, read_state};

fn legacy() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../sp-update/tests/data/webkit-legacy")
}

#[test]
fn the_first_run_starts_from_the_tauri_window_s_answer_and_only_the_first() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("window-storage.json");
    let s = FileStorage::open_or_import(path.clone(), Some(&legacy()));
    let state = read_state(&s);
    assert!(state.opted_in, "said yes in the Tauri window, still yes");
    assert!(state.asked, "not asked again");
    assert_eq!(state.last_checked, Some(1_767_225_600_000.0));
    assert_eq!(s.get_item("updateFound").as_deref(), Some("0.11.1"));
    assert!(path.exists(), "written down, so the import happens once");

    // A later answer is the Rust app's own; the window's file is not read
    // again.
    let mut s = s;
    s.set_item("updateOptIn", "false");
    let again = FileStorage::open_or_import(path, Some(&legacy()));
    assert!(!read_state(&again).opted_in);
}

#[test]
fn no_tauri_storage_means_the_question_is_asked() {
    let dir = tempfile::tempdir().unwrap();
    let s = FileStorage::open_or_import(dir.path().join("w.json"), Some(&dir.path().join("none")));
    assert!(!read_state(&s).asked);
    let s = FileStorage::open_or_import(dir.path().join("x.json"), None);
    assert!(!read_state(&s).asked);
}

#[test]
fn a_test_binary_is_not_an_app_bundle_so_it_cannot_update_itself() {
    assert!(!ManifestUpdater::for_this_copy().ready());
}
