//! The page on the desk: the paper, the binding's brads and punched hole,
//! the tab bar, the stamp and the update label, the release notes sheet,
//! the Kindle heads-up, the drag strip, and the window's keyboard and drops
//! (was `desktop/ui/frame.js`, `main.js`, `heads-up.js`, `notes-surface.js`).

use egui::epaint::{Mesh, Vertex};
use egui::{
    Align, Color32, CornerRadius, Id, Key, Layout, Modifiers, Pos2, Rect, Sense, Stroke, Ui, Vec2,
    ViewportCommand, WidgetInfo, WidgetType, accesskit,
};

use super::widgets::{self, Look};
use super::{App, convert, panel, read, tune};
use crate::model::device::{
    HEADS_UP_COPY, HEADS_UP_LATER, HEADS_UP_MS, heads_up_for, heads_up_line,
};
use crate::model::frame::{Surface, TabKey, rev_label, tab_presence, tab_step};
use crate::model::layout;
use crate::model::motion;
use crate::model::notes::{RELEASE, visible_sections};
use crate::model::send_routes::Device;
use crate::model::update::{
    self as u, CheckOutcome, Drawn, Phase, Schedule, automatic_checks, begin_check, finish_check,
    label_actionable, notes_view, remembered_offer, set_automatic, update_label, updates_possible,
};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use sp_update::flow::Event as UpdateEvent;
use sp_update::gate::EngineGate;

use crate::model::update_flow::{Flow, Offer, RunOutcome, Start};
use crate::theme::{self, Tokens, size};

pub fn tab_id(s: Surface) -> Id {
    Id::new(("tab", s.label()))
}

// --------------------------------------------------------------- updates

/// What the update's run says from its own thread.
enum RunMsg {
    Event(UpdateEvent),
    Done(RunOutcome),
    Restarted(bool),
    /// The run panicked, or its thread ended without a last word.
    Threw,
}

impl RunMsg {
    fn is_last(&self) -> bool {
        !matches!(self, RunMsg::Event(_))
    }
}

/// What the notes say when the run or the check stops without an answer.
const RUN_STOPPED: &str = "The update stopped unexpectedly. Try again.";
const CHECK_STOPPED: &str = "The update check stopped unexpectedly.";

/// What the updater's check answers.
type CheckAnswer = Result<Option<u::Found>, String>;

/// What the download ahead of the click answers: the version kept.
type PrepareAnswer = Result<String, String>;

/// An update event as the label and the notes draw it.
pub fn phase_of(event: UpdateEvent) -> Option<Phase> {
    Some(match event {
        UpdateEvent::Downloading {
            version,
            received,
            total,
            body,
        } => Phase::Downloading {
            version,
            received: received as f64,
            total: total.map(|t| t as f64),
            body,
        },
        UpdateEvent::Installing { version } => Phase::Installing { version },
        UpdateEvent::Waiting { version } => Phase::Waiting { version },
        UpdateEvent::Stalled { version } => Phase::Stalled { version },
        UpdateEvent::Restarting { version } => Phase::Restarting { version },
        UpdateEvent::Installed { version } => Phase::Installed { version },
        UpdateEvent::Failed { version, message } => Phase::Failed { version, message },
        UpdateEvent::Cleared => return None,
    })
}

#[derive(Default)]
pub struct UpdateState {
    /// The one moment, which the label and the notes both read
    /// (update-flow.js).
    pub flow: Flow,
    pub drawn: Drawn,
    pub notes_line: Option<String>,
    heard: u64,
    /// The label had the keyboard when it was last drawn.
    pub label_focused: bool,
    /// The run on its thread, while it is out.
    run: Option<Receiver<RunMsg>>,
    /// The check on its thread, while it is out, and whether a person
    /// asked for it.
    checking: Option<(Receiver<CheckAnswer>, bool)>,
    /// A quiet check's update downloading ahead of the click, with the
    /// offer it will become ("Update ready", or "Update to" if it fails).
    preparing: Option<(Receiver<PrepareAnswer>, Offer)>,
    /// When the next quiet check is due. Armed only by
    /// [`UpdateState::start_schedule`], which the shipped window calls
    /// (main.rs): a test, a capture or a headless run never checks on its
    /// own.
    pub schedule: Schedule,
    /// Run the check at once on the window's thread (tests, as the engine
    /// runtime's Inline mode).
    pub inline_checks: bool,
    /// Wakes the window when the run says something.
    pub repaint: Option<egui::Context>,
}

impl UpdateState {
    pub fn usable(&self, host: &super::Host) -> bool {
        host.updater.ready() && updates_possible(host.platform)
    }

    pub fn phase(&self) -> Option<&Phase> {
        self.flow.phase()
    }

    /// At launch, once: what an earlier check found. The first quiet
    /// check waits for the schedule (two minutes).
    pub fn boot(&mut self, host: &mut super::Host) {
        if !self.usable(host) {
            return;
        }
        let remembered = remembered_offer(host.storage.as_ref(), RELEASE.version);
        if remembered.is_none() {
            u::forget_found(host.storage.as_mut());
        }
        if self.flow.boot(remembered) {
            self.heard();
        }
    }

    /// The quiet checks from now on: the first two minutes away, then every
    /// six hours, each only while the switch is on.
    pub fn start_schedule(&mut self, now: f64) {
        self.schedule = Schedule::starting(now);
    }

    /// Whether "Check for Updates Automatically" is on.
    pub fn automatic(&self, host: &super::Host) -> bool {
        automatic_checks(host.storage.as_ref())
    }

