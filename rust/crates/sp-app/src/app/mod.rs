//! The window: one `App`, three surfaces, the release notes sheet, the
//! heads-up, and the plumbing every engine call goes through
//! (was `desktop/ui/main.js` and `app.js`).
//!
//! Every engine call goes through [`App::call`] (counted toward busy, as
//! app.js counted it) or [`App::on_book`] (which also takes its turn on the
//! book, book-queue.js). Answers come back as events at the top of the next
//! frame and are handed to the surface that asked, by [`Purpose`].

pub mod book;
pub mod bookview;
pub mod convert;
pub mod frame;
pub mod help;
pub mod host;
pub mod menu;
pub mod panel;
pub mod read;
pub mod show;
pub mod sound;
pub mod tune;
pub mod widgets;

use std::collections::HashMap;
use std::sync::Arc;

use egui::{Id, Ui};
use serde_json::Value;

use crate::engine::{Engine, Reply, Request};
use crate::model::book_queue::{BookQueue, TurnId};
use crate::model::busy::{Busy, Dialogs};
use crate::model::convert::Script;
use crate::model::device_watch::Watcher;
use crate::model::focus::{Stop, first_stop};
use crate::model::frame::Surface;
use crate::model::send_routes::Device;
use crate::runtime::{Event, JobId, Mode, Runtime};
use crate::theme::{self, Tokens};

pub use host::Host;

/// What an engine call was for, so its answer reaches the right place.
#[derive(Clone, Debug)]
pub enum Purpose {
    Version,
    Convert(convert::Purpose),
    Tune(tune::Purpose),
    Panel(panel::Purpose),
    DevicesPoll { era: u64 },
}

struct Job {
    purpose: Purpose,
    counted: bool,
    turn: Option<TurnId>,
}

/// What the last send said, kept with the book it was about.
#[derive(Clone, Debug, PartialEq)]
pub struct SendResult {
    pub book: Option<String>,
    pub line: String,
    pub bad: bool,
    pub eject: Option<crate::model::device::Eject>,
}

pub struct App {
    pub host: Host,
    rt: Runtime,
    pub busy: Busy,
    pub dialogs: Dialogs,
    pub queue: BookQueue,
    waiting_turns: HashMap<TurnId, (Request, Purpose)>,
    jobs: HashMap<JobId, Job>,
    refused: Vec<(Purpose, String)>,
    carry: Option<TurnId>,

    pub script: Option<Script>,
    pub devices: Option<Vec<Device>>,
    /// The reader's device: None until known, then Some(None) for none.
    pub reader_device: Option<Option<&'static str>>,
    pub send_result: Option<SendResult>,
    pub animate_page: Option<bool>,
    pub typewriter_sounds: Option<bool>,

    pub surface: Surface,
    pub arriving: HashMap<Surface, f64>,
    pub notes_open: bool,
    pub engine_fault: Option<String>,
    /// Put the keyboard back somewhere it can carry on from (focus.js).
    pub restore_focus: bool,
    /// The focusable controls of the showing pane, as drawn this frame.
    pub stops: Vec<Stop<Id>>,
    pub pane_id: Id,
    pub drag_hover: bool,
    pub dark: bool,
    fonts_set: bool,
    /// The title the title bar was last told.
    pub title_shown: Option<String>,
    /// The Mac menu bar (menu.rs), when this window installed one.
    #[cfg(target_os = "macos")]
    pub menus: Option<menu::Menus>,
    /// Whether the window had the focus last frame (a return is news).
    pub window_focused: bool,
    /// An update's new copy has started: this window is closing.
    pub closing: bool,
    /// Drawn for the README pictures (src/capture.rs), never set by the
    /// shipped window: the version stamp keeps its place but is not
    /// painted, and the Convert page's library slot is not drawn at all,
    /// so a picture changes only when the window's look does; and no
    /// control keeps the keyboard's focus ring, since nobody is typing.
    pub picture_mode: bool,

