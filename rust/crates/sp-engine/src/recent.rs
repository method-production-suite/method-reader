//! `recent.json`: the scripts a person opened, newest first, for the
//! suite's launcher (SUITE-STANDARDS.md §2, "For Method Launcher"). Not in
//! the TypeScript.
//!
//! The file sits in the app's data folder beside settings.json
//! ([`crate::settings::app::app_data_path`], so it follows that folder
//! wherever it goes) and holds
//! `[{"path": "/absolute/path", "opened": "<RFC 3339 time>"}]`. The
//! standard sets no cap and no rule for repeats or missing files, so these
//! are Method Reader's: at most [`RECENT_LIMIT`] entries, one per path (a
//! script opened again moves to the top), and nothing dropped because its
//! file is gone (the launcher can show it greyed).
//!
//! One per path means one per FILE: on Windows, whose file names ignore
//! case, `C:\Scripts\One.pdf` and `c:\scripts\ONE.PDF` are one entry, and
//! the newest spelling is kept ([`same_path`]). Elsewhere case is part of
//! the name, as it is for the data-folder test below: a Mac's default
//! volume ignores case, but a case-sensitive one is allowed, and the
//! engine cannot tell which a path is on without touching it.
//!
//! A damaged file (one that is there and can be read but is not JSON, or is
//! JSON but not an array) is never written over in silence. It is renamed
//! to `recent.json.damaged` beside it, replacing any older damaged copy, so
//! one copy is kept and it is the newest; then a fresh list starts, and the
//! call that set it aside says so once ([`Recorded::damaged`]), which the
//! command line, and with it the window, prints on stderr. A file that
//! cannot be read at all (no permission, a folder in its place) is left
//! alone and the record fails, since writing over what could not be read
//! would be the same silent loss. An array is never damaged: an entry in it
//! that is not an object with a string `path` is skipped, and any other
//! key an entry carries is kept.
//!
//! Two processes can record at once (the window and the command line, or
//! two command lines), and a read-then-write by each would drop one entry.
//! So every record holds an advisory lock on `recent.json.lock` beside the
//! file, through the standard library's own file locks, from before the
//! read until after the write; the write itself goes through a temp file
//! and a rename, so the launcher, which does not lock, never reads half of
//! one. The lock file stays: removing it would let a third writer lock a
//! new file while a second still held the old one.
//!
//! What counts as opened is the engine's decision ([`counts_as_opened`]):
//! a `--library` conversion, which is the window's Convert (a script
//! chosen or dropped) and the CLI's own `--library`, recorded under the
//! path the person gave, never the library copy. A `-o` or beside-the-input
//! conversion is not: that is a person's own pipeline, and the window's
//! rebuilds (a retune, the built-in sample) go that way too. Nothing inside
//! the data folder itself counts either, so the sample, which lives there,
//! is never recorded whichever way it is converted.
//!
//! Recording never fails a conversion: the caller logs the error and goes
//! on.

use serde_json::{Map, Value};

use crate::env::Env;
use crate::errors::EngineError;
use crate::fsx;
use crate::jsjson;
use crate::nodepath::host;
use crate::settings::app::app_data_path;

/// The file's name in the data folder.
pub const RECENT_FILE: &str = "recent.json";

/// The most entries the file keeps; the oldest go first.
pub const RECENT_LIMIT: usize = 20;

/// One entry: `path` and `opened`, plus any keys another writer added,
/// which are kept as they are.
pub type Entry = Map<String, Value>;

/// What one record did beyond adding the entry.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Recorded {
    /// Where the damaged file was kept, when this record found one and set
    /// it aside before starting a fresh list.
    pub damaged: Option<String>,
}

/// Where recent.json is for this platform and environment.
pub fn recent_path(platform: &str, env: &Env) -> String {
    app_data_path(platform, env, RECENT_FILE)
}

/// Where a damaged file at `path` is kept: `recent.json.damaged`.
pub fn damaged_path(path: &str) -> String {
    format!("{path}.damaged")
}

/// The lock file beside the file at `path`: `recent.json.lock`.
pub fn lock_path(path: &str) -> String {
    format!("{path}.lock")
}

/// What reading the file found.
enum Found {
    /// Missing, or a JSON array: its usable entries.
    Entries(Vec<Entry>),
    /// There and readable, but not a JSON array.
    Damaged,
}

/// Read the file at `path` (see the module notes for what is damaged). An
/// error only when the file is there and cannot be read.
fn load(path: &str) -> Result<Found, EngineError> {
    let text = match jsjson::read_text(std::path::Path::new(path)) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Found::Entries(Vec::new()));
        }
        Err(e) => {
            return Err(EngineError::plain(format!(
                "could not read {path}, so it was left as it is: {e}"
            )));
        }
    };
    let Some(Value::Array(items)) = jsjson::parse(&text) else {
        return Ok(Found::Damaged);
    };
    Ok(Found::Entries(
        items
            .into_iter()
            .filter_map(|item| match item {
                Value::Object(entry) if entry.get("path").is_some_and(Value::is_string) => {
                    Some(entry)
                }
                _ => None,
            })
            .collect(),
    ))
}

