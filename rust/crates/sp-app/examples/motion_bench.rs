//! The window's motion, measured: the real App in a real window on the real
//! engine, driven through every animation the old window had, with the
//! frame recorder (src/framelog.rs) on. Each frame's line is tagged with the
//! step it belongs to, so the cadence and the cost of each animation can be
//! read out of the log.
//!
//! ```text
//! SCREEPUB_FRAME_LOG=out.tsv SCREEPUB_LIBRARY=<scratch> SCREEPUB_CONFIG_DIR=<scratch> \
//! SCREEPUB_VOLUME_ROOTS=<empty> CALIBRE_CONFIG_DIRECTORY=<scratch> \
//!   cargo run --release -p sp-app --example motion_bench -- tests/fixtures/field-station.pdf
//! ```
//!
//! It refuses to start without the scratch folders: the engine is linked in
//! and reads them, and the owner's own settings and library are never
//! touched. Calls go through the capture's Guard, so only the demo PDF is
//! converted and nothing copies, sends or starts Calibre. The window must
//! stay visible and in front while it runs (about 40 seconds): macOS stops
//! drawing a window that is hidden.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use egui::{Event, MouseWheelUnit, Pos2, Vec2};
use sp_app::app::{App, Window, convert};
use sp_app::capture::{Guard, capture_host};
use sp_app::direct::DirectEngine;
use sp_app::model::frame::Surface;
use sp_app::model::layout;
use sp_app::model::send_routes::Device;
use sp_app::runtime::Mode;

/// One step: its tag, how long it lasts once begun (ms), and what it does
/// at the start and on each frame.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Step {
    IdleWell,
    PageShow,
    ResultIdle,
    ReadEnter,
    SceneJumps,
    WheelRead,
    IndexToggle,
    HeadsUp,
    NotesSheet,
    TuneEnter,
    GroupToggle,
    WheelTune,
    IdleTune,
}

const STEPS: [(Step, &str, f64); 13] = [
    (Step::IdleWell, "idle-well", 2000.0),
    (Step::PageShow, "page-show", 0.0),
    (Step::ResultIdle, "idle-result", 2000.0),
    (Step::ReadEnter, "read-enter", 1500.0),
    (Step::SceneJumps, "scene-jump", 4200.0),
    (Step::WheelRead, "wheel-read", 2000.0),
    (Step::IndexToggle, "index-toggle", 2000.0),
    (Step::HeadsUp, "heads-up", 1500.0),
    (Step::NotesSheet, "notes-sheet", 1500.0),
    (Step::TuneEnter, "tune-enter", 1500.0),
    (Step::GroupToggle, "group-toggle", 2000.0),
    (Step::WheelTune, "wheel-tune", 2000.0),
    (Step::IdleTune, "idle-tune", 3000.0),
];

struct Bench {
    win: Window,
    demo: String,
    step: usize,
    began: Instant,
    last_wheel: Instant,
    acts: u32,
    done: bool,
}

fn need(name: &str) -> PathBuf {
    match std::env::var_os(name) {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => {
            eprintln!("motion_bench: set {name} to a scratch path first (see the file's head)");
            std::process::exit(2);
        }
    }
}

impl Bench {
    fn app(&mut self) -> &mut App {
        &mut self.win.0
    }

    fn ms(&self) -> f64 {
        self.began.elapsed().as_secs_f64() * 1000.0
    }

    fn begin(&mut self, ctx: &egui::Context) {
        self.began = Instant::now();
        self.acts = 0;
        let (step, tag, _) = STEPS[self.step];
        if let Some(log) = self.app().frame_log.as_mut() {
            log.tag = tag.into();
        }
        match step {
            Step::PageShow => {
                let demo = self.demo.clone();
                convert::drop_paths(self.app(), &[demo]);
            }
            Step::ReadEnter => self.app().go_to(Surface::Read),
            Step::TuneEnter => self.app().go_to(Surface::Tune),
            _ => {}
        }
        ctx.request_repaint();
    }

