//! Writes what sp-pdf reads from a PDF as JSON, in the oracle's shape.
//!
//!   cargo run -p sp-pdf --example dump -- <in.pdf> <out.json> [pixels-dir]

use sha2::{Digest, Sha256};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: dump <in.pdf> <out.json> [pixels-dir]");
        std::process::exit(2);
    }
    let bytes = std::fs::read(&args[1]).expect("read pdf");
    let doc = match sp_pdf::Document::load(bytes) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("load failed: {e:?}");
            std::process::exit(1);
        }
    };
    let pixels = args.get(3);
    if let Some(p) = pixels {
        std::fs::create_dir_all(p).expect("pixels dir");
    }
    let mut reader = doc.reader();
    let mut pages = Vec::new();
    for i in 0..doc.num_pages() {
        let page = reader.page(i);
        let mut images = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for op in page.ops.iter().flatten() {
            if let sp_pdf::PageOp::PaintImage { obj_id, .. } = op {
                if !seen.insert(obj_id.clone()) {
                    continue;
                }
                match reader.image(obj_id) {
                    Some(img) => {
                        let sha = hex(&Sha256::digest(&img.data));
                        if let Some(p) = pixels {
                            std::fs::write(format!("{p}/p{}-{obj_id}.raw", i + 1), &img.data)
                                .expect("write");
                        }
                        images.push(serde_json::json!({
                            "objId": obj_id, "width": img.width, "height": img.height,
                            "kind": img.kind, "len": img.data.len(), "sha256": sha,
                        }));
                    }
                    None => images.push(serde_json::json!({ "objId": obj_id, "missing": true })),
                }
            }
        }
        let mut v = serde_json::to_value(&page).expect("page json");
        v["images"] = serde_json::Value::Array(images);
        pages.push(v);
    }
    let out = serde_json::json!({ "numPages": doc.num_pages(), "pages": pages });
    std::fs::write(&args[2], serde_json::to_string_pretty(&out).expect("json")).expect("write");
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
