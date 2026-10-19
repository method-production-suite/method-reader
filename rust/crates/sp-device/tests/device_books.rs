//! What is on a reader, and removing a book from it (docs/device-library.md).
//!
//! NO TEST HERE TOUCHES A REAL READER. Every one builds a fake volume in a
//! scratch folder and hands in the device list, as eject's tests do.

mod common;
use common::{s, scratch, volume};
use sp_device::books::{BooksDeps, device_books_command, device_remove_command};
use sp_device::{ConnectedDevice, DeviceKind, ErrorCode};
use std::path::Path;

fn write(root: &str, rel: &str, bytes: &[u8]) -> String {
    let p = Path::new(root).join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, bytes).unwrap();
    s(&p)
}

fn listed(kind: DeviceKind, vol: &str) -> Vec<ConnectedDevice> {
    vec![ConnectedDevice::mounted(kind, "Reader", vol)]
}

fn names(vol: &str, kind: DeviceKind) -> Vec<String> {
    let devices = listed(kind, vol);
    let list = move || devices.clone();
    let r = device_books_command(
        vol,
        &BooksDeps {
            devices: Some(&list),
        },
    )
    .unwrap();
    r.books.into_iter().map(|b| b.name).collect()
}

#[test]
fn a_kindle_lists_its_documents_folder_at_every_depth_and_nothing_else() {
    let tmp = scratch("books");
    let vol = volume(tmp.path(), "Kindle", &["documents", "system"]);
    write(&vol, "documents/Field Station.azw3", b"a");
    write(&vol, "documents/Older/Draft.mobi", b"bb");
    write(&vol, "documents/Field Station.sdr/Field Station.apnx", b"x");
    write(&vol, "documents/notes.txt", b"t");
    write(&vol, "documents/cover.jpg", b"j");
    write(&vol, "system/thumbnails/a.mobi", b"no");
    write(&vol, "stray.epub", b"no");
    assert_eq!(
        names(&vol, DeviceKind::Kindle),
        ["Field Station.azw3", "notes.txt", "Older/Draft.mobi"]
    );
}

#[test]
fn a_kobo_lists_the_whole_volume_but_not_dot_folders() {
    let tmp = scratch("books");
    let vol = volume(tmp.path(), "KOBOeReader", &[".kobo"]);
    write(&vol, "Field Station.kepub.epub", b"a");
    write(&vol, "Scripts/Second.epub", b"b");
    write(&vol, ".kobo/KoboReader.sqlite", b"db");
    write(&vol, ".kobo/kepub/hidden.epub", b"no");
    write(&vol, ".Trashes/old.epub", b"no");
    let devices = listed(DeviceKind::Kobo, &vol);
    let list = move || devices.clone();
    let r = device_books_command(
        &vol,
        &BooksDeps {
            devices: Some(&list),
        },
    )
    .unwrap();
    let got: Vec<(String, u64, String)> = r
        .books
        .iter()
        .map(|b| (b.name.clone(), b.size, b.format.clone()))
        .collect();
    assert_eq!(
        got,
        [
            (
                "Field Station.kepub.epub".to_string(),
                1,
                "kepub".to_string()
            ),
            ("Scripts/Second.epub".to_string(), 1, "epub".to_string()),
        ]
    );
    assert_eq!(
        r.books[0].path,
        s(&Path::new(&vol).join("Field Station.kepub.epub"))
    );
}

#[test]
fn a_tolino_lists_its_books_folder() {
    let tmp = scratch("books");
    let vol = volume(tmp.path(), "tolino", &["Books"]);
    write(&vol, "Books/Field Station.epub", b"a");
    write(&vol, "loose.epub", b"no");
    assert_eq!(names(&vol, DeviceKind::Tolino), ["Field Station.epub"]);
}

#[test]
fn a_kindle_with_no_documents_folder_lists_nothing() {
    let tmp = scratch("books");
    let vol = volume(tmp.path(), "Kindle", &["system"]);
    assert!(names(&vol, DeviceKind::Kindle).is_empty());
}

#[cfg(unix)]
#[test]
fn a_link_is_never_followed() {
    let tmp = scratch("books");
    let outside = volume(tmp.path(), "Elsewhere", &[]);
    write(&outside, "secret.epub", b"no");
    let vol = volume(tmp.path(), "KOBOeReader", &[]);
    std::os::unix::fs::symlink(&outside, Path::new(&vol).join("Linked")).unwrap();
    std::os::unix::fs::symlink(
        Path::new(&outside).join("secret.epub"),
        Path::new(&vol).join("alias.epub"),
    )
    .unwrap();
    assert!(names(&vol, DeviceKind::Kobo).is_empty());
}

