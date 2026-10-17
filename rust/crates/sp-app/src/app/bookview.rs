//! Where the window shows the book: Read, the Settings preview, the
//! result's first page and the page show. Each is a view of one [`Book`]
//! (the engine's own document laid out by Blitz), kept until the document,
//! the width, the theme or the screen's scale changes.
//!
//! The books live on a thread of their own, the desk: laying a book out
//! takes Blitz 20 to 70 ms, more than a whole frame at 120 Hz, and in the
//! window's own thread it froze whatever was moving (the page show at the
//! moment the book arrived, the tabs as Read or Settings first opened).
//! The window asks; the desk lays the book out and paints it in strips;
//! what comes back is plain data (the layout's facts, and pixels), so the
//! window never waits on Blitz. Until a new layout arrives the view keeps
//! showing the last one. Strips are kept once painted, so scrolling, and
//! anything that moves over the page, only moves textures; and the strips
//! either side of the visible ones are asked for ahead of need. In tests
//! (Mode::Inline) the desk works in the caller's own thread, at once.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use egui::{
    ColorImage, CornerRadius, Id, Rect, Sense, Stroke, StrokeKind, TextureHandle, TextureOptions,
    Ui, Vec2, WidgetInfo, WidgetType,
};

use super::book::{Block, Book};
use crate::model::read::Mark;
use crate::theme::{self, Tokens};

/// The book's base size, for the result's "a line and a half" above the
/// first scene (css.ts sets the body at 1em = 16px).
pub const BOOK_EM: f32 = 16.0;

