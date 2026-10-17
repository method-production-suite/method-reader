//! The page writes itself (was `desktop/ui/page-show.js`, below its line).
//! Its clock is page-timing's; its words are page-show's.

use egui::text::{LayoutJob, TextFormat};
use egui::{FontId, Id, Pos2, Rect, Sense, Ui, Vec2, WidgetInfo, WidgetType};
use serde_json::Value;

use super::App;
use super::bookview::{BOOK_EM, View, mix, sheet};
use super::widgets;
use crate::model::layout;
use crate::model::motion::{GLOW, PAGE_LIFT};
use crate::model::page_show::{FADE_MS, HEADING, Reading, align_blocks, reading_line};
use crate::model::page_timing::{Frame, Heard, Plan, form_at, frame_at, plan_show, sounds_due};
use crate::model::read::PAGE_MARGIN_INLINE_PCT;
use crate::theme::{self, Tokens, size};

pub struct Show {
    plan: Plan,
    name: String,
    start: f64,
    now: f64,
    book_at: Option<f64>,
    skipped: bool,
    over: bool,
    fade_from: Option<f64>,
    reading: Reading,
    typed_before: usize,
    heard: Heard,
    sources: Option<Vec<Option<usize>>>,
    sounds: bool,
    /// The keys and bells sounded (what the speaker would have played).
    pub keys: u32,
    pub dings: u32,
}

impl Show {
    pub fn begin(paragraphs: &[String], name: String, now: f64, sounds: bool) -> Self {
        Self {
            plan: plan_show(paragraphs),
            name,
            start: now,
            now,
            book_at: None,
            skipped: false,
            over: false,
            fade_from: None,
            reading: Reading::default(),
            typed_before: 0,
            heard: Heard::default(),
            sources: None,
            sounds,
            keys: 0,
            dings: 0,
        }
    }

    pub fn progress(&mut self, line: &Value) {
        if !self.over {
            self.reading = reading_line(&self.reading, line);
        }
    }

    pub fn slim_line(&self) -> &str {
        &self.reading.line
    }

    /// The book exists: pair the typed paragraphs with its blocks.
    /// `book` is the words of the book's headings and paragraphs (`h2, p`)
    /// in order, without their page marks; None when it sent no pages.
    pub fn adopt(&mut self, book: Option<&[String]>, now: f64) {
        if self.over {
            return;
        }
        self.sources = Some(match book {
            Some(words) => {
                let typed: Vec<String> = self.plan.blocks.iter().map(|b| b.text.clone()).collect();
                align_blocks(&typed, words)
            }
            None => vec![],
        });
        self.book_at = Some(now - self.start);
        if self.skipped {
            self.finish(false, now);
        }
    }

    /// A click anywhere, or any key.
    pub fn press(&mut self, now: f64) {
        if self.over {
            return;
        }
        if self.book_at.is_some() {
            self.finish(false, now);
        } else {
            self.skipped = true;
        }
    }

    fn finish(&mut self, fade: bool, now: f64) {
        if self.over {
            return;
        }
        self.over = true;
        self.fade_from = fade.then_some(now);
    }

    /// One tick of the clock: sounds due, blocks formed, the end.
    pub fn tick(&mut self, now: f64) {
        self.now = now;
        if self.over {
            return;
        }
        let t = now - self.start;
        let frame = self.frame();
        let typed = if self.skipped {
            self.plan.total
        } else {
            frame.typed
        };
        if self.sounds && !self.skipped {
            let due = sounds_due(&self.plan, self.typed_before, typed, t, self.heard);
            self.keys += u32::from(due.key);
            self.dings += u32::from(due.ding);
            self.heard = due.heard;
        }
        self.typed_before = typed;
        if frame.done {
            self.finish(true, now);
        }
    }

    pub fn frame(&self) -> Frame {
        frame_at(
            &self.plan,
            self.book_at.filter(|_| self.sources.is_some()),
            self.now - self.start,
        )
    }

    /// Whether the show is still on (typing, forming, or lifting away).
    pub fn running(&self) -> bool {
        !self.over || self.fade_from.is_some_and(|f| self.now - f < FADE_MS)
    }

    /// Whether the typed page is drawn over the result's page.
    pub fn covers_page(&self) -> bool {
        self.running()
    }

    /// The typed page's opacity: whole while it runs, lifting over 250 ms,
    /// eased out (surfaces.css `.page-show-frame`).
    pub fn page_opacity(&self) -> f32 {
        match (self.over, self.fade_from) {
            (false, _) => 1.0,
            (true, Some(from)) => 1.0 - PAGE_LIFT.progress(self.now - from),
            (true, None) => 0.0,
        }
    }

