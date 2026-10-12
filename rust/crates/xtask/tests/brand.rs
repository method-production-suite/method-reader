//! brand/: the tokens, their contrast, and the component previews (was
//! tests/brand-tokens.test.ts, tests/brand-components.test.ts and their
//! helper tests/color-contrast.ts). brand/ stays HTML, CSS and JSON after
//! the cutover; only the checks moved.
//!
//! brand/tokens.json is the source of truth. The window's own copy (sp-app's
//! theme.rs) is held to it by desktop_tokens.rs; this file holds
//! brand/tokens.css and the previews to it, and the colours to WCAG.

mod repo_common;

use regex::Regex;
use repo_common::*;
use serde_json::Value;
use std::collections::BTreeMap;

// ---- the contrast arithmetic (was tests/color-contrast.ts) -------------

/// #RGB or #RRGGBB, as CSS expands it.
fn from_hex(h: &str) -> (u8, u8, u8) {
    let s = h.trim_start_matches('#');
    let w: String = if s.len() == 3 {
        s.chars().flat_map(|c| [c, c]).collect()
    } else {
        s.to_string()
    };
    let p = |i: usize| u8::from_str_radix(&w[i..i + 2], 16).expect("hex");
    (p(0), p(2), p(4))
}

/// WCAG 2.1 relative luminance.
fn luminance((r, g, b): (u8, u8, u8)) -> f64 {
    let f = |c: u8| {
        let s = f64::from(c) / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b)
}

/// WCAG 2.1 contrast ratio, either way round.
fn contrast(a: (u8, u8, u8), b: (u8, u8, u8)) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    let (hi, lo) = if x >= y { (x, y) } else { (y, x) };
    (hi + 0.05) / (lo + 0.05)
}

#[test]
fn black_on_white_is_21_to_1_either_way_round() {
    assert!((contrast(from_hex("#000000"), from_hex("#FFFFFF")) - 21.0).abs() < 1e-5);
    assert!((contrast(from_hex("#FFFFFF"), from_hex("#000000")) - 21.0).abs() < 1e-5);
}

#[test]
fn a_colour_on_itself_is_1_to_1() {
    assert_eq!(contrast(from_hex("#E8A33D"), from_hex("#E8A33D")), 1.0);
}

#[test]
fn three_digit_hex_expands_the_way_css_expands_it() {
    assert_eq!(from_hex("#FA0"), from_hex("#FFAA00"));
}

// ---- the tokens --------------------------------------------------------

fn tokens() -> Value {
    serde_json::from_str(&read("brand/tokens.json")).expect("brand/tokens.json parses")
}

fn colours() -> serde_json::Map<String, Value> {
    tokens()["colors"]
        .as_object()
        .expect("a colors table")
        .clone()
}

fn hex(c: &serde_json::Map<String, Value>, name: &str, mode: &str) -> String {
    c[name][mode]
        .as_str()
        .unwrap_or_else(|| panic!("{name}.{mode}"))
        .to_string()
}

/// (label, foreground token, fg, bg, floor): every pair the contrast test
/// checks. The token name rides along so completeness can be asked of
/// this table directly, not of a second hand-kept list.
fn pairs() -> Vec<(&'static str, &'static str, String, String, f64)> {
    let c = colours();
    let h = |n: &str, m: &str| hex(&c, n, m);
    vec![
        (
            "ink on paper",
            "ink",
            h("ink", "light"),
            h("paper", "light"),
            7.0,
        ),
        (
            "ink-soft on paper",
            "ink-soft",
            h("ink-soft", "light"),
            h("paper", "light"),
            7.0,
        ),
        (
            "ink-muted on paper",
            "ink-muted",
            h("ink-muted", "light"),
            h("paper", "light"),
            4.5,
        ),
        // Both modes, because brass does not change and neither may its
        // label. `ink` is deliberately NOT paired with brass: it flips to
        // cream in dark mode and lands at 1.67:1, the bug ink-on-brass
        // exists to prevent.
        (
            "ink-on-brass on brass",
            "ink-on-brass",
            h("ink-on-brass", "light"),
            h("brass", "light"),
            7.0,
        ),
        (
            "ink-on-brass on brass, dark",
            "ink-on-brass",
            h("ink-on-brass", "dark"),
            h("brass", "dark"),
            7.0,
        ),
        // The brad button's hover ground: a lighter stop of the same ramp.
        (
            "ink-on-brass on brass-highlight",
            "ink-on-brass",
            h("ink-on-brass", "light"),
            h("brass-highlight", "light"),
            7.0,
        ),
        (
            "alarm on paper",
            "alarm",
            h("alarm", "light"),
            h("paper", "light"),
            4.5,
        ),
        (
            "ink on paper, dark",
            "ink",
            h("ink", "dark"),
            h("paper", "dark"),
            7.0,
        ),
        (
            "ink-soft on paper, dark",
            "ink-soft",
            h("ink-soft", "dark"),
            h("paper", "dark"),
            7.0,
        ),
        (
            "ink-muted on paper, dark",
            "ink-muted",
            h("ink-muted", "dark"),
            h("paper", "dark"),
            4.5,
        ),
        (
            "brass on paper, dark",
            "brass",
            h("brass", "dark"),
            h("paper", "dark"),
            7.0,
        ),
        (
            "alarm on paper, dark",
            "alarm",
            h("alarm", "dark"),
            h("paper", "dark"),
            4.5,
        ),
    ]
}

