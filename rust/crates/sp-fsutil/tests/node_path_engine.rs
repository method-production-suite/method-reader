//! The cases sp-engine's own copy of node:path was checked with, kept when
//! the two copies were folded into this one: resolve against a given
//! folder, basename with an extension, and the win32 answers the engine's
//! settings and library paths need.

use sp_fsutil::nodepath::{posix, win32};

#[test]
fn posix_matches_node() {
    assert_eq!(posix::join(&["/a/b", "../c", "d/"]), "/a/c/d/");
    assert_eq!(posix::join(&["", ""]), ".");
    assert_eq!(posix::normalize("a/../.."), "..");
    assert_eq!(posix::resolve_from("/cwd", &["x", "../y"]), "/cwd/y");
    assert_eq!(posix::resolve_from("/cwd", &["/abs/", "z"]), "/abs/z");
    assert_eq!(posix::resolve_from("/cwd", &["/"]), "/");
    assert_eq!(posix::dirname("/a/b/"), "/a");
    assert_eq!(posix::dirname("a"), ".");
    assert_eq!(posix::dirname("/a"), "/");
    assert_eq!(posix::basename("/a/b.txt/"), "b.txt");
    assert_eq!(posix::basename_ext("/a/b.txt", ".txt"), "b");
    assert_eq!(posix::basename_ext(".txt", ".txt"), "");
    assert_eq!(posix::basename_ext("/a/b.txt/", ".txt"), "b");
    assert_eq!(posix::basename_ext("/a/b.txt", ""), "b.txt");
    assert_eq!(posix::basename_ext("/a/b.txt", ".epub"), "b.txt");
    assert_eq!(posix::extname(".hidden"), "");
    assert_eq!(posix::extname("a."), ".");
    assert_eq!(posix::extname(".."), "");
    assert_eq!(posix::extname("..."), ".");
    assert_eq!(posix::extname(".a.b"), ".b");
    assert_eq!(posix::extname("/x.y/z"), "");
}

#[test]
fn win32_matches_node() {
    assert!(win32::is_absolute("C:\\Users\\Ada"));
    assert!(win32::is_absolute("\\Users"));
    assert!(!win32::is_absolute("C:Users"));
    assert!(!win32::is_absolute("Users"));
    assert_eq!(
        win32::join(&["C:\\Users\\Ada", "Documents", "Method Reader"]),
        "C:\\Users\\Ada\\Documents\\Method Reader"
    );
    assert_eq!(win32::normalize("C:/a//b/../c"), "C:\\a\\c");
    assert_eq!(win32::resolve_from("/cwd", &["C:\\a\\..\\b"]), "C:\\b");
    assert_eq!(win32::basename("C:\\a\\b.epub"), "b.epub");
    assert_eq!(win32::basename_ext("C:\\a\\b.epub", ".epub"), "b");
    assert_eq!(win32::dirname("C:\\a\\b.epub"), "C:\\a");
    assert_eq!(win32::dirname("C:\\a"), "C:\\");
    assert_eq!(win32::extname("C:\\a\\b.epub"), ".epub");
}

#[test]
fn join_takes_owned_strings_too() {
    let parts = vec!["/lib".to_owned(), "Book.epub".to_owned()];
    assert_eq!(posix::join(&parts), "/lib/Book.epub");
}
