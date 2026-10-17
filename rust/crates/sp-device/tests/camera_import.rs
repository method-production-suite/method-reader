//! Branch mtp-kindle's tests/camera-import.test.ts, ported (BETA, off). The
//! macOS camera-import pause, driven only by a fake launchctl and pkill:
//! the runner is injected in every call, so nothing here can reach the real
//! ptpcamerad.
//!
//! The TypeScript's "the marker sits under the /dev/null config guard"
//! check belongs to sp-engine, which owns the settings path the marker
//! lives beside (camera_import_marker takes that folder).

mod common;
use sp_device::Platform;
use sp_device::camera_import::{
    CAMERA_IMPORT_NOTE, CameraImportDeps, MARKER_STALE, PTPCAMERAD, camera_import_marker,
    is_disabled, owned_by_live_run, pause_camera_import, recover_camera_import,
    with_camera_import_paused,
};
use sp_device::process::{RunOutput, Runner};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A fake launchctl and pkill. `codes` maps a verb (argv[1], or "pkill") to
/// an exit code; print-disabled prints `listing`. Every call is logged with
/// whether the marker existed at that moment.
struct FakeTools {
    marker: PathBuf,
    listing: String,
    codes: HashMap<String, Option<i32>>,
    fail_on: Option<String>,
    calls: Mutex<Vec<(Vec<String>, bool)>>,
}

impl Runner for FakeTools {
    fn run(&self, argv: &[String], _timeout: Duration) -> Result<RunOutput, String> {
        self.calls
            .lock()
            .unwrap()
            .push((argv.to_vec(), self.marker.exists()));
        let verb = if argv[0].ends_with("pkill") {
            "pkill".to_string()
        } else {
            argv[1].clone()
        };
        if self.fail_on.as_deref() == Some(verb.as_str()) {
            return Err("ENOENT".into());
        }
        let code = self.codes.get(&verb).copied().unwrap_or(Some(0));
        let stdout = if verb == "print-disabled" {
            self.listing.clone()
        } else {
            String::new()
        };
        Ok(RunOutput {
            code,
            stdout,
            stderr: String::new(),
        })
    }
}

impl FakeTools {
    fn argvs(&self) -> Vec<Vec<String>> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .map(|(a, _)| a.clone())
            .collect()
    }
    fn verbs(&self) -> Vec<String> {
        self.argvs()
            .iter()
            .map(|a| a.get(1).cloned().unwrap_or_else(|| a[0].clone()))
            .collect()
    }
}

fn tools(marker: &Path) -> FakeTools {
    FakeTools {
        marker: marker.to_path_buf(),
        listing: String::new(),
        codes: HashMap::new(),
        fail_on: None,
        calls: Mutex::new(Vec::new()),
    }
}

fn deps(platform: Platform, marker: &Path, run: Arc<FakeTools>) -> CameraImportDeps {
    CameraImportDeps {
        platform,
        uid: 501,
        run,
        marker: marker.to_path_buf(),
        launchctl: "/fake/launchctl".into(),
        pkill: "/fake/pkill".into(),
        pid: std::process::id(),
        is_alive: Arc::new(|_| false),
        now: Arc::new(|| 1_000_000),
    }
}

struct Scratch {
    dir: tempfile::TempDir,
    n: Mutex<u32>,
}

impl Scratch {
    fn new() -> Scratch {
        Scratch {
            dir: common::scratch("method-reader-camera-"),
            n: Mutex::new(0),
        }
    }
    fn marker(&self) -> PathBuf {
        let mut n = self.n.lock().unwrap();
        *n += 1;
        camera_import_marker(&self.dir.path().join(format!("config-{n}")))
    }
}

const ENABLED_LISTING: &str = "disabled services = {\n\t\"com.apple.ScreenReaderUIServer\" => disabled\n\t\"com.apple.ptpcamerad\" => enabled\n}";

fn s(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|p| p.to_string()).collect()
}

// isDisabled

