//! Port of tests/cli.test.ts: the conversion's command line, its --json
//! contract, what it refuses, the picture folder it writes and reads, its
//! progress lines, and the pure error mapping behind `unwritable`.
//!
//! Retired: "--debug lets pdf.js warnings through". The Rust reader has no
//! pdf.js and prints no warnings, so there is nothing for --debug to let
//! through; the half that matters, a normal conversion printing nothing on
//! stderr, is kept.

mod common;

use std::path::Path;

use common::{Sandbox, base64, fixture, keys, list_dir, png, progress_events, zip_entries};
use serde_json::Value;
use sp_engine::errors::{
    EngineError, ErrorCode, FsError, is_write_refusal, map_read_error, unwritable_error,
};

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn pdf() -> String {
    s(&fixture("screenplay.pdf"))
}

fn fx(name: &str) -> String {
    s(&fixture(name))
}

/// The EPUB's picture entries, sorted.
fn epub_images(epub: &Path) -> Vec<String> {
    let mut names: Vec<String> = zip_entries(epub)
        .into_iter()
        .map(|(n, _)| n)
        .filter(|n| n.starts_with("OEBPS/images/") && !n.ends_with('/'))
        .collect();
    names.sort();
    names
}

// --- --version ---

#[test]
fn version_prints_a_semver_and_exits_0() {
    let ran = Sandbox::new().run(&["--version"]);
    assert_eq!(ran.code, 0);
    let out = ran.stdout.trim();
    let v = out
        .strip_prefix("method-reader ")
        .and_then(|v| v.strip_suffix(" (Method Reader)"))
        .expect("method-reader X (Method Reader)");
    let parts: Vec<&str> = v.split(['.', '-', '+']).take(3).collect();
    assert_eq!(parts.len(), 3, "{out}");
    for p in parts {
        assert!(p.parse::<u32>().is_ok(), "{out}");
    }
}

#[test]
fn version_matches_the_workspace_version() {
    // The one version file is rust/Cargo.toml's [workspace.package]
    // (package.json's, while there was a TypeScript engine).
    let ran = Sandbox::new().run(&["--version"]);
    assert_eq!(
        ran.stdout.trim(),
        format!(
            "method-reader {} (Method Reader)",
            env!("CARGO_PKG_VERSION")
        )
    );
}

// --- quiet ---

#[test]
fn a_normal_conversion_prints_nothing_to_stderr() {
    let sb = Sandbox::new();
    let ran = sb.run(&[&pdf(), "-o", &sb.p("quiet.epub"), "--no-fountain"]);
    assert_eq!(ran.code, 0, "{}", ran.stderr);
    assert_eq!(ran.stderr, "");
}

// --- the --json contract ---

#[test]
fn success_emits_a_single_machine_readable_json_object() {
    let sb = Sandbox::new();
    let out = sb.p("cli-json-test.epub");
    let ran = sb.run(&[&pdf(), "-o", &out, "--no-fountain", "--json"]);
    assert_eq!(ran.code, 0);
    let r = ran.json();
    assert_eq!(r["ok"], true);
    assert_eq!(r["title"], "The Last Video Store");
    assert_eq!(r["author"], "A. N. Placeholder");
    assert_eq!(r["pages"], 5);
    assert_eq!(r["scenes"], 5);
    assert_eq!(r["epubPath"], out.as_str());
    assert!(r.get("fountainPath").is_none());
    let top = r["topCharacters"].as_array().expect("an array");
    assert_eq!(
        top[..3],
        [Value::from("MARGO"), "DEV".into(), "NIECE".into()]
    );
    assert!(r["warnings"].is_array());
}

