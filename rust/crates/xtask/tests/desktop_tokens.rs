//! Port of tests/desktop-tokens.test.ts (9 checks), plus the one check that
//! outlives desktop/ui: sp-app's theme.rs agrees with brand/tokens.json and
//! brand/tokens.css, read from sp-app's source without building it.

use std::fs;

use xtask::desktop_tokens::{CORE_IN_STYLE_CSS, brand_colors, build_desktop_tokens};
use xtask::paths::repo_root;

fn read(p: &str) -> String {
    fs::read_to_string(repo_root().join(p)).unwrap_or_else(|e| panic!("{p}: {e}"))
}

fn generated() -> String {
    read("desktop/ui/tokens.css")
}

fn has_desktop_ui() -> bool {
    repo_root().join("desktop/ui/tokens.css").is_file()
}

/// `--name: value;` pairs, as the TypeScript's /--([\w-]+):\s*([^;]+);/g found them.
fn declarations(css: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = css;
    while let Some(at) = rest.find("--") {
        rest = &rest[at + 2..];
        let name_len = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
            .unwrap_or(rest.len());
        let name = &rest[..name_len];
        let after = &rest[name_len..];
        if name.is_empty() || !after.starts_with(':') {
            continue;
        }
        let value = after[1..].trim_start();
        if let Some(semi) = value.find(';')
            && semi > 0
        {
            out.push((name.to_string(), value[..semi].to_string()));
        }
    }
    out
}

#[test]
fn the_committed_file_is_exactly_what_the_generator_produces() {
    if !has_desktop_ui() {
        eprintln!("skipped: desktop/ui is gone (cutover)");
        return;
    }
    let built =
        build_desktop_tokens(&read("brand/tokens.json"), &read("brand/tokens.css")).unwrap();
    assert_eq!(generated(), built);
}

#[test]
fn every_colour_it_declares_matches_brand_tokens_json() {
    if !has_desktop_ui() {
        return;
    }
    let colors = brand_colors(&read("brand/tokens.json")).unwrap();
    let g = generated();
    let light = &g[..g.find("@media").unwrap()];
    let mut checked = 0;
    for (name, value) in declarations(light) {
        let Some((_, l, _)) = colors.iter().find(|(n, _, _)| *n == name) else {
            continue;
        };
        assert_eq!(value.trim().to_lowercase(), l.to_lowercase(), "{name}");
        checked += 1;
    }
    assert!(checked >= 8, "only {checked} colours checked");
}

#[test]
fn it_does_not_redeclare_the_seven_style_css_already_pins() {
    let g = build_desktop_tokens(&read("brand/tokens.json"), &read("brand/tokens.css")).unwrap();
    for name in CORE_IN_STYLE_CSS {
        assert!(
            !declarations(&g).iter().any(|(n, _)| n == name),
            "tokens.css redeclares --{name}, which style.css owns"
        );
    }
    assert_eq!(CORE_IN_STYLE_CSS.len(), 7);
}

#[test]
fn it_carries_the_scales_the_components_need() {
    let g = build_desktop_tokens(&read("brand/tokens.json"), &read("brand/tokens.css")).unwrap();
    for token in [
        "--font-structure",
        "--font-prose",
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
        "--measure",
        "--brad-size",
        "--page-shadow",
        "--block-max",
        "--binding-margin",
        "--page-right",
        "--hole-center",
        "--hole",
        "--ink-soft",
        "--brass-highlight",
        "--brass-rim",
    ] {
        assert!(
            g.contains(&format!("{token}:")),
            "tokens.css is missing {token}"
        );
    }
}

#[test]
fn the_dark_block_overrides_the_mode_dependent_tokens_it_owns() {
    let g = build_desktop_tokens(&read("brand/tokens.json"), &read("brand/tokens.css")).unwrap();
    let dark = &g[g.find("@media").unwrap()..];
    assert!(dark.contains("--hole:"));
    assert!(dark.contains("--ink-soft:"));
    let colors = brand_colors(&read("brand/tokens.json")).unwrap();
    for want in ["ink-soft", "hole"] {
        let (_, _, d) = colors.iter().find(|(n, _, _)| n == want).unwrap();
        assert!(dark.contains(d.as_str()), "{want}");
    }
}

