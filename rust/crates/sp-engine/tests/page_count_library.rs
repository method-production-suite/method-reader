//! The page count a PDF conversion answered, kept in the script folder's
//! source.json beside the .fountain, and read back only for the very
//! .fountain it was counted with: the Library lists the same page count
//! the conversion showed.

mod common;

use common::{Scratch, names, read_json, write};
use serde_json::json;
use sp_engine::commands::library::list_library;
use sp_engine::library::{library_output, read_page_count, save_page_count, save_scene_measures};
use sp_engine::nodepath::host;
use sp_fdx::SceneMeasure;

/// Two sheets of body (one marker) under a title page: three PDF pages.
const SCRIPT: &str = "Title: Draft\n\nINT. A - DAY\n\nOne.\n\n= pg 2\n\nINT. B - DAY\n\nTwo.\n";

/// A library folder for Draft.pdf, its .fountain written: the path and the
/// library root.
fn library_script(s: &Scratch) -> (String, String) {
    let root = s.dir("lib");
    let pdf = host::join(&[s.dir("scripts"), "Draft.pdf".into()]);
    write(&pdf, "x");
    let stem = library_output(&pdf, &root).unwrap();
    let fountain = format!("{stem}.fountain");
    write(&fountain, SCRIPT);
    (fountain, root)
}

fn source_json(fountain: &str) -> String {
    host::join(&[host::dirname(fountain), "source.json".into()])
}

#[test]
fn a_saved_count_sits_beside_the_source_and_reads_back() {
    let s = Scratch::new("page-count");
    let (fountain, _) = library_script(&s);
    let before = read_json(&source_json(&fountain));
    save_page_count(&fountain, SCRIPT, Some(3)).unwrap();
    let after = read_json(&source_json(&fountain));
    assert_eq!(after["source"], before["source"], "the source is kept");
    assert_eq!(after["pages"]["count"], json!(3));
    assert_eq!(
        after["pages"]["fountainSha256"].as_str().unwrap().len(),
        64,
        "the digest of the .fountain it was counted with"
    );
    assert_eq!(read_page_count(&fountain, SCRIPT), Some(3));
    assert_eq!(
        names(&host::dirname(&fountain)),
        ["Draft.fountain", "source.json"]
    );
}

#[test]
fn the_count_and_the_scene_lengths_keep_each_other() {
    let s = Scratch::new("page-count");
    let (fountain, _) = library_script(&s);
    let measures = [SceneMeasure {
        page: Some("1".into()),
        eighths: 7,
    }];
    save_scene_measures(&fountain, SCRIPT, Some(&measures)).unwrap();
    save_page_count(&fountain, SCRIPT, Some(3)).unwrap();
    let marker = read_json(&source_json(&fountain));
    assert_eq!(
        marker["scenes"]["measured"],
        json!([{"eighths": 7, "page": "1"}])
    );
    save_scene_measures(&fountain, SCRIPT, None).unwrap();
    assert_eq!(read_page_count(&fountain, SCRIPT), Some(3));
}

#[test]
fn an_edited_fountain_has_no_saved_count() {
    let s = Scratch::new("page-count");
    let (fountain, _) = library_script(&s);
    save_page_count(&fountain, SCRIPT, Some(3)).unwrap();
    let edited = SCRIPT.replace("Two.", "Two, and more.");
    assert_eq!(read_page_count(&fountain, &edited), None);
}

#[test]
fn a_conversion_with_no_count_clears_the_old_one() {
    let s = Scratch::new("page-count");
    let (fountain, _) = library_script(&s);
    save_page_count(&fountain, SCRIPT, Some(3)).unwrap();
    save_page_count(&fountain, SCRIPT, None).unwrap();
    let marker = read_json(&source_json(&fountain));
    assert!(marker.get("pages").is_none(), "{marker}");
    assert_eq!(read_page_count(&fountain, SCRIPT), None);
}

#[test]
fn outside_the_library_nothing_is_written() {
    let s = Scratch::new("page-count");
    let fountain = host::join(&[s.dir("loose"), "Loose.fountain".into()]);
    write(&fountain, SCRIPT);
    save_page_count(&fountain, SCRIPT, Some(3)).unwrap();
    assert_eq!(names(&host::dirname(&fountain)), ["Loose.fountain"]);
    assert_eq!(read_page_count(&fountain, SCRIPT), None);
}

#[test]
fn a_damaged_count_is_ignored_not_trusted() {
    let s = Scratch::new("page-count");
    let (fountain, _) = library_script(&s);
    let path = source_json(&fountain);
    let mut marker = read_json(&path);
    let digest = {
        save_page_count(&fountain, SCRIPT, Some(3)).unwrap();
        read_json(&path)["pages"]["fountainSha256"].clone()
    };
    for bad in [
        json!({"fountainSha256": digest, "count": "three"}),
        json!({"fountainSha256": digest, "count": -1}),
        json!({"fountainSha256": digest}),
        json!(3),
    ] {
        marker["pages"] = bad.clone();
        write(&path, &marker.to_string());
        assert_eq!(read_page_count(&fountain, SCRIPT), None, "{bad}");
    }
}

#[test]
fn the_listing_prefers_the_recorded_count_and_falls_back_to_the_markers() {
    let s = Scratch::new("page-count");
    let (fountain, root) = library_script(&s);
    let pages = |root: &str| {
        serde_json::to_value(list_library(root)).unwrap()["scripts"][0]["pages"].clone()
    };
    // An older folder: no count kept, so the highest marker.
    assert_eq!(pages(&root), json!(2));
    // The conversion's own count, title page and all.
    save_page_count(&fountain, SCRIPT, Some(3)).unwrap();
    assert_eq!(pages(&root), json!(3));
    // The .fountain edited since: its markers again, never a stale count.
    write(&fountain, &SCRIPT.replace("Two.", "Two, and more."));
    assert_eq!(pages(&root), json!(2));
}
