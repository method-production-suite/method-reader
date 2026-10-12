//! The README and site pictures: was tools/capture-screens.ts,
//! tools/review-screens.ts, tools/capture/*.ts and tools/make-og.sh.
//!
//! - `cargo xtask capture-screens [--only <shot>]` takes every README and
//!   site picture from the real window running the real engine on
//!   tests/fixtures/field-station.pdf (spec
//!   docs/superpowers/specs/2026-09-22-readme-site-screens-design.md, part
//!   3), and writes assets/screens/<shot>-<theme>.png, copying the light drop
//!   and result into site/img/. A file is rewritten only when its bytes
//!   change. Prints Chrome's version, then one line per file: written or
//!   unchanged. Nothing is written unless every picture was taken.
//! - `cargo xtask review-screens --out <page.html> [--repo <dir>]`: which
//!   pictures changed since the last commit, old beside new.
//! - `cargo xtask make-og` renders site/og-card.html to site/og.png at
//!   1200 by 630, the size every social unfurler expects.
//!
//! The pieces: [`shots`] (the list and where each goes), [`page`] (the
//! window's index.html with the capture scripts in), [`gate`] and
//! [`window_argv`] (which engine calls the window may make), [`server`] and
//! [`http`] (the page server, on threads of its own), [`cdp`] (Chrome),
//! [`backend`] (what draws the pictures, and how an egui window plugs in at
//! cutover), [`run`] (one whole run and its cleanup), [`review`] and
//! [`picture`]. tools/capture/bridge.js and steps.js run INSIDE the page
//! and frame.html and hero.html are page templates: they stay as they are,
//! served from tools/capture/.
//!
//! capture-screens needs Chrome at [`cdp::CHROME`], and macOS: the demo
//! library is /Users/Shared/Documents/Screepub, because the result screen
//! prints the book's full path and that folder reads naturally with no
//! username in it. The tool marks the library as its own, refuses to run if
//! an unmarked one is already there, and removes it when done, along with
//! any folder above it that it had to create ([`run`] has the details).
//!
//! Exit codes: 0 done. 1 failed: if a picture could not be taken, or an
//! engine call was refused or failed, or the page server hit an error,
//! nothing was written; if only the cleanup afterwards failed, the pictures
//! WERE written and the message says what could not be cleaned up. 2 no
//! Chrome, so nothing was retaken (a release warns about that rather than
//! stopping). 130 or 143 interrupted, after cleaning up; nothing written.

pub mod backend;
pub mod cdp;
pub mod gate;
pub mod http;
pub mod page;
pub mod picture;
pub mod review;
pub mod run;
pub mod server;
pub mod shots;
pub mod window_argv;

use std::collections::HashMap;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::time::Duration;

use backend::{Browser, CaptureOptions, DesktopUi, Wait};
use run::{RunOptions, run_capture};
use shots::{SHOTS, Theme};

use crate::paths::{bun, repo_root};

const LIBRARY: &str = "/Users/Shared/Documents/Screepub";

/// 32 hex digits no other process can guess: SipHash under the keys std
/// seeds from the OS's randomness, over the time and a counter.
pub fn random_token() -> String {
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    let n = COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let mut out = String::new();
    for half in 0..2u8 {
        let mut h = std::hash::RandomState::new().build_hasher();
        h.write_u128(now);
        h.write_usize(n);
        h.write_u8(half);
        h.write_u32(std::process::id());
        out.push_str(&format!("{:016x}", h.finish()));
    }
    out
}

/// `--name value` and `--name=value` for the names given, as Node's
/// parseArgs reads them; anything else is refused.
pub fn parse_flags(args: &[String], names: &[&str]) -> Result<HashMap<String, String>, String> {
    let mut out = HashMap::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        i += 1;
        let Some(flag) = a.strip_prefix("--") else {
            return Err(format!("Unexpected argument '{a}'"));
        };
        let (name, value) = match flag.split_once('=') {
            Some((n, v)) => (n, Some(v.to_string())),
            None => (flag, None),
        };
        if !names.contains(&name) {
            return Err(format!("Unknown option '--{name}'"));
        }
        let value = match value {
            Some(v) => v,
            None => {
                let v = args
                    .get(i)
                    .ok_or_else(|| format!("Option '--{name} <value>' argument missing"))?;
                i += 1;
                v.clone()
            }
        };
        out.insert(name.to_string(), value);
    }
    Ok(out)
}

