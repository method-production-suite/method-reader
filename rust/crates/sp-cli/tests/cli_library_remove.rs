//! The `library-remove` verb: one script's folder out of the library, to
//! the Trash. The engine's half (every refusal, the numbering in the
//! Trash, the guard) is sp-engine/tests/library_remove.rs; this is the
//! CLI's face of it, with the sandbox's `trash/` standing for the Trash
//! (METHOD_READER_TRASH), so nothing here reaches the system Trash.

mod common;

use serde_json::{Value, json};

use common::{Ran, Sandbox, join, repo_root};
use sp_engine::commands::library_remove::{bin_name, moved_line};

/// Run in the sandbox's `empty/` folder: its own folder holds `library/`,
/// and a word is only a verb when no file of that name is there.
fn run(sb: &Sandbox, args: &[&str]) -> Ran {
    sb.run_in(&sb.path("empty"), args)
}

fn ok(sb: &Sandbox, args: &[&str]) -> Value {
    let ran = run(sb, args);
    assert_eq!(ran.code, 0, "{}\n{}", ran.stdout, ran.stderr);
    ran.json()
}

fn names(dir: &str) -> Vec<String> {
    let mut found: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    found
}

/// Field Station converted into the sandbox library from its committed
/// `.fountain`; answers the script's folder.
fn converted(sb: &Sandbox) -> String {
    let text = std::fs::read(repo_root().join("rust/oracle/field-station/script.fountain"))
        .expect("the oracle's Field Station");
    let input = sb
        .write("scripts/Field Station.fountain", text)
        .to_string_lossy()
        .into_owned();
    ok(sb, &[&input, "--library", "--json"]);
    join(&sb.p("library"), "Field Station")
}

#[test]
fn library_remove_help_describes_the_verb_and_exits_0() {
    let sb = Sandbox::new();
    let ran = run(&sb, &["library-remove", "--help"]);
    assert_eq!(ran.code, 0);
    assert!(ran.stdout.starts_with("method-reader library-remove:"));
    assert!(
        ran.stdout
            .contains("method-reader library-remove <folder> [--json]")
    );
    assert!(
        ran.stdout
            .contains("a library on a drive with no Recycle Bin"),
        "{}",
        ran.stdout
    );
    let json = run(&sb, &["library-remove", "-h", "--json"]).json();
    assert!(
        json["usage"]
            .as_str()
            .unwrap()
            .starts_with("method-reader library-remove:")
    );
}

#[test]
fn a_listed_script_goes_to_the_trash_and_off_the_listing() {
    let sb = Sandbox::new();
    let folder = converted(&sb);
    let listed = ok(&sb, &["library", "--json"]);
    assert_eq!(listed["scripts"][0]["folder"], json!(folder));

    let v = ok(&sb, &["library-remove", &folder, "--json"]);
    assert_eq!(
        v,
        json!({"ok": true, "removed": folder, "name": "Field Station"})
    );
    assert!(!std::path::Path::new(&folder).exists());
    let trash = sb.p("trash");
    assert_eq!(names(&trash), ["Field Station"]);
    let moved = join(&trash, "Field Station");
    assert!(names(&moved).contains(&"Field Station.fountain".to_owned()));
    assert!(names(&moved).contains(&"Field Station.epub".to_owned()));
    assert_eq!(ok(&sb, &["library", "--json"])["scripts"], json!([]));
    // Its source stays where it was.
    assert!(sb.path("scripts/Field Station.fountain").exists());

    // Converted again and removed in words: the second copy is numbered.
    let again = converted(&sb);
    let human = run(&sb, &["library-remove", &again]);
    assert_eq!(human.code, 0, "{}", human.stderr);
    assert_eq!(
        human.stdout,
        format!("{}\n", moved_line(&again, cfg!(windows)))
    );
    assert_eq!(names(&trash), ["Field Station", "Field Station 2"]);
}

#[test]
fn the_plain_words_line_names_each_platforms_bin() {
    assert_eq!(
        moved_line(r"C:\Users\ann\Method Reader\Field Station", true),
        r"moved C:\Users\ann\Method Reader\Field Station to the Recycle Bin"
    );
    assert_eq!(
        moved_line("/Users/ann/Method Reader/Field Station", false),
        "moved /Users/ann/Method Reader/Field Station to the Trash"
    );
}

#[test]
fn a_folder_the_listing_would_not_list_is_refused_and_nothing_moves() {
    let sb = Sandbox::new();
    let folder = converted(&sb);
    let stray = sb.mkdir("library/Loose").to_string_lossy().into_owned();
    for (target, code) in [
        (stray.clone(), "not-a-script"),
        (sb.p("library"), "not-a-script"),
        (sb.p("scripts"), "not-a-script"),
        ("Field Station".to_owned(), "usage"),
    ] {
        let ran = run(&sb, &["library-remove", &target, "--json"]);
        assert_eq!(ran.code, 1, "{target}");
        assert_eq!(ran.error_code(), code, "{target}");
    }
    let ran = run(&sb, &["library-remove", &stray, "--json"]);
    assert_eq!(
        ran.error_message(),
        format!(
            "not a script in the library (it has no source.json naming its source, or no .fountain of that name): {stray}"
        )
    );
    assert!(names(&sb.p("trash")).is_empty());
    assert!(std::path::Path::new(&folder).exists());
}

#[test]
fn it_takes_exactly_one_folder_and_no_foreign_flag() {
    let sb = Sandbox::new();
    for args in [
        &["library-remove", "--json"][..],
        &["library-remove", "/a", "/b", "--json"],
    ] {
        let ran = run(&sb, args);
        assert_eq!(ran.error_code(), "usage", "{args:?}");
        assert_eq!(
            ran.error_message(),
            "expected exactly one script folder in the library (see --help)"
        );
    }
    let ran = run(&sb, &["library-remove", "/a", "--quick", "--json"]);
    assert_eq!(ran.error_code(), "usage");
}

#[test]
fn a_trash_that_cannot_be_used_moves_nothing() {
    let mut sb = Sandbox::new();
    let folder = converted(&sb);
    sb.set(
        "METHOD_READER_TRASH",
        "/dev/null/method-reader-test-trash-guard",
    );
    let ran = run(&sb, &["library-remove", &folder, "--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "trash-failed");
    assert!(
        ran.error_message().starts_with(&format!(
            "could not move Field Station to the {}: ",
            bin_name(cfg!(windows))
        )),
        "{}",
        ran.error_message()
    );
    assert!(std::path::Path::new(&folder).exists());
}
