//! JSON the way JavaScript reads and writes it.
//!
//! The settings file, a script's sidecar and every `--json` answer are
//! written by `JSON.stringify` in the TypeScript, and existing users' files
//! must read back and write out byte for byte the same after the cutover.
//! serde_json differs from JavaScript in three ways that reach those bytes:
//!
//! - **Numbers.** JavaScript has one number type: `20.0` is written `20`,
//!   `1e21` is written `1e+21`, and an integer past 2^53 loses precision on
//!   the way in. Every number here goes through `f64` and is written by
//!   ECMAScript's `Number::toString`.
//! - **Key order.** A JavaScript object lists integer-like keys (`"0"`,
//!   `"12"`) first, in ascending order, then every other key in insertion
//!   order. `JSON.parse` then `JSON.stringify` therefore moves them, and so
//!   must this.
//! - **Reading.** `readFileSync(path, 'utf8')` replaces bytes that are not
//!   UTF-8 rather than refusing the file; [`read_text`] does the same.
//!
//! Strings are escaped as `JSON.stringify` escapes them: `"`, `\`, and the
//! control characters below U+0020 (`\b \t \n \f \r` by name, the rest as
//! `\u00xx`); everything else, U+2028 included, is written as itself.

use serde_json::{Map, Value};

/// `JSON.parse(text)`, or `None` where JavaScript would throw.
pub fn parse(text: &str) -> Option<Value> {
    serde_json::from_str(text).ok()
}

/// `readFileSync(path, 'utf8')`: the file's text, with bytes that are not
/// UTF-8 replaced by U+FFFD as Node replaces them.
pub fn read_text(path: &std::path::Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(match String::from_utf8(bytes) {
        Ok(text) => text,
        Err(err) => String::from_utf8_lossy(err.as_bytes()).into_owned(),
    })
}

/// `JSON.stringify(value)`.
pub fn stringify(value: &Value) -> String {
    let mut out = String::new();
    write_value(&mut out, value, None, 0);
    out
}

/// `JSON.stringify(value, null, 2)`.
pub fn stringify_pretty(value: &Value) -> String {
    let mut out = String::new();
    write_value(&mut out, value, Some(2), 0);
    out
}

/// `JSON.stringify` of anything serde can serialize (a result struct).
pub fn to_string<T: serde::Serialize>(value: &T) -> String {
    stringify(&serde_json::to_value(value).expect("a result always serializes"))
}

/// Whether a key is an array index as JavaScript orders object keys: the
/// canonical decimal form of an integer below 2^32 - 1.
fn array_index(key: &str) -> Option<u32> {
    if key.is_empty() || key.len() > 10 {
        return None;
    }
    if key.len() > 1 && key.starts_with('0') {
        return None;
    }
    if !key.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: u64 = key.parse().ok()?;
    if n < u64::from(u32::MAX) {
        u32::try_from(n).ok()
    } else {
        None
    }
}

/// An object's keys in the order a JavaScript object holds them.
pub fn js_key_order(map: &Map<String, Value>) -> Vec<&String> {
    let mut indexed: Vec<(u32, &String)> = Vec::new();
    let mut named: Vec<&String> = Vec::new();
    for key in map.keys() {
        match array_index(key) {
            Some(n) => indexed.push((n, key)),
            None => named.push(key),
        }
    }
    indexed.sort_by_key(|(n, _)| *n);
    indexed.into_iter().map(|(_, k)| k).chain(named).collect()
}

fn write_value(out: &mut String, value: &Value, indent: Option<usize>, depth: usize) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&number_to_string(n.as_f64().unwrap_or(f64::NAN))),
        Value::String(s) => write_string(out, s),
        Value::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(out, indent, depth + 1);
                write_value(out, item, indent, depth + 1);
            }
            newline(out, indent, depth);
            out.push(']');
        }
        Value::Object(map) => {
            if map.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (i, key) in js_key_order(map).into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                newline(out, indent, depth + 1);
                write_string(out, key);
                out.push(':');
                if indent.is_some() {
                    out.push(' ');
                }
                write_value(out, &map[key], indent, depth + 1);
            }
            newline(out, indent, depth);
            out.push('}');
        }
    }
}

fn newline(out: &mut String, indent: Option<usize>, depth: usize) {
    if let Some(width) = indent {
        out.push('\n');
        out.extend(std::iter::repeat_n(' ', width * depth));
    }
}

/// `JSON.stringify(string)`.
pub fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// ECMAScript's `Number::toString(10)`: what `String(n)` and
/// `JSON.stringify` write for a number. A value JSON cannot hold (NaN, an
/// infinity) is `null`, as `JSON.stringify` writes it.
pub fn number_to_string(v: f64) -> String {
    if !v.is_finite() {
        return "null".into();
    }
    if v == 0.0 {
        return "0".into();
    }
    let sign = if v < 0.0 { "-" } else { "" };
    // Rust's `{:e}` writes the shortest digits that round-trip, which is
    // the digit string ECMAScript asks for.
    let sci = format!("{:e}", v.abs());
    let (mantissa, exp) = sci.split_once('e').expect("{:e} always has an exponent");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i64;
    let n = exp.parse::<i64>().expect("an integer exponent") + 1;
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let e = n - 1;
        let e = if e >= 0 {
            format!("+{e}")
        } else {
            e.to_string()
        };
        if k == 1 {
            format!("{digits}e{e}")
        } else {
            format!("{}.{}e{e}", &digits[..1], &digits[1..])
        }
    };
    format!("{sign}{body}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_as_javascript_writes_them() {
        assert_eq!(number_to_string(1e21), "1e+21");
        assert_eq!(number_to_string(1e-7), "1e-7");
        assert_eq!(number_to_string(123.456), "123.456");
        assert_eq!(number_to_string(-0.0), "0");
        assert_eq!(
            number_to_string(9_007_199_254_740_994.0),
            "9007199254740994"
        );
        assert_eq!(number_to_string(0.1), "0.1");
        assert_eq!(number_to_string(20.0), "20");
        assert_eq!(number_to_string(1.5e-6), "0.0000015");
        assert_eq!(number_to_string(-2.5e22), "-2.5e+22");
    }

    #[test]
    fn integer_keys_come_first() {
        let v = parse(r#"{"b":1,"2":2,"1":3,"a":{"10":1,"9":2},"01":4}"#).unwrap();
        assert_eq!(
            stringify(&v),
            r#"{"1":3,"2":2,"b":1,"a":{"9":2,"10":1},"01":4}"#
        );
    }

    #[test]
    fn pretty_matches_json_stringify_two_spaces() {
        let v = parse(r#"{"a":[1,{"b":[]}],"c":{},"d":"x\u0001\n"}"#).unwrap();
        assert_eq!(
            stringify_pretty(&v),
            "{\n  \"a\": [\n    1,\n    {\n      \"b\": []\n    }\n  ],\n  \"c\": {},\n  \"d\": \"x\\u0001\\n\"\n}"
        );
    }
}