    pub convert: convert::State,
    pub show: Option<show::Show>,
    pub read: read::State,
    pub tune: tune::State,
    pub panel: panel::State,
    pub heads_up: frame::HeadsUp,
    pub watcher: Watcher,
    pub update: frame::UpdateState,
    /// The Help menu's sheets (help.rs).
    pub help: help::State,
    /// Help menu items chosen in the Mac menu bar, applied next frame.
    pub help_chosen: Vec<crate::model::help::HelpItem>,
    /// The book as each place shows it (bookview.rs).
    pub views: bookview::Views,
    /// METHOD_READER_FRAME_LOG's recorder (framelog.rs), when it is set.
    pub frame_log: Option<crate::framelog::FrameLog>,
    /// What this frame has learnt about the next: something moving, or a
    /// timer due (model/motion.rs). Gathered while the frame draws.
    pub wakes: crate::model::motion::Wakes,
    /// What the last frame asked of the next one.
    pub wake: crate::model::motion::Wake,
    /// Wakes the window from another thread (a request's answer).
    pub repaint: Option<egui::Context>,
    /// Every property easing toward a value (model/motion.rs Transition),
    /// by the id of what it moves.
    transitions: HashMap<Id, crate::model::motion::Transition>,
}

impl App {
    pub fn new(engine: Arc<dyn Engine>, mode: Mode, host: Host) -> Self {
        let mut app = Self {
            host,
            rt: Runtime::new(engine, mode),
            busy: Busy::default(),
            dialogs: Dialogs::default(),
            queue: BookQueue::default(),
            waiting_turns: HashMap::new(),
            jobs: HashMap::new(),
            refused: Vec::new(),
            carry: None,
            script: None,
            devices: None,
            reader_device: None,
            send_result: None,
            animate_page: None,
            typewriter_sounds: None,
            surface: Surface::Convert,
            arriving: HashMap::new(),
            notes_open: false,
            engine_fault: None,
            restore_focus: false,
            stops: Vec::new(),
            pane_id: Id::new("surface-convert"),
            drag_hover: false,
            dark: false,
            fonts_set: false,
            title_shown: None,
            #[cfg(target_os = "macos")]
            menus: None,
            window_focused: true,
            closing: false,
            picture_mode: false,
            convert: convert::State::default(),
            show: None,
            read: read::State::default(),
            tune: tune::State::default(),
            panel: panel::State::default(),
            heads_up: frame::HeadsUp::default(),
            watcher: Watcher::default(),
            update: frame::UpdateState::default(),
            help: help::State::default(),
            help_chosen: Vec::new(),
            views: bookview::Views::new(mode == Mode::Threads),
            frame_log: crate::framelog::FrameLog::from_env(),
            wakes: crate::model::motion::Wakes::default(),
            wake: crate::model::motion::Wake::Idle,
            repaint: None,
            transitions: HashMap::new(),
        };
        // The one line that proves the engine is there.
        app.call(Request::Version, Purpose::Version);
        app.update.inline_checks = mode == Mode::Inline;
        if mode == Mode::Threads {
            // The book's faces decoded while the window starts, not in the
            // frame the first book arrives (mid page show).
            std::thread::spawn(book::warm_fonts);
        }
        app.update.boot(&mut app.host);
        // What's New, once after an update (never on a fresh install).
        if crate::model::update::whats_new_due(
            app.host.storage.as_mut(),
            crate::model::notes::RELEASE.version,
        ) {
            app.notes_open = true;
        }
        convert::draw_well(&mut app);
        app
    }

    /// Wake egui when a worker answers.
    pub fn set_repaint(&mut self, ctx: egui::Context) {
        self.repaint = Some(ctx.clone());
        self.update.repaint = Some(ctx.clone());
        self.views.set_repaint(ctx.clone());
        self.rt.set_repaint(ctx);
    }

