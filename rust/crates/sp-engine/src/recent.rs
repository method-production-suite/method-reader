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

/// Where recent.json is for this platform and environment.
pub fn recent_path(platform: &str, env: &Env) -> String {
    app_data_path(platform, env, RECENT_FILE)
}

/// The file's entries, newest first. Never fails: a file that is missing,
/// unreadable, not JSON or not an array reads as empty, and an entry that
/// is not an object with a string `path` is skipped.
pub fn read_recent(path: &str) -> Vec<Entry> {
    let Some(Value::Array(items)) = fsx::read_text(path).ok().and_then(|t| jsjson::parse(&t))
    else {
        return Vec::new();
    };
    items
        .into_iter()
        .filter_map(|item| match item {
            Value::Object(entry) if entry.get("path").is_some_and(Value::is_string) => Some(entry),
            _ => None,
        })
        .collect()
}

/// `entries` with `source` opened at `opened`: on top, any older entry for
/// the same path gone, and cut to [`RECENT_LIMIT`].
pub fn with_opened(entries: Vec<Entry>, source: &str, opened: &str) -> Vec<Entry> {
    let mut top = Entry::new();
    top.insert("path".into(), Value::String(source.to_owned()));
    top.insert("opened".into(), Value::String(opened.to_owned()));
    let mut out = vec![top];
    out.extend(
        entries
            .into_iter()
            .filter(|e| e.get("path").and_then(Value::as_str) != Some(source)),
    );
    out.truncate(RECENT_LIMIT);
    out
}

/// Record `source` as opened at `opened` in the file at `path`: read, put
/// it on top, write the whole file back through a temp file and a rename,
/// so the launcher never reads half of one. Makes the folder when it is
/// not there. A corrupt file is replaced.
pub fn record_opened(path: &str, source: &str, opened: &str) -> Result<(), EngineError> {
    // One update at a time in this process: the window runs engine calls
    // as threads of one process, and two read-and-writes at once would each
    // drop the other's entry.
    static WRITING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one_at_a_time = WRITING.lock().unwrap_or_else(|p| p.into_inner());
    let entries = with_opened(read_recent(path), source, opened);
    let folder = host::dirname(path);
    if !fsx::exists(&folder) {
        fsx::mkdir_all(&folder)?;
    }
    let value = Value::Array(entries.into_iter().map(Value::Object).collect());
    let text = format!("{}\n", jsjson::stringify_pretty(&value));
    sp_fsutil::write_file_atomic(std::path::Path::new(path), text.as_bytes())
        .map_err(|e| EngineError::plain(e.to_string()))
}

/// Whether converting `source` counts as opening it (see the module
/// notes): a `--library` conversion of a file outside the data folder.
pub fn counts_as_opened(platform: &str, env: &Env, source: &str, library: bool) -> bool {
    if !library {
        return false;
    }
    let folder = host::dirname(&recent_path(platform, env));
    !is_inside(&host::resolve(&[source]), &host::resolve(&[folder]))
}

/// Whether `path` is inside `folder` (both absolute and normalized), by
/// whole names, and without regard to case on Windows.
fn is_inside(path: &str, folder: &str) -> bool {
    let (path, folder) = if cfg!(windows) {
        (path.to_lowercase(), folder.to_lowercase())
    } else {
        (path.to_owned(), folder.to_owned())
    };
    let sep = if cfg!(windows) { '\\' } else { '/' };
    let folder = folder.trim_end_matches(sep);
    path.strip_prefix(folder)
        .is_some_and(|rest| rest.starts_with(sep))
}

/// After a successful conversion of `source`: record it when it counts as
/// opened, at `now` (Unix seconds). `Ok(true)` when it was recorded.
pub fn record_conversion(
    platform: &str,
    env: &Env,
    source: &str,
    library: bool,
    now: i64,
) -> Result<bool, EngineError> {
    if !counts_as_opened(platform, env, source, library) {
        return Ok(false);
    }
    let absolute = host::resolve(&[source]);
    record_opened(&recent_path(platform, env), &absolute, &rfc3339(now))?;
    Ok(true)
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