    /// The switch, set by a person. Turning it on checks at once (the
    /// consent is fresh); the next quiet check is then six hours away.
    pub fn set_automatic(&mut self, host: &mut super::Host, on: bool) {
        set_automatic(host.storage.as_mut(), on);
        if on && self.usable(host) {
            self.check(host, false);
        }
    }

    /// The schedule's turn, from the window's timers: a quiet check when one
    /// is due, the switch is on, and nothing about an update is under way.
    pub fn tick(&mut self, host: &mut super::Host, now: f64) {
        if !self.schedule.take_due(now) {
            return;
        }
        let idle = self.checking.is_none() && self.preparing.is_none() && !self.flow.running();
        if idle && self.usable(host) && self.automatic(host) {
            self.check(host, false);
        }
    }

    /// When the window must wake for the schedule.
    pub fn next_due(&self) -> Option<f64> {
        self.schedule.next_due()
    }

    /// Asks for latest.json. The request runs on its own thread (a stalled
    /// network held the window for up to the transport's ten minutes); its
    /// answer arrives
    /// through [`UpdateState::pump`]. In tests (`inline_checks`) it runs at
    /// once, as the engine runtime's Inline mode does. A check already out
    /// is not doubled.
    pub fn check(&mut self, host: &mut super::Host, manual: bool) {
        if self.checking.is_some() {
            return;
        }
        let now = (host.clock)();
        if !begin_check(manual, host.storage.as_mut(), now) {
            self.answered(host, CheckOutcome::Skipped, manual);
            return;
        }
        // Whoever asked, the next quiet check is a full interval away.
        self.schedule.checked(now);
        if self.inline_checks {
            let answer = host.updater.check();
            let outcome = finish_check(host.storage.as_mut(), answer);
            self.answered(host, outcome, manual);
            return;
        }
        let (tx, rx) = channel();
        let updater = host.updater.clone();
        let repaint = self.repaint.clone();
        std::thread::spawn(move || {
            let answer = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| updater.check()))
                .unwrap_or_else(|_| Err(CHECK_STOPPED.to_string()));
            let _ = tx.send(answer);
            if let Some(ctx) = &repaint {
                ctx.request_repaint();
            }
        });
        self.checking = Some((rx, manual));
    }

    fn answered(&mut self, host: &super::Host, outcome: CheckOutcome, manual: bool) {
        match &outcome {
            // Already downloaded and waiting: it stays ready. Said again, so
            // the update sheet's line answers the person who asked.
            CheckOutcome::Offer { version, .. } if self.flow.is_ready(version) => {
                let phase = self.flow.phase().cloned();
                self.flow.set_phase(phase);
            }
            // A quiet check's find is downloaded before anything is said,
            // so what appears is "Update ready", a click from installing.
            CheckOutcome::Offer { version, body } if !manual && !self.flow.running() => {
                self.prepare(
                    host,
                    Offer {
                        version: version.clone(),
                        body: body.clone(),
                        live: true,
                    },
                );
            }
            _ => self.flow.check_answered(&outcome),
        }
        self.heard();
        if let CheckOutcome::Error { message } = &outcome
            && manual
        {
            self.notes_line = Some(message.clone());
        }
    }

    /// Download `offer` ahead of the click, on its own thread (at once in
    /// tests). Nothing is drawn while it downloads.
    fn prepare(&mut self, host: &super::Host, offer: Offer) {
        if self.preparing.is_some() {
            return;
        }
        if self.inline_checks {
            let answer = host.updater.prepare();
            self.prepared(answer, offer);
            return;
        }
        let (tx, rx) = channel();
        let updater = host.updater.clone();
        let repaint = self.repaint.clone();
        std::thread::spawn(move || {
            let answer =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| updater.prepare()))
                    .unwrap_or_else(|_| Err(CHECK_STOPPED.to_string()));
            let _ = tx.send(answer);
            if let Some(ctx) = &repaint {
                ctx.request_repaint();
            }
        });
        self.preparing = Some((rx, offer));
    }

    /// The download ahead of the click is done: "Update ready", or, if it
    /// failed, the offer as a check a person asked for shows it (the click
    /// then downloads it, with its progress, and says what went wrong).
    fn prepared(&mut self, answer: PrepareAnswer, offer: Offer) {
        match answer {
            Ok(version) if version == offer.version => self.flow.ready_found(offer),
            _ => self.flow.offer_found(offer),
        }
        self.heard();
    }

    fn pump_prepare(&mut self) {
        let Some((rx, _)) = &self.preparing else {
            return;
        };
        let answer = match rx.try_recv() {
            Ok(answer) => answer,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err(CHECK_STOPPED.to_string()),
        };
        if let Some((_, offer)) = self.preparing.take() {
            self.prepared(answer, offer);
        }
    }

    /// The check's answer, once it has come.
    fn pump_check(&mut self, host: &mut super::Host) {
        let Some((rx, manual)) = &self.checking else {
            return;
        };
        let manual = *manual;
        let answer = match rx.try_recv() {
            Ok(answer) => answer,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err(CHECK_STOPPED.to_string()),
        };
        self.checking = None;
        let outcome = finish_check(host.storage.as_mut(), answer);
        self.answered(host, outcome, manual);
    }

    /// A click on the label or the notes' button: the one run, or "Restart
    /// now" after a stall (update-flow.js start). The run goes on its own
    /// thread; `gate` is the window's busy count, which it retires and
    /// waits on.
    pub fn start(&mut self, host: &super::Host, gate: EngineGate) {
        let what = self.flow.start();
        self.heard();
        let (tx, rx) = channel();
        let updater = host.updater.clone();
        let repaint = self.repaint.clone();
        let wake = move || {
            if let Some(ctx) = &repaint {
                ctx.request_repaint();
            }
        };
        match what {
            Start::Nothing => return,
            Start::Run(offer) => {
                std::thread::spawn(move || {
                    let mut emit = |e| {
                        let _ = tx.send(RunMsg::Event(e));
                        wake();
                    };
                    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        updater.install(offer.live, &gate, &mut emit)
                    }));
                    let _ = tx.send(match outcome {
                        Ok(outcome) => RunMsg::Done(outcome),
                        Err(_) => RunMsg::Threw,
                    });
                    wake();
                });
            }
            Start::RestartNow(version) => {
                std::thread::spawn(move || {
                    let mut emit = |e| {
                        let _ = tx.send(RunMsg::Event(e));
                        wake();
                    };
                    let started = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        updater.restart(&version, &mut emit)
                    }));
                    let _ = tx.send(match started {
                        Ok(started) => RunMsg::Restarted(started),
                        Err(_) => RunMsg::Threw,
                    });
                    wake();
                });
            }
        }
        self.run = Some(rx);
    }

    /// What the run said since the last frame. True when the new copy has
    /// started and this window should close.
    pub fn pump(&mut self, host: &mut super::Host) -> bool {
        self.pump_check(host);
        self.pump_prepare();
        let Some(rx) = &self.run else {
            return false;
        };
        let mut close = false;
        let mut ended = false;
        let mut msgs: Vec<RunMsg> = Vec::new();
        loop {
            match rx.try_recv() {
                Ok(msg) => msgs.push(msg),
                Err(TryRecvError::Empty) => break,
                // The run's thread is gone; without a last word, it died.
                Err(TryRecvError::Disconnected) => {
                    if !msgs.iter().any(RunMsg::is_last) {
                        msgs.push(RunMsg::Threw);
                    }
                    break;
                }
            }
        }
        for msg in msgs {
            match msg {
                RunMsg::Event(e) => match phase_of(e) {
                    Some(p) => self.flow.during(p),
                    None => self.flow.set_phase(None),
                },
                RunMsg::Done(outcome) => {
                    close = outcome == RunOutcome::Restarting;
                    self.flow.finished(outcome, host.storage.as_mut());
                    ended = true;
                }
                RunMsg::Restarted(started) => {
                    close = started;
                    ended = true;
                }
                RunMsg::Threw => {
                    self.flow.threw(RUN_STOPPED);
                    ended = true;
                }
            }
        }
        if ended {
            self.run = None;
        }
        self.heard();
        close
    }

    /// The notes hear every emit of the flow, repeats included: what they
    /// drew is kept, and a line waiting on a change ("Looking…") goes.
    fn heard(&mut self) {
        if self.flow.heard == self.heard {
            return;
        }
        self.heard = self.flow.heard;
        if let Some(p) = self.flow.phase() {
            self.drawn.drew_offer = true;
            if let Some(v) = p.version() {
                self.drawn.version = Some(v.to_string());
            }
            if let Phase::Offer { body, .. }
            | Phase::Ready { body, .. }
            | Phase::Downloading { body, .. } = p
            {
                self.drawn.body = body.clone();
            }
        }
        self.notes_line = None;
    }
}

