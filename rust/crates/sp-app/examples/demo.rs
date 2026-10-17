//! The window on the fake engine, with the demo script already converted
//! and showing the surface named by the first argument (convert, read or
//! tune): for looking at screens without a file dialog.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use sp_app::app::{App, Host, Window, convert};
use sp_app::engine::FakeEngine;
use sp_app::model::frame::Surface;
use sp_app::runtime::Mode;

fn main() -> eframe::Result {
    let surface = match std::env::args().nth(1).as_deref() {
        Some("read") => Surface::Read,
        Some("tune") => Surface::Tune,
        _ => Surface::Convert,
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Method Reader demo")
            .with_inner_size([1280.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Method Reader demo",
        options,
        Box::new(move |cc| {
            let host = Host::native(Arc::new(AtomicBool::new(true)));
            let mut app = App::new(Arc::new(FakeEngine::new()), Mode::Inline, host);
            app.set_repaint(cc.egui_ctx.clone());
            convert::convert_path(&mut app, "/demo/field-station.pdf", false);
            app.pump();
            app.go_to(surface);
            Ok(Box::new(Window(app)))
        }),
    )
}
