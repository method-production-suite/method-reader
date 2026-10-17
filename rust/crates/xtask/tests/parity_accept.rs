//! `cargo xtask parity --accept <row>...`: the one way a frozen answer
//! changes. It refuses unless rows are named, shows the diff, and rewrites
//! only the named rows of the committed fixtures' answer files.

mod repo_common;

#[cfg(unix)]
use repo_common::Scratch;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn oracle() -> PathBuf {
    xtask::paths::workspace_root().join("oracle/parity")
}

fn read_map(path: &Path) -> Map<String, Value> {
    match serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap() {
        Value::Object(m) => m,
        _ => panic!("{} is not an object", path.display()),
    }
}

fn xtask(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(args)
        .output()
        .expect("xtask runs")
}

fn said(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn every_committed_answer_file_reads_back_to_its_own_bytes() {
    // So rewriting one row rewrites nothing else.
    let mut n = 0;
    for entry in std::fs::read_dir(oracle()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            xtask::parity::frozen_text(&read_map(&path)) == text,
            "{} does not round-trip",
            path.display()
        );
        n += 1;
    }
    assert_eq!(n, 7, "the seven committed fixtures");
}

#[test]
fn accept_refuses_without_rows() {
    for args in [
        vec!["parity", "--accept"],
        vec!["parity", "--accept", "--committed-only"],
        vec!["parity", "--accept", "*"],
        vec!["parity", "--accept", "help", "--command", "help"],
        vec!["parity", "--accept", "no-such-row"],
    ] {
        let out = xtask(&args);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {}", said(&out));
        let text = said(&out);
        assert!(text.contains("parity"), "{text}");
    }
    let text = said(&xtask(&["parity", "--accept"]));
    assert!(text.contains("name the rows to accept"), "{text}");
}

#[test]
fn the_help_lists_accept() {
    let out = xtask(&["parity", "--help"]);
    assert_eq!(out.status.code(), Some(0));
    let text = said(&out);
    assert!(text.contains("--accept <row>..."), "{text}");
    assert!(text.contains("DELIBERATE"), "{text}");
}

#[cfg(unix)]
#[test]
fn accept_rewrites_only_the_named_row_and_shows_the_diff() {
    let sb = Scratch::new("parity-accept");
    let dir = sb.join("oracle");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("screenplay.json");
    std::fs::copy(oracle().join("screenplay.json"), &file).unwrap();
    let original = read_map(&file);
    let old_help = original["help"]["stdout"].as_str().unwrap().to_owned();
    assert!(old_help.lines().count() > 3);
    // A stand-in CLI whose help is the old help with one line changed.
    let mut lines: Vec<&str> = old_help
        .strip_prefix("<not JSON> ")
        .unwrap()
        .split('\n')
        .collect();
    lines[2] = "  a line changed on purpose";
    let new_help = lines.join("\n");
    let fake = sb.script(
        "bin/screepub",
        &format!(
            "cat <<'HELP'\n{}HELP\n",
            new_help.trim_end_matches('\n').to_owned() + "\n"
        ),
    );
    let run = |rows: &[&str]| {
        let mut args = vec!["parity", "--accept"];
        args.extend_from_slice(rows);
        args.extend_from_slice(&[
            "--fixture",
            "screenplay",
            "--oracle-dir",
            dir.to_str().unwrap(),
            "--rust-cli",
            fake.to_str().unwrap(),
        ]);
        xtask(&args)
    };

    let out = run(&["help"]);
    let text = said(&out);
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(text.contains("NEW   screenplay help"), "{text}");
    assert!(text.contains("+   a line changed on purpose"), "{text}");
    assert!(
        text.contains(&format!("- {}", old_help.split('\n').nth(2).unwrap())),
        "{text}"
    );
    assert!(text.contains("accepted 1 rows in 1 files"), "{text}");

    let after = read_map(&file);
    let mut want = original.clone();
    want.insert(
        "help".into(),
        serde_json::json!({
            "exit": 0,
            "stdout": format!("<not JSON> {}", new_help.trim_end_matches('\n').to_owned() + "\n"),
            "stderr": "",
            "outputs": {},
        }),
    );
    assert_eq!(after, want, "only the help row changed");
    let bytes = std::fs::read_to_string(&file).unwrap();
    assert_eq!(bytes, xtask::parity::frozen_text(&want));

    // Again: nothing differs, nothing is written.
    let out = run(&["help"]);
    let text = said(&out);
    assert!(text.contains("same  screenplay help"), "{text}");
    assert!(text.contains("accepted 0 rows in 0 files"), "{text}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), bytes);
}

#[test]
fn the_line_diff_marks_what_went_and_what_came() {
    let d = xtask::parity::line_diff("a\nb\nc\nd\ne\nf\ng\nh", "a\nb\nc\nD\ne\nf\ng\nh");
    assert_eq!(d, "  ...\n  b\n  c\n- d\n+ D\n  e\n  f\n");
    assert_eq!(xtask::parity::line_diff("same", "same"), "");
    let far = xtask::parity::line_diff("1\nx\n3\n4\n5\n6\n7\n8\ny", "1\nX\n3\n4\n5\n6\n7\n8\nY");
    assert!(far.contains("  ...\n"), "{far}");
}

#[test]
fn a_text_inside_a_json_answer_is_diffed_as_its_own_lines() {
    let d = xtask::parity::value_diff(
        &serde_json::json!({"exit": 0, "stdout": {"ok": true, "usage": "a\nb\nc"}}),
        &serde_json::json!({"exit": 1, "stdout": {"ok": true, "usage": "a\nB\nc"}}),
    );
    assert!(d.contains("exit:\n- 0\n+ 1\n"), "{d}");
    assert!(d.contains("stdout.usage:\n  a\n- b\n+ B\n  c\n"), "{d}");
    assert!(!d.contains("stdout.ok"), "{d}");
}

#[test]
fn an_accepted_version_row_is_written_back_at_the_frozen_version() {
    // The version rows are read as this build's version when compared
    // (with_this_version), so what --accept writes must go back to the
    // frozen one, or the row breaks at the next version bump.
    let this = env!("CARGO_PKG_VERSION");
    let run = serde_json::json!({
        "exit": 0,
        "stdout": format!("method-reader {this} (Method Reader)\n"),
        "stderr": "",
    });
    let frozen = xtask::parity::at_frozen_version("version", &run);
    assert_eq!(
        frozen["stdout"],
        format!(
            "method-reader {} (Method Reader)\n",
            xtask::parity::FROZEN_VERSION
        )
    );
    // Every other row is written exactly as it ran.
    assert_eq!(xtask::parity::at_frozen_version("help", &run), run);
}
