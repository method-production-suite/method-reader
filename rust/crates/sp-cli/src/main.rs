//! `screepub`: screenplay PDF to reflowable EPUB3 (via Fountain), and the
//! verbs that send, export and tune what it makes. A port of src/cli.ts:
//! the same flags, the same help texts, the same `--json` contract (EVERY
//! exit in that mode is one JSON object on stdout), the same progress lines
//! on stderr and the same exit codes.

mod args;
mod convert;
mod out;
mod usage;
mod verbs;

use std::process::ExitCode;

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    // --json is the app's only channel, so the mode is pre-scanned from the
    // raw argv: the argument parser itself can fail before it would have
    // said so.
    let mut io = out::Io::new(argv.iter().any(|a| a == "--json"));
    let result = match verbs::resolve_command(&argv) {
        Some((verb, rest)) => verbs::run_verb(&mut io, verb, rest),
        None => convert::run(&mut io, &argv),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(stop) => ExitCode::from(io.finish(stop)),
    }
}
