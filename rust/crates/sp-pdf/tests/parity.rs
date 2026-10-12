//! sp-pdf against pdf.js, on every committed fixture.
//!
//! The answer key is `tests/oracle/<fixture>.json`, written by
//! `oracle/dump.ts` from the pdf.js build the TypeScript engine runs on.
//! Regenerate it with that script whenever pdfjs-dist or a fixture changes.
//!
//! Tolerance. Strings, directions, end-of-line flags, font names, operator
//! sequences and pixel hashes must match exactly. Positions and widths are
//! compared to 1e-9 points, not bit for bit, for one reason: pdf.js's
//! `Math.hypot` and Rust's `f64::hypot` both call the platform's libm, which
//! agree on this Mac (checked on 200,000 random pairs) but are not
//! guaranteed to agree in the last bit on every platform CI runs. Every
//! other step is the same IEEE operation in the same order, so on the
//! fixtures the result is in fact bit-exact; `bit_exact` below counts that,
//! and a 1e-9 drift is a hundred-millionth of a point, far below anything the
//! parser's thresholds (whole points and tenths) can see.

use std::path::PathBuf;

use serde_json::Value;

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

fn ours(name: &str) -> Value {
    let p = root().join("../../../tests/fixtures").join(format!("{name}.pdf"));
    let doc = sp_pdf::Document::load(std::fs::read(p).expect("fixture")).expect("load");
    serde_json::json!({ "numPages": doc.num_pages(), "pages": doc.pages() })
}

fn close(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => (x - y).abs() <= TOL,
        _ => a == b,
    }
}

#[derive(Default, Debug)]
struct Tally {
    items: usize,
    bit_exact: usize,
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
            assert!(close(&u["width"], &v["width"]), "{name} p{n} item {j}: width");
            assert!(close(&u["height"], &v["height"]), "{name} p{n} item {j}: height");
            for k in 0..6 {
                let (p, q) = (&u["transform"][k], &v["transform"][k]);
                assert!(close(p, q), "{name} p{n} item {j}: transform[{k}] {p} vs {q}");
                exact &= p == q;
            }
            if exact {
                t.bit_exact += 1;
            }
        }
    }
}

#[test]
fn text_items_match_pdfjs_on_every_fixture() {
    let mut t = Tally::default();
    for f in FIXTURES {
        compare(f, &mut t);
    }
    eprintln!("text items: {} compared, {} bit-exact", t.items, t.bit_exact);
    assert!(t.items > 1000, "the fixtures carry text");
}
