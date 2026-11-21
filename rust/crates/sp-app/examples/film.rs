//! The launch film: the real window on the linked engine, driven through a
//! short story as a person would drive it (a PDF dropped on the well, the
//! Preview, a Settings change, the send panel, the Library and its report,
//! one character's sides), drawn offscreen by egui through wgpu as the
//! README pictures are, one frame at a time, with a caption laid over the
//! bottom of each shot. Two opening shots are the site's own comparison
//! picture, and the last is an end card.
//!
//! It writes raw RGBA frames to stdout and nothing else there, so
//! `cargo xtask capture-film` can pipe them into ffmpeg. Every engine call
//! goes through src/capture.rs's film guard: the film's three invented
//! scripts are the only files it converts, the scratch library the only
//! place it writes, and nothing copies, sends or exports. It refuses to
//! start unless METHOD_READER_CONFIG_DIR and METHOD_READER_LIBRARY are set.
//!
//!     METHOD_READER_CONFIG_DIR=<scratch>/config METHOD_READER_LIBRARY=<scratch>/library \
//!       cargo run -p sp-app --example film -- --fixtures <repo>/tests/fixtures \
//!       --compare <compare-light.rgba> --fps 15 > film.rgba
//!
//! Exit codes: 0 filmed, 1 failed, 2 refused its orders.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::accesskit::Role;
use egui::{Align2, Color32, FontId, Pos2, Rect, Vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use sp_app::app::{App, convert, read};
use sp_app::capture::{Guard, PIXELS_PER_POINT, capture_host};
use sp_app::direct::DirectEngine;
use sp_app::model::frame::Surface;
use sp_app::runtime::Mode;

/// The suite's capture frame, in points.
const W: f32 = 1280.0;
const H: f32 = 800.0;
/// The comparison picture's size in pixels (site/img/compare-light.png).
const COMPARE: [usize; 2] = [2560, 1600];
/// The two e-readers in that picture, in its pixels: the PDF's, then the
/// book's, each with a margin of the page around it.
const PDF_DEVICE: [f32; 4] = [540.0, 770.0, 1150.0, 1560.0];
const BOOK_DEVICE: [f32; 4] = [1436.0, 770.0, 2046.0, 1560.0];

// The site's colours (site/assets/tokens.css).
const PAPER: Color32 = Color32::from_rgb(0xF7, 0xF6, 0xF2);
const INK: Color32 = Color32::from_rgb(0x1C, 0x22, 0x28);
const INK_SOFT: Color32 = Color32::from_rgb(0x3A, 0x42, 0x4C);
const ACCENT: Color32 = Color32::from_rgb(0x70, 0x52, 0xBA);

#[derive(Clone, Copy, PartialEq)]
enum Scene {
    App,
    Crop([f32; 4]),
    Card,
}

struct Film {
    app: App,
    scene: Scene,
    caption: Option<&'static str>,
    compare: Vec<u8>,
    /// One texture per crop: the whole picture is wider than a texture
    /// may be.
    textures: Vec<([u32; 4], egui::TextureHandle)>,
}

/// The pixels of `crop` out of the comparison picture.
fn cut(compare: &[u8], crop: [f32; 4]) -> egui::ColorImage {
    let [x0, y0, x1, y1] = crop.map(|v| v as usize);
    let mut rgba = Vec::with_capacity((x1 - x0) * (y1 - y0) * 4);
    for y in y0..y1 {
        let row = (y * COMPARE[0] + x0) * 4;
        rgba.extend_from_slice(&compare[row..row + (x1 - x0) * 4]);
    }
    egui::ColorImage::from_rgba_unmultiplied([x1 - x0, y1 - y0], &rgba)
}

struct Orders {
    fixtures: PathBuf,
    compare: PathBuf,
    library: PathBuf,
    fps: u32,
}

fn orders() -> Result<Orders, String> {
    let mut args = std::env::args().skip(1);
    let (mut fixtures, mut compare, mut fps) = (None, None, 15u32);
    while let Some(a) = args.next() {
        let mut value = || args.next().ok_or(format!("film: {a} needs a value"));
        match a.as_str() {
            "--fixtures" => fixtures = Some(PathBuf::from(value()?)),
            "--compare" => compare = Some(PathBuf::from(value()?)),
            "--fps" => fps = value()?.parse().map_err(|_| "film: --fps is a number")?,
            other => return Err(format!("film: no such order: {other}")),
        }
    }
    let scratch = |k: &str| {
        std::env::var(k)
            .ok()
            .filter(|v| !v.trim().is_empty())
            .ok_or(format!(
                "film: {k} is not set: the film runs only on a scratch library and settings"
            ))
    };
    scratch("METHOD_READER_CONFIG_DIR")?;
    Ok(Orders {
        fixtures: fixtures.ok_or("film: --fixtures is needed")?,
        compare: compare.ok_or("film: --compare is needed")?,
        library: PathBuf::from(scratch("METHOD_READER_LIBRARY")?),
        fps: fps.clamp(1, 60),
    })
}

fn main() {
    let o = match orders() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    if let Err(e) = film(&o) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn draw(ui: &mut egui::Ui, f: &mut Film) {
    match f.scene {
        Scene::App => f.app.frame(ui),
        Scene::Crop(crop) => {
            let key = crop.map(|v| v as u32);
            if !f.textures.iter().any(|(k, _)| *k == key) {
                let t = ui.ctx().load_texture(
                    format!("compare {key:?}"),
                    cut(&f.compare, crop),
                    egui::TextureOptions::LINEAR,
                );
                f.textures.push((key, t));
            }
            let texture = f
                .textures
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, t)| t.clone())
                .expect("just made");
            // The picture's own page colour, so its edge does not show.
            let [r, g, b, _] = f.compare[..4] else {
                unreachable!("the picture is 2560 by 1600")
            };
            let screen = ui.ctx().content_rect();
            ui.painter()
                .rect_filled(screen, 0.0, Color32::from_rgb(r, g, b));
            let [x0, y0, x1, y1] = crop;
            let (cw, ch) = (x1 - x0, y1 - y0);
            // The device as tall as the frame allows above the caption.
            let scale = (H - 110.0) / ch;
            let size = Vec2::new(cw * scale, ch * scale);
            let rect = Rect::from_center_size(Pos2::new(W / 2.0, (H - 70.0) / 2.0), size);
            let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
            ui.painter().image(texture.id(), rect, uv, Color32::WHITE);
        }
        Scene::Card => {
            let p = ui.painter();
            p.rect_filled(ui.ctx().content_rect(), 0.0, PAPER);
            p.rect_filled(
                Rect::from_min_max(Pos2::new(0.0, H - 12.0), Pos2::new(W, H)),
                0.0,
                ACCENT,
            );
            p.text(
                Pos2::new(W / 2.0, 300.0),
                Align2::CENTER_CENTER,
                "Method Reader",
                FontId::proportional(64.0),
                INK,
            );
            p.text(
                Pos2::new(W / 2.0, 370.0),
                Align2::CENTER_CENTER,
                "Free and open source.",
                FontId::proportional(30.0),
                INK_SOFT,
            );
            p.text(
                Pos2::new(W / 2.0, 430.0),
                Align2::CENTER_CENTER,
                "methodreader.com",
                FontId::proportional(30.0),
                ACCENT,
            );
        }
    }
    if let Some(words) = f.caption {
        let p = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            egui::Id::unique("film caption"),
        ));
        let galley = p.layout_no_wrap(words.into(), FontId::proportional(26.0), Color32::WHITE);
        let pad = Vec2::new(22.0, 12.0);
        let size = galley.size() + pad * 2.0;
        let rect = Rect::from_center_size(Pos2::new(W / 2.0, H - 54.0), size);
        p.rect_filled(rect, 10.0, Color32::from_rgba_unmultiplied(20, 24, 28, 225));
        p.galley(rect.min + pad, galley, Color32::WHITE);
    }
}

