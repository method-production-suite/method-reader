//! recent.json: the scripts a person opened, for the suite's launcher
//! (SUITE-STANDARDS.md §2). Every test hands the engine a scratch data
//! folder through METHOD_READER_CONFIG_DIR, never the real one.

mod common;

use serde_json::{Value, json};
use sp_engine::env::Env;
use sp_engine::recent::{
    RECENT_FILE, RECENT_LIMIT, counts_as_opened, read_recent, recent_path, record_conversion,
    record_opened, rfc3339,
};

use common::{Scratch, guarded_env};

/// The engine's environment with the data folder in `scratch/config`.
fn env_in(scratch: &Scratch) -> Env {
    let mut env = guarded_env();
    env.set("METHOD_READER_CONFIG_DIR", scratch.join("config"));
    env
}

fn file_json(path: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).expect("recent.json")).expect("JSON")
}

fn paths(path: &str) -> Vec<String> {
    file_json(path)
        .as_array()
        .expect("an array")
        .iter()
        .map(|e| e["path"].as_str().expect("a path").to_owned())
        .collect()
}

#[test]
fn the_file_sits_in_the_data_folder_beside_settings_json() {
    let s = Scratch::new("recent");
    let env = env_in(&s);
    let platform = sp_engine::env::host_platform();
    let recent = recent_path(platform, &env);
    let settings = sp_engine::settings::app::app_settings_path(platform, &env);
    assert_eq!(RECENT_FILE, "recent.json");
    assert_eq!(
        recent,
        s.path.join("config").join("recent.json").to_string_lossy()
    );
    assert_eq!(
        std::path::Path::new(&recent).parent(),
        std::path::Path::new(&settings).parent()
    );
}

#[test]
fn the_default_folder_is_the_bundle_id_folder() {
    let mut env = Env::new();
    env.set("HOME", "/Users/someone");
    assert_eq!(
        recent_path("darwin", &env),
        "/Users/someone/Library/Application Support/com.methodreader.desktop/recent.json"
    );
}

#[test]
fn the_file_is_exactly_the_standard_shape() {
    let s = Scratch::new("recent");
    let file = s.join("config/recent.json");
    record_opened(&file, "/scripts/One.pdf", "2026-10-18T09:00:00Z").unwrap();
    assert_eq!(
        file_json(&file),
        json!([{ "path": "/scripts/One.pdf", "opened": "2026-10-18T09:00:00Z" }])
    );
    // Key order too: path, then opened.
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.find("\"path\"").unwrap() < text.find("\"opened\"").unwrap());
    assert!(text.ends_with('\n'));
}

#[test]
fn newest_first_and_reopening_moves_to_the_top() {
    let s = Scratch::new("recent");
    let file = s.join("recent.json");
    record_opened(&file, "/s/A.pdf", "2026-10-18T09:00:00Z").unwrap();
    record_opened(&file, "/s/B.pdf", "2026-10-18T09:01:00Z").unwrap();
    record_opened(&file, "/s/C.pdf", "2026-10-18T09:02:00Z").unwrap();
    assert_eq!(paths(&file), ["/s/C.pdf", "/s/B.pdf", "/s/A.pdf"]);

    record_opened(&file, "/s/A.pdf", "2026-10-18T09:03:00Z").unwrap();
    assert_eq!(paths(&file), ["/s/A.pdf", "/s/C.pdf", "/s/B.pdf"]);
    // One entry per path, with the newer time.
    assert_eq!(file_json(&file)[0]["opened"], "2026-10-18T09:03:00Z");
}

#[test]
fn at_most_twenty_entries_the_oldest_going_first() {
    let s = Scratch::new("recent");
    let file = s.join("recent.json");
    assert_eq!(RECENT_LIMIT, 20);
    for i in 0..25 {
        record_opened(&file, &format!("/s/{i}.pdf"), &rfc3339(i)).unwrap();
    }
    let got = paths(&file);
    assert_eq!(got.len(), 20);
    assert_eq!(got[0], "/s/24.pdf");
    assert_eq!(got[19], "/s/5.pdf");
}

#[test]
fn a_missing_file_is_kept_for_the_launcher_to_grey() {
    let s = Scratch::new("recent");
    let file = s.join("recent.json");
    record_opened(&file, "/nowhere/Gone.pdf", "2026-10-18T09:00:00Z").unwrap();
    record_opened(&file, "/nowhere/Also gone.pdf", "2026-10-18T09:01:00Z").unwrap();
    assert_eq!(
        paths(&file),
        ["/nowhere/Also gone.pdf", "/nowhere/Gone.pdf"]
    );
}

