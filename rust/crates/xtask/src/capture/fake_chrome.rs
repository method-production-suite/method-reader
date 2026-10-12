//! A stand-in for Chrome, for tests/capture_chrome.rs (was
//! tests/fixtures/fake-chrome/chrome.sh). It does the one part of Chrome's
//! start-up that the capture's Chrome driver depends on: writes
//! DevToolsActivePort into the --user-data-dir it was given, naming the
//! port of the test's fake DevTools server, then waits to be stopped.
//!
//! The test copies this binary into a temp folder and puts two things
//! beside it: `port`, holding the fake server's port, and, for a Chrome
//! that cannot start, `die`, which makes it exit 3 at once. Either way it
//! first writes `profile` there, the --user-data-dir it was given, so the
//! test can check that exact folder is gone afterwards.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let here = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_default();
    let profile = std::env::args()
        .find_map(|a| a.strip_prefix("--user-data-dir=").map(PathBuf::from))
        .unwrap_or_default();
    let _ = std::fs::write(here.join("profile"), format!("{}\n", profile.display()));
    if here.join("die").exists() {
        return ExitCode::from(3);
    }
    let port = std::fs::read_to_string(here.join("port")).unwrap_or_default();
    let _ = std::fs::write(
        profile.join("DevToolsActivePort"),
        format!("{}\n/devtools/browser/fake\n", port.trim()),
    );
    std::thread::sleep(std::time::Duration::from_secs(60));
    ExitCode::SUCCESS
}
