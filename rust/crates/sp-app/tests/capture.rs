//! The README pictures' window (src/capture.rs, examples/capture.rs): its
//! orders and what it refuses, the line its guard holds around the engine,
//! the picture mode that hides the version stamp and the library slot, and
//! each shot reached on the fake engine. One shot is also drawn for real,
//! offscreen through wgpu, at the picture's own pixel size; on a machine
//! where wgpu has no adapter that test says so and skips.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use sp_app::app::bookview::Place;
use sp_app::app::{App, convert, panel};
use sp_app::capture::{
    Args, Guard, PIXELS_PER_POINT, Shot, Stepper, Theme, Verdict, capture_host, inside, parse_args,
    reach, verdict,
};
use sp_app::engine::{Engine, FakeEngine, Request};
use sp_app::model::frame::Surface;
use sp_app::runtime::Mode;

fn demo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tests/fixtures/field-station.pdf")
        .canonicalize()
        .expect("the committed demo script")
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_string()).collect()
}

fn scratch_env(k: &str) -> Option<String> {
    match k {
        "METHOD_READER_CONFIG_DIR" => Some("/scratch/config".into()),
        "METHOD_READER_LIBRARY" => Some("/scratch/library".into()),
        _ => None,
    }
}

fn orders(demo: &Path) -> Vec<String> {
    strings(&[
        "--shot",
        "result",
        "--theme",
        "dark",
        "--width",
        "1280",
        "--height",
        "800",
        "--demo",
        &demo.to_string_lossy(),
        "--out",
        "/scratch/result-dark.rgba",
    ])
}

// ------------------------------------------------------------------ orders

#[test]
fn the_panel_half_is_optional_and_named() {
    use sp_app::model::intent::Intent;
    let d = demo();
    let mut o = orders(&d);
    o.extend(strings(&["--panel", "edit"]));
    assert_eq!(parse_args(&o, &scratch_env).unwrap().panel, Intent::Edit);
    let mut bad = orders(&d);
    bad.extend(strings(&["--panel", "write"]));
    assert_eq!(
        parse_args(&bad, &scratch_env).unwrap_err(),
        "capture: no panel named write"
    );
}

#[test]
fn its_orders_are_one_shot_in_one_theme_at_one_size() {
    let d = demo();
    let a = parse_args(&orders(&d), &scratch_env).unwrap();
    assert_eq!(
        a,
        Args {
            shot: Shot::Result,
            theme: Theme::Dark,
            width: 1280,
            height: 800,
            demo: d,
            out: PathBuf::from("/scratch/result-dark.rgba"),
            library: PathBuf::from("/scratch/library"),
            panel: sp_app::model::intent::Intent::Read,
        }
    );
}

#[test]
fn it_refuses_to_start_without_a_scratch_settings_folder_and_library() {
    let d = demo();
    for missing in ["METHOD_READER_CONFIG_DIR", "METHOD_READER_LIBRARY"] {
        let env = |k: &str| (k != missing).then(|| scratch_env(k)).flatten();
        let e = parse_args(&orders(&d), &env).unwrap_err();
        assert!(e.contains(missing), "{e}");
        let blank = |k: &str| {
            if k == missing {
                Some("  ".into())
            } else {
                scratch_env(k)
            }
        };
        let e = parse_args(&orders(&d), &blank).unwrap_err();
        assert!(e.contains(missing), "{e}");
    }
}

#[test]
fn the_old_names_still_point_it_at_scratch_folders() {
    // The engine reads SCREEPUB_CONFIG_DIR and SCREEPUB_LIBRARY when the new
    // names are unset, so the guard accepts them the same way.
    let d = demo();
    let old = |k: &str| match k {
        "SCREEPUB_CONFIG_DIR" => Some("/scratch/config".into()),
        "SCREEPUB_LIBRARY" => Some("/old/library".into()),
        _ => None,
    };
    assert_eq!(
        parse_args(&orders(&d), &old).unwrap().library,
        PathBuf::from("/old/library")
    );
    // The new name wins, as it does in the engine.
    let both = |k: &str| match k {
        "METHOD_READER_LIBRARY" => Some("/scratch/library".into()),
        _ => old(k),
    };
    assert_eq!(
        parse_args(&orders(&d), &both).unwrap().library,
        PathBuf::from("/scratch/library")
    );
    // A blank new name is refused, never passed over for the old one.
    let blank_new = |k: &str| match k {
        "METHOD_READER_LIBRARY" => Some(" ".into()),
        _ => old(k),
    };
    let e = parse_args(&orders(&d), &blank_new).unwrap_err();
    assert!(e.contains("METHOD_READER_LIBRARY"), "{e}");
}