    /// How brightly block `i` glows: 1 the moment it forms, out to 0 over
    /// 600 ms eased out (page-show.js `page-show-glow`); 0 before it forms.
    pub fn glow(&self, i: usize) -> f32 {
        let t_ms = self.now - self.start;
        form_at(&self.plan, self.book_at, i)
            .filter(|&at| at <= t_ms && i < self.frame().formed)
            .map_or(0.0, |at| 1.0 - GLOW.progress(t_ms - at))
    }

    /// Which of the book's blocks each typed paragraph became (None: it
    /// only glows), once the book has arrived.
    pub fn sources(&self) -> Option<&[Option<usize>]> {
        self.sources.as_deref()
    }

    pub fn book_arrived(&self) -> bool {
        self.book_at.is_some()
    }

    /// The page's whole text, for the screen reader from the first moment.
    pub fn words(&self) -> Vec<String> {
        self.plan.blocks.iter().map(|b| b.text.clone()).collect()
    }
}

/// The show, ended and forgotten.
pub fn stop(app: &mut App) {
    app.show = None;
}

/// Before the book exists: the typed page in the result's box, the side
/// saying Converting and the file, and the slim line under the page.
pub fn draw(app: &mut App, ui: &mut Ui, block: layout::Span) {
    let t = app.tokens();
    let Some(show) = app.show.as_ref() else {
        return;
    };
    let geometry = layout::result(layout::Span {
        left: 0.0,
        width: block.width,
    });
    ui.add_space(layout::SPACE[8]);
    let top = ui.cursor().min;
    let page_h = geometry.page.width * 4.0 / 3.0;
    let side_rect = Rect::from_min_size(
        top + Vec2::new(geometry.side.left, 0.0),
        Vec2::new(geometry.side.width, 200.0),
    );
    let side = ui.scope_builder(egui::UiBuilder::new().max_rect(side_rect), |ui| {
        widgets::heading(ui, HEADING, theme::chrome_strong(size::BODY), t.ink);
        widgets::text(ui, &show.name, theme::chrome(size::UI), t.ink_muted);
        // The page's words, whole, for a screen reader: the typing is for eyes.
        for (i, words) in show.words().iter().enumerate() {
            let r = ui.interact(
                Rect::from_min_size(ui.cursor().min, Vec2::ZERO),
                Id::new(("show-words", i)),
                Sense::hover(),
            );
            r.widget_info(|| WidgetInfo::labeled(WidgetType::Label, true, words));
        }
    });
    let page_top = if geometry.stacked {
        side.response.rect.bottom() + layout::SPACE[7]
    } else {
        top.y
    };
    let page = Rect::from_min_size(
        Pos2::new(top.x + geometry.page.left, page_top),
        Vec2::new(geometry.page.width, page_h),
    );
    sheet(ui, &t, page);
    paint_typed(ui, &t, page, show, None, 1.0);
    let line_rect = Rect::from_min_size(
        Pos2::new(page.left(), page.bottom() + layout::SPACE[2]),
        Vec2::new(page.width(), 20.0),
    );
    let words = show.slim_line().to_string();
    ui.scope_builder(egui::UiBuilder::new().max_rect(line_rect), |ui| {
        ui.vertical_centered(|ui| {
            widgets::live(
                ui,
                Id::new("page-show-line"),
                &words,
                theme::chrome(size::LABEL),
                t.ink_muted,
            );
        });
    });
    ui.allocate_rect(
        Rect::from_min_max(
            top,
            Pos2::new(
                top.x + block.width,
                line_rect.bottom().max(side.response.rect.bottom()),
            ),
        ),
        Sense::hover(),
    );
}

