//! tests/device-remarkable.test.ts, ported. The stub records the request
//! sequence: the real interface's /upload writes into the LAST-LISTED
//! folder, so "upload to root" is only true if root is listed immediately
//! before the POST. These go through the real ureq client.

mod common;
use common::{Stub, s, scratch};
use sp_device::remarkable::{
    FilePart, HttpClient, REMARKABLE_ENDPOINT, REMARKABLE_MAX_UPLOAD_BYTES, UreqClient,
    multipart_body, probe_remarkable, remarkable_accepts, upload_to_remarkable,
};
use std::sync::atomic::Ordering;
use std::time::Duration;

fn book(scratch: &std::path::Path, name: &str) -> String {
    let dir = tempfile::Builder::new()
        .prefix("test-")
        .tempdir_in(scratch)
        .unwrap()
        .keep();
    let path = dir.join(name);
    std::fs::write(&path, "epub").unwrap();
    s(&path)
}

#[test]
fn the_fixed_usb_endpoint_is_the_documented_address() {
    assert_eq!(REMARKABLE_ENDPOINT, "http://10.11.99.1");
}

#[test]
fn upload_lists_the_root_folder_immediately_before_posting() {
    let sc = scratch("method-reader-device-remarkable-");
    let stub = Stub::start(200);
    upload_to_remarkable(&UreqClient, &book(sc.path(), "Script.epub"), &stub.url).unwrap();
    assert_eq!(stub.requests(), ["GET /documents/", "POST /upload"]);
}

#[test]
fn probe_asks_for_the_documents_listing_not_the_bare_root() {
    let stub = Stub::start(200);
    assert!(probe_remarkable(
        &UreqClient,
        &stub.url,
        Duration::from_millis(1500)
    ));
    assert_eq!(stub.requests(), ["GET /documents/"]);
}

#[test]
fn a_failed_root_listing_aborts_the_send_with_no_blind_post() {
    let sc = scratch("method-reader-device-remarkable-");
    let stub = Stub::start(200);
    stub.documents_status.store(500, Ordering::SeqCst);
    let err =
        upload_to_remarkable(&UreqClient, &book(sc.path(), "Script.epub"), &stub.url).unwrap_err();
    assert_eq!(
        err.message,
        "couldn't open the tablet's root folder (HTTP 500); nothing was uploaded."
    );
    assert!(!stub.requests().contains(&"POST /upload".to_string()));
}

#[test]
fn a_file_over_the_100_mb_cap_is_rejected_before_any_network_request() {
    let sc = scratch("method-reader-device-remarkable-");
    let stub = Stub::start(200);
    let big = sc.path().join("big.epub");
    std::fs::File::create(&big)
        .unwrap()
        .set_len(REMARKABLE_MAX_UPLOAD_BYTES + 1)
        .unwrap(); // sparse
    let err = upload_to_remarkable(&UreqClient, &s(&big), &stub.url).unwrap_err();
    assert!(err.message.contains("100 MB"), "{}", err.message);
    assert_eq!(
        err.message,
        "this file is 100 MB; the tablet's USB web interface accepts up to 100 MB."
    );
    assert!(stub.requests().is_empty());
}

#[test]
fn only_pdf_and_epub_are_accepted() {
    // The literal `not .azw3.`: the file name itself ends in .azw3, so a
    // message that merely echoed the path would pass a looser check.
    let sc = scratch("method-reader-device-remarkable-");
    let stub = Stub::start(200);
    let err =
        upload_to_remarkable(&UreqClient, &book(sc.path(), "Script.azw3"), &stub.url).unwrap_err();
    assert!(err.message.contains("not .azw3."));
    assert_eq!(err.message, "reMarkable accepts PDF and EPUB, not .azw3.");
    assert!(stub.requests().is_empty());
}

#[test]
fn probe_reports_false_when_nothing_is_serving() {
    assert!(!probe_remarkable(
        &UreqClient,
        "http://127.0.0.1:1",
        Duration::from_millis(500)
    ));
}

#[test]
fn remarkable_accepts_is_the_one_copy_of_the_pdf_epub_rule() {
    assert!(remarkable_accepts("/tmp/Script.pdf"));
    assert!(remarkable_accepts("/tmp/Script.epub"));
    // Case and a dotted stem must not fool it: the extension is the LAST dot.
    assert!(remarkable_accepts("/tmp/Script.EPUB"));
    assert!(!remarkable_accepts("/tmp/Draft.epub.azw3"));
    assert!(!remarkable_accepts("/tmp/Script.azw3"));
    assert!(!remarkable_accepts("/tmp/Script.mobi"));
    assert!(!remarkable_accepts("/tmp/Script"));
}

// Beyond the TypeScript file: what only the Rust side has to get right.

#[test]
fn an_unreachable_tablet_is_named_in_bun_s_words() {
    let sc = scratch("method-reader-device-remarkable-");
    let err = upload_to_remarkable(
        &UreqClient,
        &book(sc.path(), "Script.epub"),
        "http://127.0.0.1:1",
    )
    .unwrap_err();
    assert_eq!(
        err.message,
        "Unable to connect. Is the computer able to access the url?"
    );
}

#[test]
fn a_tablet_that_never_answers_times_out_in_bun_s_words() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let err = UreqClient
        .get(&format!("{url}/documents/"), Duration::from_millis(300))
        .unwrap_err();
    assert_eq!(err, "The operation timed out.");
    drop(listener);
}

#[test]
fn a_failed_post_names_its_status() {
    let sc = scratch("method-reader-device-remarkable-");
    let stub = Stub::start(200);
    stub.status.store(413, Ordering::SeqCst);
    stub.documents_status.store(200, Ordering::SeqCst);
    let err =
        upload_to_remarkable(&UreqClient, &book(sc.path(), "Script.pdf"), &stub.url).unwrap_err();
    assert_eq!(err.message, "reMarkable upload failed (HTTP 413).");
}

#[test]
fn the_multipart_body_is_the_one_bun_s_form_data_sends() {
    let part = FilePart {
        field: "file".into(),
        filename: "a \"b\".epub".into(),
        mime: "application/epub+zip".into(),
        bytes: vec![1, 2],
    };
    let body = multipart_body(&part, "----WebKitFormBoundaryX");
    assert_eq!(
        body,
        b"------WebKitFormBoundaryX\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a \"b\".epub\"\r\nContent-Type: application/epub+zip\r\n\r\n\x01\x02\r\n------WebKitFormBoundaryX--\r\n"
    );
}

#[test]
fn a_missing_file_stops_before_any_request_in_node_s_words() {
    let sc = scratch("method-reader-device-remarkable-");
    let stub = Stub::start(200);
    let gone = s(&sc.path().join("gone.epub"));
    let err = upload_to_remarkable(&UreqClient, &gone, &stub.url).unwrap_err();
    assert_eq!(
        err.message,
        format!("ENOENT: no such file or directory, stat '{gone}'")
    );
    assert!(stub.requests().is_empty());
}
