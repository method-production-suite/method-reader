//! `cargo xtask parity`: the Rust CLI against the TypeScript CLI's frozen
//! answers.
//!
//! For every input (the committed fixtures, then the real scripts) and every
//! row of [`commands`] that takes it, the Rust CLI gets a scratch folder
//! holding a copy of the input (and its picture folder, for a .fountain),
//! an empty settings folder, an empty library and a home folder of its own
//! (so no answer names a real home), and the environment the
//! TypeScript had when its answer was frozen: `SOURCE_DATE_EPOCH` for every
//! clock that reaches a book, `TZ=UTC` for a zip's local-time dates, and a
//! guard path for Calibre so nothing could reach the real one (a row that
//! lets Calibre run gets a scratch COPY of its settings instead).
//!
//! The TypeScript side is frozen data: rust/oracle/parity/<fixture>.json for
//! the committed fixtures, rust/oracle-real/parity/<script>.json (never
//! committed) for the real scripts. Each holds, per row, the exit code,
//! stdout (the `--json` answer as values, or a person's text, with the
//! scratch folder and every time normalized), stderr, and a summary of each
//! file the row lists ([`compare::summary`]: a digest of exactly what the
//! comparison looks at). Committed fixtures report what differs; real
//! scripts report counts only, because a difference quotes the script.
//!
//! A few rows depend on what is installed (Calibre and its plugin for the
//! KFX rows; Books, Send to Kindle and the mail handler for the route
//! rows). Each answer file records the machine it was frozen on, and those
//! rows are compared only on a machine that reads the same; elsewhere they
//! are counted as skipped.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use crate::compare;
use crate::paths;
use crate::table::{CommandRow, EnvValue, Invocation, commands};

/// The clock every frozen answer was made with: 2026-01-01T00:00:00Z.
const DEFAULT_EPOCH: u64 = 1_767_225_600;

/// Calibre's guard: a path no Calibre can start against.
const CALIBRE_GUARD: &str = "/dev/null/method-reader-parity-calibre-guard";
/// `cargo xtask parity --help`.
pub const USAGE: &str = "\
usage: cargo xtask parity [options]

  --committed-only      skip the real scripts in root fixtures/
  --fixture <stem>...   only these committed fixtures (e.g. torture)
  --command <name>...   only these rows of the command table
  --rust-cli <path>     the Rust binary (default: build crates/sp-cli,
                        or $METHOD_READER_RUST_CLI)
  --keep                keep the scratch folders and print where
  --accept <row>...     a DELIBERATE output change: run only the named rows
                        (or prefixes ending in *) on the committed fixtures,
                        show how each differs from its frozen answer, and
                        rewrite those rows' frozen answers, nothing else.
                        Read the diff, then say why in the commit.
  --oracle-dir <dir>    where the committed fixtures' frozen answers are
                        (default rust/oracle/parity; for tests)
";

struct Options {
    committed_only: bool,
    fixtures: Vec<String>,
    commands: Vec<String>,
    rust_cli: Option<PathBuf>,
    keep: bool,
    /// `--accept`: given, even empty, means accepting.
    accept: Option<Vec<String>>,
    oracle_dir: Option<PathBuf>,
}

impl Options {
    fn blank() -> Options {
        Options {
            committed_only: false,
            fixtures: Vec::new(),
            commands: Vec::new(),
            rust_cli: None,
            keep: false,
            accept: None,
            oracle_dir: None,
        }
    }
}

