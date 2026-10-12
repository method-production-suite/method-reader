//! `cargo xtask parity`: the TypeScript CLI and the Rust CLI, side by side.
//!
//! For every input (the committed fixtures, then the real scripts) and every
//! row of [`commands`] that takes it, each side gets a scratch folder of its
//! own holding a copy of the input (and its picture folder, for a
//! .fountain), an empty settings folder and an empty library, and the same
//! environment: `SOURCE_DATE_EPOCH` for every clock that reaches a book,
//! `TZ=UTC` for a zip's local-time dates, and a guard path for Calibre so
//! nothing could reach the real one (a row that lets Calibre run gets a
//! scratch COPY of its settings instead). The TypeScript side runs with
//! tools/oracle/freeze-clock.ts preloaded, which is how it honors
//! `SOURCE_DATE_EPOCH`; the Rust CLI honors it itself.
//!
//! Compared: the exit code, stdout (the `--json` answer as values, or a
//! person's text, with each run's scratch folder and every time
//! normalized), stderr where the row asks, and each file the row lists.
//! Committed fixtures report what differs; real scripts report counts only,
//! because a difference quotes the script.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use crate::compare;
use crate::paths;
use crate::table::{CommandRow, EnvValue, Invocation, commands};

/// Matches tools/oracle/freeze-clock.ts and dump.ts: 2026-01-01T00:00:00Z.
const DEFAULT_EPOCH: u64 = 1_767_225_600;

/// Calibre's guard: a path no Calibre can start against.
const CALIBRE_GUARD: &str = "/dev/null/screepub-parity-calibre-guard";
/// `cargo xtask parity --help`.
pub const USAGE: &str = "\
usage: cargo xtask parity [options]

  --committed-only      skip the real scripts in root fixtures/
  --fixture <stem>...   only these committed fixtures (e.g. torture)
  --command <name>...   only these rows of the command table
  --rust-cli <path>     the Rust binary (default: build crates/sp-cli,
                        or $SCREEPUB_RUST_CLI)
  --against-ts          compare the TypeScript CLI with itself, which
                        checks the harness and the frozen clock
  --epoch <seconds>     SOURCE_DATE_EPOCH for both sides
  --keep                keep the scratch folders and print where
";

struct Options {
    committed_only: bool,
    fixtures: Vec<String>,
    commands: Vec<String>,
    rust_cli: Option<PathBuf>,
    against_ts: bool,
    epoch: u64,
    keep: bool,
}

impl Options {
    fn blank() -> Options {
        Options {
            committed_only: false,
            fixtures: Vec::new(),
            commands: Vec::new(),
            rust_cli: None,
            against_ts: false,
            epoch: DEFAULT_EPOCH,
            keep: false,
        }
    }
}

fn parse(args: &[String], rows: &[CommandRow]) -> Result<Options, String> {
    let mut o = Options::blank();
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
        let one = Options {
            commands: vec![c.clone()],
            ..Options::blank()
        };
        if !rows.iter().any(|row| selected(&one, row)) {
            let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
            return Err(format!("parity: no command row {c:?} (rows: {names:?})"));
        }
    }
    Ok(o)
}

/// Whether `--command` names a row: exactly, or by a prefix ending in `*`.
fn selected(opts: &Options, row: &CommandRow) -> bool {
    opts.commands.is_empty()
        || opts.commands.iter().any(|c| match c.strip_suffix('*') {
            Some(prefix) => row.name.starts_with(prefix),
            None => row.name == *c,
        })
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
/// crates/sp-cli in the RELEASE profile, the one that ships: the picture
/// budget is wall-clock, and a debug build would spend it on unoptimized
/// decoding. `Ok(None)` while there is no sp-cli crate.
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
            "--release",
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
    Ok(Some(ws.join("target/release").join(exe)))
}

/// One side's run of one row.
struct Run {
    work: PathBuf,
    exit: Option<i32>,
    answer: Value,
    stderr: String,
}

fn copy_tree(from: &Path, to: &Path, skip: Option<&Path>) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for e in std::fs::read_dir(from).map_err(|e| e.to_string())? {
        let e = e.map_err(|e| e.to_string())?;
        let p = e.path();
        if skip.is_some_and(|s| s == p) {
            continue;
        }
        let t = e.file_type().map_err(|e| e.to_string())?;
        if t.is_dir() {
            copy_tree(&p, &to.join(e.file_name()), skip)?;
        } else if t.is_file() {
            std::fs::copy(&p, to.join(e.file_name())).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
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
            copy_tree(&pictures, &copy.with_extension("images"), None)?;
        }
    }
    Ok(copy)
}

