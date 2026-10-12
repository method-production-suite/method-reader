//! The window's colours, type and spacing: `desktop/ui/style.css`'s seven
//! core colours and the generated `tokens.css` (from brand/tokens.json),
//! light and dark. The values are copied from those files and checked
//! against them by the test below, so they cannot drift while both exist.

use egui::{Color32, FontFamily, FontId};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub paper: Color32,
    pub ink: Color32,
    pub ink_muted: Color32,
    pub ink_soft: Color32,
    pub ink_on_brass: Color32,
    pub brass: Color32,
    pub ground: Color32,
    pub alarm: Color32,
    pub panel: Color32,
    pub hole: Color32,
    pub present: Color32,
    pub brass_specular: Color32,
    pub brass_highlight: Color32,
    pub brass_shadow: Color32,
    pub brass_rim: Color32,
    pub brass_edge: Color32,
    pub brass_bounce: Color32,
}

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

const BRASS: [Color32; 7] = [
    hex(0xe8a33d),
    hex(0xFDF1CE),
    hex(0xF6D486),
    hex(0xBE7C1E),
    hex(0x8B5917),
    hex(0x7A5116),
    hex(0xF7CE7E),
];

pub const LIGHT: Tokens = Tokens {
    paper: hex(0xf7f2e6),
    ink: hex(0x1d1b16),
    ink_muted: hex(0x6d6960),
    ink_soft: hex(0x4A453A),
    ink_on_brass: hex(0x1d1b16),
    brass: BRASS[0],
    ground: hex(0x23272f),
    alarm: hex(0xaf3220),
    panel: hex(0xFBF8F0),
    hole: Color32::from_rgba_unmultiplied_const(29, 27, 22, 26),
    present: hex(0x3E7D4A),
    brass_specular: BRASS[1],
    brass_highlight: BRASS[2],
    brass_shadow: BRASS[3],
    brass_rim: BRASS[4],
    brass_edge: BRASS[5],
    brass_bounce: BRASS[6],
};

pub const DARK: Tokens = Tokens {
    paper: hex(0x1e1c19),
    ink: hex(0xe8e2d3),
    ink_muted: hex(0x8f887c),
    ink_soft: hex(0xC4BDAC),
    ink_on_brass: hex(0x1d1b16),
    brass: BRASS[0],
    ground: hex(0x141517),
    alarm: hex(0xea7a65),
    panel: hex(0x22201C),
    hole: Color32::from_rgba_unmultiplied_const(0, 0, 0, 115),
    present: hex(0x6FBF7E),
    brass_specular: BRASS[1],
    brass_highlight: BRASS[2],
    brass_shadow: BRASS[3],
    brass_rim: BRASS[4],
    brass_edge: BRASS[5],
    brass_bounce: BRASS[6],
};

pub fn tokens(dark: bool) -> Tokens {
    if dark { DARK } else { LIGHT }
}

/// The type scale, in points.
pub mod size {
    pub const FINE: f32 = 10.0;
    pub const NOTE: f32 = 11.0;
    pub const LABEL: f32 = 12.0;
    pub const UI: f32 = 13.0;
    pub const CAPTION: f32 = 14.0;
    pub const CODE: f32 = 15.0;
    pub const BODY: f32 = 17.0;
    pub const LEDE: f32 = 18.0;
    pub const TITLE: f32 = 30.0;
    pub const DISPLAY: f32 = 44.0;
}

/// Radii and motion.
pub const RADIUS_MARK: f32 = 2.0;
pub const RADIUS: f32 = 3.0;
pub const RADIUS_WELL: f32 = 5.0;
pub const MOTION_PRESS: f32 = 0.1;
pub const MOTION_STATE: f32 = 0.2;

/// The three faces the window uses (fonts.rs registers them).
pub const STRUCTURE: &str = "courier";
pub const STRUCTURE_BOLD: &str = "courier-bold";
pub const PROSE: &str = "literata";

pub fn structure(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(STRUCTURE.into()))
}

pub fn structure_bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(STRUCTURE_BOLD.into()))
}

