//! No Rust code runs a JavaScript runtime. The TypeScript engine, Bun and
//! the Tauri window went at the cutover; the one call left is
//! `cargo xtask rehearse-handover prepare`, which builds the old v0.11.1
//! window from its own tag (its engine was TypeScript) so the handover can
//! start from the window people have. Comments may still say what Bun
//! answered once: those are the history of a frozen answer, not a call.

use regex::Regex;
use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let name = entry.file_name();
        if name == "target" || name == "oracle" || name == "oracle-real" {
            continue;
        }
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
}

/// The lines, outside comments, that start or require a JavaScript tool.
fn js_calls(text: &str) -> Vec<String> {
    let call = Regex::new(
        r#"(Command::new|need|which|find_program)\(\s*"(bun|bunx|node|npm|npx|pnpm|yarn|deno)(\.exe)?""#,
    )
    .unwrap();
    let mut out = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let code = line.trim_start();
        if code.starts_with("//") {
            continue;
        }
        // `need(\n "bun",` spans two lines after rustfmt.
        let joined = match lines.get(i + 1) {
            Some(next) if code.ends_with('(') => format!("{code}{}", next.trim()),
            _ => code.to_string(),
        };
        if call.is_match(&joined) {
            out.push(format!("{}: {}", i + 1, line.trim()));
        }
    }
    out
}

#[test]
fn only_the_handover_rehearsal_runs_bun() {
    let ws = xtask::paths::workspace_root();
    let mut files = Vec::new();
    rust_files(&ws.join("crates"), &mut files);
    rust_files(&ws.join("handover-proof"), &mut files);
    assert!(files.len() > 100, "found the workspace's sources");
    let mut found = Vec::new();
    let mut rehearsal = 0;
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap();
        let rel = f
            .strip_prefix(&ws)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let calls = js_calls(&text);
        if rel == "crates/xtask/src/rehearse.rs" {
            rehearsal += calls.len();
            continue;
        }
        if rel == "crates/xtask/tests/no_js_runtime.rs" {
            continue;
        }
        for c in calls {
            found.push(format!("{rel}:{c}"));
        }
    }
    assert_eq!(found, Vec::<String>::new());
    assert!(
        rehearsal >= 2,
        "the rehearsal's own bun calls are still seen"
    );
}

#[test]
fn xtask_has_no_bun_path_helper() {
    let paths =
        std::fs::read_to_string(xtask::paths::workspace_root().join("crates/xtask/src/paths.rs"))
            .unwrap();
    assert!(!paths.contains("fn bun("));
    assert!(!paths.contains("\"BUN\""));
}

#[test]
fn the_scanner_sees_a_call_and_skips_a_comment() {
    assert_eq!(js_calls("    sh(Command::new(\"bun\")\n").len(), 1);
    assert_eq!(js_calls("let x = Command::new(\"node.exe\");").len(), 1);
    assert_eq!(js_calls("    need(\n        \"bun\",\n").len(), 1);
    assert!(js_calls("// what bun answered once: Command::new(\"bun\")").is_empty());
    assert!(js_calls("let s = \"bun\";").is_empty());
}