/// The typed page: each paragraph as far as it is typed, plain and left
/// aligned; a formed block in the book's own form (its slice of the book,
/// as Blitz drew it), glowing brass.
pub fn paint_typed(
    ui: &Ui,
    t: &Tokens,
    rect: Rect,
    show: &Show,
    mut book: Option<&mut View>,
    opacity: f32,
) {
    if opacity <= 0.0 {
        return;
    }
    let clip = rect.shrink(1.0);
    let painter = ui.painter().with_clip_rect(clip);
    sheet(
        ui,
        &Tokens {
            hole: t.hole.gamma_multiply(opacity),
            paper: t.paper.gamma_multiply(opacity),
            ..*t
        },
        rect,
    );
    let frame = show.frame();
    let inline = rect.width() * PAGE_MARGIN_INLINE_PCT / 100.0;
    let width = rect.width() - 2.0 * inline;
    let mut y = rect.top() + 1.5 * BOOK_EM;
    for (i, block) in show.plan.blocks.iter().enumerate() {
        let formed = i < frame.formed;
        let glow = show.glow(i);
        let ink = mix(t.ink, t.brass, glow).gamma_multiply(opacity);
        let source = show
            .sources
            .as_ref()
            .and_then(|s| s.get(i).copied().flatten());
        if formed && let (Some(j), Some(view)) = (source, book.as_deref_mut()) {
            let tint = mix(egui::Color32::WHITE, t.brass, glow * 0.6).gamma_multiply(opacity);
            let h = view.paint_block(ui, j, Pos2::new(rect.left(), y), clip, tint);
            y += h + BOOK_EM;
            continue;
        }
        let shown = if show.skipped {
            block.text.clone()
        } else {
            frame.shown.get(i).cloned().unwrap_or_default()
        };
        let mut job = LayoutJob::single_section(
            shown,
            TextFormat {
                font_id: FontId::new(BOOK_EM, egui::FontFamily::Name(theme::STRUCTURE.into())),
                color: ink,
                ..Default::default()
            },
        );
        job.wrap.max_width = width;
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        let h = galley.size().y.max(BOOK_EM);
        painter.galley(Pos2::new(rect.left() + inline, y), galley, ink);
        y += h + BOOK_EM;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn book() -> Vec<String> {
        vec![
            "EXT. RIDGE ROAD - DAY".into(),
            "DELACROIX".into(),
            "Thirty-one.".into(),
        ]
    }

    fn paragraphs() -> Vec<String> {
        vec![
            "INT. FIELD STATION - DAY".into(),
            "MARA".into(),
            "Again?".into(),
        ]
    }

    #[test]
    fn it_types_forms_and_lifts_away_by_itself() {
        let mut show = Show::begin(&paragraphs(), "a.pdf".into(), 0.0, true);
        show.tick(2000.0);
        assert!(show.running() && show.keys > 0);
        show.adopt(Some(&book()), 2500.0);
        show.tick(4000.0 + 1200.0 + 600.0 + 1.0);
        show.tick(4000.0 + 1200.0 + 600.0 + 100.0);
        assert!(show.running(), "lifting away");
        assert!(show.page_opacity() < 1.0);
        show.tick(4000.0 + 1200.0 + 600.0 + 300.0);
        assert!(!show.running());
    }

    #[test]
    fn a_press_before_the_book_types_the_page_whole_and_ends_when_it_arrives() {
        let mut show = Show::begin(&paragraphs(), "a.pdf".into(), 0.0, false);
        show.tick(100.0);
        show.press(100.0);
        assert!(show.running());
        show.adopt(Some(&book()), 300.0);
        assert!(!show.running(), "over the moment the book is there");
        assert_eq!(show.page_opacity(), 0.0);
    }

    #[test]
    fn a_press_after_the_book_ends_it_at_once() {
        let mut show = Show::begin(&paragraphs(), "a.pdf".into(), 0.0, false);
        show.adopt(None, 10.0);
        show.press(20.0);
        assert!(!show.running());
    }

    #[test]
    fn the_slim_line_follows_the_engine() {
        let mut show = Show::begin(&paragraphs(), "a.pdf".into(), 0.0, false);
        assert_eq!(show.slim_line(), "Starting up");
        show.progress(&json!({"stage": "parse", "page": 4, "pages": 18}));
        assert_eq!(show.slim_line(), "Reading page 4 of 18");
    }

    #[test]
    fn the_glow_and_the_lift_ease_out_as_the_old_window_s_did() {
        let mut show = Show::begin(&paragraphs(), "a.pdf".into(), 0.0, false);
        show.adopt(Some(&book()), 0.0);
        // Typing ends at 4000 ms; the last block forms two steps after.
        let at = form_at(&show.plan, show.book_at, 0).expect("formed");
        show.tick(at);
        assert_eq!(show.glow(0), 1.0, "full the moment it forms");
        show.tick(at + 300.0);
        let half = show.glow(0);
        // ease-out at half time is 0.6846 of the way: 0.3154 left, not 0.5.
        assert!((half - 0.3154).abs() < 1e-3, "{half}");
        show.tick(at + 600.0);
        assert_eq!(show.glow(0), 0.0);
        assert_eq!(show.glow(2), 0.0, "not formed yet");
        let end = show.plan.blocks.len() - 1;
        let over = form_at(&show.plan, show.book_at, end).unwrap() + GLOW.ms;
        show.tick(over);
        show.tick(over + 125.0);
        let lift = show.page_opacity();
        assert!((lift - (1.0 - 0.6846)).abs() < 1e-3, "{lift}");
    }

    #[test]
    fn quiet_unless_the_sounds_are_on() {
        let mut show = Show::begin(&paragraphs(), "a.pdf".into(), 0.0, false);
        show.tick(3000.0);
        assert_eq!((show.keys, show.dings), (0, 0));
    }
}