#[test]
fn only_a_listed_reader_is_read() {
    let tmp = scratch("books");
    let vol = volume(tmp.path(), "Kindle", &["documents"]);
    let list = Vec::new;
    let err = device_books_command(
        &vol,
        &BooksDeps {
            devices: Some(&list),
        },
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::UnknownDevice);
    let err = device_books_command(
        "Volumes/Kindle",
        &BooksDeps {
            devices: Some(&list),
        },
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::Usage);
}

// ---- removing ----

fn remove(vol: &str, kind: DeviceKind, file: &str) -> Result<Vec<String>, ErrorCode> {
    let devices = listed(kind, vol);
    let list = move || devices.clone();
    device_remove_command(
        vol,
        file,
        &BooksDeps {
            devices: Some(&list),
        },
    )
    .map(|r| r.removed)
    .map_err(|e| e.code)
}

#[test]
fn removing_a_kindle_book_takes_its_sdr_folder_with_it() {
    let tmp = scratch("books");
    let vol = volume(tmp.path(), "Kindle", &["documents"]);
    let book = write(&vol, "documents/Field Station.azw3", b"a");
    write(&vol, "documents/Field Station.sdr/Field Station.apnx", b"x");
    let other = write(&vol, "documents/Other.azw3", b"b");
    let removed = remove(&vol, DeviceKind::Kindle, &book).unwrap();
    let sdr = s(&Path::new(&vol).join("documents/Field Station.sdr"));
    assert_eq!(removed, [book.clone(), sdr.clone()]);
    assert!(!Path::new(&book).exists() && !Path::new(&sdr).exists());
    assert!(Path::new(&other).exists());
}

#[test]
fn removing_a_kobo_book_leaves_its_database_alone() {
    let tmp = scratch("books");
    let vol = volume(tmp.path(), "KOBOeReader", &[".kobo"]);
    let book = write(&vol, "Field Station.kepub.epub", b"a");
    let db = write(&vol, ".kobo/KoboReader.sqlite", b"db");
    assert_eq!(
        remove(&vol, DeviceKind::Kobo, &book).unwrap(),
        std::slice::from_ref(&book)
    );
    assert!(!Path::new(&book).exists());
    assert_eq!(std::fs::read(&db).unwrap(), b"db");
}

#[test]
fn nothing_outside_the_books_folder_or_not_a_book_is_removed() {
    let tmp = scratch("books");
    let vol = volume(tmp.path(), "Kindle", &["documents", "system"]);
    let outside = write(&vol, "system/version.txt", b"keep");
    let stray = write(&vol, "stray.epub", b"keep");
    let image = write(&vol, "documents/cover.jpg", b"keep");
    let sdr = s(&Path::new(&vol).join("documents/A.sdr"));
    std::fs::create_dir_all(&sdr).unwrap();
    let up = s(&Path::new(&vol).join("documents/../stray.epub"));
    let missing = s(&Path::new(&vol).join("documents/Gone.azw3"));
    for file in [&outside, &stray, &image, &sdr, &up, &missing] {
        assert_eq!(
            remove(&vol, DeviceKind::Kindle, file),
            Err(ErrorCode::NotABook),
            "{file}"
        );
    }
    for kept in [&outside, &stray, &image] {
        assert!(Path::new(kept).exists(), "{kept}");
    }
    assert!(Path::new(&sdr).is_dir());
}

#[cfg(unix)]
#[test]
fn a_book_reached_through_a_link_is_not_removed() {
    let tmp = scratch("books");
    let outside = volume(tmp.path(), "Elsewhere", &[]);
    let target = write(&outside, "keep.epub", b"keep");
    let vol = volume(tmp.path(), "KOBOeReader", &[]);
    std::os::unix::fs::symlink(&outside, Path::new(&vol).join("Linked")).unwrap();
    let through = s(&Path::new(&vol).join("Linked/keep.epub"));
    assert_eq!(
        remove(&vol, DeviceKind::Kobo, &through),
        Err(ErrorCode::NotABook)
    );
    assert!(Path::new(&target).exists());
}

#[test]
fn removing_needs_a_listed_reader_and_a_full_path() {
    let tmp = scratch("books");
    let vol = volume(tmp.path(), "Kindle", &["documents"]);
    let book = write(&vol, "documents/A.azw3", b"a");
    let list = Vec::new;
    let err = device_remove_command(
        &vol,
        &book,
        &BooksDeps {
            devices: Some(&list),
        },
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::UnknownDevice);
    assert_eq!(
        remove(&vol, DeviceKind::Kindle, "documents/A.azw3"),
        Err(ErrorCode::Usage)
    );
    assert!(Path::new(&book).exists());
}

#[test]
fn a_remarkable_has_no_volume_to_read() {
    let devices = vec![ConnectedDevice {
        kind: DeviceKind::Remarkable,
        name: "reMarkable".into(),
        volume: None,
        transport: None,
        serial: None,
    }];
    let list = move || devices.clone();
    let err = device_books_command(
        "/remarkable",
        &BooksDeps {
            devices: Some(&list),
        },
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::UnknownDevice);
}
