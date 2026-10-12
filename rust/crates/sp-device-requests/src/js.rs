//! The few JavaScript behaviours the TypeScript Worker leaned on, so the
//! port answers every input the way it did: what `String.prototype.trim`
//! and the regex `\s` call whitespace, `.length` in UTF-16 units,
//! `Number(string)`, and `toISOString().slice(0, 10)`.

/// ECMAScript WhiteSpace and LineTerminator: what `trim()` removes and
/// `\s` matches. Unlike Rust's `char::is_whitespace`, it includes U+FEFF
/// and excludes U+0085.
pub fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'
            | '\u{000A}'
            | '\u{000B}'
            | '\u{000C}'
            | '\u{000D}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// `s.trim()`.
pub fn trim(s: &str) -> &str {
    s.trim_matches(is_js_space)
}

/// `s.length`: UTF-16 code units.
pub fn length(s: &str) -> usize {
    s.encode_utf16().count()
}

/// `s.replace(/\s+/g, ' ')`.
pub fn collapse_space(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_run = false;
    for c in s.chars() {
        if is_js_space(c) {
            if !in_run {
                out.push(' ');
            }
            in_run = true;
        } else {
            out.push(c);
            in_run = false;
        }
    }
    out
}

/// `Number(s)` for a string: trimmed; empty is 0; decimal (with exponent,
/// `Infinity`), or `0x`/`0o`/`0b` integers; anything else NaN.
pub fn number(s: &str) -> f64 {
    let t = trim(s);
    if t.is_empty() {
        return 0.0;
    }
    let radix = |p: &str, r: u32| -> f64 {
        if p.is_empty() || !p.chars().all(|c| c.is_digit(r)) {
            return f64::NAN;
        }
        p.chars().fold(0.0, |acc, c| {
            acc * r as f64 + c.to_digit(r).unwrap_or(0) as f64
        })
    };
    let lower = t.to_ascii_lowercase();
    if let Some(p) = lower.strip_prefix("0x") {
        return radix(p, 16);
    }
    if let Some(p) = lower.strip_prefix("0o") {
        return radix(p, 8);
    }
    if let Some(p) = lower.strip_prefix("0b") {
        return radix(p, 2);
    }
    match t {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    // StrDecimalLiteral: digits, one dot, an exponent; nothing else (Rust's
    // parser would also take "inf", "nan" and friends, which JS does not).
    let body = t.strip_prefix(['+', '-']).unwrap_or(t);
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(i) => (&body[..i], Some(&body[i + 1..])),
        None => (body, None),
    };
    let digits_ok = {
        let (a, b) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        (!a.is_empty() || !b.is_empty())
            && a.chars().all(|c| c.is_ascii_digit())
            && b.chars().all(|c| c.is_ascii_digit())
            && mantissa.matches('.').count() <= 1
    };
    let exp_ok = exponent.is_none_or(|e| {
        let e = e.strip_prefix(['+', '-']).unwrap_or(e);
        !e.is_empty() && e.chars().all(|c| c.is_ascii_digit())
    });
    if !digits_ok || !exp_ok {
        return f64::NAN;
    }
    t.parse().unwrap_or(f64::NAN)
}

/// `/^application\/json\b/i.test(s)`.
pub fn is_json_type(s: &str) -> bool {
    let head = "application/json";
    let Some(start) = s.get(..head.len()) else {
        return false;
    };
    if !start.eq_ignore_ascii_case(head) {
        return false;
    }
    // \b after "n": the next character must not be a word character.
    !s[head.len()..]
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `new Date(ms).toISOString().slice(0, 10)`: the UTC day.
pub fn day_of(unix_ms: f64) -> String {
    let days = (unix_ms / 86_400_000.0).floor() as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_and_collapse_as_javascript() {
        assert_eq!(trim("\u{FEFF} Kobo \u{3000}"), "Kobo");
        assert_eq!(trim("\u{0085}x"), "\u{0085}x", "NEL is not JS whitespace");
        assert_eq!(collapse_space("a \t\n b\u{00A0}c"), "a b c");
        assert_eq!(length("é😀"), 3);
    }

    #[test]
    fn number_as_javascript() {
        for (s, n) in [
            ("", 0.0),
            ("  12 ", 12.0),
            ("0x10", 16.0),
            ("1e3", 1000.0),
            (".5", 0.5),
            ("5.", 5.0),
            ("-0b11", f64::NAN),
        ] {
            let got = number(s);
            assert!(got == n || (got.is_nan() && n.is_nan()), "{s:?} gave {got}");
        }
        for nan in ["abc", "inf", "NaN", "1e", ".", "1.2.3", "0x"] {
            assert!(number(nan).is_nan(), "{nan}");
        }
        assert_eq!(number("Infinity"), f64::INFINITY);
    }

    #[test]
    fn json_type_with_a_word_boundary() {
        for ok in [
            "application/json",
            "Application/JSON; charset=utf-8",
            "application/json-patch",
        ] {
            assert!(is_json_type(ok), "{ok}");
        }
        for bad in ["text/plain", "application/jsonx", " application/json", ""] {
            assert!(!is_json_type(bad), "{bad}");
        }
    }

    #[test]
    fn days_in_utc() {
        assert_eq!(day_of(0.0), "1970-01-01");
        assert_eq!(day_of(1_791_021_600_000.0), "2026-10-03");
        assert_eq!(day_of(1_790_985_599_999.0), "2026-10-02");
        assert_eq!(day_of(951_782_400_000.0), "2000-02-29");
    }
}