/// The film's camera: every step drawn and written as one frame, at the
/// film's pace in real time, so the engine's work takes as long on film as
/// it does.
struct Camera<'a, W: Write> {
    h: Harness<'a, Film>,
    out: W,
    frame: Duration,
    frames: u64,
}

impl<O: Write> Camera<'_, O> {
    fn shoot(&mut self) -> Result<(), String> {
        let start = Instant::now();
        self.h.step();
        let image = self.h.render().map_err(|e| format!("film: wgpu: {e}"))?;
        self.out
            .write_all(image.as_raw())
            .map_err(|e| format!("film: could not write a frame: {e}"))?;
        self.frames += 1;
        if let Some(rest) = self.frame.checked_sub(start.elapsed()) {
            std::thread::sleep(rest);
        }
        Ok(())
    }

    fn hold(&mut self, seconds: f32) -> Result<(), String> {
        let n = (seconds / self.frame.as_secs_f32()).round() as u64;
        for _ in 0..n {
            self.shoot()?;
        }
        Ok(())
    }

    /// Film until `done`, at most `limit` seconds.
    fn until(&mut self, what: &str, limit: f32, done: impl Fn(&App) -> bool) -> Result<(), String> {
        let end = Instant::now() + Duration::from_secs_f32(limit);
        while !done(&self.h.state().app) {
            if Instant::now() > end {
                return Err(format!("film: timed out waiting for {what}"));
            }
            self.shoot()?;
        }
        Ok(())
    }

    fn app(&mut self) -> &mut App {
        &mut self.h.state_mut().app
    }

    fn set(&mut self, scene: Scene, caption: Option<&'static str>) {
        let f = self.h.state_mut();
        f.scene = scene;
        f.caption = caption;
    }

    fn tab(&mut self, label: &str) {
        self.h.get_by_role_and_label(Role::Tab, label).click();
    }
}

