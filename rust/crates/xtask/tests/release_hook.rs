//! `cargo xtask require-release-notes`: ported from
//! tests/require-release-notes.test.ts.
//!
//! The TypeScript ran the shell hook in the real repository, where
//! docs/releases/0.5.0.md is committed and 99.0.0.md is not. Here a scratch
//! repo commits 0.5.0.md alone, so the cases do not lean on live history.

mod common;

use std::path::PathBuf;

use common::{Scratch, git, text, xtask};
use xtask::release_hook::{Decision, decide};

thread_local! {
    /// One scratch repo per test (each test runs on its own thread, whose
    /// end drops and removes it): docs/releases/0.5.0.md committed.
    static REPO: Scratch = {
        let s = common::git_repo("release-hook");
        s.write("docs/releases/0.5.0.md", "# Screepub 0.5.0\n");
        git(s.path(), &["add", "-A"]);
        git(s.path(), &["commit", "-q", "-m", "notes"]);
        s
    };
}

fn repo() -> PathBuf {
    REPO.with(|r| r.path().to_path_buf())
}

fn payload(command: &str) -> String {
    serde_json::json!({ "tool_input": { "command": command } }).to_string()
}

fn run_hook(command: &str) -> Decision {
    decide(Some(&payload(command)), &repo())
}

fn code(command: &str) -> u8 {
    run_hook(command).code
}

/// The degraded mode: a payload that is not JSON the hook can read (the
/// shell's "jq missing" case). Here the payload is cut short, so it cannot
/// parse, and the raw text is what gets scanned.
fn run_hook_raw(command: &str) -> Decision {
    let p = payload(command);
    decide(Some(&p[..p.len() - 1]), &repo())
}

#[test]
fn ignores_commands_that_have_nothing_to_do_with_tags() {
    assert_eq!(code("cargo test --workspace"), 0);
    assert_eq!(code("git status"), 0);
    assert_eq!(code("git commit -m \"wip\""), 0);
}

#[test]
fn never_blocks_deleting_a_tag() {
    assert_eq!(code("git tag -d v0.5.0"), 0);
    assert_eq!(code("git push origin :refs/tags/v0.5.0"), 0);
}

#[test]
fn the_read_only_listing_allowlist_does_not_swallow_a_real_create() {
    assert_eq!(code("git tag my-tag"), 2);
}

#[test]
fn blocks_tagging_a_version_whose_notes_are_missing() {
    let d = run_hook("git tag -a v99.0.0 -m release");
    assert_eq!(d.code, 2);
    assert!(d.stderr.contains("docs/releases/99.0.0.md"));
}

#[test]
fn blocks_gh_release_create_for_a_version_with_no_notes() {
    assert_eq!(code("gh release create v99.0.0"), 2);
}

#[test]
fn allows_tagging_a_version_whose_notes_exist() {
    assert_eq!(code("git tag -a v0.5.0 -m release"), 0);
}

#[test]
fn fails_closed_when_it_matches_but_cannot_resolve_a_version() {
    let d = run_hook("git push --follow-tags");
    assert_eq!(d.code, 2);
    assert!(d.stderr.contains("/release"));
}

#[test]
fn a_bare_lookalike_is_not_tag_touching_but_naming_it_as_a_tag_fails_closed() {
    assert_eq!(code("notaversion"), 0);
    assert_eq!(code("git tag notaversion"), 2);
}

#[test]
fn a_plain_git_push_naming_a_tag_is_matched_without_a_bulk_flag() {
    let a = run_hook("git push origin v99.0.0");
    assert_eq!(a.code, 2);
    assert!(a.stderr.contains("docs/releases/99.0.0.md"));
    let b = run_hook("git push --atomic origin main v99.0.0");
    assert_eq!(b.code, 2);
    assert!(b.stderr.contains("docs/releases/99.0.0.md"));
}

#[test]
fn a_plain_git_push_naming_a_tag_with_existing_notes_is_allowed() {
    assert_eq!(code("git push --atomic origin main v0.5.0"), 0);
}

#[test]
fn prerelease_versions_are_matched_in_full() {
    let d = run_hook("git tag -a v0.5.0-rc1 -m release");
    assert_eq!(d.code, 2);
    assert!(d.stderr.contains("docs/releases/0.5.0-rc1.md"));
}

#[test]
fn recovery_matching_is_anchored_to_the_command_form_not_a_bare_substring() {
    assert_eq!(code("git tag -d v0.5.0 && git tag v99.0.0"), 2);
    assert_eq!(code("git tag v99.0.0 # tag -d"), 2);
    assert_eq!(code("git tag -a v99.0.0 -m \"see git tag -d\""), 2);
}

#[test]
fn every_version_token_in_the_command_is_checked_not_just_the_first() {
    assert_eq!(code("git push --tags origin v0.5.0 v99.0.0"), 2);
    assert_eq!(code("git push --tags origin v99.0.0 v0.5.0"), 2);
}

