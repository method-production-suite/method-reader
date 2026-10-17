//! `cargo xtask device-checklist`: was tools/device-checklist.ts.
//!
//! Prints the device-pass checklist for tests/fixtures/torture.pdf,
//! generated from tools/torture-manifest.json rather than hand-maintained,
//! so it cannot drift from the coverage the fixture actually has.
//! Hand-keeping it would guarantee a second, stale copy of the coverage
//! list, which is the failure the manifest exists to prevent.
//!
//! The manifest itself is checked by tests/torture_coverage.rs.

use serde_json::Value;

use crate::paths;

/// Where the manifest lives, from the repository root.
pub const MANIFEST: &str = "tools/torture-manifest.json";

/// One manifest row. `covered: false` with a `why` IS a decision; silence
/// is not (tests/torture_coverage.rs).
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub entry: String,
    pub title: String,
    pub covered: bool,
    pub page: Option<f64>,
    pub side: Option<String>,
    pub how: Option<String>,
    pub why: Option<String>,
    /// Questions about a covered entry that only a person holding the
    /// device can answer, one per line of the checklist.
    pub device_checks: Vec<String>,
    /// Whether the row has a `page` key at all (an uncovered row must not).
    pub has_page_key: bool,
    /// The raw `page` value, so a check can say it is not a number.
    pub page_value: Option<Value>,
}

fn string_field(o: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    o.get(key).and_then(Value::as_str).map(str::to_string)
}

/// Parses the manifest's JSON text.
pub fn parse(text: &str) -> Result<Vec<Row>, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("{MANIFEST}: {e}"))?;
    let Value::Array(rows) = v else {
        return Err(format!("{MANIFEST}: not a list"));
    };
    rows.iter()
        .map(|r| {
            let o = r
                .as_object()
                .ok_or_else(|| format!("{MANIFEST}: a row is not an object"))?;
            Ok(Row {
                entry: string_field(o, "entry").unwrap_or_default(),
                title: string_field(o, "title").unwrap_or_default(),
                covered: o.get("covered").and_then(Value::as_bool).unwrap_or(false),
                page: o.get("page").and_then(Value::as_f64),
                side: string_field(o, "side"),
                how: string_field(o, "how"),
                why: string_field(o, "why"),
                device_checks: o
                    .get("deviceChecks")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default(),
                has_page_key: o.contains_key("page"),
                page_value: o.get("page").cloned(),
            })
        })
        .collect()
}

/// The manifest on disk.
pub fn load() -> Result<Vec<Row>, String> {
    let path = paths::repo_root().join(MANIFEST);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse(&text)
}

/// A page number the way JavaScript prints it in a template string.
fn js_number(n: Option<f64>) -> String {
    match n {
        None => "undefined".into(),
        Some(x) if x.fract() == 0.0 && x.abs() < 1e15 => format!("{}", x as i64),
        Some(x) => format!("{x}"),
    }
}

fn or_undefined(s: &Option<String>) -> &str {
    s.as_deref().unwrap_or("undefined")
}

