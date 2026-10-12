//! `cargo xtask parity`: the TypeScript CLI and the Rust CLI, side by side.
//!
//! For every input (the committed fixtures, then the real scripts) and every
//! row of [`COMMANDS`], each side gets a scratch folder of its own holding a
//! copy of the input (and its picture folder, for a .fountain), an empty
//! settings folder and an empty library, and the same environment:
//! `SOURCE_DATE_EPOCH` for every clock that reaches a book, `TZ=UTC` for a
//! zip's local-time dates, and a guard path for Calibre so nothing could
//! reach the real one. The TypeScript side runs with
//! tools/oracle/freeze-clock.ts preloaded, which is how it honors
//! `SOURCE_DATE_EPOCH`; the Rust CLI honors it itself.
//!
//! Compared: the exit code, the `--json` answer (as values, with each run's
//! scratch folder and every time normalized), and each file the row lists.
//! Committed fixtures report what differs; real scripts report counts only,
//! because a difference quotes the script.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use crate::compare;
use crate::paths;
use crate::table::{COMMANDS, CommandRow, Invocation};

/// Matches tools/oracle/freeze-clock.ts and dump.ts: 2026-01-01T00:00:00Z.
const DEFAULT_EPOCH: u64 = 1_767_225_600;

struct Options {
    committed_only: bool,
    fixtures: Vec<String>,
    commands: Vec<String>,
    rust_cli: Option<PathBuf>,
    against_ts: bool,
    epoch: u64,
    keep: bool,
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut o = Options {
        committed_only: false,
        fixtures: Vec::new(),
        commands: Vec::new(),
        rust_cli: None,
        against_ts: false,
        epoch: DEFAULT_EPOCH,
        keep: false,
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = |flag: &str| it.next().cloned().ok_or(format!("{flag} needs a value"));
        match a.as_str() {
            "--committed-only" => o.committed_only = true,
            "--fixture" => o.fixtures.push(value("--fixture")?),
            "--command" => o.commands.push(value("--command")?),
            "--rust-cli" => o.rust_cli = Some(PathBuf::from(value("--rust-cli")?)),
            "--against-ts" => o.against_ts = true,
            "--epoch" => {
                o.epoch = value("--epoch")?
                    .parse()
                    .map_err(|_| "--epoch takes whole seconds".to_string())?;
            }
            "--keep" => o.keep = true,
            other => return Err(format!("parity: unknown option {other}")),
        }
    }
    for c in &o.commands {
        if !COMMANDS.iter().any(|row| row.name == c) {
            let names: Vec<&str> = COMMANDS.iter().map(|r| r.name).collect();
            return Err(format!("parity: no command row {c:?} (rows: {names:?})"));
        }
    }
    Ok(o)
}

/// How one side is started.
struct Side {
    label: &'static str,
    program: PathBuf,
    prefix: Vec<OsString>,
}

impl Side {
    fn typescript() -> Side {
        let repo = paths::repo_root();
        Side {
            label: "ts",
            program: paths::bun(),
            prefix: vec![
                "--preload".into(),
                repo.join("tools/oracle/freeze-clock.ts").into_os_string(),
                repo.join("src/cli.ts").into_os_string(),
            ],
        }
    }

    fn rust(program: PathBuf) -> Side {
        Side {
            label: "rs",
            program,
            prefix: Vec::new(),
        }
    }
}

/// The Rust CLI: `--rust-cli`, else `$SCREEPUB_RUST_CLI`, else built from
/// crates/sp-cli. `Ok(None)` while there is no sp-cli crate.
fn rust_cli(explicit: Option<PathBuf>) -> Result<Option<PathBuf>, String> {
    if let Some(p) = explicit.or_else(|| std::env::var_os("SCREEPUB_RUST_CLI").map(PathBuf::from)) {
        return Ok(Some(p));
    }
    let ws = paths::workspace_root();
    if !ws.join("crates/sp-cli/Cargo.toml").is_file() {
        return Ok(None);
    }
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = Command::new(cargo)
        .args([
            "build",
            "--quiet",
            "--package",
            "sp-cli",
            "--bin",
            "screepub",
        ])
        .current_dir(&ws)
        .status()
        .map_err(|e| format!("could not build sp-cli: {e}"))?;
    if !status.success() {
        return Err("sp-cli did not build".into());
    }
    let exe = if cfg!(windows) {
        "screepub.exe"
    } else {
        "screepub"
    };
    Ok(Some(ws.join("target/debug").join(exe)))
}

/// One side's run of one row.
struct Run {
    work: PathBuf,
    exit: Option<i32>,
    answer: Value,
}

