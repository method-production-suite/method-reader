//! An old-named bundle (Screepub.app, Screepub Desktop.app) takes the name
//! Method Reader.app once, at launch; nothing else is ever renamed.

use std::path::{Path, PathBuf};

use sp_update_config::{BUNDLE_NAME, OLD_BUNDLE_NAMES, rename_old_bundle, undo_rename};

/// `<dir>/<name>/Contents/MacOS/method-reader-app`, with a resource beside it.
fn bundle(dir: &Path, name: &str) -> PathBuf {
    let macos = dir.join(name).join("Contents/MacOS");
    std::fs::create_dir_all(&macos).unwrap();
    std::fs::create_dir_all(dir.join(name).join("Contents/Resources")).unwrap();
    std::fs::write(dir.join(name).join("Contents/Resources/LICENSE"), "x").unwrap();
    let exe = macos.join("method-reader-app");
    std::fs::write(&exe, "bin").unwrap();
    exe
}

#[test]
fn either_old_name_becomes_method_reader_once() {
    for old in OLD_BUNDLE_NAMES {
        let apps = tempfile::tempdir().unwrap();
        let exe = bundle(apps.path(), old);
        let r = rename_old_bundle(&exe).unwrap_or_else(|| panic!("{old} not renamed"));
        assert_eq!(r.from, apps.path().join(old));
        assert_eq!(r.to, apps.path().join(BUNDLE_NAME));
        assert_eq!(
            r.exe,
            apps.path()
                .join(BUNDLE_NAME)
                .join("Contents/MacOS/method-reader-app")
        );
        assert!(r.exe.is_file(), "the program moved with its folder");
        assert!(r.to.join("Contents/Resources/LICENSE").is_file());
        assert!(!apps.path().join(old).exists());
        // Once: the new name is never renamed again.
        assert_eq!(rename_old_bundle(&r.exe), None);
    }
}

#[test]
fn an_existing_method_reader_app_is_never_replaced() {
    let apps = tempfile::tempdir().unwrap();
    let exe = bundle(apps.path(), "Screepub Desktop.app");
    let theirs = bundle(apps.path(), BUNDLE_NAME);
    assert_eq!(rename_old_bundle(&exe), None);
    assert!(exe.is_file() && theirs.is_file());
}

#[test]
fn a_name_the_person_chose_stays() {
    let apps = tempfile::tempdir().unwrap();
    for name in ["Method Reader old.app", "Screepub copy.app", "Reader.app"] {
        let exe = bundle(apps.path(), name);
        assert_eq!(rename_old_bundle(&exe), None, "{name}");
        assert!(exe.is_file(), "{name}");
    }
}

#[test]
fn outside_a_bundle_or_translocated_nothing_moves() {
    let dir = tempfile::tempdir().unwrap();
    // A development build: target/release/method-reader-app.
    let loose = dir.path().join("target/release/method-reader-app");
    std::fs::create_dir_all(loose.parent().unwrap()).unwrap();
    std::fs::write(&loose, "bin").unwrap();
    assert_eq!(rename_old_bundle(&loose), None);
    // macOS runs a quarantined download from a random read-only place.
    let moved = dir.path().join("AppTranslocation/ABCD/d");
    let exe = bundle(&moved, "Screepub.app");
    assert_eq!(rename_old_bundle(&exe), None);
    assert!(exe.is_file());
}

#[cfg(unix)]
#[test]
fn a_folder_that_refuses_the_rename_changes_nothing() {
    use std::os::unix::fs::PermissionsExt;
    let apps = tempfile::tempdir().unwrap();
    let exe = bundle(apps.path(), "Screepub Desktop.app");
    std::fs::set_permissions(apps.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
    let r = rename_old_bundle(&exe);
    std::fs::set_permissions(apps.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(r, None);
    assert!(exe.is_file());
    assert!(!apps.path().join(BUNDLE_NAME).exists());
}

#[test]
fn undo_puts_the_old_folder_back() {
    let apps = tempfile::tempdir().unwrap();
    let exe = bundle(apps.path(), "Screepub Desktop.app");
    let r = rename_old_bundle(&exe).unwrap();
    undo_rename(&r).unwrap();
    assert!(exe.is_file());
    assert!(!apps.path().join(BUNDLE_NAME).exists());
}
