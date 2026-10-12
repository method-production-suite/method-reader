//! How two runs are compared. Every function answers `None` for "the same"
//! or `Some(detail)` for the first thing that differs. A detail can quote
//! the script's own text, so the caller prints details only for committed
//! (invented) fixtures and only counts for real scripts.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;

use serde_json::Value;

use crate::table::Compare;

/// Keys whose values are times, and so differ between any two runs.
const TIME_KEYS: &[&str] = &["bookDate", "modified", "checkedAt", "lastChecked", "now"];

/// A `--json` answer made comparable: every occurrence of the run's own
/// scratch folder spelled `$WORK` (in both separator styles), and every
/// time (a [`TIME_KEYS`] value, or a string that reads as an ISO-8601
/// date-time) spelled `$TIME`.
pub fn normalize_json(v: &Value, work: &Path) -> Value {
    let roots = path_spellings(work);
    normalize(v, &roots, None)
}

fn path_spellings(work: &Path) -> Vec<String> {
    let s = work.to_string_lossy().into_owned();
    let mut out = vec![s.clone(), s.replace('\\', "/"), s.replace('/', "\\")];
    // macOS hands out /var/... temp paths whose real path is /private/var/...
    if let Ok(real) = work.canonicalize() {
        out.push(real.to_string_lossy().into_owned());
    }
    out.sort_by_key(|r| std::cmp::Reverse(r.len()));
    out.dedup();
    out
}

fn normalize(v: &Value, roots: &[String], key: Option<&str>) -> Value {
    if key.is_some_and(|k| TIME_KEYS.contains(&k)) && !v.is_null() {
        return Value::String("$TIME".into());
    }
    match v {
        Value::String(s) if looks_like_iso_time(s) => Value::String("$TIME".into()),
        Value::String(s) => {
            let mut s = s.clone();
            for r in roots {
                if !r.is_empty() {
                    s = s.replace(r.as_str(), "$WORK");
                }
            }
            Value::String(s)
        }
        Value::Array(a) => Value::Array(a.iter().map(|x| normalize(x, roots, None)).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .map(|(k, x)| (k.clone(), normalize(x, roots, Some(k))))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// `YYYY-MM-DDTHH:MM` at the start of the string.
fn looks_like_iso_time(s: &str) -> bool {
    let b = s.as_bytes();
    let digit = |i: usize| b.get(i).is_some_and(u8::is_ascii_digit);
    b.len() >= 16
        && (0..4).all(digit)
        && b[4] == b'-'
        && digit(5)
        && digit(6)
        && b[7] == b'-'
        && digit(8)
        && digit(9)
        && b[10] == b'T'
        && digit(11)
        && digit(12)
        && b[13] == b':'
        && digit(14)
        && digit(15)
}

/// Two `--json` answers (each already normalized). JSON is compared as
/// values, so key order does not count; the first differing path is named.
pub fn json(ts: &Value, rs: &Value) -> Option<String> {
    first_json_difference(ts, rs, "$")
}

fn first_json_difference(a: &Value, b: &Value, at: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let keys: BTreeSet<&String> = x.keys().chain(y.keys()).collect();
            keys.into_iter().find_map(|k| {
                let here = format!("{at}.{k}");
                match (x.get(k), y.get(k)) {
                    (Some(p), Some(q)) => first_json_difference(p, q, &here),
                    (Some(_), None) => Some(format!("{here}: only in TypeScript")),
                    (None, Some(_)) => Some(format!("{here}: only in Rust")),
                    (None, None) => None,
                }
            })
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return Some(format!("{at}: {} items against {}", x.len(), y.len()));
            }
            x.iter()
                .zip(y)
                .enumerate()
                .find_map(|(i, (p, q))| first_json_difference(p, q, &format!("{at}[{i}]")))
        }
        _ if a == b => None,
        _ => Some(format!("{at}: {a} against {b}")),
    }
}

