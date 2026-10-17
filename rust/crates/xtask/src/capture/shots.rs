//! Every picture the capture tool takes, and where each one goes (was
//! tools/capture/shots.ts).
//!
//! The Method suite's four shots, in its order (SUITE-STANDARDS.md section
//! 7.1): (1) the hero, the main view with real content: `read`, the book on
//! the Read tab; (2) the signature feature, the before and after:
//! `compare`, the site's own two-reader section held still; (3) open:
//! `drop`, with `result` beside it, the same window once the script is in;
//! (4) the file you own: `cli`, a terminal running the real command line
//! on Field Station and showing the files it wrote.
//!
//! Every shot in light AND dark, each written twice, to assets/screens/ for
//! the README and to site/img/ for the site (the website copies site/ alone),
//! so both pages can show the one matching the reader's theme.

use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
}

impl Theme {
    /// The word in file names and in `prefers-color-scheme`.
    pub fn word(self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The app, framed.
    Window,
    /// A web page, captured as it is.
    Site,
    /// A terminal window, framed like the app: the real command line run on
    /// the demo script, its transcript drawn by tools/capture/terminal.html.
    Terminal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shot {
    pub name: &'static str,
    pub kind: Kind,
    pub width: u32,
    pub height: u32,
    pub themes: &'static [Theme],
    /// What the window's title bar says in this shot (a framed shot only):
    /// for the app, its name, or the script's before it, as sp-app's
    /// `window_title` puts it.
    pub title: &'static str,
}

/// The macOS title bar pass two draws above a window picture, in CSS
/// pixels; tools/capture/frame.html draws it this tall. The window's
/// picture is its content under it, so the framed window is still
/// `width` by `height` in all.
pub const TITLE_BAR: u32 = 28;

impl Shot {
    /// How tall the window's own drawing is: the shot, less the title bar.
    pub fn content_height(&self) -> u32 {
        match self.kind {
            Kind::Window | Kind::Terminal => self.height - TITLE_BAR,
            Kind::Site => self.height,
        }
    }
}

const BOTH: &[Theme] = &[Theme::Light, Theme::Dark];

pub const SHOTS: &[Shot] = &[
    Shot {
        name: "read",
        kind: Kind::Window,
        width: 1280,
        height: 800,
        themes: BOTH,
        title: "Field Station - Method Reader",
    },
    Shot {
        name: "compare",
        kind: Kind::Site,
        width: 1280,
        height: 800,
        themes: BOTH,
        title: "",
    },
    Shot {
        name: "drop",
        kind: Kind::Window,
        width: 1280,
        height: 800,
        themes: BOTH,
        title: "Method Reader",
    },
    Shot {
        name: "result",
        kind: Kind::Window,
        width: 1280,
        height: 800,
        themes: BOTH,
        title: "Field Station - Method Reader",
    },
    Shot {
        name: "cli",
        kind: Kind::Terminal,
        width: 1280,
        height: 800,
        themes: BOTH,
        title: "Terminal",
    },
];

/// The margin pass two draws around a window picture, in CSS pixels: room
/// for the frame's shadow, deeper below than above. tools/capture/frame.html
/// says the same numbers in its `.pad` rule, and a test holds the two
/// together, because a mismatch clips the shadow or leaves a strip of
/// nothing along one edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pad {
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
    pub left: u32,
}

pub const FRAME_PAD: Pad = Pad {
    top: 44,
    right: 56,
    bottom: 68,
    left: 56,
};

/// The page size pass two is captured at: the window plus the frame.
pub fn framed_size(shot: &Shot) -> (u32, u32) {
    (
        shot.width + FRAME_PAD.left + FRAME_PAD.right,
        shot.height + FRAME_PAD.top + FRAME_PAD.bottom,
    )
}

/// Where one shot in one theme is written, relative to the repository:
/// the README's copy, then the site's.
pub fn outputs_for(shot: &Shot, theme: Theme) -> Vec<String> {
    let file = format!("{}-{}.png", shot.name, theme.word());
    vec![format!("assets/screens/{file}"), format!("site/img/{file}")]
}

/// Every output path of every shot, in shot order.
pub fn all_outputs(shots: &[Shot]) -> Vec<String> {
    shots
        .iter()
        .flat_map(|s| s.themes.iter().flat_map(move |t| outputs_for(s, *t)))
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Written {
    Written,
    Unchanged,
}

impl Written {
    pub fn word(self) -> &'static str {
        match self {
            Written::Written => "written",
            Written::Unchanged => "unchanged",
        }
    }
}

/// Same pixels in, same bytes out, so a release where the window did not
/// change commits no new images and the repository does not grow.
pub fn write_if_changed(path: &Path, bytes: &[u8]) -> std::io::Result<Written> {
    if std::fs::read(path).is_ok_and(|old| old == bytes) {
        return Ok(Written::Unchanged);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, bytes)?;
    Ok(Written::Written)
}
