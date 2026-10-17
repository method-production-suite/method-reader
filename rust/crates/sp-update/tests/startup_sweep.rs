//! The window's startup sweep (review item O5): a bundle swap that died
//! between its two renames leaves no Screepub.app, only the hidden
//! `.Screepub.app.old-<pid>`; the next start puts it back. A swap that
//! finished but could not delete its backup leaves a stale one; the next
//! start removes it.

mod common;

use common::make_bundle;
use sp_update::install::{Swept, sweep_at_startup, sweep_beside};
use std::path::Path;
use std::time::{Duration, SystemTime};

const ME: u32 = 4_000_000_000;

/// Make the folder at `path` look `by` old. Windows opens a folder only
/// when asked for one (backup semantics), with the right to change its
/// times.
fn age(path: &Path, by: Duration) {
    let mut open = std::fs::OpenOptions::new();
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_WRITE_ATTRIBUTES: u32 = 0x0100;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        open.access_mode(FILE_WRITE_ATTRIBUTES)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS);
    }
    #[cfg(not(windows))]
    open.read(true);
    let f = open.open(path).unwrap();
    f.set_modified(SystemTime::now() - by).unwrap();
}

fn version_of(bundle: &Path) -> String {
    std::fs::read_to_string(bundle.join("Contents/Resources/LICENSE")).unwrap()
}

#[test]
fn a_missing_bundle_is_put_back_from_its_hidden_backup() {
    let apps = tempfile::tempdir().unwrap();
    let backup = apps.path().join(".Screepub.app.old-123");
    make_bundle(&backup, "screepub-desktop", "0.12.0");
    let staging = apps.path().join(".Screepub.app.update-123");
    std::fs::create_dir_all(staging.join("Contents")).unwrap();
    age(&staging, Duration::from_secs(3600));

    let swept = sweep_beside(apps.path(), "Screepub.app", ME);
    let bundle = apps.path().join("Screepub.app");
    assert_eq!(swept.restored.as_deref(), Some(bundle.as_path()));
    assert_eq!(version_of(&bundle), "0.12.0");
    assert!(!backup.exists());
    // The half-made staging folder goes too, once the bundle is back.
    assert_eq!(swept.removed, std::slice::from_ref(&staging));
    assert!(!staging.exists());
}

#[test]
fn the_newest_runnable_backup_is_the_one_put_back() {
    let apps = tempfile::tempdir().unwrap();
    let older = apps.path().join(".Screepub.app.old-1");
    make_bundle(&older, "screepub-desktop", "0.11.0");
    age(&older, Duration::from_secs(7200));
    let newer = apps.path().join(".Screepub.app.old-2");
    make_bundle(&newer, "screepub-desktop", "0.12.0");
    age(&newer, Duration::from_secs(3600));
    // Newest of all, but not a bundle anything could run.
    let broken = apps.path().join(".Screepub.app.old-3");
    std::fs::create_dir_all(broken.join("Contents")).unwrap();

    let swept = sweep_beside(apps.path(), "Screepub.app", ME);
    assert!(swept.restored.is_some());
    assert_eq!(version_of(&apps.path().join("Screepub.app")), "0.12.0");
    // The older backup is stale now; the broken one is too new to judge.
    assert_eq!(swept.removed, [older]);
    assert!(broken.exists());
}

#[test]
fn a_stale_backup_beside_a_bundle_is_removed_and_a_fresh_one_kept() {
    let apps = tempfile::tempdir().unwrap();
    let bundle = apps.path().join("Screepub.app");
    make_bundle(&bundle, "screepub-desktop", "0.12.0");
    let stale = apps.path().join(".Screepub.app.old-77");
    make_bundle(&stale, "screepub-desktop", "0.11.1");
    age(&stale, Duration::from_secs(3600));
    // Minutes old: maybe another copy's swap in progress.
    let fresh = apps.path().join(".Screepub.app.update-78");
    std::fs::create_dir_all(&fresh).unwrap();

    let swept = sweep_beside(apps.path(), "Screepub.app", ME);
    assert_eq!(
        swept,
        Swept {
            restored: None,
            removed: vec![stale.clone()]
        }
    );
    assert!(!stale.exists());
    assert!(fresh.exists());
    assert_eq!(version_of(&bundle), "0.12.0", "the bundle is never touched");
}