// --------------------------------------------------------------- heads-up

#[derive(Default)]
pub struct HeadsUp {
    pub seen: Option<Vec<String>>,
    pub offered: Option<Device>,
    pub until: Option<f64>,
}

impl HeadsUp {
    pub fn close(&mut self) {
        self.offered = None;
        self.until = None;
    }

    pub fn reset(&mut self) {
        self.close();
        self.seen = None;
    }
}

fn on_result(app: &App) -> bool {
    app.surface == Surface::Convert && app.convert.view == convert::View::Done
}

pub fn heads_up_heard(app: &mut App, devices: &[Device]) {
    let open = app.script.is_some() && !app.convert.converting;
    let (seen, show) = heads_up_for(app.heads_up.seen.as_deref(), devices, open, on_result(app));
    app.heads_up.seen = Some(seen);
    if let Some(device) = show {
        app.heads_up.offered = Some(device);
        app.heads_up.until = Some(app.now() + HEADS_UP_MS);
    }
}

pub fn heads_up_tick(app: &mut App, now: f64) {
    if app.heads_up.until.is_some_and(|u| now >= u) {
        app.heads_up.close();
    }
}

pub fn heads_up_recheck(app: &mut App) {
    if app.heads_up.offered.is_some() && (on_result(app) || app.convert.converting) {
        app.heads_up.close();
    }
}

// ------------------------------------------------------------------ input

