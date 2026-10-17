//! Every `cargo xtask` command answers `--help` and `-h` with its own usage
//! and exits 0, and nothing else runs: once check-tap took `--help` for a
//! version, and capture-screens, mobi-picture-probe, bump-version and
//! release-notes each did something other than print their usage.

mod repo_common;

use repo_common::Scratch;
use std::process::{Command, Output};
use std::time::{Duration, Instant};

fn xtask(args: &[&str], cwd: &std::path::Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(args)
        .current_dir(cwd)
        // Nothing here may reach a network or a real tool.
        .env("METHOD_READER_GITHUB_API", "http://127.0.0.1:9")
        .env("METHOD_READER_GITHUB_WEB", "http://127.0.0.1:9")
        .env("CALIBRE_CONFIG_DIRECTORY", "/dev/null/screepub-help-guard")
        .env("METHOD_READER_CONFIG_DIR", "/dev/null/screepub-help-guard")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("xtask runs")
}

#[test]
fn every_command_answers_help_with_its_usage_and_exit_0() {
    let sb = Scratch::new("help");
    let mut checked = 0;
    for c in xtask::COMMANDS {
        for flag in ["--help", "-h"] {
            let started = Instant::now();
            let out = xtask(&[c.name, flag], sb.path());
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert_eq!(
                out.status.code(),
                Some(0),
                "{} {flag}: {stdout}{stderr}",
                c.name
            );
            assert!(
                stdout.starts_with(&format!("usage: cargo xtask {}", c.name)),
                "{} {flag} printed: {stdout:?}",
                c.name
            );
            assert!(stderr.trim().is_empty(), "{} {flag}: {stderr}", c.name);
            assert!(
                started.elapsed() < Duration::from_secs(20),
                "{} {flag} did more than print its usage",
                c.name
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 2 * xtask::COMMANDS.len());
    // Nothing was written where the commands ran.
    assert_eq!(std::fs::read_dir(sb.path()).unwrap().count(), 0);
}

#[test]
fn help_anywhere_in_the_arguments_still_only_prints_the_usage() {
    let sb = Scratch::new("help-anywhere");
    let repo = sb.path().to_str().unwrap();
    for args in [
        vec!["bump-version", "9.9.9", "--repo", repo, "--help"],
        vec!["release-notes", "v0.0.1", "-h"],
        vec!["mobi-picture-probe", "--out", repo, "--help"],
        vec!["check-tap", "0.11.1", "--help"],
        vec!["capture-screens", "--only", "drop", "-h"],
    ] {
        let out = xtask(&args, sb.path());
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(out.status.code(), Some(0), "{args:?}: {stdout}");
        assert!(
            stdout.starts_with("usage: cargo xtask "),
            "{args:?}: {stdout}"
        );
    }
    assert_eq!(std::fs::read_dir(sb.path()).unwrap().count(), 0);
}

#[test]
fn check_tap_takes_no_flag_for_a_version() {
    // Called directly, not through `cargo xtask`'s own --help handling.
    assert_eq!(xtask::check_tap::main(&["--help".to_string()]), 0);
    assert_eq!(xtask::check_tap::main(&["--verbose".to_string()]), 2);
    assert_eq!(
        xtask::check_tap::main(&["0.11.1".to_string(), "0.11.0".to_string()]),
        2
    );
}

#[test]
fn the_command_list_points_at_each_commands_own_help() {
    let usage = xtask::usage();
    assert!(usage.contains("`cargo xtask <command> --help`"));
    for c in xtask::COMMANDS {
        assert!(usage.contains(c.name), "{}", c.name);
    }
}
