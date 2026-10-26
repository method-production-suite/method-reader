//! The real engine behind the window: one conversion of the invented demo
//! script, in-process, exactly as the window asks for it (`Request::Convert`
//! is `<pdf> --json --progress --preview-inline --library`).
//!
//! The environment points at scratch folders: METHOD_READER_CONFIG_DIR
//! (never the real settings), METHOD_READER_LIBRARY (never the real
//! library), the same under their old SCREEPUB_ names the engine falls
//! back to, and a
//! Calibre guard path, so a Calibre started by mistake fails instead of
//! reaching the real one. The workspace forbids unsafe code, so the test
//! cannot set its own environment: it runs itself again as a child with
//! that environment, and the child does the work.

use serde_json::Value;
use sp_app::engine::{Engine, Request};

const CHILD: &str = "SP_APP_DIRECT_ENGINE_CHILD";

#[test]
fn a_conversion_streams_its_progress_and_answers_with_the_book() {
    let scratch = tempfile::tempdir().expect("scratch");
    let config = scratch.path().join("config");
    let library = scratch.path().join("library");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::create_dir_all(&library).unwrap();
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_converts", "--nocapture"])
        .env(CHILD, "1")
        .env("METHOD_READER_CONFIG_DIR", &config)
        .env("METHOD_READER_LIBRARY", &library)
        .env("SCREEPUB_CONFIG_DIR", &config)
        .env("SCREEPUB_LIBRARY", &library)
        .env(
            "CALIBRE_CONFIG_DIRECTORY",
            "/dev/null/method-reader-test-guard",
        )
        .output()
        .expect("the child runs");
    let said =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{said}");
    assert!(said.contains("1 passed"), "the child really ran: {said}");
}

#[test]
fn child_converts() {
    if std::env::var(CHILD).is_err() {
        return;
    }
    let library = std::env::var("METHOD_READER_LIBRARY").unwrap();
    let pdf = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tests/fixtures/field-station.pdf")
        .canonicalize()
        .expect("the committed fixture");
    let engine = sp_app::direct::DirectEngine::new();
    let mut lines: Vec<Value> = vec![];
    let reply = engine.run(
        &Request::Convert {
            path: pdf.to_string_lossy().into_owned(),
            force: false,
            options: None,
        },
        &mut |line| lines.push(line),
    );
    let answer = reply.answer.expect("an answer");
    assert_eq!(answer["ok"], true, "{answer}");
    assert_eq!(answer["title"], "Field Station");
    let epub = answer["epubPath"].as_str().unwrap();
    assert!(epub.starts_with(&library), "{epub}");
    assert!(std::path::Path::new(epub).exists());
    assert!(
        answer["previewHtml"]
            .as_str()
            .unwrap()
            .contains("scene-heading"),
        "the book's pages ride in the answer"
    );

    // The page reached, and the first page for the page show.
    let first = lines
        .iter()
        .find(|l| l["stage"] == "page")
        .expect("the first page");
    assert!(first["page"].as_u64().is_some_and(|p| p >= 1), "{first}");
    assert!(!first["paragraphs"].as_array().unwrap().is_empty());
    assert!(
        lines
            .iter()
            .any(|l| l["stage"] == "parse" && l["page"].is_number()),
        "{lines:?}"
    );
    assert!(lines.iter().any(|l| l["stage"] == "render"));
    assert_eq!(
        engine.in_flight().load(std::sync::atomic::Ordering::SeqCst),
        0
    );

    // The window's Convert is a script opened: the engine put the PDF the
    // person chose (not the library copy) on the suite launcher's list.
    let config = std::env::var("METHOD_READER_CONFIG_DIR").unwrap();
    let recent: Value = serde_json::from_str(
        &std::fs::read_to_string(std::path::Path::new(&config).join("recent.json"))
            .expect("a recent.json in the data folder"),
    )
    .unwrap();
    assert_eq!(recent.as_array().map(Vec::len), Some(1), "{recent}");
    assert_eq!(recent[0]["path"], pdf.to_string_lossy().as_ref());
}