/// The `--name: value;` declarations in a flat block.
fn declared(block: &str) -> BTreeMap<String, String> {
    Regex::new(r"--([\w-]+):\s*([^;]+);")
        .unwrap()
        .captures_iter(block)
        .map(|c| (c[1].to_string(), c[2].trim().to_string()))
        .collect()
}

/// From `marker` to the first `}` after it. For the media query that is
/// the close of its inner :root, so exactly the dark overrides. Assumes a
/// flat :root and one occurrence of each marker, as tokens.css has.
fn block_after<'a>(css: &'a str, marker: &str) -> &'a str {
    let start = css
        .find(marker)
        .unwrap_or_else(|| panic!("tokens.css is missing {marker}"));
    let end = css[start..].find('}').expect("a closing brace") + start;
    &css[start..end]
}

#[test]
fn tokens_css_declares_every_token_in_tokens_json_in_both_modes() {
    let css = read("brand/tokens.css");
    let light = declared(block_after(&css, ":root {"));
    let dark = declared(block_after(&css, "@media (prefers-color-scheme: dark)"));
    assert!(!dark.is_empty(), "the dark block parsed to nothing");
    let c = colours();
    assert!(c.len() > 10);
    for (name, v) in &c {
        let (l, d) = (v["light"].as_str().unwrap(), v["dark"].as_str().unwrap());
        assert_eq!(
            light.get(name).map(String::as_str),
            Some(l),
            "tokens.css :root and tokens.json disagree on --{name}"
        );
        // A token whose dark value is its light one is correctly absent
        // from the dark block: brass is a material, not a hue.
        if d != l {
            assert_eq!(
                dark.get(name).map(String::as_str),
                Some(d),
                "tokens.css dark block and tokens.json disagree on --{name}"
            );
        } else {
            assert!(
                !dark.contains_key(name),
                "--{name} should not be re-declared in dark"
            );
        }
    }
}

#[test]
fn every_text_token_clears_its_floor_on_its_own_background() {
    for (label, _, fg, bg, floor) in pairs() {
        let ratio = contrast(from_hex(&fg), from_hex(&bg));
        assert!(ratio >= floor, "{label} is {ratio:.2}:1, needs {floor}:1");
    }
}

#[test]
fn the_contrast_table_covers_every_text_bearing_token() {
    // Derived from tokens.json's own "text": true, because a second
    // hand-kept list is how ink-footnote once shipped at 2.22:1.
    let exercised: Vec<&str> = pairs().iter().map(|p| p.1).collect();
    let text: Vec<String> = colours()
        .iter()
        .filter(|(_, v)| v["text"] == Value::Bool(true))
        .map(|(k, _)| k.clone())
        .collect();
    assert!(text.len() >= 5, "{text:?}");
    for name in text {
        assert!(
            exercised.contains(&name.as_str()),
            "\"{name}\" is marked \"text\": true in tokens.json but has no contrast pair here; add one"
        );
    }
}

#[test]
fn the_measured_case_brand_readme_cites_ink_muted_clears_where_faint_ink_did_not() {
    // brand/README.md: ink at 55% alpha blends to #7F7C74 on paper and
    // lands at 3.73:1, which is why ink-muted exists.
    let c = colours();
    let paper = from_hex(&hex(&c, "paper", "light"));
    let faint = contrast(from_hex("#7F7C74"), paper);
    assert!((faint - 3.73).abs() < 0.005, "{faint}");
    assert!(contrast(from_hex(&hex(&c, "ink-muted", "light")), paper) >= 4.5);
}

// ---- the component previews --------------------------------------------

fn previews() -> Vec<(String, String)> {
    let dir = root().join("brand/components");
    let mut out: Vec<(String, String)> = std::fs::read_dir(&dir)
        .expect("brand/components")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "html"))
        .map(|p| {
            (
                p.file_stem().unwrap().to_string_lossy().into_owned(),
                std::fs::read_to_string(&p).unwrap(),
            )
        })
        .collect();
    out.sort();
    out
}

#[test]
fn nothing_sets_a_mode_flipping_ink_on_a_fixed_brass_ground() {
    // How the brass button shipped at 1.67:1 in dark mode: --brass holds
    // across modes, --ink does not, and buttons.html put one on the other.
    let rule = Regex::new(r"\{[^{}]*background:\s*var\(--brass\)[^{}]*\}").unwrap();
    let ink = Regex::new(r"color:\s*var\(--ink\)").unwrap();
    let all = previews();
    assert!(!all.is_empty());
    for (name, css) in all {
        for r in rule.find_iter(&css) {
            assert!(
                !ink.is_match(r.as_str()),
                "{name}.html sets color:var(--ink) on a var(--brass) background. --ink flips to cream in dark mode and lands at 1.67:1 there. Use --ink-on-brass."
            );
        }
    }
}

