//! Which books on a reader Method Reader made (docs/device-library.md): a
//! book is the library's when a file in a library folder has the same size
//! and the same bytes. Never by name alone: two scripts can share a name,
//! and a copy converted again since it was sent is honestly not a match.

use sp_device::books::DeviceBook;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Every file one level inside each library folder (`<library>/<script>/`),
/// by size, with its folder's name. Links are never followed.
fn library_files(root: &Path) -> HashMap<u64, Vec<(PathBuf, String)>> {
    let mut by_size: HashMap<u64, Vec<(PathBuf, String)>> = HashMap::new();
    let Ok(folders) = std::fs::read_dir(root) else {
        return by_size;
    };
    for folder in folders.flatten() {
        let is_dir = std::fs::symlink_metadata(folder.path()).is_ok_and(|m| m.is_dir());
        if !is_dir {
            continue;
        }
        let name = folder.file_name().to_string_lossy().into_owned();
        let Ok(files) = std::fs::read_dir(folder.path()) else {
            continue;
        };
        for file in files.flatten() {
            let Ok(meta) = std::fs::symlink_metadata(file.path()) else {
                continue;
            };
            if meta.is_file() {
                by_size
                    .entry(meta.len())
                    .or_default()
                    .push((file.path(), name.clone()));
            }
        }
    }
    by_size
}

/// Fills each book's `library` with the library folder whose file it is,
/// byte for byte. Files are only read when the sizes already agree.
pub fn mark_library_books(books: &mut [DeviceBook], library_root: &str) {
    let by_size = library_files(Path::new(library_root));
    for book in books.iter_mut() {
        let Some(candidates) = by_size.get(&book.size) else {
            continue;
        };
        let Ok(bytes) = std::fs::read(&book.path) else {
            continue;
        };
        book.library = candidates
            .iter()
            .find(|(path, _)| std::fs::read(path).is_ok_and(|b| b == bytes))
            .map(|(_, folder)| folder.clone());
    }
}