#[test]
fn a_moved_scale_block_is_refused_rather_than_silently_dropped() {
    let css = read("brand/tokens.css").replace("--measure:", "--elsewhere:");
    let err = build_desktop_tokens(&read("brand/tokens.json"), &css).unwrap_err();
    assert!(err.contains("--measure:"), "{err}");
}

// ---- the fonts are bundled, not assumed (desktop/ui until cutover) ----

fn names_in(dir: &str) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(repo_root().join(dir))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[test]
fn the_same_six_subsets_the_website_self_hosts_are_present() {
    if !has_desktop_ui() {
        return;
    }
    let site = names_in("site/fonts");
    assert_eq!(names_in("desktop/ui/fonts"), site);
    assert_eq!(site.len(), 6);
}

#[test]
fn each_is_a_real_woff2_and_byte_identical_to_the_website_copy() {
    if !has_desktop_ui() {
        return;
    }
    for name in names_in("desktop/ui/fonts") {
        let bytes = fs::read(repo_root().join("desktop/ui/fonts").join(&name)).unwrap();
        assert_eq!(&bytes[..4], b"wOF2", "{name}");
        assert!(bytes.len() > 2000, "{name}");
        assert_eq!(
            bytes,
            fs::read(repo_root().join("site/fonts").join(&name)).unwrap(),
            "{name}"
        );
    }
}

#[test]
fn third_party_notices_no_longer_says_the_app_never_uses_them() {
    let notices = read("THIRD-PARTY-NOTICES.md");
    let flat = notices.split_whitespace().collect::<Vec<_>>().join(" ");
    if has_desktop_ui() {
        assert!(flat.contains("desktop/ui/fonts/"));
    }
    assert!(!flat.contains("Website only; the app has no running prose"));
}

// ---- sp-app's theme.rs against the sources it was copied from ----

fn theme_source() -> String {
    fs::read_to_string(repo_root().join("rust/crates/sp-app/src/theme.rs"))
        .expect("rust/crates/sp-app/src/theme.rs")
}

/// The body between `start` and the next `end`.
fn block<'a>(src: &'a str, start: &str, end: &str) -> &'a str {
    let at = src
        .find(start)
        .unwrap_or_else(|| panic!("theme.rs no longer has {start}"))
        + start.len();
    let len = src[at..].find(end).unwrap();
    &src[at..at + len]
}

fn hex_of(expr: &str) -> Option<[u8; 4]> {
    let inner = expr.strip_prefix("hex(0x")?.strip_suffix(')')?;
    let v = u32::from_str_radix(inner, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255])
}

/// Every colour theme.rs gives LIGHT or DARK, as RGBA.
fn theme_colors(src: &str, which: &str) -> Vec<(String, [u8; 4])> {
    let brass: Vec<[u8; 4]> = block(src, "const BRASS: [Color32; 7] = [", "];")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| hex_of(s).unwrap_or_else(|| panic!("BRASS entry {s}")))
        .collect();
    let body = block(src, &format!("pub const {which}: Tokens = Tokens {{"), "};");
    let mut out = Vec::new();
    for line in body.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (field, expr) = line.split_once(':').unwrap();
        let expr = expr.trim().trim_end_matches(',');
        let rgba = if let Some(c) = hex_of(expr) {
            c
        } else if let Some(i) = expr
            .strip_prefix("BRASS[")
            .and_then(|r| r.strip_suffix(']'))
        {
            brass[i.parse::<usize>().unwrap()]
        } else if let Some(args) = expr
            .strip_prefix("Color32::from_rgba_unmultiplied_const(")
            .and_then(|r| r.strip_suffix(')'))
        {
            let n: Vec<u8> = args.split(',').map(|a| a.trim().parse().unwrap()).collect();
            [n[0], n[1], n[2], n[3]]
        } else {
            panic!("theme.rs {which}.{field}: cannot read {expr}");
        };
        out.push((field.trim().replace('_', "-"), rgba));
    }
    out
}

