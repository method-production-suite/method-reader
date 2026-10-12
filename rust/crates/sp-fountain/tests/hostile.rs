//! Hostile .fountain input: the tokenizer must answer or refuse, never
//! abort the process or hang.
//!
//! jsre is a recursive backtracking matcher, so a long run of blank lines,
//! lyrics or centered lines recursed once per line and overflowed the stack
//! (an abort no `catch_unwind` sees; the window's engine thread has std's
//! 2 MiB stack and died at about 3,000 blank lines). fountain-js runs the
//! same patterns on V8's irregexp, whose backtrack stack lives on the heap,
//! and answers all three in well under a second. And a few patterns
//! backtrack exponentially in BOTH engines on a crafted title page, so the
//! matcher also carries a step budget and refuses what it cannot finish.

use std::time::{Duration, Instant};

use sp_fountain::tokenizer::tokenize;
use sp_model::TokenType;

/// Every test runs on a thread with the window's engine stack (std's
/// default), which is the smallest stack the tokenizer is ever called on.
fn on_small_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn a_long_run_of_blank_lines_tokenizes() {
    let text = format!("INT. A - DAY\n\nHello.{}End.\n", "\n".repeat(10_000));
    let tokens = on_small_stack(move || tokenize(&text)).unwrap();
    assert!(tokens.iter().any(|t| t.text.as_deref() == Some("End.")));
}

#[test]
fn a_long_lyric_block_tokenizes() {
    let text = format!("INT. A - DAY\n\n{}\n", vec!["~la la"; 10_000].join("\n"));
    let tokens = on_small_stack(move || tokenize(&text)).unwrap();
    assert!(tokens.iter().any(|t| t.kind == TokenType::Lyrics));
}

#[test]
fn a_long_centered_block_tokenizes() {
    let text = format!("INT. A\n\n{}\n", vec![">x<"; 10_000].join("\n"));
    let tokens = on_small_stack(move || tokenize(&text)).unwrap();
    assert!(tokens.iter().any(|t| t.kind == TokenType::Centered));
}

#[test]
fn a_run_too_long_for_any_stack_is_refused_not_an_abort() {
    let text = format!("INT. A - DAY\n\nHello.{}End.\n", "\n".repeat(3_000_000));
    let r = on_small_stack(move || tokenize(&text));
    let e = r.expect_err("refused");
    assert!(e.message.contains("too complex"), "{}", e.message);
}

#[test]
fn an_exponential_title_page_is_refused_in_bounded_time() {
    // `title_page_block` nests `(?:...\S.*)+` inside `(?:...)+`; a key
    // line ending in a separator `.` cannot match backtracks over every way
    // to split the value. fountain-js takes minutes on this too.
    let text = format!("Title: {}\u{2028}", " a".repeat(40));
    let started = Instant::now();
    let r = on_small_stack(move || tokenize(&text));
    assert!(
        started.elapsed() < Duration::from_secs(60),
        "{:?}",
        started.elapsed()
    );
    let e = r.expect_err("refused");
    assert!(e.message.contains("too complex"), "{}", e.message);
}
