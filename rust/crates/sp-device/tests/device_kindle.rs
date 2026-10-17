//! tests/device-kindle.test.ts, ported.

mod common;
use common::{join, scratch, volume};
use sp_device::kindle::{copy_to_kindle_volume, is_kindle_volume};
use std::path::MAIN_SEPARATOR_STR as SEP;

#[test]
fn a_volume_named_kindle_with_documents_is_detected() {
    let s = scratch("method-reader-device-kindle-");
    assert!(is_kindle_volume(&volume(
        s.path(),
        "Kindle",
        &["documents"]
    )));
}

#[test]
fn an_unnamed_volume_with_documents_and_system_is_detected() {
    let s = scratch("method-reader-device-kindle-");
    assert!(is_kindle_volume(&volume(
        s.path(),
        "NO NAME",
        &["documents", "system"]
    )));
}

#[test]
fn a_system_file_not_a_directory_still_detects_a_kindle() {
    // The check is "did the firmware leave its marker here?", not "is it a
    // folder". Narrowing it to a folder must fail here.
    let s = scratch("method-reader-device-kindle-");
    let dir = volume(s.path(), "NO NAME", &["documents"]);
    std::fs::write(join(&dir, "system"), "a marker file, not a folder").unwrap();
    assert!(is_kindle_volume(&dir));
}

#[test]
fn the_kindle_name_match_is_case_insensitive() {
    let s = scratch("method-reader-device-kindle-");
    assert!(is_kindle_volume(&volume(
        s.path(),
        "KINDLE",
        &["documents"]
    )));
    assert!(is_kindle_volume(&volume(
        s.path(),
        "kindle",
        &["documents"]
    )));
    assert!(is_kindle_volume(&volume(
        s.path(),
        "Sams Kindle",
        &["documents"]
    )));
}

#[test]
fn a_volume_without_documents_is_rejected() {
    let s = scratch("method-reader-device-kindle-");
    assert!(!is_kindle_volume(&volume(s.path(), "Kindle-empty", &[])));
}

#[test]
fn a_generic_thumb_drive_is_rejected() {
    let s = scratch("method-reader-device-kindle-");
    assert!(!is_kindle_volume(&volume(
        s.path(),
        "USB STICK",
        &["documents"]
    )));
}

#[test]
fn a_documents_file_does_not_make_it_a_kindle() {
    let s = scratch("method-reader-device-kindle-");
    let dir = volume(s.path(), "Kindle-file", &[]);
    std::fs::write(join(&dir, "documents"), "not a directory").unwrap();
    assert!(!is_kindle_volume(&dir));
}

#[test]
fn copy_lands_in_documents_keeps_the_content_and_overwrites_on_a_second_copy() {
    let s = scratch("method-reader-device-kindle-");
    let vol = volume(s.path(), "Kindle", &["documents"]);
    let src = join(&join(&vol, ".."), "Test.epub");
    std::fs::write(&src, "v1").unwrap();

    let dest = copy_to_kindle_volume(&src, &vol).unwrap();
    assert!(
        dest.ends_with(&format!("Kindle{SEP}documents{SEP}Test.epub")),
        "{dest}"
    );
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "v1");

    std::fs::write(&src, "v2").unwrap();
    copy_to_kindle_volume(&src, &vol).unwrap();
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "v2");
}

#[cfg(unix)]
#[test]
fn a_documents_folder_that_is_a_link_off_the_volume_is_refused() {
    let s = scratch("method-reader-device-kindle-");
    let vol = volume(s.path(), "Kindle", &["system"]);
    let elsewhere = join(&join(&vol, ".."), "elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, join(&vol, "documents")).unwrap();
    let src = join(&join(&vol, ".."), "Test.epub");
    std::fs::write(&src, "v1").unwrap();
    let err = copy_to_kindle_volume(&src, &vol).unwrap_err();
    assert!(err.message.contains("documents"), "{}", err.message);
    assert_eq!(
        std::fs::read_dir(&elsewhere).unwrap().count(),
        0,
        "nothing written off the volume"
    );
}
