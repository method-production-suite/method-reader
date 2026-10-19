//! How two runs are compared. Every function answers `None` for "the same"
//! or `Some(detail)` for the first thing that differs. A detail can quote
//! the script's own text, so the caller prints details only for committed
//! (invented) fixtures and only counts for real scripts.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};

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

/// Every way `work` may be spelled in what a run wrote, longest first.
pub fn path_spellings(work: &Path) -> Vec<String> {
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
            let mut s = pictures_by_content(s);
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

/// Two output files (or folders) by `how`, each side's scratch folder
/// (`works`) spelled `$WORK` in any text or JSON before comparing. Missing
/// on both sides is the same.
pub fn files_in(ts: &Path, rs: &Path, how: Compare, works: (&Path, &Path)) -> Option<String> {
    let text_like = matches!(how, Compare::Text | Compare::Json | Compare::Html);
    if text_like && ts.is_file() && rs.is_file() {
        let (Ok(a), Ok(b)) = (std::fs::read(ts), std::fs::read(rs)) else {
            return Some("could not be read".into());
        };
        let fix = |b: &[u8], work: &Path| {
            let roots = path_spellings(work);
            let mut t = String::from_utf8_lossy(b).into_owned();
            for r in &roots {
                if !r.is_empty() {
                    t = t.replace(r.as_str(), "$WORK");
                }
            }
            t.into_bytes()
        };
        let (a, b) = (fix(&a, works.0), fix(&b, works.1));
        return match how {
            Compare::Json => json_text(&a, &b),
            Compare::Html => html(&a, &b),
            _ => text(&a, &b),
        };
    }
    if how == Compare::Tree && ts.is_dir() && rs.is_dir() {
        return trees_in(ts, rs, works);
    }
    files(ts, rs, how)
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
    if how == Compare::Tree {
        return trees(ts, rs);
    }
    let (Ok(a), Ok(b)) = (std::fs::read(ts), std::fs::read(rs)) else {
        return Some("could not be read".into());
    };
    match how {
        Compare::Text => text(&a, &b),
        Compare::ZipEntries => zip_entries(&a, &b),
        Compare::Folder | Compare::Tree => unreachable!("handled above"),
        Compare::Json => json_text(&a, &b),
        Compare::Html => html(&a, &b),
        Compare::PalmDb => palm_db(&a, &b),
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
        } else if x.name.ends_with(".png") {
            png(&x.data, &y.data)
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
        let d = if name.ends_with(".png") {
            png(&x, &y)
        } else {
            bytes(&x, &y)
        };
        d.map(|d| format!("{name}: {d}"))
    })
}

/// Every file under `dir`, as paths relative to it, sorted.
fn tree_files(dir: &Path) -> Vec<PathBuf> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };
        for e in read.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(base, &p, out);
            } else if let Ok(rel) = p.strip_prefix(base) {
                out.push(rel.to_path_buf());
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

/// Two folder trees: the same relative paths, each file by its extension.
pub fn trees(a: &Path, b: &Path) -> Option<String> {
    trees_in(a, b, (a, b))
}

fn trees_in(a: &Path, b: &Path, works: (&Path, &Path)) -> Option<String> {
    let (la, lb) = (tree_files(a), tree_files(b));
    if la != lb {
        return Some(format!("files {la:?} against {lb:?}"));
    }
    la.iter().find_map(|rel| {
        let p = a.join(rel);
        let q = b.join(rel);
        let name = rel.to_string_lossy();
        let how = match rel.extension().and_then(|e| e.to_str()).unwrap_or("") {
            "epub" => Compare::ZipEntries,
            "mobi" | "azw3" => Compare::PalmDb,
            "json" => Compare::Json,
            "html" => Compare::Html,
            "png" => {
                return png(&std::fs::read(&p).ok()?, &std::fs::read(&q).ok()?)
                    .map(|d| format!("{name}: {d}"));
            }
            _ => Compare::Text,
        };
        files_in(&p, &q, how, works).map(|d| format!("{name}: {d}"))
    })
}

// ---- pictures: compared by what they decode to (PARITY.md, Decisions) ----

/// A PNG's IHDR and its inflated image data (the filtered scanlines: the
/// encoders pick filters by the same rule, so equal pixels are equal
/// scanlines), or `None` for bytes that are not a PNG we can read.
pub fn png_content(b: &[u8]) -> Option<Vec<u8>> {
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    if b.len() < 8 || b[..8] != SIGNATURE {
        return None;
    }
    let mut at = 8;
    let mut header = Vec::new();
    let mut idat = Vec::new();
    while at + 8 <= b.len() {
        let len = u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]) as usize;
        let kind = &b[at + 4..at + 8];
        let data = b.get(at + 8..at + 8 + len)?;
        match kind {
            b"IHDR" => header.extend_from_slice(data),
            b"IDAT" => idat.extend_from_slice(data),
            b"IEND" => break,
            _ => header.extend_from_slice(data),
        }
        at += 12 + len;
    }
    let pixels = miniz_oxide::inflate::decompress_to_vec_zlib(&idat).ok()?;
    header.extend_from_slice(&pixels);
    Some(header)
}

