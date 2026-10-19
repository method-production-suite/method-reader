//! The `screepub` binary, the command's 0.12 name, kept for a few releases:
//! it says once on stderr that the command is now method-reader, then does
//! exactly what method-reader does. Stderr only, so stdout (and with it the
//! `--json` contract) and the exit code are method-reader's own. `mcp` is
//! the exception: it runs without the line, as MCP clients start it.

use std::process::ExitCode;

fn main() -> ExitCode {
    if sp_cli::old_name_notice_wanted() {
        eprintln!("{}", sp_cli::OLD_NAME_NOTICE);
    }
    ExitCode::from(sp_cli::main())
}