/// A brand colour, `#RRGGBB` or `rgba(r,g,b,a)`, as RGBA with the alpha
/// rounded to a byte.
fn brand_rgba(value: &str) -> [u8; 4] {
    if let Some(h) = value.strip_prefix('#') {
        let v = u32::from_str_radix(h, 16).unwrap();
        return [(v >> 16) as u8, (v >> 8) as u8, v as u8, 255];
    }
    let args = value
        .strip_prefix("rgba(")
        .and_then(|r| r.strip_suffix(')'))
        .unwrap_or_else(|| panic!("brand colour {value}"));
    let p: Vec<&str> = args.split(',').map(str::trim).collect();
    let alpha: f64 = p[3].parse().unwrap();
    [
        p[0].parse().unwrap(),
        p[1].parse().unwrap(),
        p[2].parse().unwrap(),
        (alpha * 255.0).round() as u8,
    ]
}

#[test]
fn sp_app_s_colours_are_brand_tokens_json_s_in_both_modes() {
    let src = theme_source();
    let colors = brand_colors(&read("brand/tokens.json")).unwrap();
    for (which, dark) in [("LIGHT", false), ("DARK", true)] {
        let theme = theme_colors(&src, which);
        assert!(theme.len() >= 17, "{which} has {} colours", theme.len());
        for (name, rgba) in &theme {
            let (_, l, d) = colors
                .iter()
                .find(|(n, _, _)| n == name)
                .unwrap_or_else(|| {
                    panic!("theme.rs {which} has {name}, which brand/tokens.json does not")
                });
            let want = brand_rgba(if dark { d } else { l });
            assert_eq!(*rgba, want, "{which} {name}");
        }
        // And the other way: no brand colour the window lacks.
        for (name, _, _) in &colors {
            assert!(
                theme.iter().any(|(n, _)| n == name),
                "brand/tokens.json has {name}, which theme.rs {which} does not"
            );
        }
    }
}

#[test]
fn sp_app_s_type_scale_radii_and_motion_are_brand_tokens_css_s() {
    let src = theme_source();
    let css = read("brand/tokens.css");
    let decl = declarations(&css[..css.find("@media").unwrap_or(css.len())]);
    let px = |name: &str| -> f32 {
        let (_, v) = decl
            .iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("brand/tokens.css has no --{name}"));
        let v = v.trim();
        if let Some(ms) = v.strip_suffix("ms") {
            ms.parse::<f32>().unwrap() / 1000.0
        } else if let Some(s) = v.strip_suffix('s') {
            s.parse().unwrap()
        } else {
            v.trim_end_matches("px").parse().unwrap()
        }
    };
    let theme = |konst: &str| -> f32 {
        let line = src
            .lines()
            .map(str::trim)
            .find(|l| l.starts_with(&format!("pub const {konst}: f32 = ")))
            .unwrap_or_else(|| panic!("theme.rs has no {konst}"));
        line.rsplit("= ")
            .next()
            .unwrap()
            .trim_end_matches(';')
            .parse()
            .unwrap()
    };
    for (konst, token) in [
        ("FINE", "text-fine"),
        ("NOTE", "text-note"),
        ("LABEL", "text-label"),
        ("UI", "text-ui"),
        ("CAPTION", "text-caption"),
        ("CODE", "text-code"),
        ("BODY", "text-body"),
        ("LEDE", "text-lede"),
        ("TITLE", "text-title"),
        ("DISPLAY", "text-display"),
        ("RADIUS_MARK", "radius-mark"),
        ("RADIUS", "radius"),
        ("RADIUS_WELL", "radius-well"),
        ("MOTION_PRESS", "motion-press"),
        ("MOTION_STATE", "motion-state"),
    ] {
        assert_eq!(theme(konst), px(token), "{konst} against --{token}");
    }
}
