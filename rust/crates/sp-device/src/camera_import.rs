//! BETA, with MTP Kindles (off until hardware day).
//!
//! macOS's camera import, paused for the length of one copy to an MTP
//! Kindle. Ported from branch mtp-kindle's src/device/camera-import.ts.
//!
//! macOS's camera-import daemon, ptpcamerad, claims every MTP and PTP device
//! the moment it connects and holds it exclusively, so no other program can
//! open a Kindle. The shipped precedent disables it in the person's own
//! launchd domain and kills it, which needs no password, and enables it
//! again afterwards. Method Reader does that for ONE copy, never longer:
//!
//!  - Darwin only. Anywhere else every function here does nothing.
//!  - If the daemon is already disabled, someone chose that: leave it
//!    alone, and do not enable it afterwards either.
//!  - A marker file is written BEFORE anything is disabled, and removed
//!    only once the daemon is enabled again. Every engine run starts by
//!    looking for it ([`recover_camera_import`]), so a crash mid-copy is
//!    repaired by the next run.
//!  - The marker names the run that wrote it (its pid). The device poll
//!    starts a new engine run every two seconds DURING a copy, and recovery
//!    in one of those must not turn camera import back on under the copy
//!    still running: it repairs only a marker whose run is gone, or one
//!    older than any copy can last ([`MARKER_STALE`]).
//!  - Restoring never fails the copy: a restore that failed keeps the
//!    marker so the next run tries again.
//!
//! Every launchctl and pkill call goes through a [`Runner`]. The tests use
//! fakes, and `METHOD_READER_LAUNCHCTL` / `METHOD_READER_PKILL` name the tools so a
//! test guard can point them where nothing real runs.

use crate::process::{Platform, Runner, SpawnRunner};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

/// The daemon's launchd label.
pub const PTPCAMERAD: &str = "com.apple.ptpcamerad";
/// Its process name, for pkill.
pub const PTPCAMERAD_PROCESS: &str = "ptpcamerad";

/// What the window shows for the whole of a copy to an MTP Kindle on a Mac
/// (the route device's `copyNote`). The engine writes the words; the window
/// only shows them.
pub const CAMERA_IMPORT_NOTE: &str = "While it copies, Photos and Image Capture cannot import from a camera or phone. Method Reader turns that back on when the copy ends.";

/// The marker's file name, in the app's config folder beside settings.json.
pub const MARKER_NAME: &str = "camera-import-paused";

/// How long one launchctl or pkill may take. Both answer at once; this only
/// stops a wedged one from holding the copy.
pub const STEP_TIMEOUT: Duration = Duration::from_secs(10);

/// A marker older than this is repaired even when its pid is alive: no copy
/// lasts longer (the copy's own ceiling is ten minutes), so a live pid that
/// old is another process that reused the number.
pub const MARKER_STALE: Duration = Duration::from_secs(15 * 60);

/// Where the marker lives: beside settings.json, in the folder sp-engine's
/// app-settings path names (so METHOD_READER_CONFIG_DIR, the test guard
/// included, moves it too). The settings path is sp-engine's to compute.
pub fn camera_import_marker(config_dir: &Path) -> PathBuf {
    config_dir.join(MARKER_NAME)
}

/// The launchd service target for the daemon in this person's domain.
pub fn service_target(uid: u32) -> String {
    format!("user/{uid}/{PTPCAMERAD}")
}

/// Whether `launchctl print-disabled user/<uid>` lists the daemon as
/// disabled. Both spellings launchctl has used are read: `=> disabled`
/// (macOS 13 and later) and `=> true` (older).
pub fn is_disabled(print_disabled: &str, label: &str) -> bool {
    for line in print_disabled.lines() {
        let line = line.trim_start();
        let Some(rest) = line.strip_prefix('"') else {
            continue;
        };
        let Some(close) = rest.find('"') else {
            continue;
        };
        if close == 0 {
            continue;
        }
        let name = &rest[..close];
        let after = rest[close + 1..].trim_start();
        let Some(value) = after.strip_prefix("=>") else {
            continue;
        };
        let value: String = value
            .trim_start()
            .chars()
            .take_while(|c| !c.is_whitespace())
            .collect();
        if value.is_empty() || name != label {
            continue;
        }
        return value == "disabled" || value == "true";
    }
    false
}