#[test]
fn is_disabled_reads_both_spellings_launchctl_has_used() {
    assert!(is_disabled(
        "\t\"com.apple.ptpcamerad\" => disabled",
        PTPCAMERAD
    ));
    assert!(is_disabled(
        "\t\"com.apple.ptpcamerad\" => true",
        PTPCAMERAD
    ));
    assert!(!is_disabled(
        "\t\"com.apple.ptpcamerad\" => enabled",
        PTPCAMERAD
    ));
    assert!(!is_disabled(
        "\t\"com.apple.ptpcamerad\" => false",
        PTPCAMERAD
    ));
}

#[test]
fn another_service_being_disabled_says_nothing_about_this_one() {
    assert!(!is_disabled(ENABLED_LISTING, PTPCAMERAD));
    assert!(!is_disabled(
        "\t\"com.apple.ptpcamerad.extra\" => disabled",
        PTPCAMERAD
    ));
    assert!(!is_disabled("", PTPCAMERAD));
}

// pauseCameraImport

#[test]
fn off_a_mac_it_does_nothing_at_all() {
    let sc = Scratch::new();
    for platform in [Platform::Linux, Platform::Win32] {
        let marker = sc.marker();
        let t = Arc::new(tools(&marker));
        let pause = pause_camera_import(&deps(platform, &marker, t.clone()));
        assert!(!pause.paused);
        assert!(pause.restore());
        assert!(t.argvs().is_empty());
        assert!(!marker.exists());
    }
}

#[test]
fn on_a_mac_marker_first_then_disable_and_kill_in_the_person_s_own_domain() {
    let sc = Scratch::new();
    let marker = sc.marker();
    let t = Arc::new(FakeTools {
        listing: ENABLED_LISTING.into(),
        ..tools(&marker)
    });
    let pause = pause_camera_import(&deps(Platform::Darwin, &marker, t.clone()));
    assert!(pause.paused);
    assert_eq!(
        t.argvs(),
        [
            s(&["/fake/launchctl", "print-disabled", "user/501"]),
            s(&[
                "/fake/launchctl",
                "disable",
                "user/501/com.apple.ptpcamerad"
            ]),
            s(&["/fake/pkill", "-9", "-x", "ptpcamerad"]),
        ]
    );
    // The marker was there before the disable ran: a crash after it is
    // always repairable.
    let calls = t.calls.lock().unwrap().clone();
    assert!(!calls[0].1);
    assert!(calls[1].1);
    assert!(marker.exists());

    assert!(pause.restore());
    assert_eq!(
        t.argvs().last().unwrap(),
        &s(&["/fake/launchctl", "enable", "user/501/com.apple.ptpcamerad"])
    );
    assert!(!marker.exists());
}

#[test]
fn the_marker_is_the_typescript_s_json() {
    let sc = Scratch::new();
    let marker = sc.marker();
    let t = Arc::new(tools(&marker));
    let mut d = deps(Platform::Darwin, &marker, t);
    d.pid = 4242;
    let _pause = pause_camera_import(&d);
    assert_eq!(
        std::fs::read_to_string(&marker).unwrap(),
        "{\"uid\":501,\"pid\":4242,\"at\":1000000}\n"
    );
}

#[test]
fn restore_runs_once_however_often_it_is_asked() {
    let sc = Scratch::new();
    let marker = sc.marker();
    let t = Arc::new(tools(&marker));
    let pause = pause_camera_import(&deps(Platform::Darwin, &marker, t.clone()));
    std::thread::scope(|scope| {
        scope.spawn(|| pause.restore());
        scope.spawn(|| pause.restore());
    });
    pause.restore();
    assert_eq!(t.verbs().iter().filter(|v| *v == "enable").count(), 1);
}

#[test]
fn pkill_finding_nothing_to_kill_is_fine() {
    let sc = Scratch::new();
    let marker = sc.marker();
    let t = Arc::new(FakeTools {
        codes: HashMap::from([("pkill".into(), Some(1))]),
        ..tools(&marker)
    });
    let pause = pause_camera_import(&deps(Platform::Darwin, &marker, t));
    assert!(pause.paused);
    pause.restore();
}

