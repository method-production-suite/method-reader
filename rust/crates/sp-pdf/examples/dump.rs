//! Writes what sp-pdf reads from a PDF as JSON, in the oracle's shape.
//!
//!   cargo run -p sp-pdf --example dump -- <in.pdf> <out.json>

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: dump <in.pdf> <out.json>");
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
    let pages = doc.pages();
    let out = serde_json::json!({ "numPages": doc.num_pages(), "pages": pages });
    std::fs::write(&args[2], serde_json::to_string_pretty(&out).expect("json")).expect("write");
}
