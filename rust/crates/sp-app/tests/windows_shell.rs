//! The Windows window is a GUI program, and nothing it starts opens a
//! console. Both are compile-time facts of code that has never run on
//! Windows here, so they are checked in the source.
//!
//! Without `windows_subsystem = "windows"` the release window opens a
//! console behind itself (closing it kills the app), and without
//! CREATE_NO_WINDOW every PowerShell eject and every minutes-long
//! ebook-convert flashes or holds a console over the window.

use std::path::Path;

fn crate_src(krate: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(krate)
        .join("src")
}

fn rust_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn the_release_window_is_a_windows_gui_program() {
    let main = std::fs::read_to_string(crate_src("sp-app").join("main.rs")).unwrap();
    assert!(
        main.contains(
            r#"#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]"#
        ),
        "main.rs declares the GUI subsystem for release builds"
    );
}

#[test]
fn every_child_the_engine_starts_goes_through_quiet_command() {
    for krate in ["sp-device", "sp-engine", "sp-cli", "sp-app"] {
        let mut files = Vec::new();
        rust_files(&crate_src(krate), &mut files);
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap();
            for (n, line) in text.lines().enumerate() {
                let code = line.split("//").next().unwrap_or("");
                if code.contains("Command::new(") && !code.contains("fn quiet_command") {
                    let ok = file.ends_with("sp-device/src/process.rs")
                        && code.trim() == "let mut command = Command::new(program);";
                    assert!(ok, "{}:{} starts a child directly", file.display(), n + 1);
                }
            }
        }
    }
}
