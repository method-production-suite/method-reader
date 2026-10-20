//! The scene lengths a conversion measured, kept in the script folder's
//! source.json beside the .fountain, and read back only for the very
//! .fountain they were measured with.

mod common;

use common::{Scratch, names, read, read_json, write};
use serde_json::json;
use sp_engine::library::{library_output, read_scene_measures, save_scene_measures};
use sp_engine::nodepath::host;
use sp_fdx::SceneMeasure;

const SCRIPT: &str = "INT. A - DAY\n\nOne.\n\n= pg 2\n\nINT. B - DAY\n\nTwo.\n";

fn measures() -> Vec<SceneMeasure> {
    vec![
        SceneMeasure {
            page: Some("1".into()),
            eighths: 7,
        },
        SceneMeasure {
            page: None,
            eighths: 0,
        },
    ]
}

/// A library folder for Draft.pdf, its .fountain written: the path.
fn library_script(s: &Scratch) -> String {
    let root = s.dir("lib");
    let pdf = host::join(&[s.dir("scripts"), "Draft.pdf".into()]);
    write(&pdf, "x");
    let stem = library_output(&pdf, &root).unwrap();
    let fountain = format!("{stem}.fountain");
    write(&fountain, SCRIPT);
    fountain
}

fn source_json(fountain: &str) -> String {
    host::join(&[host::dirname(fountain), "source.json".into()])
}

#[test]
fn saved_lengths_sit_beside_the_source_and_read_back() {
    let s = Scratch::new("scene-measures");
    let fountain = library_script(&s);
    let before = read_json(&source_json(&fountain));

    save_scene_measures(&fountain, SCRIPT, Some(&measures())).unwrap();

    let after = read_json(&source_json(&fountain));
    assert_eq!(
        after["source"], before["source"],
        "the folder's source is kept"
    );
    assert_eq!(
        after["scenes"]["measured"],
        json!([{"eighths": 7, "page": "1"}, {"eighths": 0}])
    );
    assert_eq!(
        after["scenes"]["fountainSha256"].as_str().unwrap().len(),
        64,
        "the digest of the .fountain they were measured with"
    );
    assert_eq!(read_scene_measures(&fountain, SCRIPT), Some(measures()));
    // Nothing else lands in the folder.
    let folder = host::dirname(&fountain);
    assert_eq!(names(&folder), ["Draft.fountain", "source.json"]);
}

#[test]
fn an_edited_fountain_has_no_saved_lengths() {
    let s = Scratch::new("scene-measures");
    let fountain = library_script(&s);
    save_scene_measures(&fountain, SCRIPT, Some(&measures())).unwrap();
    let edited = SCRIPT.replace("Two.", "Two, and more.");
    assert_eq!(read_scene_measures(&fountain, &edited), None);
}

#[test]
fn a_conversion_with_nothing_measured_clears_the_old_lengths() {
    let s = Scratch::new("scene-measures");
    let fountain = library_script(&s);
    save_scene_measures(&fountain, SCRIPT, Some(&measures())).unwrap();
    save_scene_measures(&fountain, SCRIPT, None).unwrap();
    let marker = read_json(&source_json(&fountain));
    assert!(marker.get("scenes").is_none(), "{marker}");
    assert_eq!(read_scene_measures(&fountain, SCRIPT), None);
}

#[test]
fn outside_the_library_nothing_is_written() {
    let s = Scratch::new("scene-measures");
    let fountain = host::join(&[s.dir("loose"), "Loose.fountain".into()]);
    write(&fountain, SCRIPT);
    save_scene_measures(&fountain, SCRIPT, Some(&measures())).unwrap();
    assert_eq!(names(&host::dirname(&fountain)), ["Loose.fountain"]);
    assert_eq!(read_scene_measures(&fountain, SCRIPT), None);
}

#[test]
fn the_folder_is_still_the_script_s_own_with_lengths_in_its_marker() {
    // The folder's owner is read from "source" alone, so the next
    // conversion of the same PDF lands in the same folder, not a hashed one.
    let s = Scratch::new("scene-measures");
    let fountain = library_script(&s);
    save_scene_measures(&fountain, SCRIPT, Some(&measures())).unwrap();
    let pdf = host::join(&[s.dir("scripts"), "Draft.pdf".into()]);
    let again = library_output(&pdf, &s.dir("lib")).unwrap();
    assert_eq!(format!("{again}.fountain"), fountain);
    assert!(read(&source_json(&fountain)).contains("\"scenes\""));
}

#[test]
fn a_marker_from_an_older_version_reads_as_no_lengths() {
    let s = Scratch::new("scene-measures");
    let fountain = library_script(&s);
    // 0.13's marker: only the source.
    assert_eq!(read_scene_measures(&fountain, SCRIPT), None);
    // And a hand-damaged entry is ignored, not trusted.
    let path = source_json(&fountain);
    let mut marker = read_json(&path);
    marker["scenes"] = json!({"fountainSha256": "x", "measured": "nonsense"});
    write(&path, &marker.to_string());
    assert_eq!(read_scene_measures(&fountain, SCRIPT), None);
}
