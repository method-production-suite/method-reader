//! sp-pdf against pdf.js, on every committed fixture.
//!
//! The answer key is `tests/oracle/<fixture>.json`, written by
//! `oracle/dump.ts` from the pdf.js build the TypeScript engine runs on.
//! Regenerate it with that script whenever pdfjs-dist or a fixture changes.
//!
//! What is compared, per page: the viewport and view box; every text item's
//! string, direction, end-of-line flag, font name, position and widths; every
//! operator the parser walks, with its operands; and every painted image's
//! id, size, kind and pixels (a SHA-256 of the bytes pdf.js would hand over).
//!
//! Tolerance. Strings, flags, names, operators, ids and pixel hashes must
//! match exactly. Positions and widths are compared to 1e-9 points rather
//! than bit for bit, for one reason: pdf.js's `Math.hypot` and Rust's
//! `f64::hypot` both call the platform's libm, which agree on this Mac
//! (checked on 200,000 random pairs) but are not guaranteed to agree in the
//! last bit on every platform CI runs. Every other step is the same IEEE
//! operation in the same order, so on the fixtures the result is in fact
//! bit-exact (`bit_exact` counts it), and 1e-9 is a hundred-millionth of a
//! point, far below anything the parser's thresholds (whole points, tenths)
//! can see. Operator operands are Float32 in pdf.js where they are Float32
//! there (path boxes, form matrices) and must match exactly.

use std::collections::HashSet;
use std::path::PathBuf;

use serde_json::Value;
use sha2::{Digest, Sha256};

const FIXTURES: [&str; 7] = [
    "screenplay",
    "prose",
    "blank-pages",
    "torture",
    "field-station",
    "images",
    "jpx",
];

const TOL: f64 = 1e-9;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn oracle(name: &str) -> Value {
    let p = root().join("tests/oracle").join(format!("{name}.json"));
    serde_json::from_str(&std::fs::read_to_string(p).expect("oracle json")).expect("parse")
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Ours, in the oracle's shape: pages, each with the images it painted.
fn ours(name: &str) -> Value {
    let p = root()
        .join("../../../tests/fixtures")
        .join(format!("{name}.pdf"));
    let doc = sp_pdf::Document::load(std::fs::read(p).expect("fixture")).expect("load");
    let mut reader = doc.reader();
    let mut pages = Vec::new();
    for i in 0..doc.num_pages() {
        let page = reader.page(i);
        let mut images = Vec::new();
        let mut seen = HashSet::new();
        for op in page.ops.iter().flatten() {
            if let sp_pdf::PageOp::PaintImage { obj_id, .. } = op
                && seen.insert(obj_id.clone())
            {
                images.push(match reader.image(obj_id) {
                    Some(img) => serde_json::json!({
                        "objId": obj_id, "width": img.width, "height": img.height,
                        "kind": img.kind, "len": img.data.len(),
                        "sha256": hex(&Sha256::digest(&img.data)),
                    }),
                    None => serde_json::json!({ "objId": obj_id, "missing": true }),
                });
            }
        }
        let mut v = serde_json::to_value(&page).expect("page json");
        v["images"] = Value::Array(images);
        pages.push(v);
    }
    serde_json::json!({ "numPages": doc.num_pages(), "pages": pages })
}

fn close(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => (x - y).abs() <= TOL,
        _ => a == b,
    }
}

/// Numbers compared as numbers (the oracle writes 72, serde writes 72.0).
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

#[derive(Default, Debug)]
struct Tally {
    items: usize,
    bit_exact: usize,
    ops: usize,
    images: usize,
}

fn compare(name: &str, t: &mut Tally) {
    let a = oracle(name);
    let b = ours(name);
    assert_eq!(a["numPages"], b["numPages"], "{name}: page count");
    let pa = a["pages"].as_array().expect("pages");
    let pb = b["pages"].as_array().expect("pages");
    assert_eq!(pa.len(), pb.len(), "{name}: pages read");
    for (x, y) in pa.iter().zip(pb) {
        let n = &x["pageNum"];
        for k in ["width", "height", "rotate"] {
            assert!(close(&x[k], &y[k]), "{name} p{n}: {k} {} vs {}", x[k], y[k]);
        }
        for i in 0..4 {
            assert!(close(&x["view"][i], &y["view"][i]), "{name} p{n}: view");
        }
        let ia = x["items"].as_array().expect("items");
        let ib = y["items"].as_array().expect("items");
        assert_eq!(ia.len(), ib.len(), "{name} p{n}: item count");
        for (j, (u, v)) in ia.iter().zip(ib).enumerate() {
            t.items += 1;
            assert_eq!(u["str"], v["str"], "{name} p{n} item {j}: str");
            assert_eq!(u["dir"], v["dir"], "{name} p{n} item {j}: dir");
            assert_eq!(u["hasEOL"], v["hasEOL"], "{name} p{n} item {j}: hasEOL");
            let fa = &x["fonts"][u["fontName"].as_str().unwrap_or("")];
            let fb = &y["fonts"][v["fontName"].as_str().unwrap_or("")];
            assert_eq!(fa, fb, "{name} p{n} item {j}: font name");
            let mut exact = u["width"] == v["width"] && u["height"] == v["height"];
            assert!(
                close(&u["width"], &v["width"]),
                "{name} p{n} item {j}: width"
            );
            assert!(
                close(&u["height"], &v["height"]),
                "{name} p{n} item {j}: height"
            );
            for k in 0..6 {
                let (p, q) = (&u["transform"][k], &v["transform"][k]);
                assert!(
                    close(p, q),
                    "{name} p{n} item {j}: transform[{k}] {p} vs {q}"
                );
                exact &= p.as_f64() == q.as_f64();
            }
            if exact {
                t.bit_exact += 1;
            }
        }
        let oa = x["ops"].as_array().expect("oracle ops");
        let ob = y["ops"].as_array().expect("our ops");
        assert_eq!(oa.len(), ob.len(), "{name} p{n}: operator count");
        for (j, (u, v)) in oa.iter().zip(ob).enumerate() {
            t.ops += 1;
            assert!(same(u, v), "{name} p{n} op {j}: {u} vs {v}");
        }
        let ma = x["images"].as_array().expect("oracle images");
        let mb = y["images"].as_array().expect("our images");
        assert_eq!(ma.len(), mb.len(), "{name} p{n}: image count");
        for (u, v) in ma.iter().zip(mb) {
            t.images += 1;
            assert!(same(u, v), "{name} p{n}: image {u} vs {v}");
        }
    }
}

#[test]
fn every_fixture_matches_pdfjs() {
    let mut t = Tally::default();
    for f in FIXTURES {
        compare(f, &mut t);
    }
    eprintln!(
        "parity: {} text items ({} bit-exact), {} operators, {} images",
        t.items, t.bit_exact, t.ops, t.images
    );
    assert!(t.items > 1000, "the fixtures carry text");
    assert!(t.images > 20, "the fixtures carry pictures");
}
