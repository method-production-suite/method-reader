//! The window's colours, type and spacing, light and dark. They were
//! desktop/ui/style.css's seven core colours and its generated tokens.css,
//! both copied from brand/; the tests below hold every value to
//! brand/tokens.json (colours) and brand/tokens.css (scales), the source.

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

    fn fields(t: &Tokens) -> [(&'static str, Color32); 17] {
        [
            ("paper", t.paper),
            ("ink", t.ink),
            ("ink-muted", t.ink_muted),
            ("ink-soft", t.ink_soft),
            ("ink-on-brass", t.ink_on_brass),
            ("brass", t.brass),
            ("ground", t.ground),
            ("alarm", t.alarm),
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

    // The seven core colours were desktop/ui/style.css's and the rest its
    // generated tokens.css's; both were copied from brand/tokens.json, so
    // every colour is held to that one source, in both modes, both ways.
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