fn parse(args: &[String], rows: &[CommandRow]) -> Result<Options, String> {
    let mut o = Options::blank();
    let mut it = args.iter().peekable();
    while let Some(a) = it.next() {
        let mut value = |flag: &str| it.next().cloned().ok_or(format!("{flag} needs a value"));
        match a.as_str() {
            "--committed-only" => o.committed_only = true,
            "--fixture" => o.fixtures.push(value("--fixture")?),
            "--command" => o.commands.push(value("--command")?),
            "--rust-cli" => o.rust_cli = Some(PathBuf::from(value("--rust-cli")?)),
            "--oracle-dir" => o.oracle_dir = Some(PathBuf::from(value("--oracle-dir")?)),
            "--keep" => o.keep = true,
            "--accept" => {
                let names = o.accept.get_or_insert_with(Vec::new);
                while let Some(n) = it.next_if(|n| !n.starts_with("--")) {
                    names.push(n.clone());
                }
            }
            other => return Err(format!("parity: unknown option {other}")),
        }
    }
    if let Some(names) = &o.accept {
        if names.is_empty() {
            return Err("parity --accept: name the rows to accept (for example \
                 --accept help help-short); nothing is accepted wholesale"
                .into());
        }
        if let Some(bare) = names.iter().find(|n| n.trim_end_matches('*').is_empty()) {
            return Err(format!(
                "parity --accept: {bare:?} names every row; name the rows to accept"
            ));
        }
        if !o.commands.is_empty() {
            return Err("parity --accept: the rows are --accept's own; leave out --command".into());
        }
    }
    for c in o.commands.iter().chain(o.accept.iter().flatten()) {
        if !rows
            .iter()
            .any(|row| names_row(std::slice::from_ref(c), row))
        {
            let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
            return Err(format!("parity: no command row {c:?} (rows: {names:?})"));
        }
    }
    Ok(o)
}

/// Whether one of `names` names a row: exactly, or by a prefix ending in `*`.
fn names_row(names: &[String], row: &CommandRow) -> bool {
    names.iter().any(|c| match c.strip_suffix('*') {
        Some(prefix) => row.name.starts_with(prefix),
        None => row.name == *c,
    })
}

/// Whether a row runs: `--accept`'s rows when accepting, else `--command`'s
/// (all rows when it is not given).
fn selected(opts: &Options, row: &CommandRow) -> bool {
    match &opts.accept {
        Some(names) => names_row(names, row),
        None => opts.commands.is_empty() || names_row(&opts.commands, row),
    }
}

/// How one side is started.
struct Side {
    label: &'static str,
    program: PathBuf,
    prefix: Vec<OsString>,
}

impl Side {
    fn rust(program: PathBuf) -> Side {
        Side {
            label: "rs",
            program,
            prefix: Vec::new(),
        }
    }
}