#[test]
fn this_processes_own_swap_and_anything_else_are_left_alone() {
    let apps = tempfile::tempdir().unwrap();
    make_bundle(
        &apps.path().join("Screepub.app"),
        "screepub-desktop",
        "0.12.0",
    );
    let mine = apps.path().join(format!(".Screepub.app.old-{ME}"));
    std::fs::create_dir_all(&mine).unwrap();
    let other_app = apps.path().join(".Other.app.old-5");
    std::fs::create_dir_all(&other_app).unwrap();
    let not_a_pid = apps.path().join(".Screepub.app.old-x");
    std::fs::create_dir_all(&not_a_pid).unwrap();
    let a_file = apps.path().join(".Screepub.app.old-6");
    std::fs::write(&a_file, "not a folder").unwrap();
    for p in [&mine, &other_app, &not_a_pid] {
        age(p, Duration::from_secs(3600));
    }
    let swept = sweep_beside(apps.path(), "Screepub.app", ME);
    assert_eq!(swept, Swept::default());
    for p in [&mine, &other_app, &not_a_pid, &a_file] {
        assert!(p.exists(), "{}", p.display());
    }
}

#[cfg(unix)]
#[test]
fn a_link_named_like_a_backup_is_never_followed_or_restored() {
    let apps = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    make_bundle(
        &elsewhere.path().join("Evil.app"),
        "screepub-desktop",
        "6.6.6",
    );
    std::os::unix::fs::symlink(
        elsewhere.path().join("Evil.app"),
        apps.path().join(".Screepub.app.old-9"),
    )
    .unwrap();
    let swept = sweep_beside(apps.path(), "Screepub.app", ME);
    assert_eq!(swept, Swept::default());
    assert!(!apps.path().join("Screepub.app").exists());
    assert!(elsewhere.path().join("Evil.app").exists());
}

#[test]
fn no_bundle_and_no_usable_backup_touches_nothing() {
    let apps = tempfile::tempdir().unwrap();
    let staging = apps.path().join(".Screepub.app.update-12");
    std::fs::create_dir_all(&staging).unwrap();
    age(&staging, Duration::from_secs(3600));
    assert_eq!(
        sweep_beside(apps.path(), "Screepub.app", ME),
        Swept::default()
    );
    assert!(staging.exists(), "nothing to fall back on: evidence kept");
}

#[test]
fn started_from_the_hidden_backup_the_window_puts_itself_back() {
    let apps = tempfile::tempdir().unwrap();
    let backup = apps.path().join(".Screepub.app.old-123");
    make_bundle(&backup, "screepub-desktop", "0.12.0");
    let exe = backup.join("Contents/MacOS/screepub-desktop");
    let swept = sweep_at_startup(&exe);
    assert_eq!(
        swept.restored.as_deref(),
        Some(apps.path().join("Screepub.app").as_path())
    );
    assert!(!backup.exists());
}

#[test]
fn started_from_the_bundle_the_window_clears_its_stale_backups() {
    let apps = tempfile::tempdir().unwrap();
    let bundle = apps.path().join("Screepub.app");
    make_bundle(&bundle, "screepub-desktop", "0.12.0");
    let stale = apps.path().join(".Screepub.app.old-123");
    std::fs::create_dir_all(&stale).unwrap();
    age(&stale, Duration::from_secs(3600));
    let swept = sweep_at_startup(&bundle.join("Contents/MacOS/screepub-desktop"));
    assert_eq!(swept.removed, [stale]);
}

#[test]
fn outside_a_bundle_the_sweep_does_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let stale = dir.path().join(".Screepub.app.old-123");
    std::fs::create_dir_all(&stale).unwrap();
    age(&stale, Duration::from_secs(3600));
    for exe in [
        dir.path().join("method-reader-app"),
        dir.path().join("target/release/method-reader-app"),
        dir.path()
            .join(".Screepub.app.update-5/Contents/MacOS/screepub-desktop"),
    ] {
        assert_eq!(
            sweep_at_startup(&exe),
            Swept::default(),
            "{}",
            exe.display()
        );
    }
    assert!(stale.exists());
}

