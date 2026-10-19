//! The `device-books` and `device-remove` verbs: what is on a reader, and
//! taking one book off it (docs/device-library.md). Rust's own: the
//! TypeScript had neither.
//!
//! Both act ONLY on a volume the engine itself lists as a reader right now,
//! matched on its resolved path as [`crate::eject`] does, so a path from the
//! window is never trusted on its own. Listing never follows a link. Removing
//! takes one regular book file inside the reader's books folder, reached
//! through no link, and on a Kindle the `.sdr` folder of reading position and
//! notes that goes with it; every refusal comes before anything is touched.

use crate::error::{CliError, ErrorCode};
use crate::list::{ListDevicesOptions, list_devices};
use crate::types::{ConnectedDevice, DeviceKind};
use serde::Serialize;
use sp_fsutil::nodepath::host;
use std::path::{Component, Path, PathBuf};

/// The extensions a reader opens, longest first so `.kepub.epub` wins over
/// `.epub`, each with the format word the answer gives.
const FORMATS: [(&str, &str); 8] = [
    (".kepub.epub", "kepub"),
    (".epub", "epub"),
    (".azw3", "azw3"),
    (".azw", "azw"),
    (".mobi", "mobi"),
    (".kfx", "kfx"),
    (".pdf", "pdf"),
    (".txt", "txt"),
];

/// One book on a reader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeviceBook {
    /// Its full path on this computer.
    pub path: String,
    /// Its path inside the reader's books folder, with `/` between folders.
    pub name: String,
    pub size: u64,
    /// `epub`, `kepub`, `azw3`, `azw`, `mobi`, `kfx`, `pdf` or `txt`.
    pub format: String,
    /// The library folder holding a file with the same size and bytes, when
    /// there is one (filled by the caller, which knows the library).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub library: Option<String>,
}

/// `device-books`' answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BooksResult {
    pub volume: String,
    pub kind: DeviceKind,
    /// The folder books are listed from (and may be removed from).
    pub folder: String,
    pub books: Vec<DeviceBook>,
}

/// `device-remove`'s answer: everything that went, the book first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemoveResult {
    pub removed: Vec<String>,
}

/// Where the reader list comes from: injected by every test.
#[derive(Default)]
pub struct BooksDeps<'a> {
    pub devices: Option<&'a dyn Fn() -> Vec<ConnectedDevice>>,
}

/// The format of a file name, if it is a book.
pub fn book_format(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    FORMATS
        .iter()
        .find(|(ext, _)| lower.ends_with(ext) && lower.len() > ext.len())
        .map(|(_, f)| *f)
}

/// The folder a reader's books live in: Kindle `documents/`, Kobo the
/// volume itself, tolino `Books/` (where [`crate::transfer`] puts them).
pub fn books_folder(kind: DeviceKind, volume: &str) -> Option<String> {
    match kind {
        DeviceKind::Kindle => Some(host::join(&[volume, "documents"])),
        DeviceKind::Kobo => Some(volume.to_string()),
        DeviceKind::Tolino => Some(host::join(&[volume, "Books"])),
        DeviceKind::Remarkable => None,
    }
}

/// The listed reader whose volume `volume` names, or the refusal.
fn listed_reader(
    verb: &str,
    volume: &str,
    deps: &BooksDeps,
) -> Result<(ConnectedDevice, String), CliError> {
    if !Path::new(volume).is_absolute() {
        return Err(CliError::new(
            ErrorCode::Usage,
            format!("{verb} needs a full path to a reader's volume, not \"{volume}\""),
        ));
    }
    let wanted = host::resolve(&[volume]);
    let devices = match deps.devices {
        Some(devices) => devices(),
        None => list_devices(&ListDevicesOptions {
            skip_remarkable: true,
            skip_mtp: true,
            ..Default::default()
        }),
    };
    devices
        .into_iter()
        .find_map(|d| {
            let v = d.volume.as_deref().map(|v| host::resolve(&[v]))?;
            (v == wanted && books_folder(d.kind, &v).is_some()).then_some((d, v))
        })
        .ok_or_else(|| {
            CliError::new(
                ErrorCode::UnknownDevice,
                format!("not a reader Method Reader can see: {volume}"),
            )
        })
}

/// A folder name the listing never enters: a dot folder (`.kobo`, a Mac's
/// `.Trashes`), or a Kindle's `<book>.sdr` of reading position and notes.
fn skipped_folder(name: &str) -> bool {
    name.starts_with('.') || name.to_lowercase().ends_with(".sdr")
}

fn walk(dir: &Path, rel: &str, out: &mut Vec<DeviceBook>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        // symlink_metadata: a link is neither entered nor listed.
        let Ok(meta) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        let inner = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        if meta.is_dir() {
            if !skipped_folder(&name) {
                walk(&entry.path(), &inner, out);
            }
        } else if meta.is_file()
            && !name.starts_with('.')
            && let Some(format) = book_format(&name)
        {
            out.push(DeviceBook {
                path: entry.path().to_string_lossy().into_owned(),
                name: inner,
                size: meta.len(),
                format: format.to_string(),
                library: None,
            });
        }
    }
}

