//! The Screepub window. Until sp-engine exists it runs on the fake engine
//! (src/engine.rs), which answers every call from fixtures, so every screen
//! can be opened and tried.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use sp_app::app::{App, Host, Window};
use sp_app::engine::FakeEngine;
use sp_app::runtime::Mode;

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

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Screepub")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([640.0, 480.0])
            .with_fullsize_content_view(true)
            .with_title_shown(false)
            .with_titlebar_shown(false),
        ..Default::default()
    };
    eframe::run_native(
        "Screepub",
        options,
        Box::new(|cc| {
            let host = Host::native(reduce_motion_flag());
            let mut app = App::new(Arc::new(FakeEngine::new()), Mode::Threads, host);
            app.set_repaint(cc.egui_ctx.clone());
            Ok(Box::new(Window(app)))
        }),
    )
}
