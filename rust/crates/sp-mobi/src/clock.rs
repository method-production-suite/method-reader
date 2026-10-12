//! The one clock the Kindle books read.
//!
//! A MOBI and an AZW3 record when they were built: the PalmDB header's
//! creation and modification times, and (for an AZW3 whose EPUB names no
//! date) EXTH 106. The TypeScript read `Date.now()`; the port reads
//! `SOURCE_DATE_EPOCH` first, the reproducible-builds convention the oracle
//! and the parity harness freeze the TypeScript clock with, and the system
//! clock only when it is unset.

use std::time::{SystemTime, UNIX_EPOCH};

/// Seconds since 1970: `SOURCE_DATE_EPOCH` when it is set to a number,
/// otherwise now. A fraction is dropped, as `Math.floor(Date.now() / 1000)`
/// drops one.
pub fn now_seconds() -> u64 {
    if let Ok(raw) = std::env::var("SOURCE_DATE_EPOCH")
        && let Ok(v) = raw.trim().parse::<f64>()
        && v.is_finite()
        && v >= 0.0
    {
        // Non-negative and finite by the checks above; the floor is the
        // point of the cast.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        return v.floor() as u64;
    }
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// `new Date(seconds * 1000).toISOString()` without its milliseconds:
/// `2026-01-01T00:00:00Z`.
pub fn iso_seconds(seconds: u64) -> String {
    let days = seconds / 86_400;
    let rem = seconds % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Howard Hinnant's days-to-civil, for days since 1970-01-01.
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::iso_seconds;

    #[test]
    fn iso_matches_javascript() {
        assert_eq!(iso_seconds(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_seconds(1_767_225_600), "2026-01-01T00:00:00Z");
        assert_eq!(iso_seconds(1_790_858_096), "2026-10-01T12:34:56Z");
        assert_eq!(iso_seconds(951_782_400), "2000-02-29T00:00:00Z");
    }
}
