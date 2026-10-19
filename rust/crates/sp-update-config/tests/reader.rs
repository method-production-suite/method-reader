//! What Method Reader hands method-update, and the Reader-only behaviour
//! that rides on it: the one-time rename from `Screepub.app` to
//! `Method Reader.app`, the sweep that treats the two names as one bundle,
//! the window's own sentences, and the transport every check uses.
//! method-update's own tests run the same machinery under invented names
//! (`Old Example.app`, `Method Example.app`); these hold it to Reader's.

use std::path::Path;
use std::time::{Duration, SystemTime};

use method_update::install::{
    InstallError, Swept, install_app, install_destination, sweep_at_startup_stamped,
    sweep_beside_stamped,
};
use method_update::{Installer, transport};
use sp_update_config::{
    BUNDLE_NAME, ENDPOINT, LEGACY_BUNDLE_NAME, LEGACY_ENDPOINT, PUBKEY, bundle_names, update_config,
};

fn no_admin(_: &Path, _: &Path) -> Result<(), InstallError> {
    panic!("no administrator should be needed in a temp folder")
}

/// Every leftover dated long ago (a test can set an mtime, never a ctime).
fn ancient(_: &std::fs::Metadata) -> SystemTime {
    SystemTime::UNIX_EPOCH
}

/// A bundle on disk: Info.plist, one executable, one resource.
fn make_bundle(at: &Path, exe: &str, version: &str) {
    let contents = at.join("Contents");
    std::fs::create_dir_all(contents.join("MacOS")).unwrap();
    std::fs::create_dir_all(contents.join("Resources")).unwrap();
    std::fs::write(
        contents.join("Info.plist"),
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>{exe}</string>
  <key>CFBundleIdentifier</key>
  <string>{}</string>
  <key>CFBundleShortVersionString</key>
  <string>{version}</string>
</dict>
</plist>
"#,
            sp_update_config::IDENTIFIER
        ),
    )
    .unwrap();
    let bin = contents.join("MacOS").join(exe);
    std::fs::write(&bin, format!("#!/bin/sh\necho {version}\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::fs::write(contents.join("Resources").join("LICENSE"), version).unwrap();
}

/// The update archive `sp-release update-archive` writes: top folder
/// `Method Reader.app`.
fn new_archive(version: &str) -> Vec<u8> {
    let build = tempfile::tempdir().unwrap();
    let bundle = build.path().join("Built.app");
    make_bundle(&bundle, "method-reader-app", version);
    let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut b = tar::Builder::new(gz);
    b.append_dir_all("Method Reader.app", &bundle).unwrap();
    b.into_inner().unwrap().finish().unwrap()
}

fn version_of(bundle: &Path) -> String {
    std::fs::read_to_string(bundle.join("Contents/Resources/LICENSE")).unwrap()
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[test]
fn the_names_are_the_new_one_and_the_legacy_one() {
    assert_eq!(BUNDLE_NAME, "Method Reader.app");
    assert_eq!(LEGACY_BUNDLE_NAME, "Screepub.app");
    let config = update_config();
    assert_eq!(config.display_name, "Method Reader");
    assert_eq!(config.bundle, bundle_names());
    assert_eq!(config.bundle.current, BUNDLE_NAME);
    assert_eq!(config.bundle.legacy.as_deref(), Some(LEGACY_BUNDLE_NAME));
}

#[test]
fn the_sentences_are_the_ones_the_window_always_said() {
    let m = update_config().messages;
    // sp-app's busy.rs UPDATED_MESSAGE, the Tauri window's app.js sentence.
    assert_eq!(
        m.updated,
        "Method Reader has been updated. Restart it to keep going."
    );
    assert_eq!(
        m.too_large,
        "The update is larger than any Method Reader could be."
    );
}

#[test]
fn every_check_asks_reader_s_endpoint_with_reader_s_key_and_name() {
    let config = update_config();
    assert_eq!(config.unavailable_reason(), None);
    let t = config.transport(Some(Installer::App));
    assert_eq!(t.endpoints, [ENDPOINT]);
    assert_eq!(t.pubkey, PUBKEY);
    assert!(t.require_signed_version, "every Reader release records it");
    assert!(!t.allow_insecure);
    assert_eq!(t.timeout, Some(Duration::from_secs(10 * 60)));
    assert_eq!(t.installer, Some(Installer::App));
    assert_eq!(
        t.user_agent,
        concat!("method-reader-update/", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(
        t.allowed_hosts,
        [
            "github.com",
            "objects.githubusercontent.com",
            "release-assets.githubusercontent.com"
        ]
    );
    // The release download and the old repository's redirect stay on the
    // hosts the host rule lets through.
    for url in [
        ENDPOINT,
        LEGACY_ENDPOINT,
        "https://github.com/method-production-suite/method-reader/releases/download/v0.13.1/Method-Reader-macOS.app.tar.gz",
        "https://release-assets.githubusercontent.com/github-production-release-asset/1/2",
    ] {
        assert!(
            transport::host_allowed(url, &t.allowed_hosts, false),
            "{url}"
        );
    }
    assert!(!transport::host_allowed(
        "http://github.com/x",
        &t.allowed_hosts,
        false
    ));
}

#[test]
fn an_old_screepub_bundle_alone_is_installed_as_method_reader() {
    let apps = tempfile::tempdir().unwrap();
    let old = apps.path().join("Screepub.app");
    make_bundle(&old, "Screepub", "0.13.0");
    let new = apps.path().join("Method Reader.app");
    let names = bundle_names();
    assert_eq!(install_destination(&old, &names), new);

    let installed = install_app(&new_archive("0.13.1"), &old, &names, &no_admin).unwrap();
    assert_eq!(installed.bundle, new);
    assert_eq!(installed.leftover, None);
    assert_eq!(version_of(&new), "0.13.1");
    assert_eq!(names_in(apps.path()), ["Method Reader.app"]);

    // The restart starts the NEW path's executable.
    let cmd = method_update::restart::relaunch_command(&installed.bundle, &[]).unwrap();
    assert_eq!(
        Path::new(cmd.get_program()),
        new.join("Contents/MacOS/method-reader-app")
    );
}

#[test]
fn a_method_reader_bundle_beside_an_old_one_is_never_touched() {
    let apps = tempfile::tempdir().unwrap();
    let old = apps.path().join("Screepub.app");
    make_bundle(&old, "Screepub", "0.13.0");
    let other = apps.path().join("Method Reader.app");
    make_bundle(&other, "method-reader-app", "0.14.0");
    let names = bundle_names();
    assert_eq!(install_destination(&old, &names), old);

    let installed = install_app(&new_archive("0.13.1"), &old, &names, &no_admin).unwrap();
    assert_eq!(installed.bundle, old, "updated in place");
    assert_eq!(version_of(&old), "0.13.1");
    assert_eq!(version_of(&other), "0.14.0");
}

#[test]
fn a_method_reader_bundle_updates_in_place() {
    let apps = tempfile::tempdir().unwrap();
    let bundle = apps.path().join("Method Reader.app");
    make_bundle(&bundle, "method-reader-app", "0.13.1");
    let names = bundle_names();
    assert_eq!(install_destination(&bundle, &names), bundle);
    let installed = install_app(&new_archive("0.13.2"), &bundle, &names, &no_admin).unwrap();
    assert_eq!(installed.bundle, bundle);
    assert_eq!(names_in(apps.path()), ["Method Reader.app"]);
}

#[test]
fn the_sweep_treats_screepub_and_method_reader_as_one_bundle() {
    // A renaming swap that died between its renames: no bundle under
    // either name, only the old bundle's backup and the staging folder.
    let apps = tempfile::tempdir().unwrap();
    let backup = apps.path().join(".Screepub.app.old-123");
    make_bundle(&backup, "Screepub", "0.13.0");
    let staging = apps.path().join(".Method Reader.app.update-123");
    std::fs::create_dir_all(staging.join("Contents")).unwrap();
    let swept = sweep_at_startup_stamped(
        &backup.join("Contents/MacOS/Screepub"),
        &bundle_names(),
        ancient,
    );
    let bundle = apps.path().join("Screepub.app");
    assert_eq!(swept.restored.as_deref(), Some(bundle.as_path()));
    assert_eq!(version_of(&bundle), "0.13.0");
    assert_eq!(swept.removed, std::slice::from_ref(&staging));

    // After the rename, the new bundle clears the old name's stale backup.
    let apps = tempfile::tempdir().unwrap();
    let bundle = apps.path().join("Method Reader.app");
    make_bundle(&bundle, "method-reader-app", "0.13.1");
    let stale = apps.path().join(".Screepub.app.old-77");
    make_bundle(&stale, "Screepub", "0.13.0");
    let swept = sweep_beside_stamped(
        apps.path(),
        BUNDLE_NAME,
        &bundle_names(),
        4_000_000_000,
        Some(&bundle),
        ancient,
    );
    assert_eq!(
        swept,
        Swept {
            restored: None,
            removed: vec![stale]
        }
    );
    assert_eq!(names_in(apps.path()), ["Method Reader.app"]);
}
