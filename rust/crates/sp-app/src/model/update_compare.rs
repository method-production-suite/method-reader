//! Whether a release is newer: a port of the TypeScript engine's
//! src/update/compare.ts, which the Tauri window's generated
//! update-compare.js was made from. sp-engine's update::compare is the
//! engine's own port; this copy's tests hold it to the same recorded
//! answers.

use std::cmp::Ordering;

pub const CHECK_INTERVAL_MS: f64 = 24.0 * 60.0 * 60.0 * 1000.0;

pub fn normalize_version(version: &str) -> String {
    let mut v = version.trim();
    if let Some(rest) = v.strip_prefix(['v', 'V']) {
        v = rest;
    }
    match v.find('+') {
        Some(plus) => v[..plus].to_string(),
        None => v.to_string(),
    }
}

/// `git describe`'s suffix: `<count>-g<hash>`, `dirty`, or both.
pub fn is_describe_suffix(pre: &str) -> bool {
    if pre == "dirty" {
        return true;
    }
    let body = pre.strip_suffix("-dirty").unwrap_or(pre);
    let Some(dash) = body.find('-').filter(|&d| d > 0) else {
        return false;
    };
    let (count, hash) = (&body[..dash], &body[dash + 1..]);
    if !count.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let Some(hex) = hash.strip_prefix(['g', 'G']) else {
        return false;
    };
    hex.len() >= 4 && hex.chars().all(|c| c.is_ascii_hexdigit())
}

/// The numbers, or None for a part that is not a non-negative integer
/// (JavaScript's parseInt reads a leading run of digits).
fn parts(version: &str) -> (Vec<Option<u64>>, Option<String>) {
    let (core, pre) = match version.find('-') {
        Some(d) => (&version[..d], Some(version[d + 1..].to_string())),
        None => (version, None),
    };
    let nums = core
        .split('.')
        .map(|p| {
            let digits: String = p.chars().take_while(char::is_ascii_digit).collect();
            if digits.is_empty() || p.starts_with('-') {
                None
            } else {
                digits.parse().ok()
            }
        })
        .collect();
    (nums, pre)
}

fn is_readable(version: &str) -> bool {
    let (nums, _) = parts(&normalize_version(version));
    !nums.is_empty() && nums.iter().all(Option::is_some)
}

