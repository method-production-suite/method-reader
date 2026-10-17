//! Copy a file over whatever is at the destination, which on a USB send is
//! the reader's previous copy of the book. Ported from src/replace-file.ts.
//! Used by every device transfer path (sp-device's kindle and transfer), so
//! the replace semantics cannot drift between them.
//!
//! The copy goes to a hidden file in the destination's own folder, is
//! flushed to the device (fsync), and is then renamed over the destination.
//! A copy that fails part way (a nearly full reader, a cable pulled
//! mid-write) leaves the old copy exactly as it was: the partial is removed
//! and the error goes to the caller. Same folder means same volume, so the
//! rename is one step and never a second copy.
//!
//! The flush comes before the rename because without it the rename can
//! reach a USB reader before the copied bytes do, so a reader unplugged
//! right after "Sent" could hold a truncated book under the book's own name.
//!
//! The cost: while the copy is in flight the reader holds both copies, so a
//! reader with room for only one refuses the send and keeps the old book.
//!
//! The error that reaches the caller names the book, never the partial: the
//! send's failure line shows it to the reader as it is, and a hidden file
//! they never chose is no help there.

use crate::error::NodeIoError;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

/// Replace `destination` with a copy of `source`, flushed with fsync.
pub fn replace_file(source: &Path, destination: &Path) -> Result<(), NodeIoError> {
    replace_file_with(source, destination, |f: &File| f.sync_all())
}

/// [`replace_file`] with the flush as a seam: a test sees WHEN it runs (on
/// the finished partial, before the old book is replaced); the flush itself
/// cannot be observed.
pub fn replace_file_with(
    source: &Path,
    destination: &Path,
    mut sync: impl FnMut(&File) -> io::Result<()>,
) -> Result<(), NodeIoError> {
    let partial = partial_path_for(destination);
    let result = (|| {
        // The partial is made new, never opened as found: on a volume that
        // can hold links, one planted at the partial's (predictable) name
        // would have had the book written through it, off the reader. A
        // left-over partial, or a link, is removed first; remove_file never
        // follows a link.
        let _ = fs::remove_file(&partial);
        let copy_err =
            |e: io::Error| NodeIoError::from_io(&e, "copyfile", Some(source), Some(&partial));
        let mut from = File::open(source).map_err(copy_err)?;
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&partial)
            .map_err(|e| {
                // A folder in the partial's place (which remove_file could
                // not take): copyfile's own answer for it.
                if e.kind() == io::ErrorKind::AlreadyExists && partial.is_dir() {
                    copy_err(io::Error::from(io::ErrorKind::IsADirectory))
                } else {
                    copy_err(e)
                }
            })?;
        io::copy(&mut from, &mut file).map_err(copy_err)?;
        sync(&file).map_err(|e| NodeIoError::from_io(&e, "fsync", None, None))?;
        drop(file);
        fs::rename(&partial, destination)
            .map_err(|e| NodeIoError::from_io(&e, "rename", Some(&partial), Some(destination)))
    })();
    match result {
        Ok(()) => Ok(()),
        Err(error) => {
            // Left behind if this fails, and written over by this book's next
            // send (see partial_path_for). Why the copy failed is the error
            // worth reporting; a clean-up that failed as well must not take
            // its place.
            let _ = fs::remove_file(&partial);
            Err(naming_the_book(error, &partial, destination))
        }
    }
}

/// The hidden file a copy to `destination` goes through: same folder, and a
/// SHORT name of fixed length, because FAT32 and exFAT stop a name at 255
/// and a book with a long name must still go across. The eight hex
/// characters come from the book's name, so a partial left by a send that
/// was killed outright is the same file the next send of that book writes
/// over. The leading dot keeps Finder and the reader's library scan blind
/// to it, and `.partial` is not an extension any reader opens.
pub fn partial_path_for(destination: &Path) -> PathBuf {
    let name = destination
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let digest = Sha256::digest(name.as_bytes());
    let mark: String = digest.iter().take(4).map(|b| format!("{b:02x}")).collect();
    let file = format!(".method-reader-{mark}.partial");
    match destination.parent() {
        Some(parent) => parent.join(file),
        None => PathBuf::from(file),
    }
}

/// The system's error, with the book where the partial was: the code and the
/// sentence are kept, and a rename's `'<partial>' -> '<book>'` collapses to
/// the book alone.
fn naming_the_book(error: NodeIoError, partial: &Path, destination: &Path) -> NodeIoError {
    let p = partial.display().to_string();
    let d = destination.display().to_string();
    let message = error
        .message
        .replace(&format!("'{p}' -> '{d}'"), &format!("'{d}'"))
        .replace(&p, &d);
    NodeIoError {
        code: error.code,
        message,
    }
}