/// The Rust CLI: `--rust-cli`, else `$METHOD_READER_RUST_CLI`, else built from
/// crates/sp-cli in the RELEASE profile, the one that ships: the picture
/// budget is wall-clock, and a debug build would spend it on unoptimized
/// decoding. `Ok(None)` while there is no sp-cli crate.
fn rust_cli(explicit: Option<PathBuf>) -> Result<Option<PathBuf>, String> {
    if let Some(p) =
        explicit.or_else(|| std::env::var_os("METHOD_READER_RUST_CLI").map(PathBuf::from))
    {
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
            sp_release::names::CLI_EXE,
        ])
        .current_dir(&ws)
        .status()
        .map_err(|e| format!("could not build sp-cli: {e}"))?;
    if !status.success() {
        return Err("sp-cli did not build".into());
    }
    let exe = format!(
        "{}{}",
        sp_release::names::CLI_EXE,
        std::env::consts::EXE_SUFFIX
    );
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
    for d in ["in", "out", "config", "library", "home"] {
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
            .env("METHOD_READER_CONFIG_DIR", work.join("config"))
            .env("METHOD_READER_LIBRARY", work.join("library"))
            .env("CALIBRE_CONFIG_DIRECTORY", CALIBRE_GUARD)
            .env("SOURCE_DATE_EPOCH", epoch.to_string())
            .env("TZ", "UTC")
            // A home of its own: a frozen answer names no one's home folder.
            .env("HOME", work.join("home"))
            .env("USERPROFILE", work.join("home"))
            .env_remove("METHOD_READER_VOLUME_ROOTS")
            .env_remove("METHOD_READER_REMARKABLE_ENDPOINT");
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

/// The version the TypeScript reported when its answers were frozen (the
/// cutover). Only the rows that print the version itself carry it.
pub const FROZEN_VERSION: &str = "0.11.1";

/// The rows whose whole answer is the program's own version.
const VERSION_ROWS: [&str; 2] = ["version", "version-json"];

/// `frozen` with the version frozen at the cutover read as this build's
/// version, for the rows that print it. Every release bumps the version, so
/// without this the first release after the cutover failed parity on
/// `--version` alone (v0.12.1). Every other row stays exact.
fn with_this_version(row: &CommandRow, frozen: &Value) -> Value {
    if !VERSION_ROWS.contains(&row.name.as_str()) {
        return frozen.clone();
    }
    let text = frozen
        .to_string()
        .replace(FROZEN_VERSION, env!("CARGO_PKG_VERSION"));
    serde_json::from_str(&text).unwrap_or_else(|_| frozen.clone())
}

/// The other direction, for `--accept`: a version row's answer with this
/// build's version written back as [`FROZEN_VERSION`], so the accepted row
/// still compares through [`with_this_version`] after the next bump. Any
/// other row is written exactly as it ran.
pub fn at_frozen_version(row_name: &str, ran: &Value) -> Value {
    if !VERSION_ROWS.contains(&row_name) {
        return ran.clone();
    }
    let text = ran
        .to_string()
        .replace(env!("CARGO_PKG_VERSION"), FROZEN_VERSION);
    serde_json::from_str(&text).unwrap_or_else(|_| ran.clone())
}

/// What differs between the frozen answer and the Rust run, as (what,
/// detail) pairs.
fn diff_frozen(row: &CommandRow, ts: &Value, rs: &Run) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let exit = ts["exit"].as_i64().map(|c| c as i32);
    if exit != rs.exit {
        out.push((
            "exit code".into(),
            format!("{exit:?} against {:?}", rs.exit),
        ));
    }
    let frozen_stdout = with_this_version(row, &ts["stdout"]);
    if let Some(d) = compare::json(&frozen_stdout, &rs.answer) {
        out.push(("stdout".into(), d));
    }
    if row.stderr
        && let Some(d) = compare::text(
            ts["stderr"].as_str().unwrap_or_default().as_bytes(),
            rs.stderr.as_bytes(),
        )
    {
        out.push(("stderr".into(), d));
    }
    for o in &row.outputs {
        let want = ts["outputs"].get(&o.file).cloned().unwrap_or(Value::Null);
        let got = compare::summary(&rs.work.join(&o.file), o.compare, &rs.work);
        let d = match (&want, &got) {
            (Value::Null, Value::Null) => None,
            (_, Value::Null) => Some("written only by TypeScript".to_owned()),
            (Value::Null, _) => Some("written only by Rust".to_owned()),
            _ => compare::json(&want, &got),
        };
        if let Some(d) = d {
            out.push((o.file.clone(), d));
        }
    }
    out
}

/// Rows whose answers depend on what this machine has installed.
fn machine_bound(row: &CommandRow) -> bool {
    row.name.starts_with("route")
        || row
            .env
            .iter()
            .any(|(_, v)| matches!(v, EnvValue::CalibreCopy))
}

/// Where an input's frozen answers live.
fn frozen_file(input: &Path, committed: bool, oracle_dir: Option<&Path>) -> PathBuf {
    let base = match (committed, oracle_dir) {
        (true, Some(dir)) => dir.to_path_buf(),
        (true, None) => paths::workspace_root().join("oracle/parity"),
        (false, _) => paths::workspace_root().join("oracle-real/parity"),
    };
    base.join(format!("{}.json", paths::stem(input)))
}

