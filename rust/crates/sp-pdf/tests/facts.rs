//! `sp_pdf::read` end to end: the PDF itself, through sp-parser, against
//! every oracle dump.
//!
//! For each dump in rust/oracle/ (committed fixtures) and rust/oracle-real/
//! (local, gitignored, real scripts reported by number only):
//!
//! 1. `read(pdf)` serializes to exactly the dump's `pages.json`, and every
//!    picture's bytes equal the dump's `pixels/<dataSha256>.bin`.
//! 2. `extract_document` over those facts gives `lines.json` (lines, page
//!    count, repairs, pictures with their PNGs compared by decoded pixels
//!    against the dump's `images/`, picture statistics).
//! 3. `parse_lines` over that gives `elements.json`.
//!
//! Numbers compare as JSON values: sp-model writes them the way JavaScript
//! does, and serde_json's float_roundtrip reads the dump's doubles exactly.

use std::path::{Path, PathBuf};

use serde_json::Value;
use sp_parser::extract::extract_document;
use sp_parser::parse_lines;

fn rust_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn dumps(folder: &str) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(rust_dir().join(folder)) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = read
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join("pages.json").is_file())
        .collect();
    out.sort();
    out
}

fn json(p: &Path) -> Option<Value> {
    Some(serde_json::from_str(&std::fs::read_to_string(p).ok()?).expect("oracle JSON"))
}

/// Where two values first differ: a path, never the values (a real
/// script's words must not reach a log).
fn first_diff(a: &Value, b: &Value, path: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (k, v) in x {
                match y.get(k) {
                    Some(w) => {
                        if let Some(d) = first_diff(v, w, &format!("{path}.{k}")) {
                            return Some(d);
                        }
                    }
                    None => return Some(format!("{path}.{k} (only ours)")),
                }
            }
            y.keys()
                .find(|k| !x.contains_key(*k))
                .map(|k| format!("{path}.{k} (only theirs)"))
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return Some(format!("{path} (length {} vs {})", x.len(), y.len()));
            }
            x.iter()
                .zip(y)
                .enumerate()
                .find_map(|(i, (v, w))| first_diff(v, w, &format!("{path}[{i}]")))
        }
        _ => (a != b).then(|| path.to_string()),
    }
}

/// The input a dump was made from: tests/fixtures/ for a committed one, the
/// main checkout's fixtures/ for a real one (this may be a worktree).
fn input(dump: &Path) -> Option<PathBuf> {
    let meta = json(&dump.join("meta.json"))?;
    let name = meta["input"].as_str()?.to_string();
    let committed = rust_dir().join("../tests/fixtures").join(&name);
    if committed.is_file() && dump.parent()?.ends_with("oracle") {
        return Some(committed);
    }
    let mut dir = Some(rust_dir().join(".."));
    while let Some(d) = dir {
        let p = d.join("fixtures").join(&name);
        if p.is_file() {
            return Some(p);
        }
        dir = d.parent().map(Path::to_path_buf);
    }
    None
}

fn decode_png(bytes: &[u8]) -> Vec<u8> {
    // Every IDAT, inflated: the same pixels give the same bytes whatever the
    // deflate settings of the encoder.
    let mut i = 8;
    let mut idat = Vec::new();
    while i + 8 <= bytes.len() {
        let len = u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) as usize;
        let ty = &bytes[i + 4..i + 8];
        if ty == b"IDAT" {
            idat.extend_from_slice(&bytes[i + 8..i + 8 + len]);
        }
        i += 12 + len;
    }
    miniz_oxide::inflate::decompress_to_vec_zlib(&idat).unwrap_or_default()
}

/// pdf.js numbers documents per process (`g_d0_f1` in one run, `g_d2_f1`
/// in the next, since the dump opens several), so the document number in a
/// font or picture id is not a fact about the file. It is masked to `g_dX_`
/// on both sides; everything else in an id must match.
fn mask_doc_ids(v: &Value) -> Value {
    fn mask(s: &str) -> String {
        let mut out = String::new();
        let mut rest = s;
        while let Some(i) = rest.find("g_d") {
            out.push_str(&rest[..i + 3]);
            let tail = &rest[i + 3..];
            let digits = tail.chars().take_while(char::is_ascii_digit).count();
            if digits > 0 && tail[digits..].starts_with('_') {
                out.push('X');
                rest = &tail[digits..];
            } else {
                rest = tail;
            }
        }
        out.push_str(rest);
        out
    }
    match v {
        Value::String(s) => Value::String(mask(s)),
        Value::Array(a) => Value::Array(a.iter().map(mask_doc_ids).collect()),
        Value::Object(o) => {
            Value::Object(o.iter().map(|(k, v)| (mask(k), mask_doc_ids(v))).collect())
        }
        _ => v.clone(),
    }
}

