//! Port of `src/update/compare.ts`: is this release newer than the build
//! that is running, and may the app ask at all?
//!
//! The updater's transport answers the first question with semver, which
//! is wrong in one constant way: a build past a tag describes itself as
//! `0.6.0-1-g965cb10`, and semver reads that as a PRE-release of 0.6.0, so
//! it offers 0.6.0 as an upgrade. Accepting installs a downgrade on every
//! developer machine. This is the second opinion.
//!
//! No network, no filesystem, no clock.

use std::cmp::Ordering;

use serde::Serialize;

/// `normalizeVersion`: strip a leading `v` and any `+build` metadata; a
/// pre-release suffix is kept.
pub fn normalize_version(version: &str) -> String {
    let mut v = crate::env::js_trim(version);
    if v.starts_with('v') || v.starts_with('V') {
        v = &v[1..];
    }
    match v.find('+') {
        Some(plus) => v[..plus].to_owned(),
        None => v.to_owned(),
    }
}

/// `isDescribeSuffix`: git describe output after the tag (`N-g<hex>`,
/// optionally `-dirty`), or a bare `dirty`.
pub fn is_describe_suffix(pre: &str) -> bool {
    if pre == "dirty" {
        return true;
    }
    let body = pre.strip_suffix("-dirty").unwrap_or(pre);
    let Some(dash) = body.find('-').filter(|&d| d > 0) else {
        return false;
    };
    let count = &body[..dash];
    let hash = &body[dash + 1..];
    if count.is_empty() || !count.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let Some(hex) = hash.strip_prefix('g').or_else(|| hash.strip_prefix('G')) else {
        return false;
    };
    hex.len() >= 4 && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

/// `Number.parseInt(p, 10)`: leading whitespace, a sign, then digits; NaN
/// (`None`) when there are none.
fn parse_int(p: &str) -> Option<f64> {
    let s = crate::env::js_trim_start(p);
    let (neg, rest) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    let n: f64 = digits.parse().ok()?;
    Some(if neg { -n } else { n })
}

/// `parts(version)`: numeric components (NaN as `None`) and the
/// pre-release tag.
fn parts(version: &str) -> (Vec<Option<f64>>, Option<String>) {
    let (core, pre) = match version.find('-') {
        Some(d) => (&version[..d], Some(version[d + 1..].to_owned())),
        None => (version, None),
    };
    (core.split('.').map(parse_int).collect(), pre)
}

fn is_readable(version: &str) -> bool {
    let (nums, _) = parts(&normalize_version(version));
    !nums.is_empty()
        && nums
            .iter()
            .all(|n| n.is_some_and(|n| n >= 0.0 && n.fract() == 0.0 && n.is_finite()))
}

/// `isNewer(candidate, current)`: strictly newer, numerically per
/// component, a release beating its own pre-release, and never past a
/// running build that carries a git-describe suffix.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    let (cand_nums, cand_pre) = parts(&normalize_version(candidate));
    let (curr_nums, curr_pre) = parts(&normalize_version(current));

    let width = cand_nums.len().max(curr_nums.len());
    for i in 0..width {
        // A missing component is zero. NaN never compares equal, and is
        // never greater, as in JavaScript.
        let a = cand_nums.get(i).copied().unwrap_or(Some(0.0));
        let b = curr_nums.get(i).copied().unwrap_or(Some(0.0));
        match (a, b) {
            (Some(a), Some(b)) if a == b => {}
            (Some(a), Some(b)) => return a > b,
            _ => return false,
        }
    }

    match (cand_pre, curr_pre) {
        (None, None) => false,
        (None, Some(curr)) => !is_describe_suffix(&curr),
        (Some(_), None) => false,
        (Some(cand), Some(curr)) => {
            if is_describe_suffix(&curr) {
                return false;
            }
            locale_compare_numeric(&cand, &curr) == Ordering::Greater
        }
    }
}