/// Two PNGs by their decoded content; bytes that are not a PNG, by bytes.
pub fn png(a: &[u8], b: &[u8]) -> Option<String> {
    match (png_content(a), png_content(b)) {
        (Some(x), Some(y)) if x == y => None,
        (Some(_), Some(_)) => Some(format!(
            "a PNG of {} bytes against {}, and their pixels differ",
            a.len(),
            b.len()
        )),
        _ => bytes(a, b),
    }
}

/// Two JSON files as values (key order does not count).
pub fn json_text(a: &[u8], b: &[u8]) -> Option<String> {
    match (
        serde_json::from_slice::<Value>(a),
        serde_json::from_slice::<Value>(b),
    ) {
        (Ok(x), Ok(y)) => json(&x, &y),
        _ => text(a, b),
    }
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => u32::from(c - b'A'),
            b'a'..=b'z' => u32::from(c - b'a') + 26,
            b'0'..=b'9' => u32::from(c - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        })
    };
    let bytes: Vec<u8> = s.bytes().filter(|&c| c != b'=').collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            n |= val(c)? << (18 - 6 * i);
        }
        let take = chunk.len().saturating_sub(1);
        for i in 0..take {
            out.push((n >> (16 - 8 * i)) as u8);
        }
    }
    Some(out)
}

