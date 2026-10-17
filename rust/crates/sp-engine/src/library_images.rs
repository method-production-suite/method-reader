//! Port of `src/library-images.ts`: the picture folder beside a
//! `.fountain`, `<stem>.images/`, written by a PDF conversion and read back
//! by anything that renders a book from the text. ONE module for every
//! caller, so a re-render and the export path's Kindle rebuild find the
//! same pictures under the same rules.
//!
//! The predicates and limits it shares with the parser are sp-parser's own
//! (`isGeneratedImageFile`, `IMAGE_MAX_EDGE`, `isPng`, `MAX_IMAGES`), and
//! `isSafeImageFile`, the security allowlist, is sp-fountain's: two copies
//! of a check drift exactly like two copies of a parser.

use std::collections::{BTreeMap, BTreeSet};

use crate::errors::{EngineError, FsError};
use crate::fsx;
use crate::nodepath::host;

use sp_fountain::notes::is_safe_image_file;
use sp_parser::artwork::MAX_IMAGES;
use sp_parser::images::{IMAGE_MAX_EDGE, is_generated_image_file};
use sp_parser::png::is_png;

/// `imagesDirFor(fountainPath)`: same directory, same stem, `.images`.
pub fn images_dir_for(fountain_path: &str) -> String {
    let base = host::basename_ext(fountain_path, &host::extname(fountain_path));
    format!(
        "{}.images",
        host::join(&[host::dirname(fountain_path), base])
    )
}

/// `MAX_IMAGE_FILE_BYTES`: what our own encoder can emit at
/// `IMAGE_MAX_EDGE`, plus five percent. Computed, not hand-derived.
pub const MAX_IMAGE_FILE_BYTES: u64 = max_image_file_bytes();

const fn max_image_file_bytes() -> u64 {
    // Math.ceil((edge * 3 + 1) * edge * 1.05), in integers: * 105 / 100,
    // rounded up.
    let edge = IMAGE_MAX_EDGE as u64;
    let raw = (edge * 3 + 1) * edge * 105;
    raw.div_ceil(100)
}

/// `isImageTemp(name)`: a leftover `.p001-01.png.99.tmp` (or `.99-3.tmp`) from an
/// interrupted write.
pub fn is_image_temp(name: &str) -> bool {
    let Some(rest) = name.strip_prefix('.').and_then(|r| r.strip_suffix(".tmp")) else {
        return false;
    };
    // `^\.(.+)\.\d+\.tmp$`, greedy: the LAST `.digits` before `.tmp`.
    let Some(dot) = rest.rfind('.') else {
        return false;
    };
    let (inner, pid) = (&rest[..dot], &rest[dot + 1..]);
    // `<pid>` (the TypeScript's) or `<pid>-<n>` (fsx::temp_path_for's).
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let stamp_ok = match pid.split_once('-') {
        Some((p, n)) => digits(p) && digits(n),
        None => digits(pid),
    };
    !inner.is_empty() && stamp_ok && is_generated_image_file(inner)
}

/// `ImagesRead`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ImagesRead {
    /// The pictures, by name, in name order.
    pub images: BTreeMap<String, Vec<u8>>,
    /// Entries that named a picture and did not yield one.
    pub refused: usize,
    /// Names the cap turned away.
    pub over_cap: usize,
    /// Pictures asked for that the folder does not list.
    pub not_found: usize,
    /// True when `<stem>.images` is a link or a file: nothing was read.
    pub folder_refused: bool,
}

/// What is at a picture folder's path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FolderAt {
    Missing,
    Folder,
    Refused,
}

/// `folderAt(dir)`: `lstat`, so a symlink, wherever it points, is refused.
fn folder_at(dir: &str) -> FolderAt {
    match std::fs::symlink_metadata(dir) {
        Ok(m) if m.is_dir() => FolderAt::Folder,
        Ok(_) => FolderAt::Refused,
        Err(_) => FolderAt::Missing,
    }
}

/// The names in a folder, or `None` when it cannot be listed.
fn list(dir: &str) -> Option<Vec<String>> {
    let entries = std::fs::read_dir(dir).ok()?;
    Some(
        entries
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .collect(),
    )
}

