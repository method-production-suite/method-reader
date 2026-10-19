//! scripts/check-crate-ages.py's exceptions, executed (SUITE-STANDARDS.md
//! section 6): every exception says its kind, a RUSTSEC id (security) or
//! "ages-out YYYY-MM-DD", and `--release` honours only the security ones.
//! `--print-exceptions` answers without the network, so this runs the
//! script itself against scratch exception files. Skipped, saying so,
//! where no Python 3.11 or later is on PATH.

mod repo_common;

use repo_common::*;
use std::process::{Command, Output};

/// A Python that has tomllib (3.11+), or None.
fn python() -> Option<&'static str> {
    ["python3", "python"].into_iter().find(|p| {
        Command::new(p)
            .args(["-c", "import tomllib"])
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

fn run(py: &str, exceptions: &std::path::Path, release: bool) -> Output {
    let mut c = Command::new(py);
    c.arg(root().join("scripts/check-crate-ages.py"))
        .arg("--exceptions")
        .arg(exceptions)
        .arg("--print-exceptions");
    if release {
        c.arg("--release");
    }
    c.output().expect("python runs")
}

fn honoured(out: &Output) -> Vec<String> {
    assert!(out.status.success(), "{}", said(out));
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

const BOTH: &str = "# header\n\nrustls 0.23.46   # RUSTSEC-2026-0346 to 0349 (2026-10-14)\nparley 0.7.0     # ages-out 2026-10-23: already in the lock\nzip 9.0.3        # yanked before it; RUSTSEC-2026-0367 is the fix\n";

#[test]
fn a_release_honours_only_the_security_exceptions() {
    let Some(py) = python() else {
        eprintln!("skipped: no Python 3.11 or later on PATH");
        return;
    };
    let sc = Scratch::new("crate-ages");
    let f = sc.write("exceptions.txt", BOTH);
    // Without --release, both kinds; a RUSTSEC id anywhere in the reason
    // makes it security.
    assert_eq!(
        honoured(&run(py, &f, false)),
        vec!["parley 0.7.0", "rustls 0.23.46", "zip 9.0.3"]
    );
    // With it, the ages-out crate is no longer excused, so it fails a
    // release while it is young.
    assert_eq!(
        honoured(&run(py, &f, true)),
        vec!["rustls 0.23.46", "zip 9.0.3"]
    );
}

#[test]
fn an_exception_that_says_neither_kind_is_refused() {
    let Some(py) = python() else {
        eprintln!("skipped: no Python 3.11 or later on PATH");
        return;
    };
    let sc = Scratch::new("crate-ages-bad");
    for (tag, line) in [
        ("no-kind", "foo 1.2.3   # a security fix, trust me\n"),
        ("no-reason", "foo 1.2.3\n"),
        ("bad-date", "foo 1.2.3   # ages-out soon\n"),
        ("three-words", "foo 1.2.3 extra   # RUSTSEC-2026-0001\n"),
    ] {
        let f = sc.write(&format!("{tag}.txt"), format!("{BOTH}{line}"));
        for release in [false, true] {
            let out = run(py, &f, release);
            assert!(!out.status.success(), "{tag} passed");
            assert!(
                said(&out).contains("every exception needs a kind"),
                "{tag}: {}",
                said(&out)
            );
        }
    }
}

#[test]
fn the_committed_exceptions_all_say_their_kind() {
    let Some(py) = python() else {
        eprintln!("skipped: no Python 3.11 or later on PATH");
        return;
    };
    let f = root().join("scripts/crate-age-exceptions.txt");
    honoured(&run(py, &f, false));
    honoured(&run(py, &f, true));
}