/// The text with every inline PNG (`data:image/png;base64,...`) replaced by
/// a digest of what it decodes to.
fn pictures_by_content(t: &str) -> String {
    const MARK: &str = "data:image/png;base64,";
    let mut out = String::with_capacity(t.len());
    let mut rest = t;
    while let Some(i) = rest.find(MARK) {
        out.push_str(&rest[..i + MARK.len()]);
        let after = &rest[i + MARK.len()..];
        let end = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '='))
            .unwrap_or(after.len());
        let data = &after[..end];
        match base64_decode(data).and_then(|b| png_content(&b)) {
            Some(content) => out.push_str(&format!("<pixels {:016x}>", fnv(&content))),
            None => out.push_str(data),
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

fn fnv(b: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &x in b {
        h ^= u64::from(x);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// HTML byte for byte, but each inline PNG by its pixels.
pub fn html(a: &[u8], b: &[u8]) -> Option<String> {
    if a == b {
        return None;
    }
    let (sa, sb) = (String::from_utf8_lossy(a), String::from_utf8_lossy(b));
    text(
        pictures_by_content(&sa).as_bytes(),
        pictures_by_content(&sb).as_bytes(),
    )
}

fn palm_records(b: &[u8]) -> Option<Vec<&[u8]>> {
    let count = usize::from(u16::from_be_bytes([*b.get(76)?, *b.get(77)?]));
    let offsets: Vec<usize> = (0..count)
        .map(|i| {
            let o = 78 + i * 8;
            b.get(o..o + 4)
                .map(|x| u32::from_be_bytes([x[0], x[1], x[2], x[3]]) as usize)
        })
        .collect::<Option<_>>()?;
    let mut out = Vec::with_capacity(count);
    for (i, &start) in offsets.iter().enumerate() {
        let end = offsets.get(i + 1).copied().unwrap_or(b.len());
        out.push(b.get(start..end)?);
    }
    Some(out)
}

/// A MOBI or AZW3 (a PalmDB): the header up to the record list byte for
/// byte, then record by record, each byte for byte, or by its pixels where
/// both are PNGs. The record offsets are not compared: a picture of other
/// bytes moves every record after it.
pub fn palm_db(a: &[u8], b: &[u8]) -> Option<String> {
    if a == b {
        return None;
    }
    let (Some(ra), Some(rb)) = (palm_records(a), palm_records(b)) else {
        return bytes(a, b);
    };
    if a.get(..76) != b.get(..76) {
        return Some(format!(
            "the PalmDB header differs: {}",
            bytes(&a[..76.min(a.len())], &b[..76.min(b.len())]).unwrap_or_default()
        ));
    }
    if ra.len() != rb.len() {
        return Some(format!("{} records against {}", ra.len(), rb.len()));
    }
    for (i, (x, y)) in ra.iter().zip(&rb).enumerate() {
        if x == y {
            continue;
        }
        let d = if png_content(x).is_some() {
            png(x, y)
        } else {
            bytes(x, y)
        };
        if let Some(d) = d {
            return Some(format!("record {i}: {d}"));
        }
    }
    None
}

// ---- summaries: what the frozen answers keep of a file ----
//
// The TypeScript's books are not kept whole: 494 runs of books and their
// picture folders would weigh hundreds of megabytes. A summary keeps, for
// every unit the comparisons above look at, a digest of exactly what they
// compare: a zip's entry names in order, each entry's compression method
// and content (a PNG by its pixels), a PalmDB's header and each record (a
// PNG record by its pixels), a folder's names and each file by its rule,
// a text with each run's scratch folder spelled `$WORK`, JSON as a value
// with its keys sorted. Two files whose summaries are equal are files the
// comparisons above call the same.

/// 16 hex digits of the SHA-256 of `b`.
pub fn digest(b: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(b)
        .iter()
        .take(8)
        .map(|x| format!("{x:02x}"))
        .collect()
}

/// A picture by its pixels, anything else by its bytes.
fn content_digest(b: &[u8]) -> String {
    match png_content(b) {
        Some(px) => format!("px:{}", digest(&px)),
        None => digest(b),
    }
}

/// `v` as JSON text with every object's keys sorted.
pub fn canonical(v: &Value) -> String {
    match v {
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            let fields: Vec<String> = keys
                .iter()
                .map(|k| format!("{}:{}", Value::String((*k).clone()), canonical(&o[*k])))
                .collect();
            format!("{{{}}}", fields.join(","))
        }
        Value::Array(a) => format!(
            "[{}]",
            a.iter().map(canonical).collect::<Vec<_>>().join(",")
        ),
        other => other.to_string(),
    }
}

fn spelled_work(b: &[u8], work: &Path) -> Vec<u8> {
    let mut t = String::from_utf8_lossy(b).into_owned();
    for r in path_spellings(work) {
        if !r.is_empty() {
            t = t.replace(r.as_str(), "$WORK");
        }
    }
    t.into_bytes()
}

/// What [`files_in`] compares of the output at `path`, as a value: `null`
/// for nothing there. `work` is the run's scratch folder.
pub fn summary(path: &Path, how: Compare, work: &Path) -> Value {
    use serde_json::json;
    if !path.exists() {
        return Value::Null;
    }
    if path.is_dir() {
        return match how {
            Compare::Tree => {
                let mut files = serde_json::Map::new();
                for rel in tree_files(path) {
                    let how = match rel.extension().and_then(|e| e.to_str()).unwrap_or("") {
                        "epub" => Compare::ZipEntries,
                        "mobi" | "azw3" => Compare::PalmDb,
                        "json" => Compare::Json,
                        "html" => Compare::Html,
                        "png" => Compare::Folder,
                        _ => Compare::Text,
                    };
                    let key = rel.to_string_lossy().replace('\\', "/");
                    files.insert(key, summary(&path.join(&rel), how, work));
                }
                json!({ "tree": files })
            }
            _ => {
                let mut names: Vec<String> = std::fs::read_dir(path)
                    .map(|r| {
                        r.filter_map(Result::ok)
                            .map(|e| e.file_name().to_string_lossy().into_owned())
                            .collect()
                    })
                    .unwrap_or_default();
                names.sort();
                let mut files = serde_json::Map::new();
                for name in names {
                    let b = std::fs::read(path.join(&name)).unwrap_or_default();
                    files.insert(name, Value::String(content_digest(&b)));
                }
                json!({ "folder": files })
            }
        };
    }
    let Ok(b) = std::fs::read(path) else {
        return json!({ "unreadable": true });
    };
    match how {
        Compare::Text => json!({ "text": digest(&spelled_work(&b, work)) }),
        Compare::Json => {
            let b = spelled_work(&b, work);
            match serde_json::from_slice::<Value>(&b) {
                Ok(v) => json!({ "json": digest(canonical(&v).as_bytes()) }),
                Err(_) => json!({ "text": digest(&b) }),
            }
        }
        Compare::Html => {
            let t = String::from_utf8_lossy(&spelled_work(&b, work)).into_owned();
            json!({ "html": digest(pictures_by_content(&t).as_bytes()) })
        }
        // A lone file under a folder rule: a picture by its pixels.
        Compare::Folder | Compare::Tree => json!({ "file": content_digest(&b) }),
        Compare::ZipEntries => match read_zip(&b) {
            Ok(entries) => {
                let list: Vec<Value> = entries
                    .iter()
                    .map(|e| {
                        let d = if e.name.ends_with(".png") {
                            content_digest(&e.data)
                        } else {
                            digest(&e.data)
                        };
                        json!({ "name": e.name, "stored": e.stored, "content": d })
                    })
                    .collect();
                json!({ "zip": list })
            }
            Err(_) => json!({ "bytes": digest(&b) }),
        },
        Compare::PalmDb => match palm_records(&b) {
            Some(records) => {
                let list: Vec<Value> = records
                    .iter()
                    .map(|r| Value::String(content_digest(r)))
                    .collect();
                json!({ "header": digest(&b[..76.min(b.len())]), "records": list })
            }
            None => json!({ "bytes": digest(&b) }),
        },
    }
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
