//! src/device/kindle.ts: USB-mounted Kindle detection and transfer. Older
//! Kindles mount as a mass-storage volume with a `documents/` folder (newer
//! firmware is MTP and never appears as a volume; see [`crate::mtp`]).

use crate::paths::{exists, is_directory};
use sp_fsutil::NodeIoError;
use sp_fsutil::nodepath::host;
use std::path::Path;

/// A mounted volume looks like a Kindle when it has a `documents/` folder
/// and either a Kindle-ish volume name or a `system` entry.
///
/// `system` is tested for existence, not for being a folder: the check is
/// "the firmware left its marker here", and narrowing it would reject a
/// Kindle whose marker is of another type. `documents` is the one that must
/// be a real directory: the book is copied INTO it.
pub fn is_kindle_volume(volume: &str) -> bool {
    if !is_directory(&host::join(&[volume, "documents"])) {
        return false;
    }
    if host::basename(volume).to_lowercase().contains("kindle") {
        return true;
    }
    exists(&host::join(&[volume, "system"]))
}

/// Copy a book into the device's documents folder, replacing any previous
/// copy. Returns the destination path. The folder is made if it is missing
/// (a no-op on any real Kindle, which is_kindle_volume guarantees has one).
pub fn copy_to_kindle_volume(file: &str, volume: &str) -> Result<String, NodeIoError> {
    let dest_dir = host::join(&[volume, "documents"]);
    refuse_link(&dest_dir)?;
    make_dir_all(&dest_dir)?;
    let dest = host::join(&[&dest_dir, &host::basename(file)]);
    sp_fsutil::replace_file(Path::new(file), Path::new(&dest))?;
    Ok(dest)
}

/// The book's folder on the reader must be a folder ON the reader: a link
/// there (a volume formatted HFS+ or ext4 can hold one) would send the book,
/// and replace a same-named file, wherever it points. Refused with Node's
/// ELOOP words, naming the folder.
pub(crate) fn refuse_link(dir: &str) -> Result<(), NodeIoError> {
    match std::fs::symlink_metadata(dir) {
        Ok(m) if m.file_type().is_symlink() => Err(NodeIoError {
            code: Some("ELOOP".into()),
            message: format!(
                "ELOOP: too many symbolic links encountered, the reader's folder is a link: '{dir}'"
            ),
        }),
        _ => Ok(()),
    }
}

/// `mkdirSync(dir, { recursive: true })`, in Node's words when it fails.
pub(crate) fn make_dir_all(dir: &str) -> Result<(), NodeIoError> {
    std::fs::create_dir_all(dir)
        .map_err(|e| NodeIoError::from_io(&e, "mkdir", Some(Path::new(dir)), None))
}