/// `device-books <volume>`: the books on that reader, files before folders'
/// contents at each level, by name.
pub fn device_books_command(volume: &str, deps: &BooksDeps) -> Result<BooksResult, CliError> {
    let (device, volume) = listed_reader("device-books", volume, deps)?;
    let folder = books_folder(device.kind, &volume).unwrap_or_default();
    let mut books = Vec::new();
    // The books folder itself must be a real folder: a link there is never
    // entered.
    if std::fs::symlink_metadata(&folder).is_ok_and(|m| m.is_dir()) {
        walk(Path::new(&folder), "", &mut books);
    }
    books.sort_by(|a, b| {
        let depth = |n: &str| n.contains('/');
        (depth(&a.name), &a.name).cmp(&(depth(&b.name), &b.name))
    });
    Ok(BooksResult {
        volume,
        kind: device.kind,
        folder,
        books,
    })
}

fn not_a_book(file: &str, why: &str) -> CliError {
    CliError::new(
        ErrorCode::NotABook,
        format!("not a book on this reader ({why}): {file}"),
    )
}

/// `device-remove <volume> <file>`: removes one book, and on a Kindle its
/// `.sdr` folder, after every check in the module comment.
pub fn device_remove_command(
    volume: &str,
    file: &str,
    deps: &BooksDeps,
) -> Result<RemoveResult, CliError> {
    let (device, volume) = listed_reader("device-remove", volume, deps)?;
    if !Path::new(file).is_absolute() {
        return Err(CliError::new(
            ErrorCode::Usage,
            format!("device-remove needs the book's full path, not \"{file}\""),
        ));
    }
    let folder = books_folder(device.kind, &volume).unwrap_or_default();
    // Resolved by its words first, so `..` cannot climb out.
    let resolved = PathBuf::from(host::resolve(&[file]));
    let inside = resolved
        .strip_prefix(&folder)
        .ok()
        .filter(|rest| !rest.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .ok_or_else(|| not_a_book(file, "outside the reader's books folder"))?;
    // No part of the way down may be a link, a dot folder or a Kindle's
    // .sdr folder: the listing never goes there, so removing never does.
    let mut at = PathBuf::from(&folder);
    if std::fs::symlink_metadata(&at).map_or(true, |m| !m.is_dir()) {
        return Err(not_a_book(file, "the reader has no books folder"));
    }
    let parts: Vec<_> = inside.components().collect();
    for (i, part) in parts.iter().enumerate() {
        let Component::Normal(name) = part else {
            return Err(not_a_book(file, "outside the reader's books folder"));
        };
        at.push(name);
        let meta = std::fs::symlink_metadata(&at)
            .map_err(|_| not_a_book(file, "there is no such file"))?;
        if meta.file_type().is_symlink() {
            return Err(not_a_book(file, "reached through a link"));
        }
        let last = i + 1 == parts.len();
        let name = name.to_string_lossy();
        if !last && (meta.is_file() || skipped_folder(&name)) {
            return Err(not_a_book(file, "not where the reader keeps books"));
        }
        if last && (!meta.is_file() || name.starts_with('.') || book_format(&name).is_none()) {
            return Err(not_a_book(file, "not a book file"));
        }
    }
    let book = at.to_string_lossy().into_owned();
    let failed = |what: &str, e: std::io::Error| {
        CliError::new(
            ErrorCode::RemoveFailed,
            format!("the reader would not let {what} go: {e}"),
        )
    };
    std::fs::remove_file(&at).map_err(|e| failed(&book, e))?;
    let mut removed = vec![book];
    if device.kind == DeviceKind::Kindle
        && let Some(stem) = kindle_stem(&at)
    {
        let sdr = at.with_file_name(format!("{stem}.sdr"));
        if std::fs::symlink_metadata(&sdr).is_ok_and(|m| m.is_dir()) {
            let sdr_text = sdr.to_string_lossy().into_owned();
            // The book is already gone by now (it goes first, so a failure
            // never leaves a book without its notes). Say so: a reader
            // unplugged in between must not read as nothing having happened.
            if let Err(e) = std::fs::remove_dir_all(&sdr) {
                return Err(CliError::new(
                    ErrorCode::RemoveFailed,
                    format!(
                        "removed {}, but the reader would not let its notes folder {sdr_text} go: {e}",
                        removed[0]
                    ),
                ));
            }
            removed.push(sdr_text);
        }
    }
    Ok(RemoveResult { removed })
}

/// The name a Kindle gives a book's `.sdr` folder: the file name without its
/// extension.
fn kindle_stem(book: &Path) -> Option<String> {
    book.file_stem().map(|s| s.to_string_lossy().into_owned())
}