    pub fn now(&self) -> f64 {
        (self.host.clock)()
    }

    pub fn tokens(&self) -> Tokens {
        theme::tokens(self.dark)
    }

    pub fn reduced_motion(&self) -> bool {
        self.host.reduced_motion()
    }

    /// Run an engine call, counted toward busy when it is somebody's work.
    pub fn call(&mut self, request: Request, purpose: Purpose) {
        self.start(request, purpose, None);
    }

    fn start(&mut self, request: Request, purpose: Purpose, turn: Option<TurnId>) {
        // A save whose script has gone since it was queued is dropped on its
        // turn, as tune.js's era check dropped it: what it owed went with
        // the script.
        if let Purpose::Tune(tune::Purpose::Save { era, .. }) = &purpose
            && *era != self.tune.era
        {
            if let Some(turn) = turn {
                self.finish_turn(turn);
            }
            return;
        }
        match self.busy.starting(request.call()) {
            Ok(counted) => {
                let job = self.rt.spawn(request);
                match &purpose {
                    Purpose::Convert(convert::Purpose::Convert { .. }) => convert::started(self),
                    Purpose::Tune(tune::Purpose::Save { .. }) => tune::save_started(self),
                    _ => {}
                }
                self.jobs.insert(
                    job,
                    Job {
                        purpose,
                        counted,
                        turn,
                    },
                );
            }
            Err(message) => {
                // A refused call is no work; its turn ends at once.
                if let Some(turn) = turn {
                    self.finish_turn(turn);
                }
                self.refused.push((purpose, message.to_string()));
            }
        }
    }

    /// From inside an answer's handler: the next call of the same piece of
    /// work, on the same turn (tune.js's flush saves, then rebuilds, on one
    /// turn on the book).
    pub fn continue_turn(&mut self, request: Request, purpose: Purpose) {
        let turn = self.carry.take();
        self.start(request, purpose, turn);
    }

    /// The Settings page's own save, on its turn, without cutting its own
    /// settle short a second time.
    pub fn save_on_book(&mut self, book: &str, request: Request, purpose: Purpose) {
        match self.queue.ask(book, "save") {
            Ok(asked) if asked.started => self.start(request, purpose, Some(asked.id)),
            Ok(asked) => {
                self.waiting_turns.insert(asked.id, (request, purpose));
            }
            Err(message) => self.refused.push((purpose, message)),
        }
    }

    /// Run an engine call on its turn on `book` (book-queue.js). What the
    /// Settings page owes is flushed first, so its save goes ahead.
    pub fn on_book(&mut self, book: &str, label: &str, request: Request, purpose: Purpose) {
        tune::cut_settle_short(self);
        match self.queue.ask(book, label) {
            Ok(asked) if asked.started => self.start(request, purpose, Some(asked.id)),
            Ok(asked) => {
                self.waiting_turns.insert(asked.id, (request, purpose));
            }
            Err(message) => self.refused.push((purpose, message)),
        }
    }

    fn finish_turn(&mut self, turn: TurnId) {
        for started in self.queue.finish(turn) {
            if let Some((request, purpose)) = self.waiting_turns.remove(&started) {
                self.start(request, purpose, Some(started));
            }
        }
    }

    /// Everything that has arrived since the last frame.
    pub fn pump(&mut self) {
        let refused = std::mem::take(&mut self.refused);
        let events = self.rt.poll();
        if !refused.is_empty() || !events.is_empty() {
            // News redraws more than one place: one more frame lets every
            // part of the window catch up, as egui's own requests do.
            self.wakes.moving();
        }
        for (purpose, message) in refused {
            self.deliver(purpose, Reply::failed(message));
        }
        for event in events {
            match event {
                Event::Progress { job, line } => {
                    if let Some(Job {
                        purpose: Purpose::Convert(p),
                        ..
                    }) = self.jobs.get(&job)
                    {
                        let p = p.clone();
                        convert::progress(self, &p, &line);
                    }
                }
                Event::Done { job, reply } => {
                    let Some(job) = self.jobs.remove(&job) else {
                        continue;
                    };
                    let now = self.now();
                    self.busy.ended(job.counted, now);
                    // The handler may carry the turn on (continue_turn);
                    // otherwise it ends once the answer has been read.
                    self.carry = job.turn;
                    self.deliver(job.purpose, reply);
                    if let Some(turn) = self.carry.take() {
                        self.finish_turn(turn);
                    }
                }
            }
        }
    }

