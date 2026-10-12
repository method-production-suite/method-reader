//! What draws the pictures, behind two traits, so the run (claiming the
//! library, the shot list, framing, the all-or-nothing write, cleanup) does
//! not care what the window is made of.
//!
//! - [`Browser`] draws web pages: the site's hero (hero.html) and pass two
//!   of every window picture, the frame (frame.html). Chrome over the
//!   DevTools protocol ([`super::cdp`]). Both pages are HTML and stay HTML,
//!   so this one outlives the cutover.
//! - [`WindowBackend`] draws pass one: the bare window, in one shot's
//!   state, at the shot's size, in one theme, as PNG bytes at twice the CSS
//!   size (a 1280 by 800 window is a 2560 by 1600 picture).
//!
//! Today's window backend is [`DesktopUi`]: Chrome loads desktop/ui through
//! the capture server, tools/capture/bridge.js stands in for Tauri and posts
//! the window's engine calls to /engine (run through the gate, run by the
//! real CLI), and tools/capture/steps.js drives the window into the shot
//! and says `ready`.
//!
//! # At cutover: an egui backend
//!
//! The window becomes rust/crates/sp-app, drawn by egui, and egui_kittest
//! (already sp-app's dev-dependency; its tests/screens.rs drives the
//! screens with `Harness`) can render a screen headlessly to an image. An
//! `EguiWindow` implementing [`WindowBackend`] must provide, for each
//! window shot in [`super::shots::SHOTS`] (drop, result, read):
//!
//! - **the size**: `shot.width` by `shot.height` points at a pixels-per-point
//!   of 2.0, so its picture matches Chrome's deviceScaleFactor 2 and the
//!   frame (frame.html draws the pass-one image at the CSS size `?w=&h=`);
//! - **the theme**: light or dark forced, whatever the OS says (sp-app's
//!   theme.rs picks the tokens; the backend sets the choice, not mundy);
//! - **the shot's state**, reached the way steps.js reaches it: `drop` is
//!   the idle drop well; `result` is the demo PDF converted (the engine
//!   linked in, writing into the run's scratch library) with the send
//!   panel's device cards (no reader's device in the scratch settings) and
//!   the book's first page typeset; `read` is the same book on the Read
//!   tab with its scene index and script drawn;
//! - **settled**: no animation running, fonts loaded, the version stamp and
//!   the library line hidden (steps.js's hideStamp and hideLibrarySlot), so
//!   a picture changes only when the window's look does;
//! - **the same seams**: the engine run with the run's environment
//!   ([`super::run::engine_env`]: scratch library, scratch app settings, no
//!   volumes, a dead reMarkable address, a scratch COPY of Calibre's
//!   settings), and nothing that copies, sends, builds or ejects;
//! - **PNG bytes**: [`super::picture::encode_rgba`] turns the harness's
//!   RGBA image into them deterministically, so same pixels give the same
//!   bytes and `write_if_changed` keeps an unchanged picture unchanged.
//!
//! It then goes in [`super::run::RunOptions::window`]; the hero and the
//! frame still come from the [`Browser`], so Chrome stays a need for the
//! README pictures even after the window stops being HTML. The gate and the
//! /engine route are DesktopUi's concern only: an in-process engine needs
//! no HTTP door, but the egui backend must hold the same line in code (the
//! demo PDF only, the scratch library only).

use std::time::Duration;

use super::shots::{Shot, Theme};

/// When a page is ready to be photographed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wait {
    /// The page sets `window.__captureState` to `ready` or `failed` (the
    /// capture's own pages: the window, hero.html, frame.html).
    PageSaysReady,
    /// A page that says nothing (site/og-card.html): loaded, fonts too.
    Loaded,
}

pub struct CaptureOptions<'a> {
    pub url: String,
    /// CSS pixels.
    pub width: u32,
    pub height: u32,
    pub theme: Theme,
    pub transparent: bool,
    /// Device pixels per CSS pixel: 2 for the README, 1 for the social card.
    pub scale: f64,
    pub wait: Wait,
    /// Asked on every poll: a sentence here fails the capture at once, the
    /// way a page server that has already failed should, rather than after
    /// the whole wait.
    pub failure: Option<&'a dyn Fn() -> Option<String>>,
    /// Added to a timeout's message: what might explain the page never
    /// saying ready (the files the server could not find, say).
    pub explain: Option<&'a dyn Fn() -> String>,
    pub timeout: Duration,
}

impl CaptureOptions<'_> {
    /// A capture of `url` at the README's scale, waiting on the page.
    pub fn page(url: String, width: u32, height: u32, theme: Theme) -> CaptureOptions<'static> {
        CaptureOptions {
            url,
            width,
            height,
            theme,
            transparent: false,
            scale: 2.0,
            wait: Wait::PageSaysReady,
            failure: None,
            explain: None,
            timeout: super::cdp::CAPTURE_TIMEOUT,
        }
    }
}

/// Something that draws web pages to PNG.
pub trait Browser {
    /// What it calls itself, e.g. Chrome/153.0.8010.53.
    fn version(&self) -> String;
    fn capture(&mut self, o: &CaptureOptions<'_>) -> Result<Vec<u8>, String>;
    /// Safe to call more than once; later calls answer what the first did.
    fn close(&mut self) -> Result<(), String>;
}

/// What a window backend gets for one pass-one picture.
pub struct PassOne<'a> {
    pub shot: &'a Shot,
    pub theme: Theme,
    /// The capture server, `http://127.0.0.1:<port>`.
    pub base: &'a str,
    pub browser: &'a mut dyn Browser,
    pub failure: &'a dyn Fn() -> Option<String>,
    pub explain: &'a dyn Fn() -> String,
}

/// Something that draws the bare window in one shot's state.
pub trait WindowBackend {
    fn pass_one(&mut self, p: PassOne<'_>) -> Result<Vec<u8>, String>;
}

/// The window as it is today: desktop/ui in Chrome, through the server.
pub struct DesktopUi;

impl WindowBackend for DesktopUi {
    fn pass_one(&mut self, p: PassOne<'_>) -> Result<Vec<u8>, String> {
        let mut o = CaptureOptions::page(
            format!("{}/capture/window.html?shot={}", p.base, p.shot.name),
            p.shot.width,
            p.shot.height,
            p.theme,
        );
        o.failure = Some(p.failure);
        o.explain = Some(p.explain);
        p.browser.capture(&o)
    }
}