/// The file's entries, newest first. Never fails: a file that is missing,
/// unreadable, not JSON or not an array reads as empty, and an entry that
/// is not an object with a string `path` is skipped.
pub fn read_recent(path: &str) -> Vec<Entry> {
    match load(path) {
        Ok(Found::Entries(entries)) => entries,
        _ => Vec::new(),
    }
}

/// `path` as this platform compares it: lowercased on Windows, whose file
/// names ignore case, and as it is everywhere else.
fn path_key(platform: &str, path: &str) -> String {
    if platform == "win32" {
        path.to_lowercase()
    } else {
        path.to_owned()
    }
}

/// Whether `a` and `b` name the same file on `platform`: without regard to
/// case on Windows, exactly elsewhere (see the module notes).
pub fn same_path(platform: &str, a: &str, b: &str) -> bool {
    path_key(platform, a) == path_key(platform, b)
}

/// `entries` with `source` opened at `opened`: on top, any older entry for
/// the same file on `platform` gone, and cut to [`RECENT_LIMIT`].
pub fn with_opened(platform: &str, entries: Vec<Entry>, source: &str, opened: &str) -> Vec<Entry> {
    let mut top = Entry::new();
    top.insert("path".into(), Value::String(source.to_owned()));
    top.insert("opened".into(), Value::String(opened.to_owned()));
    let mut out = vec![top];
    out.extend(entries.into_iter().filter(|e| {
        !e.get("path")
            .and_then(Value::as_str)
            .is_some_and(|p| same_path(platform, p, source))
    }));
    out.truncate(RECENT_LIMIT);
    out
}

/// Record `source` as opened at `opened` in the file at `path`, on
/// `platform`: under the lock, read, set a damaged file aside, put `source`
/// on top, and write the whole file back through a temp file and a rename
/// (see the module notes). Makes the folder when it is not there.
pub fn record_opened(
    platform: &str,
    path: &str,
    source: &str,
    opened: &str,
) -> Result<Recorded, EngineError> {
    // One update at a time in this process too: the window runs engine
    // calls as threads of one process.
    static WRITING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one_at_a_time = WRITING.lock().unwrap_or_else(|p| p.into_inner());
    let folder = host::dirname(path);
    if !fsx::exists(&folder) {
        fsx::mkdir_all(&folder)?;
    }
    // And one at a time across processes: held until this returns.
    let lock = lock_path(path);
    let lock_file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&lock)
        .map_err(|e| EngineError::plain(format!("could not open {lock}: {e}")))?;
    lock_file
        .lock()
        .map_err(|e| EngineError::plain(format!("could not lock {lock}: {e}")))?;
    let mut recorded = Recorded::default();
    let entries = match load(path)? {
        Found::Entries(entries) => entries,
        Found::Damaged => {
            let aside = damaged_path(path);
            std::fs::rename(path, &aside).map_err(|e| {
                EngineError::plain(format!(
                    "{path} is damaged and could not be kept as {aside}, so it was left as it is: {e}"
                ))
            })?;
            recorded.damaged = Some(aside);
            Vec::new()
        }
    };
    let entries = with_opened(platform, entries, source, opened);
    let value = Value::Array(entries.into_iter().map(Value::Object).collect());
    let text = format!("{}\n", jsjson::stringify_pretty(&value));
    sp_fsutil::write_file_atomic(std::path::Path::new(path), text.as_bytes())
        .map_err(|e| EngineError::plain(e.to_string()))?;
    Ok(recorded)
}

/// Whether converting `source` counts as opening it (see the module
/// notes): a `--library` conversion of a file outside the data folder.
pub fn counts_as_opened(platform: &str, env: &Env, source: &str, library: bool) -> bool {
    if !library {
        return false;
    }
    let folder = host::dirname(&recent_path(platform, env));
    !is_inside(
        platform,
        &host::resolve(&[source]),
        &host::resolve(&[folder]),
    )
}

/// Whether `path` is inside `folder` (both absolute and normalized), by
/// whole names, and without regard to case on Windows.
fn is_inside(platform: &str, path: &str, folder: &str) -> bool {
    let (path, folder) = (path_key(platform, path), path_key(platform, folder));
    let sep = if platform == "win32" { '\\' } else { '/' };
    let folder = folder.trim_end_matches(sep);
    path.strip_prefix(folder)
        .is_some_and(|rest| rest.starts_with(sep))
}

/// After a successful conversion of `source`: record it when it counts as
/// opened, at `now` (Unix seconds). `Ok(None)` when it does not count.
pub fn record_conversion(
    platform: &str,
    env: &Env,
    source: &str,
    library: bool,
    now: i64,
) -> Result<Option<Recorded>, EngineError> {
    if !counts_as_opened(platform, env, source, library) {
        return Ok(None);
    }
    let absolute = host::resolve(&[source]);
    record_opened(
        platform,
        &recent_path(platform, env),
        &absolute,
        &rfc3339(now),
    )
    .map(Some)
}

/// A Unix time in seconds as an RFC 3339 UTC time to the second,
/// `2026-10-18T09:34:05Z`.
pub fn rfc3339(epoch_seconds: i64) -> String {
    sp_epub::build::iso_seconds(epoch_seconds)
}

/// This moment in Unix seconds.
pub fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}