    fn deliver(&mut self, purpose: Purpose, reply: Reply) {
        match purpose {
            Purpose::Version => {
                if let Err(message) = reply.answer {
                    self.engine_fault = Some(message);
                }
            }
            Purpose::Convert(p) => convert::answered(self, p, reply),
            Purpose::Tune(p) => tune::answered(self, p, reply),
            Purpose::Panel(p) => panel::answered(self, p, reply),
            Purpose::DevicesPoll { era } => {
                let answer = reply.answer.ok();
                if let Some(list) = self.watcher.answered(era, answer.as_ref()) {
                    self.devices_changed(list);
                }
            }
        }
    }

    /// The watcher heard a new list: the panel and the heads-up listen.
    fn devices_changed(&mut self, list: Vec<Device>) {
        self.devices = Some(list.clone());
        panel::devices_changed(self, &list);
        frame::heads_up_heard(self, &list);
    }

    /// Whether any engine call is out (for tests waiting on the runtime).
    pub fn quiet(&self) -> bool {
        self.jobs.is_empty() && self.waiting_turns.is_empty() && self.refused.is_empty()
    }

    /// Timers: the devices poll, the Settings settle, the heads-up's eight
    /// seconds, the reMarkable look.
    pub fn tick(&mut self) {
        let now = self.now();
        if let Some(era) = self.watcher.due(now) {
            self.call(
                Request::Devices { quick: true },
                Purpose::DevicesPoll { era },
            );
        }
        tune::tick(self, now);
        frame::heads_up_tick(self, now);
        panel::tick(self, now);
        self.update.tick(&mut self.host, now);
    }

    /// The timers above, as they stand once the frame has drawn (a knob
    /// moved this frame has started its settle): the window wakes for
    /// whichever falls due first, and sleeps between them.
    fn note_timers(&mut self) {
        let due = [
            self.watcher.next_due(),
            self.tune.deadline,
            self.heads_up.until,
            panel::next_due(self),
            self.update.next_due(),
        ];
        for at in due.into_iter().flatten() {
            self.wakes.at(at);
        }
    }

    /// Something on screen is mid-animation this frame: draw the next.
    pub fn moving(&mut self) {
        self.wakes.moving();
    }

    /// A property following `to` the way a CSS transition follows it: a new
    /// value eases in from wherever it is, along `timing`; at once under
    /// Reduce Motion. The first time `id` is asked it is already there.
    pub fn ease(&mut self, id: Id, to: f32, timing: crate::model::motion::Timing) -> f32 {
        self.ease_from(id, to, to, timing)
    }

    /// The same, for something that enters: the first time `id` is asked
    /// it starts at `from` and eases to `to` (a CSS animation on insert).
    /// [`App::forget_motion`] lets it enter again.
    pub fn ease_from(
        &mut self,
        id: Id,
        from: f32,
        to: f32,
        timing: crate::model::motion::Timing,
    ) -> f32 {
        let now = self.now();
        let instant = self.reduced_motion();
        let t = self
            .transitions
            .entry(id)
            .or_insert_with(|| crate::model::motion::Transition::at(from, timing));
        t.set(to, now, instant);
        let (value, running) = (t.value(now), t.running(now));
        if running {
            self.wakes.moving();
        }
        value
    }