const SAMPLE_CHILD: &str = "SP_APP_DIRECT_ENGINE_SAMPLE_CHILD";

/// The sample, as the window converts it (src/app/sample.rs): the built-in
/// PDF into a folder of its own with `-o`, then remade from its
/// `.fountain`, and the library left empty.
#[test]
fn the_sample_converts_into_its_own_folder_and_reopens_from_its_fountain() {
    let scratch = tempfile::tempdir().expect("scratch");
    let config = scratch.path().join("config");
    let library = scratch.path().join("library");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::create_dir_all(&library).unwrap();
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_converts_the_sample", "--nocapture"])
        .env(SAMPLE_CHILD, "1")
        .env("METHOD_READER_CONFIG_DIR", &config)
        .env("METHOD_READER_LIBRARY", &library)
        .env("SCREEPUB_CONFIG_DIR", &config)
        .env("SCREEPUB_LIBRARY", &library)
        .env(
            "CALIBRE_CONFIG_DIRECTORY",
            "/dev/null/method-reader-test-guard",
        )
        .output()
        .expect("the child runs");
    let said =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{said}");
    assert!(said.contains("1 passed"), "the child really ran: {said}");
    assert_eq!(
        std::fs::read_dir(&library).unwrap().count(),
        0,
        "nothing in the library"
    );
}

#[test]
fn child_converts_the_sample() {
    if std::env::var(SAMPLE_CHILD).is_err() {
        return;
    }
    use sp_app::model::sample::{FOLDER, PDF, Paths};
    let config = std::env::var("METHOD_READER_CONFIG_DIR").unwrap();
    let p = Paths::in_folder(&std::path::Path::new(&config).join(FOLDER));
    std::fs::create_dir_all(&p.dir).unwrap();
    std::fs::write(&p.pdf, PDF).unwrap();
    let engine = sp_app::direct::DirectEngine::new();
    let mut lines: Vec<Value> = vec![];
    let answer = engine
        .run(
            &Request::ConvertTo {
                path: p.pdf.clone(),
                out: p.epub.clone(),
            },
            &mut |line| lines.push(line),
        )
        .answer
        .expect("an answer");
    assert_eq!(answer["ok"], true, "{answer}");
    assert_eq!(answer["title"], "Field Station");
    assert_eq!(answer["epubPath"], p.epub.as_str());
    assert_eq!(answer["fountainPath"], p.fountain.as_str());
    assert!(answer["pages"].as_u64().is_some());
    assert!(
        answer["previewHtml"]
            .as_str()
            .unwrap()
            .contains("scene-heading")
    );
    assert!(lines.iter().any(|l| l["stage"] == "render"));
    assert!(std::path::Path::new(&p.fountain).is_file());

    let again = engine
        .run(
            &Request::ConvertTo {
                path: p.fountain.clone(),
                out: p.epub.clone(),
            },
            &mut |_| {},
        )
        .answer
        .expect("an answer");
    assert_eq!(again["ok"], true, "{again}");
    assert_eq!(again["epubPath"], p.epub.as_str());
    assert_eq!(again["previewHtml"], answer["previewHtml"], "the same book");
}

const SHELF_CHILD: &str = "SP_APP_DIRECT_ENGINE_SHELF_CHILD";