#[test]
fn it_refuses_orders_it_does_not_understand() {
    let d = demo();
    let with = |i: usize, v: &str| {
        let mut o = orders(&d);
        o[i] = v.to_string();
        parse_args(&o, &scratch_env).unwrap_err()
    };
    assert!(with(1, "hero").contains("no shot named hero"));
    assert!(with(3, "sepia").contains("no theme named sepia"));
    assert!(with(5, "wide").contains("--width wide"));
    assert!(with(7, "0").contains("--height 0"));
    assert!(with(9, "tests/fixtures/field-station.pdf").contains("whole path"));
    assert!(with(9, "/nowhere/field-station.pdf").contains("whole path"));
    assert!(with(0, "--colour").contains("unknown option '--colour'"));
    assert!(with(0, "shot").contains("unexpected argument"));
    let mut twice = orders(&d);
    twice.extend(strings(&["--theme", "light"]));
    assert!(
        parse_args(&twice, &scratch_env)
            .unwrap_err()
            .contains("twice")
    );
    let mut short = orders(&d);
    short.truncate(10);
    assert!(
        parse_args(&short, &scratch_env)
            .unwrap_err()
            .contains("--out is required")
    );
    let mut dangling = orders(&d);
    dangling.pop();
    assert!(
        parse_args(&dangling, &scratch_env)
            .unwrap_err()
            .contains("--out needs a value")
    );
}

// ------------------------------------------------------------------ the line

/// A whole path on this system: a Windows one needs its drive.
#[cfg(not(windows))]
const LIB: &str = "/scratch/library";
#[cfg(windows)]
const LIB: &str = "C:\\scratch\\library";

fn ask(r: Request) -> Verdict {
    verdict(&r, &demo(), Path::new(LIB))
}

fn book(rel: &str) -> String {
    format!("{LIB}/Field Station/{rel}")
}

#[test]
fn it_runs_the_reads_and_the_demo_scripts_conversion() {
    for r in [
        Request::Version,
        Request::AppSettings,
        Request::Devices { quick: true },
        Request::Devices { quick: false },
        Request::Convert {
            path: demo().to_string_lossy().into_owned(),
            force: false,
            options: None,
        },
        Request::Convert {
            path: demo().to_string_lossy().into_owned(),
            force: true,
            options: None,
        },
        Request::Settings {
            fountain: book("Field Station.fountain"),
            set: None,
        },
    ] {
        assert_eq!(ask(r.clone()), Verdict::Run, "{r:?}");
    }
}

#[test]
fn it_keeps_calibre_out_as_a_machine_without_it_would() {
    assert_eq!(
        ask(Request::KindleCheck {
            epub: book("Field Station.epub")
        }),
        Verdict::KeepOut
    );
    assert_eq!(ask(Request::KfxStatus), Verdict::KeepOut);
}

