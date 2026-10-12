//! Robustness probe: reads deterministic corruptions of a PDF (truncations,
//! byte flips, spliced garbage, decompressed-stream damage is reached
//! through the flips) and reports any panic. The engine parses files it did
//! not write, so a panic anywhere in here is a bug.
//!
//!   cargo run --release -p sp-pdf --example mutate -- <in.pdf> [count]

use std::panic::{AssertUnwindSafe, catch_unwind};

fn read_all(bytes: Vec<u8>) {
    let Ok(doc) = sp_pdf::Document::load(bytes) else {
        return;
    };
    let mut r = doc.reader();
    for i in 0..doc.num_pages().min(40) {
        let p = r.page(i);
        for op in p.ops.iter().flatten() {
            if let sp_pdf::PageOp::PaintImage { obj_id, .. } = op {
                let _ = r.image(obj_id);
            }
        }
    }
}

/// xorshift, so a run is reproducible.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let original = std::fs::read(&args[1]).expect("read");
    let count: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(200);
    if std::env::var_os("MUTATE_SHOW").is_none() {
        std::panic::set_hook(Box::new(|_| {}));
    } else {
        std::panic::set_hook(Box::new(|info| {
            eprintln!("panic at {:?}", info.location());
        }));
    }
    let mut rng = Rng(0x9e3779b97f4a7c15);
    let mut panics = 0;
    let mut first: Option<String> = None;
    for k in 0..count {
        let mut b = original.clone();
        match k % 4 {
            0 => {
                let cut = rng.below(b.len());
                b.truncate(cut);
            }
            1 => {
                for _ in 0..1 + rng.below(20) {
                    let i = rng.below(b.len());
                    b[i] ^= 1 << rng.below(8);
                }
            }
            2 => {
                let i = rng.below(b.len());
                let n = 1 + rng.below(64);
                let junk: Vec<u8> = (0..n).map(|_| rng.next() as u8).collect();
                b.splice(i..i, junk);
            }
            _ => {
                for _ in 0..1 + rng.below(200) {
                    let i = rng.below(b.len());
                    b[i] = rng.next() as u8;
                }
            }
        }
        let r = catch_unwind(AssertUnwindSafe(|| read_all(b)));
        if r.is_err() {
            panics += 1;
            if first.is_none() {
                first = Some(format!("mutation {k}"));
            }
        }
    }
    println!(
        "{count} mutations, {panics} panics{}",
        first.map(|f| format!(" (first: {f})")).unwrap_or_default()
    );
}
