//! Which books on a reader are Method Reader's (docs/device-library.md):
//! a library file with the same size and the same bytes. No real reader
//! and no real library: both are scratch folders.

use sp_device::books::DeviceBook;
use sp_engine::device_library::mark_library_books;
use std::path::Path;

fn write(root: &Path, rel: &str, bytes: &[u8]) -> String {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, bytes).unwrap();
    p.to_string_lossy().into_owned()
}

fn book(path: &str) -> DeviceBook {
    DeviceBook {
        path: path.to_string(),
        name: Path::new(path)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned(),
        size: std::fs::metadata(path).unwrap().len(),
        format: "azw3".into(),
        library: None,
    }
}

#[test]
fn a_book_is_the_librarys_only_when_size_and_bytes_both_match() {
    let lib = tempfile::tempdir().unwrap();
    let dev = tempfile::tempdir().unwrap();
    write(
        lib.path(),
        "Field Station/Field Station.azw3",
        b"the same bytes",
    );
    write(
        lib.path(),
        "Field Station/Field Station.epub",
        b"another book",
    );
    write(lib.path(), "Second/Second.azw3", b"second book!!!");
    let same = write(
        dev.path(),
        "documents/Field Station.azw3",
        b"the same bytes",
    );
    // Same size as a library file, different bytes: an older conversion.
    let older = write(dev.path(), "documents/Second.azw3", b"second book???");
    // Same name, different size.
    let renamed = write(dev.path(), "documents/Field Station.epub", b"not it");
    // Renamed on the reader but byte for byte the library's: still its.
    let moved = write(dev.path(), "documents/My copy.azw3", b"the same bytes");
    let mut books = vec![book(&same), book(&older), book(&renamed), book(&moved)];
    mark_library_books(&mut books, &lib.path().to_string_lossy());
    let got: Vec<Option<&str>> = books.iter().map(|b| b.library.as_deref()).collect();
    assert_eq!(
        got,
        [Some("Field Station"), None, None, Some("Field Station")]
    );
}

#[test]
fn no_library_folder_means_nothing_matches() {
    let dev = tempfile::tempdir().unwrap();
    let f = write(dev.path(), "a.epub", b"x");
    let mut books = vec![book(&f)];
    mark_library_books(&mut books, "/no/such/library");
    assert_eq!(books[0].library, None);
}

#[cfg(unix)]
#[test]
fn a_link_in_the_library_is_not_followed() {
    let lib = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let dev = tempfile::tempdir().unwrap();
    write(elsewhere.path(), "Linked/Linked.azw3", b"bytes");
    std::os::unix::fs::symlink(elsewhere.path().join("Linked"), lib.path().join("Linked")).unwrap();
    let f = write(dev.path(), "documents/Linked.azw3", b"bytes");
    let mut books = vec![book(&f)];
    mark_library_books(&mut books, &lib.path().to_string_lossy());
    assert_eq!(books[0].library, None);
}