/// `cargo xtask capture-screens`: its exit code.
pub fn capture_main(args: &[String]) -> u8 {
    let flags = match parse_flags(args, &["only"]) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("capture: {e}");
            return 1;
        }
    };
    let shots: Vec<_> = match flags.get("only") {
        None => SHOTS.to_vec(),
        Some(only) => SHOTS.iter().filter(|s| s.name == only).copied().collect(),
    };
    if shots.is_empty() {
        eprintln!("capture: no shot named {}", flags["only"]);
        return 1;
    }
    let chrome = PathBuf::from(cdp::CHROME);
    if !chrome.exists() {
        eprintln!(
            "capture: no Chrome at {}. Pictures NOT retaken.",
            cdp::CHROME
        );
        return 2;
    }

    // A first signal stops the run at its next look, which cleans up and
    // answers 130 or 143; a second means "now", cleanup or not.
    let signal = Arc::new(AtomicUsize::new(0));
    let once = Arc::new(AtomicBool::new(false));
    for (sig, code) in [
        (signal_hook::consts::SIGINT, 130),
        (signal_hook::consts::SIGTERM, 143),
    ] {
        let _ = signal_hook::flag::register_conditional_shutdown(sig, code, Arc::clone(&once));
        let _ = signal_hook::flag::register(sig, Arc::clone(&once));
        let _ = signal_hook::flag::register_usize(sig, Arc::clone(&signal), sig as usize);
    }

    let repo = repo_root();
    let result = run_capture(RunOptions {
        shots,
        demo_pdf: repo
            .join("tests")
            .join("fixtures")
            .join("field-station.pdf"),
        library: PathBuf::from(LIBRARY),
        launch: Box::new(move || cdp::launch(&chrome).map(|c| Box::new(c) as Box<dyn Browser>)),
        window: Box::new(DesktopUi),
        engine: vec![
            bun().to_string_lossy().into_owned(),
            repo.join("src")
                .join("cli.ts")
                .to_string_lossy()
                .into_owned(),
        ],
        out_dir: repo.clone(),
        repo_dir: repo,
        tmp: None,
        calibre_config: None,
        log: Box::new(|line| println!("{line}")),
        interrupt: Some(signal),
    });
    match result {
        Ok(()) => 0,
        Err(e) => {
            if let Some(sig) = e.interrupted {
                eprintln!(
                    "capture: {}, cleaned up",
                    if sig == 2 { "SIGINT" } else { "SIGTERM" }
                );
            } else {
                eprintln!("{}", e.message);
            }
            for c in &e.cleanup {
                eprintln!("{c}");
            }
            match e.interrupted {
                Some(2) => 130,
                Some(_) => 143,
                None => 1,
            }
        }
    }
}

/// `cargo xtask review-screens`: its exit code.
pub fn review_main(args: &[String]) -> u8 {
    review::main(args, &repo_root())
}

/// A file:// URL for an absolute path, every byte outside the unreserved
/// set and `/` percent-encoded.
pub fn file_url(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file://");
    if !text.starts_with('/') {
        out.push('/');
    }
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b"/-._~:".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `cargo xtask make-og`: renders site/og-card.html to site/og.png at
/// 1200x630 on a transparent background, then prints its size the way
/// macOS's `sips -g pixelWidth -g pixelHeight` did. Re-run after editing
/// og-card.html. Exit codes: 0 done, 1 no Chrome or failed.
pub fn make_og_main(_args: &[String]) -> u8 {
    let chrome = Path::new(cdp::CHROME);
    if !chrome.exists() {
        eprintln!("Google Chrome not found at {}", cdp::CHROME);
        return 1;
    }
    let site = repo_root().join("site");
    match make_og(chrome, &site.join("og-card.html"), &site.join("og.png")) {
        Ok((w, h)) => {
            println!(
                "{}\n  pixelWidth: {w}\n  pixelHeight: {h}",
                site.join("og.png").display()
            );
            0
        }
        Err(e) => {
            eprintln!("make-og: {e}");
            1
        }
    }
}

/// Draws `card` with the Chrome at `chrome` into `out`; answers its size.
pub fn make_og(chrome: &Path, card: &Path, out: &Path) -> Result<(u32, u32), String> {
    let mut browser = cdp::launch(chrome)?;
    let shot = browser.capture(&CaptureOptions {
        transparent: true,
        scale: 1.0,
        wait: Wait::Loaded,
        timeout: Duration::from_secs(60),
        ..CaptureOptions::page(file_url(card), 1200, 630, Theme::Light)
    });
    let closed = browser.close();
    let png = shot?;
    closed?;
    std::fs::write(out, &png).map_err(|e| format!("could not write {}: {e}", out.display()))?;
    picture::size(&png)
}
