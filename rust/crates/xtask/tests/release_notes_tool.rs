//! `cargo xtask release-notes`: ported from tests/release-notes-tool.test.ts.
//! Every git-backed case runs in a throwaway repo, never on live history.

mod common;

use common::{git, git_repo, text, xtask};
use xtask::release_notes::{
    changed_registry_entries, collect_commits, diff_verdicts, is_user_visible, parse_verdicts,
    sanitize,
};

fn commit_file(dir: &std::path::Path, file: &str, body: &str, message: &str) {
    std::fs::write(dir.join(file), body).expect("write");
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", message]);
}

// ── sanitize ──────────────────────────────────────────────────────────────

#[test]
fn sanitize_strips_c0_controls_and_del_but_keeps_everything_else() {
    let nasty = "real\u{1B}[31mesc\ttab\u{7F}del";
    assert_eq!(sanitize(nasty), "real[31mesctabdel");
}

#[test]
fn sanitize_truncates_to_200_characters() {
    assert_eq!(sanitize(&"x".repeat(250)), "x".repeat(200));
}

// ── collect_commits ───────────────────────────────────────────────────────

#[test]
fn returns_subjects_since_the_given_tag() {
    let s = git_repo("relnotes");
    commit_file(s.path(), "b.txt", "two", "add the second thing");
    let commits = collect_commits(s.path(), "v0.1.0", None).expect("commits");
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].subject, "add the second thing");
}

#[test]
fn truncates_a_subject_longer_than_200_characters() {
    let s = git_repo("relnotes");
    commit_file(s.path(), "c.txt", "three", &"x".repeat(400));
    let commits = collect_commits(s.path(), "v0.1.0", None).expect("commits");
    assert_eq!(commits[0].subject, "x".repeat(200));
}

#[test]
fn strips_real_control_bytes_from_the_subject() {
    // The raw bytes go to git through a file (-F), so git receives exactly
    // these bytes, not an escaped spelling of them.
    let s = git_repo("relnotes");
    let msg = s.write("MSG", "real\u{1B}[31mesc\ttab\u{7F}del");
    std::fs::write(s.join("d.txt"), "four").expect("write");
    git(s.path(), &["add", "d.txt"]);
    git(s.path(), &["commit", "-q", "-F", &msg.to_string_lossy()]);
    let commits = collect_commits(s.path(), "v0.1.0", None).expect("commits");
    assert_eq!(commits[0].subject, "real[31mesctabdel");
}

#[test]
fn returns_commits_oldest_first_and_flags_authors_who_are_not_the_owner() {
    let s = git_repo("relnotes");
    commit_file(s.path(), "b.txt", "two", "add the second thing");
    git(s.path(), &["config", "user.name", "Someone Else"]);
    git(s.path(), &["config", "user.email", "someone@example.com"]);
    commit_file(s.path(), "c.txt", "three", "add the third thing");
    let commits = collect_commits(s.path(), "v0.1.0", Some("Dev")).expect("commits");
    let subjects: Vec<&str> = commits.iter().map(|c| c.subject.as_str()).collect();
    assert_eq!(subjects, ["add the second thing", "add the third thing"]);
    let owners: Vec<bool> = commits.iter().map(|c| c.owner_authored).collect();
    assert_eq!(owners, [true, false]);
}

#[test]
fn defaults_the_owner_to_the_repos_own_git_config_not_a_hardcoded_name() {
    let s = git_repo("relnotes");
    commit_file(s.path(), "b.txt", "two", "add the second thing");
    let commits = collect_commits(s.path(), "v0.1.0", None).expect("commits");
    assert!(commits[0].owner_authored);
}

#[test]
fn since_tag_reaches_git_as_one_argument_not_a_shell_command() {
    let s = git_repo("relnotes");
    let marker = s.join("PWNED");
    let tag = format!("v0.1.0 ; touch {} ; echo", marker.display());
    assert!(collect_commits(s.path(), &tag, None).is_err());
    assert!(!marker.exists());
}

// ── verdict diffing ───────────────────────────────────────────────────────

const REGISTRY_BEFORE: &str = "
### 5a. Scene heading keeps its context
- **Device verdict: pending 2026-07-30 \u{2014}** does the chain bind alone?
- **Code:** src/epub/css.ts

### 16. Transitions never begin a page
- **Device verdict: pending 2026-07-30 \u{2014}** does a transition start a page?

### 8c. Whole-speech keep
- **Device verdict: pending 2026-07-30 \u{2014}** does a kept speech move whole?
";

const REGISTRY_AFTER: &str = "
### 5a. Scene heading keeps its context
- **Device verdict 2026-07-30: BINDS \u{2014} the chain holds without the wrapper.**
- **Code:** src/epub/css.ts

