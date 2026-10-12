//! src/atomic-write.ts has no test file of its own: its checks live in
//! tests/library-images.test.ts (the temp name, asked of the module rather
//! than spelled out) and tests/desktop-shell.test.ts (which counts its
//! callers). These pin the same promises here, where the module now is.

use sp_fsutil::{copy_file_atomic, temp_path_for, write_file_atomic};
use std::fs;
use std::path::Path;

fn scratch() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("screepub-atomic-write-")
        .tempdir()
        .unwrap()
}

fn names(dir: &Path) -> Vec<String> {
    let mut n: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    n.sort();
    n
}

#[test]
fn the_temp_sibling_is_hidden_beside_the_file_and_carries_the_pid() {
    let path = Path::new("/lib/Script/settings.json");
    let tmp = temp_path_for(path);
    assert_eq!(tmp.parent(), path.parent());
    assert_eq!(
        tmp.file_name().unwrap().to_string_lossy(),
        format!(".settings.json.{}.tmp", std::process::id())
    );
}

#[test]
fn a_write_lands_whole_and_leaves_no_temp_behind() {
    let s = scratch();
    let path = s.path().join("source.json");
    fs::write(&path, "old").unwrap();
    write_file_atomic(&path, "new").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    assert_eq!(names(s.path()), ["source.json"]);
}

#[test]
fn a_copy_lands_whole_and_leaves_no_temp_behind() {
    let s = scratch();
    let src = s.path().join("a.png");
    let dest = s.path().join("b.png");
    fs::write(&src, [1u8, 2, 3]).unwrap();
    copy_file_atomic(&src, &dest).unwrap();
    assert_eq!(fs::read(&dest).unwrap(), [1, 2, 3]);
    assert_eq!(names(s.path()), ["a.png", "b.png"]);
}

#[test]
fn a_write_that_cannot_be_promoted_keeps_the_old_contents_and_takes_the_temp_away() {
    // A folder at the path refuses the rename.
    let s = scratch();
    let path = s.path().join("taken");
    fs::create_dir(&path).unwrap();
    fs::write(path.join("inside"), "kept").unwrap();
    let err = write_file_atomic(&path, "new").unwrap_err();
    assert!(err.message.contains("rename"), "{}", err.message);
    assert_eq!(names(s.path()), ["taken"]);
    assert_eq!(fs::read_to_string(path.join("inside")).unwrap(), "kept");
}

#[test]
fn a_write_into_a_missing_folder_says_enoent_on_the_temp_open_as_node_does() {
    let s = scratch();
    let path = s.path().join("no-such-folder").join("x.json");
    let err = write_file_atomic(&path, "x").unwrap_err();
    assert_eq!(err.code.as_deref(), Some("ENOENT"));
    assert_eq!(
        err.message,
        format!(
            "ENOENT: no such file or directory, open '{}'",
            temp_path_for(&path).display()
        )
    );
}

#[test]
fn a_copy_of_a_missing_source_fails_and_leaves_nothing() {
    let s = scratch();
    let err = copy_file_atomic(&s.path().join("gone"), &s.path().join("b")).unwrap_err();
    assert_eq!(err.code.as_deref(), Some("ENOENT"));
    assert!(names(s.path()).is_empty());
}