/// Everything the pause touches, injectable.
#[derive(Clone)]
pub struct CameraImportDeps {
    pub platform: Platform,
    /// The person's user id, for the `user/<uid>` launchd domain.
    pub uid: u32,
    pub run: Arc<dyn Runner>,
    pub marker: PathBuf,
    /// `METHOD_READER_LAUNCHCTL` / `METHOD_READER_PKILL` as this run sees them.
    pub launchctl: String,
    pub pkill: String,
    /// This run's pid, written into the marker.
    pub pid: u32,
    /// Whether a pid is a running process.
    pub is_alive: Arc<dyn Fn(u32) -> bool + Send + Sync>,
    /// Milliseconds since the epoch.
    pub now: Arc<dyn Fn() -> u64 + Send + Sync>,
}

impl CameraImportDeps {
    /// This machine, this run, the real tools (or the ones the environment
    /// names), and the marker in `config_dir`.
    pub fn host(config_dir: &Path) -> CameraImportDeps {
        let tool = |name: &str, fallback: &str| {
            std::env::var(name)
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| fallback.into())
        };
        CameraImportDeps {
            platform: Platform::host(),
            uid: current_uid(),
            run: Arc::new(SpawnRunner),
            marker: camera_import_marker(config_dir),
            launchctl: tool("METHOD_READER_LAUNCHCTL", "/bin/launchctl"),
            pkill: tool("METHOD_READER_PKILL", "/usr/bin/pkill"),
            pid: std::process::id(),
            is_alive: Arc::new(process_alive),
            now: Arc::new(now_ms),
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(unix)]
fn current_uid() -> u32 {
    rustix::process::getuid().as_raw()
}

#[cfg(not(unix))]
fn current_uid() -> u32 {
    0
}

/// kill(pid, 0) sends nothing and only asks whether the process exists:
/// ESRCH is gone, EPERM is alive (someone else's).
#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    let Ok(raw) = i32::try_from(pid) else {
        return false;
    };
    let Some(pid) = rustix::process::Pid::from_raw(raw) else {
        return false;
    };
    match rustix::process::test_kill_process(pid) {
        Ok(()) => true,
        Err(e) => e == rustix::io::Errno::PERM,
    }
}

#[cfg(not(unix))]
fn process_alive(_pid: u32) -> bool {
    false
}

/// One step, its failure an answer: a code, None for a timeout, or -1 when
/// the tool could not be started.
fn step(deps: &CameraImportDeps, argv: &[String]) -> (Option<i32>, String) {
    match deps.run.run(argv, STEP_TIMEOUT) {
        Ok(out) => (out.code, out.stdout),
        Err(_) => (Some(-1), String::new()),
    }
}

/// Turn camera import back on and remove the marker. True when it is on
/// again. On failure the marker stays for the next run.
fn enable(deps: &CameraImportDeps) -> bool {
    let (code, _) = step(
        deps,
        &[
            deps.launchctl.clone(),
            "enable".into(),
            service_target(deps.uid),
        ],
    );
    if code != Some(0) {
        return false;
    }
    // The daemon is on; a marker left behind only costs one more harmless
    // enable on the next run.
    let _ = std::fs::remove_file(&deps.marker);
    true
}

/// One pause. [`CameraImportPause::restore`] enables the daemon again if
/// this pause disabled it, once however often it is asked.
pub struct CameraImportPause {
    /// True when this pause disabled the daemon and restore() will enable it.
    pub paused: bool,
    /// True when the daemon was already disabled before Method Reader looked.
    pub already_off: bool,
    deps: Option<CameraImportDeps>,
    restored: OnceLock<bool>,
}

impl CameraImportPause {
    fn nothing(already_off: bool) -> CameraImportPause {
        CameraImportPause {
            paused: false,
            already_off,
            deps: None,
            restored: OnceLock::new(),
        }
    }

    /// Enable it again, if this pause disabled it. Runs once; every later
    /// call answers the first one's result.
    pub fn restore(&self) -> bool {
        *self.restored.get_or_init(|| match &self.deps {
            Some(deps) => enable(deps),
            None => true,
        })
    }
}

