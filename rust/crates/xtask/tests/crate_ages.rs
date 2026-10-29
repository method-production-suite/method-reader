//! scripts/check-crate-ages.py's exceptions, executed (SUITE-STANDARDS.md
//! section 6): every exception says its kind, a RUSTSEC id (security) or
//! "ages-out YYYY-MM-DD", or "owner-approved override" for one named
//! release, and `--release` honours only the security ones and the
//! override whose release is `--tag`.
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
    run_tag(py, exceptions, release, None)
}

fn run_tag(py: &str, exceptions: &std::path::Path, release: bool, tag: Option<&str>) -> Output {
    let mut c = Command::new(py);
    c.arg(root().join("scripts/check-crate-ages.py"))
        .arg("--exceptions")
        .arg(exceptions)
        .arg("--print-exceptions");
    if release {
        c.arg("--release");
    }
    if let Some(tag) = tag {
        c.args(["--tag", tag]);
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
fn a_github_advisory_id_makes_an_exception_security() {
    let Some(py) = python() else {
        eprintln!("skipped: no Python 3.11 or later on PATH");
        return;
    };
    let sc = Scratch::new("crate-ages-ghsa");
    // GitHub can publish an advisory days before RustSec gives it an id.
    let f = sc.write(
        "exceptions.txt",
        format!("{BOTH}foo 1.2.3   # GHSA-m665-v7pm-7grj (2026-10-28), the fix\n"),
    );
    assert_eq!(
        honoured(&run(py, &f, true)),
        vec!["foo 1.2.3", "rustls 0.23.46", "zip 9.0.3"]
    );
    // Something shaped only roughly like one says no kind.
    let bad = sc.write(
        "bad.txt",
        format!("{BOTH}foo 1.2.3   # GHSA-1234-5678-abcd\n"),
    );
    let out = run(py, &bad, true);
    assert!(!out.status.success(), "a malformed GHSA id passed");
    assert!(
        said(&out).contains("every exception needs a kind"),
        "{}",
        said(&out)
    );
}

#[test]
fn an_owner_approved_override_holds_for_its_named_release_only() {
    let Some(py) = python() else {
        eprintln!("skipped: no Python 3.11 or later on PATH");
        return;
    };
    let sc = Scratch::new("crate-ages-owner");
    let f = sc.write(
        "exceptions.txt",
        format!("{BOTH}foo 1.2.3   # owner-approved override, the owner, 2026-10-27, release v1.0.0 (\"Yes\")\n"),
    );
    // Main and a rehearsal honour it like any other line.
    assert!(honoured(&run(py, &f, false)).contains(&"foo 1.2.3".to_string()));
    // A release honours it only when it is the release named.
    let named = honoured(&run_tag(py, &f, true, Some("v1.0.0")));
    assert_eq!(named, vec!["foo 1.2.3", "rustls 0.23.46", "zip 9.0.3"]);
    for other in [Some("v1.0.1"), Some("v0.6.0"), None] {
        let got = honoured(&run_tag(py, &f, true, other));
        assert_eq!(got, vec!["rustls 0.23.46", "zip 9.0.3"], "{other:?}");
    }
}

#[test]
fn an_override_for_a_near_miss_version_never_counts_for_v1_0_0() {
    let Some(py) = python() else {
        eprintln!("skipped: no Python 3.11 or later on PATH");
        return;
    };
    let sc = Scratch::new("crate-ages-near-miss");
    for (tag, named) in [
        ("rc", "v1.0.0-rc1"),
        ("four-part", "v1.0.0.1"),
        ("build", "v1.0.0+build"),
        ("suffix", "v1.0.0x"),
    ] {
        let f = sc.write(
            &format!("{tag}.txt"),
            format!("{BOTH}foo 1.2.3   # owner-approved override, the owner, 2026-10-27, release {named}\n"),
        );
        // Not read as v1.0.0: the line names no release the check knows,
        // so it is refused outright, whatever the tag.
        let out = run_tag(py, &f, true, Some("v1.0.0"));
        assert!(!out.status.success(), "{named} counted for v1.0.0");
        assert!(
            said(&out).contains("names no \"release vX.Y.Z\""),
            "{named}: {}",
            said(&out)
        );
    }
}

#[test]
fn the_release_workflow_names_its_tag_to_the_crate_age_check() {
    let wf = std::fs::read_to_string(root().join(".github/workflows/release.yml")).unwrap();
    assert!(
        wf.contains("check-crate-ages.py --lock \"$lock\" --release --tag \"$GITHUB_REF_NAME\""),
        "release.yml's --release run must pass the tag, or no override can hold"
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
        (
            "no-release",
            "foo 1.2.3   # owner-approved override, the owner, 2026-10-27\n",
        ),
        ("three-words", "foo 1.2.3 extra   # RUSTSEC-2026-0001\n"),
        // An id with more after it is not that id.
        ("long-rustsec", "foo 1.2.3   # RUSTSEC-2026-03460\n"),
        ("long-ghsa", "foo 1.2.3   # GHSA-m665-v7pm-7grjzz\n"),
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