/// A natural comparison of prerelease tags (localeCompare with numeric).
fn natural(a: &str, b: &str) -> Ordering {
    fn chunks(s: &str) -> Vec<(bool, String)> {
        let mut out: Vec<(bool, String)> = Vec::new();
        for c in s.chars() {
            let digit = c.is_ascii_digit();
            match out.last_mut() {
                Some((d, run)) if *d == digit => run.push(c),
                _ => out.push((digit, c.to_string())),
            }
        }
        out
    }
    let (ca, cb) = (chunks(a), chunks(b));
    for (x, y) in ca.iter().zip(cb.iter()) {
        let ord = match (x, y) {
            ((true, n), (true, m)) => n
                .parse::<u128>()
                .unwrap_or(0)
                .cmp(&m.parse::<u128>().unwrap_or(0)),
            ((_, s), (_, t)) => s.to_lowercase().cmp(&t.to_lowercase()),
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    ca.len().cmp(&cb.len())
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    let (cand, cand_pre) = parts(&normalize_version(candidate));
    let (curr, curr_pre) = parts(&normalize_version(current));
    let width = cand.len().max(curr.len());
    for i in 0..width {
        let a = cand.get(i).copied().flatten().unwrap_or(0);
        let b = curr.get(i).copied().flatten().unwrap_or(0);
        if a != b {
            return a > b;
        }
    }
    match (cand_pre, curr_pre) {
        (None, None) => false,
        (None, Some(cp)) => !is_describe_suffix(&cp),
        (Some(_), None) => false,
        (Some(p), Some(cp)) => {
            if is_describe_suffix(&cp) {
                return false;
            }
            natural(&p, &cp) == Ordering::Greater
        }
    }
}

/// Whether to offer `version` to a build running `current`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pick {
    Offer(String),
    No(String),
}

pub fn pick_update(candidate: Option<(&str, &str)>) -> Pick {
    let Some((version, current)) = candidate else {
        return Pick::No("no update was reported".into());
    };
    if !is_readable(version) || !is_readable(current) {
        return Pick::No(format!(
            "could not read the versions: offered {version:?}, running {current:?}"
        ));
    }
    if !is_newer(version, current) {
        return Pick::No(format!("{version} is not newer than the running {current}"));
    }
    Pick::Offer(version.into())
}

/// The Tauri window's once-a-day throttle, kept with its recorded answers.
/// From 0.13 the window schedules its own quiet checks
/// (`update::Schedule`: two minutes after launch, then every six hours).
pub fn should_check(opted_in: bool, last_checked: Option<f64>, now: f64) -> bool {
    if !opted_in {
        return false;
    }
    match last_checked {
        None => true,
        Some(last) => now - last >= CHECK_INTERVAL_MS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_by_number_and_prefixes_and_metadata_ignored() {
        assert!(is_newer("0.12.0", "0.11.1"));
        assert!(is_newer("v1.0", "0.99.9"));
        assert!(!is_newer("0.11.1+build.5", "0.11.1"));
        assert!(!is_newer("0.11.0", "0.11.1"));
        assert!(is_newer("0.11.1.1", "0.11.1"));
    }

    #[test]
    fn prereleases_and_describe_suffixes() {
        assert!(is_newer("0.12.0", "0.12.0-beta.1"));
        assert!(!is_newer("0.12.0-beta.1", "0.12.0"));
        assert!(is_newer("0.12.0-beta.10", "0.12.0-beta.2"));
        assert!(!is_newer("0.11.1", "0.11.1-3-gabc1234"));
        assert!(!is_newer("0.11.1", "0.11.1-dirty"));
        assert!(is_describe_suffix("12-gdeadbeef-dirty"));
        assert!(!is_describe_suffix("beta-gx"));
    }

    #[test]
    fn a_pick_explains_itself() {
        assert_eq!(
            pick_update(Some(("0.12.0", "0.11.1"))),
            Pick::Offer("0.12.0".into())
        );
        assert_eq!(pick_update(None), Pick::No("no update was reported".into()));
        assert!(
            matches!(pick_update(Some(("latest", "0.11.1"))), Pick::No(r) if r.starts_with("could not read"))
        );
        assert!(
            matches!(pick_update(Some(("0.11.1", "0.11.1"))), Pick::No(r) if r.contains("not newer"))
        );
    }

    #[test]
    fn at_most_once_a_day_and_only_when_asked() {
        assert!(!should_check(false, None, 0.0));
        assert!(should_check(true, None, 0.0));
        assert!(!should_check(true, Some(0.0), CHECK_INTERVAL_MS - 1.0));
        assert!(should_check(true, Some(0.0), CHECK_INTERVAL_MS));
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
    fn the_recorded_answers() {
        for (candidate, current, newer, normalized) in RECORDED_IS_NEWER {
            assert_eq!(is_newer(candidate, current), newer, "{candidate} {current}");
            assert_eq!(normalize_version(candidate), normalized, "{candidate}");
        }
        for (suffix, answer) in RECORDED_SUFFIXES {
            assert_eq!(is_describe_suffix(suffix), answer, "{suffix}");
        }
        assert_eq!(pick_update(None), Pick::No("no update was reported".into()));
        assert_eq!(
            pick_update(Some(("0.7.0", "0.6.0"))),
            Pick::Offer("0.7.0".into())
        );
        assert_eq!(
            pick_update(Some(("0.6.0", "0.6.0-1-g965cb10"))),
            Pick::No("0.6.0 is not newer than the running 0.6.0-1-g965cb10".into())
        );
        assert_eq!(
            pick_update(Some(("not-a-version", "0.6.0"))),
            Pick::No(
                "could not read the versions: offered \"not-a-version\", running \"0.6.0\"".into()
            )
        );
        assert_eq!(
            pick_update(Some(("0.6.0", ""))),
            Pick::No("could not read the versions: offered \"0.6.0\", running \"\"".into())
        );
        let now = 1_800_000_000_000.0;
        let h = 3_600_000.0;
        let checks: Vec<bool> = [
            (false, None),
            (true, None),
            (true, Some(now - h)),
            (true, Some(now - 25.0 * h)),
            (true, Some(now + h)),
        ]
        .iter()
        .map(|(o, l)| should_check(*o, *l, now))
        .collect();
        assert_eq!(checks, [false, true, false, true, false]);
        assert_eq!(CHECK_INTERVAL_MS, 86_400_000.0);
    }
}
