//! rust/deny.toml and the CI job that runs it. Each advisory deny.toml lets
//! through carries its reason, and CI runs `cargo deny check` on every push.

mod repo_common;

use repo_common::*;

#[test]
fn every_ignored_advisory_says_why() {
    let deny = read("rust/deny.toml");
    let ids: Vec<&str> = deny
        .lines()
        .filter(|l| l.trim_start().starts_with("{ id = \"RUSTSEC-"))
        .collect();
    // Named, not counted: a new exception is a decision, made here too.
    // The first three were known at the cutover (2026-10-12); 0370 to 0372
    // are zbus 5's helper crates, folded into zbus 6 (2026-10-16).
    let named: Vec<&str> = ids
        .iter()
        .filter_map(|l| l.split("id = \"").nth(1).and_then(|r| r.split('"').next()))
        .collect();
    assert_eq!(
        named,
        [
            "RUSTSEC-2024-0370",
            "RUSTSEC-2026-0192",
            "RUSTSEC-2024-0449",
            "RUSTSEC-2026-0370",
            "RUSTSEC-2026-0371",
            "RUSTSEC-2026-0372",
        ],
        "every ignored advisory, in order: {ids:?}"
    );
    for line in ids {
        let reason = line
            .split("reason = \"")
            .nth(1)
            .and_then(|r| r.split('"').next())
            .unwrap_or_default();
        assert!(reason.len() > 20, "{line}");
    }
}

#[test]
fn the_licenses_allowed_are_named_in_the_notices() {
    let deny = read("rust/deny.toml");
    let notices = read("THIRD-PARTY-NOTICES.md");
    let at = deny.find("allow = [").expect("the allow list");
    let list = &deny[at..deny[at..].find(']').map(|e| at + e).unwrap()];
    let mut n = 0;
    for l in list.lines().skip(1) {
        let Some(name) = l.trim().strip_prefix('"').and_then(|l| l.split('"').next()) else {
            continue;
        };
        n += 1;
        // CC0-1.0 is only notify's, inside sp-mtp, which ships in no release
        // yet (deny.toml says so beside it).
        if name == "CC0-1.0" {
            continue;
        }
        // The notices name a license by its SPDX id somewhere in their tables.
        let id = name.split(" WITH ").next().unwrap();
        assert!(
            notices.contains(id),
            "{name} is allowed but the notices never name it"
        );
    }
    assert!(n >= 10, "{n}");
}

#[test]
fn rust_yml_runs_cargo_deny_on_every_push() {
    let wf = workflow("rust.yml");
    let deny = job(&wf, "deny");
    assert!(run_text(deny).contains("cargo deny check"));
    assert!(run_text(deny).contains("cargo install cargo-deny --version"));
}
