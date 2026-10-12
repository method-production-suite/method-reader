//! `cargo xtask <command>`: Screepub's own tooling.
//!
//! ```text
//! cargo xtask oracle [--committed-only] [--real <dir>]
//! cargo xtask parity [--committed-only] [--fixture <stem>]... [--command <name>]...
//!                    [--rust-cli <path>] [--against-ts] [--epoch <seconds>] [--keep]
//! ```
//!
//! `oracle` regenerates the TypeScript engine's stage dumps (rust/oracle/,
//! and rust/oracle-real/ for the real scripts) by shelling out to
//! tools/oracle/dump-all.ts. `parity` runs the TypeScript CLI and the Rust
//! CLI side by side over every fixture, for every row of
//! [`table::COMMANDS`], and diffs what they answer and write.

mod compare;
mod oracle;
mod parity;
mod paths;
mod table;

use std::process::ExitCode;

const USAGE: &str = "\
usage: cargo xtask <command>

commands:
  oracle   regenerate the TypeScript oracle dumps (bun tools/oracle/dump-all.ts)
           [--committed-only] [--real <dir>]
  parity   run the TypeScript and Rust CLIs over every fixture and diff them
           [--committed-only]      skip the real scripts in root fixtures/
           [--fixture <stem>]...   only these committed fixtures (e.g. torture)
           [--command <name>]...   only these rows of the command table
           [--rust-cli <path>]     the Rust binary (default: build crates/sp-cli,
                                   or $SCREEPUB_RUST_CLI)
           [--against-ts]          compare the TypeScript CLI with itself, which
                                   checks the harness and the frozen clock
           [--epoch <seconds>]     SOURCE_DATE_EPOCH for both sides
           [--keep]                keep the scratch folders and print where
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some((command, rest)) = args.split_first() else {
        eprint!("{USAGE}");
        return ExitCode::from(2);
    };
    let result = match command.as_str() {
        "oracle" => oracle::run(rest),
        "parity" => parity::run(rest),
        "-h" | "--help" | "help" => {
            print!("{USAGE}");
            Ok(true)
        }
        other => Err(format!("unknown command: {other}\n\n{USAGE}")),
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(message) => {
            eprintln!("xtask: {message}");
            ExitCode::from(2)
        }
    }
}
