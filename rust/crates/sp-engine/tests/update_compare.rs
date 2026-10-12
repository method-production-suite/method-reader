//! Port of tests/update-compare.test.ts: the updater's second opinion, and
//! the once-a-day, opt-in-only rule.

use sp_engine::update::compare::{
    CHECK_INTERVAL_MS, UpdateCandidate, UpdateDecision, is_describe_suffix, is_newer,
    normalize_version, pick_update, should_check,
};

fn pick(version: &str, current: &str) -> UpdateDecision {
    pick_update(Some(&UpdateCandidate {
        version: version.into(),
        current_version: current.into(),
    }))
}

fn refusal(d: UpdateDecision) -> String {
    match d {
        UpdateDecision::Refuse { reason, .. } => reason,
        UpdateDecision::Offer { version, .. } => panic!("expected a refusal, got {version}"),
    }
}

// ---- normalizeVersion ----

#[test]
fn a_leading_v_is_dropped_either_case() {
    assert_eq!(normalize_version("v0.6.0"), "0.6.0");
    assert_eq!(normalize_version("V0.6.0"), "0.6.0");
    assert_eq!(normalize_version("0.6.0"), "0.6.0");
}

#[test]
fn build_metadata_is_dropped() {
    assert_eq!(normalize_version("0.6.0+ci.7"), "0.6.0");
    assert_eq!(normalize_version("v0.6.0+ci.7"), "0.6.0");
}

#[test]
fn surrounding_whitespace_goes() {
    assert_eq!(normalize_version("  v0.6.0\n"), "0.6.0");
}

#[test]
fn a_pre_release_suffix_is_kept() {
    assert_eq!(normalize_version("v0.6.0-beta.1"), "0.6.0-beta.1");
}

// ---- isNewer: ordinary cases ----

#[test]
fn a_later_patch_minor_or_major_is_newer() {
    assert!(is_newer("0.6.1", "0.6.0"));
    assert!(is_newer("0.7.0", "0.6.9"));
    assert!(is_newer("1.0.0", "0.9.9"));
}

#[test]
fn the_same_version_is_not_newer() {
    assert!(!is_newer("0.6.0", "0.6.0"));
    assert!(!is_newer("v0.6.0", "0.6.0"));
}

#[test]
fn an_older_release_is_never_newer() {
    assert!(!is_newer("0.5.4", "0.6.0"));
}

#[test]
fn ten_beats_nine_numerically() {
    assert!(is_newer("0.10.0", "0.9.0"));
    assert!(!is_newer("0.9.0", "0.10.0"));
    assert!(is_newer("0.6.10", "0.6.9"));
}

#[test]
fn missing_components_count_as_zero() {
    assert!(!is_newer("1", "1.0.0"));
    assert!(is_newer("1.0.1", "1"));
}

// ---- pre-releases ----

#[test]
fn a_release_beats_its_pre_release() {
    assert!(is_newer("0.6.0", "0.6.0-beta.1"));
}

#[test]
fn a_pre_release_never_beats_its_release() {
    assert!(!is_newer("0.6.0-beta.1", "0.6.0"));
}

#[test]
fn two_pre_releases_compare_numerically() {
    assert!(is_newer("0.6.0-beta.10", "0.6.0-beta.9"));
    assert!(!is_newer("0.6.0-beta.9", "0.6.0-beta.10"));
}

// ---- isDescribeSuffix ----

#[test]
fn n_g_hex_is_git_describe_output() {
    assert!(is_describe_suffix("1-g965cb10"));
    assert!(is_describe_suffix("42-gabcdef0"));
}

#[test]
fn so_is_dirty() {
    assert!(is_describe_suffix("1-g965cb10-dirty"));
    assert!(is_describe_suffix("dirty"));
}

#[test]
fn an_ordinary_pre_release_is_not_describe_output() {
    assert!(!is_describe_suffix("beta.1"));
    assert!(!is_describe_suffix("rc1"));
    assert!(!is_describe_suffix("alpha"));
}

#[test]
fn near_misses_are_not_describe_output() {
    assert!(!is_describe_suffix("1-g"));
    assert!(!is_describe_suffix("1-gzzzzzz"));
    assert!(!is_describe_suffix("x-g965cb10"));
    assert!(!is_describe_suffix("-g965cb10"));
    assert!(!is_describe_suffix("g965cb10"));
}