/// Cmd/Ctrl-O, a file dropped or dragged over, and a key or click ending
/// the page show.
pub fn input(app: &mut App, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let (open, dropped, hovering, pressed) = ctx.input_mut(|i| {
        let open = i.consume_key(Modifiers::COMMAND, Key::O);
        let dropped: Vec<String> = i
            .raw
            .dropped_files
            .iter()
            .map(|f| f.path().to_string_lossy().into_owned())
            .filter(|p| !p.is_empty())
            .collect();
        let pressed = i
            .events
            .iter()
            .any(|e| matches!(e, egui::Event::Key { pressed: true, .. }))
            || i.pointer.any_pressed();
        (open, dropped, !i.raw.hovered_files.is_empty(), pressed)
    });
    app.drag_hover = hovering;
    // Keyboard or pointer, for the focus rings drawn below.
    widgets::note_ring_input(&ctx);
    // Back from another app (perhaps from installing Kindle Previewer).
    let focused = ctx.input(|i| i.focused);
    if focused && !app.window_focused {
        panel::focus_returned(app);
    }
    app.window_focused = focused;
    let now = app.now();
    if pressed && let Some(show) = app.show.as_mut() {
        show.press(now);
    }
    if open {
        convert::choose(app);
    }
    if !dropped.is_empty() {
        convert::drop_paths(app, &dropped);
    }
    if let Some(show) = app.show.as_mut() {
        let heard = (show.keys, show.dings);
        show.tick(now);
        if show.keys > heard.0 {
            app.host.speaker.play(super::host::Sound::Key);
        }
        if show.dings > heard.1 {
            app.host.speaker.play(super::host::Sound::Ding);
        }
        if !show.running() && show.book_arrived() {
            app.show = None;
        }
    }
}

// ---------------------------------------------------------------- drawing

pub fn draw(app: &mut App, ui: &mut Ui) {
    let t = app.tokens();
    let window = ui.ctx().content_rect().size();
    let block = layout::block(window.x);
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(t.paper))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let top = ui.cursor().min;
                    // The drag strip: as tall as the sheet's top padding, part of
                    // the page, so it scrolls away and never covers a control.
                    // DRAG alone, not Sense::drag(), which is focusable: the
                    // first Tab vanished into it.
                    let strip = Rect::from_min_size(top, Vec2::new(window.x, layout::SPACE[7]));
                    let drag = ui.interact(strip, Id::new("drag-strip"), Sense::DRAG);
                    if drag.drag_started() {
                        ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
                    }
                    ui.add_space(layout::SPACE[7]);
                    let min_height = (window.y - layout::SPACE[7]).max(0.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.add_space(block.left);
                        ui.vertical(|ui| {
                            ui.set_width(block.width);
                            ui.set_min_height(min_height);
                            sheet(app, ui, block, window);
                        });
                    });
                });
        });
    paint_rail(ui, &t, window);
    notes_sheet(app, ui);
    super::help::draw(app, ui);
}

fn sheet(app: &mut App, ui: &mut Ui, block: layout::Span, window: Vec2) {
    let t = app.tokens();
    let sheet_top = ui.cursor().min.y;
    tab_bar(app, ui);
    ui.add_space(layout::SPACE[6]);
    heads_up(app, ui);
    // The pane: a focusable tabpanel, labelled by its tab, so Tab can leave
    // the bar and land in the surface before it has a control.
    let pane_id = Id::new(("surface", app.surface.label()));
    app.pane_id = pane_id;
    let pane = ui.interact(
        Rect::from_min_size(ui.cursor().min, Vec2::new(1.0, 1.0)),
        pane_id,
        Sense::focusable_noninteractive(),
    );
    ui.ctx().accesskit_node_builder(pane.id, |n| {
        n.set_role(accesskit::Role::TabPanel);
        n.set_label(app.surface.label());
        n.push_labelled_by(tab_id(app.surface).accesskit_id());
    });
    // Where a surface that fits the window (Read) must end for the foot
    // below it to sit whole inside the window: the page's top, plus the
    // window, less the foot.
    let page_top = sheet_top - layout::SPACE[7];
    let surface_end = page_top + window.y - foot_height(app, ui, block);
    match app.surface {
        Surface::Convert => convert::draw(app, ui, block, window),
        Surface::Read => read::draw(app, ui, block, window, surface_end),
        Surface::Tune => tune::draw(app, ui, block, window),
    }
    // The foot: the update label beside the rev stamp, the dead-engine line
    // above them, at the block's right edge.
    ui.add_space(layout::SPACE[10]);
    let min_bottom = sheet_top + window.y - layout::SPACE[7] - layout::SPACE[10];
    if ui.cursor().min.y < min_bottom {
        ui.add_space(min_bottom - ui.cursor().min.y);
    }
    if let Some(fault) = app.engine_fault.clone() {
        ui.with_layout(Layout::top_down(Align::Max), |ui| {
            ui.set_max_width(block.width.min(66.0 * 8.0));
            widgets::text(ui, &fault, theme::structure(size::NOTE), t.alarm);
        });
    }
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = layout::SPACE[4];
        if app.picture_mode {
            // visibility: hidden, as the capture's steps.js did: the place
            // stays, nothing is painted and nothing can be pressed.
            stamp_space(ui, &t, &rev_label(RELEASE.version));
        } else {
            let stamp = stamp_button(ui, &t, &rev_label(RELEASE.version));
            if stamp.clicked() {
                app.help.sheet = None;
                app.notes_open = true;
            }
            // Windows and Linux: the Help menu, beside the stamp.
            super::help::menu(app, ui, &t);
        }
        if let Some(words) = update_label(app.update.phase()) {
            let actionable = label_actionable(app.update.phase());
            let label = update_button(ui, &t, &words, actionable);
            app.update.label_focused = label.has_focus();
            if label.clicked() && actionable {
                let gate = app.busy.gate();
                app.update.start(&app.host, gate);
            }
        } else if std::mem::take(&mut app.update.label_focused) {
            // The label went while the keyboard stood on it: the page's
            // first stop takes it (main.js).
            app.restore_focus = true;
        }
    });
    ui.add_space(layout::SPACE[5]);
}