#[test]
fn the_buttons_declare_hover_and_keyboard_focus() {
    let css = read("brand/components/buttons.html");
    assert!(
        css.contains(":hover"),
        "buttons.html declares no :hover state"
    );
    assert!(
        css.contains(":focus-visible"),
        "buttons.html declares no :focus-visible state"
    );
    for variant in ["btn-brad", "btn-outline"] {
        assert!(
            css.contains(&format!(".{variant}:hover")),
            ".{variant} has no :hover state"
        );
    }
    // :enabled matches only form controls, so gating hover on it drops
    // every link variant, and the download CTA is an <a>.
    assert!(
        !css.contains(":hover:enabled"),
        "hover is gated on :enabled, which no <a> ever matches; use :not(:disabled)"
    );
}

/// Line one, with a byte-order mark and a CR taken off. A BOM renders as
/// nothing, so a BOM-prefixed marker and the real one look identical.
fn first_line(text: &str) -> &str {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    text.split('\n').next().unwrap_or("").trim_end_matches('\r')
}

fn ds_card() -> Regex {
    Regex::new(r#"^<!-- @dsCard group="(Brand|Type|Components)" -->$"#).unwrap()
}

#[test]
fn every_expected_preview_is_present() {
    // Kept only to catch a deletion or rename; the invariants below
    // enumerate the folder, so a new file is checked with no list to update.
    let found: Vec<String> = previews().into_iter().map(|(n, _)| n).collect();
    for name in [
        "brad",
        "page-frame",
        "title-block",
        "slugline",
        "transition-rule",
        "buttons",
        "device-table",
        "shot-frame",
        "drop-well",
        "progress",
        "failure-notice",
        "result-card",
    ] {
        assert!(
            found.contains(&name.to_string()),
            "{name}.html is missing from brand/components/"
        );
    }
}

#[test]
fn every_preview_opens_with_a_ds_card_marker_naming_its_group() {
    for (name, text) in previews() {
        assert!(
            ds_card().is_match(first_line(&text)),
            "{name}.html must open with a @dsCard marker"
        );
    }
}

#[test]
fn first_line_strips_a_leading_bom_so_a_bom_prefixed_marker_still_matches() {
    let with_bom = "\u{FEFF}<!-- @dsCard group=\"Brand\" -->\r\n<p>rest of file</p>";
    assert!(ds_card().is_match(first_line(with_bom)));
}

#[test]
fn tokens_css_declares_the_type_radius_and_motion_scales() {
    let css = read("brand/tokens.css");
    for token in [
        "--text-fine",
        "--text-note",
        "--text-label",
        "--text-ui",
        "--text-caption",
        "--text-code",
        "--text-body",
        "--text-lede",
        "--text-title",
        "--text-display",
        "--radius-mark",
        "--radius",
        "--radius-well",
        "--motion-press",
        "--motion-state",
        "--ease",
        "--space-1",
        "--space-10",
    ] {
        assert!(
            css.contains(&format!("{token}:")),
            "tokens.css does not declare {token}"
        );
    }
}

#[test]
fn no_preview_hardcodes_a_font_size_radius_or_easing() {
    // Spacing is deliberately not enforced: eight one-off rem values stay
    // literal because snapping them would have changed the design.
    let size = Regex::new(r"(font-size|border-radius):\s*[\d.]+px").unwrap();
    let ease = Regex::new(r"[\d.]+m?s\s+(ease|linear|cubic-bezier)").unwrap();
    for (name, css) in previews() {
        if let Some(m) = size.find(&css) {
            panic!(
                "{name}.html sets a literal ({}); use a scale token",
                m.as_str()
            );
        }
        if let Some(m) = ease.find(&css) {
            panic!(
                "{name}.html hardcodes the duration \"{}\"; use --motion-* and --ease",
                m.as_str()
            );
        }
    }
}

#[test]
fn previews_use_tokens_never_raw_brand_colours() {
    // White and black carry opacity in the brass gradients; everything
    // else must be a token. SVG fragment ids (#dad, #face) and numeric
    // character references (&#8984;) are the same shape, so they go first.
    let carriers = ["#FFF", "#FFFFFF", "#000", "#000000"];
    let strip = [
        Regex::new(r"url\(#[\w-]+\)").unwrap(),
        Regex::new(r#"href="[^"]*""#).unwrap(),
        Regex::new(r"&#x?[0-9a-fA-F]+;").unwrap(),
    ];
    let literal = Regex::new(r"#[0-9a-fA-F]{3,8}\b").unwrap();
    for (name, raw) in previews() {
        let mut text = raw.clone();
        for re in &strip {
            text = re.replace_all(&text, "").into_owned();
        }
        for m in literal.find_iter(&text) {
            assert!(
                carriers.contains(&m.as_str().to_uppercase().as_str()),
                "{name}.html hardcodes {}; use a var(--token) instead",
                m.as_str()
            );
        }
    }
}