    /// `id` has left the screen: the next time it is drawn it enters anew.
    pub fn forget_motion(&mut self, id: Id) {
        self.transitions.remove(&id);
    }

    /// The script on screen changed, or went away (main.js scriptChanged).
    pub fn script_changed(&mut self) {
        let open = self.script.is_some();
        let now = self.now();
        for s in [Surface::Read, Surface::Tune] {
            if open && !self.arriving.contains_key(&s) {
                self.arriving.insert(s, now);
            }
            if !open {
                self.arriving.remove(&s);
            }
        }
        if !open && self.surface != Surface::Convert {
            self.go_to(Surface::Convert);
        }
        if open {
            self.watcher.start(now);
        } else {
            self.watcher.stop();
            self.heads_up.reset();
            self.devices = None;
        }
        tune::script_changed(self);
        read::script_changed(self);
        frame::heads_up_recheck(self);
    }

    pub fn tab_available(&self, s: Surface) -> bool {
        !s.needs_script() || self.script.is_some()
    }

    /// Show a surface (frame.js setSurface).
    pub fn go_to(&mut self, s: Surface) {
        if s == self.surface {
            return;
        }
        let was = self.surface;
        self.surface = s;
        match was {
            Surface::Convert => panel::hidden(self),
            Surface::Read => read::hidden(self),
            Surface::Tune => {}
        }
        match s {
            Surface::Convert => {
                if self.convert.view == convert::View::Done {
                    panel::shown(self);
                }
            }
            Surface::Read => read::shown(self),
            Surface::Tune => tune::shown(self),
        }
        self.restore_focus = true;
        frame::heads_up_recheck(self);
    }

    /// Record a control the keyboard could stand on (in drawing order).
    pub fn stop(&mut self, id: Id, enabled: bool) {
        self.stops.push(Stop {
            id,
            enabled,
            shown: true,
        });
    }