/// The checklist, exactly as the TypeScript printed it (without the final
/// newline `console.log` adds).
pub fn render(manifest: &[Row]) -> String {
    let mut device: Vec<&Row> = manifest
        .iter()
        .filter(|r| r.covered && matches!(r.side.as_deref(), Some("device" | "both")))
        .collect();
    device.sort_by(|a, b| {
        a.page
            .unwrap_or(0.0)
            .total_cmp(&b.page.unwrap_or(0.0))
            .then_with(|| a.entry.cmp(&b.entry))
    });
    let uncovered: Vec<&Row> = manifest.iter().filter(|r| !r.covered).collect();
    let checked: Vec<&Row> = manifest
        .iter()
        .filter(|r| !r.device_checks.is_empty())
        .collect();
    // The picture entries (#19, #19a, #19b) and Method Reader's own AZW3 (#20)
    // each get a section of their own: different questions about the same
    // book.
    let pictures: Vec<&Row> = checked
        .iter()
        .copied()
        .filter(|r| r.entry.starts_with("19"))
        .collect();
    let azw3: Vec<&Row> = checked
        .iter()
        .copied()
        .filter(|r| r.entry == "20")
        .collect();
    let picture_checks: usize = pictures.iter().map(|r| r.device_checks.len()).sum();
    let azw3_checks: usize = azw3.iter().map(|r| r.device_checks.len()).sum();

    let mut out: Vec<String> = Vec::new();
    let mut push = |s: &str| out.push(s.to_string());
    for line in [
        "# Device pass: THE PROOF SHEET",
        "",
        "Convert `tests/fixtures/torture.pdf` and sideload it over USB into",
        "the Kindle's `documents/` folder.",
        "",
        "**Use KFX.** AZW3 and MOBI ignore widows/orphans entirely (registry",
        "17), so on those formats the split-minimums check below would pass",
        "without proving anything.",
        "",
        "**Read it at two font sizes at least.** Page breaks move with font",
        "size, so a keep that holds at one size can fail at another. Changing",
        "size re-rolls every break point and turns one file into two trials.",
        "",
        "Speeches name their own numbers, so a defect reports as \"speech",
        "twenty-seven, one line alone at the bottom\" and can be found again.",
        "",
    ] {
        push(line);
    }
    for row in &device {
        push(&format!(
            "- [ ] **#{}, {}** (sheet {})",
            row.entry,
            row.title,
            js_number(row.page)
        ));
        push(&format!("      {}", or_undefined(&row.how)));
        push("");
    }
    push("## Not covered by this fixture");
    push("");
    for row in &uncovered {
        push(&format!(
            "- **#{}, {}.** {}",
            row.entry,
            row.title,
            or_undefined(&row.why)
        ));
    }
    push("");

    if !pictures.is_empty() {
        for line in [
            "## The pictures",
            "",
            "torture.pdf draws its own pictures, so these are checks on the same",
            "book as everything above, except two. The picture size an older",
            "Kindle accepts in a .mobi has four small books of its own, built with",
            "`cargo xtask mobi-picture-probe --out <folder>`. And the default book",
            "makes its title card the cover (#19b), so the check on a full-page",
            "plate at the front reads a second build with the cover off, whose",
            "command that check gives. What each of torture.pdf's pictures is, and",
            "which sheet it is on, is in the #19, #19a and #19b entries above.",
            "",
            "Read them in the KFX or the AZW3. The .mobi, the file for the oldest",
            "Kindles, is text-only for now (MOBI_CARRIES_PICTURES in",
            "src/mobi/html.ts), until the probe books above open with every",
            "picture on an older Kindle; its rows below say what to check either",
            "way. Once it carries them, it has no way to give a picture a share",
            "of the text column, so the device sizes each one there: a width",
            "unlike the KFX's is not a finding in the .mobi.",
            "",
            "Dark mode has nothing to show here: none of these pictures has",
            "transparency, so each is a solid rectangle on a dark page as on a",
            "light one. How a reader composites a transparent picture onto a dark",
            "page needs a script that has one.",
            "",
        ] {
            push(line);
        }
        for row in &pictures {
            push(&format!("### #{}, {}", row.entry, row.title));
            push("");
            for check in &row.device_checks {
                push(&format!("- [ ] {check}"));
                push("");
            }
        }
    }

    if !azw3.is_empty() {
        for line in [
            "## Method Reader's own AZW3 (beta)",
            "",
            "The same book as an AZW3 that Method Reader writes itself, with no",
            "Calibre: what a Kindle gets over USB wherever KFX is not set up. It",
            "has passed Calibre's own reader in the tests and has never been on a",
            "Kindle, so these come first on a device pass. The command that builds",
            "it is in the #20 entry above.",
            "",
        ] {
            push(line);
        }
        for row in &azw3 {
            for check in &row.device_checks {
                push(&format!("- [ ] {check}"));
                push("");
            }
        }
    }

    push(&format!(
        "{} device-side entries, {} declared gaps, {} picture checks, {} AZW3 checks.",
        device.len(),
        uncovered.len(),
        picture_checks,
        azw3_checks
    ));
    push("");
    push("Findings go in the named entry's `Device verdict:` slot in");
    push("`docs/formatting-options-log.md`, in your own words.");
    out.join("\n")
}

/// `cargo xtask device-checklist`.
pub fn main(args: &[String]) -> u8 {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("usage: cargo xtask device-checklist");
        return 0;
    }
    match load() {
        Ok(rows) => {
            println!("{}", render(&rows));
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}
