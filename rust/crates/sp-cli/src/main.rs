//! The `method-reader` binary: the library's [`sp_cli::run`] on the
//! process's own argv and streams.

use std::process::ExitCode;

fn main() -> ExitCode {
    ExitCode::from(sp_cli::main())
}
