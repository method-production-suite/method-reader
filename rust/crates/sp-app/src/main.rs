//! The Screepub window, on the real engine (src/direct.rs): sp-engine and
//! sp-device linked in, every call the same `sp_cli::run` the `screepub`
//! binary is. With `--version` or `--smoke` it runs headless instead
//! (src/headless.rs).
//!
//! On Windows a release build is a GUI-subsystem program, so no console
//! opens behind the window (as the Tauri shell's main.rs declared). The
//! headless answers still reach a caller that pipes stdout, which is how
//! smoke-bundle reads them.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sp_app::app::{App, Host, Window};
use sp_app::direct::{DirectEngine, wait_for_calls};
use sp_app::runtime::Mode;

/// How long a window closed mid-call waits for the engine to finish its
/// writes before the process ends (the sidecar had no limit; a conversion
/// that has not finished in ten minutes is not going to).
const FINISH_CAP: Duration = Duration::from_secs(600);

fn reduce_motion_flag() -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    let writer = flag.clone();
    // Reduce Motion as the OS reports it, kept current while the window runs.
    std::mem::forget(mundy::Preferences::subscribe(
        mundy::Interest::ReducedMotion,
        move |p| {
            writer.store(
                p.reduced_motion == mundy::ReducedMotion::Reduce,
                Ordering::Relaxed,
            );
        },
    ));
    flag
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(what) = sp_app::headless::parse(&args) {
        return ExitCode::from(sp_app::headless::run(what));
    }
    sp_cli::recover_camera_import();
    // An update swap that died between its renames left the bundle hidden
    // beside its own name, or left a backup behind: put it back, or clear
    // it (sp_update::install::sweep_at_startup; nothing outside a bundle).
    if let Ok(exe) = std::env::current_exe() {
        let swept = sp_update::install::sweep_at_startup(&exe);
        if let Some(back) = &swept.restored {
            eprintln!(
                "screepub-window: put {} back after an interrupted update",
                back.display()
            );
        }
        for gone in &swept.removed {
            eprintln!(
                "screepub-window: removed {}, left by an earlier update",
                gone.display()
            );
        }
    }
    let engine = Arc::new(DirectEngine::new());
    let in_flight = engine.in_flight();
    let options = sp_app::app::native_options();
    let result = eframe::run_native(
        "Screepub",
        options,
        Box::new(move |cc| {
            let host = Host::native(reduce_motion_flag());
            let mut app = App::new(engine, Mode::Threads, host);
            app.set_repaint(cc.egui_ctx.clone());
            #[cfg(target_os = "macos")]
            {
                app.menus = sp_app::app::menu::Menus::install();
            }
            Ok(Box::new(Window(app)))
        }),
    );
    // The window is gone; a call it started still runs. Let it finish its
    // writes, as the sidecar did after the Tauri window quit (row W13).
    wait_for_calls(&in_flight, FINISH_CAP);
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("screepub-window: {e}");
            ExitCode::FAILURE
        }
    }
}