fn tab_bar(app: &mut App, ui: &mut Ui) {
    let t = app.tokens();
    let now = app.now();
    let reduced = app.reduced_motion();
    let bar = ui.horizontal(|ui| {
        // The gaps between tabs move the window, as the strip does:
        // registered first, so every tab sits above it.
        let gaps = Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), 20.0));
        let gap = ui.interact(gaps, Id::new("tab-gaps"), Sense::DRAG);
        if gap.drag_started() {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
        }
        ui.spacing_mut().item_spacing.x = layout::SPACE[5];
        for (index, s) in Surface::ALL.into_iter().enumerate() {
            let presence = tab_presence(app.tab_available(s), s == app.surface);
            if !presence.shown {
                continue;
            }
            // Arriving: fade and rise over 0.2 s, eased out, staggered by
            // position (style.css tab-arrive).
            let arrival = app.arriving.get(&s).map(|at| {
                let delay = if index >= 2 {
                    motion::TAB_STAGGER_MS * (index - 1) as f64
                } else {
                    0.0
                };
                let timing = motion::TAB_ARRIVE.delayed(delay);
                (timing.progress(now - at), timing.running(now - at))
            });
            let k = match arrival {
                Some((k, running)) if !reduced => {
                    if running {
                        app.moving();
                    }
                    k
                }
                _ => 1.0,
            };
            let font = theme::structure_bold(size::NOTE);
            let words = widgets::caps(s.label());
            let galley = ui.painter().layout_no_wrap(words, font, t.ink);
            let size = galley.size() + Vec2::new(0.0, layout::SPACE[1] + 2.0);
            // A tab claims drags too: egui hands a press's drag to whatever
            // below senses one, and the gaps under every tab do.
            let sense = if presence.tab_stop {
                Sense::click_and_drag()
            } else {
                Sense::CLICK | Sense::DRAG
            };
            let rect = ui.allocate_space(size).1;
            let response = ui.interact(rect, tab_id(s), sense);
            let selected = s == app.surface;
            response.widget_info(|| {
                WidgetInfo::selected(WidgetType::Button, true, selected, s.label())
            });
            ui.ctx().accesskit_node_builder(response.id, |n| {
                n.set_role(accesskit::Role::Tab);
                n.set_selected(selected);
                n.clear_toggled();
            });
            // The colour follows hover and selection over 0.2 s
            // (`.tab { transition: color }`); the underline does not move.
            let lit = app.ease(
                response.id.with("ink"),
                if selected || response.hovered() {
                    1.0
                } else {
                    0.0
                },
                motion::STATE,
            );
            let ink = super::bookview::mix(t.ink_muted, t.ink, lit);
            let offset = Vec2::new(0.0, (1.0 - k) * -motion::TAB_RISE);
            ui.painter()
                .galley(rect.min + offset, galley, ink.gamma_multiply(k));
            if selected {
                ui.painter().hline(
                    rect.x_range(),
                    rect.bottom() - 0.75,
                    Stroke::new(1.5, t.brass),
                );
            }
            widgets::focus_ring(ui, &t, &response, 3.0);
            if response.clicked() {
                app.go_to(s);
            }
            widgets::own_arrows(ui, response.id);
            if response.has_focus() {
                let key = ui.input_mut(|i| {
                    if i.consume_key(egui::Modifiers::NONE, Key::ArrowRight) {
                        Some(TabKey::Right)
                    } else if i.consume_key(egui::Modifiers::NONE, Key::ArrowLeft) {
                        Some(TabKey::Left)
                    } else if i.consume_key(egui::Modifiers::NONE, Key::Home) {
                        Some(TabKey::Home)
                    } else if i.consume_key(egui::Modifiers::NONE, Key::End) {
                        Some(TabKey::End)
                    } else {
                        None
                    }
                });
                if let Some(key) = key
                    && let Some(next) = tab_step(key, app.surface, |x| app.tab_available(x))
                {
                    app.go_to(next);
                    app.restore_focus = false;
                    ui.memory_mut(|m| m.request_focus(tab_id(next)));
                }
            }
        }
    });
    ui.ctx().accesskit_node_builder(bar.response.id, |n| {
        n.set_role(accesskit::Role::TabList);
        n.set_bounds(widgets::bounds(bar.response.rect));
        n.set_label(crate::model::names::PRODUCT);
    });
}

/// How tall the foot `sheet` draws below a surface is: the gap above it,
/// the dead-engine line if there is one, the stamp's row (as tall as the
/// update label, its taller member) and the margin under it, with the
/// spacing egui puts after each.
fn foot_height(app: &App, ui: &Ui, block: layout::Span) -> f32 {
    let gap = ui.spacing().item_spacing.y;
    let font = theme::structure(size::FINE);
    let row = ui.fonts_mut(|f| f.row_height(&font)) + layout::SPACE[1];
    let fault = app.engine_fault.as_ref().map_or(0.0, |fault| {
        let galley = ui.painter().layout(
            fault.clone(),
            theme::structure(size::NOTE),
            Color32::PLACEHOLDER,
            block.width.min(66.0 * 8.0),
        );
        galley.size().y + gap
    });
    layout::SPACE[10] + fault + row + gap + layout::SPACE[5]
}

/// The rev stamp: a printer's mark at the foot of the page, and a button.
/// The stamp's room, empty: what [`stamp_button`] takes, drawn by nothing.
fn stamp_space(ui: &mut Ui, t: &Tokens, words: &str) {
    let font = theme::structure(size::FINE);
    let galley = ui
        .painter()
        .layout_no_wrap(widgets::caps(words), font, t.ink_muted);
    ui.allocate_exact_size(galley.size(), Sense::hover());
}

