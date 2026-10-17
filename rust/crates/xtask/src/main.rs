//! `cargo xtask <command>`: Method Reader's own tooling. The commands live in
//! the library (src/lib.rs lists them); this only hands over the arguments.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    ExitCode::from(xtask::run(&args))
}