#[test]
fn already_disabled_by_the_person_left_alone_and_never_enabled_after() {
    let sc = Scratch::new();
    let marker = sc.marker();
    let t = Arc::new(FakeTools {
        listing: "\t\"com.apple.ptpcamerad\" => disabled".into(),
        ..tools(&marker)
    });
    let pause = pause_camera_import(&deps(Platform::Darwin, &marker, t.clone()));
    assert!(!pause.paused);
    assert!(pause.already_off);
    pause.restore();
    assert_eq!(t.verbs(), ["print-disabled"]);
    assert!(!marker.exists());
}

#[test]
fn a_disable_that_fails_changes_nothing_and_leaves_no_marker() {
    let sc = Scratch::new();
    let marker = sc.marker();
    let t = Arc::new(FakeTools {
        codes: HashMap::from([("disable".into(), Some(1))]),
        ..tools(&marker)
    });
    let pause = pause_camera_import(&deps(Platform::Darwin, &marker, t.clone()));
    assert!(!pause.paused);
    assert_eq!(t.verbs(), ["print-disabled", "disable", "enable"]);
    assert!(!marker.exists());
}

#[test]
fn no_marker_can_be_written_no_pause() {
    let sc = Scratch::new();
    let blocker = sc.dir.path().join("a-file");
    std::fs::write(&blocker, "x").unwrap();
    let marker = blocker.join("camera-import-paused");
    let t = Arc::new(tools(&marker));
    let pause = pause_camera_import(&deps(Platform::Darwin, &marker, t.clone()));
    assert!(!pause.paused);
    assert_eq!(t.verbs(), ["print-disabled"]);
}

#[test]
fn a_restore_that_fails_keeps_the_marker_for_the_next_run() {
    let sc = Scratch::new();
    let marker = sc.marker();
    let t = Arc::new(FakeTools {
        codes: HashMap::from([("enable".into(), Some(5))]),
        ..tools(&marker)
    });
    let pause = pause_camera_import(&deps(Platform::Darwin, &marker, t));
    assert!(!pause.restore());
    assert!(marker.exists());
}

#[test]
fn a_launchctl_that_cannot_start_is_a_failure_to_pause_not_a_crash() {
    let sc = Scratch::new();
    let marker = sc.marker();
    let t = Arc::new(FakeTools {
        fail_on: Some("disable".into()),
        ..tools(&marker)
    });
    let pause = pause_camera_import(&deps(Platform::Darwin, &marker, t));
    assert!(!pause.paused);
}

// withCameraImportPaused

#[test]
fn restores_after_a_copy_that_worked_and_after_one_that_panicked() {
    let sc = Scratch::new();
    let marker = sc.marker();
    let t = Arc::new(tools(&marker));
    let d = deps(Platform::Darwin, &marker, t.clone());
    let answer = with_camera_import_paused(&d, |pause| {
        assert!(pause.paused);
        assert!(marker.exists());
        "copied"
    });
    assert_eq!(answer, "copied");
    assert!(!marker.exists());

    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_camera_import_paused(&d, |_| -> () { panic!("the cable fell out") })
    }));
    assert!(failed.is_err());
    assert_eq!(t.verbs().iter().filter(|v| *v == "enable").count(), 2);
    assert!(!marker.exists());
}

// recoverCameraImport

#[test]
fn a_marker_left_by_a_crash_is_repaired_and_removed() {
    let sc = Scratch::new();
    let marker = sc.marker();
    let t = Arc::new(tools(&marker));
    // A run that paused and then died before restoring.
    let mut crashed = deps(Platform::Darwin, &marker, t);
    crashed.pid = 4242;
    std::mem::forget(pause_camera_import(&crashed));
    assert!(marker.exists());
    let later = Arc::new(tools(&marker));
    let mut d = deps(Platform::Darwin, &marker, later.clone());
    d.pid = 5000;
    assert!(recover_camera_import(&d));
    assert_eq!(
        later.argvs(),
        [s(&[
            "/fake/launchctl",
            "enable",
            "user/501/com.apple.ptpcamerad"
        ])]
    );
    assert!(!marker.exists());
}

