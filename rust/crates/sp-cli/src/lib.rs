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
    let result = match verbs::resolve_command(argv) {
        Some((verb, rest)) => verbs::run_verb(&mut io, verb, rest),
        None => convert::run(&mut io, argv),
    };
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
