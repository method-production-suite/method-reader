//! Every picture the capture tool takes, and where each one goes (was
//! tools/capture/shots.ts).
//!
//! The README gets all of them (spec 2026-09-22, decision 7): the hero, the
//! drop-and-result pair and reading, window pictures in light AND dark so
//! GitHub shows the one matching the reader's theme. The site gets the light
//! drop and result for its "Drag and drop" section; it has no dark mode.

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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shot {
    pub name: &'static str,
    pub kind: Kind,
    pub width: u32,
    pub height: u32,
    pub themes: &'static [Theme],
    /// Copied into site/img/ as well, light only.
    pub site: bool,
}

const BOTH: &[Theme] = &[Theme::Light, Theme::Dark];

pub const SHOTS: &[Shot] = &[
    Shot {
        name: "hero",
        kind: Kind::Site,
        width: 1200,
        height: 760,
        themes: &[Theme::Light],
        site: false,
    },
    Shot {
        name: "drop",
        kind: Kind::Window,
        width: 1280,
        height: 800,
        themes: BOTH,
        site: true,
    },
    Shot {
        name: "result",
        kind: Kind::Window,
        width: 1280,
        height: 800,
        themes: BOTH,
        site: true,
    },
    Shot {
        name: "read",
        kind: Kind::Window,
        width: 1280,
        height: 800,
        themes: BOTH,
        site: false,
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

/// Where one shot in one theme is written, relative to the repository.
pub fn outputs_for(shot: &Shot, theme: Theme) -> Vec<String> {
    let file = format!("{}-{}.png", shot.name, theme.word());
    let mut out = vec![format!("assets/screens/{file}")];
    if shot.site && theme == Theme::Light {
        out.push(format!("site/img/{file}"));
    }
    out
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
