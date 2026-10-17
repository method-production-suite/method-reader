//! `method-reader`: screenplay PDF to reflowable EPUB3 (via Fountain), and the
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

/// What the old command name, `screepub`, says on stderr before it runs.
pub const OLD_NAME_NOTICE: &str =
    "screepub is now called method-reader. This old name will stop working in a later release.";

/// A binary's whole run on this process's argv and streams: the one-time
/// move of the 0.12 folders, the camera-import recovery, then [`run`].
/// Both command names call this.
pub fn main() -> u8 {
    let argv: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    move_legacy_folders();
    recover_camera_import();
    run(&argv, Box::new(Std))
}

/// Move Screepub's settings and library folders to Method Reader's, once
/// (sp_engine::legacy). Never fails: a folder that cannot move stays in use
/// where it is. Call once at startup, from a real run only: it acts on this
/// process's own environment.
pub fn move_legacy_folders() {
    let _ = sp_engine::legacy::move_legacy_folders(
        sp_engine::env::host_platform(),
        &sp_engine::env::Env::process(),
    );
}

/// The app-data folder's name: the bundle identifier, which the window's
/// tests hold to sp_update::IDENTIFIER.
pub use sp_engine::settings::app::APP_DATA_FOLDER;

/// The file `name` in the app's data folder, for this process's own
/// environment (sp_engine::settings::app::app_data_path): the override
/// folder when one is set, else Method Reader's folder, or a 0.12 folder
/// while only that one holds the file. The window keeps its own storage
/// there.
pub fn app_data_file(name: &str) -> std::path::PathBuf {
    sp_engine::settings::app::app_data_path(
        sp_engine::env::host_platform(),
        &sp_engine::env::Env::process(),
        name,
    )
    .into()
}

/// The platform's app-data folder called `name` (an identifier), for this
/// process's environment: where a copy built under another identifier (a
/// handover rehearsal) keeps its own storage.
pub fn app_data_folder_named(name: &str) -> std::path::PathBuf {
    sp_engine::settings::app::app_data_folder_named(
        sp_engine::env::host_platform(),
        &sp_engine::env::Env::process(),
        name,
    )
    .into()
}

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
