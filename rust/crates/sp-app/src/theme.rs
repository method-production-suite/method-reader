//! The window's colours, type and spacing, light and dark: Method Reader's
//! part of the Method family palette (site/assets/tokens.css, docs/BRAND.md),
//! warm neutrals and a violet accent, amber only for the Beta badge. The
//! tests below hold every value to brand/tokens.json (colours) and
//! brand/tokens.css (scales), the source.
//!
//! The field names are the window's own and older than the palette: `brass`
//! is the accent role (it was a brass yellow under the Screepub name) and
//! `brass_*` the ramp that shades the brads in it. brand/README.md maps each
//! name to the site token it takes its value from.

use egui::{Color32, FontFamily, FontId};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    /// The page surface (`--bg`).
    pub paper: Color32,
    /// Headings, sluglines, cues (`--ink`).
    pub ink: Color32,
    /// Secondary text (`--muted`).
    pub ink_muted: Color32,
    /// Body prose (`--ink-soft`).
    pub ink_soft: Color32,
    /// Text on an accent fill (`--accent-ink`).
    pub ink_on_brass: Color32,
    /// The one accent (`--accent`).
    pub brass: Color32,
    /// The desk seen through a punched hole.
    pub ground: Color32,
    /// Errors only.
    pub alarm: Color32,
    /// The Beta badge: the family's caution amber (`--note`), never an accent.
    pub note: Color32,
    /// A card on the paper (`--surface`).
    pub panel: Color32,
    /// Hairlines, hover grounds, punched-hole shading.
    pub hole: Color32,
    /// The dot on a reader the computer can see.
    pub present: Color32,
    pub brass_specular: Color32,
    /// The ramp's highlight stop, and the accent button's hover.
    pub brass_highlight: Color32,
    pub brass_shadow: Color32,
    pub brass_rim: Color32,
    pub brass_edge: Color32,
    pub brass_bounce: Color32,
}

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

pub const LIGHT: Tokens = Tokens {
    paper: hex(0xF7F6F2),
    ink: hex(0x1C2228),
    ink_muted: hex(0x59626D),
    ink_soft: hex(0x3A424C),
    ink_on_brass: hex(0xFFFFFF),
    brass: hex(0x7052BA),
    ground: hex(0x1C2228),
    alarm: hex(0xAF3220),
    note: hex(0x8A5A00),
    panel: hex(0xFFFFFF),
    hole: Color32::from_rgba_unmultiplied_const(28, 34, 40, 26),
    present: hex(0x3E7D4A),
    brass_specular: hex(0xF3EFFB),
    brass_highlight: hex(0x7C5FC6),
    brass_shadow: hex(0x573E96),
    brass_rim: hex(0x3F2C6E),
    brass_edge: hex(0x33245A),
    brass_bounce: hex(0xCDBDF6),
};