#[test]
fn it_refuses_everything_that_copies_sends_builds_ejects_exports_or_installs() {
    let epub = book("Field Station.epub");
    for r in [
        Request::Send {
            file: epub.clone(),
            device: None,
        },
        Request::Eject {
            volume: "/Volumes/Kindle".into(),
        },
        Request::Export {
            epub: epub.clone(),
            for_format: "kindle".into(),
            fountain: None,
            options: None,
            out: None,
        },
        Request::Export {
            epub: epub.clone(),
            for_format: "epub".into(),
            fountain: None,
            options: None,
            out: Some(book("copy.epub")),
        },
        Request::Route {
            key: "save-epub".into(),
            epub: epub.clone(),
            out: None,
            fountain: None,
            options: None,
        },
        Request::Routes {
            epub: epub.clone(),
            quick: true,
        },
        Request::KfxInstall,
        Request::Reveal { path: epub.clone() },
        Request::EmailSetup,
        Request::SetAppSettings {
            json: r#"{"readerDevice":"kindle"}"#.into(),
        },
        Request::Reconvert {
            fountain: book("Field Station.fountain"),
            epub: epub.clone(),
            options: "{}".into(),
        },
        Request::Settings {
            fountain: book("Field Station.fountain"),
            set: Some(r#"{"x":1}"#.into()),
        },
    ] {
        assert_eq!(ask(r.clone()), Verdict::Refuse, "{r:?}");
    }
}

#[test]
fn it_converts_the_demo_script_and_no_other_file() {
    let other = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../tests/fixtures/screenplay.pdf")
        .canonicalize()
        .unwrap();
    for path in [
        other.to_string_lossy().into_owned(),
        "tests/fixtures/field-station.pdf".into(),
        String::new(),
    ] {
        let r = Request::Convert {
            path: path.clone(),
            force: false,
            options: None,
        };
        assert_eq!(ask(r), Verdict::Refuse, "{path}");
    }
    // The demo with options it was never given is not the window's call.
    let r = Request::Convert {
        path: demo().to_string_lossy().into_owned(),
        force: false,
        options: Some("{}".into()),
    };
    assert_eq!(ask(r), Verdict::Refuse);
}

#[test]
fn it_reads_books_only_from_inside_the_scratch_library() {
    let lib = Path::new(LIB);
    assert!(inside(&book("x.fountain"), lib));
    assert!(!inside(LIB, lib), "the library itself is not a book");
    assert!(!inside(&format!("{LIB}/../real/x.fountain"), lib));
    assert!(!inside(&format!("{LIB}-old/x.fountain"), lib));
    assert!(!inside("scratch/library/x.fountain", lib));
    let r = Request::Settings {
        fountain: "/Users/someone/Screepub/x.fountain".into(),
        set: None,
    };
    assert_eq!(ask(r), Verdict::Refuse);
}

/// Windows compares a drive written plainly or verbatim (what
/// canonicalize answers), either separator, and names in any case, as
/// the same place; a folder with no drive is not a whole path.
#[cfg(windows)]
#[test]
fn on_windows_a_book_is_inside_however_its_path_is_spelled() {
    let lib = Path::new(LIB);
    assert!(inside(r"c:\SCRATCH\Library\Field Station\x.fountain", lib));
    assert!(inside(r"\\?\C:\scratch\library\x.fountain", lib));
    assert!(inside("C:/scratch/library/x.fountain", lib));
    assert!(inside(
        r"C:\scratch\library\x.fountain",
        Path::new(r"\\?\C:\scratch\library")
    ));
    assert!(!inside(r"D:\scratch\library\x.fountain", lib));
    assert!(!inside(r"\scratch\library\x.fountain", lib));
    assert!(!inside(r"C:\scratch\library\..\..\x.fountain", lib));
    assert!(
        !inside(r"\\?\C:\scratch\library", lib),
        "the library itself"
    );
}

#[test]
fn a_refused_call_never_reaches_the_engine_and_is_written_down() {
    let fake = Arc::new(FakeEngine::new());
    let guard = Guard::new(fake.clone(), demo(), PathBuf::from(LIB));
    let refused = guard.refused();
    let send = Request::Send {
        file: book("Field Station.epub"),
        device: None,
    };
    let said = guard.run(&send, &mut |_| {}).answer.unwrap();
    assert_eq!(said["ok"], false);
    let kept = guard.run(&Request::KfxStatus, &mut |_| {}).answer.unwrap();
    assert_eq!(kept["ok"], false);
    let version = guard.run(&Request::Version, &mut |_| {}).answer.unwrap();
    assert_eq!(version["ok"], true);
    assert_eq!(fake.calls(), [Request::Version]);
    assert_eq!(
        *refused.lock().unwrap(),
        [send.argv().join(" ")],
        "only the refusal fails the picture; Calibre kept out does not"
    );
}

// ------------------------------------------------------------------ the window

struct Window(Harness<'static, App>);

impl Stepper for Window {
    fn step(&mut self) {
        self.0.step();
    }
    fn app(&self) -> &App {
        self.0.state()
    }
    fn app_mut(&mut self) -> &mut App {
        self.0.state_mut()
    }
}

/// The capture's window on the fake engine, behind the guard, with the
/// fake's library as the scratch one.
fn window(theme: Theme, ppp: f32) -> (Window, Arc<std::sync::Mutex<Vec<String>>>) {
    let guard = Guard::new(
        Arc::new(FakeEngine::new()),
        demo(),
        PathBuf::from("/library"),
    );
    let refused = guard.refused();
    let app = App::new(Arc::new(guard), Mode::Threads, capture_host());
    let mut h = Harness::builder()
        .with_size([1280.0, 800.0])
        .with_pixels_per_point(ppp)
        .with_theme(theme.egui())
        .build_ui_state(|ui, app: &mut App| app.frame(ui), app);
    h.input_mut().system_theme = Some(theme.egui());
    (Window(h), refused)
}

fn has(h: &Harness<'static, App>, words: &str) -> bool {
    h.query_all_by_value(words).next().is_some() || h.query_all_by_label(words).next().is_some()
}

const LIMIT: Duration = Duration::from_secs(60);

#[test]
fn the_picture_hides_the_stamp_and_the_library_slot_and_nothing_else() {
    let stamp = format!("rev {}", env!("CARGO_PKG_VERSION"));
    let (mut w, _) = window(Theme::Light, 1.0);
    for _ in 0..8 {
        w.step();
    }
    // The shipped window draws both.
    assert!(has(&w.0, &stamp));
    assert!(has(&w.0, "Change…"));
    let library = w
        .app()
        .convert
        .library
        .clone()
        .expect("the library probe answered");
    let line = sp_app::model::convert::library_line(&library);
    assert!(has(&w.0, &line), "{line}");
    reach(&mut w, Shot::Drop, &demo().to_string_lossy(), LIMIT).unwrap();
    assert!(w.app().picture_mode);
    assert!(!has(&w.0, &stamp), "the stamp is not drawn");
    assert!(!has(&w.0, "Change…"), "the library slot is not drawn");
    assert!(!has(&w.0, &line), "the library line is not drawn");
    // Nothing painted spells the stamp either.
    let painted =
        w.0.output()
            .shapes
            .iter()
            .any(|c| shape_says(&c.shape, "rev "));
    assert!(!painted, "the stamp is painted");
    // The well itself is still there.
    assert!(has(&w.0, "Needs selectable text, not a scan."));
    assert_eq!(w.app().convert.view, convert::View::Idle);
}

fn shape_says(shape: &egui::Shape, words: &str) -> bool {
    match shape {
        egui::Shape::Vec(list) => list.iter().any(|s| shape_says(s, words)),
        egui::Shape::Text(t) => t.galley.text().contains(words),
        _ => false,
    }
}

#[test]
fn the_result_shot_is_the_converted_demo_with_the_device_cards_and_its_first_page() {
    let (mut w, refused) = window(Theme::Light, 1.0);
    reach(&mut w, Shot::Result, &demo().to_string_lossy(), LIMIT).unwrap();
    let app = w.app();
    assert_eq!(app.convert.view, convert::View::Done);
    assert_eq!(app.panel.mode, Some(panel::Mode::Cards));
    assert!(app.views.peek(Place::FirstPage).is_some());
    assert_eq!(
        w.0.ctx.memory(|m| m.focused()),
        None,
        "a focus ring in the picture"
    );
    assert_eq!(app.surface, Surface::Convert);
    assert!(app.quiet());
    assert!(refused.lock().unwrap().is_empty(), "{:?}", refused.lock());
}

#[test]
fn the_read_shot_is_the_same_book_on_read_with_its_scene_index() {
    let (mut w, refused) = window(Theme::Dark, 1.0);
    reach(&mut w, Shot::Read, &demo().to_string_lossy(), LIMIT).unwrap();
    let app = w.app();
    assert_eq!(app.surface, Surface::Read);
    assert!(!app.read.marks.is_empty());
    assert!(app.views.peek(Place::Read).is_some_and(|v| v.dark()));
    assert!(app.dark, "dark whatever the OS says");
    assert!(refused.lock().unwrap().is_empty(), "{:?}", refused.lock());
}

#[test]
fn a_refused_conversion_fails_the_shot_saying_what_the_window_said() {
    let fake = Arc::new(FakeEngine::new());
    fake.always(
        "convert",
        sp_app::engine::Reply::ok(serde_json::json!({"ok": false, "error": {
            "code": "scanned", "message": "This PDF has no text."}})),
    );
    let guard = Guard::new(fake, demo(), PathBuf::from("/library"));
    let app = App::new(Arc::new(guard), Mode::Threads, capture_host());
    let h = Harness::builder()
        .with_size([1280.0, 800.0])
        .build_ui_state(|ui, app: &mut App| app.frame(ui), app);
    let mut w = Window(h);
    let e = reach(&mut w, Shot::Result, &demo().to_string_lossy(), LIMIT).unwrap_err();
    assert!(e.contains("refused the demo script"), "{e}");
    assert!(e.contains("scanned"), "{e}");
}

#[test]
fn a_shot_drawn_for_real_is_the_windows_size_at_two_pixels_a_point() {
    let mut drawn = Vec::new();
    for theme in [Theme::Light, Theme::Dark] {
        let (mut w, _) = window(theme, PIXELS_PER_POINT);
        reach(&mut w, Shot::Drop, &demo().to_string_lossy(), LIMIT).unwrap();
        let image = match w.0.render() {
            Ok(i) => i,
            Err(e) => {
                eprintln!("skipped: wgpu could not draw here: {e}");
                return;
            }
        };
        assert_eq!(image.dimensions(), (2560, 1600));
        drawn.push(image);
    }
    // Light and dark are forced, not taken from the OS: the paper differs.
    let light = |raw: &[u8]| {
        raw.chunks_exact(4)
            .map(|p| u64::from(p[0]) + u64::from(p[1]) + u64::from(p[2]))
            .sum::<u64>()
    };
    assert!(
        light(drawn[0].as_raw()) > 2 * light(drawn[1].as_raw()),
        "light is not lighter than dark"
    );
    // The same window drawn twice is the same picture, pixel for pixel.
    let (mut again, _) = window(Theme::Light, PIXELS_PER_POINT);
    reach(&mut again, Shot::Drop, &demo().to_string_lossy(), LIMIT).unwrap();
    let image = again.0.render().unwrap();
    assert!(
        image.as_raw() == drawn[0].as_raw(),
        "the drop well drew differently twice"
    );
}
