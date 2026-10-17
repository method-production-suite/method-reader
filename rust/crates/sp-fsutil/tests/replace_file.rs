//! tests/replace-file.test.ts, ported. Every USB send goes through
//! replace_file. A reader that is nearly full, or a cable that drops
//! mid-copy, used to lose the book outright: the old copy was deleted first
//! and the new one never arrived. A copy that fails must leave the reader
//! exactly as it was.

use sp_fsutil::{partial_path_for, replace_file, replace_file_with};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn temp() -> (tempfile::TempDir, PathBuf) {
    let scratch = tempfile::Builder::new()
        .prefix("method-reader-replace-file-")
        .tempdir()
        .unwrap();
    let dir = scratch.path().join("replace");
    fs::create_dir_all(&dir).unwrap();
    (scratch, dir)
}

fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

#[test]
fn copies_to_a_destination_that_does_not_exist() {
    let (_s, dir) = temp();
    let src = dir.join("a.txt");
    fs::write(&src, "v1").unwrap();
    replace_file(&src, &dir.join("b.txt")).unwrap();
    assert_eq!(read(&dir.join("b.txt")), "v1");
}

#[test]
fn overwrites_an_existing_destination_and_leaves_nothing_beside_it() {
    let (_s, dir) = temp();
    let src = dir.join("a.txt");
    let dest = dir.join("b.txt");
    fs::write(&src, "v1").unwrap();
    fs::write(&dest, "old").unwrap();
    replace_file(&src, &dest).unwrap();
    assert_eq!(read(&dest), "v1");
    assert_eq!(listing(&dir), ["a.txt", "b.txt"]);
}

#[test]
fn a_copy_that_fails_leaves_the_old_copy_byte_for_byte_and_nothing_beside_it() {
    // Real failures, not mocked ones: a directory as the source, which the
    // copy refuses, and a source that has gone.
    for source in ["a directory", "gone"] {
        let (_s, dir) = temp();
        let src = dir.join("not-a-book");
        if source == "a directory" {
            fs::create_dir(&src).unwrap();
        }
        let dest = dir.join("b.azw3");
        let old = [0x00u8, 0xff, 0x10, 0x42, 0x0a, 0x0d];
        fs::write(&dest, old).unwrap();
        assert!(replace_file(&src, &dest).is_err(), "{source}: should fail");
        assert_eq!(
            fs::read(&dest).unwrap(),
            old,
            "{source}: the old copy changed"
        );
        let left: Vec<String> = listing(&dir)
            .into_iter()
            .filter(|f| f != "not-a-book")
            .collect();
        assert_eq!(left, ["b.azw3"]);
    }
}

#[test]
fn a_copy_that_lands_but_cannot_be_put_in_place_is_taken_away_again() {
    // A directory standing at the destination refuses the rename on every
    // platform. The copy written first must not be left behind.
    let (_s, dir) = temp();
    let src = dir.join("a.txt");
    fs::write(&src, "v1").unwrap();
    let dest = dir.join("b.txt");
    fs::create_dir(&dest).unwrap();
    fs::write(dest.join("inside.txt"), "kept").unwrap();
    let err = replace_file(&src, &dest).unwrap_err();
    assert!(
        err.message
            .contains(&format!("rename '{}'", dest.display())),
        "{}",
        err.message
    );
    assert_eq!(listing(&dir), ["a.txt", "b.txt"]);
    assert_eq!(read(&dest.join("inside.txt")), "kept");
}

#[test]
fn a_partial_left_by_an_earlier_send_that_died_is_written_over() {
    let (_s, dir) = temp();
    let src = dir.join("a.txt");
    let dest = dir.join("b.txt");
    fs::write(&src, "v2").unwrap();
    fs::write(&dest, "v1").unwrap();
    fs::write(partial_path_for(&dest), "half of v").unwrap();
    replace_file(&src, &dest).unwrap();
    assert_eq!(read(&dest), "v2");
    assert_eq!(listing(&dir), ["a.txt", "b.txt"]);
}

#[test]
fn a_book_with_a_250_character_name_still_goes_across() {
    let (_s, dir) = temp();
    let src = dir.join("a.txt");
    fs::write(&src, "v1").unwrap();
    let dest = dir.join(format!("{}.azw3", "x".repeat(245)));
    replace_file(&src, &dest).unwrap();
    assert_eq!(read(&dest), "v1");
}

#[test]
fn the_partial_is_short_hidden_beside_the_book_and_the_same_for_the_same_book() {
    let dest = Path::new("/Volumes")
        .join("Kindle")
        .join("documents")
        .join(format!("{}.azw3", "x".repeat(245)));
    let partial = partial_path_for(&dest);
    assert_eq!(partial.parent(), dest.parent());
    let name = partial.file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        name.starts_with(".method-reader-") && name.ends_with(".partial"),
        "{name}"
    );
    let mark = &name[".method-reader-".len()..name.len() - ".partial".len()];
    assert_eq!(mark.len(), 8);
    assert!(
        mark.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    );
    assert_eq!(partial_path_for(&dest), partial);
    assert_ne!(
        partial_path_for(&dest.parent().unwrap().join("Another.azw3")),
        partial
    );
}

