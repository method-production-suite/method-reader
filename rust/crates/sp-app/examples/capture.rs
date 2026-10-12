//! The real window, photographed: the real App on the linked engine (with
//! the real settings code) converts one PDF, and egui renders the Convert
//! result and Read to PNGs through wgpu, offscreen, as egui_kittest's
//! snapshots do. Offscreen because macOS paints nothing for a window it
//! counts as hidden, and a window started from a shell often is.
//!
//!     SCREEPUB_CONFIG_DIR=<scratch>/config SCREEPUB_LIBRARY=<scratch>/library \
//!       cargo run -p sp-app --example capture -- <file.pdf> <out-folder>
//!
//! It refuses to run unless both are set, so it never reaches the real
//! settings or the real library.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use sp_app::app::{App, Host, convert};
use sp_app::direct::DirectEngine;
use sp_app::engine::{Engine, Reply, Request};
use sp_app::model::frame::Surface;
use sp_app::runtime::Mode;

/// The real engine, less the three calls that start Calibre on a machine
/// that has it (the Kindle file check, the KFX probe and its install all
/// list Calibre's plugins): Calibre runs only inside tests that isolate its
/// config, never in a run like this one. Those calls answer a refusal.
struct NoCalibre(DirectEngine);

impl Engine for NoCalibre {
    fn run(&self, request: &Request, progress: &mut dyn FnMut(serde_json::Value)) -> Reply {
        let calibre = matches!(
            request,
            Request::KindleCheck { .. } | Request::KfxStatus | Request::KfxInstall
        ) || matches!(request, Request::Export { for_format, .. } if for_format == "kindle" || for_format == "kfx");
        if calibre {
            return Reply::ok(serde_json::json!({"ok": false, "error": {
                "code": "internal", "message": "Calibre is kept out of this capture run."}}));
        }
        self.0.run(request, progress)
    }
}

fn step_until(h: &mut Harness<'static, App>, what: &str, done: impl Fn(&App) -> bool) {
    let start = Instant::now();
    while !done(h.state()) {
        assert!(start.elapsed() < Duration::from_secs(60), "never: {what}");
        std::thread::sleep(Duration::from_millis(16));
        h.step();
    }
    // A second more, for anything still easing in.
    for _ in 0..60 {
        std::thread::sleep(Duration::from_millis(16));
        h.step();
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [pdf, out] = &args[..] else {
        eprintln!("usage: capture <file.pdf> <out-folder>");
        std::process::exit(2);
    };
    for var in ["SCREEPUB_CONFIG_DIR", "SCREEPUB_LIBRARY"] {
        if std::env::var_os(var).is_none_or(|v| v.is_empty()) {
            eprintln!("capture: set {var} to a scratch folder first");
            std::process::exit(2);
        }
    }
    let pdf = std::fs::canonicalize(pdf).expect("the PDF exists");
    let out = PathBuf::from(out);
    std::fs::create_dir_all(&out).expect("the out folder");

    let host = Host::native(Arc::new(AtomicBool::new(false)));
    let app = App::new(
        Arc::new(NoCalibre(DirectEngine::new())),
        Mode::Threads,
        host,
    );
    let mut h = Harness::builder()
        .with_size([1280.0, 800.0])
        .with_pixels_per_point(2.0)
        .build_ui_state(|ui, app: &mut App| app.frame(ui), app);
    h.step();
    convert::convert_path(h.state_mut(), &pdf.to_string_lossy(), false);
    step_until(&mut h, "the result", |app| {
        app.convert.view == convert::View::Done && app.show.is_none()
    });
    let shot = |h: &mut Harness<'static, App>, name: &str| {
        let image = h.render().expect("wgpu renders");
        let path = out.join(name);
        image.save(&path).expect("the PNG writes");
        println!("saved {}", path.display());
    };
    shot(&mut h, "convert-result.png");
    h.state_mut().go_to(Surface::Read);
    step_until(&mut h, "Read", |app| app.surface == Surface::Read);
    shot(&mut h, "read.png");
    println!("library book: {}", h.state().convert.book);
}
