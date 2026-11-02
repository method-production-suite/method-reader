//! The `library` verb: what the library folder holds, newest first. The
//! engine's half (what counts as a script, the order, the fields) is
//! sp-engine/tests/library_list.rs; this is the CLI's face of it, reading
//! the library METHOD_READER_LIBRARY names, as a conversion writes it.

mod common;

use serde_json::{Value, json};

use common::{Ran, Sandbox, join, repo_root};

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

/// Field Station's committed `.fountain`, saved in the sandbox under its
/// own name.
fn field_station(sb: &Sandbox) -> String {
    let text = std::fs::read(repo_root().join("rust/oracle/field-station/script.fountain"))
        .expect("the oracle's Field Station");
    sb.write("scripts/Field Station.fountain", text)
        .to_string_lossy()
        .into_owned()
}

#[test]
fn library_help_describes_the_verb_and_exits_0() {
    let sb = Sandbox::new();
    let ran = run(&sb, &["library", "--help"]);
    assert_eq!(ran.code, 0);
    assert!(ran.stdout.starts_with("method-reader library:"));
    assert!(ran.stdout.contains("method-reader library [--json]"));
    let json = run(&sb, &["library", "--help", "--json"]).json();
    assert!(
        json["usage"]
            .as_str()
            .unwrap()
            .starts_with("method-reader library:")
    );
}

#[test]
fn a_conversion_into_the_library_is_listed_with_its_book() {
    let mut sb = Sandbox::new();
    let root = sb.p("lib");
    sb.set("METHOD_READER_LIBRARY", root.clone());
    let input = field_station(&sb);
    ok(&sb, &[&input, "--library", "--json"]);

    let v = ok(&sb, &["library", "--json"]);
    let folder = join(&root, "Field Station");
    let stem = join(&folder, "Field Station");
    let script = &v["scripts"][0];
    assert_eq!(v["ok"], json!(true));
    assert_eq!(v["root"], json!(root));
    assert_eq!(v["scripts"].as_array().unwrap().len(), 1);
    assert_eq!(script["folder"], json!(folder));
    assert_eq!(script["name"], json!("Field Station"));
    assert_eq!(script["title"], json!("Field Station"));
    assert_eq!(script["source"], json!(input));
    assert_eq!(script["sourceExists"], json!(true));
    assert_eq!(script["fountain"], json!(format!("{stem}.fountain")));
    assert_eq!(script["epub"], json!(format!("{stem}.epub")));
    assert_eq!(script["pages"], json!(18));
    assert_eq!(script["scenes"], json!(11));
    assert!(script["converted"].as_str().unwrap().ends_with('Z'));
    assert!(script["others"].is_array());
    // The first library conversion pins the settings it started from.
    assert_eq!(
        script["settings"],
        json!(format!("{stem}.method-reader.json"))
    );

    // Its PDF (here, its .fountain) gone: still listed, and flagged.
    std::fs::remove_file(&input).unwrap();
    let v = ok(&sb, &["library", "--json"]);
    assert_eq!(v["scripts"][0]["sourceExists"], json!(false));

    let human = run(&sb, &["library"]);
    assert_eq!(human.code, 0);
    assert_eq!(human.stdout, "Field Station: 18 pages, 11 scenes\n");
}

#[test]
fn an_empty_library_says_so_and_where_it_is_and_is_not_made() {
    let mut sb = Sandbox::new();
    let root = sb.p("lib/not-yet");
    sb.set("METHOD_READER_LIBRARY", root.clone());
    assert_eq!(
        ok(&sb, &["library", "--json"]),
        json!({ "ok": true, "root": root, "scripts": [] })
    );
    let human = run(&sb, &["library"]);
    assert_eq!(human.code, 0);
    assert_eq!(
        human.stdout,
        format!("no scripts in the library yet ({root})\n")
    );
    assert!(!std::path::Path::new(&root).exists());
}

#[test]
fn the_verb_takes_no_arguments_and_no_other_verbs_flags() {
    let sb = Sandbox::new();
    let ran = run(&sb, &["library", "extra", "--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "usage");
    assert!(ran.error_message().contains("library takes no arguments"));
    let ran = run(&sb, &["library", "--quick", "--json"]);
    assert_eq!(ran.error_code(), "usage");
    assert!(ran.error_message().contains("library takes no --quick"));
}
