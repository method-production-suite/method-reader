//! tests/device-transfer.test.ts, ported.

mod common;
use common::{join, scratch, volume};
use sp_device::transfer::{TransferError, copy_to_device};
use sp_device::{ConnectedDevice, DeviceKind};
use std::path::MAIN_SEPARATOR_STR as SEP;

fn read(p: &str) -> String {
    std::fs::read_to_string(p).unwrap()
}

#[test]
fn copy_to_kindle_lands_in_documents() {
    let s = scratch("method-reader-device-transfer-");
    let vol = volume(s.path(), "Kindle", &["documents"]);
    let src = join(&join(&vol, ".."), "Test.epub");
    std::fs::write(&src, "v1").unwrap();
    let dest = copy_to_device(
        &src,
        &ConnectedDevice::mounted(DeviceKind::Kindle, "Kindle", &vol),
    )
    .unwrap();
    assert!(dest.ends_with(&format!("documents{SEP}Test.epub")));
    assert_eq!(read(&dest), "v1");
}

#[test]
fn copy_to_kobo_lands_in_the_volume_root() {
    let s = scratch("method-reader-device-transfer-");
    let vol = volume(s.path(), "KOBOeReader", &[".kobo"]);
    let src = join(&join(&vol, ".."), "Book.epub");
    std::fs::write(&src, "content").unwrap();
    let dest = copy_to_device(
        &src,
        &ConnectedDevice::mounted(DeviceKind::Kobo, "KOBOeReader", &vol),
    )
    .unwrap();
    assert_eq!(dest, join(&vol, "Book.epub"));
    assert_eq!(read(&dest), "content");
}

#[test]
fn copy_to_tolino_lands_in_the_books_subfolder() {
    let s = scratch("method-reader-device-transfer-");
    let vol = volume(s.path(), "tolino", &[]);
    let src = join(&join(&vol, ".."), "Story.epub");
    std::fs::write(&src, "text").unwrap();
    let dest = copy_to_device(
        &src,
        &ConnectedDevice::mounted(DeviceKind::Tolino, "tolino", &vol),
    )
    .unwrap();
    assert_eq!(dest, join(&join(&vol, "Books"), "Story.epub"));
    assert_eq!(read(&dest), "text");
}

#[test]
fn the_tolino_books_folder_is_made_if_missing() {
    let s = scratch("method-reader-device-transfer-");
    let vol = volume(s.path(), "tolino", &[]);
    let src = join(&join(&vol, ".."), "Book.epub");
    std::fs::write(&src, "data").unwrap();
    copy_to_device(
        &src,
        &ConnectedDevice::mounted(DeviceKind::Tolino, "tolino", &vol),
    )
    .unwrap();
    assert!(std::path::Path::new(&join(&vol, "Books")).is_dir());
}

#[test]
fn copy_overwrites_an_existing_file() {
    let s = scratch("method-reader-device-transfer-");
    let vol = volume(s.path(), "Kindle", &["documents"]);
    let src = join(&join(&vol, ".."), "Test.epub");
    std::fs::write(&src, "v1").unwrap();
    let device = ConnectedDevice::mounted(DeviceKind::Kindle, "Kindle", &vol);
    let dest = copy_to_device(&src, &device).unwrap();
    assert_eq!(read(&dest), "v1");
    std::fs::write(&src, "v2").unwrap();
    copy_to_device(&src, &device).unwrap();
    assert_eq!(read(&dest), "v2");
}

#[test]
fn a_remarkable_is_refused_even_with_a_volume_present() {
    let s = scratch("method-reader-device-transfer-");
    let vol = volume(s.path(), "ReMarkable", &[]);
    let src = join(&join(&vol, ".."), "Book.epub");
    std::fs::write(&src, "content").unwrap();
    let device = ConnectedDevice::mounted(DeviceKind::Remarkable, "reMarkable", &vol);
    let err = copy_to_device(&src, &device).unwrap_err();
    assert_eq!(err, TransferError::NoVolume);
    assert_eq!(err.to_string(), "Device has no mounted volume.");
}

#[test]
fn a_device_with_no_volume_is_refused() {
    let s = scratch("method-reader-device-transfer-");
    let src = join(&common::s(s.path()), "Book.epub");
    std::fs::write(&src, "content").unwrap();
    let device = ConnectedDevice {
        volume: None,
        ..ConnectedDevice::mounted(DeviceKind::Kindle, "Kindle", "")
    };
    assert_eq!(
        copy_to_device(&src, &device).unwrap_err(),
        TransferError::NoVolume
    );
}

#[test]
fn a_kobo_copy_overwrites_an_existing_file_at_the_volume_root() {
    let s = scratch("method-reader-device-transfer-");
    let vol = volume(s.path(), "KOBOeReader", &[".kobo"]);
    let src = join(&join(&vol, ".."), "Book.epub");
    std::fs::write(&src, "v1").unwrap();
    let device = ConnectedDevice::mounted(DeviceKind::Kobo, "KOBOeReader", &vol);
    let dest = copy_to_device(&src, &device).unwrap();
    assert_eq!(read(&dest), "v1");
    std::fs::write(&src, "v2").unwrap();
    copy_to_device(&src, &device).unwrap();
    assert_eq!(read(&dest), "v2");
}

#[test]
fn a_tolino_copy_overwrites_an_existing_file_in_books() {
    let s = scratch("method-reader-device-transfer-");
    let vol = volume(s.path(), "tolino", &[]);
    let src = join(&join(&vol, ".."), "Book.epub");
    std::fs::write(&src, "v1").unwrap();
    let device = ConnectedDevice::mounted(DeviceKind::Tolino, "tolino", &vol);
    let dest = copy_to_device(&src, &device).unwrap();
    assert_eq!(read(&dest), "v1");
    std::fs::write(&src, "v2").unwrap();
    copy_to_device(&src, &device).unwrap();
    assert_eq!(read(&dest), "v2");
}