/// A frozen answer file's text, as the freezing wrote it: two-space JSON in
/// the rows' own order, and a newline. Every committed file reads back to
/// these exact bytes (tests/parity_accept.rs), so accepting one row
/// rewrites that row and nothing else.
pub fn frozen_text(answers: &serde_json::Map<String, Value>) -> String {
    let mut text = serde_json::to_string_pretty(answers).expect("JSON values serialize");
    text.push('\n');
    text
}

fn read_frozen(
    input: &Path,
    committed: bool,
    oracle_dir: Option<&Path>,
) -> Result<serde_json::Map<String, Value>, String> {
    let file = frozen_file(input, committed, oracle_dir);
    let text = std::fs::read_to_string(&file).map_err(|e| {
        format!(
            "no frozen answers for {} ({e})",
            if committed {
                file.display().to_string()
            } else {
                "a real script".to_owned()
            }
        )
    })?;
    match serde_json::from_str(&text) {
        Ok(Value::Object(m)) => Ok(m),
        _ => Err("a frozen answer file is not a JSON object".into()),
    }
}

pub fn run(args: &[String]) -> Result<bool, String> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{USAGE}");
        return Ok(true);
    }
    let all = commands();
    let opts = parse(args, &all)?;
    let side = match rust_cli(opts.rust_cli.clone())? {
        Some(p) => Side::rust(p),
        None => return Err("no Rust CLI: crates/sp-cli does not build".into()),
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

    // One folder per run, even for two runs in one process (the tests).
    static RUNS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let nth = RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let scratch =
        std::env::temp_dir().join(format!("method-reader-parity-{}-{nth}", std::process::id()));
    let machine = crate::machine::facts(None);
    if !cfg!(target_os = "macos") {
        println!(
            "parity: the frozen answers are a Mac's; another system may differ where the CLI does"
        );
    }
    let mut clean = true;
    let mut case = 0usize;
    let mut run_case = |input: &Path, row: &CommandRow| -> Result<Run, String> {
        case += 1;
        let base = scratch.join(format!("{case:04}"));
        run_side(&side, row, input, &base.join(side.label), DEFAULT_EPOCH)
    };

    if let Some(names) = &opts.accept {
        let result = accept(names, &rows, &committed, &opts, &machine, &mut run_case);
        if opts.keep {
            println!("scratch kept in {}", scratch.display());
        } else {
            let _ = std::fs::remove_dir_all(&scratch);
        }
        return result;
    }

    println!(
        "parity: {} against the frozen TypeScript answers",
        side.program.display()
    );
    let (mut runs, mut same, mut skipped) = (0usize, 0usize, 0usize);
    for input in &committed {
        let frozen = read_frozen(input, true, opts.oracle_dir.as_deref())?;
        let name = paths::stem(input);
        let here = frozen.get("$machine") == Some(&machine);
        for row in rows.iter().filter(|r| r.inputs.accepts(input, true)) {
            if machine_bound(row) && !here {
                skipped += 1;
                println!("  skip  {name} {} (frozen on another machine)", row.name);
                continue;
            }
            runs += 1;
            let Some(want) = frozen.get(&row.name) else {
                clean = false;
                println!("  DIFF  {name} {}\n          no frozen answer", row.name);
                continue;
            };
            let got = run_case(input, row)?;
            let diffs = diff_frozen(row, want, &got);
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
        "  committed fixtures: {} inputs, {runs} runs, {same} identical, {} differ, {skipped} skipped",
        committed.len(),
        runs - same
    );

    if !real.is_empty() {
        let (mut cases, mut same, mut skipped, mut missing) = (0usize, 0usize, 0usize, 0usize);
        let mut by_output: BTreeMap<String, usize> = BTreeMap::new();
        for input in &real {
            let Ok(frozen) = read_frozen(input, false, None) else {
                missing += 1;
                continue;
            };
            let here = frozen.get("$machine") == Some(&machine);
            for row in rows.iter().filter(|r| r.inputs.accepts(input, false)) {
                if machine_bound(row) && !here {
                    skipped += 1;
                    continue;
                }
                cases += 1;
                let Some(want) = frozen.get(&row.name) else {
                    clean = false;
                    *by_output
                        .entry(format!("{} no frozen answer", row.name))
                        .or_default() += 1;
                    continue;
                };
                let got = run_case(input, row)?;
                let diffs = diff_frozen(row, want, &got);
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
            "  real scripts: {} inputs, {cases} runs, {same} identical, {} differ, {skipped} skipped",
            real.len() - missing,
            cases - same
        );
        if missing > 0 {
            println!(
                "          {missing} real scripts have no frozen answers in rust/oracle-real/parity"
            );
        }
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

/// A row's frozen answer, as the freezing wrote it, from a Rust run: the
/// exit code, stdout, stderr where the row compares it (else what was
/// there), and each listed output's summary.
fn frozen_value(row: &CommandRow, rs: &Run, before: Option<&Value>) -> Value {
    let mut outputs = serde_json::Map::new();
    for o in &row.outputs {
        outputs.insert(
            o.file.clone(),
            compare::summary(&rs.work.join(&o.file), o.compare, &rs.work),
        );
    }
    let stderr = if row.stderr {
        Value::String(rs.stderr.clone())
    } else {
        before.map_or(Value::Null, |b| b["stderr"].clone())
    };
    serde_json::json!({
        "exit": rs.exit,
        "stdout": rs.answer,
        "stderr": stderr,
        "outputs": outputs,
    })
}

/// `--accept`: the named rows on the committed fixtures, each shown against
/// its frozen answer and, where it differs, written in its place. Rows this
/// machine cannot answer as the freezing one did (Calibre, routes) are left
/// alone. Real scripts are never touched: their answers quote the scripts.
fn accept(
    names: &[String],
    rows: &[&CommandRow],
    committed: &[PathBuf],
    opts: &Options,
    machine: &Value,
    run_case: &mut dyn FnMut(&Path, &CommandRow) -> Result<Run, String>,
) -> Result<bool, String> {
    println!(
        "parity --accept {}: the committed fixtures' frozen answers for these rows",
        names.join(" ")
    );
    let (mut changed_rows, mut files, mut same, mut left) = (0usize, 0usize, 0usize, 0usize);
    for input in committed {
        let mut frozen = read_frozen(input, true, opts.oracle_dir.as_deref())?;
        let name = paths::stem(input);
        let here = frozen.get("$machine") == Some(machine);
        let mut changed = false;
        for row in rows.iter().filter(|r| r.inputs.accepts(input, true)) {
            if machine_bound(row) && !here {
                left += 1;
                println!("  left  {name} {} (frozen on another machine)", row.name);
                continue;
            }
            let before = frozen.get(&row.name).cloned();
            let got = run_case(input, row)?;
            let after = at_frozen_version(&row.name, &frozen_value(row, &got, before.as_ref()));
            if before.as_ref() == Some(&after) {
                same += 1;
                println!("  same  {name} {}", row.name);
                continue;
            }
            println!("  NEW   {name} {}", row.name);
            print!(
                "{}",
                indent(&value_diff(before.as_ref().unwrap_or(&Value::Null), &after))
            );
            frozen.insert(row.name.clone(), after);
            changed = true;
            changed_rows += 1;
        }
        if changed {
            let file = frozen_file(input, true, opts.oracle_dir.as_deref());
            std::fs::write(&file, frozen_text(&frozen))
                .map_err(|e| format!("could not write {}: {e}", file.display()))?;
            files += 1;
        }
    }
    println!(
        "  accepted {changed_rows} rows in {files} files; {same} already matched, {left} left alone"
    );
    if changed_rows > 0 {
        println!(
            "parity: frozen answers rewritten. Read the diff above, then say why the output changed in the commit message."
        );
    }
    Ok(true)
}

fn indent(text: &str) -> String {
    text.lines().map(|l| format!("          {l}\n")).collect()
}

/// How a row's frozen answer changes, field by field: the exit code, and a
/// line diff of stdout, stderr and the outputs' summaries.
pub fn value_diff(before: &Value, after: &Value) -> String {
    let mut out = String::new();
    for key in ["exit", "stdout", "stderr", "outputs"] {
        field_diff(key, &before[key], &after[key], &mut out);
    }
    out
}

/// One field's diff, under its JSON path. Objects are followed key by key,
/// so a long text inside a `--json` answer (a help text's `usage`) is
/// diffed as its own lines, not as one escaped string.
fn field_diff(path: &str, b: &Value, a: &Value, out: &mut String) {
    if b == a {
        return;
    }
    if let (Value::Object(x), Value::Object(y)) = (b, a) {
        let mut keys: Vec<&String> = x.keys().collect();
        keys.extend(y.keys().filter(|k| !x.contains_key(*k)));
        for k in keys {
            let none = Value::Null;
            field_diff(
                &format!("{path}.{k}"),
                x.get(k).unwrap_or(&none),
                y.get(k).unwrap_or(&none),
                out,
            );
        }
        return;
    }
    out.push_str(&format!("{path}:\n"));
    out.push_str(&line_diff(&as_lines(b), &as_lines(a)));
}

/// A text field as its own lines; anything else as two-space JSON.
fn as_lines(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => serde_json::to_string_pretty(other).expect("JSON values serialize"),
    }
}

/// A unified-style line diff: `-` for a line only before, `+` only after,
/// two lines of unchanged context around each change.
pub fn line_diff(before: &str, after: &str) -> String {
    let a: Vec<&str> = before.split('\n').collect();
    let b: Vec<&str> = after.split('\n').collect();
    let head = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let tail = a[head..]
        .iter()
        .rev()
        .zip(b[head..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (mid_a, mid_b) = (&a[head..a.len() - tail], &b[head..b.len() - tail]);
    // The longest common subsequence of the middle; a middle too large for
    // that is shown as all of one side, then all of the other.
    let mut ops: Vec<(char, &str)> = Vec::new();
    if mid_a.len().saturating_mul(mid_b.len()) <= 4_000_000 {
        let (n, m) = (mid_a.len(), mid_b.len());
        let mut lcs = vec![vec![0u32; m + 1]; n + 1];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                lcs[i][j] = if mid_a[i] == mid_b[j] {
                    lcs[i + 1][j + 1] + 1
                } else {
                    lcs[i + 1][j].max(lcs[i][j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n || j < m {
            if i < n && j < m && mid_a[i] == mid_b[j] {
                ops.push((' ', mid_a[i]));
                i += 1;
                j += 1;
            } else if i < n && (j == m || lcs[i + 1][j] >= lcs[i][j + 1]) {
                ops.push(('-', mid_a[i]));
                i += 1;
            } else {
                ops.push(('+', mid_b[j]));
                j += 1;
            }
        }
    } else {
        ops.extend(mid_a.iter().map(|l| ('-', *l)));
        ops.extend(mid_b.iter().map(|l| ('+', *l)));
    }
    let mut all: Vec<(char, &str)> = a[..head].iter().map(|l| (' ', *l)).collect();
    all.extend(ops);
    all.extend(a[a.len() - tail..].iter().map(|l| (' ', *l)));
    let near = |k: usize| {
        all[k.saturating_sub(2)..(k + 3).min(all.len())]
            .iter()
            .any(|(op, _)| *op != ' ')
    };
    let mut out = String::new();
    let mut skipped = false;
    for (k, (op, line)) in all.iter().enumerate() {
        if near(k) {
            if skipped {
                out.push_str("  ...\n");
                skipped = false;
            }
            out.push_str(&format!("{op} {line}\n"));
        } else {
            skipped = true;
        }
    }
    out
}
