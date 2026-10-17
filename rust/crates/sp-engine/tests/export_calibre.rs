//! Port of tests/export-calibre.test.ts: finding and running Calibre's
//! tools. Every spawn here is a stand-in script, never Calibre itself. The
//! two checks that run REAL Calibre on the engine's own EPUB (toKepub's
//! koboSpan, and the dialogue column surviving an exploded AZW3) are
//! export_calibre_real.rs.

#![cfg_attr(not(unix), allow(dead_code, unused_imports))]

mod common;

use common::{Scratch, guarded_env};
use sp_engine::export::calibre::{
    CALIBRE_FORMAT_GUARDS, CONVERSION_TIMEOUT_MS, calibre_tool, is_calibre_available,
    is_calibre_failed, run_calibre, to_kepub,
};

#[test]
fn the_format_guards_are_exactly_the_device_verified_trio() {
    assert_eq!(
        CALIBRE_FORMAT_GUARDS,
        [
            "--page-breaks-before=/",
            "--chapter-mark=none",
            "--disable-remove-fake-margins"
        ]
    );
}

#[test]
fn is_available_agrees_with_the_tool_finder() {
    let env = guarded_env();
    match calibre_tool("ebook-convert", &env) {
        Some(tool) => {
            assert!(is_calibre_available(&env));
            assert!(std::path::Path::new(&tool).exists());
        }
        None => assert!(!is_calibre_available(&env)),
    }
}

#[test]
fn an_unknown_tool_name_is_not_discovered() {
    assert_eq!(
        calibre_tool("definitely-not-a-calibre-tool", &guarded_env()),
        None
    );
}

/// darwin's candidates are fixed paths and never PATH; Linux scans PATH.
#[cfg(all(unix, not(target_os = "macos")))]
mod path_scan {
    use super::*;

    fn on_path(dir: &str) -> sp_engine::env::Env {
        let path = guarded_env().get("PATH").unwrap_or("").to_owned();
        guarded_env().with("PATH", &format!("{dir}:{path}"))
    }

    #[test]
    fn a_tool_on_path_is_discovered() {
        let s = Scratch::new("export-calibre");
        let fake = s.join("method-reader-fake-calibre-tool");
        common::executable(&fake, "#!/bin/sh\nexit 0\n");
        assert_eq!(
            calibre_tool("method-reader-fake-calibre-tool", &on_path(&s.str())),
            Some(fake)
        );
    }

    #[test]
    fn a_same_named_file_that_is_not_executable_is_refused() {
        let s = Scratch::new("export-calibre");
        let fake = s.join("method-reader-fake-calibre-tool2");
        std::fs::write(&fake, "#!/bin/sh\nexit 0\n").unwrap();
        common::chmod(&fake, 0o644);
        assert_eq!(
            calibre_tool("method-reader-fake-calibre-tool2", &on_path(&s.str())),
            None
        );
    }

    #[test]
    fn to_kepub_converts_to_kepub_then_renames() {
        if calibre_tool("ebook-convert", &guarded_env()).is_some() {
            return; // never shadow a real install
        }
        let tools = Scratch::new("calibre-fake");
        let fake = tools.join("ebook-convert");
        let log = tools.join("argv.log");
        common::executable(
            &fake,
            format!("#!/bin/sh\nfor a in \"$@\"; do echo \"$a\"; done > \"{log}\"\ntouch \"$2\"\n"),
        );
        let work = Scratch::new("calibre-work");
        let epub = work.join("book.epub");
        std::fs::write(&epub, "fake epub bytes").unwrap();
        let raw = work.join("book.kepub");
        let fin = work.join("book.kepub.epub");
        std::fs::write(&fin, "stale leftover").unwrap();
        let out = to_kepub(&epub, &on_path(&tools.str())).unwrap();
        assert_eq!(out, fin);
        assert!(!std::path::Path::new(&raw).exists());
        assert_ne!(common::read(&out), "stale leftover");
        let argv: Vec<String> = common::read(&log).lines().map(str::to_owned).collect();
        let mut want = vec![epub, raw];
        want.extend(CALIBRE_FORMAT_GUARDS.iter().map(|s| (*s).to_owned()));
        assert_eq!(argv, want);
    }
}

