//! The `library-rename` verb's engine half: a name kept for one script in
//! its folder's source.json, nothing on disk renamed
//! (docs/superpowers/specs/2026-11-03-library-rename-design.md). The
//! book's rebuild and a PDF reconversion are the command line's, in
//! sp-cli/tests/cli_library_rename.rs. Every script here is invented.

mod common;

use common::{Scratch, names, read_json, write};
use serde_json::json;
use sp_engine::commands::library::list_library;
use sp_engine::commands::library_rename::{
    LibraryRenamed, kept_name_of, rename_in_library, renamed_line,
};
use sp_engine::convert::{ConvertOptions, convert_fountain};
use sp_engine::errors::EngineError;
use sp_engine::export::artifact::{
    FreshKindleArtifactOptions, RungBuilders, fresh_artifact_of, mobi_sibling,
};
use sp_engine::export::formats::KindleRung;
use sp_engine::export::renderers::Books;
use sp_engine::library::{
    MAX_NAME_CHARS, NAME_KEY, library_output, read_kept_name, save_kept_name,
};
use sp_engine::nodepath::host;
use sp_model::DEFAULT_FORMAT_OPTIONS;

const WREN: &str = "\
Title: The Lamp Keeper

INT. LIGHTHOUSE - NIGHT

WREN
Morning already.
";

/// A script folder the way a library conversion leaves it; answers the
/// folder.
fn script(root: &str, source: &str) -> String {
    let prefix = library_output(source, root).expect("a library folder");
    write(&format!("{prefix}.fountain"), WREN);
    write(&format!("{prefix}.epub"), "PK stand-in");
    host::dirname(&prefix)
}

fn marker(folder: &str) -> serde_json::Value {
    read_json(&host::join(&[folder, "source.json"]))
}

fn refusal(err: EngineError) -> (String, String) {
    match err {
        EngineError::Cli(e) => (e.code.as_str().to_owned(), e.message),
        other => panic!("not a refusal: {other:?}"),
    }
}

fn title_listed(root: &str) -> Option<String> {
    list_library(root).scripts[0].title.clone()
}

#[test]
fn a_name_is_kept_beside_the_source_and_the_listing_calls_the_script_by_it() {
    let s = Scratch::new("library-rename");
    let root = s.dir("lib");
    let folder = script(&root, "/scripts/lampkeeper_v3_FINAL.pdf");
    assert_eq!(title_listed(&root).as_deref(), Some("The Lamp Keeper"));

    let renamed = rename_in_library(&root, &folder, "  Harbour Light  ").unwrap();
    assert_eq!(
        serde_json::to_value(&renamed).unwrap(),
        json!({
            "folder": folder,
            "title": "Harbour Light",
            "epub": host::join(&[&folder, "lampkeeper_v3_FINAL.epub"]),
        })
    );
    assert_eq!(
        renamed_line(&renamed),
        format!("renamed {folder} to \"Harbour Light\"")
    );
    // Only the name was written; the source is kept, and nothing on disk
    // was renamed.
    assert_eq!(
        marker(&folder),
        json!({"name": "Harbour Light", "source": "/scripts/lampkeeper_v3_FINAL.pdf"})
    );
    assert_eq!(
        names(&folder),
        [
            "lampkeeper_v3_FINAL.epub",
            "lampkeeper_v3_FINAL.fountain",
            "source.json"
        ]
    );
    assert_eq!(
        common::read(&renamed.fountain),
        WREN,
        "the .fountain is not edited"
    );
    let listed = &list_library(&root).scripts[0];
    assert_eq!(listed.title.as_deref(), Some("Harbour Light"));
    assert_eq!(listed.name, "lampkeeper_v3_FINAL");
    assert_eq!(
        read_kept_name(&renamed.fountain).as_deref(),
        Some("Harbour Light")
    );
}