#[test]
fn the_partial_name_is_the_typescript_one() {
    // What src/replace-file.ts answers for this path (asked of it with bun,
    // 2026-10-11): the same book's partial is the same file whichever
    // engine left it, so a Rust send writes over a TypeScript send's leftover.
    let partial = partial_path_for(Path::new("/v/documents/Field Station.azw3"));
    assert_eq!(
        partial,
        Path::new("/v/documents").join(".method-reader-0bbeaa17.partial")
    );
}

#[test]
fn a_failed_copy_names_the_book_with_the_system_code_never_the_partial() {
    let (_s, dir) = temp();
    let dest = dir.join("Field Station.azw3");
    fs::write(&dest, "old").unwrap();
    let err = replace_file(&dir.join("gone.azw3"), &dest).unwrap_err();
    assert_eq!(err.code.as_deref(), Some("ENOENT"));
    assert!(err.message.starts_with("ENOENT"), "{}", err.message);
    assert!(
        err.message.contains(&dest.display().to_string()),
        "{}",
        err.message
    );
    assert!(!err.message.contains(".method-reader-"), "{}", err.message);
    assert!(!err.message.contains("partial"), "{}", err.message);
}

#[test]
fn a_clean_up_that_fails_too_does_not_hide_why_the_copy_failed() {
    // A directory where the partial goes: the copy onto it fails (EISDIR),
    // and taking it away fails as well, since it is not empty.
    let (_s, dir) = temp();
    let src = dir.join("a.txt");
    fs::write(&src, "v1").unwrap();
    let dest = dir.join("b.txt");
    fs::write(&dest, "old").unwrap();
    fs::create_dir(partial_path_for(&dest)).unwrap();
    fs::write(partial_path_for(&dest).join("inside"), "x").unwrap();
    let err = replace_file(&src, &dest).unwrap_err();
    assert_eq!(err.code.as_deref(), Some("EISDIR"), "{}", err.message);
    assert!(err.message.contains(&dest.display().to_string()));
    assert_eq!(read(&dest), "old");
}

#[test]
fn the_copy_is_flushed_to_the_device_before_it_replaces_the_old_book() {
    let (_s, dir) = temp();
    let src = dir.join("a.azw3");
    let dest = dir.join("b.azw3");
    fs::write(&src, "the new book").unwrap();
    fs::write(&dest, "the old book").unwrap();
    let mut seen: Vec<(String, String)> = Vec::new();
    replace_file_with(&src, &dest, |_| {
        seen.push((read(&partial_path_for(&dest)), read(&dest)));
        Ok(())
    })
    .unwrap();
    assert_eq!(
        seen,
        [("the new book".to_string(), "the old book".to_string())]
    );
    assert_eq!(read(&dest), "the new book");
    assert_eq!(listing(&dir), ["a.azw3", "b.azw3"]);
}

#[test]
fn a_flush_that_fails_leaves_the_old_book_and_takes_the_partial_away() {
    let (_s, dir) = temp();
    let src = dir.join("a.azw3");
    let dest = dir.join("b.azw3");
    fs::write(&src, "the new book").unwrap();
    fs::write(&dest, "the old book").unwrap();
    let err = replace_file_with(&src, &dest, |_| Err(eio())).unwrap_err();
    assert!(err.message.contains("EIO"), "{}", err.message);
    assert_eq!(err.message, "EIO: i/o error, fsync");
    assert_eq!(read(&dest), "the old book");
    assert_eq!(listing(&dir), ["a.azw3", "b.azw3"]);
}

#[cfg(unix)]
fn eio() -> io::Error {
    io::Error::from_raw_os_error(5) // EIO is 5 on macOS and Linux alike.
}

#[cfg(windows)]
fn eio() -> io::Error {
    io::Error::from_raw_os_error(1117) // ERROR_IO_DEVICE, libuv's EIO
}

#[cfg(unix)]
#[test]
fn a_partial_planted_as_a_link_is_never_written_through() {
    // A volume formatted HFS+ or ext4 can hold a link where the partial
    // goes; fs::copy opened it with truncate and wrote the book into
    // whatever it pointed at, outside the reader.
    let (_s, dir) = temp();
    let victim = dir.join("victim.txt");
    fs::write(&victim, "keep me").unwrap();
    let src = dir.join("book.epub");
    fs::write(&src, "the book").unwrap();
    let dest = dir.join("out").join("book.epub");
    fs::create_dir_all(dest.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&victim, partial_path_for(&dest)).unwrap();
    replace_file(&src, &dest).unwrap();
    assert_eq!(read(&victim), "keep me");
    assert_eq!(read(&dest), "the book");
}