// ---- the one-time rename: leftovers under either name ----
//
// A renaming swap moves `Screepub.app` aside to `.Screepub.app.old-<pid>`
// and its staging `.Method Reader.app.update-<pid>` into
// `Method Reader.app`. A sweep beside either name looks after both.

#[test]
fn a_renaming_swap_that_died_between_its_renames_puts_the_old_bundle_back() {
    let apps = tempfile::tempdir().unwrap();
    let backup = apps.path().join(".Screepub.app.old-123");
    make_bundle(&backup, "Screepub", "0.13.0");
    age(&backup, Duration::from_secs(3600));
    let staging = apps.path().join(".Method Reader.app.update-123");
    std::fs::create_dir_all(staging.join("Contents")).unwrap();
    age(&staging, Duration::from_secs(3600));

    // Launch Services follows the moved bundle: the Dock starts the backup.
    let swept = sweep_at_startup(&backup.join("Contents/MacOS/Screepub"));
    let bundle = apps.path().join("Screepub.app");
    assert_eq!(swept.restored.as_deref(), Some(bundle.as_path()));
    assert_eq!(version_of(&bundle), "0.13.0");
    assert_eq!(swept.removed, std::slice::from_ref(&staging));
    assert!(!apps.path().join("Method Reader.app").exists());
}

#[test]
fn the_new_bundle_clears_the_old_name_s_stale_backup() {
    let apps = tempfile::tempdir().unwrap();
    let bundle = apps.path().join("Method Reader.app");
    make_bundle(&bundle, "method-reader-app", "0.13.1");
    let stale = apps.path().join(".Screepub.app.old-77");
    make_bundle(&stale, "Screepub", "0.13.0");
    age(&stale, Duration::from_secs(3600));
    let fresh = apps.path().join(".Screepub.app.old-78");
    std::fs::create_dir_all(&fresh).unwrap();

    let swept = sweep_at_startup(&bundle.join("Contents/MacOS/method-reader-app"));
    assert_eq!(
        swept,
        Swept {
            restored: None,
            removed: vec![stale.clone()]
        }
    );
    assert!(fresh.exists(), "minutes old: maybe a swap in progress");
    assert!(!apps.path().join("Screepub.app").exists());
}

#[test]
fn started_from_the_old_backup_beside_the_new_bundle_nothing_is_put_back() {
    // The rename finished but could not delete the backup, and the Dock
    // still points at the moved old bundle. Putting it back would leave two
    // copies; the new bundle is already where it belongs.
    let apps = tempfile::tempdir().unwrap();
    make_bundle(
        &apps.path().join("Method Reader.app"),
        "method-reader-app",
        "0.13.1",
    );
    let backup = apps.path().join(".Screepub.app.old-123");
    make_bundle(&backup, "Screepub", "0.13.0");
    let swept = sweep_at_startup(&backup.join("Contents/MacOS/Screepub"));
    assert_eq!(swept.restored, None);
    assert!(!apps.path().join("Screepub.app").exists());
}

#[test]
fn the_old_bundle_clears_a_renaming_swap_s_stale_staging() {
    let apps = tempfile::tempdir().unwrap();
    let bundle = apps.path().join("Screepub.app");
    make_bundle(&bundle, "Screepub", "0.13.0");
    let staging = apps.path().join(".Method Reader.app.update-5");
    std::fs::create_dir_all(staging.join("Contents")).unwrap();
    age(&staging, Duration::from_secs(3600));
    let swept = sweep_beside(apps.path(), "Screepub.app", ME);
    assert_eq!(swept.removed, [staging]);
    assert_eq!(swept.restored, None);
}

#[test]
fn a_missing_new_bundle_is_put_back_from_its_own_backup() {
    let apps = tempfile::tempdir().unwrap();
    let backup = apps.path().join(".Method Reader.app.old-9");
    make_bundle(&backup, "method-reader-app", "0.13.1");
    let swept = sweep_beside(apps.path(), "Method Reader.app", ME);
    let bundle = apps.path().join("Method Reader.app");
    assert_eq!(swept.restored.as_deref(), Some(bundle.as_path()));
    assert!(!apps.path().join("Screepub.app").exists());
}
