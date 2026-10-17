//! The update consent, carried across the handover.
//!
//! The Tauri window kept four values in WebKit localStorage
//! (desktop/ui/update.js: `updateLastChecked`, `updateOptIn`,
//! `updateAsked`, `updateFound`), and WebKit keeps that storage per bundle
//! identifier, under `~/Library/WebKit/<identifier>/WebsiteData/`. The Rust
//! app has no WebKit, so on its first run (when its own
//! `window-storage.json` does not exist yet) it reads those four values
//! from the window's file, under the identifier the window had (the app's
//! caller names it: from 0.13 the app's own identifier differs), and
//! starts from them. Someone who said
//! yes stays opted in; someone who said no is not asked again.
//!
//! Two layouts are known: the older
//! `WebsiteData/LocalStorage/tauri_localhost_0.localstorage`, and the
//! newer `WebsiteData/Default/<hash>/<hash>/LocalStorage/localstorage.sqlite3`
//! with an `origin` file beside `LocalStorage/` naming the origin
//! (`tauri://localhost` for a Tauri window on a Mac). Both are SQLite with
//! `ItemTable(key TEXT, value BLOB)`, the value UTF-16LE. Nothing found, or
//! nothing readable, means the window's own default: the question is asked
//! once. The window's files are only ever read.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::sqlite::{Database, Value, utf16};

/// The four keys desktop/ui/update.js stores, and nothing else.
pub const UPDATE_KEYS: [&str; 4] = [
    "updateLastChecked",
    "updateOptIn",
    "updateAsked",
    "updateFound",
];

/// Where WebKit keeps a bundle identifier's website data, for this user.
pub fn website_data(home: &Path, identifier: &str) -> PathBuf {
    home.join("Library")
        .join("WebKit")
        .join(identifier)
        .join("WebsiteData")
}

fn is_storage_file(p: &Path) -> bool {
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned());
    match name {
        Some(n) => n == "localstorage.sqlite3" || n.ends_with(".localstorage"),
        None => false,
    }
}

/// How sure we are that a storage file is the window's: 2 names the Tauri
/// origin, 0 does not say.
fn score(p: &Path) -> u8 {
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    if name.starts_with("tauri_localhost") {
        return 2;
    }
    // Default/<hash>/<hash>/LocalStorage/localstorage.sqlite3: the origin
    // file sits beside LocalStorage/.
    if let Some(origin) = p
        .parent()
        .and_then(Path::parent)
        .map(|d| d.join("origin"))
        .and_then(|f| std::fs::read(f).ok())
    {
        let has = |needle: &[u8]| origin.windows(needle.len()).any(|w| w == needle);
        if has(b"tauri") && has(b"localhost") {
            return 2;
        }
    }
    0
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk(&p, depth - 1, out);
        } else if is_storage_file(&p) {
            out.push(p);
        }
    }
}

/// The window's localStorage file under `website_data`, if one can be
/// told apart: the one naming the Tauri origin, or the only one there is.
pub fn find_storage(website_data: &Path) -> Option<PathBuf> {
    let mut found = Vec::new();
    walk(website_data, 6, &mut found);
    let named: Vec<&PathBuf> = found.iter().filter(|p| score(p) == 2).collect();
    match (named.as_slice(), found.as_slice()) {
        ([one], _) => Some((*one).clone()),
        ([], [only]) => Some(only.clone()),
        _ => None,
    }
}

/// The four update values in one localStorage file.
pub fn read_update_keys(file: &Path) -> Result<HashMap<String, String>, String> {
    let db = Database::open(file)?;
    let mut out = HashMap::new();
    for row in db.rows("ItemTable")? {
        let key = match row.first() {
            Some(Value::Text(k)) => k.clone(),
            Some(Value::Blob(b)) => utf16(b, false),
            _ => continue,
        };
        if !UPDATE_KEYS.contains(&key.as_str()) {
            continue;
        }
        let value = match row.get(1) {
            Some(Value::Blob(b)) => utf16(b, false),
            Some(Value::Text(t)) => t.clone(),
            _ => continue,
        };
        out.insert(key, value);
    }
    Ok(out)
}

/// What the Rust app starts its window storage from on its first run: the
/// window's four values, or nothing.
pub fn import_update_consent(website_data: &Path) -> HashMap<String, String> {
    find_storage(website_data)
        .and_then(|f| read_update_keys(&f).ok())
        .unwrap_or_default()
}
