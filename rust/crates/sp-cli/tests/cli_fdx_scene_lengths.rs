//! A scene's length in eighths reaches the .fdx: straight from the
//! conversion with `--fdx`, and later from the library's .fountain through
//! `export --for fdx`, by way of the lengths kept in the script folder's
//! source.json. A .fountain anywhere else, or one edited since, gives an
//! FDX with each scene's page and no length.

mod common;

use serde_json::Value;

use common::Sandbox;

fn ok(sb: &Sandbox, args: &[&str]) -> Value {
    let ran = sb.run(args);
    assert_eq!(ran.code, 0, "{}\n{}", ran.stdout, ran.stderr);
    ran.json()
}

/// The `Length` of every SceneProperties, in order (`None` where absent),
/// and how many there are.
fn lengths(fdx: &str) -> Vec<Option<String>> {
    fdx.split("<SceneProperties")
        .skip(1)
        .map(|rest| {
            let tag = &rest[..rest.find("/>").unwrap()];
            tag.split("Length=\"")
                .nth(1)
                .map(|v| v[..v.find('"').unwrap()].to_owned())
        })
        .collect()
}

fn read(path: &str) -> String {
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn the_conversion_s_fdx_carries_each_scene_s_length() {
    let sb = Sandbox::new();
    let pdf = sb.copy_fixture("screenplay.pdf", "scripts/Draft.pdf");
    sb.mkdir("out");
    let fdx = sb.p("out/Draft.fdx");
    ok(&sb, &[pdf.to_str().unwrap(), "--fdx", &fdx, "--json"]);
    let got = lengths(&read(&fdx));
    assert_eq!(got.len(), 5, "one SceneProperties per scene");
    assert!(got.iter().all(Option::is_some), "{got:?}");
    // Outside the library nothing is kept: a later export of that
    // .fountain names pages only.
    let ran = ok(
        &sb,
        &[
            "export",
            &sb.p("scripts/Draft.epub"),
            "--for",
            "fdx",
            "--fountain",
            &sb.p("scripts/Draft.fountain"),
            "--out",
            &sb.p("out/again.fdx"),
            "--json",
        ],
    );
    assert_eq!(ran["ok"], Value::Bool(true));
    let again = lengths(&read(&sb.p("out/again.fdx")));
    assert_eq!(again, vec![None; 5]);
}

#[test]
fn a_library_script_s_export_reads_the_kept_lengths_until_the_fountain_changes() {
    let sb = Sandbox::new();
    let pdf = sb.copy_fixture("screenplay.pdf", "scripts/Draft.pdf");
    sb.mkdir("out");
    let answer = ok(&sb, &[pdf.to_str().unwrap(), "--library", "--json"]);
    let epub = answer["epubPath"].as_str().unwrap().to_owned();
    let fountain = answer["fountainPath"].as_str().unwrap().to_owned();

    let export = |out: &str| {
        ok(
            &sb,
            &[
                "export",
                &epub,
                "--for",
                "fdx",
                "--fountain",
                &fountain,
                "--out",
                out,
                "--json",
            ],
        );
        lengths(&read(out))
    };

    // Measured at convert time, kept beside the script, read back.
    let direct = sb.p("out/direct.fdx");
    ok(
        &sb,
        &[
            pdf.to_str().unwrap(),
            "--library",
            "--fdx",
            &direct,
            "--json",
        ],
    );
    let kept = export(&sb.p("out/kept.fdx"));
    assert_eq!(kept, lengths(&read(&direct)));
    assert!(kept.iter().all(Option::is_some), "{kept:?}");

    // Edited by hand: the kept lengths no longer describe it.
    let edited = read(&fountain).replacen("\n\n", "\n\nA new line of action.\n\n", 2);
    std::fs::write(&fountain, edited).unwrap();
    assert_eq!(export(&sb.p("out/edited.fdx")), vec![None; 5]);
}
