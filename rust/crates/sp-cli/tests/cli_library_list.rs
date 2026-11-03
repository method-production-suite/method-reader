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
fn a_pdf_conversion_and_its_listing_agree_on_the_page_count() {
    let mut sb = Sandbox::new();
    let root = sb.p("lib");
    sb.set("METHOD_READER_LIBRARY", root.clone());
    let pdf = sb
        .copy_fixture("field-station.pdf", "scripts/Field Station.pdf")
        .to_string_lossy()
        .into_owned();
    let converted = ok(&sb, &[&pdf, "--library", "--json"]);
    assert_eq!(converted["pages"], json!(19), "the title page counts");
    let v = ok(&sb, &["library", "--json"]);
    assert_eq!(v["scripts"][0]["pages"], converted["pages"]);
    assert_eq!(v["scripts"][0]["scenes"], converted["scenes"]);
    // Kept in source.json, bound to the .fountain it was counted with.
    let marker: Value = serde_json::from_str(
        &std::fs::read_to_string(join(&root, "Field Station/source.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(marker["source"], json!(pdf));
    assert_eq!(marker["pages"]["count"], json!(19));
    assert_eq!(
        marker["pages"]["fountainSha256"].as_str().unwrap().len(),
        64
    );
    // The same script converted again from its .fountain has no PDF page
    // count: the kept one is cleared and its markers count instead.
    let fountain = v["scripts"][0]["fountain"].as_str().unwrap().to_owned();
    let copy = sb.write(
        "again/Field Station.fountain",
        std::fs::read(&fountain).unwrap(),
    );
    ok(&sb, &[&copy.to_string_lossy(), "--library", "--json"]);
    let v = ok(&sb, &["library", "--json"]);
    let again = v["scripts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["source"] == json!(copy.to_string_lossy()))
        .expect("the .fountain's own folder");
    assert_eq!(again["pages"], json!(18), "its highest marker");
}

#[test]
fn a_library_conversion_keeps_when_it_converted_and_the_listing_shows_it() {
    use sp_engine::recent::{now_seconds, rfc3339};
    let mut sb = Sandbox::new();
    let root = sb.p("lib");
    sb.set("METHOD_READER_LIBRARY", root.clone());
    let pdf = sb
        .copy_fixture("field-station.pdf", "scripts/Field Station.pdf")
        .to_string_lossy()
        .into_owned();
    let marker = || -> Value {
        serde_json::from_str(
            &std::fs::read_to_string(join(&root, "Field Station/source.json")).unwrap(),
        )
        .unwrap()
    };
    let before = rfc3339(now_seconds());
    ok(&sb, &[&pdf, "--library", "--json"]);
    let after = rfc3339(now_seconds());
    // One shape, so text order is time order.
    let kept = marker()["converted"].as_str().unwrap().to_owned();
    assert!(
        before <= kept && kept <= after,
        "{before} <= {kept} <= {after}"
    );
    assert_eq!(kept.len(), 20, "{kept}");
    assert_eq!(marker()["source"], json!(pdf), "the source is kept");
    assert_eq!(marker()["pages"]["count"], json!(19), "the count is kept");
    let listed = |sb: &Sandbox| ok(sb, &["library", "--json"])["scripts"][0]["converted"].clone();
    assert_eq!(listed(&sb), json!(kept));

    // Edited outside the app, a day on: the listed date stays the
    // conversion's.
    let fountain = join(&root, "Field Station/Field Station.fountain");
    let mut text = std::fs::read_to_string(&fountain).unwrap();
    text.push_str("\nThe end, revised.\n");
    std::fs::write(&fountain, text).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&fountain)
        .unwrap()
        .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(86_400))
        .unwrap();
    assert_eq!(listed(&sb), json!(kept));

    // A conversion from a .fountain records its time as well.
    let input = field_station(&sb);
    let before = rfc3339(now_seconds());
    ok(&sb, &[&input, "--library", "--json"]);
    let after = rfc3339(now_seconds());
    let v = ok(&sb, &["library", "--json"]);
    let folder = v["scripts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["source"] == json!(input))
        .expect("the .fountain's own folder")["folder"]
        .as_str()
        .unwrap();
    let again: Value =
        serde_json::from_str(&std::fs::read_to_string(join(folder, "source.json")).unwrap())
            .unwrap();
    let kept = again["converted"].as_str().unwrap();
    assert!(before.as_str() <= kept && kept <= after.as_str(), "{kept}");
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