### 16. Transitions never begin a page
- **Device verdict: pending 2026-07-30 \u{2014}** does a transition start a page?

### 8c. Whole-speech keep
- **Device verdict: pending 2026-07-30 \u{2014}** does a kept speech move whole?

### 17. Print split minimums
- **Device support:** KFX honors widows/orphans from fw 5.12.3.
- **Device verdict: pending \u{2014}** next KFX pass.
";

#[test]
fn parse_verdicts_finds_every_entry_still_marked_pending() {
    let mut ids: Vec<String> = parse_verdicts(REGISTRY_BEFORE)
        .into_iter()
        .map(|v| v.entry)
        .collect();
    ids.sort();
    assert_eq!(ids, ["16", "5a", "8c"]);
}

#[test]
fn an_entry_pending_at_both_refs_appears_in_neither_bucket() {
    let (opened, resolved) = diff_verdicts(REGISTRY_BEFORE, REGISTRY_AFTER);
    let names: Vec<String> = opened
        .iter()
        .chain(&resolved)
        .map(|v| v.entry.clone())
        .collect();
    assert!(!names.contains(&"16".to_string()));
    assert!(!names.contains(&"8c".to_string()));
}

#[test]
fn a_verdict_that_became_pending_this_cycle_is_opened() {
    let (opened, _) = diff_verdicts(REGISTRY_BEFORE, REGISTRY_AFTER);
    let ids: Vec<&str> = opened.iter().map(|v| v.entry.as_str()).collect();
    assert_eq!(ids, ["17"]);
}

#[test]
fn a_verdict_filled_in_this_cycle_is_resolved() {
    let (_, resolved) = diff_verdicts(REGISTRY_BEFORE, REGISTRY_AFTER);
    let ids: Vec<&str> = resolved.iter().map(|v| v.entry.as_str()).collect();
    assert_eq!(ids, ["5a"]);
}

#[test]
fn an_opened_verdict_carries_its_device_support_line() {
    let (opened, _) = diff_verdicts(REGISTRY_BEFORE, REGISTRY_AFTER);
    assert!(opened[0].support.contains("KFX honors widows/orphans"));
}

#[test]
fn a_resolved_verdict_carries_the_after_ref_device_verdict_sentence() {
    let (_, resolved) = diff_verdicts(REGISTRY_BEFORE, REGISTRY_AFTER);
    assert!(
        resolved[0]
            .verdict
            .as_deref()
            .expect("a verdict")
            .contains("BINDS \u{2014} the chain holds without the wrapper")
    );
}

#[test]
fn a_device_verdict_sentence_that_wraps_across_a_line_break_is_not_truncated() {
    let before = "### 5a. Wraps\n- **Device verdict: pending \u{2014}** tbd\n";
    let after = "### 5a. Wraps\n\
                 - **Device verdict 2026-07-30: BINDS \u{2014} the chain holds without the\n  \
                 wrapper.** First isolated evidence follows in unbolded prose.\n";
    let (_, resolved) = diff_verdicts(before, after);
    assert_eq!(
        resolved[0].verdict.as_deref(),
        Some("**Device verdict 2026-07-30: BINDS \u{2014} the chain holds without the wrapper.**")
    );
}

#[test]
fn a_device_support_sentence_that_wraps_across_a_line_break_is_not_truncated() {
    let after = "### 17. Wraps\n\
                 - **Device support:** KFX honors widows/orphans from fw 5.12.3 (Kindle\n  \
                 Previewer 3.35 added them ~2019); ignored elsewhere.\n\
                 - **Device verdict: pending \u{2014}** next pass\n";
    assert_eq!(
        parse_verdicts(after)[0].support,
        "KFX honors widows/orphans from fw 5.12.3 (Kindle Previewer 3.35 added them ~2019); ignored elsewhere."
    );
}

// ── registry entry body boundaries ───────────────────────────────────────

#[test]
fn an_edit_under_a_later_non_entry_heading_is_not_attributed_to_the_entry_above() {
    let before = "### 15. Not-a-screenplay guard\n- 0 scenes and 0 dialogue -> error.\n\n## Mac app notes\n- some unrelated note.\n";
    let after = "### 15. Not-a-screenplay guard\n- 0 scenes and 0 dialogue -> error.\n\n## Mac app notes\n- a COMPLETELY DIFFERENT unrelated note.\n";
    assert!(changed_registry_entries(before, after).is_empty());
}

