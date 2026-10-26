//! `method-reader mcp`: a Model Context Protocol server over stdio, so coding
//! agents can inspect a screenplay PDF and make editable scripts and e-books
//! from it with typed tools instead of shell commands (docs/mcp.md).
//!
//! **Adapted from Method Budget's server** (method-budget,
//! `crates/method-cli/src/mcp/`, MIT OR Apache-2.0 like this crate). The
//! suite plans one shared `method-mcp` crate in method-libs
//! (SUITE-STANDARDS.md §1), so the protocol layer here is kept a copy of
//! Budget's, not a rewrite: the same revisions and version negotiation, the
//! same JSON-RPC handling and error shapes, the same tool results and the
//! same caching hints. Moving both into that crate should then be a matter
//! of deleting the copies. What differs, and why:
//!
//! - [`protocol`]: Budget's, with Reader's name and instructions, and with
//!   resources and prompts (Budget has neither yet), whose list and read
//!   results carry the same caching hints the standard names.
//! - [`schema`]: Budget's, whole, but for its formula builder.
//! - [`paths`]: Budget's roots and checks, but every path must be absolute
//!   (no path is taken relative to a root), and a file is never replaced
//!   unless the call says so.
//! - [`tools`]: Reader's own. Budget's tools edit budgets through its
//!   engine; these run the `method-reader` command line in this process
//!   ([`crate::run`] with `--json`), the code path the window and the
//!   command line use, so the server holds no second copy of any decision.
//! - [`catalog`]: Reader's resources and prompts.
//!
//! Nothing here opens a network connection or starts a program. Logs go to
//! stderr; stdout carries protocol messages only.

pub mod catalog;
pub mod paths;
pub mod protocol;
pub mod schema;
pub mod tools;

#[cfg(test)]
mod tests;

use std::path::PathBuf;

/// How the server was started.
#[derive(Clone, Debug)]
pub struct Config {
    /// Folders the server may read and write in (at least one).
    pub roots: Vec<PathBuf>,
    /// Hide every tool that writes a file.
    pub read_only: bool,
}

/// Limits on what the server accepts, so a confused or hostile client can't
/// make it use unbounded memory or time.
pub mod limits {
    /// Longest protocol message (one line of stdin), in bytes.
    pub const MAX_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
    /// Largest script the server reads, in bytes.
    pub const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;
    /// Longest free text (a title, an author), in characters.
    pub const MAX_TEXT: usize = 10_000;
    /// Longest name, in characters.
    pub const MAX_NAME: usize = 200;
    /// Longest path argument, in bytes.
    pub const MAX_PATH: usize = 4_096;
    /// Highest page number `preview_page` accepts.
    pub const MAX_PAGE: i64 = 100_000;
}

/// The `mcp` subcommand's help.
pub const USAGE: &str = "method-reader mcp: serve Method Reader's tools to coding agents

Usage:
  method-reader mcp [--root DIR]... [--read-only]

Serves Method Reader's tools over the Model Context Protocol on stdin and
stdout: inspect a screenplay PDF, preview one of its pages as text, turn it
into an editable script (.fountain, .fdx) and make an e-book from it (EPUB,
AZW3, MOBI). Every path a tool is given must be absolute and inside a --root
folder (repeatable; default: the current folder). Nothing is sent anywhere.
Logs go to stderr. See docs/mcp.md.

Options:
  --root <dir>           a folder the tools may read and write in
  --read-only            offer only the tools that write nothing
  -h, --help             show this help
";

/// The `mcp` subcommand's arguments, when `argv` asks for it: the first
/// argument is exactly `mcp` and no file of that name exists (the rule every
/// verb follows, `verbs::resolve_command`).
pub fn command(argv: &[String]) -> Option<&[String]> {
    let first = argv.first()?;
    (first == "mcp" && !std::path::Path::new(first).exists()).then(|| &argv[1..])
}

/// Runs `method-reader mcp <args>` on this process's streams; the exit code.
pub fn run_command(args: &[String]) -> u8 {
    let mut roots: Vec<PathBuf> = Vec::new();
    let mut read_only = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return 0;
            }
            "--read-only" => read_only = true,
            "--root" => match it.next() {
                Some(dir) => roots.push(PathBuf::from(dir)),
                None => return usage_error("--root needs a folder"),
            },
            other => match other.strip_prefix("--root=") {
                Some(dir) => roots.push(PathBuf::from(dir)),
                None => return usage_error(&format!("unexpected argument `{other}`")),
            },
        }
    }
    if roots.is_empty() {
        match std::env::current_dir() {
            Ok(d) => roots.push(d),
            Err(e) => {
                eprintln!("method-reader mcp: current folder: {e}");
                return 1;
            }
        }
    }
    match serve_stdio(Config { roots, read_only }) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("method-reader mcp: {e}");
            1
        }
    }
}

fn usage_error(message: &str) -> u8 {
    eprintln!("method-reader mcp: {message} (see method-reader mcp --help)");
    1
}

/// Runs the server on this process's stdin and stdout until stdin closes.
pub fn serve_stdio(config: Config) -> Result<(), String> {
    let server = protocol::Server::new(&config)?;
    eprintln!(
        "method-reader mcp: serving {} tool(s) over stdio{}; roots: {}",
        server.tool_count(),
        if config.read_only { " (read-only)" } else { "" },
        server
            .roots()
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    let source = protocol::spawn_reader(std::io::stdin());
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    protocol::run(server, source, &mut out).map_err(|e| format!("stdout: {e}"))
}