#[test]
fn an_empty_name_clears_it_and_the_title_page_comes_back() {
    let s = Scratch::new("library-rename");
    let root = s.dir("lib");
    let folder = script(&root, "/scripts/lampkeeper.pdf");
    rename_in_library(&root, &folder, "Harbour Light").unwrap();
    let cleared = rename_in_library(&root, &folder, " \t ").unwrap();
    assert_eq!(cleared.title, "The Lamp Keeper");
    assert_eq!(cleared.name, None);
    assert_eq!(
        renamed_line(&cleared),
        format!("{folder} is called by its title page again")
    );
    assert!(marker(&folder).get(NAME_KEY).is_none(), "the key is gone");
    assert_eq!(title_listed(&root).as_deref(), Some("The Lamp Keeper"));

    // A script with no Title on its title page is called by its file
    // name, as its row is.
    let bare = script(&root, "/scripts/Weatherglass.pdf");
    write(
        &host::join(&[&bare, "Weatherglass.fountain"]),
        "INT. HOUSE - DAY\n\nRain.\n",
    );
    assert_eq!(
        rename_in_library(&root, &bare, "").unwrap().title,
        "Weatherglass"
    );
}

#[test]
fn a_long_name_or_one_with_a_line_break_is_refused_and_nothing_is_written() {
    let s = Scratch::new("library-rename");
    let root = s.dir("lib");
    let folder = script(&root, "/scripts/lampkeeper.pdf");
    rename_in_library(&root, &folder, "Harbour Light").unwrap();
    let before = marker(&folder);
    let longest = "x".repeat(MAX_NAME_CHARS);
    assert_eq!(
        kept_name_of(&longest).unwrap().as_deref(),
        Some(longest.as_str())
    );
    // Characters, not bytes: 200 of them in a script that needs two
    // bytes each still fit.
    assert!(kept_name_of(&"é".repeat(MAX_NAME_CHARS)).is_ok());
    for (name, says) in [
        (
            "x".repeat(MAX_NAME_CHARS + 1),
            "a script's name can be at most 200 characters long, and this one is 201",
        ),
        (
            "Harbour\nLight".to_owned(),
            "a script's name is one line: it cannot hold a line break",
        ),
        (
            "Harbour\r\nLight".to_owned(),
            "a script's name is one line: it cannot hold a line break",
        ),
        (
            "Harbour\u{2028}Light".to_owned(),
            "a script's name is one line: it cannot hold a line break",
        ),
        (
            "Harbour\u{1}Light".to_owned(),
            "a script's name cannot hold a control character, such as a tab",
        ),
        (
            "Harbour\tLight".to_owned(),
            "a script's name cannot hold a control character, such as a tab",
        ),
        (
            "Harbour\u{7F}Light".to_owned(),
            "a script's name cannot hold a control character, such as a tab",
        ),
        (
            "Harbour\u{FFFF}Light".to_owned(),
            "a script's name cannot hold a control character, such as a tab",
        ),
    ] {
        let err = rename_in_library(&root, &folder, &name).unwrap_err();
        assert_eq!(
            refusal(err),
            ("usage".to_owned(), says.to_owned()),
            "{name:?}"
        );
        assert_eq!(marker(&folder), before, "{name:?}");
    }
    // A line break at either end is trimmed away, not refused.
    assert_eq!(
        kept_name_of("\nHarbour Light\n").unwrap().as_deref(),
        Some("Harbour Light")
    );
}

#[test]
fn only_a_folder_the_listing_lists_is_renamed() {
    let s = Scratch::new("library-rename");
    let root = s.dir("lib");
    let folder = script(&root, "/scripts/lampkeeper.pdf");
    let stray = s.dir("lib/Loose");
    let outside = s.dir("elsewhere");
    for (target, code) in [
        (stray.clone(), "not-a-script"),
        (root.clone(), "not-a-script"),
        (outside.clone(), "not-a-script"),
        (host::join(&[&root, "nothing here"]), "not-a-script"),
        ("lampkeeper".to_owned(), "usage"),
    ] {
        let err = rename_in_library(&root, &target, "Harbour Light").unwrap_err();
        assert_eq!(refusal(err).0, code, "{target}");
    }
    let err = rename_in_library(&root, "lampkeeper", "x").unwrap_err();
    assert_eq!(
        refusal(err).1,
        "library-rename needs a script folder's full path, not \"lampkeeper\""
    );
    assert!(
        names(&stray).is_empty(),
        "nothing written in the stray folder"
    );
    assert!(marker(&folder).get(NAME_KEY).is_none());
}

