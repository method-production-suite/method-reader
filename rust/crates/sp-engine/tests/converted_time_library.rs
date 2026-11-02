//! When a library conversion converted, kept in the script folder's
//! source.json beside the .fountain: the Library lists that time, so
//! editing the script outside the app does not move its date. An older
//! folder, with no time kept, falls back to the .fountain's modified time.

mod common;

use std::time::{Duration, UNIX_EPOCH};

use common::{Scratch, names, read_json, write};
use serde_json::{Value, json};
use sp_engine::commands::library::list_library;
use sp_engine::library::{
    library_output, read_converted_time, save_converted_time, save_page_count,
};
use sp_engine::nodepath::host;

const SCRIPT: &str = "Title: Draft\n\nINT. A - DAY\n\nOne.\n\n= pg 2\n\nINT. B - DAY\n\nTwo.\n";

/// 2026-11-02T18:00:00Z.
const EVENING: i64 = 1_793_642_400;
/// 2026-11-02T17:00:00Z.
const AFTERNOON: i64 = EVENING - 3600;

/// Its modified time set to `epoch` seconds.
fn stamp(path: &str, epoch: i64) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(UNIX_EPOCH + Duration::from_secs(u64::try_from(epoch).unwrap()))
        .unwrap();
}

/// A library folder for `<name>.pdf` in `root`, its .fountain written and
/// modified at `modified`: the .fountain's path.
fn library_script(s: &Scratch, root: &str, name: &str, modified: i64) -> String {
    let pdf = host::join(&[s.dir("scripts"), format!("{name}.pdf")]);
    write(&pdf, "x");
    let stem = library_output(&pdf, root).unwrap();
    let fountain = format!("{stem}.fountain");
    write(&fountain, SCRIPT);
    stamp(&fountain, modified);
    fountain
}

fn source_json(fountain: &str) -> String {
    host::join(&[host::dirname(fountain), "source.json".into()])
}

fn listing(root: &str) -> Value {
    serde_json::to_value(list_library(root)).unwrap()
}

#[test]
fn a_saved_time_sits_beside_the_source_and_reads_back() {
    let s = Scratch::new("converted-time");
    let root = s.dir("lib");
    let fountain = library_script(&s, &root, "Draft", 0);
    let before = read_json(&source_json(&fountain));
    save_page_count(&fountain, SCRIPT, Some(3)).unwrap();
    save_converted_time(&fountain, EVENING).unwrap();
    let after = read_json(&source_json(&fountain));
    assert_eq!(after["source"], before["source"], "the source is kept");
    assert_eq!(after["pages"]["count"], json!(3), "the page count is kept");
    assert_eq!(after["converted"], json!("2026-11-02T18:00:00Z"));
    assert_eq!(
        read_converted_time(&fountain).as_deref(),
        Some("2026-11-02T18:00:00Z")
    );
    assert_eq!(
        names(&host::dirname(&fountain)),
        ["Draft.fountain", "source.json"]
    );
}

#[test]
fn a_reconversion_moves_the_time() {
    let s = Scratch::new("converted-time");
    let root = s.dir("lib");
    let fountain = library_script(&s, &root, "Draft", 0);
    save_converted_time(&fountain, AFTERNOON).unwrap();
    save_converted_time(&fountain, EVENING).unwrap();
    assert_eq!(
        read_converted_time(&fountain).as_deref(),
        Some("2026-11-02T18:00:00Z")
    );
}

#[test]
fn the_listing_prefers_the_recorded_time_and_an_edit_does_not_move_it() {
    let s = Scratch::new("converted-time");
    let root = s.dir("lib");
    let fountain = library_script(&s, &root, "Draft", AFTERNOON);
    // An older folder: no time kept, so the .fountain's modified time.
    assert_eq!(
        listing(&root)["scripts"][0]["converted"],
        json!("2026-11-02T17:00:00Z")
    );
    save_converted_time(&fountain, EVENING).unwrap();
    assert_eq!(
        listing(&root)["scripts"][0]["converted"],
        json!("2026-11-02T18:00:00Z")
    );
    // Edited outside the app a day later: the date stays the conversion's.
    write(&fountain, &SCRIPT.replace("Two.", "Two, and more."));
    stamp(&fountain, EVENING + 86_400);
    assert_eq!(
        listing(&root)["scripts"][0]["converted"],
        json!("2026-11-02T18:00:00Z")
    );
}

#[test]
fn scripts_come_newest_conversion_first_whatever_their_files_say() {
    let s = Scratch::new("converted-time");
    let root = s.dir("lib");
    // Aster's .fountain was touched last, but Bramble converted last.
    let aster = library_script(&s, &root, "Aster", EVENING + 86_400);
    let bramble = library_script(&s, &root, "Bramble", AFTERNOON);
    save_converted_time(&aster, AFTERNOON).unwrap();
    save_converted_time(&bramble, EVENING).unwrap();
    let order: Vec<Value> = listing(&root)["scripts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].clone())
        .collect();
    assert_eq!(order, [json!("Bramble"), json!("Aster")]);
}

#[test]
fn a_damaged_time_is_ignored_and_the_modified_time_listed() {
    let s = Scratch::new("converted-time");
    let root = s.dir("lib");
    let fountain = library_script(&s, &root, "Draft", AFTERNOON);
    let path = source_json(&fountain);
    let mut marker = read_json(&path);
    for bad in [
        json!(1_793_642_400),
        json!("yesterday"),
        json!("2026-11-02 18:00:00"),
        json!("2026-11-02T18:00:00+01:00"),
        json!("2026-11-02T18:00:00.5Z"),
        json!(null),
        json!({"at": "2026-11-02T18:00:00Z"}),
    ] {
        marker["converted"] = bad.clone();
        write(&path, &marker.to_string());
        assert_eq!(read_converted_time(&fountain), None, "{bad}");
        assert_eq!(
            listing(&root)["scripts"][0]["converted"],
            json!("2026-11-02T17:00:00Z"),
            "{bad}"
        );
    }
}

#[test]
fn outside_the_library_nothing_is_written() {
    let s = Scratch::new("converted-time");
    let fountain = host::join(&[s.dir("loose"), "Loose.fountain".into()]);
    write(&fountain, SCRIPT);
    save_converted_time(&fountain, EVENING).unwrap();
    assert_eq!(names(&host::dirname(&fountain)), ["Loose.fountain"]);
    assert_eq!(read_converted_time(&fountain), None);
}
