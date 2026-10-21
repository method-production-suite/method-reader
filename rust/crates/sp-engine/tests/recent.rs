//! recent.json: the scripts a person opened, for the suite's launcher
//! (SUITE-STANDARDS.md §2). Every test hands the engine a scratch data
//! folder through METHOD_READER_CONFIG_DIR, never the real one.

mod common;

use serde_json::{Value, json};
use sp_engine::env::Env;
use sp_engine::recent::{
    RECENT_FILE, RECENT_LIMIT, counts_as_opened, damaged_path, read_recent, recent_path,
    record_conversion, record_opened, rfc3339, same_path, with_opened,
};

use common::{Scratch, guarded_env};

/// The engine's environment with the data folder in `scratch/config`.
fn env_in(scratch: &Scratch) -> Env {
    let mut env = guarded_env();
    env.set("METHOD_READER_CONFIG_DIR", scratch.join("config"));
    env
}

/// This build's platform, for the tests whose answer does not depend on it.
fn host() -> &'static str {
    sp_engine::env::host_platform()
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
    record_opened(host(), &file, "/scripts/One.pdf", "2026-10-18T09:00:00Z").unwrap();
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
    record_opened(host(), &file, "/s/A.pdf", "2026-10-18T09:00:00Z").unwrap();
    record_opened(host(), &file, "/s/B.pdf", "2026-10-18T09:01:00Z").unwrap();
    record_opened(host(), &file, "/s/C.pdf", "2026-10-18T09:02:00Z").unwrap();
    assert_eq!(paths(&file), ["/s/C.pdf", "/s/B.pdf", "/s/A.pdf"]);

    record_opened(host(), &file, "/s/A.pdf", "2026-10-18T09:03:00Z").unwrap();
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
        record_opened(host(), &file, &format!("/s/{i}.pdf"), &rfc3339(i)).unwrap();
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
    record_opened(host(), &file, "/nowhere/Gone.pdf", "2026-10-18T09:00:00Z").unwrap();
    record_opened(
        host(),
        &file,
        "/nowhere/Also gone.pdf",
        "2026-10-18T09:01:00Z",
    )
    .unwrap();
    assert_eq!(
        paths(&file),
        ["/nowhere/Also gone.pdf", "/nowhere/Gone.pdf"]
    );
}

#[test]
fn a_missing_file_reads_as_empty_and_sets_nothing_aside() {
    let s = Scratch::new("recent");
    let file = s.join("recent.json");
    assert!(read_recent(&file).is_empty());
    let recorded = record_opened(host(), &file, "/s/New.pdf", "2026-10-18T09:00:00Z").unwrap();
    assert_eq!(recorded.damaged, None);
    assert!(!std::path::Path::new(&damaged_path(&file)).exists());
}

#[test]
fn a_damaged_file_is_kept_aside_before_a_fresh_list_starts() {
    let s = Scratch::new("recent");
    let file = s.join("recent.json");
    let aside = damaged_path(&file);
    assert_eq!(aside, format!("{file}.damaged"));
    for bad in ["not json", "{\"path\": \"/x\"}", "[{\"path\": \"/s/Cut", ""] {
        std::fs::write(&file, bad).unwrap();
        assert!(read_recent(&file).is_empty(), "{bad:?}");
        let recorded = record_opened(host(), &file, "/s/New.pdf", "2026-10-18T09:00:00Z").unwrap();
        // Said once, by the call that set it aside.
        assert_eq!(recorded.damaged.as_deref(), Some(aside.as_str()), "{bad:?}");
        // One copy, the newest damaged file, byte for byte.
        assert_eq!(std::fs::read_to_string(&aside).unwrap(), bad);
        assert_eq!(
            file_json(&file),
            json!([{ "path": "/s/New.pdf", "opened": "2026-10-18T09:00:00Z" }])
        );
        // The fresh file is whole, so the next record says nothing.
        let next = record_opened(host(), &file, "/s/New.pdf", "2026-10-18T09:01:00Z").unwrap();
        assert_eq!(next.damaged, None);
        assert_eq!(std::fs::read_to_string(&aside).unwrap(), bad);
    }
}

#[test]
fn an_array_with_nothing_usable_is_not_damaged() {
    // Valid JSON in the standard's shape: its unusable entries are dropped
    // (as below), and nothing is set aside.
    let s = Scratch::new("recent");
    let file = s.join("recent.json");
    std::fs::write(&file, "[1, \"two\", null]").unwrap();
    let recorded = record_opened(host(), &file, "/s/New.pdf", "2026-10-18T09:00:00Z").unwrap();
    assert_eq!(recorded.damaged, None);
    assert!(!std::path::Path::new(&damaged_path(&file)).exists());
    assert_eq!(paths(&file), ["/s/New.pdf"]);
}

#[test]
fn a_file_it_cannot_read_is_left_alone() {
    // A folder where the file should be: it cannot be read, so it is not
    // called damaged, not moved, and not written over.
    let s = Scratch::new("recent");
    let file = s.join("recent.json");
    let inside = s.dir("recent.json/inside");
    assert!(record_opened(host(), &file, "/s/New.pdf", "2026-10-18T09:00:00Z").is_err());
    assert!(std::path::Path::new(&inside).is_dir());
    assert!(!std::path::Path::new(&damaged_path(&file)).exists());
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
    record_opened(host(), &file, "/s/New.pdf", "2026-10-18T09:00:00Z").unwrap();
    assert_eq!(
        file_json(&file),
        json!([
            { "path": "/s/New.pdf", "opened": "2026-10-18T09:00:00Z" },
            { "path": "/s/Old.pdf", "opened": "2026-10-01T00:00:00Z", "bookmark": "abc" }
        ])
    );
}