#[test]
fn to_kepub_is_calibre_missing_when_calibre_is_absent() {
    // An environment with no PATH and no Program Files: only the fixed
    // install paths remain, which is the absence this needs on a machine
    // without Calibre.
    let env = sp_engine::env::Env::new();
    if calibre_tool("ebook-convert", &env).is_some() {
        return;
    }
    let err = to_kepub("/tmp/screepub-does-not-exist.epub", &env).unwrap_err();
    assert_eq!(err.name(), "CalibreMissingError");
}

#[test]
fn a_conversion_is_given_ten_minutes() {
    assert_eq!(CONVERSION_TIMEOUT_MS, 10 * 60 * 1000);
}

#[cfg(unix)]
fn alive(pid: &str) -> bool {
    std::process::Command::new("kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(unix)]
#[test]
fn a_run_that_outlives_its_timeout_is_stopped_children_and_all() {
    let s = Scratch::new("hang");
    let tool = s.join("ebook-convert");
    let child_pid = s.join("child.pid");
    common::executable(
        &tool,
        format!("#!/bin/sh\nsleep 30 &\necho $! > \"{child_pid}\"\nwait\n"),
    );
    let started = std::time::Instant::now();
    // Long enough that the script has started its child before the stop.
    let err = run_calibre(&tool, &[], &guarded_env(), 1500).unwrap_err();
    assert_eq!(err.name(), "CalibreTimedOutError");
    assert!(is_calibre_failed(&err));
    assert!(err.message().contains("still running after"));
    assert_eq!(
        err.message(),
        "ebook-convert was still running after 1500 ms, so it was stopped. If Kindle Previewer or Calibre opened a window, close it and try again."
    );
    assert!(started.elapsed().as_secs() < 10);
    let pid = common::read(&child_pid).trim().to_owned();
    assert!(!pid.is_empty());
    let mut waited = 0;
    while alive(&pid) && waited < 2000 {
        std::thread::sleep(std::time::Duration::from_millis(25));
        waited += 25;
    }
    assert!(!alive(&pid), "the grandchild outlived the stop");
}

#[cfg(unix)]
#[test]
fn a_failing_tool_reports_its_stderr_or_its_exit_code() {
    let s = Scratch::new("fail");
    let loud = s.join("loud");
    common::executable(&loud, "#!/bin/sh\necho '  it broke  ' >&2\nexit 3\n");
    let quiet = s.join("quiet");
    common::executable(&quiet, "#!/bin/sh\nexit 4\n");
    assert_eq!(
        run_calibre(&loud, &[], &guarded_env(), 10_000)
            .unwrap_err()
            .message(),
        "ebook-convert failed: it broke"
    );
    assert_eq!(
        run_calibre(&quiet, &[], &guarded_env(), 10_000)
            .unwrap_err()
            .message(),
        "ebook-convert failed: exit 4"
    );
}

#[cfg(unix)]
#[test]
fn the_tool_gets_exactly_the_environment_it_was_handed() {
    // The Rust form of "the engine hands Calibre the scratch copy, not the
    // environment bun started with": nothing is inherited.
    let s = Scratch::new("env");
    let tool = s.join("ebook-convert");
    let seen = s.join("seen.txt");
    common::executable(
        &tool,
        "#!/bin/sh\nprintf '%s|%s' \"$CALIBRE_CONFIG_DIRECTORY\" \"$CARGO\" > \"$1\"\n",
    );
    let env = guarded_env().with("CALIBRE_CONFIG_DIRECTORY", "/scratch/copy");
    run_calibre(&tool, std::slice::from_ref(&seen), &env, 10_000).unwrap();
    assert_eq!(common::read(&seen), "/scratch/copy|");
}