pub const DARK: Tokens = Tokens {
    paper: hex(0x111417),
    ink: hex(0xE4E8EC),
    ink_muted: hex(0x9AA3AD),
    ink_soft: hex(0xC3CAD2),
    ink_on_brass: hex(0x160F26),
    brass: hex(0xB49CF0),
    ground: hex(0x0B0C0E),
    alarm: hex(0xEA7A65),
    note: hex(0xE3B341),
    panel: hex(0x1B1F24),
    hole: Color32::from_rgba_unmultiplied_const(0, 0, 0, 115),
    present: hex(0x6FBF7E),
    brass_specular: hex(0xF5F1FD),
    brass_highlight: hex(0xCDBDF6),
    brass_shadow: hex(0x8F74D6),
    brass_rim: hex(0x6B51B2),
    brass_edge: hex(0x574096),
    brass_bounce: hex(0xE4DAFA),
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

    /// A brand file, read from the repository (brand/ outlives desktop/).
    fn brand(file: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/../../../brand/{file}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect(file)
        .replace("\r\n", "\n")
    }

    /// `#RRGGBB` or `rgba(r,g,b,a)` as a colour, the alpha rounded to a byte
    /// as a browser rounds it.
    fn colour(value: &str) -> Color32 {
        if let Some(h) = value.strip_prefix('#') {
            return hex(u32::from_str_radix(h, 16).expect(value));
        }
        let args = value
            .strip_prefix("rgba(")
            .and_then(|r| r.strip_suffix(')'))
            .expect(value);
        let p: Vec<&str> = args.split(',').map(str::trim).collect();
        let alpha: f64 = p[3].parse().unwrap();
        Color32::from_rgba_unmultiplied(
            p[0].parse().unwrap(),
            p[1].parse().unwrap(),
            p[2].parse().unwrap(),
            (alpha * 255.0).round() as u8,
        )
    }

    fn fields(t: &Tokens) -> [(&'static str, Color32); 18] {
        [
            ("paper", t.paper),
            ("ink", t.ink),
            ("ink-muted", t.ink_muted),
            ("ink-soft", t.ink_soft),
            ("ink-on-brass", t.ink_on_brass),
            ("brass", t.brass),
            ("ground", t.ground),
            ("alarm", t.alarm),
            ("note", t.note),
            ("panel", t.panel),
            ("hole", t.hole),
            ("present", t.present),
            ("brass-specular", t.brass_specular),
            ("brass-highlight", t.brass_highlight),
            ("brass-shadow", t.brass_shadow),
            ("brass-rim", t.brass_rim),
            ("brass-edge", t.brass_edge),
            ("brass-bounce", t.brass_bounce),
        ]
    }

    // Every colour is held to brand/tokens.json, the one source, in both
    // modes, both ways.
    #[test]
    fn every_colour_is_brand_tokens_json_s_in_both_modes() {
        let json: serde_json::Value = serde_json::from_str(&brand("tokens.json")).unwrap();
        let colors = json["colors"].as_object().expect("colors");
        for (mode, t) in [("light", &LIGHT), ("dark", &DARK)] {
            let ours = fields(t);
            for (name, c) in ours {
                let want = colors
                    .get(name)
                    .and_then(|v| v[mode].as_str())
                    .unwrap_or_else(|| panic!("brand/tokens.json has no {mode} {name}"));
                assert_eq!(c, colour(want), "{mode} {name}");
            }
            for name in colors.keys() {
                assert!(
                    ours.iter().any(|(n, _)| n == name),
                    "brand/tokens.json has {name}, which the window lacks"
                );
            }
        }
    }

    /// brand/tokens.css's light block, as (name, value) pairs.
    fn declarations() -> Vec<(String, String)> {
        let css = brand("tokens.css");
        let root = &css[..css.find("@media").unwrap_or(css.len())];
        root.lines()
            .filter_map(|l| {
                let rest = l.trim().strip_prefix("--")?;
                let (name, value) = rest.split_once(':')?;
                let value = value.split(';').next()?.trim();
                Some((name.to_string(), value.to_string()))
            })
            .collect()
    }

    // W22: the type scale, radii, motion and spacing are brand/tokens.css's
    // (desktop/ui/tokens.css carried them verbatim from there).
    #[test]
    fn the_type_scale_radii_motion_and_spacing_are_brand_tokens_css_s() {
        let decl = declarations();
        let value = |name: &str| -> String {
            decl.iter()
                .find(|(n, _)| n == name)
                .map(|(_, v)| v.clone())
                .unwrap_or_else(|| panic!("brand/tokens.css has no --{name}"))
        };
        let px = |name: &str, v: f32| assert_eq!(value(name), format!("{v}px"), "--{name}");
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
        assert_eq!(value("motion-press"), format!("{MOTION_PRESS}s"));
        assert_eq!(value("motion-state"), format!("{MOTION_STATE}s"));
        // The spacing scale, at the window's 16px root.
        let space = crate::model::layout::SPACE;
        for (i, points) in space.iter().enumerate().skip(1) {
            let rem = points / 16.0;
            assert_eq!(
                value(&format!("space-{i}")),
                format!("{rem}rem"),
                "space-{i}"
            );
        }
        assert!(
            !decl
                .iter()
                .any(|(n, _)| *n == format!("space-{}", space.len())),
            "brand/tokens.css has a spacing step the window lacks"
        );
    }
}
