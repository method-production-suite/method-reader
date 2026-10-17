//! What outlives tests/desktop-tokens.test.ts at the Rust cutover: the
//! window's look is the brand's. sp-app's theme.rs agrees with
//! brand/tokens.json and brand/tokens.css (read from sp-app's source
//! without building it), the six WOFF2 subsets the window bundles are the
//! website's, byte for byte, and THIRD-PARTY-NOTICES.md says where they are.
//!
//! Retired with desktop/ui/tokens.css and its generator (`cargo xtask
//! build-desktop-tokens`): the six checks of the generated file (it matched
//! the generator, its colours matched tokens.json, it left style.css's
//! seven alone, it carried the scales, its dark block, a moved scale block
//! refused). The scales and colours they guarded are held in theme.rs by
//! the last two checks here.

use std::fs;

use xtask::brand::brand_colors;
use xtask::paths::repo_root;

fn read(p: &str) -> String {
    // A Windows checkout carries CRLF; git stores LF, and LF is what
    // the generator writes.
    fs::read_to_string(repo_root().join(p))
        .unwrap_or_else(|e| panic!("{p}: {e}"))
        .replace("\r\n", "\n")
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

/// Where the window's fonts live: sp-app embeds them (src/fonts.rs).
const APP_FONTS: &str = "rust/crates/sp-app/fonts";

// ---- the fonts are bundled, not assumed ----

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
    let site = names_in("site/fonts");
    assert_eq!(names_in(APP_FONTS), site);
    assert_eq!(site.len(), 6);
}

#[test]
fn each_is_a_real_woff2_and_byte_identical_to_the_website_copy() {
    for name in names_in(APP_FONTS) {
        let bytes = fs::read(repo_root().join(APP_FONTS).join(&name)).unwrap();
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
    assert!(flat.contains("`rust/crates/sp-app/fonts/`"), "{flat}");
    assert!(!flat.contains("desktop/ui/fonts/"));
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
    let body = block(src, &format!("pub const {which}: Tokens = Tokens {{"), "};");
    let mut out = Vec::new();
    for line in body.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (field, expr) = line.split_once(':').unwrap();
        let expr = expr.trim().trim_end_matches(',');
        let rgba = if let Some(c) = hex_of(expr) {
            c
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
        assert!(theme.len() >= 18, "{which} has {} colours", theme.len());
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