/// `readImages(fountainPath, asked)`: the pictures beside a `.fountain`,
/// bounded on six axes (a real folder, a safe name, a regular file, a size
/// our encoder could make, a PNG header, and the cap), each loss counted.
pub fn read_images(fountain_path: &str, asked: &BTreeSet<String>) -> ImagesRead {
    let dir = images_dir_for(fountain_path);
    let mut read = ImagesRead::default();
    match folder_at(&dir) {
        FolderAt::Missing => {
            read.not_found = asked.len();
            return read;
        }
        FolderAt::Refused => {
            read.folder_refused = true;
            return read;
        }
        FolderAt::Folder => {}
    }
    let Some(listed) = list(&dir) else {
        read.not_found = asked.len();
        return read;
    };
    let mut names: Vec<String> = listed
        .into_iter()
        .filter(|n| is_safe_image_file(n))
        .collect();
    names.sort();
    let listed: BTreeSet<&String> = names.iter().collect();
    read.not_found = asked.iter().filter(|n| !listed.contains(n)).count();
    let counts = |name: &str| asked.is_empty() || asked.contains(name);
    for name in &names {
        if read.images.len() >= MAX_IMAGES {
            if counts(name) {
                read.over_cap += 1;
            }
            continue;
        }
        let path = host::join(&[dir.as_str(), name.as_str()]);
        let ok = (|| {
            let info = std::fs::symlink_metadata(&path).ok()?;
            if !info.is_file() || info.len() > MAX_IMAGE_FILE_BYTES {
                return None;
            }
            let bytes = std::fs::read(&path).ok()?;
            is_png(&bytes).then_some(bytes)
        })();
        match ok {
            Some(bytes) => {
                read.images.insert(name.clone(), bytes);
            }
            None => {
                if counts(name) {
                    read.refused += 1;
                }
            }
        }
    }
    read
}

/// `imageFolderWarnings(read)`: the sentences for pictures a folder held
/// and the book does not show.
pub fn image_folder_warnings(read: &ImagesRead) -> Vec<String> {
    let mut out = Vec::new();
    if read.folder_refused {
        out.push("This script's picture folder is not a folder of its own (it is a link or a file), so no pictures were read from it.".to_owned());
    }
    let kept = read.images.len();
    if read.over_cap > 0 {
        out.push(format!(
            "Method Reader found {} pictures in this script's picture folder. Only the first {kept} were used.",
            kept + read.over_cap
        ));
    }
    if read.refused > 0 {
        out.push(if read.refused == 1 {
            "1 picture could not be read out of this script's picture folder, so it was left out."
                .to_owned()
        } else {
            format!(
                "{} pictures could not be read out of this script's picture folder, so they were left out.",
                read.refused
            )
        });
    }
    if read.not_found > 0 {
        out.push(if read.not_found == 1 {
            "1 picture this script shows could not be found in its picture folder, so the book was made without it.".to_owned()
        } else {
            format!(
                "{} pictures this script shows could not be found in its picture folder, so the book was made without them.",
                read.not_found
            )
        });
    }
    out
}

/// `pruneImages(dir, keep)`: remove this writer's OWN stale files (names
/// `isGeneratedImageFile` or `isImageTemp` accepts) and say how many. Never
/// creates the folder, never reaches through a link, and removes the
/// folder only if that emptied it.
pub fn prune_images(dir: &str, keep: &BTreeSet<String>) -> usize {
    if folder_at(dir) != FolderAt::Folder {
        return 0;
    }
    let Some(existing) = list(dir) else {
        return 0;
    };
    let mut removed = 0;
    for name in existing {
        if keep.contains(&name) {
            continue;
        }
        if !is_generated_image_file(&name) && !is_image_temp(&name) {
            continue;
        }
        if std::fs::remove_file(host::join(&[dir, name.as_str()])).is_ok() {
            removed += 1;
        }
    }
    if removed > 0 {
        let _ = std::fs::remove_dir(dir);
    }
    removed
}

/// `writeImages(dir, images)`: write the picture folder (the folder only,
/// never its parents), refuse a link or a file at its path with ENOTDIR,
/// write each picture atomically, then prune what the last conversion
/// left. Fails with what the filesystem said, untranslated.
pub fn write_images(dir: &str, images: &[(String, Vec<u8>)]) -> Result<(), EngineError> {
    if let Err(err) = fsx::mkdir(dir)
        && err.fs_code() != Some("EEXIST")
    {
        return Err(err);
    }
    if folder_at(dir) != FolderAt::Folder {
        return Err(EngineError::Fs(FsError::with_code(
            "ENOTDIR",
            format!("{dir} is not a folder of its own (a link or a file)"),
        )));
    }
    let mut written = BTreeSet::new();
    for (name, bytes) in images {
        fsx::write_atomic(&host::join(&[dir, name.as_str()]), bytes)?;
        written.insert(name.clone());
    }
    prune_images(dir, &written);
    Ok(())
}