/// `a.localeCompare(b, undefined, { numeric: true })` for the version tags
/// this compares: runs of digits compare as numbers, letters compare
/// without case first (lower before upper only to break a tie), and
/// punctuation sorts before digits and digits before letters, in ICU's
/// root order.
pub fn locale_compare_numeric(a: &str, b: &str) -> Ordering {
    let ka = collation_key(a);
    let kb = collation_key(b);
    let primary = ka.iter().map(|e| &e.0).cmp(kb.iter().map(|e| &e.0));
    if primary != Ordering::Equal {
        return primary;
    }
    ka.iter().map(|e| e.1).cmp(kb.iter().map(|e| e.1))
}

/// One collation element: its primary weight and its tertiary (case) bit.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Primary {
    Punct(u32),
    Number(Vec<u8>),
    Letter(char),
    Other(char),
}

/// ICU root's order for ASCII punctuation and symbols.
const PUNCT_ORDER: &str = "_-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$";

fn collation_key(s: &str) -> Vec<(Primary, u8)> {
    let mut out = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            let run: String = chars[start..i].iter().collect();
            let trimmed = run.trim_start_matches('0');
            // Shorter numbers first, then digit by digit: numeric order
            // for any length.
            let mut key = vec![u8::try_from(trimmed.len().min(255)).unwrap_or(255)];
            key.extend(trimmed.bytes());
            out.push((Primary::Number(key), 0));
            continue;
        }
        let element = if c.is_alphabetic() {
            let lower = c.to_lowercase().next().unwrap_or(c);
            (Primary::Letter(lower), u8::from(c != lower))
        } else if let Some(at) = PUNCT_ORDER.find(c) {
            (Primary::Punct(u32::try_from(at).unwrap_or(0)), 0)
        } else if c.is_whitespace() {
            (Primary::Punct(0), 0)
        } else {
            (Primary::Other(c), 0)
        };
        out.push(element);
        i += 1;
    }
    out
}

/// What the updater hands over: the release it found and the build that is
/// running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateCandidate {
    pub version: String,
    pub current_version: String,
}

/// `UpdateDecision`: `{ offer: true, version }` or `{ offer: false, reason }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum UpdateDecision {
    Offer { offer: bool, version: String },
    Refuse { offer: bool, reason: String },
}

impl UpdateDecision {
    fn offer(version: String) -> Self {
        UpdateDecision::Offer {
            offer: true,
            version,
        }
    }

    fn refuse(reason: String) -> Self {
        UpdateDecision::Refuse {
            offer: false,
            reason,
        }
    }

    pub fn is_offer(&self) -> bool {
        matches!(self, UpdateDecision::Offer { .. })
    }
}

/// `JSON.stringify(s)` of a string.
fn quoted(s: &str) -> String {
    let mut out = String::new();
    crate::jsjson::write_string(&mut out, s);
    out
}

/// `pickUpdate(candidate)`: the second opinion. Every refusal carries a
/// reason.
pub fn pick_update(candidate: Option<&UpdateCandidate>) -> UpdateDecision {
    let Some(candidate) = candidate else {
        return UpdateDecision::refuse("no update was reported".into());
    };
    let version = &candidate.version;
    let current = &candidate.current_version;
    if !is_readable(version) || !is_readable(current) {
        return UpdateDecision::refuse(format!(
            "could not read the versions: offered {}, running {}",
            quoted(version),
            quoted(current)
        ));
    }
    if !is_newer(version, current) {
        return UpdateDecision::refuse(format!(
            "{version} is not newer than the running {current}"
        ));
    }
    UpdateDecision::offer(version.clone())
}

/// One day, in milliseconds.
pub const CHECK_INTERVAL_MS: f64 = 24.0 * 60.0 * 60.0 * 1000.0;

/// `shouldCheck(optedIn, lastChecked, now)`: whether to make a network
/// request at all. Opt-in first and alone; a clock set backwards reads as
/// fresh, never overdue.
pub fn should_check(opted_in: bool, last_checked: Option<f64>, now: f64) -> bool {
    if !opted_in {
        return false;
    }
    match last_checked {
        None => true,
        Some(last) => now - last >= CHECK_INTERVAL_MS,
    }
}