/// Pause camera import for one copy. On any platform but darwin, or when
/// it cannot be paused, the answer says so and restore() does nothing: the
/// copy goes ahead and, if the daemon still holds the Kindle, fails with the
/// helper's `busy`, which names it.
pub fn pause_camera_import(deps: &CameraImportDeps) -> CameraImportPause {
    if deps.platform != Platform::Darwin {
        return CameraImportPause::nothing(false);
    }
    let (code, listing) = step(
        deps,
        &[
            deps.launchctl.clone(),
            "print-disabled".into(),
            format!("user/{}", deps.uid),
        ],
    );
    if code == Some(0) && is_disabled(&listing, PTPCAMERAD) {
        return CameraImportPause::nothing(true);
    }

    // The marker first, so a crash at any later point leaves the evidence
    // the next run repairs from. No marker, no pause.
    let marker = format!(
        "{{\"uid\":{},\"pid\":{},\"at\":{}}}\n",
        deps.uid,
        deps.pid,
        (deps.now)()
    );
    let written = deps
        .marker
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .map_err(|_| ())
        .and_then(|()| sp_fsutil::write_file_atomic(&deps.marker, marker).map_err(|_| ()));
    if written.is_err() {
        return CameraImportPause::nothing(false);
    }

    let (disabled, _) = step(
        deps,
        &[
            deps.launchctl.clone(),
            "disable".into(),
            service_target(deps.uid),
        ],
    );
    if disabled != Some(0) {
        // Nothing changed, but enable anyway before dropping the marker: a
        // disable that timed out may still have landed.
        enable(deps);
        return CameraImportPause::nothing(false);
    }
    // Exit 1 is "no such process", which is fine: nothing held the Kindle.
    step(
        deps,
        &[
            deps.pkill.clone(),
            "-9".into(),
            "-x".into(),
            PTPCAMERAD_PROCESS.into(),
        ],
    );

    CameraImportPause {
        paused: true,
        already_off: false,
        deps: Some(deps.clone()),
        restored: OnceLock::new(),
    }
}

/// Whether a marker belongs to a copy that is still running: its pid is
/// alive, is not this run, and the marker is younger than any copy can be.
/// A marker that cannot be read is nobody's, and is repaired.
pub fn owned_by_live_run(
    text: &str,
    self_pid: u32,
    is_alive: &dyn Fn(u32) -> bool,
    now: u64,
) -> bool {
    let Ok(serde_json::Value::Object(marker)) = serde_json::from_str::<serde_json::Value>(text)
    else {
        return false;
    };
    // A JSON number that is a positive whole number, as Number.isInteger.
    let Some(pid) = marker.get("pid").and_then(serde_json::Value::as_f64) else {
        return false;
    };
    if pid.fract() != 0.0 || pid <= 0.0 || pid > u32::MAX as f64 {
        return false;
    }
    let pid = pid as u32;
    if pid == self_pid {
        return false;
    }
    let Some(at) = marker.get("at").and_then(serde_json::Value::as_f64) else {
        return false;
    };
    if now as f64 - at > MARKER_STALE.as_millis() as f64 {
        return false;
    }
    is_alive(pid)
}

/// At the start of every engine run: if a marker says an earlier run paused
/// camera import and never turned it back on, turn it back on. A marker
/// whose run is still copying is left alone. Darwin only, one look at the
/// disk when there is nothing to do, and never fails. True when it restored
/// something.
pub fn recover_camera_import(deps: &CameraImportDeps) -> bool {
    if deps.platform != Platform::Darwin {
        return false;
    }
    if !deps.marker.exists() {
        return false;
    }
    // There, but unreadable: nobody's, so repaired.
    let text = std::fs::read_to_string(&deps.marker).unwrap_or_default();
    if owned_by_live_run(&text, deps.pid, &*deps.is_alive, (deps.now)()) {
        return false;
    }
    enable(deps)
}

/// Restores on drop, so a copy that panics still turns camera import back on.
struct RestoreOnDrop<'a>(&'a CameraImportPause);

impl Drop for RestoreOnDrop<'_> {
    fn drop(&mut self) {
        self.0.restore();
    }
}

/// Pause, run `copy`, and restore afterwards, whatever `copy` did.
pub fn with_camera_import_paused<T>(
    deps: &CameraImportDeps,
    copy: impl FnOnce(&CameraImportPause) -> T,
) -> T {
    let pause = pause_camera_import(deps);
    let _restore = RestoreOnDrop(&pause);
    copy(&pause)
}