#[test]
fn a_pending_mention_under_a_later_non_entry_heading_does_not_make_the_entry_above_pending() {
    let registry = "### 1. Resolved thing\n- **Device verdict 2026-07-30: BINDS.**\n\n## Some other section\nProse here still says: Device verdict: pending on an unrelated topic.\n";
    assert!(parse_verdicts(registry).is_empty());
}

// ── changed_registry_entries ─────────────────────────────────────────────

#[test]
fn reports_entries_whose_text_differs_between_the_two_refs() {
    let before = "### 1. Continuous scene flow\n- **What:** scenes flow.\n\n### 2. Column\n- **What:** narrow.\n";
    let after = "### 1. Continuous scene flow\n- **What:** scenes flow.\n- **MOBI arm:** pagebreaks.\n\n### 2. Column\n- **What:** narrow.\n";
    assert_eq!(
        changed_registry_entries(before, after),
        vec![("1".to_string(), "Continuous scene flow".to_string())]
    );
}

#[test]
fn reports_a_brand_new_entry() {
    let before = "### 1. One\n- a\n";
    let after = "### 1. One\n- a\n\n### 17. Print split minimums\n- b\n";
    assert_eq!(
        changed_registry_entries(before, after),
        vec![("17".to_string(), "Print split minimums".to_string())]
    );
}

#[test]
fn reports_nothing_when_the_registry_is_untouched() {
    let same = "### 1. One\n- a\n";
    assert!(changed_registry_entries(same, same).is_empty());
}

// ── is_user_visible ──────────────────────────────────────────────────────

#[test]
fn tools_and_tests_only_is_not_reader_visible() {
    assert!(!is_user_visible(&[
        "tools/release-notes.ts",
        "tests/release-notes-tool.test.ts"
    ]));
}

#[test]
fn docs_tests_and_github_together_are_still_not_reader_visible() {
    assert!(!is_user_visible(&[
        "docs/formatting-options-log.md",
        "tests/x.test.ts",
        ".github/workflows/ci.yml"
    ]));
}

#[test]
fn a_change_under_src_is_reader_visible() {
    assert!(is_user_visible(&["src/epub/css.ts", "tests/x.test.ts"]));
}

#[test]
fn site_and_brand_are_not_reader_visible() {
    assert!(!is_user_visible(&["site/index.html", "brand/tokens.css"]));
}

#[test]
fn a_site_change_alongside_a_src_change_is_still_reader_visible() {
    assert!(is_user_visible(&["site/index.html", "src/options.ts"]));
}

// ── the command ──────────────────────────────────────────────────────────

#[test]
fn the_command_prints_the_facts_as_json_for_the_range() {
    let s = git_repo("relnotes-cli");
    s.write("docs/formatting-options-log.md", REGISTRY_BEFORE);
    git(s.path(), &["add", "-A"]);
    git(s.path(), &["commit", "-q", "-m", "registry"]);
    git(s.path(), &["tag", "v0.2.0"]);
    s.write("docs/formatting-options-log.md", REGISTRY_AFTER);
    s.write("src/a.ts", "x");
    git(s.path(), &["add", "-A"]);
    git(s.path(), &["commit", "-q", "-m", "a reader-visible change"]);

    let out = xtask(&["release-notes", "v0.2.0"], &[], Some(s.path()), None);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let facts: serde_json::Value = serde_json::from_str(&text(&out.stdout)).expect("JSON");
    assert_eq!(facts["sinceTag"], "v0.2.0");
    assert_eq!(facts["commits"][0]["subject"], "a reader-visible change");
    assert_eq!(facts["commits"][0]["ownerAuthored"], true);
    assert_eq!(facts["verdictsOpenedInRange"][0]["entry"], "17");
    assert!(facts["verdictsOpenedInRange"][0].get("verdict").is_none());
    assert_eq!(facts["verdictsResolvedInRange"][0]["entry"], "5a");
    assert_eq!(facts["registryChanges"].as_array().expect("array").len(), 2);
    assert_eq!(facts["userVisible"], true);
    let keys: Vec<&String> = facts.as_object().expect("object").keys().collect();
    assert_eq!(
        keys,
        [
            "sinceTag",
            "commits",
            "verdictsOpenedInRange",
            "verdictsResolvedInRange",
            "registryChanges",
            "userVisible"
        ]
    );

    // From a subfolder (cargo xtask runs from rust/), the same facts.
    let below = xtask(
        &["release-notes", "v0.2.0"],
        &[],
        Some(&s.join("src")),
        None,
    );
    assert_eq!(text(&below.stdout), text(&out.stdout));

    let usage = xtask(&["release-notes"], &[], Some(s.path()), None);
    assert_eq!(usage.status.code(), Some(2));
}
