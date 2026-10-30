//! The helper ships inside the Mac app beside the window, so the version it
//! reports is the app's own: sp-mtp inherits the workspace version, and
//! `cargo xtask bump-version` moves it with every other crate.

use std::path::Path;
use std::process::Command;

fn workspace_version() -> String {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
    let text = std::fs::read_to_string(&manifest).expect("read rust/Cargo.toml");
    let table = text
        .split("[workspace.package]")
        .nth(1)
        .expect("a [workspace.package] table");
    let table = table.split("\n[").next().unwrap_or(table);
    table
        .lines()
        .find_map(|l| {
            let l = l.trim();
            l.strip_prefix("version")
                .map(str::trim_start)
                .and_then(|r| r.strip_prefix('='))
                .map(|r| r.trim().trim_matches('"').to_string())
        })
        .expect("a version in [workspace.package]")
}

#[test]
fn version_answers_one_object_with_the_apps_version() {
    let out = Command::new(env!("CARGO_BIN_EXE_method-reader-mtp"))
        .arg("--version")
        .output()
        .expect("run the helper");
    assert!(out.status.success(), "exit {:?}", out.status);
    let stdout = String::from_utf8(out.stdout).expect("utf-8");
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 1, "one JSON object per run: {stdout}");
    let v: serde_json::Value = serde_json::from_str(lines[0]).expect("JSON");
    assert_eq!(v["ok"], serde_json::Value::Bool(true), "{v}");
    assert_eq!(
        v["version"].as_str(),
        Some(workspace_version().as_str()),
        "{v}"
    );
}