fn done(app: &App) -> bool {
    app.convert.view == convert::View::Done && app.show.is_none() && app.quiet()
}

fn idle(app: &App) -> bool {
    app.convert.view == convert::View::Idle && app.quiet()
}

/// Off camera: a script converted into the library, then the well again.
fn convert_quietly(h: &mut Harness<'_, Film>, pdf: &str) -> Result<(), String> {
    convert::drop_paths(&mut h.state_mut().app, &[pdf.to_string()]);
    let end = Instant::now() + Duration::from_secs(120);
    while !done(&h.state().app) {
        if Instant::now() > end {
            return Err(format!("film: {pdf} did not convert"));
        }
        std::thread::sleep(Duration::from_millis(16));
        h.step();
    }
    convert::reset(&mut h.state_mut().app);
    while !idle(&h.state().app) {
        if Instant::now() > end {
            return Err("film: the well did not come back".into());
        }
        std::thread::sleep(Duration::from_millis(16));
        h.step();
    }
    Ok(())
}

fn film(o: &Orders) -> Result<(), String> {
    let script = |name: &str| o.fixtures.join(name);
    let scripts = vec![
        script("field-station.pdf"),
        script("screenplay.pdf"),
        script("images.pdf"),
    ];
    for s in &scripts {
        if !s.is_file() {
            return Err(format!("film: no script at {}", s.display()));
        }
    }
    let compare = std::fs::read(&o.compare)
        .map_err(|e| format!("film: could not read {}: {e}", o.compare.display()))?;
    if compare.len() != COMPARE[0] * COMPARE[1] * 4 {
        return Err("film: the comparison picture is not 2560 by 1600 RGBA".into());
    }
    let guard = Guard::film(
        Arc::new(DirectEngine::new()),
        scripts.clone(),
        o.library.clone(),
    );
    let refused = guard.refused();
    let mut app = App::new(Arc::new(guard), Mode::Threads, capture_host());
    app.picture_mode = true;
    let frame = Duration::from_secs_f64(1.0 / f64::from(o.fps));
    let mut h = Harness::builder()
        .with_size([W, H])
        .with_pixels_per_point(PIXELS_PER_POINT)
        .with_theme(egui::Theme::Light)
        .with_step_dt(frame.as_secs_f32())
        .build_ui_state(
            draw,
            Film {
                app,
                scene: Scene::App,
                caption: None,
                compare,
                textures: Vec::new(),
            },
        );
    h.input_mut().system_theme = Some(egui::Theme::Light);

    // Off camera: the well, and two more invented scripts in the library.
    let end = Instant::now() + Duration::from_secs(60);
    while !(idle(&h.state().app) && h.state().app.convert.library.is_some()) {
        if Instant::now() > end {
            return Err("film: the drop well never came".into());
        }
        std::thread::sleep(Duration::from_millis(16));
        h.step();
    }
    for s in &scripts[1..] {
        convert_quietly(&mut h, &s.to_string_lossy())?;
    }

    let stdout = std::io::stdout();
    let mut cam = Camera {
        h,
        out: std::io::BufWriter::with_capacity(1 << 24, stdout.lock()),
        frame,
        frames: 0,
    };

    // 1 and 2: the same scene, as a PDF and as a book.
    cam.set(
        Scene::Crop(PDF_DEVICE),
        Some("A script PDF on an e-reader."),
    );
    cam.hold(6.0)?;
    cam.set(
        Scene::Crop(BOOK_DEVICE),
        Some("The same scene, rebuilt as a book."),
    );
    cam.hold(6.0)?;

    // 3: drop in a PDF.
    cam.set(Scene::App, Some("Drop in a PDF."));
    cam.hold(1.5)?;
    let demo = scripts[0].to_string_lossy().into_owned();
    convert::drop_paths(cam.app(), &[demo]);
    cam.until("the finished book", 120.0, done)?;
    cam.hold(3.0)?;

    // 4: reading it, then a setting changed and the book made again.
    cam.set(
        Scene::App,
        Some("Set it the way you like to read. It still reads like a script."),
    );
    cam.tab("Preview");
    cam.until("the Preview", 10.0, |a| {
        a.surface == Surface::Read && a.quiet()
    })?;
    cam.hold(1.5)?;
    // Two scenes further on, as a click in the scene index goes.
    for at in [2, 4] {
        let id = cam
            .app()
            .read
            .marks
            .get(at)
            .map(|m| m.id.clone())
            .ok_or("film: the Preview has too few scenes")?;
        read::go_to_scene(cam.app(), &id);
        cam.hold(1.6)?;
    }
    cam.tab("Settings");
    cam.until("Settings", 10.0, |a| {
        a.surface == Surface::Tune && a.quiet()
    })?;
    cam.hold(1.5)?;
    for em in [1.2, 1.4, 1.6] {
        cam.h
            .get_by_role_and_label(Role::Slider, "Space between elements")
            .set_value_accesskit(em);
        cam.hold(0.8)?;
    }
    cam.until("the book made again", 60.0, |a| a.quiet())?;
    cam.hold(2.5)?;

    // 5: the send panel, as it first asks (no reader in the scratch settings).
    cam.set(Scene::App, Some("Send it to your reader."));
    cam.tab("Convert");
    cam.until("the send panel", 10.0, |a| {
        a.surface == Surface::Convert && a.quiet()
    })?;
    cam.hold(6.0)?;

    // 6: the Library, and Field Station's report.
    cam.set(Scene::App, Some("Every script, kept, with a report."));
    cam.tab("Library");
    cam.until("the library's list", 20.0, |a| {
        a.surface == Surface::Library
            && !a.library.looking
            && a.library
                .listing
                .as_ref()
                .is_some_and(|l| l.rows.len() == 3)
            && a.quiet()
    })?;
    cam.hold(2.0)?;
    cam.h.get_by_label("Report for Field Station").click();
    cam.until("the report", 30.0, |a| {
        matches!(a.library.report, Some(Ok(_))) && a.quiet()
    })?;
    cam.hold(6.0)?;

    // 7: one character's sides, opened as the book.
    cam.set(Scene::App, Some("Sides for an actor, in one click."));
    cam.h.get_by_label("Make sides for MARA").click();
    cam.until("the sides", 60.0, |a| a.quiet())?;
    cam.hold(1.5)?;
    cam.h
        .get_all_by_label("Send to e-reader")
        .next()
        .ok_or("film: no Send to e-reader for the sides")?
        .click();
    cam.until("the sides book", 60.0, done)?;
    cam.tab("Preview");
    cam.until("the sides in the Preview", 10.0, |a| {
        a.surface == Surface::Read && a.quiet()
    })?;
    cam.hold(4.0)?;

    // 9: the end card.
    cam.set(Scene::Card, Some("Free. No account. Nothing uploaded."));
    cam.hold(6.0)?;

    cam.out
        .flush()
        .map_err(|e| format!("film: could not write the last frames: {e}"))?;
    let refused = refused.lock().map(|r| r.clone()).unwrap_or_default();
    if !refused.is_empty() {
        return Err(format!(
            "film: the window made engine calls the film refuses:\n  {}",
            refused.join("\n  ")
        ));
    }
    eprintln!(
        "film: {} frames at {} a second, {:.1} seconds",
        cam.frames,
        1.0 / cam.frame.as_secs_f32(),
        cam.frames as f32 * cam.frame.as_secs_f32()
    );
    Ok(())
}