// ---- the defence ----

#[test]
fn a_tag_is_not_an_update_for_a_build_past_it() {
    assert!(!is_newer("0.6.0", "0.6.0-1-g965cb10"));
    assert!(!is_newer("0.6.0", "0.6.0-1-g965cb10-dirty"));
    assert!(!is_newer("0.6.0", "0.6.0-dirty"));
}

#[test]
fn semver_would_disagree_and_that_is_the_point() {
    assert!(is_newer("0.6.0", "0.6.0-beta.1"));
    assert!(!is_newer("0.6.0", "0.6.0-1-g965cb10"));
}

#[test]
fn a_later_release_is_still_offered_to_a_dev_build() {
    assert!(is_newer("0.7.0", "0.6.0-1-g965cb10"));
    assert!(is_newer("0.6.1", "0.6.0-1-g965cb10"));
}

#[test]
fn a_dev_build_is_never_an_update_for_a_release() {
    assert!(!is_newer("0.6.0-1-g965cb10", "0.6.0"));
}

// ---- the Swift app's pairs ----

#[test]
fn a_dev_build_is_offered_the_matching_stable() {
    assert!(is_newer("0.3.0", "0.3.0-dev"));
}

#[test]
fn the_remaining_pairs_the_swift_app_pinned() {
    assert!(is_newer("v0.4.0", "0.3.0"));
    assert!(is_newer("0.10.0", "0.9.0"));
    assert!(!is_newer("0.3.0", "0.3.0"));
    assert!(!is_newer("0.2.9", "0.3.0"));
    assert!(!is_newer("0.3.0-beta.1", "0.3.0"));
    assert!(is_newer("1.0.0", "0.99.99"));
    assert!(!is_newer("0.3.0+ci.7", "0.3.0"));
    assert!(!is_newer("0.3.0", "0.3.0-1-g965cb10"));
    assert!(!is_newer("v0.3.0", "0.3.0-1-g965cb10-dirty"));
    assert!(!is_newer("0.3.0", "0.3.0-dirty"));
    assert!(is_newer("0.3.1", "0.3.0-1-g965cb10"));
}

// ---- shouldCheck ----

const NOW: f64 = 1_800_000_000_000.0;

fn hours(n: f64) -> f64 {
    n * 3_600_000.0
}

#[test]
fn never_checks_without_opt_in() {
    assert!(!should_check(false, None, NOW));
    assert!(!should_check(false, Some(NOW - hours(48.0)), NOW));
}

#[test]
fn the_first_opted_in_launch_checks() {
    assert!(should_check(true, None, NOW));
}

#[test]
fn an_hour_old_check_is_fresh() {
    assert!(!should_check(true, Some(NOW - hours(1.0)), NOW));
}

#[test]
fn a_day_old_check_rechecks() {
    assert!(should_check(true, Some(NOW - hours(25.0)), NOW));
}

#[test]
fn a_clock_set_backwards_does_not_trigger_a_storm() {
    assert!(!should_check(true, Some(NOW + hours(1.0)), NOW));
}

#[test]
fn exactly_at_the_interval_checks() {
    assert!(should_check(true, Some(NOW - CHECK_INTERVAL_MS), NOW));
    assert!(!should_check(
        true,
        Some(NOW - CHECK_INTERVAL_MS + 1.0),
        NOW
    ));
}

#[test]
fn the_interval_is_a_day() {
    assert_eq!(CHECK_INTERVAL_MS, 24.0 * 60.0 * 60.0 * 1000.0);
}

// ---- pickUpdate ----

#[test]
fn a_newer_release_is_offered() {
    assert_eq!(
        pick("0.7.0", "0.6.0"),
        UpdateDecision::Offer {
            offer: true,
            version: "0.7.0".into()
        }
    );
}

#[test]
fn none_in_nothing_offered() {
    assert_eq!(
        pick_update(None),
        UpdateDecision::Refuse {
            offer: false,
            reason: "no update was reported".into()
        }
    );
}