/// Two output files (or folders) by `how`. Missing on both sides is the same.
pub fn files(ts: &Path, rs: &Path, how: Compare) -> Option<String> {
    match (ts.exists(), rs.exists()) {
        (false, false) => return None,
        (true, false) => return Some("written only by TypeScript".into()),
        (false, true) => return Some("written only by Rust".into()),
        (true, true) => {}
    }
    if how == Compare::Folder {
        return folders(ts, rs);
    }
    let (Ok(a), Ok(b)) = (std::fs::read(ts), std::fs::read(rs)) else {
        return Some("could not be read".into());
    };
    match how {
        Compare::Bytes => bytes(&a, &b),
        Compare::Text => text(&a, &b),
        Compare::ZipEntries => zip_entries(&a, &b),
        Compare::Folder => unreachable!("handled above"),
    }
}

/// Byte for byte; names the first differing offset.
pub fn bytes(a: &[u8], b: &[u8]) -> Option<String> {
    if a == b {
        return None;
    }
    let at = a
        .iter()
        .zip(b)
        .position(|(x, y)| x != y)
        .unwrap_or(a.len().min(b.len()));
    Some(format!(
        "{} bytes against {}, first difference at byte {at}",
        a.len(),
        b.len()
    ))
}

/// Byte for byte, reported as the first differing line.
pub fn text(a: &[u8], b: &[u8]) -> Option<String> {
    if a == b {
        return None;
    }
    let (sa, sb) = (String::from_utf8_lossy(a), String::from_utf8_lossy(b));
    let (la, lb): (Vec<&str>, Vec<&str>) = (sa.split('\n').collect(), sb.split('\n').collect());
    for i in 0..la.len().max(lb.len()) {
        let (x, y) = (la.get(i).copied(), lb.get(i).copied());
        if x != y {
            return Some(format!(
                "line {}: {:?} against {:?}",
                i + 1,
                x.unwrap_or("<end>"),
                y.unwrap_or("<end>")
            ));
        }
    }
    // Same lines, different bytes: only possible through invalid UTF-8.
    bytes(a, b)
}

struct Entry {
    name: String,
    stored: bool,
    data: Vec<u8>,
}

fn read_zip(b: &[u8]) -> Result<Vec<Entry>, String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(b)).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(z.len());
    for i in 0..z.len() {
        let mut f = z.by_index(i).map_err(|e| e.to_string())?;
        let mut data = Vec::new();
        f.read_to_end(&mut data).map_err(|e| e.to_string())?;
        out.push(Entry {
            name: f.name().map_err(|e| e.to_string())?.into_owned(),
            stored: f.compression() == zip::CompressionMethod::Stored,
            data,
        });
    }
    Ok(out)
}

/// Two zips entry by entry: names and order, compression method, contents.
pub fn zip_entries(a: &[u8], b: &[u8]) -> Option<String> {
    let za = match read_zip(a) {
        Ok(z) => z,
        Err(e) => return Some(format!("TypeScript's zip does not read: {e}")),
    };
    let zb = match read_zip(b) {
        Ok(z) => z,
        Err(e) => return Some(format!("Rust's zip does not read: {e}")),
    };
    let na: Vec<&str> = za.iter().map(|e| e.name.as_str()).collect();
    let nb: Vec<&str> = zb.iter().map(|e| e.name.as_str()).collect();
    if na != nb {
        return Some(format!("entries {na:?} against {nb:?}"));
    }
    for (x, y) in za.iter().zip(&zb) {
        if x.stored != y.stored {
            let m = |s: bool| if s { "stored" } else { "deflated" };
            return Some(format!(
                "{}: {} against {}",
                x.name,
                m(x.stored),
                m(y.stored)
            ));
        }
        let is_text = [".xhtml", ".opf", ".css", ".xml", ".ncx", "mimetype"]
            .iter()
            .any(|ext| x.name.ends_with(ext));
        let diff = if is_text {
            text(&x.data, &y.data)
        } else {
            bytes(&x.data, &y.data)
        };
        if let Some(d) = diff {
            return Some(format!("{}: {d}", x.name));
        }
    }
    None
}

