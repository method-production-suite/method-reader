//! `screepub`: screenplay PDF to reflowable EPUB3 (via Fountain), and the
//! verbs that send, export and tune what it makes. A port of src/cli.ts:
//! the same flags, the same help texts, the same `--json` contract (EVERY
//! exit in that mode is one JSON object on stdout), the same progress lines
//! on stderr and the same exit codes.
//!
//! The binary and the window both call [`run`]: the window hands it the
//! argv it would have given the sidecar and a [`Sink`] that hears the two
//! streams, so the window and the CLI cannot disagree about any answer.

mod args;
mod convert;
mod out;
mod usage;
mod verbs;

pub use out::{Sink, Std};

/// Run one command line, writing to `sink`; the exit code.
pub fn run(argv: &[String], sink: Box<dyn Sink>) -> u8 {
    // --json is the app's only channel, so the mode is pre-scanned from the
    // raw argv: the argument parser itself can fail before it would have
    // said so.
    let mut io = out::Io::with_sink(argv.iter().any(|a| a == "--json"), sink);
    guarded(&mut io, |io| match verbs::resolve_command(argv) {
        Some((verb, rest)) => verbs::run_verb(io, verb, rest),
        None => convert::run(io, argv),
    })
}

/// `f`, with a panic anywhere in the engine answered as the catch-all
/// `internal` error: in `--json` mode EVERY exit is one JSON object, and the
/// window, which runs this in its own process, reads that answer rather
/// than a dead thread. (Node's top-level catch did the same for a throw.)
fn guarded(io: &mut out::Io, f: impl FnOnce(&mut out::Io) -> out::Run) -> u8 {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&mut *io)))
        .unwrap_or_else(|panic| {
            let why = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                .unwrap_or_else(|| "the engine stopped unexpectedly".to_owned());
            Err(out::Stop::internal(why))
        });
    match result {
        Ok(()) => 0,
        Err(stop) => io.finish(stop),
    }
}

/// If an earlier run paused macOS's camera import for a copy to an MTP
/// Kindle and never turned it back on (it crashed, or was killed), turn it
/// back on now. One look at the disk when there is nothing to do; never
/// fails. Call once at startup.
pub fn recover_camera_import() {
    sp_device::camera_import::recover_camera_import(&verbs::camera_import());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    struct Capture(Arc<Mutex<Vec<String>>>);
    impl Sink for Capture {
        fn out(&mut self, line: &str) {
            self.0.lock().unwrap().push(line.to_owned());
        }
        fn err(&mut self, _: &str) {}
    }

    #[test]
    fn a_panic_is_still_one_json_answer() {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let mut io = out::Io::with_sink(true, Box::new(Capture(lines.clone())));
        let code = guarded(&mut io, |_| panic!("index out of bounds"));
        assert_eq!(code, 1);
        let last = lines.lock().unwrap().last().cloned().expect("an answer");
        let v: serde_json::Value = serde_json::from_str(&last).expect("JSON");
        assert_eq!(v["ok"], false);
        assert_eq!(v["error"]["code"], "internal");
        assert_eq!(v["error"]["message"], "Error: index out of bounds");
    }
}
