//! The window: one `App`, three surfaces, the release notes sheet, the
//! heads-up, and the plumbing every engine call goes through
//! (`desktop/ui/main.js` and `app.js`).
//!
//! Every engine call goes through [`App::call`] (counted toward busy, as
//! app.js counted it) or [`App::on_book`] (which also takes its turn on the
//! book, book-queue.js). Answers come back as events at the top of the next
//! frame and are handed to the surface that asked, by [`Purpose`].

pub mod book;
pub mod bookview;
pub mod convert;
pub mod frame;
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
    /// The Mac menu bar (menu.rs), when this window installed one.
    #[cfg(target_os = "macos")]
    pub menus: Option<menu::Menus>,
    /// Whether the window had the focus last frame (a return is news).
    pub window_focused: bool,
    /// An update's new copy has started: this window is closing.
    pub closing: bool,

    pub convert: convert::State,
    pub show: Option<show::Show>,
    pub read: read::State,
    pub tune: tune::State,
    pub panel: panel::State,
    pub heads_up: frame::HeadsUp,
    pub watcher: Watcher,
    pub update: frame::UpdateState,
    /// The book as each place shows it (bookview.rs).
    pub views: bookview::Views,
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
            #[cfg(target_os = "macos")]
            menus: None,
            window_focused: true,
            closing: false,
            convert: convert::State::default(),
            show: None,
            read: read::State::default(),
            tune: tune::State::default(),
            panel: panel::State::default(),
            heads_up: frame::HeadsUp::default(),
            watcher: Watcher::default(),
            update: frame::UpdateState::default(),
            views: bookview::Views::default(),
        };
        // The one line that proves the engine is there.
        app.call(Request::Version, Purpose::Version);
        app.update.boot(&mut app.host);
        convert::draw_well(&mut app);
        app
    }

    /// Wake egui when a worker answers.
    pub fn set_repaint(&mut self, ctx: egui::Context) {
        self.update.repaint = Some(ctx.clone());
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
        for (purpose, message) in std::mem::take(&mut self.refused) {
            self.deliver(purpose, Reply::failed(message));
        }
        for event in self.rt.poll() {
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
            menus.pump(ui.ctx());
        }
        if !self.fonts_set {
            self.fonts_set = true;
            ui.ctx().set_fonts(crate::fonts::definitions());
            // The faces are bound from the next frame on; draw nothing
            // that names them until then.
            ui.ctx().request_repaint();
            return;
        }
        self.dark = ui.ctx().system_theme() == Some(egui::Theme::Dark);
        self.views.scale = ui.ctx().pixels_per_point();
        self.pump();
        if self.update.pump(&mut self.host) {
            // The new copy is starting: this one goes (tauri's restart).
            self.closing = true;
        }
        if self.closing {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.tick();
        frame::input(self, ui);
        self.stops.clear();
        frame::draw(self, ui);
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
        panel::pay_brass(self, ui.ctx());
        if !self.quiet()
            || self.show.as_ref().is_some_and(show::Show::running)
            || self.watcher.running()
        {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
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

/// The window as tauri.conf.json described it: "Screepub", 1280 by 800,
/// a 640 by 480 floor, and on a Mac no title bar of its own (the paper runs
/// under the traffic lights).
pub fn viewport() -> egui::ViewportBuilder {
    egui::ViewportBuilder::default()
        .with_title("Screepub")
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([640.0, 480.0])
        .with_resizable(true)
        .with_fullsize_content_view(true)
        .with_title_shown(false)
        .with_titlebar_shown(false)
}

/// eframe's window: the App, drawn once a frame.
pub struct Window(pub App);

impl eframe::App for Window {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.0.frame(ui);
    }
}