#[test]
fn the_write_is_atomic_and_leaves_only_the_lock_file_beside_it() {
    let s = Scratch::new("recent");
    let dir = s.dir("data");
    let file = format!("{dir}/recent.json");
    record_opened(host(), &file, "/s/A.pdf", "2026-10-18T09:00:00Z").unwrap();
    record_opened(host(), &file, "/s/B.pdf", "2026-10-18T09:01:00Z").unwrap();
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    // The lock file stays: removing it would let a third writer lock a new
    // file while a second still held the old one.
    assert_eq!(names, ["recent.json", "recent.json.lock"]);
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
    record_opened(host(), &file, "/s/A.pdf", "2026-10-18T09:00:00Z").unwrap();
    let before = std::fs::read(&file).unwrap();
    // A folder where the temp file would go cannot be written through.
    let dir = s.path.clone();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    let failed = record_opened(host(), &file, "/s/B.pdf", "2026-10-18T09:01:00Z");
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
    assert!(
        record_conversion(platform, &env, &script, true, 1_792_281_600)
            .unwrap()
            .is_some()
    );
    let file = recent_path(platform, &env);
    assert_eq!(
        file_json(&file),
        json!([{ "path": script, "opened": "2026-10-18T00:00:00Z" }])
    );
    // Not a library conversion: nothing written, nothing changed.
    let other = s.join("scripts/Two.pdf");
    assert!(
        record_conversion(platform, &env, &other, false, 1_792_281_700)
            .unwrap()
            .is_none()
    );
    assert_eq!(paths(&file), [script]);
}

#[test]
fn windows_compares_paths_without_regard_to_case() {
    assert!(same_path(
        "win32",
        r"C:\Scripts\One.pdf",
        r"c:\scripts\ONE.PDF"
    ));
    assert!(!same_path(
        "win32",
        r"C:\Scripts\One.pdf",
        r"C:\Scripts\Two.pdf"
    ));
    // Elsewhere case is part of the name, as the data-folder test already
    // treats it: a Mac's default volume ignores case, but a case-sensitive
    // one is allowed, and two spellings there are two files.
    for platform in ["darwin", "linux"] {
        assert!(same_path(platform, "/s/One.pdf", "/s/One.pdf"));
        assert!(
            !same_path(platform, "/s/One.pdf", "/s/one.pdf"),
            "{platform}"
        );
    }
}

#[test]
fn on_windows_another_spelling_of_the_same_file_is_one_entry() {
    let entry = |path: &str, opened: &str| {
        json!({ "path": path, "opened": opened })
            .as_object()
            .unwrap()
            .clone()
    };
    let old = vec![
        entry(r"C:\Scripts\ONE.pdf", "2026-10-18T09:00:00Z"),
        entry(r"C:\Scripts\Two.pdf", "2026-10-18T08:00:00Z"),
    ];
    let got = with_opened(
        "win32",
        old.clone(),
        r"c:\scripts\one.PDF",
        "2026-10-18T10:00:00Z",
    );
    let got: Vec<&str> = got.iter().map(|e| e["path"].as_str().unwrap()).collect();
    // The newest spelling is the one kept, on top.
    assert_eq!(got, [r"c:\scripts\one.PDF", r"C:\Scripts\Two.pdf"]);

    let got = with_opened("darwin", old, r"c:\scripts\one.PDF", "2026-10-18T10:00:00Z");
    assert_eq!(got.len(), 3);
}

/// Each copy of this binary in the race below records this many entries.
const RACE_EACH: usize = 5;
/// How many copies race.
const RACE_PROCESSES: usize = 4;

/// One racer of the test below, run in its own process: once the start
/// file appears, it records [`RACE_EACH`] entries as fast as it can. In
/// the ordinary run the variables are unset and it does nothing.
#[test]
fn race_child() {
    let (Ok(file), Ok(tag)) = (
        std::env::var("METHOD_READER_RECENT_RACE_FILE"),
        std::env::var("METHOD_READER_RECENT_RACE_TAG"),
    ) else {
        return;
    };
    let go = format!("{file}.go");
    let waited = std::time::Instant::now();
    while !std::path::Path::new(&go).exists() {
        assert!(waited.elapsed().as_secs() < 60, "no start file");
        std::thread::yield_now();
    }
    for i in 0..RACE_EACH {
        let source = format!("/race/{tag}-{i}.pdf");
        record_opened(host(), &file, &source, &rfc3339(0)).unwrap();
    }
}

#[test]
fn processes_writing_at_the_same_moment_lose_no_entry() {
    // The window and the command line are two processes: this test runs
    // its own binary several times over, each copy running race_child on
    // the same scratch file.
    assert_eq!(RACE_EACH * RACE_PROCESSES, RECENT_LIMIT);
    let s = Scratch::new("recent");
    let file = s.join("recent.json");
    let exe = std::env::current_exe().unwrap();
    let children: Vec<_> = (0..RACE_PROCESSES)
        .map(|n| {
            std::process::Command::new(&exe)
                .args(["--exact", "race_child", "--test-threads=1", "--quiet"])
                .env("METHOD_READER_RECENT_RACE_FILE", &file)
                .env("METHOD_READER_RECENT_RACE_TAG", n.to_string())
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap()
        })
        .collect();
    // Let every copy start and wait, then let them all go at once.
    std::thread::sleep(std::time::Duration::from_millis(500));
    std::fs::write(format!("{file}.go"), "").unwrap();
    for mut child in children {
        assert!(child.wait().unwrap().success());
    }
    let mut got = paths(&file);
    got.sort();
    let mut want: Vec<String> = (0..RACE_PROCESSES)
        .flat_map(|n| (0..RACE_EACH).map(move |i| format!("/race/{n}-{i}.pdf")))
        .collect();
    want.sort();
    assert_eq!(got, want);
}