/// The folder Calibre would use on this machine, by Calibre's own rule.
fn real_calibre_config() -> PathBuf {
    if let Ok(set) = std::env::var("CALIBRE_CONFIG_DIRECTORY")
        && !set.trim().is_empty()
        && !set.starts_with("/dev/null")
    {
        return PathBuf::from(set.trim());
    }
    let home = std::env::home_dir().unwrap_or_default();
    if cfg!(target_os = "macos") {
        return home.join("Library/Preferences/calibre");
    }
    if cfg!(windows) {
        return std::env::var_os("APPDATA")
            .map_or_else(|| home.join("AppData").join("Roaming"), PathBuf::from)
            .join("calibre");
    }
    match std::env::var("XDG_CONFIG_HOME") {
        Ok(x) if !x.trim().is_empty() => PathBuf::from(x.trim()).join("calibre"),
        _ => home.join(".config").join("calibre"),
    }
}

/// A scratch copy of the Calibre settings folder (caches left behind).
fn calibre_copy(work: &Path) -> Result<PathBuf, String> {
    let copy = work.join("calibre");
    let real = real_calibre_config();
    if real.is_dir() {
        copy_tree(&real, &copy, Some(&real.join("caches")))?;
    } else {
        std::fs::create_dir_all(&copy).map_err(|e| e.to_string())?;
    }
    Ok(copy)
}

fn normalize_text(text: &str, work: &Path) -> String {
    let v = compare::normalize_json(&Value::String(text.to_owned()), work);
    v.as_str().unwrap_or_default().to_owned()
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
    let inv = Invocation { input: &copy, work };
    if let Some(prepare) = &row.prepare {
        prepare(&inv);
    }
    let mut extra: Vec<(&str, OsString)> = Vec::new();
    for (key, value) in &row.env {
        let v = match value {
            EnvValue::Work(rel) => work.join(rel).into_os_string(),
            EnvValue::Text(t) => OsString::from(t),
            EnvValue::CalibreCopy => calibre_copy(work)?.into_os_string(),
        };
        extra.push((key, v));
    }
    let run = |args: &[OsString]| {
        let mut c = Command::new(&side.program);
        c.args(&side.prefix)
            .args(args)
            // Not the repo: a file there named like a verb would turn a
            // verb row into a conversion. The scratch folder is empty but
            // for what the row put there.
            .current_dir(work.join("in"))
            .env("SCREEPUB_CONFIG_DIR", work.join("config"))
            .env("SCREEPUB_LIBRARY", work.join("library"))
            .env("CALIBRE_CONFIG_DIRECTORY", CALIBRE_GUARD)
            .env("SOURCE_DATE_EPOCH", epoch.to_string())
            .env("TZ", "UTC")
            .env_remove("SCREEPUB_VOLUME_ROOTS")
            .env_remove("SCREEPUB_REMARKABLE_ENDPOINT");
        for (k, v) in &extra {
            c.env(k, v);
        }
        c.output()
            .map_err(|e| format!("could not start {}: {e}", side.program.display()))
    };
    for step in &row.steps {
        run(&step(&inv))?;
    }
    let output = run(&(row.args)(&inv))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let answer = match serde_json::from_str::<Value>(stdout.trim()) {
        Ok(v) if stdout.trim().lines().count() == 1 => compare::normalize_json(&v, work),
        _ => Value::String(format!("<not JSON> {}", normalize_text(&stdout, work))),
    };
    Ok(Run {
        work: work.to_path_buf(),
        exit: output.status.code(),
        answer,
        stderr: normalize_text(&String::from_utf8_lossy(&output.stderr), work),
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
        out.push(("stdout".into(), d));
    }
    if row.stderr
        && let Some(d) = compare::text(ts.stderr.as_bytes(), rs.stderr.as_bytes())
    {
        out.push(("stderr".into(), d));
    }
    for o in &row.outputs {
        let (a, b) = (ts.work.join(&o.file), rs.work.join(&o.file));
        if let Some(d) = compare::files_in(&a, &b, o.compare, (&ts.work, &rs.work)) {
            out.push((o.file.clone(), d));
        }
    }
    out
}

pub fn run(args: &[String]) -> Result<bool, String> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{USAGE}");
        return Ok(true);
    }
    let all = commands();
    let opts = parse(args, &all)?;
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
    let rows: Vec<&CommandRow> = all.iter().filter(|r| selected(&opts, r)).collect();

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
    let (mut runs, mut same) = (0usize, 0usize);
    for input in &committed {
        for row in rows.iter().filter(|r| r.inputs.accepts(input, true)) {
            let diffs = run_case(input, row)?;
            let name = paths::stem(input);
            runs += 1;
            if diffs.is_empty() {
                same += 1;
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
    println!(
        "  committed fixtures: {} inputs, {runs} runs, {same} identical, {} differ",
        committed.len(),
        runs - same
    );

    if !real.is_empty() {
        let (mut cases, mut same) = (0usize, 0usize);
        let mut by_output: BTreeMap<String, usize> = BTreeMap::new();
        for input in &real {
            for row in rows.iter().filter(|r| r.inputs.accepts(input, false)) {
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
