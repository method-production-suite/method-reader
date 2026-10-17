//! Port of the spawned half of tests/cli-reveal.test.ts: refusals and
//! --help ONLY, never a valid existing path, so nothing can open a file
//! manager. The handler and the real opener are sp-device's
//! (sp-device/tests/reveal.rs).

mod common;

use common::Sandbox;

#[test]
fn reveal_help_describes_the_verb_and_exits_0() {
    let sb = Sandbox::new();
    let ran = sb.run(&["reveal", "--help"]);
    assert_eq!(ran.code, 0);
    assert!(ran.stdout.contains("method-reader reveal"));
    assert!(ran.stdout.contains("file manager"));
}

#[test]
fn no_positional_is_a_usage_refusal() {
    let sb = Sandbox::new();
    let ran = sb.run(&["reveal", "--json"]);
    assert_eq!(ran.code, 1);
    let answer = ran.json();
    assert_eq!(answer["ok"], false);
    assert_eq!(answer["error"]["code"], "usage");
}

#[test]
fn two_positionals_is_a_usage_refusal_even_when_both_are_absolute_and_neither_exists() {
    // The count is checked before either path is: both absolute and both
    // missing, so a check of either path first would answer for the wrong
    // reason. Safe either way: neither path exists.
    let sb = Sandbox::new();
    let a = sb.p("two-positionals-a.pdf");
    let b = sb.p("two-positionals-b.pdf");
    let ran = sb.run(&["reveal", &a, &b, "--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "usage");
    assert!(ran.error_message().contains("exactly one"));
}

#[test]
fn a_relative_path_is_refused_as_usage_before_anything_opens() {
    let sb = Sandbox::new();
    let ran = sb.run(&["reveal", "Field Station.pdf", "--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "usage");
}

#[test]
fn a_missing_absolute_path_inside_scratch_is_refused_as_unreadable_before_anything_opens() {
    let sb = Sandbox::new();
    let missing = sb.p("nope.pdf");
    let ran = sb.run(&["reveal", &missing, "--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "unreadable");
    assert!(ran.error_message().contains(&missing));
}

#[test]
fn the_top_level_help_usage_lists_reveal() {
    let sb = Sandbox::new();
    let ran = sb.run(&["--help"]);
    assert_eq!(ran.code, 0);
    assert!(ran.stdout.contains("method-reader reveal <file> [--json]"));
    assert!(
        ran.stdout
            .contains("show a file in the system's file manager")
    );
}
