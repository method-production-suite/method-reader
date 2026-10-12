//! `src/update/compare.ts` and the logic of `src/cli-update.ts`: the
//! updater's second opinion and its once-a-day rule. Both verbs are
//! OFFLINE: the caller supplies the versions or the timestamps, and nothing
//! here fetches anything.

pub mod compare;

use serde::Serialize;

pub use compare::{UpdateCandidate, UpdateDecision, pick_update, should_check};

/// `updateDecisionCommand`: judge one offered version against the running
/// one. A missing flag is an error (nothing was judged), never a refusal.
pub fn update_decision_command(
    offered: Option<&str>,
    current: Option<&str>,
) -> Result<UpdateDecision, String> {
    let offered =
        offered.ok_or("update-decision needs --offered <version> (the release that was found)")?;
    let current =
        current.ok_or("update-decision needs --current <version> (the build that is running)")?;
    Ok(pick_update(Some(&UpdateCandidate {
        version: offered.to_owned(),
        current_version: current.to_owned(),
    })))
}

/// `{ check }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ShouldCheckAnswer {
    pub check: bool,
}

/// `Number(text)`: JavaScript's string-to-number conversion, which is
/// stricter than `parseInt` (the whole string, trimmed, must be a number)
/// and looser than Rust's (`""` is 0, `0x1f` is hex, `Infinity` parses).
pub fn js_number(text: &str) -> f64 {
    let s = crate::env::js_trim(text);
    if s.is_empty() {
        return 0.0;
    }
    let radix = |prefix: [&str; 2], radix: u32| -> Option<f64> {
        let digits = s
            .strip_prefix(prefix[0])
            .or_else(|| s.strip_prefix(prefix[1]))?;
        if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
            return Some(f64::NAN);
        }
        Some(digits.chars().fold(0.0, |acc, c| {
            acc * f64::from(radix) + f64::from(c.to_digit(radix).unwrap_or(0))
        }))
    };
    if let Some(n) = radix(["0x", "0X"], 16)
        .or_else(|| radix(["0o", "0O"], 8))
        .or_else(|| radix(["0b", "0B"], 2))
    {
        return n;
    }
    let (sign, body) = match s.as_bytes()[0] {
        b'-' => (-1.0, &s[1..]),
        b'+' => (1.0, &s[1..]),
        _ => (1.0, s),
    };
    if body == "Infinity" {
        return sign * f64::INFINITY;
    }
    // A decimal literal: digits, an optional fraction, an optional
    // exponent, and nothing else (Rust would also take "inf" and "nan").
    let valid = {
        let b = body.as_bytes();
        let mut i = 0;
        let int_start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        let mut digits = i - int_start;
        if i < b.len() && b[i] == b'.' {
            i += 1;
            let frac_start = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            digits += i - frac_start;
        }
        let mut ok = digits > 0;
        if ok && i < b.len() && (b[i] == b'e' || b[i] == b'E') {
            i += 1;
            if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
                i += 1;
            }
            let exp_start = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            ok = i > exp_start;
        }
        ok && i == b.len()
    };
    if !valid {
        return f64::NAN;
    }
    body.parse::<f64>().map_or(f64::NAN, |n| sign * n)
}

/// `updateShouldCheckCommand`: whether a network request is allowed at
/// all, right now. A `--last-checked` that is not a number is an error,
/// never "never checked".
pub fn update_should_check_command(
    opted_in: bool,
    last_checked: Option<&str>,
    now: f64,
) -> Result<ShouldCheckAnswer, String> {
    let last = match last_checked {
        None => None,
        Some(text) => {
            let n = js_number(text);
            if !n.is_finite() {
                return Err(format!(
                    "update-should-check: --last-checked must be epoch milliseconds (got \"{text}\")"
                ));
            }
            Some(n)
        }
    };
    Ok(ShouldCheckAnswer {
        check: should_check(opted_in, last, now),
    })
}