fn copy_input(input: &Path, into: &Path) -> Result<PathBuf, String> {
    let name = input.file_name().ok_or("input has no file name")?;
    let copy = into.join(name);
    std::fs::copy(input, &copy).map_err(|e| format!("copy input: {e}"))?;
    // A .fountain's pictures sit in <stem>.images beside it.
    if input
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("fountain"))
    {
        let pictures = input.with_extension("images");
        if pictures.is_dir() {
            let to = copy.with_extension("images");
            std::fs::create_dir_all(&to).map_err(|e| e.to_string())?;
            for e in std::fs::read_dir(&pictures).map_err(|e| e.to_string())? {
                let e = e.map_err(|e| e.to_string())?;
                std::fs::copy(e.path(), to.join(e.file_name())).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(copy)
}

fn run_side(
    side: &Side,
    row: &CommandRow,
    input: &Path,
    work: &Path,
    epoch: u64,
) -> Result<Run, String> {
    for d in ["in", "out", "config", "library"] {
        std::fs::create_dir_all(work.join(d)).map_err(|e| format!("scratch folder: {e}"))?;
    }
    let copy = copy_input(input, &work.join("in"))?;
    let out = work.join("out");
    let args = (row.args)(&Invocation {
        input: &copy,
        out: &out,
    });
    let output = Command::new(&side.program)
        .args(&side.prefix)
        .args(&args)
        .current_dir(paths::repo_root())
        .env("SCREEPUB_CONFIG_DIR", work.join("config"))
        .env("SCREEPUB_LIBRARY", work.join("library"))
        .env(
            "CALIBRE_CONFIG_DIRECTORY",
            "/dev/null/screepub-parity-calibre-guard",
        )
        .env("SOURCE_DATE_EPOCH", epoch.to_string())
        .env("TZ", "UTC")
        .output()
        .map_err(|e| format!("could not start {}: {e}", side.program.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let answer = serde_json::from_str::<Value>(stdout.trim())
        .unwrap_or_else(|_| Value::String(format!("<not JSON> {}", stdout.trim())));
    Ok(Run {
        work: work.to_path_buf(),
        exit: output.status.code(),
        answer: compare::normalize_json(&answer, work),
    })
}

/// What differs between the two runs, as (what, detail) pairs.
fn diff_runs(row: &CommandRow, ts: &Run, rs: &Run) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if ts.exit != rs.exit {
        out.push((
            "exit code".into(),
            format!("{:?} against {:?}", ts.exit, rs.exit),
        ));
    }
    if let Some(d) = compare::json(&ts.answer, &rs.answer) {
        out.push(("--json".into(), d));
    }
    for o in row.outputs {
        let (a, b) = (
            ts.work.join("out").join(o.file),
            rs.work.join("out").join(o.file),
        );
        if let Some(d) = compare::files(&a, &b, o.compare) {
            out.push((o.file.to_string(), d));
        }
    }
    out
}

pub fn run(args: &[String]) -> Result<bool, String> {
    let opts = parse(args)?;
    let ts = Side::typescript();
    let other = if opts.against_ts {
        Side {
            label: "ts2",
            ..Side::typescript()
        }
    } else {
        match rust_cli(opts.rust_cli.clone())? {
            Some(p) => Side::rust(p),
            None => {
                println!(
                    "no Rust CLI yet: crates/sp-cli does not exist. Pass --rust-cli <path>, \
                     or --against-ts to check the harness against the TypeScript CLI alone."
                );
                return Ok(true);
            }
        }
    };
    let rows: Vec<&CommandRow> = COMMANDS
        .iter()
        .filter(|r| opts.commands.is_empty() || opts.commands.iter().any(|c| c == r.name))
        .collect();

    let committed: Vec<PathBuf> = paths::committed_fixtures()
        .into_iter()
        .filter(|p| opts.fixtures.is_empty() || opts.fixtures.contains(&paths::stem(p)))
        .collect();
    // --fixture names committed fixtures, so it also means "no real scripts".
    let real = if opts.committed_only || !opts.fixtures.is_empty() {
        Vec::new()
    } else {
        paths::real_fixtures()
    };

    let scratch = std::env::temp_dir().join(format!("screepub-parity-{}", std::process::id()));
    let mut clean = true;
    let mut case = 0usize;
    let mut run_case = |input: &Path, row: &CommandRow| -> Result<Vec<(String, String)>, String> {
        case += 1;
        let base = scratch.join(format!("{case:04}"));
        let a = run_side(&ts, row, input, &base.join(ts.label), opts.epoch)?;
        let b = run_side(&other, row, input, &base.join(other.label), opts.epoch)?;
        Ok(diff_runs(row, &a, &b))
    };

    println!("parity: TypeScript against {}", other.program.display());
    for input in &committed {
        for row in rows.iter().filter(|r| r.inputs.accepts(input)) {
            let diffs = run_case(input, row)?;
            let name = paths::stem(input);
            if diffs.is_empty() {
                println!("  ok    {name} {}", row.name);
            } else {
                clean = false;
                println!("  DIFF  {name} {}", row.name);
                for (what, detail) in diffs {
                    println!("          {what}: {detail}");
                }
            }
        }
    }

    if !real.is_empty() {
        let (mut cases, mut same) = (0usize, 0usize);
        let mut by_output: BTreeMap<String, usize> = BTreeMap::new();
        for input in &real {
            for row in rows.iter().filter(|r| r.inputs.accepts(input)) {
                cases += 1;
                let diffs = run_case(input, row)?;
                if diffs.is_empty() {
                    same += 1;
                } else {
                    clean = false;
                    for (what, _) in diffs {
                        *by_output.entry(format!("{} {what}", row.name)).or_default() += 1;
                    }
                }
            }
        }
        println!(
            "  real scripts: {} inputs, {cases} runs, {same} identical, {} differ",
            real.len(),
            cases - same
        );
        for (what, n) in by_output {
            println!("          {what}: {n}");
        }
    }

    if opts.keep {
        println!("scratch kept in {}", scratch.display());
    } else {
        let _ = std::fs::remove_dir_all(&scratch);
    }
    println!(
        "{}",
        if clean {
            "parity: clean"
        } else {
            "parity: DIFFERENCES"
        }
    );
    Ok(clean)
}
