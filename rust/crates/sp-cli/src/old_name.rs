//! The `screepub` binary, the command's 0.12 name, kept for a few releases:
//! it says once on stderr that the command is now method-reader, then does
//! exactly what method-reader does. Stderr only, so stdout (and with it the
//! `--json` contract) and the exit code are method-reader's own.

use std::process::ExitCode;

fn main() -> ExitCode {
    eprintln!("{}", sp_cli::OLD_NAME_NOTICE);
    ExitCode::from(sp_cli::main())
}
