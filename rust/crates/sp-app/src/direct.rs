//! The real engine: every [`Request`] runs as the `method-reader` command line
//! it stands for ([`Request::argv`]), through `sp_cli::run`, in this
//! process. The CLI's verbs dispatch to sp-engine's and sp-device's
//! handlers, so the window and the CLI answer every request with the same
//! code and cannot disagree.
//!
//! The streams are read the way app.js read the sidecar's pipes: stderr
//! lines that are `{"progress": ...}` objects are handed on as they arrive
//! (the stage and percent, the page reached, the first page for the page
//! show), and the last stdout line is the answer.
//!
//! Each call runs on its own thread, so a panic in the engine is one
//! failed call, never a crashed window. [`DirectEngine::in_flight`] counts
//! the calls still running, so a window closed mid-call can let them finish
//! (inventory row W13: the sidecar outlived the window and its atomic
//! writes completed).

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::engine::{Engine, Reply, Request};

/// Each engine call's thread stack: 8 MiB, a main thread's on macOS and
/// Linux.
const ENGINE_STACK: usize = 8 * 1024 * 1024;

enum Line {
    Out(String),
    Err(String),
}

struct Pipes(Sender<Line>);

impl sp_cli::Sink for Pipes {
    fn out(&mut self, line: &str) {
        let _ = self.0.send(Line::Out(line.to_string()));
    }
    fn err(&mut self, line: &str) {
        let _ = self.0.send(Line::Err(line.to_string()));
    }
}

/// The calls running right now; dropping the guard ends one.
struct Running(Arc<AtomicUsize>);

impl Drop for Running {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

#[derive(Default)]
pub struct DirectEngine {
    in_flight: Arc<AtomicUsize>,
}

/// What the run's stdout and stderr said, as the window reads them: the
/// answer is the last stdout line, which in `--json` mode is always one
/// JSON object. (app.js: "the engine did not answer in JSON" otherwise.)
pub fn answer_from(stdout: &[String]) -> Reply {
    let Some(last) = stdout.iter().rev().find(|l| !l.trim().is_empty()) else {
        return Reply::failed("The engine stopped without answering.");
    };
    match serde_json::from_str::<Value>(last) {
        Ok(v) if v.is_object() => Reply::ok(v),
        _ => {
            let shown: String = last.chars().take(300).collect();
            Reply::failed(format!("the engine did not answer in JSON:\n{shown}…"))
        }
    }
}

/// A stderr line's progress object, if it is one.
pub fn progress_of(line: &str) -> Option<Value> {
    let v: Value = serde_json::from_str(line).ok()?;
    v.get("progress").filter(|p| p.is_object()).cloned()
}

impl DirectEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// The counter of calls still running, for the window's close.
    pub fn in_flight(&self) -> Arc<AtomicUsize> {
        self.in_flight.clone()
    }
}

/// Wait until no call is running, at most `cap`. True when they finished.
pub fn wait_for_calls(in_flight: &AtomicUsize, cap: Duration) -> bool {
    let until = Instant::now() + cap;
    while in_flight.load(Ordering::SeqCst) > 0 {
        if Instant::now() >= until {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    true
}

impl Engine for DirectEngine {
    fn run(&self, request: &Request, progress: &mut dyn FnMut(Value)) -> Reply {
        self.in_flight.fetch_add(1, Ordering::SeqCst);
        let running = Running(self.in_flight.clone());
        let argv = request.argv();
        let (tx, rx) = channel();
        let worker = std::thread::Builder::new()
            .name("method-reader-engine".into())
            // The stack the `method-reader` binary's main thread has (std's
            // default for a spawned thread is 2 MiB), so the window and the
            // CLI run the same engine with the same headroom: a stack
            // overflow aborts the process, past any catch_unwind.
            .stack_size(ENGINE_STACK)
            .spawn(move || {
                // The count ends when the engine itself returns, not when
                // the window stops listening.
                let _running = running;
                sp_cli::run(&argv, Box::new(Pipes(tx)))
            });
        let worker = match worker {
            Ok(w) => w,
            Err(e) => return Reply::failed(format!("could not start the engine: {e}")),
        };
        let mut stdout = Vec::new();
        for line in rx {
            match line {
                Line::Out(s) => stdout.push(s),
                Line::Err(s) => {
                    if let Some(p) = progress_of(&s) {
                        progress(p);
                    }
                }
            }
        }
        match worker.join() {
            Ok(_code) => answer_from(&stdout),
            Err(panic) => {
                let why = panic
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_else(|| "it stopped unexpectedly".into());
                Reply::failed(format!("The engine stopped: {why}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_answer_is_the_last_stdout_line() {
        let r = answer_from(&["hello".into(), r#"{"ok":true,"x":1}"#.into()]);
        assert_eq!(r.answer.unwrap()["x"], 1);
        assert!(answer_from(&[]).answer.is_err());
        let not = answer_from(&["plain words".into()]).answer.unwrap_err();
        assert!(not.starts_with("the engine did not answer in JSON:\nplain words"));
    }

    #[test]
    fn only_progress_objects_are_progress() {
        assert_eq!(
            progress_of(r#"{"progress":{"stage":"page","page":1}}"#).unwrap()["page"],
            1
        );
        assert!(progress_of("method-reader: using this script's saved settings").is_none());
        assert!(progress_of(r#"{"ok":true}"#).is_none());
    }

    #[test]
    fn a_closed_window_waits_for_the_call_still_running() {
        let n = Arc::new(AtomicUsize::new(1));
        let done = n.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(120));
            done.fetch_sub(1, Ordering::SeqCst);
        });
        let t = Instant::now();
        assert!(wait_for_calls(&n, Duration::from_secs(5)));
        assert!(t.elapsed() >= Duration::from_millis(100));
        n.fetch_add(1, Ordering::SeqCst);
        assert!(!wait_for_calls(&n, Duration::from_millis(60)), "capped");
    }

    #[test]
    fn the_version_is_the_clis_own() {
        let e = DirectEngine::new();
        let reply = e.run(&Request::Version, &mut |_| {});
        let v = reply.answer.unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(e.in_flight().load(Ordering::SeqCst), 0);
    }
}
