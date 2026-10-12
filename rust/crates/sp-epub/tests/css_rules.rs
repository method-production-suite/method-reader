//! Port of tests/css-rules.test.ts: the selector-exact rule reader
//! (tests/common/mod.rs, from tests/css-rules.ts) that the stylesheet tests
//! lean on. A small hand-written fixture reproduces the shape that bit once
//! (commit 3abeba3): a grouped selector list BEFORE a bare rule for one of
//! its own members, plus a comment whose text overlaps a property name.

mod common;

use common::{each_rule, rule_for};
use sp_epub::default_css;

const FIXTURE: &str = "
/* shadow rule: the comma-joined list is a distinct entry, page-break-inside
   is mentioned here purely to prove comments don't leak into extraction */
.keep-together, table.dual-dialogue { -webkit-column-break-inside: avoid; }

.keep-together {
  page-break-inside: avoid;
  break-inside: avoid;
}

table.dual-dialogue {
  width: 100%;
  page-break-inside: avoid;
  break-inside: avoid;
}
";

/// A cosmetic line-wrap of the same grouped selector.
const WRAPPED_FIXTURE: &str = "
.keep-together,
table.dual-dialogue { -webkit-column-break-inside: avoid; }

table.dual-dialogue {
  width: 100%;
}
";

#[test]
fn a_grouped_selector_wrapped_across_two_lines_is_still_found_by_its_single_line_selector() {
    let rule = rule_for(WRAPPED_FIXTURE, ".keep-together, table.dual-dialogue");
    assert!(rule.contains("-webkit-column-break-inside: avoid"));
}

#[test]
fn a_bare_member_of_a_line_wrapped_group_is_not_returned_by_the_wrapped_grouped_rule() {
    let rule = rule_for(WRAPPED_FIXTURE, "table.dual-dialogue");
    assert!(!rule.contains("-webkit-column-break-inside"));
    assert!(rule.contains("width: 100%"));
}

#[test]
fn exact_match_returns_the_body_of_the_selector_requested() {
    let rule = rule_for(FIXTURE, "table.dual-dialogue");
    assert!(rule.contains("width: 100%"));
    assert!(rule.contains("break-inside: avoid"));
}

#[test]
fn asking_for_the_bare_selector_does_not_return_the_grouped_rule() {
    let rule = rule_for(FIXTURE, "table.dual-dialogue");
    assert!(!rule.contains("-webkit-column-break-inside"));
}

#[test]
fn asking_for_the_grouped_selector_verbatim_does_return_it() {
    let rule = rule_for(FIXTURE, ".keep-together, table.dual-dialogue");
    assert!(rule.contains("-webkit-column-break-inside: avoid"));
}

#[test]
fn the_shadowing_case_an_unanchored_regex_grabs_the_grouped_rule_rule_for_does_not() {
    // The failure mode that shipped once: an unanchored
    // /table\.dual-dialogue\s*{[^}]*}/ matches inside the comma list.
    let old_style = regex::Regex::new(r"table\.dual-dialogue\s*\{[^}]*\}")
        .unwrap()
        .find(FIXTURE)
        .unwrap()
        .as_str()
        .to_owned();
    assert!(old_style.contains("-webkit-column-break-inside")); // the bug, demonstrated
    let correct = rule_for(FIXTURE, "table.dual-dialogue");
    assert!(!correct.contains("-webkit-column-break-inside"));
    assert!(correct.contains("width: 100%"));
}

#[test]
fn a_leading_comment_never_contaminates_the_returned_rule() {
    let rule = rule_for(FIXTURE, ".keep-together, table.dual-dialogue");
    // The fixture's own comment contains this exact string.
    assert!(!rule.contains("page-break-inside"));
}

#[test]
fn each_rule_reports_a_wrapped_grouped_selector_whole_not_by_its_last_line() {
    let selectors: Vec<String> = each_rule(WRAPPED_FIXTURE)
        .into_iter()
        .map(|r| r.selector)
        .collect();
    assert_eq!(
        selectors,
        [".keep-together, table.dual-dialogue", "table.dual-dialogue"]
    );
}

#[test]
fn each_rule_pairs_each_selector_with_its_own_body() {
    let rules = each_rule(FIXTURE);
    let grouped = rules
        .iter()
        .find(|r| r.selector == ".keep-together, table.dual-dialogue")
        .unwrap();
    assert!(grouped.body.contains("-webkit-column-break-inside: avoid"));
    assert!(!grouped.body.contains("width: 100%"));
}

#[test]
#[should_panic(expected = "no rule for selector: p.does-not-exist")]
fn not_found_panics_rather_than_returning_nothing() {
    rule_for(FIXTURE, "p.does-not-exist");
}

#[test]
fn against_the_real_stylesheet_the_historical_incident_stays_fixed() {
    // table.dual-dialogue requested bare resolves to the real rule, not the
    // multicol shadow, which is emitted AFTER it.
    let css = default_css();
    let rule = rule_for(&css, "table.dual-dialogue");
    assert!(rule.contains("width: 100%"));
    assert!(!rule.contains("-webkit-column-break-inside"));
    let both: Vec<_> = each_rule(&css)
        .into_iter()
        .filter(|r| r.selector == "table.dual-dialogue")
        .collect();
    assert_eq!(both.len(), 2);
    assert!(both[1].body.contains("-webkit-column-break-inside: avoid"));
}
