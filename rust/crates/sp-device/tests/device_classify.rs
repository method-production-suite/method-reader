//! tests/device-classify.test.ts, ported.

mod common;
use common::{join, scratch, volume};
use sp_device::DeviceKind;
use sp_device::classify::{classify, volume_name};

#[test]
fn a_volume_with_a_kobo_dir_is_kobo() {
    let s = scratch("method-reader-device-classify-");
    assert_eq!(
        classify(&volume(s.path(), "KOBOeReader", &[".kobo"])),
        Some(DeviceKind::Kobo)
    );
}

#[test]
fn a_volume_named_tolino_is_tolino() {
    let s = scratch("method-reader-device-classify-");
    assert_eq!(
        classify(&volume(s.path(), "tolino", &[])),
        Some(DeviceKind::Tolino)
    );
}

#[test]
fn tolino_detection_is_case_insensitive() {
    let s = scratch("method-reader-device-classify-");
    assert_eq!(
        classify(&volume(s.path(), "TOLINO vision", &[])),
        Some(DeviceKind::Tolino)
    );
}

#[test]
fn a_kindle_volume_classifies_as_kindle() {
    let s = scratch("method-reader-device-classify-");
    assert_eq!(
        classify(&volume(s.path(), "Kindle", &["documents"])),
        Some(DeviceKind::Kindle)
    );
}

#[test]
fn a_generic_thumb_drive_is_no_device() {
    let s = scratch("method-reader-device-classify-");
    assert_eq!(
        classify(&volume(s.path(), "USB STICK", &["documents"])),
        None
    );
}

#[test]
fn a_kobo_file_does_not_make_it_a_kobo() {
    // Only a directory counts.
    let s = scratch("method-reader-device-classify-");
    let dir = volume(s.path(), "NotAKobo", &[]);
    std::fs::write(join(&dir, ".kobo"), "x").unwrap();
    assert_eq!(classify(&dir), None);
}

#[test]
fn kindle_wins_over_a_bare_name_collision() {
    // documents/ + system/ is a Kindle even if the name says nothing.
    let s = scratch("method-reader-device-classify-");
    assert_eq!(
        classify(&volume(s.path(), "NO NAME", &["documents", "system"])),
        Some(DeviceKind::Kindle)
    );
}

#[test]
fn volume_name_is_the_directory_name() {
    let s = scratch("method-reader-device-classify-");
    assert_eq!(
        volume_name(&volume(s.path(), "KOBOeReader", &[])),
        "KOBOeReader"
    );
}

#[test]
fn ambiguous_volume_kindle_wins_over_kobo() {
    let s = scratch("method-reader-device-classify-");
    assert_eq!(
        classify(&volume(s.path(), "Kindle", &["documents", ".kobo"])),
        Some(DeviceKind::Kindle)
    );
}

#[test]
fn ambiguous_volume_kobo_wins_over_tolino() {
    let s = scratch("method-reader-device-classify-");
    assert_eq!(
        classify(&volume(s.path(), "tolino", &[".kobo"])),
        Some(DeviceKind::Kobo)
    );
}

#[test]
fn a_windows_drive_root_names_itself_by_its_drive_designator() {
    assert_eq!(volume_name("D:\\"), "D:");
}

#[test]
fn a_lowercase_windows_drive_root_names_itself_by_its_drive_designator_too() {
    assert_eq!(volume_name("d:\\"), "d:");
}

#[cfg(unix)]
#[test]
fn a_windows_shaped_drive_root_with_a_kobo_signature_still_classifies_as_kobo() {
    // Backslash is a plain filename character on this POSIX host, standing in
    // for the shape of a real Windows drive root.
    let s = scratch("method-reader-device-classify-");
    let drive_root = join(&common::s(s.path()), "D:\\");
    std::fs::create_dir_all(join(&drive_root, ".kobo")).unwrap();
    assert_eq!(classify(&drive_root), Some(DeviceKind::Kobo));
}
