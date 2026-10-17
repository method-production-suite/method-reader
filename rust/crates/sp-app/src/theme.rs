//! The window's colours, type and spacing, light and dark: Method Reader's
//! part of the Method family palette (site/assets/tokens.css, docs/BRAND.md),
//! warm neutrals and a violet accent, amber only for the Beta badge. The
//! tests below hold every value to brand/tokens.json (colours) and
//! brand/tokens.css (scales), the source.
//!
//! The field names are the window's own and older than the palette: `brass`
//! is the accent role (it was a brass yellow under the Screepub name) and
//! `brass_*` its ramp. brand/README.md maps each name to the site token it
//! takes its value from.
//!
//! Type: the chrome (every control, label, tab, heading and message) is
//! the system's own UI sans, [`chrome`] and [`chrome_strong`], as in the
//! family's other windows; the script's faces, [`structure`] (Courier
//! Prime) and [`prose`] (Literata), set the screenplay itself and nothing
//! else. fonts.rs says where each face comes from.

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
    /// The tab strip under the title bar, a code chip (`--surface-2`).
    pub bar: Color32,
    /// Hairlines: control, card and sheet borders (`--rule`).
    pub rule: Color32,
    /// A border that carries meaning, 3:1 or more (`--rule-strong`).
    pub rule_strong: Color32,
    /// A selection's ground (`--accent-soft`).
    pub brass_soft: Color32,
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
    bar: hex(0xEFEDE7),
    rule: hex(0xDEDBD3),
    rule_strong: hex(0x8F897C),
    brass_soft: hex(0xECE7F7),
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
    bar: hex(0x22272E),
    rule: hex(0x2B3138),
    rule_strong: hex(0x67717D),
    brass_soft: hex(0x2A2140),
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

/// The type scale, in points. The chrome sets controls and running text at
/// UI (13, the family's body size), secondary lines at LABEL, fine print at
/// NOTE, section heads at CAPTION and a page's heading at BODY.
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
pub const RADIUS: f32 = 5.0;
pub const RADIUS_WELL: f32 = 8.0;
pub const MOTION_PRESS: f32 = 0.1;
pub const MOTION_STATE: f32 = 0.2;

/// The faces the window uses (fonts.rs registers them): the chrome's
/// system sans at two weights, and the script's Courier Prime and Literata.
pub const CHROME: &str = "chrome";
pub const CHROME_STRONG: &str = "chrome-strong";
pub const STRUCTURE: &str = "courier";
pub const STRUCTURE_BOLD: &str = "courier-bold";
pub const PROSE: &str = "literata";

/// The chrome's text: the system UI sans, regular.
pub fn chrome(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(CHROME.into()))
}

/// The chrome's strong text (headings, the primary button, the selected
/// tab): the system UI sans at 600, the family's `font-weight: 600`.
pub fn chrome_strong(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(CHROME_STRONG.into()))
}

pub fn structure(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(STRUCTURE.into()))
}

pub fn structure_bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(STRUCTURE_BOLD.into()))
}

pub fn prose(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(PROSE.into()))
}

/// egui's own widgets (check boxes, radio buttons, menus, sliders, text
/// fields, tooltips) in the family's dress: the chrome's face, the rule
/// hairline, the control radius, the accent only for selection and focus.
pub fn apply_style(ctx: &egui::Context) {
    for (theme, t) in [(egui::Theme::Light, LIGHT), (egui::Theme::Dark, DARK)] {
        ctx.style_mut_of(theme, |style| dress(style, &t));
    }
}

fn dress(style: &mut egui::Style, t: &Tokens) {
    use egui::{CornerRadius, Stroke, TextStyle};
    style.text_styles = [
        (TextStyle::Small, chrome(size::NOTE)),
        (TextStyle::Body, chrome(size::UI)),
        (TextStyle::Button, chrome(size::UI)),
        (TextStyle::Heading, chrome_strong(size::BODY)),
        (TextStyle::Monospace, structure(size::UI)),
    ]
    .into();
    style.spacing.button_padding = egui::vec2(10.0, 4.0);
    style.spacing.interact_size.y = 24.0;
    style.spacing.icon_width = 16.0;
    style.spacing.icon_width_inner = 9.0;
    let v = &mut style.visuals;
    let radius = CornerRadius::same(RADIUS as u8);
    v.panel_fill = t.paper;
    v.window_fill = t.panel;
    v.window_stroke = Stroke::new(1.0, t.rule);
    v.window_corner_radius = CornerRadius::same(RADIUS_WELL as u8);
    v.menu_corner_radius = CornerRadius::same(RADIUS_WELL as u8);
    v.extreme_bg_color = t.panel;
    v.text_edit_bg_color = Some(t.panel);
    v.faint_bg_color = t.bar;
    v.code_bg_color = t.bar;
    v.hyperlink_color = t.brass;
    v.error_fg_color = t.alarm;
    v.warn_fg_color = t.note;
    v.selection.bg_fill = t.brass_soft;
    v.selection.stroke = Stroke::new(1.0, t.brass);
    v.text_cursor.stroke = Stroke::new(2.0, t.brass);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = t.paper;
    w.noninteractive.weak_bg_fill = t.paper;
    w.noninteractive.bg_stroke = Stroke::new(1.0, t.rule);
    w.noninteractive.fg_stroke = Stroke::new(1.0, t.ink_soft);
    w.noninteractive.corner_radius = radius;
    // `bg_fill` is a slider's rail and a check box's ground; `weak_bg_fill`
    // a button's.
    w.inactive.bg_fill = t.bar;
    w.inactive.weak_bg_fill = t.panel;
    w.inactive.bg_stroke = Stroke::new(1.0, t.rule_strong.lerp_to_gamma(t.rule, 0.6));
    w.inactive.fg_stroke = Stroke::new(1.0, t.ink);
    w.inactive.corner_radius = radius;
    w.hovered.bg_fill = t.bar;
    w.hovered.weak_bg_fill = t.bar;
    w.hovered.bg_stroke = Stroke::new(1.0, t.rule_strong);
    w.hovered.fg_stroke = Stroke::new(1.5, t.ink);
    w.hovered.corner_radius = radius;
    w.hovered.expansion = 0.0;
    w.active.bg_fill = t.brass_soft;
    w.active.weak_bg_fill = t.brass_soft;
    w.active.bg_stroke = Stroke::new(1.0, t.brass);
    w.active.fg_stroke = Stroke::new(1.5, t.ink);
    w.active.corner_radius = radius;
    w.active.expansion = 0.0;
    w.open.bg_fill = t.panel;
    w.open.weak_bg_fill = t.bar;
    w.open.bg_stroke = Stroke::new(1.0, t.rule_strong);
    w.open.fg_stroke = Stroke::new(1.0, t.ink);
    w.open.corner_radius = radius;
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

    fn fields(t: &Tokens) -> [(&'static str, Color32); 22] {
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
            ("bar", t.bar),
            ("rule", t.rule),
            ("rule-strong", t.rule_strong),
            ("brass-soft", t.brass_soft),
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