/// The device library as the window asks for it (src/app/device_library.rs),
/// on an invented Kindle in a scratch folder: the engine's real answers
/// read back by the window's own model, and only the one book removed. The
/// reader list is METHOD_READER_VOLUME_ROOTS pointed at the scratch
/// volumes, so no real reader is ever looked at, and the reMarkable
/// address is one nothing listens on.
#[test]
fn the_device_library_lists_a_readers_books_and_removes_one() {
    let scratch = tempfile::tempdir().expect("scratch");
    let root = scratch.path().canonicalize().unwrap();
    let config = root.join("config");
    let library = root.join("library");
    let kindle = root.join("vols").join("Kindle");
    for d in [
        config.clone(),
        library.join("Field Station"),
        kindle.join("documents").join("Field Station.sdr"),
        kindle.join("system"),
    ] {
        std::fs::create_dir_all(d).unwrap();
    }
    let ours = b"the library's own bytes";
    std::fs::write(library.join("Field Station/Field Station.azw3"), ours).unwrap();
    std::fs::write(kindle.join("documents/Field Station.azw3"), ours).unwrap();
    std::fs::write(
        kindle.join("documents/Field Station.sdr/Field Station.apnx"),
        b"notes",
    )
    .unwrap();
    std::fs::write(kindle.join("documents/Reading list.pdf"), b"not ours").unwrap();
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_reads_the_shelf", "--nocapture"])
        .env(SHELF_CHILD, kindle.to_string_lossy().as_ref())
        .env("METHOD_READER_CONFIG_DIR", &config)
        .env("METHOD_READER_LIBRARY", &library)
        .env("SCREEPUB_CONFIG_DIR", &config)
        .env("SCREEPUB_LIBRARY", &library)
        .env("METHOD_READER_VOLUME_ROOTS", root.join("vols"))
        .env("METHOD_READER_REMARKABLE_ENDPOINT", "http://127.0.0.1:9")
        .env(
            "CALIBRE_CONFIG_DIRECTORY",
            "/dev/null/method-reader-test-guard",
        )
        .output()
        .expect("the child runs");
    let said =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{said}");
    assert!(said.contains("1 passed"), "the child really ran: {said}");
    assert!(!kindle.join("documents/Field Station.azw3").exists());
    assert!(!kindle.join("documents/Field Station.sdr").exists());
    assert!(kindle.join("documents/Reading list.pdf").exists());
    assert!(
        library.join("Field Station/Field Station.azw3").exists(),
        "the library's copy is never touched"
    );
}

#[test]
fn child_reads_the_shelf() {
    let Ok(volume) = std::env::var(SHELF_CHILD) else {
        return;
    };
    use sp_app::model::device_library::{removed_line, shelf_from};
    use sp_app::model::send_routes::Device;
    let engine = sp_app::direct::DirectEngine::new();
    let answer = engine
        .run(
            &Request::DeviceBooks {
                volume: volume.clone(),
            },
            &mut |_| {},
        )
        .answer
        .expect("an answer");
    let shelf = shelf_from(&answer).unwrap_or_else(|| panic!("a list the window reads: {answer}"));
    assert_eq!(shelf.kind, "kindle");
    let (ours, others) = shelf.groups();
    let names = |l: &[&sp_app::model::device_library::Book]| {
        l.iter().map(|b| b.name.clone()).collect::<Vec<_>>()
    };
    assert_eq!(names(&ours), ["Field Station.azw3"]);
    assert_eq!(ours[0].library.as_deref(), Some("Field Station"));
    assert_eq!(names(&others), ["Reading list.pdf"]);

    let book = ours[0].clone();
    let device = Device {
        id: "k".into(),
        kind: "kindle".into(),
        name: "Kindle".into(),
        volume: Some(volume.clone()),
        transport: None,
        copy_note: None,
    };
    let answer = engine
        .run(
            &Request::DeviceRemove {
                volume: volume.clone(),
                file: book.path.clone(),
            },
            &mut |_| {},
        )
        .answer
        .expect("an answer");
    assert_eq!(
        removed_line(&answer, &book, &device),
        (
            "Removed Field Station.azw3 and its notes from your Kindle.".to_string(),
            false
        ),
        "{answer}"
    );
    // A file outside the books folder is refused in the engine's words.
    let answer = engine
        .run(
            &Request::DeviceRemove {
                volume: volume.clone(),
                file: format!("{volume}/system"),
            },
            &mut |_| {},
        )
        .answer
        .expect("an answer");
    assert_eq!(answer["error"]["code"], "not-a-book", "{answer}");
    assert!(removed_line(&answer, &book, &device).1);
}