#[test]
fn guard_errors_emit_structured_json_with_a_code_and_exit_1() {
    let sb = Sandbox::new();
    let ran = sb.run(&[&fx("prose.pdf"), "-o", &sb.p("nope.epub"), "--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "not-screenplay");
    assert!(ran.error_message().contains("--force"));
}

#[test]
fn scanned_pdfs_report_their_own_error_code() {
    let sb = Sandbox::new();
    let ran = sb.run(&[&fx("blank-pages.pdf"), "-o", &sb.p("nope2.epub"), "--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "scanned");
}

#[test]
fn an_unknown_flag_with_json_still_emits_json_not_a_stack_trace() {
    let ran = Sandbox::new().run(&[&pdf(), "--json", "--no-such-flag"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "usage");
}

#[test]
fn no_input_with_json_emits_json_not_help_text_on_stdout() {
    let ran = Sandbox::new().run(&["--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "usage");
}

#[test]
fn two_inputs_is_a_usage_error_not_an_internal_one() {
    let ran = Sandbox::new().run(&[&pdf(), &fx("prose.pdf"), "--json"]);
    assert_eq!(ran.error_code(), "usage");
}

#[test]
fn a_directory_named_pdf_reports_unreadable() {
    let sb = Sandbox::new();
    let dir = sb.mkdir("a-folder.pdf");
    let ran = sb.run(&[&s(&dir), "--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "unreadable");
}

#[test]
fn a_malformed_options_file_reports_bad_options() {
    let sb = Sandbox::new();
    let opts = sb.write("bad-options.json", "not json");
    let ran = sb.run(&[&pdf(), "--options", &s(&opts), "--json"]);
    assert_eq!(ran.error_code(), "bad-options");
}

#[test]
fn a_corrupt_pdf_emits_pure_json_on_stdout_never_a_stack_trace() {
    let sb = Sandbox::new();
    let p = sb.write("garbage.pdf", "not a pdf at all");
    let ran = sb.run(&[&s(&p), "--json"]);
    assert_eq!(ran.code, 1);
    // json() fails if anything non-JSON leaked.
    assert!(!ran.error_code().is_empty());
}

#[cfg(unix)]
#[test]
fn an_unreadable_permissions_file_emits_json_with_a_code() {
    let sb = Sandbox::new();
    let p = sb.write("locked.pdf", "x");
    common::chmod(&p, 0o000);
    let ran = sb.run(&[&s(&p), "--json"]);
    common::chmod(&p, 0o644);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "unreadable");
}

#[test]
fn the_success_payload_keys_match_the_committed_contract_sample() {
    // Every optional output the sample carries is requested, so each of its
    // keys is present. imagesPath and imageCount appear only when the
    // conversion carried pictures, and settingsPath only when it used the
    // script's saved settings, so the sample leaves those three out.
    let sb = Sandbox::new();
    let ran = sb.run(&[
        &pdf(),
        "-o",
        &sb.p("contract.epub"),
        "--json",
        "--mobi",
        "--debug",
        "--preview-html",
        &sb.p("contract.html"),
    ]);
    let sample: Value = serde_json::from_slice(
        &std::fs::read(fixture("engine-result-sample.json")).expect("the sample"),
    )
    .expect("JSON");
    assert_eq!(keys(&ran.json()), keys(&sample));
}

// --- an output folder that does not exist ---
//
// Refused before anything is read or written, in the CLI's own one line,
// naming the folder. The folder is never made for the caller.

/// The run, with `flag` aimed into the missing folder. Under the other
/// flags -o still names a real folder, so the book NOT being there proves
/// the refusal came before the conversion wrote anything.
fn args_into(flag: &str, target: &str, beside: &str) -> Vec<String> {
    let mut args = vec![pdf(), flag.to_owned(), target.to_owned()];
    if flag != "-o" {
        args.extend(["-o".to_owned(), beside.to_owned()]);
    }
    args
}

fn run_owned(sb: &Sandbox, args: &[String]) -> common::Ran {
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    sb.run(&refs)
}

fn missing_folder_text(flag: &str, file: &str) {
    let sb = Sandbox::new();
    let missing = sb.p("no-such-folder");
    let beside = sb.p(&format!("missing-folder-text{flag}.epub"));
    let ran = run_owned(&sb, &args_into(flag, &format!("{missing}/{file}"), &beside));
    assert_eq!(ran.code, 1);
    assert_eq!(ran.stdout, "");
    let lines: Vec<&str> = ran.stderr.trim().lines().collect();
    assert_eq!(lines.len(), 1, "{:?}", ran.stderr);
    assert!(lines[0].starts_with("method-reader: "), "{}", lines[0]);
    assert!(lines[0].contains(flag), "{}", lines[0]);
    assert!(lines[0].contains(&missing), "{}", lines[0]);
    assert!(!lines[0].contains("ENOENT"));
    assert!(!lines[0].contains('\u{2014}'));
    assert!(!Path::new(&missing).exists());
    assert!(!Path::new(&beside).exists());
}

fn missing_folder_json(flag: &str, file: &str) {
    let sb = Sandbox::new();
    let missing = sb.p("no-such-folder");
    let beside = sb.p(&format!("missing-folder-json{flag}.epub"));
    let mut args = args_into(flag, &format!("{missing}/{file}"), &beside);
    args.push("--json".into());
    let ran = run_owned(&sb, &args);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "usage");
    let m = ran.error_message();
    assert!(m.contains(flag), "{m}");
    assert!(m.contains(&missing), "{m}");
    assert!(!Path::new(&missing).exists());
    assert!(!Path::new(&beside).exists());
}

// --- an output that cannot be written ---

fn names_a_folder(flag: &str) {
    let sb = Sandbox::new();
    let folder = s(&sb.mkdir(&format!("names-a-folder{flag}")));
    let beside = sb.p(&format!("names-a-folder-beside{flag}.epub"));
    let mut args = vec![pdf(), flag.to_owned(), folder.clone(), "--json".into()];
    if flag != "-o" {
        args.extend(["-o".to_owned(), beside.clone()]);
    }
    let ran = run_owned(&sb, &args);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "usage");
    let m = ran.error_message();
    assert!(m.contains(flag), "{m}");
    assert!(m.contains(&folder), "{m}");
    assert!(m.contains("is a folder"), "{m}");
    assert!(!Path::new(&beside).exists());
}

#[cfg(unix)]
fn no_write_permission(flag: &str, file: &str) {
    let sb = Sandbox::new();
    let locked = sb.mkdir(&format!("no-write{flag}"));
    common::chmod(&locked, 0o555);
    let target = format!("{}/{file}", s(&locked));
    let mut args = vec![pdf(), flag.to_owned(), target.clone(), "--json".into()];
    if flag != "-o" {
        args.extend([
            "-o".to_owned(),
            sb.p(&format!("no-write-beside{flag}.epub")),
        ]);
    }
    let ran = run_owned(&sb, &args);
    common::chmod(&locked, 0o755);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "unwritable");
    let m = ran.error_message();
    assert!(m.contains(&target), "{m}");
    assert!(m.contains("permission denied"), "{m}");
    assert!(!m.contains(".tmp"), "{m}");
}

macro_rules! output_tests {
    ($($name:ident: $flag:literal, $file:literal;)*) => {
        $(mod $name {
            #[test]
            fn one_line_naming_the_missing_folder_nothing_written() {
                super::missing_folder_text($flag, $file);
            }

            #[test]
            fn under_json_a_usage_answer_naming_the_missing_folder() {
                super::missing_folder_json($flag, $file);
            }

            #[test]
            fn naming_a_folder_is_refused_before_converting() {
                super::names_a_folder($flag);
            }

            #[cfg(unix)]
            #[test]
            fn into_a_folder_with_no_write_permission_is_unwritable_naming_the_path() {
                super::no_write_permission($flag, $file);
            }
        })*
    };
}

output_tests! {
    output_o: "-o", "x.epub";
    output_fountain: "--fountain", "x.fountain";
    output_preview_html: "--preview-html", "x.html";
    output_fdx: "--fdx", "x.fdx";
    output_azw3: "--azw3", "x.azw3";
}

#[test]
fn a_folder_that_is_really_a_file_is_named_as_one() {
    let sb = Sandbox::new();
    let not_a_folder = s(&sb.write("plain-file.txt", "not a folder"));
    let ran = sb.run(&[&pdf(), "-o", &format!("{not_a_folder}/x.epub"), "--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "usage");
    let m = ran.error_message();
    assert!(m.contains(&not_a_folder), "{m}");
    assert!(m.contains("is a file"), "{m}");
}

#[cfg(unix)]
#[test]
fn an_unwritable_output_without_json_is_one_line_not_a_stack_trace() {
    let sb = Sandbox::new();
    let locked = sb.mkdir("no-write-text");
    common::chmod(&locked, 0o555);
    let target = format!("{}/x.epub", s(&locked));
    let ran = sb.run(&[&pdf(), "-o", &target]);
    common::chmod(&locked, 0o755);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.stdout, "");
    let lines: Vec<&str> = ran.stderr.trim().lines().collect();
    assert_eq!(lines.len(), 1, "{:?}", ran.stderr);
    assert!(lines[0].starts_with("method-reader: "));
    assert!(lines[0].contains(&target));
    assert!(!lines[0].contains('\u{2014}'));
}

#[test]
fn a_folder_where_the_default_output_goes_is_named_in_plain_words_with_no_temp_file_left() {
    // No -o, so no path to check up front: the book goes beside the input,
    // and a folder already sits at that name.
    let sb = Sandbox::new();
    let dir = sb.mkdir("default-collides");
    sb.copy_fixture("screenplay.pdf", "default-collides/x.pdf");
    sb.mkdir("default-collides/x.epub");
    let ran = sb.run(&[&s(&dir.join("x.pdf")), "--no-fountain", "--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "unwritable");
    let m = ran.error_message();
    assert!(m.contains(&s(&dir.join("x.epub"))), "{m}");
    assert!(m.contains("a folder, not a file"), "{m}");
    assert!(!m.contains(".tmp"), "{m}");
    assert_eq!(list_dir(&dir), ["x.epub", "x.pdf"]);
}

#[test]
fn the_debug_dump_written_in_place_fails_the_same_way() {
    let sb = Sandbox::new();
    let dir = sb.mkdir("debug-collides");
    sb.mkdir("debug-collides/x.elements.json");
    let ran = sb.run(&[
        &pdf(),
        "-o",
        &s(&dir.join("x.epub")),
        "--no-fountain",
        "--debug",
        "--json",
    ]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "unwritable");
    let m = ran.error_message();
    assert!(m.contains(&s(&dir.join("x.elements.json"))), "{m}");
    assert!(m.contains("a folder, not a file"), "{m}");
}

// --- the picture folder beside the .fountain ---

const PNG_MAGIC: [u8; 8] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

/// A fresh folder in the sandbox, and `out.epub` in it.
fn images_run(sb: &Sandbox, dir: &str, input: &str, extra: &[&str]) -> common::Ran {
    let out = sb.path(dir).join("out.epub");
    let mut args = vec![fx(input), "-o".into(), s(&out)];
    args.extend(extra.iter().map(|a| (*a).to_owned()));
    run_owned(sb, &args)
}

#[test]
fn a_pdf_with_pictures_writes_them_beside_its_fountain() {
    let sb = Sandbox::new();
    let dir = sb.mkdir("d");
    let ran = images_run(&sb, "d", "images.pdf", &[]);
    assert_eq!(ran.code, 0, "{}", ran.stderr);
    assert!(dir.join("out.fountain").exists());
    // Exactly the folder's contents: a leftover temp would fail here.
    assert_eq!(
        list_dir(&dir.join("out.images")),
        ["p001-01.png", "p002-01.png"]
    );
    let first = std::fs::read(dir.join("out.images/p001-01.png")).unwrap();
    assert_eq!(first[..8], PNG_MAGIC);
    assert!(first.len() > 100);
}

#[test]
fn every_picture_written_is_one_the_fountain_asks_for() {
    let sb = Sandbox::new();
    let dir = sb.mkdir("d");
    images_run(&sb, "d", "images.pdf", &[]);
    let text = std::fs::read_to_string(dir.join("out.fountain")).unwrap();
    let names = list_dir(&dir.join("out.images"));
    assert!(!names.is_empty());
    for name in names {
        assert!(text.contains(&format!("[[img: {name}")), "{name}");
    }
}

#[test]
fn json_reports_the_picture_folder_and_the_count() {
    let sb = Sandbox::new();
    let dir = sb.mkdir("d");
    let res = images_run(&sb, "d", "images.pdf", &["--json"]).json();
    assert_eq!(res["ok"], true);
    assert_eq!(res["imagesPath"], s(&dir.join("out.images")).as_str());
    assert_eq!(res["imageCount"], 2);
    assert_eq!(res["warnings"], serde_json::json!([]));
}

#[test]
fn the_human_output_names_the_picture_folder_it_wrote() {
    let sb = Sandbox::new();
    let dir = sb.mkdir("d");
    let ran = images_run(&sb, "d", "images.pdf", &[]);
    assert!(
        ran.stdout
            .contains(&format!("wrote {}", s(&dir.join("out.images")))),
        "{}",
        ran.stdout
    );
}

#[test]
fn a_script_with_no_pictures_gets_no_folder_and_no_fields() {
    let sb = Sandbox::new();
    let dir = sb.mkdir("d");
    let res = images_run(&sb, "d", "screenplay.pdf", &["--json"]).json();
    assert!(res.get("imagesPath").is_none());
    assert!(res.get("imageCount").is_none());
    assert!(!dir.join("out.images").exists());
}

#[test]
fn under_library_the_picture_folder_sits_in_the_scripts_own_library_folder() {
    let mut sb = Sandbox::new();
    let scripts = sb.mkdir("d/scripts");
    let root = sb.path("d/lib");
    sb.copy_fixture("images.pdf", "d/scripts/Plates.pdf");
    sb.set("METHOD_READER_LIBRARY", s(&root));
    let ran = sb.run(&[&s(&scripts.join("Plates.pdf")), "--library", "--json"]);
    assert_eq!(ran.code, 0, "{}", ran.stdout);
    let res = ran.json();
    assert_eq!(
        res["fountainPath"],
        common::join(&s(&root), "Plates/Plates.fountain").as_str()
    );
    assert_eq!(
        res["imagesPath"],
        common::join(&s(&root), "Plates/Plates.images").as_str()
    );
    assert_eq!(
        list_dir(&root.join("Plates/Plates.images")),
        ["p001-01.png", "p002-01.png"]
    );
    // Nothing beside the PDF: the library conversion writes into the
    // library only.
    assert_eq!(list_dir(&scripts), ["Plates.pdf"]);
}

#[test]
fn a_re_conversion_that_lost_its_art_clears_the_folder_it_left() {
    let sb = Sandbox::new();
    let dir = sb.mkdir("d");
    images_run(&sb, "d", "images.pdf", &[]);
    assert_eq!(list_dir(&dir.join("out.images")).len(), 2);
    let res = images_run(&sb, "d", "screenplay.pdf", &["--json"]).json();
    assert!(res.get("imageCount").is_none());
    assert!(!dir.join("out.images").exists());
}

#[test]
fn a_picture_folder_that_cannot_be_written_is_unwritable_and_says_the_book_is_there() {
    // Partial output is the contract: the EPUB was written before the
    // folder failed, so the answer says what is on disk, naming the folder
    // the user would look for, never a temp file.
    let sb = Sandbox::new();
    let dir = sb.mkdir("d");
    sb.write("d/out.images", "a file, not a folder");
    let ran = images_run(&sb, "d", "images.pdf", &["--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "unwritable");
    let m = ran.error_message();
    assert!(m.contains(&s(&dir.join("out.images"))), "{m}");
    assert!(
        m.contains(&format!("{} was already written", s(&dir.join("out.epub")))),
        "{m}"
    );
    assert!(!m.contains(".tmp"), "{m}");
    assert!(dir.join("out.epub").exists());
    // Pictures before text: the failed folder stops the run before the
    // .fountain is written.
    assert!(!dir.join("out.fountain").exists());
    assert!(m.contains("The .fountain was not updated."), "{m}");
}

#[cfg(unix)]
#[test]
fn a_failed_re_conversion_keeps_the_old_fountain_naming_the_old_pictures() {
    let sb = Sandbox::new();
    let dir = sb.mkdir("d");
    images_run(&sb, "d", "images.pdf", &[]);
    // Marked, because the second run converts the SAME PDF: a .fountain
    // rewritten ahead of its pictures would come out byte-identical.
    let fountain = dir.join("out.fountain");
    let mut text = std::fs::read_to_string(&fountain).unwrap();
    text.push_str("\n[[marker: from the run before]]\n");
    std::fs::write(&fountain, &text).unwrap();
    assert!(text.contains("[[img: p001-01.png"));
    common::chmod(&dir.join("out.images"), 0o555);
    let ran = images_run(&sb, "d", "images.pdf", &["--json"]);
    common::chmod(&dir.join("out.images"), 0o755);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "unwritable");
    assert!(
        ran.error_message()
            .contains("The .fountain was not updated.")
    );
    assert_eq!(std::fs::read_to_string(&fountain).unwrap(), text);
    assert!(text.contains("[[marker: from the run before]]"));
}

#[cfg(unix)]
#[test]
fn a_picture_folder_that_is_a_symlink_is_unwritable_and_its_target_is_untouched() {
    let sb = Sandbox::new();
    let dir = sb.mkdir("d");
    let elsewhere = sb.mkdir("elsewhere");
    sb.write("elsewhere/p009-01.png", png(9));
    std::os::unix::fs::symlink(&elsewhere, dir.join("out.images")).unwrap();
    let ran = images_run(&sb, "d", "images.pdf", &["--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "unwritable");
    assert!(ran.error_message().contains(&s(&dir.join("out.images"))));
    assert_eq!(list_dir(&elsewhere), ["p009-01.png"]);
    assert!(!dir.join("out.fountain").exists());
}

#[cfg(unix)]
#[test]
fn a_script_with_no_pictures_never_prunes_through_a_symlinked_folder() {
    let sb = Sandbox::new();
    let dir = sb.mkdir("d");
    let elsewhere = sb.mkdir("elsewhere");
    sb.write("elsewhere/p009-01.png", png(9));
    std::os::unix::fs::symlink(&elsewhere, dir.join("out.images")).unwrap();
    let ran = images_run(&sb, "d", "screenplay.pdf", &[]);
    assert_eq!(ran.code, 0, "{}", ran.stderr);
    assert_eq!(list_dir(&elsewhere), ["p009-01.png"]);
}

#[test]
fn a_picture_folder_whose_parent_is_missing_is_refused_before_anything_is_written() {
    let sb = Sandbox::new();
    let dir = sb.mkdir("d");
    let fountain = dir.join("missing/deeper/x.fountain");
    let ran = images_run(
        &sb,
        "d",
        "images.pdf",
        &["--fountain", &s(&fountain), "--json"],
    );
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "usage");
    let m = ran.error_message();
    assert!(m.contains(&s(&dir.join("missing/deeper"))), "{m}");
    assert!(!m.contains(".tmp"));
    assert!(list_dir(&dir).is_empty());
}

#[test]
fn a_scanned_pdf_writes_no_folder_at_all() {
    let sb = Sandbox::new();
    let dir = sb.mkdir("d");
    let ran = images_run(&sb, "d", "blank-pages.pdf", &["--json"]);
    assert_eq!(ran.code, 1);
    assert_eq!(ran.error_code(), "scanned");
    assert!(list_dir(&dir).is_empty());
}

#[test]
fn no_fountain_writes_no_picture_folder_either() {
    // The folder is the .fountain's other half. The pictures are not lost:
    // the book carries its own copy of each.
    let sb = Sandbox::new();
    let dir = sb.mkdir("d");
    let res = images_run(&sb, "d", "images.pdf", &["--no-fountain", "--json"]).json();
    assert!(res.get("imagesPath").is_none());
    // Still reported as CARRIED: a fact about the conversion.
    assert_eq!(res["imageCount"], 2);
    assert_eq!(list_dir(&dir), ["out.epub"]);
    assert_eq!(
        epub_images(&dir.join("out.epub")),
        ["OEBPS/images/p001-01.png", "OEBPS/images/p002-01.png"]
    );
}

#[test]
fn fountain_elsewhere_takes_the_picture_folder_with_it() {
    let sb = Sandbox::new();
    let dir = sb.mkdir("d");
    let other = sb.mkdir("d/text");
    let res = images_run(
        &sb,
        "d",
        "images.pdf",
        &["--fountain", &s(&other.join("script.fountain")), "--json"],
    )
    .json();
    assert_eq!(res["imagesPath"], s(&other.join("script.images")).as_str());
    assert!(!dir.join("out.images").exists());
    assert_eq!(list_dir(&other.join("script.images")).len(), 2);
}

#[test]
fn pictures_the_user_put_in_the_folder_are_never_deleted() {
    // THE data-loss test: only names the engine itself can produce may
    // ever be removed.
    let sb = Sandbox::new();
    let dir = sb.mkdir("d");
    let folder = sb.mkdir("d/lookbook.images");
    for name in [
        "board-01.png",
        "sunset.png",
        "A-01.png",
        "p1.png",
        "readme.md",
    ] {
        std::fs::write(folder.join(name), png(0)).unwrap();
    }
    images_run(
        &sb,
        "d",
        "images.pdf",
        &["--fountain", &s(&dir.join("lookbook.fountain"))],
    );
    assert_eq!(
        list_dir(&folder),
        [
            "A-01.png",
            "board-01.png",
            "p001-01.png",
            "p002-01.png",
            "p1.png",
            "readme.md",
            "sunset.png",
        ]
    );
}

#[test]
fn a_crashed_runs_temp_file_does_not_live_in_the_folder_forever() {
    let sb = Sandbox::new();
    let dir = sb.mkdir("d");
    images_run(&sb, "d", "images.pdf", &[]);
    std::fs::write(dir.join("out.images/.p001-01.png.999.tmp"), png(0)).unwrap();
    images_run(&sb, "d", "images.pdf", &[]);
    assert_eq!(
        list_dir(&dir.join("out.images")),
        ["p001-01.png", "p002-01.png"]
    );
}

#[test]
fn a_second_conversion_clears_the_first_runs_stale_pictures() {
    let sb = Sandbox::new();
    let dir = sb.mkdir("d");
    images_run(&sb, "d", "images.pdf", &[]);
    std::fs::write(dir.join("out.images/p009-09.png"), png(0)).unwrap();
    std::fs::write(dir.join("out.images/notes.txt"), "mine").unwrap();
    images_run(&sb, "d", "images.pdf", &[]);
    // The prune is bounded to our own pictures.
    assert_eq!(
        list_dir(&dir.join("out.images")),
        ["notes.txt", "p001-01.png", "p002-01.png"]
    );
}

#[test]
fn a_picture_is_written_whole_or_not_at_all() {
    let sb = Sandbox::new();
    let dir = sb.mkdir("d");
    images_run(&sb, "d", "images.pdf", &[]);
    for name in list_dir(&dir.join("out.images")) {
        assert!(!name.starts_with('.'), "{name}");
        let len = std::fs::metadata(dir.join("out.images").join(&name))
            .unwrap()
            .len();
        assert!(len > 100, "{name}");
    }
}

// --- a .fountain input reads its picture folder ---

/// A script whose notes name two pictures, and the folder beside it.
fn fountain_script(sb: &Sandbox, name: &str) -> std::path::PathBuf {
    let dir = sb.mkdir(name);
    std::fs::write(
        dir.join("x.fountain"),
        "Title: T\n\nINT. LAB - DAY\n\nBefore.\n\n[[img: p001-01.png w=40]]\n\n\
         Between.\n\n[[img: p002-01.png w=60]]\n\nAfter.\n",
    )
    .unwrap();
    std::fs::create_dir(dir.join("x.images")).unwrap();
    dir
}

struct Rendered {
    res: Value,
    body: String,
    images: Vec<String>,
}

fn render(sb: &Sandbox, dir: &Path, extra: &[&str]) -> Rendered {
    let epub = dir.join("x.epub");
    let mut args = vec![
        s(&dir.join("x.fountain")),
        "-o".into(),
        s(&epub),
        "--json".into(),
    ];
    args.extend(extra.iter().map(|a| (*a).to_owned()));
    let ran = run_owned(sb, &args);
    assert_eq!(ran.code, 0, "{}", ran.stdout);
    Rendered {
        res: ran.json(),
        body: common::zip_text(&epub, "OEBPS/text/body001.xhtml"),
        images: epub_images(&epub),
    }
}

fn figure(name: &str, w: u32) -> String {
    format!("<p class=\"figure w{w}\"><img src=\"../images/{name}\" alt=\"Illustration\"/></p>")
}

#[test]
fn a_fountains_notes_render_the_pictures_beside_them_and_the_book_carries_them() {
    let sb = Sandbox::new();
    let dir = fountain_script(&sb, "f");
    std::fs::write(dir.join("x.images/p001-01.png"), png(1)).unwrap();
    std::fs::write(dir.join("x.images/p002-01.png"), png(2)).unwrap();
    let r = render(&sb, &dir, &[]);
    assert_eq!(r.res["warnings"], serde_json::json!([]));
    assert!(r.body.contains(&figure("p001-01.png", 40)), "{}", r.body);
    assert!(r.body.contains(&figure("p002-01.png", 60)));
    assert_eq!(
        r.images,
        ["OEBPS/images/p001-01.png", "OEBPS/images/p002-01.png"]
    );
    // Nothing was written, so the answer names no folder and counts nothing.
    assert!(r.res.get("imagesPath").is_none());
    assert!(r.res.get("imageCount").is_none());
}

#[test]
fn the_windows_re_render_answer_shows_the_same_pictures_each_inside_its_figure() {
    // The window's Tune call: the .fountain, --json, --preview-inline. Each
    // picture rides in its <img> as a data: URI.
    let sb = Sandbox::new();
    let dir = fountain_script(&sb, "f");
    std::fs::write(dir.join("x.images/p001-01.png"), png(1)).unwrap();
    std::fs::write(dir.join("x.images/p002-01.png"), png(2)).unwrap();
    let r = render(&sb, &dir, &["--preview-inline"]);
    let inlined = |tag: u8, w: u32| {
        format!(
            "<p class=\"figure w{w}\"><img src=\"data:image/png;base64,{}\" alt=\"Illustration\"/></p>",
            base64(&png(tag))
        )
    };
    let html = r.res["previewHtml"].as_str().expect("previewHtml");
    assert!(html.contains(&inlined(1, 40)));
    assert!(html.contains(&inlined(2, 60)));
}

#[cfg(unix)]
#[test]
fn a_picture_the_folder_will_not_give_up_is_a_warning_and_no_figure() {
    let sb = Sandbox::new();
    let dir = fountain_script(&sb, "f");
    std::fs::write(dir.join("x.images/p001-01.png"), "not a picture").unwrap();
    std::fs::write(dir.join("elsewhere.png"), png(2)).unwrap();
    std::os::unix::fs::symlink(dir.join("elsewhere.png"), dir.join("x.images/p002-01.png"))
        .unwrap();
    let r = render(&sb, &dir, &[]);
    assert_eq!(
        r.res["warnings"],
        serde_json::json!([
            "2 pictures could not be read out of this script's picture folder, so they were left out."
        ])
    );
    assert!(!r.body.contains("class=\"figure"));
    assert!(r.images.is_empty());
}

#[test]
fn with_include_images_off_the_picture_folder_is_not_read_at_all() {
    let sb = Sandbox::new();
    let dir = fountain_script(&sb, "f");
    std::fs::write(dir.join("x.images/p001-01.png"), "not a picture").unwrap();
    std::fs::write(dir.join("x.images/p002-01.png"), png(2)).unwrap();
    let r = render(&sb, &dir, &["--options-json", "{\"includeImages\":false}"]);
    assert_eq!(r.res["warnings"], serde_json::json!([]));
    assert!(!r.body.contains("class=\"figure"));
    assert!(r.images.is_empty());
    // The control: the same folder, read, does warn.
    let on = render(&sb, &dir, &[]);
    assert_eq!(
        on.res["warnings"],
        serde_json::json!([
            "1 picture could not be read out of this script's picture folder, so it was left out."
        ])
    );
}

#[test]
fn a_fountain_render_never_writes_or_prunes_the_picture_folder_and_makes_none() {
    let sb = Sandbox::new();
    let dir = fountain_script(&sb, "f");
    let kept = [
        ".p003-01.png.999.tmp",
        "p001-01.png",
        "p002-01.png",
        "p009-09.png",
    ];
    for name in kept {
        std::fs::write(dir.join("x.images").join(name), png(0)).unwrap();
    }
    render(&sb, &dir, &[]);
    assert_eq!(list_dir(&dir.join("x.images")), kept);
    let bare = fountain_script(&sb, "bare");
    std::fs::remove_dir_all(bare.join("x.images")).unwrap();
    let r = render(&sb, &bare, &[]);
    assert!(!r.body.contains("class=\"figure"));
    assert!(!bare.join("x.images").exists());
}

#[test]
fn a_picture_the_folder_does_not_hold_is_a_warning_and_the_book_is_still_made() {
    // An iCloud copy evicted while offline is renamed `.p002-01.png.icloud`.
    let sb = Sandbox::new();
    let dir = fountain_script(&sb, "f");
    std::fs::write(dir.join("x.images/p001-01.png"), png(1)).unwrap();
    std::fs::write(dir.join("x.images/.p002-01.png.icloud"), "").unwrap();
    let r = render(&sb, &dir, &[]);
    assert_eq!(r.res["ok"], true);
    assert_eq!(
        r.res["warnings"],
        serde_json::json!([
            "1 picture this script shows could not be found in its picture folder, so the book was made without it."
        ])
    );
    assert!(r.body.contains(&figure("p001-01.png", 40)));
    assert_eq!(r.images, ["OEBPS/images/p001-01.png"]);
}

#[test]
fn a_picture_folder_that_is_not_there_loses_every_picture_said_once() {
    let sb = Sandbox::new();
    let dir = fountain_script(&sb, "f");
    std::fs::remove_dir_all(dir.join("x.images")).unwrap();
    let r = render(&sb, &dir, &[]);
    assert_eq!(
        r.res["warnings"],
        serde_json::json!([
            "2 pictures this script shows could not be found in its picture folder, so the book was made without them."
        ])
    );
    assert!(!r.body.contains("class=\"figure"));
}

#[test]
fn each_lost_picture_is_said_once_under_what_happened_to_it() {
    // p001 is listed and refused, p002 is not listed at all.
    let sb = Sandbox::new();
    let dir = fountain_script(&sb, "f");
    std::fs::write(dir.join("x.images/p001-01.png"), "not a picture").unwrap();
    let r = render(&sb, &dir, &[]);
    assert_eq!(
        r.res["warnings"],
        serde_json::json!([
            "1 picture could not be read out of this script's picture folder, so it was left out.",
            "1 picture this script shows could not be found in its picture folder, so the book was made without it."
        ])
    );
}

#[cfg(unix)]
#[test]
fn a_picture_folder_that_is_a_link_is_said_once_as_a_link() {
    let sb = Sandbox::new();
    let dir = fountain_script(&sb, "f");
    std::fs::remove_dir_all(dir.join("x.images")).unwrap();
    let elsewhere = sb.mkdir("f/elsewhere");
    std::fs::write(elsewhere.join("p001-01.png"), png(1)).unwrap();
    std::os::unix::fs::symlink(&elsewhere, dir.join("x.images")).unwrap();
    let r = render(&sb, &dir, &[]);
    assert_eq!(
        r.res["warnings"],
        serde_json::json!([
            "This script's picture folder is not a folder of its own (it is a link or a file), so no pictures were read from it."
        ])
    );
}

#[test]
fn with_include_images_off_a_missing_picture_is_not_a_loss_and_says_nothing() {
    let sb = Sandbox::new();
    let dir = fountain_script(&sb, "f");
    std::fs::remove_dir_all(dir.join("x.images")).unwrap();
    let r = render(&sb, &dir, &["--options-json", "{\"includeImages\":false}"]);
    assert_eq!(r.res["warnings"], serde_json::json!([]));
}

// --- conversion error mapping ---

#[test]
fn password_detection_is_typed_not_substring() {
    // A path that merely CONTAINS "password" must not classify: the
    // conversion's errors are an enum (sp_engine::convert::ConvertError),
    // so only the reader's own Encrypted answer can be `password`.
    let sb = Sandbox::new();
    let p = sb.write("password-notes/x.pdf", "not a pdf, and broke");
    let ran = sb.run(&[&s(&p), "--json"]);
    assert_eq!(ran.error_code(), "unreadable");
}

#[test]
fn missing_directory_and_permission_errors_are_all_unreadable() {
    for code in ["ENOENT", "EISDIR", "EACCES"] {
        let err = EngineError::Fs(FsError::with_code(code, code));
        let mapped = map_read_error(&err).unwrap_or_else(|| panic!("{code} maps"));
        assert_eq!(mapped.code, ErrorCode::Unreadable, "{code}");
    }
}

#[test]
fn only_the_filesystem_refusing_counts_as_unwritable() {
    for code in [
        "ENOENT", "EACCES", "EPERM", "EEXIST", "ENOSPC", "EROFS", "EISDIR", "ENOTDIR",
    ] {
        let err = EngineError::Fs(FsError::with_code(code, code));
        assert_eq!(
            format!("{code}: {}", is_write_refusal(&err)),
            format!("{code}: true")
        );
    }
    // A bug of ours must arrive as `internal`, not be dressed up as the
    // disk's fault.
    let bug = EngineError::Named {
        name: "TypeError",
        message: "cannot read properties of undefined".into(),
    };
    assert!(!is_write_refusal(&bug));
    assert!(!is_write_refusal(&EngineError::plain("a string throw")));
}

#[test]
fn a_corrupt_pdf_maps_to_unreadable() {
    // pdf.js's InvalidPDFException is the reader's LoadError::Invalid here.
    let sb = Sandbox::new();
    let p = sb.write("corrupt.pdf", "%PDF-1.4\nnot really\n");
    let ran = sb.run(&[&s(&p), "--json"]);
    assert_eq!(ran.error_code(), "unreadable");
}

// --- a picture folder that cannot be written (unwritable_error) ---

fn refusal(code: &str) -> EngineError {
    EngineError::Fs(FsError::with_code(
        code,
        format!("{code}: open '/lib/.p001-01.png.80708.tmp'"),
    ))
}

#[test]
fn a_picture_folder_refusal_is_unwritable_for_every_way_a_disk_says_no_naming_the_folder() {
    for code in [
        "ENOENT", "EACCES", "EPERM", "EEXIST", "ENOSPC", "EROFS", "EISDIR", "ENOTDIR",
    ] {
        let err = unwritable_error("/lib/A.images", &refusal(code), &[]).expect(code);
        assert_eq!(err.code, ErrorCode::Unwritable);
        assert!(err.message.contains("/lib/A.images"));
        assert!(err.message.contains(code));
        // The temp path the error came from is an implementation detail.
        assert!(!err.message.contains(".tmp"));
    }
}

#[test]
fn a_picture_folder_refusal_says_what_was_already_written_in_one_sentence_that_agrees_with_itself()
{
    assert_eq!(
        unwritable_error("/lib/A.images", &refusal("ENOSPC"), &["/lib/A.epub".into()])
            .unwrap()
            .message,
        "cannot write the pictures to /lib/A.images (ENOSPC). /lib/A.epub was already written. \
         The .fountain was not updated."
    );
    assert!(
        unwritable_error(
            "/lib/A.images",
            &refusal("ENOSPC"),
            &["/lib/A.epub".into(), "/lib/A.mobi".into()]
        )
        .unwrap()
        .message
        .contains("/lib/A.epub, /lib/A.mobi were already written.")
    );
}

#[test]
fn a_picture_folder_refusal_says_the_fountain_was_not_updated() {
    assert_eq!(
        unwritable_error("/lib/A.images", &refusal("EACCES"), &[])
            .unwrap()
            .message,
        "cannot write the pictures to /lib/A.images (EACCES). The .fountain was not updated."
    );
}

#[test]
fn anything_that_is_not_the_disk_refusing_is_left_to_the_catch_all() {
    let bug = EngineError::Named {
        name: "TypeError",
        message: "x is undefined".into(),
    };
    assert!(unwritable_error("/lib/A.images", &bug, &[]).is_none());
    assert!(unwritable_error("/lib/A.images", &refusal("EMFILE"), &[]).is_none());
    assert!(unwritable_error("/lib/A.images", &EngineError::plain("a string"), &[]).is_none());
}

// --- --progress ---

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("a number: {v}"))
}

#[test]
fn progress_keeps_stdout_a_single_json_object_while_reporting_on_stderr() {
    let sb = Sandbox::new();
    let ran = sb.run(&[
        &pdf(),
        "-o",
        &sb.p("prog.epub"),
        "--no-fountain",
        "--json",
        "--progress",
    ]);
    assert_eq!(ran.code, 0);
    // The whole of stdout, parsed as one value.
    let whole: Value = serde_json::from_str(&ran.stdout).expect("stdout is one JSON value");
    assert_eq!(whole["ok"], true);

    let events = progress_events(&ran.stderr);
    assert_eq!(events.iter().filter(|e| e["stage"] == "page").count(), 1);
    let ticks: Vec<&Value> = events.iter().filter(|e| e["stage"] != "page").collect();
    assert!(!ticks.is_empty());
    for t in &ticks {
        assert!(
            ["parse", "pictures", "render"].contains(&t["stage"].as_str().unwrap()),
            "{t}"
        );
        assert!((0.0..=100.0).contains(&f(&t["percent"])), "{t}");
    }

    // A reading tick names the page it has reached and how many there are.
    let reading: Vec<&&Value> = ticks.iter().filter(|t| t["stage"] == "parse").collect();
    assert!(!reading.is_empty());
    for t in &reading {
        let page = t["page"].as_u64().expect("an integer page");
        assert!(page >= 1 && page <= t["pages"].as_u64().unwrap(), "{t}");
    }
    let last = reading.last().unwrap();
    assert_eq!(last["page"], last["pages"]);

    // A bar that goes backwards is worse than no bar.
    let percents: Vec<f64> = ticks.iter().map(|t| f(&t["percent"])).collect();
    assert!(percents.windows(2).all(|w| w[0] <= w[1]), "{percents:?}");
    assert_eq!(*percents.last().unwrap(), 100.0);

    // The first render tick is said while the bar is short of 100.
    let first_render = ticks.iter().find(|t| t["stage"] == "render").unwrap();
    assert!(f(&first_render["percent"]) < 100.0);
}

#[test]
fn progress_names_each_stage_as_it_begins_even_where_the_percent_does_not_move() {
    let sb = Sandbox::new();
    let ran = sb.run(&[
        &fx("images.pdf"),
        "-o",
        &sb.p("prog-pictures.epub"),
        "--no-fountain",
        "--json",
        "--progress",
    ]);
    assert_eq!(ran.code, 0);
    let ticks: Vec<(String, f64, Option<u64>)> = progress_events(&ran.stderr)
        .into_iter()
        .filter(|e| e["stage"] != "page")
        .map(|t| {
            (
                t["stage"].as_str().unwrap().to_owned(),
                f(&t["percent"]),
                t["page"].as_u64(),
            )
        })
        .collect();
    let firsts: Vec<(String, f64)> = ticks
        .iter()
        .enumerate()
        .filter(|(i, t)| *i == 0 || ticks[i - 1].0 != t.0)
        .map(|(_, t)| (t.0.clone(), t.1))
        .collect();
    assert_eq!(
        firsts[1..],
        [("pictures".to_owned(), 60.0), ("render".to_owned(), 85.0)]
    );
    assert_eq!(*ticks.last().unwrap(), ("render".to_owned(), 100.0, None));
    // Still throttled within a stage: no two lines say the same thing.
    let said: std::collections::BTreeSet<String> = ticks.iter().map(|t| format!("{t:?}")).collect();
    assert_eq!(said.len(), ticks.len());
}

#[test]
fn progress_is_opt_in_so_no_progress_means_no_stderr() {
    let sb = Sandbox::new();
    let ran = sb.run(&[
        &pdf(),
        "-o",
        &sb.p("noprog.epub"),
        "--no-fountain",
        "--json",
    ]);
    assert_eq!(ran.code, 0);
    assert_eq!(ran.stderr, "");
}

// --- the default conversion path is unchanged by verb dispatch ---

#[test]
fn a_pdf_whose_stem_is_a_verb_converts_exactly_as_any_other_would() {
    let sb = Sandbox::new();
    for name in ["send.pdf", "devices.pdf"] {
        let input = sb.copy_fixture("screenplay.pdf", name);
        let out = sb.p(&format!("{name}.epub"));
        let ran = sb.run(&[&s(&input), "-o", &out, "--no-fountain", "--json"]);
        assert_eq!(ran.code, 0, "{name}");
        let r = ran.json();
        assert_eq!(r["ok"], true);
        assert_eq!(r["epubPath"], out.as_str());
        assert!(r["pages"].as_u64().unwrap() > 0);
    }
}

// --- --options-json ---

/// Convert with `--options-json payload`, writing the preview; the preview.
fn preview_with(payload: &str) -> String {
    let sb = Sandbox::new();
    let preview = sb.p("p.html");
    let ran = sb.run(&[
        &pdf(),
        "--json",
        "--preview-html",
        &preview,
        "-o",
        &sb.p("p.epub"),
        "--no-fountain",
        "--options-json",
        payload,
    ]);
    assert_eq!(ran.json()["ok"], true, "{}", ran.stdout);
    std::fs::read_to_string(preview).expect("the preview")
}

#[test]
fn options_json_applies_a_knob_passed_as_an_argv_string() {
    let html = preview_with("{\"dialogueSideMarginPct\":7}");
    // Anchored on the declaration: the title page has its own 20% margin.
    assert!(html.contains("margin-left: 7%"));
    assert!(!html.contains("margin-left: 20%"));
}

#[test]
fn options_json_clamps_out_of_range_values_instead_of_trusting_them() {
    let html = preview_with("{\"dialogueSideMarginPct\":999}");
    assert!(html.contains("margin-left: 30%"));
    assert!(!html.contains("margin-left: 999%"));
}

#[test]
fn options_json_rejects_a_non_object_payload_with_bad_options() {
    let ran = Sandbox::new().run(&[&pdf(), "--json", "--options-json", "[1,2]"]);
    assert_eq!(ran.error_code(), "bad-options");
    assert!(ran.error_message().contains("--options-json"));
}

#[test]
fn options_json_rejects_malformed_json_with_bad_options_and_does_not_leak_the_payload() {
    let ran = Sandbox::new().run(&[&pdf(), "--json", "--options-json", "{oops"]);
    assert_eq!(ran.error_code(), "bad-options");
    assert!(!ran.error_message().contains("oops"));
}

#[test]
fn options_and_options_json_together_are_refused_rather_than_picking_one() {
    let ran = Sandbox::new().run(&[
        &pdf(),
        "--json",
        "--options",
        "x.json",
        "--options-json",
        "{}",
    ]);
    assert_eq!(ran.error_code(), "bad-options");
    assert!(ran.error_message().contains("not both"));
}

#[test]
fn options_json_ignores_an_unknown_key_without_failing_the_conversion() {
    let html = preview_with("{\"notARealKnob\":123,\"dialogueSideMarginPct\":12}");
    assert!(html.contains("margin-left: 12%"));
}

#[test]
fn options_json_falls_back_to_the_default_for_a_wrong_typed_value_instead_of_crashing() {
    let html = preview_with("{\"dialogueSideMarginPct\":\"wide\"}");
    assert!(!html.contains("wide%"));
    assert!(html.contains("margin-left: 20%"));
}

// --- --preview-inline ---

#[test]
fn preview_inline_puts_the_same_document_in_the_json_that_preview_html_writes_to_disk() {
    let sb = Sandbox::new();
    let preview = sb.p("parity.html");
    let ran = sb.run(&[
        &pdf(),
        "--json",
        "--preview-inline",
        "--preview-html",
        &preview,
        "-o",
        &sb.p("parity.epub"),
        "--no-fountain",
    ]);
    let answer = ran.json();
    assert_eq!(answer["ok"], true);
    let on_disk = std::fs::read_to_string(preview).unwrap();
    assert_eq!(answer["previewHtml"].as_str().unwrap(), on_disk);
}

fn inline_answer() -> common::Ran {
    let sb = Sandbox::new();
    sb.run(&[
        &pdf(),
        "--json",
        "--preview-inline",
        "-o",
        &sb.p("inline.epub"),
        "--no-fountain",
    ])
}

#[test]
fn the_inlined_preview_carries_the_stylesheet_not_a_link_to_one() {
    let answer = inline_answer().json();
    let html = answer["previewHtml"].as_str().unwrap();
    assert!(html.contains("<style>"));
    assert!(!html.contains("<link rel=\"stylesheet\""));
    assert!(html.contains("h2.scene-heading"));
}

#[test]
fn preview_inline_leaves_stdout_exactly_one_json_object() {
    // json() asserts one line that parses.
    inline_answer().json();
}

#[test]
fn the_preview_html_key_is_absent_unless_asked_for() {
    let sb = Sandbox::new();
    let ran = sb.run(&[&pdf(), "--json", "-o", &sb.p("a.epub"), "--no-fountain"]);
    assert!(ran.json().get("previewHtml").is_none());
}

#[test]
fn preview_inline_is_a_usage_error_without_json() {
    let ran = Sandbox::new().run(&[&pdf(), "--preview-inline"]);
    assert_eq!(ran.code, 1);
    assert!(ran.stderr.contains("--preview-inline"), "{}", ran.stderr);
}

#[test]
fn with_no_preview_html_the_inlined_document_is_still_the_real_preview() {
    let answer = inline_answer().json();
    assert_eq!(answer["ok"], true);
    let html = answer["previewHtml"].as_str().expect("a string");
    assert!(html.len() > 1000);
    assert!(html.contains("<p class=\"character\">MARGO</p>"));
    assert!(answer.get("previewHtmlPath").is_none());
}

// --- a large answer survives a pipe ---

/// An invented screenplay, big enough to need several pipe buffers.
fn generated(scenes: usize) -> String {
    let mut out = String::from("Title: Generated Load Sample\nAuthor: Test Harness\n\n");
    for i in 0..scenes {
        out.push_str(&format!("INT. TEST ROOM {i} - DAY\n\n"));
        out.push_str(&format!(
            "A plain room with a numbered door. Nothing in it matters except its\n\
             length, which is the whole point of sample number {i}.\n\n"
        ));
        out.push_str(&format!(
            "ALPHA\nThis is line {i} of a script that exists only to be big.\n\n"
        ));
        out.push_str(&format!(
            "BETA\n(flatly)\nUnderstood. That was line {i}. There will be more.\n\n"
        ));
    }
    out
}

#[test]
fn a_large_answer_survives_a_pipe_every_byte_of_it_every_time() {
    // Eight conversions at once, contending for the machine, each reading
    // its pipe at full speed: the shape that caught the TypeScript's lost
    // tail. Each has its own output path.
    const ATTEMPTS: usize = 8;
    let sb = Sandbox::new();
    let script = s(&sb.write("pipe-load.fountain", generated(1000)));
    let sb = &sb;
    let answers: Vec<String> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..ATTEMPTS)
            .map(|i| {
                let script = script.clone();
                scope.spawn(move || {
                    let out = sb.p(&format!("pipe-load-{i}.epub"));
                    sb.run(&[
                        &script,
                        "--json",
                        "--preview-inline",
                        "--no-fountain",
                        "-o",
                        &out,
                    ])
                    .stdout
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut lengths = Vec::new();
    for (i, out) in answers.iter().enumerate() {
        let out = out.trim();
        lengths.push(out.len());
        let parses = serde_json::from_str::<Value>(out).is_ok_and(|v| v["ok"] == true);
        assert_eq!(
            format!("attempt {i} parses: {parses}"),
            format!("attempt {i} parses: true")
        );
    }
    assert!(lengths.iter().all(|l| *l == lengths[0]), "{lengths:?}");
    // An answer that fits in one 64 KiB pipe buffer could not show the defect.
    assert!(lengths[0] > 4 * 65536, "{}", lengths[0]);
}