/// Two folders: the same file names, each byte for byte.
pub fn folders(a: &Path, b: &Path) -> Option<String> {
    let list = |p: &Path| -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(p)
            .map(|r| {
                r.filter_map(Result::ok)
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    };
    let (la, lb) = (list(a), list(b));
    if la != lb {
        return Some(format!("files {la:?} against {lb:?}"));
    }
    la.iter().find_map(|name| {
        let (x, y) = (
            std::fs::read(a.join(name)).ok()?,
            std::fs::read(b.join(name)).ok()?,
        );
        bytes(&x, &y).map(|d| format!("{name}: {d}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;
    use std::path::PathBuf;

    #[test]
    fn work_paths_and_times_normalize_away() {
        let work = PathBuf::from("/tmp/screepub-parity-1/ts");
        let v = json!({
            "ok": true,
            "epubPath": "/tmp/screepub-parity-1/ts/out/book.epub",
            "bookDate": 1767225600000u64,
            "list": [{ "at": "2026-01-01T00:00:00Z" }],
            "title": "INT. 2026-01-01 is not a time"
        });
        let n = normalize_json(&v, &work);
        assert_eq!(n["epubPath"], json!("$WORK/out/book.epub"));
        assert_eq!(n["bookDate"], json!("$TIME"));
        assert_eq!(n["list"][0]["at"], json!("$TIME"));
        assert_eq!(n["title"], v["title"]);
        assert_eq!(n["ok"], json!(true));
    }

    #[test]
    fn json_differences_name_the_first_path() {
        assert_eq!(
            json(&json!({"a": 1, "b": [1, 2]}), &json!({"b": [1, 2], "a": 1})),
            None
        );
        assert_eq!(
            json(&json!({"a": {"b": [1, 2]}}), &json!({"a": {"b": [1, 3]}})).unwrap(),
            "$.a.b[1]: 2 against 3"
        );
        assert_eq!(
            json(&json!({"a": 1}), &json!({})).unwrap(),
            "$.a: only in TypeScript"
        );
    }

    #[test]
    fn text_names_the_first_differing_line() {
        assert_eq!(text(b"a\nb\n", b"a\nb\n"), None);
        assert_eq!(
            text(b"a\nb\n", b"a\nc\n").unwrap(),
            r#"line 2: "b" against "c""#
        );
    }

    fn zip_of(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, data, stored) in entries {
            let method = if *stored {
                zip::CompressionMethod::Stored
            } else {
                zip::CompressionMethod::Deflated
            };
            let opts = zip::write::SimpleFileOptions::default().compression_method(method);
            w.start_file(*name, opts).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn zips_compare_by_entry_not_by_deflate_stream() {
        let a = zip_of(&[
            ("mimetype", b"application/epub+zip", true),
            ("x.css", b"p{}", false),
        ]);
        let b = zip_of(&[
            ("mimetype", b"application/epub+zip", true),
            ("x.css", b"p{}", false),
        ]);
        assert_eq!(zip_entries(&a, &b), None);

        let other = zip_of(&[
            ("mimetype", b"application/epub+zip", true),
            ("x.css", b"q{}", false),
        ]);
        assert!(
            zip_entries(&a, &other)
                .unwrap()
                .starts_with("x.css: line 1")
        );

        let deflated_mimetype = zip_of(&[
            ("mimetype", b"application/epub+zip", false),
            ("x.css", b"p{}", false),
        ]);
        assert_eq!(
            zip_entries(&a, &deflated_mimetype).unwrap(),
            "mimetype: stored against deflated"
        );

        let reordered = zip_of(&[
            ("x.css", b"p{}", false),
            ("mimetype", b"application/epub+zip", true),
        ]);
        assert!(zip_entries(&a, &reordered).unwrap().starts_with("entries"));
    }
}