fn stamp_button(ui: &mut Ui, t: &Tokens, words: &str) -> egui::Response {
    let font = theme::structure(size::FINE);
    let galley = ui
        .painter()
        .layout_no_wrap(widgets::caps(words), font, t.ink_muted);
    let (rect, response) = ui.allocate_exact_size(galley.size(), Sense::click());
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, words));
    let ink = if response.hovered() {
        t.ink
    } else {
        t.ink_muted
    };
    ui.painter()
        .galley_with_override_text_color(rect.min, galley, ink);
    widgets::focus_ring(ui, t, &response, 3.0);
    response
}

/// The update label: the stamp's type in ink, a brass rule under it; inert
/// moments are disabled to the screen reader and take no pointer.
fn update_button(ui: &mut Ui, t: &Tokens, words: &str, actionable: bool) -> egui::Response {
    let font = theme::structure(size::FINE);
    let galley = ui
        .painter()
        .layout_no_wrap(widgets::caps(words), font, t.ink);
    let size = galley.size() + Vec2::new(0.0, layout::SPACE[1]);
    let (rect, response) = ui.allocate_exact_size(
        size,
        if actionable {
            Sense::click()
        } else {
            Sense::focusable_noninteractive()
        },
    );
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, actionable, words));
    ui.painter().galley(rect.min, galley, t.ink);
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - 0.75,
        Stroke::new(1.5, t.brass),
    );
    widgets::focus_ring(ui, t, &response, 3.0);
    response
}

fn heads_up(app: &mut App, ui: &mut Ui) {
    let motion_id = Id::new("heads-up-in");
    let Some(device) = app.heads_up.offered.clone() else {
        app.forget_motion(motion_id);
        return;
    };
    // heads-up-in: it takes its place in the page at once, and slides down
    // half its own height into it as it fades in.
    let k = app.ease_from(motion_id, 0.0, 1.0, motion::HEADS_UP_IN);
    let height_id = Id::new("heads-up-height");
    let height = ui
        .memory(|m| m.data.get_temp::<f32>(height_id))
        .unwrap_or(0.0);
    let top = ui.cursor().min;
    let width = ui.available_width();
    let mut slid = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_size(
                top + Vec2::new(0.0, -(1.0 - k) * height * 0.5),
                Vec2::new(width, f32::INFINITY),
            ))
            .layout(*ui.layout()),
    );
    slid.multiply_opacity(k);
    let t = app.tokens();
    let title = app
        .script
        .as_ref()
        .map(|s| s.title.clone())
        .unwrap_or_default();
    widgets::panel_frame(&t)
        .inner_margin(egui::Margin::symmetric(16, 12))
        .show(&mut slid, |ui| {
            ui.horizontal_wrapped(|ui| {
                widgets::live(
                    ui,
                    Id::new("heads-up"),
                    &heads_up_line(&title),
                    theme::structure(size::UI),
                    t.ink,
                );
                let copy = widgets::button(ui, &t, Look::Brass, HEADS_UP_COPY, true);
                let later = widgets::button(ui, &t, Look::Quiet, HEADS_UP_LATER, true);
                if copy.clicked() {
                    app.heads_up.close();
                    app.restore_focus = true;
                    if !app.convert.converting {
                        app.go_to(Surface::Convert);
                        convert::show_result(app);
                        panel::copy_to(app, device.clone());
                    }
                }
                if later.clicked() {
                    app.heads_up.close();
                    app.restore_focus = true;
                }
            });
        });
    let drawn = slid.min_rect().height();
    ui.memory_mut(|m| m.data.insert_temp(height_id, drawn));
    ui.advance_cursor_after_rect(Rect::from_min_size(top, Vec2::new(width, drawn)));
    ui.add_space(layout::SPACE[4]);
}

// -------------------------------------------------------------- the notes

fn notes_sheet(app: &mut App, ui: &mut Ui) {
    let motion_id = Id::new("sheet-rise");
    if !app.notes_open {
        app.forget_motion(motion_id);
        return;
    }
    let t = app.tokens();
    let ctx = ui.ctx().clone();
    // sheet-rise: the sheet comes up 16px as it fades in; the dimming
    // behind it is there at once.
    let k = app.ease_from(motion_id, 0.0, 1.0, motion::SHEET_RISE);
    let area = egui::Modal::default_area(Id::new("release-notes"))
        .fade_in(false)
        .anchor(
            egui::Align2::CENTER_CENTER,
            Vec2::new(0.0, (1.0 - k) * motion::SHEET_RISE_PX),
        );
    let modal = egui::Modal::new(Id::new("release-notes"))
        .area(area)
        .frame(
            egui::Frame::NONE
                .fill(t.paper.gamma_multiply(k))
                .corner_radius(CornerRadius::same(theme::RADIUS_WELL as u8))
                .inner_margin(egui::Margin::symmetric(32, 19)),
        )
        .backdrop_color(Color32::from_black_alpha(107))
        .show(&ctx, |ui| {
            ui.multiply_opacity(k);
            let width = (44.0 * 16.0f32).min(ctx.content_rect().width() * 0.88);
            ui.set_width(width);
            ui.ctx().accesskit_node_builder(ui.id(), |n| {
                n.set_role(accesskit::Role::Dialog);
                n.set_label("Release notes");
            });
            let mut close = false;
            ui.with_layout(Layout::top_down(Align::Max), |ui| {
                close = widgets::button(ui, &t, Look::Quiet, "Close", true).clicked();
            });
            egui::ScrollArea::vertical()
                .max_height(ctx.content_rect().height() * 0.82 - 80.0)
                .show(ui, |ui| {
                    widgets::slug(
                        ui,
                        &t,
                        &format!("{} {}", crate::model::names::PRODUCT, RELEASE.version),
                    );
                    widgets::prose(ui, &t, RELEASE.headline);
                    for section in visible_sections(RELEASE.sections) {
                        ui.add_space(layout::SPACE[6]);
                        let h = widgets::state_label(ui, &t, section.title);
                        ui.ctx()
                            .accesskit_node_builder(h.id, |n| n.set_role(accesskit::Role::Heading));
                        for item in section.items {
                            let mut job = egui::text::LayoutJob::default();
                            if !item.lead.is_empty() {
                                job.append(
                                    item.lead,
                                    0.0,
                                    egui::TextFormat {
                                        font_id: theme::structure_bold(size::UI),
                                        color: t.ink,
                                        ..Default::default()
                                    },
                                );
                                job.append(
                                    " ",
                                    0.0,
                                    egui::TextFormat {
                                        font_id: theme::prose(size::BODY),
                                        color: t.ink_soft,
                                        ..Default::default()
                                    },
                                );
                            }
                            job.append(
                                item.body,
                                0.0,
                                egui::TextFormat {
                                    font_id: theme::prose(size::BODY),
                                    color: t.ink_soft,
                                    ..Default::default()
                                },
                            );
                            ui.add(egui::Label::new(job).wrap());
                        }
                    }
                    updates_block(app, ui, &t);
                });
            close
        });
    if modal.inner || modal.should_close() {
        app.notes_open = false;
        app.restore_focus = true;
    }
}