#[test]
fn a_kept_name_that_is_not_one_this_engine_writes_is_not_read() {
    let s = Scratch::new("library-rename");
    let root = s.dir("lib");
    let folder = script(&root, "/scripts/lampkeeper.pdf");
    let fountain = host::join(&[&folder, "lampkeeper.fountain"]);
    let source = "/scripts/lampkeeper.pdf";
    for bad in [
        json!(7),
        json!("   "),
        json!("Two\nLines"),
        json!("x".repeat(MAX_NAME_CHARS + 1)),
    ] {
        write(
            &host::join(&[&folder, "source.json"]),
            &json!({"name": bad, "source": source}).to_string(),
        );
        assert_eq!(read_kept_name(&fountain), None, "{bad}");
        assert_eq!(title_listed(&root).as_deref(), Some("The Lamp Keeper"));
    }
    // Outside a script folder nothing is written.
    let loose = s.join("Loose.fountain");
    write(&loose, WREN);
    save_kept_name(&loose, Some("Harbour Light")).unwrap();
    assert_eq!(names(&s.str()), ["Loose.fountain", "lib"]);
}

#[test]
fn a_kept_name_titles_the_book_and_an_explicit_title_still_wins() {
    let opf = |opts: ConvertOptions| {
        let book = convert_fountain(WREN, opts, None).unwrap();
        (book.title.clone(), common::epub::opf(&book.epub))
    };
    let (title, package) = opf(ConvertOptions {
        kept_name: Some("Harbour Light".into()),
        ..ConvertOptions::default()
    });
    assert_eq!(title, "Harbour Light");
    assert!(
        package.contains("<dc:title>Harbour Light</dc:title>"),
        "{package}"
    );
    let (title, package) = opf(ConvertOptions {
        title: Some("Given Outright".into()),
        kept_name: Some("Harbour Light".into()),
        ..ConvertOptions::default()
    });
    assert_eq!(title, "Given Outright");
    assert!(package.contains("<dc:title>Given Outright</dc:title>"));
    // Without either, the title page's; without that, the placeholder.
    assert_eq!(opf(ConvertOptions::default()).0, "The Lamp Keeper");
    let bare = convert_fountain(
        "INT. HOUSE - DAY\n\nRain.\n",
        ConvertOptions::default(),
        None,
    );
    assert_eq!(bare.unwrap().title, "Untitled Screenplay");
}

struct NoCalibre;

impl RungBuilders for NoCalibre {
    fn kfx(&self, _epub: &str, _s: &mut dyn FnMut(&str)) -> Result<String, EngineError> {
        Err(EngineError::plain("no Calibre here"))
    }

    fn azw3(&self, _epub: &str) -> Result<String, EngineError> {
        Err(EngineError::plain("not asked"))
    }
}

#[test]
fn the_mobi_made_again_from_the_fountain_takes_the_kept_name_too() {
    let s = Scratch::new("library-rename");
    let root = s.dir("lib");
    let folder = script(&root, "/scripts/lampkeeper.pdf");
    rename_in_library(&root, &folder, "Harbour Light").unwrap();
    let epub = host::join(&[&folder, "lampkeeper.epub"]);
    let opts = FreshKindleArtifactOptions {
        epub: epub.clone(),
        fountain_path: Some(host::join(&[&folder, "lampkeeper.fountain"])),
        format: DEFAULT_FORMAT_OPTIONS,
        calibre_available: false,
        kfx_ready: false,
    };
    let out = fresh_artifact_of(KindleRung::Mobi, &opts, &NoCalibre, &Books, &mut |_| {}).unwrap();
    assert_eq!(out, mobi_sibling(&epub));
    let mobi = String::from_utf8_lossy(&std::fs::read(&out).unwrap()).into_owned();
    assert!(mobi.contains("Harbour Light"), "titled by the kept name");
}

#[test]
fn a_renamed_answer_is_its_own_shape() {
    // The fields the command line prints, and none it does not.
    let renamed = LibraryRenamed {
        folder: "/l/A".into(),
        title: "B".into(),
        epub: "/l/A/A.epub".into(),
        fountain: "/l/A/A.fountain".into(),
        name: Some("B".into()),
    };
    assert_eq!(
        serde_json::to_value(&renamed).unwrap(),
        json!({"folder": "/l/A", "title": "B", "epub": "/l/A/A.epub"})
    );
}