    /// The step's own moves, at its moments.
    fn act(&mut self) {
        let (step, _, _) = STEPS[self.step];
        let t = self.ms();
        let due = |every: f64, acts: u32| t >= every * f64::from(acts + 1);
        match step {
            Step::SceneJumps if due(700.0, self.acts) && self.acts < 5 => {
                // Far, near, far: the scene index pressed, as a person would.
                let n = self.acts as usize;
                let app = self.app();
                let marks = app.read.marks.clone();
                if !marks.is_empty() {
                    let pick = [marks.len() - 1, 1, marks.len() / 2, 0, marks.len() - 2];
                    let i = pick[n % pick.len()].min(marks.len() - 1);
                    sp_app::app::read::go_to_scene(app, &marks[i].id);
                }
                self.acts += 1;
            }
            Step::IndexToggle if due(500.0, self.acts) && self.acts < 3 => {
                let app = self.app();
                app.read.index_open = !app.read.index_open;
                self.acts += 1;
            }
            Step::HeadsUp if self.acts == 0 => {
                self.app().heads_up.offered = Some(Device {
                    id: "bench".into(),
                    kind: "kindle".into(),
                    name: "Kindle".into(),
                    volume: Some("/Volumes/Kindle".into()),
                    transport: None,
                    copy_note: None,
                });
                self.acts += 1;
            }
            Step::HeadsUp if t >= 1000.0 && self.acts == 1 => {
                self.app().heads_up.close();
                self.acts += 1;
            }
            Step::NotesSheet if self.acts == 0 => {
                self.app().notes_open = true;
                self.acts += 1;
            }
            Step::NotesSheet if t >= 1000.0 && self.acts == 1 => {
                self.app().notes_open = false;
                self.acts += 1;
            }
            Step::GroupToggle if due(500.0, self.acts) && self.acts < 3 => {
                let app = self.app();
                if let Some(o) = app.tune.open.get_mut(1) {
                    *o = !*o;
                }
                self.acts += 1;
            }
            _ => {}
        }
    }

    fn step_over(&mut self) -> bool {
        let (step, _, lasts) = STEPS[self.step];
        if step == Step::PageShow {
            let app = &self.win.0;
            return app.convert.view == convert::View::Done && app.show.is_none() && app.quiet();
        }
        self.ms() >= lasts
    }

    /// Where a trackpad would be: over the script on Read, over the preview
    /// on Settings (layout.rs's geometry for this window).
    fn wheel_at(&self, ctx: &egui::Context) -> Option<Pos2> {
        let size = ctx.content_rect().size();
        let block = layout::block(size.x);
        let span = layout::Span {
            left: 0.0,
            width: block.width,
        };
        match STEPS[self.step].0 {
            Step::WheelRead => {
                let g = layout::read(span);
                Some(Pos2::new(
                    block.left + g.script.left + g.script.width / 2.0,
                    size.y * 0.6,
                ))
            }
            Step::WheelTune => {
                let g = layout::settings(span);
                Some(Pos2::new(
                    block.left + g.preview.left + g.preview.width / 2.0,
                    size.y * 0.6,
                ))
            }
            _ => None,
        }
    }
}

impl eframe::App for Bench {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if self.done {
            return;
        }
        // A trackpad scroll: points at about 1200 per second, down then up.
        let now = Instant::now();
        let dt = now.duration_since(self.last_wheel).as_secs_f32().min(0.05);
        self.last_wheel = now;
        if let Some(at) = self.wheel_at(ctx) {
            let down = self.ms() < 1000.0;
            let dy = 1200.0 * dt * if down { -1.0 } else { 1.0 };
            raw.events.push(Event::PointerMoved(at));
            raw.events.push(Event::MouseWheel {
                unit: MouseWheelUnit::Point,
                delta: Vec2::new(0.0, dy),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            });
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.done {
            return;
        }
        if self.step_over() {
            self.step += 1;
            if self.step == STEPS.len() {
                self.done = true;
                if let Some(log) = self.app().frame_log.as_mut() {
                    log.flush();
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            self.begin(&ctx);
        }
        self.act();
        eframe::App::ui(&mut self.win, ui, frame);
        // The bench's own clock: wake at the next moment it acts, never
        // more often than the window itself asks. A wheel step asks every
        // frame, as a trackpad's stream of events does.
        if STEPS[self.step].0 == Step::WheelRead || STEPS[self.step].0 == Step::WheelTune {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

fn main() -> eframe::Result {
    let demo = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "tests/fixtures/field-station.pdf".into());
    let demo = std::fs::canonicalize(&demo)
        .unwrap_or_else(|e| {
            eprintln!("motion_bench: {demo}: {e}");
            std::process::exit(2);
        })
        .to_string_lossy()
        .into_owned();
    need("SCREEPUB_FRAME_LOG");
    need("SCREEPUB_CONFIG_DIR");
    need("SCREEPUB_VOLUME_ROOTS");
    need("CALIBRE_CONFIG_DIRECTORY");
    let library = need("SCREEPUB_LIBRARY");
    let options = eframe::NativeOptions {
        viewport: sp_app::app::viewport(),
        ..Default::default()
    };
    eframe::run_native(
        "Screepub motion bench",
        options,
        Box::new(move |cc| {
            let engine = Arc::new(Guard::new(
                Arc::new(DirectEngine::new()),
                PathBuf::from(&demo),
                library,
            ));
            let mut app = App::new(engine, Mode::Threads, capture_host());
            app.set_repaint(cc.egui_ctx.clone());
            if let Some(log) = app.frame_log.as_mut() {
                log.tag = STEPS[0].1.into();
            }
            Ok(Box::new(Bench {
                win: Window(app),
                demo,
                step: 0,
                began: Instant::now(),
                last_wheel: Instant::now(),
                acts: 0,
                done: false,
            }))
        }),
    )
}
