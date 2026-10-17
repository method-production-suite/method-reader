//! One README picture of the real window: the real App on the linked
//! engine, behind src/capture.rs's guard, driven into one shot and drawn
//! offscreen by egui through wgpu, as egui_kittest's snapshots are.
//! Offscreen because macOS paints nothing for a window it counts as
//! hidden, and a window started from a shell often is.
//!
//! `cargo xtask capture-screens` runs it once per picture with the run's
//! own environment and frames what it draws; run by hand it is
//!
//!     METHOD_READER_CONFIG_DIR=<scratch>/config METHOD_READER_LIBRARY=<scratch>/library \
//!       cargo run -p sp-app --example capture -- --shot result --theme dark \
//!       --width 1280 --height 800 --demo <repo>/tests/fixtures/field-station.pdf \
//!       --out <scratch>/result-dark.rgba
//!
//! It refuses to run unless both folders are set, so it never reaches the
//! real settings or the real library. It writes the picture's raw RGBA
//! (width x height x 4 bytes at two pixels a point) and nothing else; the
//! xtask turns that into PNG bytes. Exit codes: 0 drawn, 1 failed (nothing
//! written), 2 refused its orders.

use std::sync::Arc;

use egui_kittest::Harness;
use sp_app::app::App;
use sp_app::capture::{
    Guard, PIXELS_PER_POINT, REACH_LIMIT, Stepper, USAGE, capture_host, parse_args, reach_with,
};
use sp_app::direct::DirectEngine;
use sp_app::runtime::Mode;

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

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let env = |k: &str| std::env::var(k).ok();
    let a = match parse_args(&args, &env) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            std::process::exit(2);
        }
    };
    if let Err(e) = draw(&a) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn draw(a: &sp_app::capture::Args) -> Result<(), String> {
    let guard = Guard::new(
        Arc::new(DirectEngine::new()),
        a.demo.clone(),
        a.library.clone(),
    );
    let refused = guard.refused();
    let app = App::new(Arc::new(guard), Mode::Threads, capture_host());
    let mut h = Harness::builder()
        .with_size([a.width as f32, a.height as f32])
        .with_pixels_per_point(PIXELS_PER_POINT)
        .with_theme(a.theme.egui())
        .build_ui_state(|ui, app: &mut App| app.frame(ui), app);
    // Light or dark whatever the OS says: the window follows the system
    // theme egui is told, and this one is told by the capture.
    h.input_mut().system_theme = Some(a.theme.egui());
    let mut w = Window(h);
    reach_with(
        &mut w,
        a.shot,
        a.panel,
        &a.demo.to_string_lossy(),
        REACH_LIMIT,
    )?;
    let refused = refused.lock().map(|r| r.clone()).unwrap_or_default();
    if !refused.is_empty() {
        return Err(format!(
            "capture: the window made engine calls the capture refuses:\n  {}",
            refused.join("\n  ")
        ));
    }
    let image = w.0.render().map_err(|e| format!("capture: wgpu: {e}"))?;
    let (pw, ph) = (
        (a.width as f32 * PIXELS_PER_POINT).round() as u32,
        (a.height as f32 * PIXELS_PER_POINT).round() as u32,
    );
    if image.dimensions() != (pw, ph) {
        return Err(format!(
            "capture: drew {} by {} pixels, not {pw} by {ph}",
            image.width(),
            image.height()
        ));
    }
    std::fs::write(&a.out, image.as_raw())
        .map_err(|e| format!("capture: could not write {}: {e}", a.out.display()))
}
