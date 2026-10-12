//! The few JavaScript built-ins whose exact behavior reaches the bytes of a
//! book: number to string, `toFixed`, `Math.round`, `\s`, `trim()` and
//! `String.prototype.length`.

/// JavaScript's `\s` (and the set `trim()` removes): WhiteSpace plus
/// LineTerminator. Not Rust's `char::is_whitespace`, which differs on
/// U+0085 (Unicode whitespace, not JavaScript's) and U+FEFF (the reverse).
pub fn is_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// A JavaScript LineTerminator, where a multiline `^` may match after.
pub fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// `String.prototype.trim()`.
pub fn trim(s: &str) -> &str {
    s.trim_matches(is_space)
}

/// `s.length`: UTF-16 code units.
pub fn length(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// `String(v)` / `${v}` for a number.
pub fn number(v: f64) -> String {
    if v == 0.0 {
        // -0 too: String(-0) is "0".
        return "0".to_owned();
    }
    if v.is_nan() {
        return "NaN".to_owned();
    }
    if v.is_infinite() {
        return if v > 0.0 { "Infinity" } else { "-Infinity" }.to_owned();
    }
    let a = v.abs();
    if (1e-6..1e21).contains(&a) {
        // Rust's Display is the shortest round-trip form without an
        // exponent, which is JavaScript's in this range.
        return format!("{v}");
    }
    // JavaScript's exponent form: "1e+21", "1.5e-7".
    let e = format!("{v:e}");
    match e.split_once('e') {
        Some((m, x)) if !x.starts_with('-') => format!("{m}e+{x}"),
        _ => e,
    }
}

/// `Number.prototype.toFixed(digits)`: the exact decimal value of `v`,
/// rounded half away from zero (the spec's "pick the larger n" on the
/// magnitude), which is not Rust's `{:.N}` on an exact tie.
pub fn to_fixed(v: f64, digits: usize) -> String {
    if !v.is_finite() || v.abs() >= 1e21 {
        return number(v);
    }
    let negative = v < 0.0;
    // Every finite double's decimal expansion ends within 1074 places, so
    // this is exact, not rounded.
    let exact = format!("{:.1100}", v.abs());
    let (int, frac) = exact.split_once('.').unwrap_or((&exact, ""));
    let mut digits_vec: Vec<u8> = int.bytes().chain(frac.bytes().take(digits)).collect();
    let round_up = frac.as_bytes().get(digits).is_some_and(|&d| d >= b'5');
    if round_up {
        let mut i = digits_vec.len();
        loop {
            if i == 0 {
                digits_vec.insert(0, b'1');
                break;
            }
            i -= 1;
            if digits_vec[i] == b'9' {
                digits_vec[i] = b'0';
            } else {
                digits_vec[i] += 1;
                break;
            }
        }
    }
    let split = digits_vec.len() - digits;
    let mut out = String::new();
    if negative {
        // The sign is the input's: (-0.001).toFixed(2) is "-0.00".
        out.push('-');
    }
    out.push_str(std::str::from_utf8(&digits_vec[..split]).unwrap_or("0"));
    if digits > 0 {
        out.push('.');
        out.push_str(std::str::from_utf8(&digits_vec[split..]).unwrap_or(""));
    }
    out
}

/// `Number(s)` for the decimal strings `to_fixed` writes.
pub fn parse_number(s: &str) -> f64 {
    s.parse::<f64>().unwrap_or(f64::NAN)
}

/// `Math.round(v)`: nearest integer, a tie toward +Infinity.
pub fn round(v: f64) -> f64 {
    if !v.is_finite() {
        return v;
    }
    let f = v.floor();
    if v - f >= 0.5 { f + 1.0 } else { f }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_print_as_javascript_prints_them() {
        assert_eq!(number(20.0), "20");
        assert_eq!(number(12.5), "12.5");
        assert_eq!(number(-0.0), "0");
        assert_eq!(number(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(number(1e21), "1e+21");
        assert_eq!(number(1.5e-7), "1.5e-7");
        assert_eq!(number(1e-6), "0.000001");
    }

    #[test]
    fn to_fixed_rounds_the_exact_value_half_up() {
        assert_eq!(to_fixed(1.6, 2), "1.60");
        assert_eq!(to_fixed(0.625, 2), "0.63");
        assert_eq!(to_fixed(0.125, 2), "0.13");
        assert_eq!(to_fixed(1.005, 2), "1.00"); // 1.00499999...
        assert_eq!(to_fixed(0.7 * 1.6, 2), "1.12");
        assert_eq!(to_fixed(9.999, 2), "10.00");
        assert_eq!(to_fixed(2.5, 0), "3");
        assert_eq!(to_fixed(-0.001, 2), "-0.00");
    }

    #[test]
    fn math_round_ties_go_up() {
        assert_eq!(round(8.5), 9.0);
        assert_eq!(round(6.25), 6.0);
        assert_eq!(round(0.499_999_999_999_999_94), 0.0);
        assert_eq!(round(-2.5), -2.0);
    }

    #[test]
    fn javascript_whitespace_is_not_rusts() {
        assert!(is_space('\u{FEFF}'));
        assert!(!is_space('\u{85}'));
        assert_eq!(trim("\u{FEFF} a \u{3000}"), "a");
        assert_eq!(length("a\u{1F600}"), 3);
    }
}
