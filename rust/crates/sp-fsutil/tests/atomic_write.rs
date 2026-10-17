//! src/atomic-write.ts has no test file of its own: its checks live in
//! tests/library-images.test.ts (the temp name, asked of the module rather
//! than spelled out) and tests/desktop-shell.test.ts (which counts its
//! callers). These pin the same promises here, where the module now is.

use sp_fsutil::{copy_file_atomic, temp_path_for, write_file_atomic};
use std::fs;
use std::path::Path;

fn scratch() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("method-reader-atomic-write-")
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
    let name = tmp.file_name().unwrap().to_string_lossy().into_owned();
    let prefix = format!(".settings.json.{}-", std::process::id());
    let n = name
        .strip_prefix(&prefix)
        .and_then(|r| r.strip_suffix(".tmp"))
        .unwrap_or_else(|| panic!("{name}"));
    assert!(
        !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()),
        "{name}"
    );
    assert_ne!(temp_path_for(path), tmp, "every write gets its own");
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
    // Windows answers ACCESS_DENIED here; the reason a person reads is the
    // folder, on every system.
    assert_eq!(err.code.as_deref(), Some("EISDIR"), "{}", err.message);
    assert_eq!(names(s.path()), ["taken"]);
    assert_eq!(fs::read_to_string(path.join("inside")).unwrap(), "kept");
}

#[test]
fn a_write_into_a_missing_folder_says_enoent_on_the_temp_open_as_node_does() {
    let s = scratch();
    let path = s.path().join("no-such-folder").join("x.json");
    let err = write_file_atomic(&path, "x").unwrap_err();
    assert_eq!(err.code.as_deref(), Some("ENOENT"));
    let temp_start = path.parent().unwrap().join(".x.json.");
    let expected = format!(
        "ENOENT: no such file or directory, open '{}",
        temp_start.display()
    );
    assert!(err.message.starts_with(&expected), "{}", err.message);
    assert!(err.message.ends_with(".tmp'"), "{}", err.message);
}

#[test]
fn a_folder_made_under_a_file_says_enotdir_on_every_system() {
    // Windows answers ALREADY_EXISTS for a path that does not exist.
    let s = scratch();
    let blocker = s.path().join("a-file");
    fs::write(&blocker, "x").unwrap();
    let made = blocker.join("Kobo");
    let io = fs::create_dir_all(&made).unwrap_err();
    let err = sp_fsutil::NodeIoError::from_io(&io, "mkdir", Some(&made), None);
    assert_eq!(err.code.as_deref(), Some("ENOTDIR"), "{}", err.message);
    // A file AT the name is still EEXIST.
    let io = fs::create_dir(&blocker).unwrap_err();
    let err = sp_fsutil::NodeIoError::from_io(&io, "mkdir", Some(&blocker), None);
    assert_eq!(err.code.as_deref(), Some("EEXIST"), "{}", err.message);
}

#[test]
fn reading_a_folder_as_a_file_says_eisdir_on_every_system() {
    // Windows refuses to open a folder with ACCESS_DENIED.
    let s = scratch();
    let io = fs::read(s.path()).unwrap_err();
    let err = sp_fsutil::NodeIoError::from_io(&io, "open", Some(s.path()), None);
    assert_eq!(err.code.as_deref(), Some("EISDIR"), "{}", err.message);
}

#[test]
fn a_copy_of_a_missing_source_fails_and_leaves_nothing() {
    let s = scratch();
    let err = copy_file_atomic(&s.path().join("gone"), &s.path().join("b")).unwrap_err();
    assert_eq!(err.code.as_deref(), Some("ENOENT"));
    assert!(names(s.path()).is_empty());
}

#[test]
fn two_threads_writing_one_file_each_land_whole() {
    // The engine used to be a process per call, so the pid kept temp files
    // apart. In the window every call is a thread of ONE process, and two
    // settings saves at once shared one temp file: one truncated it while
    // the other renamed it, promoting a half-written settings.json, and the
    // second rename failed with ENOENT.
    let s = scratch();
    let path = s.path().join("settings.json");
    let payloads: Vec<Vec<u8>> = (0..8u8).map(|t| vec![b'a' + t; 256 * 1024]).collect();
    std::thread::scope(|scope| {
        for p in &payloads {
            let path = &path;
            scope.spawn(move || {
                for _ in 0..40 {
                    write_file_atomic(path, p).expect("every write succeeds");
                    let now = fs::read(path).unwrap();
                    assert!(
                        now.len() == p.len() && now.iter().all(|&b| b == now[0]),
                        "a reader saw a torn file"
                    );
                }
            });
        }
    });
    assert_eq!(names(s.path()), ["settings.json"]);
}