/// The Updates block: in the release notes, and the whole of the sheet
/// Help > Check for Updates… opens. False when this copy cannot update
/// itself (and the block said where releases are instead).
pub(super) fn updates_block(app: &mut App, ui: &mut Ui, t: &Tokens) -> bool {
    ui.add_space(layout::SPACE[6]);
    if !app.update.usable(&app.host) {
        widgets::caption(ui, t, u::NOT_HERE, false);
        return false;
    }
    widgets::state_label(ui, t, "Updates");
    let mut on = app.update.automatic(&app.host);
    let box_ = ui.checkbox(&mut on, u::AUTO_LABEL);
    let says = widgets::caption(ui, t, u::AUTO_SAYS, false);
    widgets::described_by(ui, box_.id, says.id);
    if box_.changed() {
        app.update.set_automatic(&mut app.host, on);
        app.update.notes_line = Some(if on { u::AUTO_ON } else { u::AUTO_OFF }.into());
    }
    let view = notes_view(app.update.phase(), &app.update.drawn, RELEASE.version);
    let check = widgets::button(ui, t, Look::Small, u::CHECK_BUTTON, !view.check_disabled);
    if check.clicked() {
        app.update.drawn.drew_offer = true;
        // A failure's message reaches the notes line when the answer comes.
        app.update.check(&mut app.host, true);
    }
    let view = notes_view(app.update.phase(), &app.update.drawn, RELEASE.version);
    let said = app
        .update
        .notes_line
        .clone()
        .or(view.say)
        .unwrap_or_default();
    widgets::live(
        ui,
        Id::new("update-status"),
        &said,
        theme::prose(size::CAPTION),
        t.ink_muted,
    );
    if let Some(body) = view.body {
        widgets::prose(ui, t, &body);
    }
    if let Some((words, disabled)) = view.install {
        let b = widgets::button(ui, t, Look::Brass, &words, !disabled);
        if b.clicked() {
            let gate = app.busy.gate();
            app.update.start(&app.host, gate);
        }
    }
    true
}

// --------------------------------------------------------------- the rail

/// Two brads and the punched hole, fixed to the window; painted, hidden
/// from assistive technology.
fn paint_rail(ui: &Ui, t: &Tokens, window: Vec2) {
    let painter = ui
        .ctx()
        .layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new("rail")));
    for f in layout::rail(window.x, window.y) {
        let rect = Rect::from_center_size(Pos2::new(f.x, f.y), Vec2::splat(f.size));
        if f.punch {
            punch(&painter, t, rect);
        } else {
            brad(&painter, t, rect);
        }
    }
}

/// An SVG radialGradient in objectBoundingBox units, focal point included.
struct Grad<'a> {
    cx: f32,
    cy: f32,
    r: f32,
    fx: f32,
    fy: f32,
    stops: &'a [(f32, Color32)],
}

fn stop_color(stops: &[(f32, Color32)], t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mut prev = stops[0];
    if t <= prev.0 {
        return prev.1;
    }
    for &next in &stops[1..] {
        if t <= next.0 {
            let k = (t - prev.0) / (next.0 - prev.0).max(1e-6);
            return super::bookview::mix(prev.1, next.1, k);
        }
        prev = next;
    }
    prev.1
}

