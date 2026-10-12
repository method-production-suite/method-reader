//! What every screen test starts from: the fake engine, the fake computer,
//! and a headless window driven through egui_kittest.

#![allow(dead_code)]

use std::sync::Arc;

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use sp_app::app::App;
use sp_app::app::host::Fakes;
use sp_app::engine::FakeEngine;
use sp_app::model::convert::Platform;
use sp_app::runtime::Mode;

pub struct World {
    pub engine: Arc<FakeEngine>,
    pub fakes: Fakes,
}

pub fn world() -> World {
    World {
        engine: Arc::new(FakeEngine::new()),
        fakes: Fakes::default(),
    }
}

pub type H = Harness<'static, App>;

pub fn harness_sized(w: &World, platform: Platform, size: [f32; 2]) -> H {
    let app = App::new(w.engine.clone(), Mode::Inline, w.fakes.host(platform));
    let mut h = Harness::builder()
        .with_size(size)
        .build_ui_state(|ui, app: &mut App| app.frame(ui), app);
    h.run_steps(4);
    h
}

/// A window whose engine calls run on worker threads, as the real one's do:
/// for a test that holds a call (FakeEngine::hold) and looks at the screen
/// while it is out. `settle` steps it until the workers' events are in.
pub fn harness_threads(w: &World, platform: Platform) -> H {
    let app = App::new(w.engine.clone(), Mode::Threads, w.fakes.host(platform));
    let mut h = Harness::builder()
        .with_size([1280.0, 800.0])
        .build_ui_state(|ui, app: &mut App| app.frame(ui), app);
    settle(&mut h);
    h
}

/// Step a threaded window for a moment of real time.
pub fn settle(h: &mut H) {
    for _ in 0..6 {
        std::thread::sleep(std::time::Duration::from_millis(15));
        h.run_steps(2);
    }
}

pub fn harness(w: &World, platform: Platform) -> H {
    harness_sized(w, platform, [1280.0, 800.0])
}

pub fn steps(h: &mut H) {
    h.run_steps(4);
}

pub fn has(h: &H, words: &str) -> bool {
    h.query_all_by_value(words).next().is_some() || h.query_all_by_label(words).next().is_some()
}

/// Cmd/Ctrl-O with the open dialog answering the demo script.
pub fn convert(w: &World, h: &mut H) {
    w.fakes.answer_dialog(Some("/scripts/field-station.pdf"));
    h.key_press_modifiers(Modifiers::COMMAND, Key::O);
    steps(h);
}
