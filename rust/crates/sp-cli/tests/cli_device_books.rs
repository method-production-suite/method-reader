//! `device-books` and `device-remove` through the real binary, on an
//! invented Kindle in a scratch folder (docs/device-library.md). No test
//! touches a real reader or the real library: the Sandbox points both at
//! scratch folders.

mod common;
use common::Sandbox;
use serde_json::Value;

/// A sandbox with one invented Kindle, `vols/Kindle`, and a library holding
/// one script whose book is also on the reader.
fn kindle() -> (Sandbox, String) {
    let mut sb = Sandbox::new();
    sb.mkdir("vols/Kindle/documents");
    sb.mkdir("vols/Kindle/system");
    sb.write(
        "library/Field Station/Field Station.azw3",
        b"the library's own bytes",
    );
    sb.write(
        "vols/Kindle/documents/Field Station.azw3",
        b"the library's own bytes",
    );
    sb.write(
        "vols/Kindle/documents/Field Station.sdr/Field Station.apnx",
        b"notes",
    );
    sb.write("vols/Kindle/documents/Someone Else.mobi", b"not ours");
    let vols = sb.p("vols");
    sb.set("METHOD_READER_VOLUME_ROOTS", vols);
    let vol = sb.p("vols/Kindle");
    (sb, vol)
}

#[test]
fn device_books_lists_the_kindles_books_and_names_the_librarys() {
    let (sb, vol) = kindle();
    let v = sb.run(&["device-books", &vol, "--json"]).json();
    assert_eq!(v["ok"], true);
    assert_eq!(v["kind"], "kindle");
    let books: Vec<(String, Value)> = v["books"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| (b["name"].as_str().unwrap().to_owned(), b["library"].clone()))
        .collect();
    assert_eq!(
        books,
        [
            (
                "Field Station.azw3".to_owned(),
                Value::from("Field Station")
            ),
            ("Someone Else.mobi".to_owned(), Value::Null),
        ]
    );
}

#[test]
fn device_remove_takes_the_book_and_its_sdr_and_answers_what_went() {
    let (sb, vol) = kindle();
    let book = sb.p("vols/Kindle/documents/Field Station.azw3");
    let v = sb.run(&["device-remove", &vol, &book, "--json"]).json();
    assert_eq!(v["ok"], true);
    let removed: Vec<&str> = v["removed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r.as_str().unwrap())
        .collect();
    assert_eq!(
        removed,
        [
            book.as_str(),
            sb.p("vols/Kindle/documents/Field Station.sdr").as_str()
        ]
    );
    assert!(!sb.path("vols/Kindle/documents/Field Station.azw3").exists());
    assert!(sb.path("vols/Kindle/documents/Someone Else.mobi").exists());
    // The library's copy is never touched.
    assert!(sb.path("library/Field Station/Field Station.azw3").exists());
}

#[test]
fn the_refusals_answer_the_contract_codes_and_touch_nothing() {
    let (sb, vol) = kindle();
    let outside = sb.write("vols/Kindle/system/keep.txt", b"keep");
    let ran = sb.run(&["device-remove", &vol, &outside.to_string_lossy(), "--json"]);
    assert_eq!(ran.error_code(), "not-a-book");
    assert!(outside.exists());
    let not_a_reader = sb.p("library");
    let ran = sb.run(&["device-books", &not_a_reader, "--json"]);
    assert_eq!(ran.error_code(), "unknown-device");
    let ran = sb.run(&["device-remove", &vol, "--json"]);
    assert_eq!(ran.error_code(), "usage");
}