/// How tall one strip of the book is, in points.
pub const STRIP: f32 = 512.0;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Place {
    Read,
    Tune,
    FirstPage,
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
struct Key {
    html: u64,
    width: u32,
    dark: bool,
    scale: u32,
}

/// What a laid-out book tells the window: made on the desk, plain data.
#[derive(Clone, Debug, PartialEq)]
pub struct Laid {
    pub width: u32,
    /// Device pixels per point the book was laid out for.
    pub scale: f32,
    pub height: f32,
    /// The scenes (`section.scene`), their headings' words, where they are.
    pub scenes: Vec<(Mark, String)>,
    /// Every heading and paragraph (`h2, p`), in order.
    pub blocks: Vec<Block>,
}

impl Laid {
    fn of(book: &Book) -> Self {
        Self {
            width: book.width,
            scale: book.scale,
            height: book.height,
            scenes: book.scenes.clone(),
            blocks: book.blocks.clone(),
        }
    }
}

enum Job {
    Lay {
        place: Place,
        key: Key,
        html: Arc<String>,
        tokens: Tokens,
        scale: f32,
    },
    Strip {
        place: Place,
        key: Key,
        index: u32,
    },
    Block {
        place: Place,
        key: Key,
        index: usize,
    },
}

impl Job {
    fn place(&self) -> Place {
        match self {
            Job::Lay { place, .. } | Job::Strip { place, .. } | Job::Block { place, .. } => *place,
        }
    }
}

enum Done {
    Laid {
        place: Place,
        key: Key,
        laid: Laid,
    },
    Strip {
        place: Place,
        key: Key,
        index: u32,
        image: ColorImage,
    },
    Block {
        place: Place,
        key: Key,
        index: usize,
        image: ColorImage,
    },
}

impl Done {
    fn place(&self) -> Place {
        match self {
            Done::Laid { place, .. } | Done::Strip { place, .. } | Done::Block { place, .. } => {
                *place
            }
        }
    }
}

/// Where the books are: one per place, laid out and painted on request.
#[derive(Default)]
struct Desk {
    books: HashMap<Place, (Key, Book)>,
}

impl Desk {
    fn work(&mut self, job: Job) -> Option<Done> {
        match job {
            Job::Lay {
                place,
                key,
                html,
                tokens,
                scale,
            } => {
                let book = Book::new_scaled(&html, key.width, &tokens, key.dark, scale);
                let laid = Laid::of(&book);
                self.books.insert(place, (key, book));
                Some(Done::Laid { place, key, laid })
            }
            Job::Strip { place, key, index } => {
                let (k, book) = self.books.get_mut(&place)?;
                if *k != key {
                    return None;
                }
                let start = index as f32 * STRIP;
                let part = (book.height - start).clamp(1.0, STRIP).ceil() as u32;
                let image = book.paint(start, part);
                Some(Done::Strip {
                    place,
                    key,
                    index,
                    image,
                })
            }
            Job::Block { place, key, index } => {
                let (k, book) = self.books.get_mut(&place)?;
                if *k != key {
                    return None;
                }
                let rect = book.blocks.get(index)?.rect;
                let image = book.paint(rect.top(), rect.height().max(1.0).round() as u32);
                Some(Done::Block {
                    place,
                    key,
                    index,
                    image,
                })
            }
        }
    }

    /// The desk's thread: works through what was asked, newest layout
    /// first and stale ones dropped, waking the window after each answer.
    fn run(
        mut self,
        jobs: Receiver<Job>,
        done: Sender<Done>,
        wake: Arc<Mutex<Option<egui::Context>>>,
    ) {
        while let Ok(first) = jobs.recv() {
            let mut batch: Vec<Job> = std::iter::once(first).chain(jobs.try_iter()).collect();
            // A layout asked again for the same place (a window being
            // resized) makes every earlier one for that place moot.
            let mut latest: HashMap<Place, usize> = HashMap::new();
            for (i, job) in batch.iter().enumerate() {
                if matches!(job, Job::Lay { .. }) {
                    latest.insert(job.place(), i);
                }
            }
            let mut i = 0;
            batch.retain(|job| {
                let keep = !matches!(job, Job::Lay { .. }) || latest.get(&job.place()) == Some(&i);
                i += 1;
                keep
            });
            // Layouts first: everything else waits on them.
            batch.sort_by_key(|job| !matches!(job, Job::Lay { .. }));
            for job in batch {
                if let Some(answer) = self.work(job) {
                    if done.send(answer).is_err() {
                        return;
                    }
                    if let Some(ctx) = wake.lock().ok().and_then(|w| w.clone()) {
                        ctx.request_repaint_after(crate::model::motion::NEXT_FRAME);
                    }
                }
            }
        }
    }
}

/// How a view reaches the desk: in this thread (tests), or on its own.
#[derive(Clone)]
enum Asker {
    Here(Rc<RefCell<Desk>>),
    Away(Sender<Job>),
}

impl Asker {
    /// Here, the answer at once; away, None (it arrives through pump).
    fn ask(&self, job: Job) -> Option<Done> {
        match self {
            Asker::Here(desk) => desk.borrow_mut().work(job),
            Asker::Away(tx) => {
                let _ = tx.send(job);
                None
            }
        }
    }
}

pub struct Views {
    views: HashMap<Place, View>,
    /// The screen's device pixels per point, set by the window each frame.
    pub scale: f32,
    asker: Asker,
    answers: Option<Receiver<Done>>,
    wake: Arc<Mutex<Option<egui::Context>>>,
}

impl Default for Views {
    fn default() -> Self {
        Self::new(false)
    }
}

fn hash(html: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    html.hash(&mut h);
    h.finish()
}

impl Views {
    /// With `threaded`, the desk gets a thread of its own (the window);
    /// without, it works in the caller's thread (tests).
    pub fn new(threaded: bool) -> Self {
        let wake: Arc<Mutex<Option<egui::Context>>> = Arc::default();
        let (asker, answers) = if threaded {
            let (jobs_tx, jobs_rx) = channel();
            let (done_tx, done_rx) = channel();
            let w = wake.clone();
            std::thread::Builder::new()
                .name("method-reader-book".into())
                .spawn(move || Desk::default().run(jobs_rx, done_tx, w))
                .expect("the book's thread");
            (Asker::Away(jobs_tx), Some(done_rx))
        } else {
            (Asker::Here(Rc::default()), None)
        };
        Self {
            views: HashMap::new(),
            scale: 1.0,
            asker,
            answers,
            wake,
        }
    }

    /// The window to wake when the desk answers.
    pub fn set_repaint(&self, ctx: egui::Context) {
        if let Ok(mut w) = self.wake.lock() {
            *w = Some(ctx);
        }
    }

    /// The desk's answers since the last frame, taken in. True when any
    /// arrived (the window draws again to show them).
    pub fn pump(&mut self, ctx: &egui::Context) -> bool {
        let Some(rx) = &self.answers else {
            return false;
        };
        let answers: Vec<Done> = rx.try_iter().collect();
        let any = !answers.is_empty();
        for done in answers {
            if let Some(view) = self.views.get_mut(&done.place()) {
                view.take(ctx, done);
            }
        }
        any
    }

    /// The view of `html` at `width` in this theme. A change in any of
    /// them asks for a new layout; the view shows the last one until it
    /// comes.
    pub fn get(
        &mut self,
        place: Place,
        html: &str,
        width: f32,
        t: &Tokens,
        dark: bool,
    ) -> &mut View {
        let key = Key {
            html: hash(html),
            width: width.max(1.0).round() as u32,
            dark,
            scale: (self.scale * 100.0).round() as u32,
        };
        let asker = self.asker.clone();
        let view = self.views.entry(place).or_insert_with(|| View {
            place,
            asker: asker.clone(),
            wanted: None,
            laid: None,
            strips: HashMap::new(),
            asked: HashSet::new(),
            blocks: HashMap::new(),
            blocks_asked: HashSet::new(),
        });
        if view.wanted != Some(key) {
            view.wanted = Some(key);
            let job = Job::Lay {
                place,
                key,
                html: Arc::new(html.to_string()),
                tokens: *t,
                scale: self.scale,
            };
            if let Some(done) = asker.ask(job) {
                view.take_here(done);
            }
        }
        view
    }

    /// The view at `place`, once it has a layout to show.
    pub fn peek(&self, place: Place) -> Option<&View> {
        self.views.get(&place).filter(|v| v.laid.is_some())
    }

    /// The view at `place`, once its layout is the one last asked for.
    pub fn peek_fresh(&self, place: Place) -> Option<&View> {
        self.views.get(&place).filter(|v| v.fresh())
    }

    pub fn peek_fresh_mut(&mut self, place: Place) -> Option<&mut View> {
        self.views.get_mut(&place).filter(|v| v.fresh())
    }

    pub fn forget(&mut self) {
        self.views.clear();
    }
}

pub struct View {
    place: Place,
    asker: Asker,
    /// The layout last asked for.
    wanted: Option<Key>,
    /// The layout shown, and what it says.
    laid: Option<(Key, Laid)>,
    strips: HashMap<u32, TextureHandle>,
    asked: HashSet<u32>,
    blocks: HashMap<usize, TextureHandle>,
    blocks_asked: HashSet<usize>,
}

impl View {
    /// What the shown layout says, once there is one.
    pub fn laid(&self) -> Option<&Laid> {
        self.laid.as_ref().map(|(_, l)| l)
    }

    /// Whether the layout shown is the one last asked for.
    pub fn fresh(&self) -> bool {
        self.laid.as_ref().map(|(k, _)| *k) == self.wanted && self.wanted.is_some()
    }

    /// Whether the layout shown is dressed for the dark theme.
    pub fn dark(&self) -> bool {
        self.laid.as_ref().is_some_and(|(k, _)| k.dark)
    }

    pub fn marks(&self) -> Vec<Mark> {
        self.laid()
            .map(|l| l.scenes.iter().map(|(m, _)| m.clone()).collect())
            .unwrap_or_default()
    }

    /// The book's height in points; 0 before its layout comes.
    pub fn height(&self) -> f32 {
        self.laid().map_or(0.0, |l| l.height)
    }

    /// The words of every heading and paragraph, in order.
    pub fn words(&self) -> Vec<String> {
        self.laid()
            .map(|l| l.blocks.iter().map(|b| b.words.clone()).collect())
            .unwrap_or_default()
    }

    fn take_here(&mut self, done: Done) {
        // In this thread a layout comes back at once; pixels come back
        // through take, which needs the context to make textures.
        if let Done::Laid { key, laid, .. } = done {
            self.laid_out(key, laid);
        }
    }

    fn laid_out(&mut self, key: Key, laid: Laid) {
        if self.wanted != Some(key) {
            return;
        }
        self.laid = Some((key, laid));
        self.strips.clear();
        self.asked.clear();
        self.blocks.clear();
        self.blocks_asked.clear();
    }

    fn shown_key(&self) -> Option<Key> {
        self.laid.as_ref().map(|(k, _)| *k)
    }

    fn take(&mut self, ctx: &egui::Context, done: Done) {
        let name = |what: &str, i: usize| format!("book-{:?}-{what}-{i}", self.place);
        match done {
            Done::Laid { key, laid, .. } => self.laid_out(key, laid),
            Done::Strip {
                key, index, image, ..
            } if self.shown_key() == Some(key) => {
                self.asked.remove(&index);
                let tex =
                    ctx.load_texture(name("strip", index as usize), image, TextureOptions::LINEAR);
                self.strips.insert(index, tex);
            }
            Done::Block {
                key, index, image, ..
            } if self.shown_key() == Some(key) => {
                self.blocks_asked.remove(&index);
                let tex = ctx.load_texture(name("block", index), image, TextureOptions::LINEAR);
                self.blocks.insert(index, tex);
            }
            _ => {}
        }
    }

    fn ask(&mut self, ctx: &egui::Context, job: Job) {
        if let Some(done) = self.asker.ask(job) {
            self.take(ctx, done);
        }
    }

    fn want_strip(&mut self, ctx: &egui::Context, index: u32) {
        let Some(key) = self.shown_key() else {
            return;
        };
        if self.strips.contains_key(&index) || !self.asked.insert(index) {
            return;
        }
        self.ask(
            ctx,
            Job::Strip {
                place: self.place,
                key,
                index,
            },
        );
    }

    /// Ask ahead for the pictures of these blocks (the page show's, before
    /// they form).
    pub fn want_blocks(&mut self, ctx: &egui::Context, indices: impl IntoIterator<Item = usize>) {
        let Some(key) = self.shown_key() else {
            return;
        };
        for index in indices {
            if self.blocks.contains_key(&index) || !self.blocks_asked.insert(index) {
                continue;
            }
            self.ask(
                ctx,
                Job::Block {
                    place: self.place,
                    key,
                    index,
                },
            );
        }
    }

    /// Paint the book from `top` points down into `rect`: the strips it
    /// covers, placed on whole device pixels so the type stays sharp. A
    /// strip not painted yet is asked for (and shows as the paper under
    /// it until it comes); the strips either side are asked for ahead of
    /// need; the rest are let go.
    pub fn paint_slice(&mut self, ui: &Ui, _id: &str, rect: Rect, top: f32) {
        let Some((height, scale)) = self.laid().map(|l| (l.height, l.scale)) else {
            return;
        };
        let n = ((height.max(1.0) / STRIP).ceil() as u32).max(1);
        let top = top.max(0.0);
        let first = ((top / STRIP).floor() as u32).min(n - 1);
        let last = (((top + rect.height()) / STRIP).floor() as u32).min(n - 1);
        let keep = first.saturating_sub(2)..=(last + 2);
        self.strips.retain(|i, _| keep.contains(i));
        self.asked.retain(|i| keep.contains(i));
        let ctx = ui.ctx().clone();
        for i in first..=last {
            self.want_strip(&ctx, i);
        }
        for i in [
            last + 1,
            first.wrapping_sub(1),
            last + 2,
            first.wrapping_sub(2),
        ] {
            if i < n {
                self.want_strip(&ctx, i);
            }
        }
        let ppp = ctx.pixels_per_point();
        let snap = |v: f32| (v * ppp).round() / ppp;
        let painter = ui.painter().with_clip_rect(rect);
        for i in first..=last {
            let Some(tex) = self.strips.get(&i) else {
                continue;
            };
            let [w, h] = tex.size();
            let size = Vec2::new(w as f32, h as f32) / scale;
            let at = egui::pos2(snap(rect.left()), snap(rect.top() + i as f32 * STRIP - top));
            painter.image(
                tex.id(),
                Rect::from_min_size(at, size),
                Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
    }

    /// One block of the book as the book draws it (the page show's formed
    /// blocks), tinted toward `tint` while it glows. Its height, whether or
    /// not its picture has come yet.
    pub fn paint_block(
        &mut self,
        ui: &Ui,
        index: usize,
        at: egui::Pos2,
        clip: Rect,
        tint: egui::Color32,
    ) -> f32 {
        let Some((rect, scale)) = self
            .laid()
            .and_then(|l| l.blocks.get(index).map(|b| (b.rect, l.scale)))
        else {
            return 0.0;
        };
        let ctx = ui.ctx().clone();
        self.want_blocks(&ctx, [index]);
        if let Some(tex) = self.blocks.get(&index) {
            let [w, h] = tex.size();
            let size = Vec2::new(w as f32, h as f32) / scale;
            ui.painter().with_clip_rect(clip).image(
                tex.id(),
                Rect::from_min_size(egui::pos2(clip.left(), at.y), size),
                Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                tint,
            );
        }
        rect.height()
    }

    /// The book's words, block by block, for the screen reader: one node per
    /// heading and paragraph at its place, `origin` being the book's top-left
    /// on screen.
    pub fn speak(&self, ui: &Ui, salt: &str, origin: egui::Pos2, _visible: Rect) {
        let Some(laid) = self.laid() else {
            return;
        };
        for (i, block) in laid.blocks.iter().enumerate() {
            let r = block.rect.translate(origin.to_vec2());
            let node = ui.interact(r, Id::new((salt, "book-block", i)), Sense::hover());
            node.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, &block.words));
        }
    }
}

/// The page as a sheet of paper: hairline, the well's corners, paper.
pub fn sheet(ui: &Ui, t: &Tokens, rect: Rect) {
    let r = CornerRadius::same(theme::RADIUS_WELL as u8);
    ui.painter().rect_filled(rect, r, t.paper);
    ui.painter()
        .rect_stroke(rect, r, Stroke::new(1.0, t.hole), StrokeKind::Inside);
}

/// `a` moved `k` of the way to `b` (premultiplied, as Color32 stores it).
pub fn mix(a: egui::Color32, b: egui::Color32, k: f32) -> egui::Color32 {
    let k = k.clamp(0.0, 1.0);
    let ch = |x: u8, y: u8| (f32::from(x) * (1.0 - k) + f32::from(y) * k).round() as u8;
    egui::Color32::from_rgba_premultiplied(
        ch(a.r(), b.r()),
        ch(a.g(), b.g()),
        ch(a.b(), b.b()),
        ch(a.a(), b.a()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::LIGHT;

    fn oracle() -> String {
        std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../oracle/field-station/book.preview.html"
        ))
        .expect("an oracle preview")
    }

    fn frame(ctx: &egui::Context, run: impl FnMut(&mut Ui)) {
        // No painter here: the textures made are let go unapplied.
        ctx.run_ui(egui::RawInput::default(), run)
            .textures_delta
            .clear();
    }

    #[test]
    fn a_strip_is_painted_once_and_scrolling_only_moves_it() {
        let ctx = egui::Context::default();
        let mut views = Views::new(false);
        let html = oracle();
        let mut tops = vec![];
        for top in [0.0, 3.0, 40.0, 100.0, 200.0, 7.5] {
            frame(&ctx, |ui| {
                let view = views.get(Place::Read, &html, 706.0, &LIGHT, false);
                view.paint_slice(
                    ui,
                    "read",
                    Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(706.0, 300.0)),
                    top,
                );
                tops.push(view.strips.len());
            });
        }
        // Strip 0 and the two ahead of it, painted on the first frame and
        // never again while the reader scrolls inside strip 0.
        assert!(tops.iter().all(|n| *n == 3), "{tops:?}");
        let view = views.peek(Place::Read).expect("laid out");
        assert!(view.fresh());
        assert!(view.height() > 5000.0);
    }

    #[test]
    fn a_new_width_is_laid_out_again_and_its_strips_painted_afresh() {
        let ctx = egui::Context::default();
        let mut views = Views::new(false);
        let html = oracle();
        let rect = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(560.0, 300.0));
        frame(&ctx, |ui| {
            views
                .get(Place::Tune, &html, 560.0, &LIGHT, false)
                .paint_slice(ui, "t", rect, 0.0);
        });
        let narrow = views.peek(Place::Tune).unwrap().height();
        frame(&ctx, |ui| {
            views
                .get(Place::Tune, &html, 420.0, &LIGHT, false)
                .paint_slice(ui, "t", rect, 0.0);
        });
        let view = views.peek(Place::Tune).unwrap();
        assert!(view.height() > narrow, "a narrower column runs longer");
        assert_eq!(view.laid().unwrap().width, 420);
        assert!(view.strips.values().all(|t| t.size()[0] == 420));
    }

    #[test]
    fn on_its_own_thread_the_desk_answers_and_the_view_waits_for_it() {
        let ctx = egui::Context::default();
        let mut views = Views::new(true);
        let html = oracle();
        let view = views.get(Place::FirstPage, &html, 400.0, &LIGHT, false);
        assert!(view.laid().is_none(), "asked, not waited for");
        let mut fresh = false;
        for _ in 0..500 {
            views.pump(&ctx);
            if views.peek_fresh(Place::FirstPage).is_some() {
                fresh = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(fresh, "the layout came back");
        let words = views.peek(Place::FirstPage).unwrap().words();
        assert!(words.iter().any(|w| w.contains("RIDGE")));
    }
}