pub fn prose(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(PROSE.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn css(path: &str) -> String {
        std::fs::read_to_string(format!("{}/../../../{path}", env!("CARGO_MANIFEST_DIR")))
            .expect(path)
    }

    fn hexstr(c: Color32) -> String {
        format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
    }

    #[test]
    fn the_core_colours_are_style_css_s() {
        let style = css("desktop/ui/style.css").to_lowercase();
        let (light, dark) = style
            .split_once("@media (prefers-color-scheme: dark)")
            .unwrap();
        for (name, c) in [
            ("paper", LIGHT.paper),
            ("ink", LIGHT.ink),
            ("ink-muted", LIGHT.ink_muted),
            ("brass", LIGHT.brass),
            ("ground", LIGHT.ground),
            ("alarm", LIGHT.alarm),
        ] {
            assert!(
                light.contains(&format!("--{name}: {}", hexstr(c))),
                "{name}"
            );
        }
        for (name, c) in [
            ("paper", DARK.paper),
            ("ink", DARK.ink),
            ("ink-muted", DARK.ink_muted),
            ("ground", DARK.ground),
            ("alarm", DARK.alarm),
        ] {
            assert!(
                dark.contains(&format!("--{name}: {}", hexstr(c))),
                "dark {name}"
            );
        }
    }

    #[test]
    fn the_brand_tokens_are_tokens_css_s() {
        let tokens = css("desktop/ui/tokens.css").to_lowercase();
        let (light, dark) = tokens
            .split_once("@media (prefers-color-scheme: dark)")
            .unwrap();
        for (name, c) in [
            ("panel", LIGHT.panel),
            ("ink-soft", LIGHT.ink_soft),
            ("present", LIGHT.present),
            ("brass-specular", LIGHT.brass_specular),
            ("brass-highlight", LIGHT.brass_highlight),
            ("brass-shadow", LIGHT.brass_shadow),
            ("brass-rim", LIGHT.brass_rim),
            ("brass-edge", LIGHT.brass_edge),
            ("brass-bounce", LIGHT.brass_bounce),
        ] {
            assert!(
                light.contains(&format!("--{name}: {}", hexstr(c))),
                "{name}"
            );
        }
        for (name, c) in [
            ("panel", DARK.panel),
            ("ink-soft", DARK.ink_soft),
            ("present", DARK.present),
        ] {
            assert!(
                dark.contains(&format!("--{name}: {}", hexstr(c))),
                "dark {name}"
            );
        }
        assert!(light.contains("--hole: rgba(29,27,22,0.10)"));
        assert!(dark.contains("--hole: rgba(0,0,0,0.45)"));
    }

    // W22: the type scale, radii and motion are tokens.css's.
    #[test]
    fn the_type_scale_radii_and_motion_are_tokens_css_s() {
        let tokens = css("desktop/ui/tokens.css");
        let px = |name: &str, v: f32| {
            assert!(tokens.contains(&format!("--{name}: {v}px;")), "{name} {v}")
        };
        px("text-fine", size::FINE);
        px("text-note", size::NOTE);
        px("text-label", size::LABEL);
        px("text-ui", size::UI);
        px("text-caption", size::CAPTION);
        px("text-code", size::CODE);
        px("text-body", size::BODY);
        px("text-lede", size::LEDE);
        px("text-title", size::TITLE);
        px("text-display", size::DISPLAY);
        px("radius-mark", RADIUS_MARK);
        px("radius", RADIUS);
        px("radius-well", RADIUS_WELL);
        assert!(tokens.contains(&format!("--motion-press: {MOTION_PRESS}s;")));
        assert!(tokens.contains(&format!("--motion-state: {MOTION_STATE}s;")));
        // The spacing scale, at the window's 16px root.
        for (i, points) in crate::model::layout::SPACE.iter().enumerate().skip(1) {
            let rem = points / 16.0;
            assert!(
                tokens.contains(&format!("--space-{i}: {rem}rem;")),
                "space-{i} {rem}"
            );
        }
    }
}