/// Fill an ellipse (centre, radii, rotation in degrees, all in the
/// symbol's 100-unit box mapped onto `rect`) with a flat colour or a
/// radial gradient.
fn ellipse(
    painter: &egui::Painter,
    rect: Rect,
    shape: [f32; 5],
    fill: Color32,
    grad: Option<&Grad>,
) {
    let [cx, cy, rx, ry, rot] = shape;
    let c = Pos2::new(cx, cy);
    let map = |p: Pos2| rect.min + p.to_vec2() * (rect.width() / 100.0);
    let (sin, cos) = rot.to_radians().sin_cos();
    let at = |u: f32, v: f32| Pos2::new(c.x + u * cos - v * sin, c.y + u * sin + v * cos);
    let color_at = |u: f32, v: f32| -> Color32 {
        let Some(g) = grad else { return fill };
        // In the ellipse's own bounding box, before rotation.
        let p = Vec2::new((u + rx) / (2.0 * rx), (v + ry) / (2.0 * ry));
        let (f, ce) = (Vec2::new(g.fx, g.fy), Vec2::new(g.cx, g.cy));
        let d = ce - f;
        let q = p - f;
        let a = d.length_sq() - g.r * g.r;
        let qd = q.dot(d);
        let disc = (qd * qd - a * q.length_sq()).max(0.0);
        let t = if a.abs() < 1e-6 {
            0.0
        } else {
            (qd - disc.sqrt()) / a
        };
        stop_color(g.stops, t)
    };
    let rings = 14;
    let segments = 48;
    // The fan starts at the gradient's focal point, so its first stop (a
    // specular point) is a vertex and every ray runs the stops in order.
    let (u0, v0) = match grad {
        Some(g) => ((g.fx * 2.0 - 1.0) * rx, (g.fy * 2.0 - 1.0) * ry),
        None => (0.0, 0.0),
    };
    let mut mesh = Mesh::default();
    mesh.vertices.push(Vertex {
        pos: map(at(u0, v0)),
        uv: egui::epaint::WHITE_UV,
        color: color_at(u0, v0),
    });
    for ring in 1..=rings {
        let k = ring as f32 / rings as f32;
        for s in 0..segments {
            let a = s as f32 / segments as f32 * std::f32::consts::TAU;
            let (eu, ev) = (a.cos() * rx, a.sin() * ry);
            let (u, v) = (u0 + (eu - u0) * k, v0 + (ev - v0) * k);
            mesh.vertices.push(Vertex {
                pos: map(at(u, v)),
                uv: egui::epaint::WHITE_UV,
                color: color_at(u, v),
            });
        }
    }
    let idx = |ring: u32, s: u32| 1 + (ring - 1) * segments + (s % segments);
    for s in 0..segments {
        mesh.add_triangle(0, idx(1, s), idx(1, s + 1));
    }
    for ring in 1..rings as u32 {
        for s in 0..segments {
            mesh.add_triangle(idx(ring, s), idx(ring + 1, s), idx(ring + 1, s + 1));
            mesh.add_triangle(idx(ring, s), idx(ring + 1, s + 1), idx(ring, s + 1));
        }
    }
    painter.add(mesh);
}

/// brand/components/brad.html, layer by layer.
fn brad(painter: &egui::Painter, t: &Tokens, rect: Rect) {
    let black = |a: f32| Color32::from_black_alpha((a * 255.0) as u8);
    let white = |a: f32| Color32::from_white_alpha((a * 255.0) as u8);
    let cast = Grad {
        cx: 0.5,
        cy: 0.5,
        r: 0.5,
        fx: 0.5,
        fy: 0.5,
        stops: &[(0.52, black(0.36)), (1.0, black(0.0))],
    };
    ellipse(
        painter,
        rect,
        [53.0, 56.0, 47.0, 46.0, 0.0],
        Color32::TRANSPARENT,
        Some(&cast),
    );
    ellipse(
        painter,
        rect,
        [50.0, 50.0, 41.0, 41.0, 0.0],
        t.brass_edge,
        None,
    );
    let face_stops = [
        (0.0, t.brass_specular),
        (0.17, t.brass_highlight),
        (0.45, t.brass),
        (0.76, t.brass_shadow),
        (1.0, t.brass_rim),
    ];
    let face = Grad {
        cx: 0.5,
        cy: 0.5,
        r: 0.58,
        fx: 0.33,
        fy: 0.27,
        stops: &face_stops,
    };
    ellipse(
        painter,
        rect,
        [50.0, 50.0, 39.5, 39.5, 0.0],
        t.brass,
        Some(&face),
    );
    ellipse(
        painter,
        rect,
        [63.0, 67.0, 15.0, 8.0, -28.0],
        t.brass_bounce.gamma_multiply(0.34),
        None,
    );
    let spec = Grad {
        cx: 0.5,
        cy: 0.5,
        r: 0.5,
        fx: 0.5,
        fy: 0.5,
        stops: &[(0.0, white(0.88)), (0.55, white(0.2)), (1.0, white(0.0))],
    };
    ellipse(
        painter,
        rect,
        [38.0, 33.0, 17.0, 11.0, -28.0],
        Color32::TRANSPARENT,
        Some(&spec),
    );
    painter.circle_stroke(
        rect.center(),
        39.5 * rect.width() / 100.0,
        Stroke::new(
            1.6 * rect.width() / 100.0,
            t.brass_edge.gamma_multiply(0.55),
        ),
    );
}

/// The punched hole: the paper's lip, the desk through it, its shade.
fn punch(painter: &egui::Painter, t: &Tokens, rect: Rect) {
    ellipse(painter, rect, [50.0, 52.4, 31.5, 31.5, 0.0], t.paper, None);
    ellipse(painter, rect, [50.0, 50.0, 30.0, 30.0, 0.0], t.ground, None);
    let shade = Grad {
        cx: 0.5,
        cy: 0.4,
        r: 0.54,
        fx: 0.5,
        fy: 0.4,
        stops: &[
            (0.7, Color32::from_black_alpha(153)),
            (1.0, Color32::from_black_alpha(0)),
        ],
    };
    ellipse(
        painter,
        rect,
        [50.0, 50.0, 30.0, 30.0, 0.0],
        Color32::TRANSPARENT,
        Some(&shade),
    );
}