#[test]
fn read_only_tag_listing_is_never_blocked() {
    for c in [
        "git tag",
        "git tag --list",
        "git tag -l 'v*'",
        "git tag --contains HEAD",
        "git tag --points-at HEAD",
        "git tag --sort=-v:refname",
    ] {
        assert_eq!(code(c), 0, "{c}");
    }
}

#[test]
fn compound_detection_catches_or_pipe_single_ampersand_and_substitution() {
    for c in [
        "git tag --list || git tag v99.0.0",
        "git tag --list | xargs -I{} git tag v99.0.0",
        "git tag --list & git tag v99.0.0",
        "git tag --list $(git tag v99.0.0)",
        "git tag -d v0.5.0 || git tag v99.0.0",
    ] {
        assert_eq!(code(c), 2, "{c}");
    }
}

#[test]
fn a_metacharacter_alone_does_not_make_a_command_compound() {
    assert_eq!(code("git tag --list | wc -l"), 0);
}

#[test]
fn a_create_whose_message_merely_mentions_a_listing_flag_is_still_checked() {
    for c in [
        "git tag -a v99.0.0 -m \"--list\"",
        "git tag -a v99.0.0 -m \"see --format\"",
    ] {
        let d = run_hook(c);
        assert_eq!(d.code, 2, "{c}");
        assert!(d.stderr.contains("docs/releases/99.0.0.md"), "{c}");
    }
}

#[test]
fn recovery_still_works_when_the_payload_cannot_be_read() {
    // The shell's jq-missing mode: the raw payload starts with "{", so the
    // anchored recovery checks could never match; the bare substring test
    // takes over instead of leaving recovery blocked.
    assert_eq!(run_hook_raw("git tag -d v99.0.0").code, 0);
    assert_eq!(run_hook_raw("git push origin :refs/tags/v99.0.0").code, 0);
    // And a real create still blocks in that mode.
    let d = run_hook_raw("git tag -a v99.0.0 -m release");
    assert_eq!(d.code, 2);
    assert!(d.stderr.contains("docs/releases/99.0.0.md"));
}

#[test]
fn listing_and_recovery_are_recognized_when_git_tag_is_not_the_first_token() {
    for c in [
        "cd /tmp && git tag --list",
        "echo hi && git tag --list",
        "GIT_PAGER=cat git tag --list",
        "for x in 1; do git tag --list; done",
        "git log --oneline && git tag",
        "grep -rn \"git tag\" docs/",
        "echo \"run git tag to list\"",
    ] {
        assert_eq!(code(c), 0, "{c}");
    }
}

#[test]
fn the_standard_two_step_undo_is_recovery_not_a_compound_create() {
    assert_eq!(
        code("git tag -d v9.9.9 && git push origin :refs/tags/v9.9.9"),
        0
    );
    assert_eq!(
        code("git tag -d v9.9.9;  git push origin :refs/tags/v9.9.9"),
        0
    );
}

#[test]
fn modern_and_long_form_delete_spellings_are_recovery() {
    for c in [
        "git push --delete origin v9.9.9",
        "git push -d origin v9.9.9",
        "git tag --delete v9.9.9",
        "git tag -d v9.9.9",
    ] {
        assert_eq!(code(c), 0, "{c}");
    }
}

#[test]
fn a_delete_followed_by_a_real_create_in_the_same_compound_command_still_blocks() {
    let d = run_hook("git tag -d v0.5.0 && git tag v99.0.0");
    assert_eq!(d.code, 2);
    assert!(d.stderr.contains("docs/releases/99.0.0.md"));
}

#[test]
fn a_long_unresolvable_command_is_quoted_truncated() {
    let long = format!("git push --follow-tags {}", "x".repeat(300));
    let d = run_hook(&long);
    assert_eq!(d.code, 2);
    assert!(d.stderr.contains("...[truncated]"));
}

#[test]
fn the_command_reads_stdin_and_answers_with_its_exit_code() {
    // Through the binary, as Claude Code runs it: blocked, allowed, and
    // nothing on stdin at all.
    let blocked = xtask(
        &["require-release-notes"],
        &[],
        Some(&repo()),
        Some(&payload("git tag -a v99.0.0 -m release")),
    );
    assert_eq!(blocked.status.code(), Some(2));
    assert!(text(&blocked.stderr).contains("docs/releases/99.0.0.md"));
    let allowed = xtask(
        &["require-release-notes"],
        &[],
        Some(&repo()),
        Some(&payload("git tag -a v0.5.0 -m release")),
    );
    assert_eq!(allowed.status.code(), Some(0));
    assert_eq!(text(&allowed.stderr), "");
    let empty = xtask(&["require-release-notes"], &[], Some(&repo()), None);
    assert_eq!(empty.status.code(), Some(0));
}