    /// One frame: answers in, timers, input, drawing, the keyboard back.
    pub fn frame(&mut self, ui: &mut Ui) {
        #[cfg(target_os = "macos")]
        if let Some(menus) = &self.menus {
            let chosen = menus.pump(ui.ctx());
            self.help_chosen.extend(chosen);
            menus.sync(
                self.update.usable(&self.host),
                self.update.automatic(&self.host),
            );
        }
        for item in std::mem::take(&mut self.help_chosen) {
            help::choose(self, item);
        }
        if !self.fonts_set {
            self.fonts_set = true;
            ui.ctx().set_fonts(crate::fonts::definitions());
            theme::apply_style(ui.ctx());
            // The window's spacing is the stylesheet's rem scale (6.4, 12.8,
            // 19.2 points), which lands between pixels as a browser's does;
            // egui's debug-build "Unaligned" overlay would paint over it.
            // `Style::debug` exists only in a debug build of egui, so a
            // release build (the one that ships) has no overlay to turn off.
            #[cfg(debug_assertions)]
            ui.ctx().all_styles_mut(|s| s.debug.show_unaligned = false);
            // The faces are bound from the next frame on; draw nothing
            // that names them until then.
            ui.ctx().request_repaint();
            return;
        }
        self.wakes = crate::model::motion::Wakes::default();
        self.dark = ui.ctx().system_theme() == Some(egui::Theme::Dark);
        self.views.scale = ui.ctx().pixels_per_point();
        // The book's layouts and pictures, as its thread has made them.
        if self.views.pump(ui.ctx()) {
            self.moving();
        }
        self.pump();
        if self.update.pump(&mut self.host) {
            // The new copy is starting: this one goes (tauri's restart).
            self.closing = true;
        }
        if self.closing {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.tick();
        // The title bar names the script, then the app (Method Budget's
        // "Name - Method Budget"); told only when it changes.
        let title =
            crate::model::convert::window_title(self.script.as_ref().map(|s| s.title.as_str()));
        if self.title_shown.as_deref() != Some(title.as_str()) {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title_shown = Some(title);
        }
        frame::input(self, ui);
        self.stops.clear();
        frame::draw(self, ui);
        convert::adopt_when_laid(self, ui.ctx());
        panel::give_back_focus(self, ui.ctx());
        if self.restore_focus {
            self.restore_focus = false;
            let pane = Stop {
                id: self.pane_id,
                enabled: true,
                shown: true,
            };
            let tab = Stop {
                id: frame::tab_id(self.surface),
                enabled: true,
                shown: true,
            };
            let convert = Stop {
                id: frame::tab_id(Surface::Convert),
                enabled: true,
                shown: true,
            };
            let mut plan = self.stops.clone();
            plan.extend([pane, tab, convert]);
            if let Some(id) = first_stop(&plan) {
                ui.memory_mut(|m| m.request_focus(id));
            }
        }
        if self.picture_mode {
            // Nobody is at the keyboard in a picture: no control wears the
            // focus ring the window's own focus restoring would give it.
            ui.memory_mut(|m| {
                if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
            });
        }
        panel::pay_brass(self, ui.ctx());
        if self.show.as_ref().is_some_and(show::Show::running) {
            self.moving();
        }
        // A frame that heard from the keyboard or the pointer gets one more,
        // so what changed late in it (a press, a focus moved) is drawn; and
        // a call refused while drawing is answered on the next.
        let input = ui.ctx().input(|i| !i.events.is_empty() || i.any_touches());
        if input || !self.refused.is_empty() {
            self.moving();
        }
        self.note_timers();
        // The next frame: on the display's refresh while anything moves,
        // when a timer falls due, or not until something happens. An
        // engine answer wakes the window itself (runtime.rs).
        self.wake = self.wakes.wake(self.now());
        match self.wake {
            // Not request_repaint(): egui answers that with TWO frames, the
            // second asked at the start of the next pass, so a window
            // asking every frame got a stray frame between two vsyncs
            // (measured: frames 3 ms then 13 ms apart, the screen still at
            // 8.3). A delay shorter than a frame is the next frame, once.
            crate::model::motion::Wake::Next => ui
                .ctx()
                .request_repaint_after(crate::model::motion::NEXT_FRAME),
            crate::model::motion::Wake::In(ms) => ui
                .ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(ms / 1000.0)),
            crate::model::motion::Wake::Idle => {}
        }
    }

    /// The JSON the engine answered, or the sentence a failed call left.
    pub fn answer_or(reply: &Reply) -> Value {
        match &reply.answer {
            Ok(v) => v.clone(),
            Err(m) => serde_json::json!({"ok": false, "error": {"code": "internal", "message": m}}),
        }
    }
}

/// The window under the product's name: 1280 by 800, a 640 by 480 floor,
/// and the system's own title bar showing the app's name (and the
/// script's, once there is one), as Method Budget's and Method Schedule's
/// windows do. The page starts under it; the traffic lights sit in it.
pub fn viewport() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title(crate::model::names::PRODUCT)
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([640.0, 480.0])
        .with_resizable(true)
        .with_fullsize_content_view(false)
        .with_title_shown(true)
        .with_titlebar_shown(true)
}

/// How the window is opened: [`viewport`], presented on vsync (eframe's
/// AutoVsync, so on the display's own refresh). One frame in flight
/// instead of eframe's two was measured too (examples/motion_bench.rs,
/// BENCH_FRAMES_IN_FLIGHT): no steadier on this Mac, so the default stays.
pub fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: viewport(),
        ..Default::default()
    }
}

/// eframe's window: the App, drawn once a frame.
pub struct Window(pub App);

impl eframe::App for Window {
    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        let began = std::time::Instant::now();
        self.0.frame(ui);
        let wants = self.0.wake.word();
        if let Some(log) = self.0.frame_log.as_mut() {
            log.record(began, began.elapsed(), frame.info().cpu_usage, &wants);
        }
    }
}