#[test]
fn a_downgrade_to_a_dev_build_is_refused_with_a_reason() {
    let reason = refusal(pick("0.6.0", "0.6.0-1-g965cb10"));
    assert!(reason.to_lowercase().contains("not newer"));
    assert!(reason.contains("0.6.0-1-g965cb10"));
}

#[test]
fn a_malformed_version_is_refused() {
    assert!(
        refusal(pick("not-a-version", "0.6.0"))
            .to_lowercase()
            .contains("could not read")
    );
}

#[test]
fn a_missing_current_version_is_refused() {
    assert!(
        refusal(pick("0.6.0", ""))
            .to_lowercase()
            .contains("could not read")
    );
}

// ---- the recorded answers ----
//
// What the TypeScript engine answered (src/update/compare.ts, through
// bun), once recorded in xtask's tests/data/compare-answers.json and held
// against the window's generated module and sp-app's copy by xtask's
// tests/update_compare.rs until the Rust cutover retired both. The
// questions and the answers, exactly as recorded.

const RECORDED_IS_NEWER: [(&str, &str, bool, &str); 14] = [
    ("0.6.0", "0.6.0-1-g965cb10", false, "0.6.0"),
    ("0.6.0", "0.6.0-1-g965cb10-dirty", false, "0.6.0"),
    ("0.6.0", "0.6.0-dirty", false, "0.6.0"),
    ("0.7.0", "0.6.0-1-g965cb10", true, "0.7.0"),
    ("0.6.1", "0.6.0", true, "0.6.1"),
    ("0.6.0", "0.6.0", false, "0.6.0"),
    ("0.5.4", "0.6.0", false, "0.5.4"),
    ("0.10.0", "0.9.0", true, "0.10.0"),
    ("1.0.0", "0.99.99", true, "1.0.0"),
    ("0.6.0", "0.6.0-beta.1", true, "0.6.0"),
    ("0.6.0", "0.3.0-dev", true, "0.6.0"),
    ("0.6.0-beta.10", "0.6.0-beta.9", true, "0.6.0-beta.10"),
    ("v0.4.0", "0.3.0", true, "0.4.0"),
    ("0.3.0+ci.7", "0.3.0", false, "0.3.0"),
];
const RECORDED_SUFFIXES: [(&str, bool); 6] = [
    ("1-g965cb10", true),
    ("dirty", true),
    ("beta.1", false),
    ("dev", false),
    ("1-g", false),
    ("x-g965cb10", false),
];

#[test]
fn the_recorded_answers_is_newer_and_normalize_version() {
    for (candidate, current, newer, normalized) in RECORDED_IS_NEWER {
        assert_eq!(is_newer(candidate, current), newer, "{candidate} {current}");
        assert_eq!(normalize_version(candidate), normalized, "{candidate}");
    }
}

#[test]
fn the_recorded_answers_is_describe_suffix() {
    for (suffix, answer) in RECORDED_SUFFIXES {
        assert_eq!(is_describe_suffix(suffix), answer, "{suffix}");
    }
}

#[test]
fn the_recorded_answers_pick_update_word_for_word() {
    assert_eq!(refusal(pick_update(None)), "no update was reported");
    assert_eq!(
        pick("0.7.0", "0.6.0"),
        UpdateDecision::Offer {
            offer: true,
            version: "0.7.0".into()
        }
    );
    assert_eq!(
        refusal(pick("0.6.0", "0.6.0-1-g965cb10")),
        "0.6.0 is not newer than the running 0.6.0-1-g965cb10"
    );
    assert_eq!(
        refusal(pick("not-a-version", "0.6.0")),
        "could not read the versions: offered \"not-a-version\", running \"0.6.0\""
    );
    assert_eq!(
        refusal(pick("0.6.0", "")),
        "could not read the versions: offered \"0.6.0\", running \"\""
    );
}

#[test]
fn the_recorded_answers_should_check() {
    let answers: Vec<bool> = [
        (false, None),
        (true, None),
        (true, Some(NOW - hours(1.0))),
        (true, Some(NOW - hours(25.0))),
        (true, Some(NOW + hours(1.0))),
    ]
    .iter()
    .map(|(o, l)| should_check(*o, *l, NOW))
    .collect();
    assert_eq!(answers, [false, true, false, true, false]);
    assert_eq!(CHECK_INTERVAL_MS, 86_400_000.0);
}
