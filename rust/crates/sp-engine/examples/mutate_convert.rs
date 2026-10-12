//! Robustness probe for the whole conversion: deterministic corruptions of
//! a PDF or a .fountain (truncations, bit flips, spliced garbage, scattered
//! random bytes) run through `convert_pdf` / `convert_fountain` with MOBI
//! and AZW3 built too, and any panic reported. sp-pdf's own probe
//! (sp-pdf/examples/mutate.rs) stops at the PDF layer; this one goes on
//! through the parser, the serializer, the tokenizer and every book maker,
//! because a stranger's file reaches all of them.
//!
//!   cargo run --release -p sp-engine --example mutate_convert -- <in.pdf|in.fountain> [count]
//!
//! Each input runs on a thread with the app's own worker stack size, so a
//! recursion too deep for the window shows here as an abort, not a pass.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::{Duration, Instant};

use sp_engine::convert::{ConvertOptions, convert_fountain, convert_pdf};

fn run(bytes: Vec<u8>, pdf: bool) {
    let opts = ConvertOptions {
        force: true,
        mobi: true,
        ..Default::default()
    };
    let result = if pdf {
        convert_pdf(bytes, opts)
    } else {
        convert_fountain(&String::from_utf8_lossy(&bytes), opts, None)
    };
    match result {
        Ok(r) => {
            let _ = sp_azw3::build_azw3(&r.epub);
        }
        Err(e) if std::env::var_os("MUTATE_SHOW").is_some() => eprintln!("refused: {e}"),
        Err(_) => {}
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
    let pdf = original.starts_with(b"%PDF");
    let count: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(200);
    if std::env::var_os("MUTATE_SHOW").is_none() {
        std::panic::set_hook(Box::new(|_| {}));
    } else {
        std::panic::set_hook(Box::new(|info| {
            eprintln!(
                "panic at {:?}: {}\n{}",
                info.location(),
                info.payload_as_str().unwrap_or(""),
                std::backtrace::Backtrace::force_capture()
            );
        }));
    }
    let only: Option<usize> = std::env::var("MUTATE_ONLY")
        .ok()
        .and_then(|s| s.parse().ok());
    if count == 0 {
        // The input itself, unmutated: a hand-made hostile file.
        let started = Instant::now();
        let ok = std::thread::Builder::new()
            .stack_size(sp_engine_worker_stack())
            .spawn(move || catch_unwind(AssertUnwindSafe(|| run(original, pdf))).is_ok())
            .expect("spawn")
            .join()
            .unwrap_or(false);
        println!(
            "as is: {} in {:?}",
            if ok { "ok" } else { "PANIC" },
            started.elapsed()
        );
        return;
    }
    let mut rng = Rng(0x9e3779b97f4a7c15);
    let mut panics = 0;
    let mut slowest = (Duration::ZERO, 0usize);
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
        if only.is_some_and(|o| o != k) {
            continue;
        }
        if std::env::var_os("MUTATE_SAVE").is_some() && only == Some(k) {
            std::fs::write("mutation.bin", &b).expect("save");
        }
        let started = Instant::now();
        let r = std::thread::Builder::new()
            .stack_size(sp_engine_worker_stack())
            .spawn(move || catch_unwind(AssertUnwindSafe(|| run(b, pdf))).is_ok())
            .expect("spawn")
            .join()
            .unwrap_or(false);
        let took = started.elapsed();
        if took > slowest.0 {
            slowest = (took, k);
        }
        if !r {
            panics += 1;
            if first.is_none() {
                first = Some(format!("mutation {k}"));
            }
        }
    }
    println!(
        "{count} mutations, {panics} panics{}; slowest {:?} (mutation {})",
        first.map(|f| format!(" (first: {f})")).unwrap_or_default(),
        slowest.0,
        slowest.1
    );
}

/// std's default for a spawned thread, which is what sp-app's worker gets.
fn sp_engine_worker_stack() -> usize {
    std::env::var("RUST_MIN_STACK")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2 * 1024 * 1024)
}