#[test]
fn a_copy_still_running_keeps_its_pause_the_device_poll_s_runs_leave_it_alone() {
    let sc = Scratch::new();
    let marker = sc.marker();
    let now = Arc::new(AtomicU64::new(1_000_000));
    let clock = {
        let now = Arc::clone(&now);
        Arc::new(move || now.load(Ordering::SeqCst))
    };
    let t = Arc::new(tools(&marker));
    let mut copying = deps(Platform::Darwin, &marker, t);
    copying.pid = 4242;
    copying.now = clock.clone();
    std::mem::forget(pause_camera_import(&copying));

    let poll = Arc::new(tools(&marker));
    let mut d = deps(Platform::Darwin, &marker, poll.clone());
    d.pid = 5000;
    d.now = clock;
    d.is_alive = Arc::new(|pid| pid == 4242);
    assert!(!recover_camera_import(&d));
    assert!(poll.argvs().is_empty());
    assert!(marker.exists());
    // Older than any copy can last: the pid was reused, so it is repaired.
    now.fetch_add(MARKER_STALE.as_millis() as u64 + 1, Ordering::SeqCst);
    assert!(recover_camera_import(&d));
    assert!(!marker.exists());
}

#[test]
fn owned_by_live_run_only_a_well_formed_young_marker_of_another_live_pid() {
    let alive = |_: u32| true;
    let at = 1_000u64;
    let ok = r#"{"uid":501,"pid":77,"at":1000}"#;
    assert!(owned_by_live_run(ok, 1, &alive, at + 10));
    assert!(!owned_by_live_run(ok, 1, &|_| false, at + 10));
    assert!(!owned_by_live_run(ok, 77, &alive, at + 10)); // this very run
    assert!(!owned_by_live_run(
        ok,
        1,
        &alive,
        at + MARKER_STALE.as_millis() as u64 + 1
    ));
    for bad in [
        "",
        "not json",
        "null",
        r#"{"uid":501}"#,
        r#"{"pid":"77","at":1000}"#,
        r#"{"pid":77}"#,
        r#"{"pid":-1,"at":1000}"#,
    ] {
        assert!(!owned_by_live_run(bad, 1, &alive, at + 10), "{bad}");
    }
}

#[test]
fn no_marker_nothing_run_the_engine_starts_every_two_seconds() {
    let sc = Scratch::new();
    let marker = sc.marker();
    let t = Arc::new(tools(&marker));
    assert!(!recover_camera_import(&deps(
        Platform::Darwin,
        &marker,
        t.clone()
    )));
    assert!(t.argvs().is_empty());
}

#[test]
fn off_a_mac_a_marker_is_never_acted_on() {
    let sc = Scratch::new();
    let marker = sc.marker();
    let t = Arc::new(tools(&marker));
    std::mem::forget(pause_camera_import(&deps(Platform::Darwin, &marker, t)));
    let later = Arc::new(tools(&marker));
    assert!(!recover_camera_import(&deps(
        Platform::Linux,
        &marker,
        later.clone()
    )));
    assert!(later.argvs().is_empty());
}

#[test]
fn the_words_the_window_shows_while_a_copy_pauses_camera_import() {
    assert!(CAMERA_IMPORT_NOTE.contains("Photos and Image Capture"));
    assert!(CAMERA_IMPORT_NOTE.contains("turns that back on"));
    // Sam's rule for copy Claude writes: no em dashes.
    assert!(!CAMERA_IMPORT_NOTE.contains('\u{2014}'));
    assert_eq!(
        CAMERA_IMPORT_NOTE,
        "While it copies, Photos and Image Capture cannot import from a camera or phone. Method Reader turns that back on when the copy ends."
    );
}

#[test]
fn the_marker_sits_beside_the_settings_in_the_config_folder() {
    assert_eq!(
        camera_import_marker(Path::new("/cfg")),
        Path::new("/cfg").join("camera-import-paused")
    );
}
