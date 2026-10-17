//! The one-time bundle rename. Every 0.12.x copy updates in place, so 0.13
//! first lands at the old path, `Screepub.app`. Its own updater then
//! installs the next version as `Method Reader.app` beside it and removes
//! the old bundle, and the restart starts the new path. A `Method Reader.app`
//! already there (a fresh download beside an old copy) is never touched:
//! the old bundle then updates in place, as before.

mod common;

use common::{archive_bundle, make_bundle, tar_gz};
use sp_update::install::{
    BUNDLE_NAME, InstallError, LEGACY_BUNDLE_NAME, install_app, install_destination,
};
use std::path::Path;

fn no_admin(_: &Path, _: &Path) -> Result<(), InstallError> {
    panic!("no administrator should be needed in a temp folder")
}

fn version_of(bundle: &Path) -> String {
    std::fs::read_to_string(bundle.join("Contents/Resources/LICENSE")).unwrap()
}

fn names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

fn new_archive(version: &str) -> Vec<u8> {
    let build = tempfile::tempdir().unwrap();
    make_bundle(
        &build.path().join("Built.app"),
        "method-reader-app",
        version,
    );
    archive_bundle(&build.path().join("Built.app"))
}

#[test]
fn the_names_are_the_new_one_and_the_legacy_one() {
    assert_eq!(BUNDLE_NAME, "Method Reader.app");
    assert_eq!(LEGACY_BUNDLE_NAME, "Screepub.app");
}

#[test]
fn an_old_bundle_alone_is_installed_as_method_reader() {
    let apps = tempfile::tempdir().unwrap();
    let old = apps.path().join("Screepub.app");
    make_bundle(&old, "Screepub", "0.13.0");
    let new = apps.path().join("Method Reader.app");
    assert_eq!(install_destination(&old), new);

    let installed = install_app(&new_archive("0.13.1"), &old, &no_admin).unwrap();
    assert_eq!(installed.bundle, new);
    assert_eq!(installed.leftover, None);
    assert_eq!(version_of(&new), "0.13.1");
    // The old bundle is gone and nothing hidden is left beside the new one.
    assert_eq!(names(apps.path()), ["Method Reader.app"]);

    // The restart starts the NEW path's executable.
    let cmd = sp_update::restart::relaunch_command(&installed.bundle, &[]).unwrap();
    assert_eq!(
        Path::new(cmd.get_program()),
        new.join("Contents/MacOS/method-reader-app")
    );
}

#[test]
fn a_method_reader_bundle_already_beside_it_is_never_touched() {
    let apps = tempfile::tempdir().unwrap();
    let old = apps.path().join("Screepub.app");
    make_bundle(&old, "Screepub", "0.13.0");
    let other = apps.path().join("Method Reader.app");
    make_bundle(&other, "method-reader-app", "0.14.0");
    assert_eq!(install_destination(&old), old);

    let installed = install_app(&new_archive("0.13.1"), &old, &no_admin).unwrap();
    assert_eq!(installed.bundle, old, "the old path updates in place");
    assert_eq!(version_of(&old), "0.13.1");
    assert_eq!(version_of(&other), "0.14.0", "the other copy is left alone");
    assert_eq!(names(apps.path()), ["Method Reader.app", "Screepub.app"]);
}

#[cfg(unix)]
#[test]
fn even_a_link_named_method_reader_counts_as_already_there() {
    let apps = tempfile::tempdir().unwrap();
    let old = apps.path().join("Screepub.app");
    make_bundle(&old, "Screepub", "0.13.0");
    std::os::unix::fs::symlink("/nowhere", apps.path().join("Method Reader.app")).unwrap();
    assert_eq!(install_destination(&old), old);
}

#[test]
fn a_method_reader_bundle_updates_in_place() {
    let apps = tempfile::tempdir().unwrap();
    let bundle = apps.path().join("Method Reader.app");
    make_bundle(&bundle, "method-reader-app", "0.13.1");
    assert_eq!(install_destination(&bundle), bundle);
    let installed = install_app(&new_archive("0.13.2"), &bundle, &no_admin).unwrap();
    assert_eq!(installed.bundle, bundle);
    assert_eq!(version_of(&bundle), "0.13.2");
    assert_eq!(names(apps.path()), ["Method Reader.app"]);
}

#[test]
fn a_bundle_someone_renamed_keeps_its_name() {
    let apps = tempfile::tempdir().unwrap();
    let bundle = apps.path().join("Scripts to Kindle.app");
    make_bundle(&bundle, "Screepub", "0.13.0");
    assert_eq!(install_destination(&bundle), bundle);
    let installed = install_app(&new_archive("0.13.1"), &bundle, &no_admin).unwrap();
    assert_eq!(installed.bundle, bundle);
    assert_eq!(names(apps.path()), ["Scripts to Kindle.app"]);
}