#[test]
fn a_corrupt_or_missing_file_reads_as_empty_and_is_replaced() {
    let s = Scratch::new("recent");
    let file = s.join("recent.json");
    assert!(read_recent(&file).is_empty());
    for bad in ["not json", "{\"path\": \"/x\"}", "[1, \"two\", null]", ""] {
        std::fs::write(&file, bad).unwrap();
        assert!(read_recent(&file).is_empty(), "{bad:?}");
        record_opened(&file, "/s/New.pdf", "2026-10-18T09:00:00Z").unwrap();
        assert_eq!(
            file_json(&file),
            json!([{ "path": "/s/New.pdf", "opened": "2026-10-18T09:00:00Z" }])
        );
    }
}

#[test]
fn entries_it_cannot_use_are_dropped_and_extra_keys_are_kept() {
    let s = Scratch::new("recent");
    let file = s.join("recent.json");
    std::fs::write(
        &file,
        r#"[{"path": "/s/Old.pdf", "opened": "2026-10-01T00:00:00Z", "bookmark": "abc"},
            {"opened": "2026-10-02T00:00:00Z"}, 7]"#,
    )
    .unwrap();
    record_opened(&file, "/s/New.pdf", "2026-10-18T09:00:00Z").unwrap();
    assert_eq!(
        file_json(&file),
        json!([
            { "path": "/s/New.pdf", "opened": "2026-10-18T09:00:00Z" },
            { "path": "/s/Old.pdf", "opened": "2026-10-01T00:00:00Z", "bookmark": "abc" }
        ])
    );
}

#[test]
fn the_write_is_atomic_and_leaves_no_temp_file() {
    let s = Scratch::new("recent");
    let dir = s.dir("data");
    let file = format!("{dir}/recent.json");
    record_opened(&file, "/s/A.pdf", "2026-10-18T09:00:00Z").unwrap();
    record_opened(&file, "/s/B.pdf", "2026-10-18T09:01:00Z").unwrap();
    let names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["recent.json"]);
}

// Unix only: a read-only folder is how this makes the write fail, and
// Windows has no mode bits to set (the compiled test also failed there on
// an unused variable once, in the weekly full build).
#[cfg(unix)]
#[test]
fn a_write_that_fails_leaves_the_old_file_whole() {
    use std::os::unix::fs::PermissionsExt;
    let s = Scratch::new("recent");
    let file = s.join("recent.json");
    record_opened(&file, "/s/A.pdf", "2026-10-18T09:00:00Z").unwrap();
    let before = std::fs::read(&file).unwrap();
    // A folder where the temp file would go cannot be written through.
    let dir = s.path.clone();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let failed = record_opened(&file, "/s/B.pdf", "2026-10-18T09:01:00Z");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(failed.is_err());
    assert_eq!(std::fs::read(&file).unwrap(), before);
}

#[test]
fn rfc3339_is_utc_to_the_second() {
    assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
    assert_eq!(rfc3339(1_792_281_600), "2026-10-18T00:00:00Z");
    assert_eq!(rfc3339(1_792_316_045), "2026-10-18T09:34:05Z");
}

#[test]
fn only_a_library_conversion_counts_as_opened() {
    let s = Scratch::new("recent");
    let env = env_in(&s);
    let platform = sp_engine::env::host_platform();
    let script = s.join("scripts/One.pdf");
    assert!(counts_as_opened(platform, &env, &script, true));
    // -o and beside-the-input runs are a person's own pipeline, and the
    // window's own rebuilds (a retune, the sample) go through -o.
    assert!(!counts_as_opened(platform, &env, &script, false));
}

#[test]
fn nothing_inside_the_data_folder_counts_so_the_sample_is_never_recorded() {
    let s = Scratch::new("recent");
    let env = env_in(&s);
    let platform = sp_engine::env::host_platform();
    let sample = s.join("config/Sample/Field Station.pdf");
    assert!(!counts_as_opened(platform, &env, &sample, true));
    assert!(!counts_as_opened(platform, &env, &sample, false));
    // A folder that only starts with the same letters is not inside it.
    let beside = s.join("config-other/Field Station.pdf");
    assert!(counts_as_opened(platform, &env, &beside, true));
}

#[test]
fn record_conversion_writes_the_absolute_source_path() {
    let s = Scratch::new("recent");
    let env = env_in(&s);
    let platform = sp_engine::env::host_platform();
    // Built with the platform's own separator: the engine records the path
    // as the platform spells it (backslashes on Windows).
    let script = s
        .path
        .join("scripts")
        .join("One.pdf")
        .to_string_lossy()
        .into_owned();
    assert!(record_conversion(platform, &env, &script, true, 1_792_281_600).unwrap());
    let file = recent_path(platform, &env);
    assert_eq!(
        file_json(&file),
        json!([{ "path": script, "opened": "2026-10-18T00:00:00Z" }])
    );
    // Not a library conversion: nothing written, nothing changed.
    let other = s.join("scripts/Two.pdf");
    assert!(!record_conversion(platform, &env, &other, false, 1_792_281_700).unwrap());
    assert_eq!(paths(&file), [script]);
}
