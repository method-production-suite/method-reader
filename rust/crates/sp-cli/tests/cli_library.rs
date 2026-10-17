//! Port of tests/library.test.ts's `the CLI with --library` block: what a
//! library conversion writes, and where. (The rest of that file, where the
//! library is and which folder a script owns, is sp-engine's
//! tests/library.rs.)

#![cfg_attr(not(unix), allow(dead_code, unused_imports))]

mod common;

use std::path::Path;

use serde_json::{Value, json};

use common::{Sandbox, repo_root};

fn names(dir: &Path) -> Vec<String> {
    let mut n: Vec<String> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    n.sort();
    n
}

fn sorted_keys(v: &Value) -> Vec<String> {
    let mut k: Vec<String> = v.as_object().expect("an object").keys().cloned().collect();
    k.sort();
    k
}

/// screenplay.pdf copied to `scripts/<name>`.
fn script(sb: &Sandbox, name: &str) -> String {
    sb.copy_fixture("screenplay.pdf", &format!("scripts/{name}"))
        .to_string_lossy()
        .into_owned()
}

fn ok(sb: &Sandbox, args: &[&str]) -> Value {
    let ran = sb.run(args);
    assert_eq!(ran.code, 0, "{}\n{}", ran.stdout, ran.stderr);
    ran.json()
}

#[test]
fn nothing_lands_beside_the_pdf_and_everything_lands_in_the_library() {
    let mut sb = Sandbox::new();
    let pdf = script(&sb, "Bright Angel.pdf");
    let root = sb.p("lib/not-yet-made");
    sb.set("METHOD_READER_LIBRARY", root.clone());
    let scripts = sb.path("scripts");
    let before = names(&scripts);

    let answer = ok(&sb, &[&pdf, "--library", "--json"]);
    assert_eq!(answer["ok"], json!(true));

    // The user's folder is exactly as they left it.
    assert_eq!(names(&scripts), before);
    assert_eq!(before, ["Bright Angel.pdf"]);

    let folder = common::join(&root, "Bright Angel");
    assert_eq!(
        answer["epubPath"],
        json!(common::join(&folder, "Bright Angel.epub"))
    );
    assert_eq!(
        answer["fountainPath"],
        json!(common::join(&folder, "Bright Angel.fountain"))
    );
    assert!(Path::new(answer["epubPath"].as_str().unwrap()).exists());
    assert!(Path::new(answer["fountainPath"].as_str().unwrap()).exists());
    // A library root that did not exist is made on demand; the
    // .method-reader.json is the pin.
    assert_eq!(
        names(Path::new(&folder)),
        [
            "Bright Angel.epub",
            "Bright Angel.fountain",
            "Bright Angel.method-reader.json",
            "source.json",
        ]
    );
}

#[test]
fn without_library_the_cli_still_writes_beside_its_input() {
    let sb = Sandbox::new();
    let pdf = script(&sb, "Beside.pdf");
    ok(&sb, &[&pdf, "--json"]);
    assert_eq!(
        names(&sb.path("scripts")),
        ["Beside.epub", "Beside.fountain", "Beside.pdf"]
    );
    assert!(names(&sb.path("library")).is_empty());
}

#[test]
fn a_sidecar_beside_the_pdf_survives_the_move_into_the_library() {
    let sb = Sandbox::new();
    let pdf = script(&sb, "Tuned.pdf");
    // 41 is not the default, so finding it proves the file travelled.
    sb.write(
        "scripts/Tuned.method-reader.json",
        "{\"cueIndentPct\":41}\n",
    );
    let defaults: Value = serde_json::from_str(
        &std::fs::read_to_string(repo_root().join("format-defaults.json")).unwrap(),
    )
    .unwrap();
    assert_ne!(defaults["cueIndentPct"], json!(41));

    let answer = ok(&sb, &[&pdf, "--library", "--json"]);
    // Read back the way the window does: `method-reader settings` on the library
    // .fountain the answer handed over.
    let said = ok(
        &sb,
        &[
            "settings",
            answer["fountainPath"].as_str().unwrap(),
            "--json",
        ],
    );
    assert_eq!(said["settings"]["cueIndentPct"], json!(41));
    assert_eq!(
        said["sidecar"],
        json!(sb.p("library/Tuned/Tuned.method-reader.json"))
    );
}

#[test]
fn a_refused_input_leaves_no_trace_in_the_library() {
    let sb = Sandbox::new();
    let readme = sb.write("scripts/readme.md", "# not a screenplay\n");
    let readme = readme.to_string_lossy();
    let prose = sb.copy_fixture("prose.pdf", "elsewhere/prose.pdf");

    let wrong_type = sb.run(&[&readme, "--library", "--json"]);
    assert_eq!(wrong_type.error_code(), "unsupported-type");
    let missing = sb.run(&[&sb.p("scripts/ghost.pdf"), "--library", "--json"]);
    assert_eq!(missing.error_code(), "unreadable");
    let not_screenplay = sb.run(&[&prose.to_string_lossy(), "--library", "--json"]);
    assert_eq!(not_screenplay.error_code(), "not-screenplay");

    // NOTHING: the root itself is untouched.
    assert!(names(&sb.path("library")).is_empty());
    assert_eq!(names(&sb.path("scripts")), ["readme.md"]);

    // And the name is still free, so the real script of that name gets it.
    let real = sb.copy_fixture("screenplay.pdf", "real/readme.pdf");
    let answer = ok(&sb, &[&real.to_string_lossy(), "--library", "--json"]);
    assert_eq!(
        answer["epubPath"],
        json!(sb.p("library/readme/readme.epub"))
    );
}

#[test]
fn library_and_o_together_is_a_refusal_not_a_silent_winner() {
    let sb = Sandbox::new();
    let pdf = script(&sb, "Both.pdf");
    sb.mkdir("out");
    let out = sb.p("out/both.epub");
    let ran = sb.run(&[&pdf, "--library", "-o", &out, "--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "usage");
    // Neither destination was written: the refusal comes before any work.
    assert!(!Path::new(&out).exists());
}

/// Root bypasses ordinary permission bits.
#[cfg(unix)]
fn is_root() -> bool {
    std::process::Command::new("id")
        .arg("-u")
        .output()
        .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
}

#[cfg(unix)]
#[test]
fn a_library_it_cannot_make_is_one_json_object_not_a_stack_trace() {
    use std::os::unix::fs::PermissionsExt;
    if is_root() {
        return;
    }
    let mut sb = Sandbox::new();
    let locked = sb.mkdir("locked");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o500)).unwrap();
    let pdf = script(&sb, "NoRoom.pdf");
    sb.set("METHOD_READER_LIBRARY", sb.p("locked/screepub"));

    let ran = sb.run(&[&pdf, "--library", "--json"]);
    // Unlocked again before anything can fail, so the scratch folder goes.
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(ran.code, 1);
    // Exactly one object on stdout, with the code the window has a heading
    // for.
    let answer = ran.json();
    assert_eq!(sorted_keys(&answer), ["error", "ok"]);
    assert_eq!(answer["ok"], json!(false));
    assert_eq!(sorted_keys(&answer["error"]), ["code", "message"]);
    assert_eq!(answer["error"]["code"], json!("library"));
    assert!(ran.error_message().contains("library folder"));
    assert_eq!(ran.stderr, "");
    // It failed before converting: nothing was written anywhere.
    assert_eq!(names(&sb.path("scripts")), ["NoRoom.pdf"]);
}