#[test]
fn a_refused_archive_leaves_the_old_bundle_and_nothing_beside_it() {
    let apps = tempfile::tempdir().unwrap();
    let old = apps.path().join("Screepub.app");
    make_bundle(&old, "Screepub", "0.13.0");
    let no_plist = tar_gz(&[
        ("Method Reader.app/", b"", 0o755),
        ("Method Reader.app/Contents/MacOS/x", b"x", 0o755),
    ]);
    let err = install_app(&no_plist, &old, &no_admin).unwrap_err();
    assert!(err.to_string().contains("Info.plist"), "{err}");
    assert_eq!(version_of(&old), "0.13.0");
    assert_eq!(names(apps.path()), ["Screepub.app"]);
}

#[cfg(unix)]
#[test]
fn an_old_bundle_that_cannot_be_deleted_is_left_over_beside_the_new_one() {
    use std::os::unix::fs::PermissionsExt;
    let apps = tempfile::tempdir().unwrap();
    let old = apps.path().join("Screepub.app");
    make_bundle(&old, "Screepub", "0.13.0");
    let locked = old.join("Contents/Resources/locked");
    std::fs::create_dir_all(&locked).unwrap();
    std::fs::write(locked.join("f"), "x").unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();

    let installed = install_app(&new_archive("0.13.1"), &old, &no_admin).unwrap();
    assert_eq!(installed.bundle, apps.path().join("Method Reader.app"));
    let leftover: std::path::PathBuf = installed.leftover.expect("the old bundle stays behind");
    // Hidden, so the startup sweep finds it and nothing launches it by name.
    assert_eq!(
        leftover.file_name().unwrap().to_string_lossy(),
        format!(".Screepub.app.old-{}", std::process::id())
    );
    assert!(!old.exists());
    std::fs::set_permissions(
        leftover.join("Contents/Resources/locked"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
}

/// The administrator's shell command, run as this account in a scratch
/// folder (the same /bin/sh `do shell script` uses).
#[cfg(unix)]
fn run_admin_shell(bundle: &Path, new: &Path, dest: &Path) -> bool {
    std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(sp_update::install::admin_swap_shell(bundle, new, dest))
        .status()
        .unwrap()
        .success()
}

#[cfg(unix)]
#[test]
fn the_administrator_swap_renames_too() {
    let apps = tempfile::tempdir().unwrap();
    let old = apps.path().join("Screepub.app");
    make_bundle(&old, "Screepub", "0.13.0");
    let staged = tempfile::tempdir().unwrap();
    let new = staged.path().join("Method Reader.app");
    make_bundle(&new, "method-reader-app", "0.13.1");
    let dest = install_destination(&old);
    assert!(run_admin_shell(&old, &new, &dest));
    assert_eq!(version_of(&dest), "0.13.1");
    assert_eq!(names(apps.path()), ["Method Reader.app"]);
}

#[cfg(unix)]
#[test]
fn a_failed_administrator_rename_puts_the_old_bundle_back() {
    let apps = tempfile::tempdir().unwrap();
    let old = apps.path().join("Screepub.app");
    make_bundle(&old, "Screepub", "0.13.0");
    let missing = apps.path().join("not-there.app");
    let dest = install_destination(&old);
    assert!(!run_admin_shell(&old, &missing, &dest));
    assert_eq!(version_of(&old), "0.13.0");
    assert_eq!(names(apps.path()), ["Screepub.app"]);
}

#[cfg(unix)]
#[test]
fn the_administrator_path_reports_the_renamed_bundle() {
    use std::os::unix::fs::PermissionsExt;
    // A folder this account cannot write: the administrator does the swap
    // (here this account, once the folder is writable again, running the
    // same shell command the real one would).
    let apps = tempfile::tempdir().unwrap();
    let old = apps.path().join("Screepub.app");
    make_bundle(&old, "Screepub", "0.13.0");
    std::fs::set_permissions(apps.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
    let dir = apps.path().to_path_buf();
    let admin = |bundle: &Path, new: &Path| -> Result<(), InstallError> {
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(run_admin_shell(bundle, new, &install_destination(bundle)));
        Ok(())
    };
    let result = install_app(&new_archive("0.13.1"), &old, &admin);
    std::fs::set_permissions(apps.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    let installed = result.unwrap();
    assert_eq!(installed.bundle, apps.path().join("Method Reader.app"));
    assert_eq!(version_of(&installed.bundle), "0.13.1");
    assert_eq!(names(apps.path()), ["Method Reader.app"]);
}