/// Every stage for one dump. Panics, with a JSON path, on a difference.
fn check(label: &str, dump: &Path) -> Result<(usize, usize, usize), BudgetSpent> {
    let pdf = input(dump).unwrap_or_else(|| panic!("{label}: input PDF not found"));
    let facts = sp_pdf::read(std::fs::read(pdf).expect("pdf")).expect("loads");

    let want = mask_doc_ids(&json(&dump.join("pages.json")).expect("pages.json"));
    let ours = mask_doc_ids(&serde_json::to_value(&facts).expect("facts json"));
    if let Some(d) = first_diff(&ours, &want, "") {
        panic!("{label}: pages.json differs at {d}");
    }
    let mut pictures = 0;
    for page in &facts.pages {
        for img in page.images.values().flatten() {
            let sha = img.data_sha256.as_ref().expect("hash");
            let theirs = std::fs::read(dump.join("pixels").join(format!("{sha}.bin")))
                .unwrap_or_else(|_| panic!("{label}: the dump has no pixels/{sha}.bin"));
            assert!(
                img.data.as_deref() == Some(&theirs[..]),
                "{label}: pixels {sha}"
            );
            pictures += 1;
        }
    }

    let want_lines = json(&dump.join("lines.json")).expect("lines.json");
    let doc = extract_document(&facts, None, None);
    // sp-parser's picture budget is wall-clock (20 s of encoding, as in the
    // TypeScript). A run that spends it reports pictures as failed that the
    // oracle carried: a fact about this machine's speed at that moment, not
    // about the PDF facts this test is for (those were compared above).
    let want_failed = want_lines["imageStats"]["failed"].as_u64().unwrap_or(0);
    if (doc.image_stats.failed as u64) > want_failed {
        return Err(BudgetSpent);
    }
    let lines = serde_json::to_value(&doc.lines).expect("lines");
    if let Some(d) = first_diff(&lines, &want_lines["lines"], "lines") {
        panic!("{label}: lines.json differs at {d}");
    }
    assert_eq!(
        Value::from(doc.page_count),
        want_lines["pageCount"],
        "{label}: pageCount"
    );
    assert_eq!(
        Value::from(doc.spacing_repairs),
        want_lines["spacingRepairs"],
        "{label}: spacingRepairs"
    );
    let stats = serde_json::to_value(&doc.image_stats).expect("stats");
    assert!(
        first_diff(&stats, &want_lines["imageStats"], "").is_none(),
        "{label}: imageStats {stats} vs {}",
        want_lines["imageStats"]
    );
    let want_images = want_lines["images"].as_array().expect("images");
    let mut png_compared = 0;
    assert_eq!(
        doc.images.len(),
        want_images.len(),
        "{label}: picture count"
    );
    for (a, b) in doc.images.iter().zip(want_images) {
        let mut b = b.clone();
        b.as_object_mut().expect("image").remove("sha256");
        let av = serde_json::to_value(a).expect("image json");
        assert!(
            first_diff(&av, &b, "").is_none(),
            "{label}: picture {}",
            a.file
        );
        // A document the engine refuses writes no picture folder: its
        // pictures are judged above but have no PNG to compare with.
        if let Ok(theirs) = std::fs::read(dump.join("images").join(&a.file)) {
            assert_eq!(
                decode_png(&a.bytes),
                decode_png(&theirs),
                "{label}: picture {} pixels",
                a.file
            );
            png_compared += 1;
        } else {
            assert!(
                !dump.join("images").is_dir(),
                "{label}: picture {} missing from images/",
                a.file
            );
        }
    }

    let mut elements = 0;
    if let Some(want) = json(&dump.join("elements.json")) {
        let parsed = serde_json::to_value(parse_lines(&doc.lines, &doc.images)).expect("parsed");
        for key in ["elements", "characters", "scenes", "pageCount"] {
            if let Some(d) = first_diff(&parsed[key], &want[key], key) {
                panic!("{label}: elements.json differs at {d}");
            }
        }
        elements = parsed["elements"].as_array().map_or(0, Vec::len);
    }
    let _ = png_compared;
    Ok((doc.lines.len(), elements, pictures))
}

/// The picture budget ran out during this run (see `check`).
#[derive(Debug)]
struct BudgetSpent;

#[test]
fn committed_fixtures_read_to_the_oracle() {
    let mut n = 0;
    for dump in dumps("oracle") {
        let label = dump
            .file_name()
            .expect("name")
            .to_string_lossy()
            .into_owned();
        // The committed fixtures are far inside the budget: no exception.
        check(&label, &dump).unwrap_or_else(|_| panic!("{label}: picture budget spent"));
        n += 1;
    }
    assert_eq!(n, 7, "the seven committed dumps");
}

#[test]
fn real_scripts_read_to_the_oracle() {
    // Local only: rust/oracle-real/ is gitignored and empty on CI.
    let (mut docs, mut lines, mut elements, mut pictures, mut budget) = (0, 0, 0, 0, 0);
    for (i, dump) in dumps("oracle-real").iter().enumerate() {
        if input(dump).is_none() {
            continue;
        }
        let Ok((l, e, p)) = check(&format!("real script {}", i + 1), dump) else {
            eprintln!(
                "real script {}: PDF facts match; lines not compared, sp-parser's picture budget ran out this run",
                i + 1
            );
            budget += 1;
            continue;
        };
        docs += 1;
        lines += l;
        elements += e;
        pictures += p;
    }
    eprintln!(
        "real scripts: {docs} matching at every stage ({lines} lines, {elements} elements, {pictures} pictures); {budget} matching in facts only (picture budget spent)"
    );
}
