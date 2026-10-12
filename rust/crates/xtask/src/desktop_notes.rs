//! `cargo xtask build-desktop-notes`: was tools/build-desktop-notes.ts.
//!
//! The window has no filesystem and no Markdown library, and it must work
//! offline, so the release notes are turned into data at build time, from
//! docs/releases/<version>.md, as
//! `rust/crates/sp-app/src/model/notes_release.rs`: the window's `RELEASE`
//! constant, which sp-app's notes.rs `include!`s. (Until the Rust cutover
//! it also wrote the Tauri window's desktop/ui/notes.js; that went with the
//! Tauri window.)
//!
//! The subset is exactly the shape docs/release-notes-template.md
//! produces: one H1, a headline paragraph, and H2 sections whose bullets
//! are either the bold-led `**Claim.** Explanation.` form or, under Good
//! to know (template §9), a PLAIN caveat sentence, which may end in a
//! `<!-- caveat: ... -->` marker that is stripped because the template
//! says it never renders. A plain bullet becomes an item with an empty
//! `lead`, not a dropped item: the first version of the generator matched
//! only the bold form, which took 0.5.4's Good to know caveat out of the
//! window with no error at all. Anything still outside the shape (a
//! heading with no bullets under it, or a line that is neither a bullet
//! nor a continuation of one) is refused rather than dropped silently.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

/// The window's constant, `include!`d by sp-app's model/notes.rs.
pub const RUST_MODULE: &str = "rust/crates/sp-app/src/model/notes_release.rs";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub lead: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    pub title: String,
    pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseNotes {
    pub version: String,
    pub headline: String,
    pub sections: Vec<Section>,
}

static BOLD_BULLET: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^- \*\*(.+?)\*\*\s*(.*)$").expect("regex"));
static PLAIN_BULLET: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^- (.+)$").expect("regex"));
static CONTINUATION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s+\S").expect("regex"));
static COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<!--.*?-->").expect("regex"));
static LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([^\]]+)\]\([^)]+\)").expect("regex"));
static BOLD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\*\*([^*]+)\*\*").expect("regex"));
static CODE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`([^`]+)`").expect("regex"));
static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").expect("regex"));

/// Strip the inline markup the template uses, leaving the sentence. Code
/// spans and links keep their TEXT, because the text is the sentence. The
/// caveat marker comment is dropped too: template §9 says it does not
/// render on the GitHub release page, and the window is the same reader.
fn plain(value: &str) -> String {
    let s = COMMENT.replace_all(value, "");
    let s = LINK.replace_all(&s, "$1");
    let s = BOLD.replace_all(&s, "$1");
    let s = CODE.replace_all(&s, "$1");
    let s = SPACES.replace_all(&s, " ");
    s.trim().to_string()
}

/// JSON.stringify of a string, as the error messages quote a line.
fn json(s: &str) -> String {
    serde_json::to_string(s).expect("a string serializes")
}

struct Open {
    lead: String,
    body: Vec<String>,
}

/// The notes as the window will read them, or the reason they cannot be.
pub fn parse_release_notes(version: &str, markdown: &str) -> Result<ReleaseNotes, String> {
    let mut headline = String::new();
    let mut sections: Vec<Section> = Vec::new();
    let mut open: Option<Open> = None;

    fn close(open: &mut Option<Open>, sections: &mut [Section]) {
        if let (Some(o), Some(current)) = (open.take(), sections.last_mut()) {
            current.items.push(Item {
                lead: o.lead,
                body: plain(&o.body.join(" ")),
            });
        }
    }

    for raw in markdown.split('\n') {
        let line = raw.trim_end();
        if line.starts_with("# ") {
            continue; // the title; version carries it
        }
        if let Some(title) = line.strip_prefix("## ") {
            close(&mut open, &mut sections);
            sections.push(Section {
                title: title.trim().to_string(),
                items: Vec::new(),
            });
            continue;
        }
        if let Some(c) = BOLD_BULLET.captures(line) {
            close(&mut open, &mut sections);
            open = Some(Open {
                lead: plain(&c[1]),
                body: vec![c[2].to_string()],
            });
            continue;
        }
        if let Some(c) = PLAIN_BULLET.captures(line) {
            // Good to know's caveats have no bold claim to separate out, so
            // the whole sentence is the body and the lead is empty.
            close(&mut open, &mut sections);
            open = Some(Open {
                lead: String::new(),
                body: vec![c[1].to_string()],
            });
            continue;
        }
        if let Some(o) = open.as_mut()
            && CONTINUATION.is_match(raw)
        {
            // A wrapped continuation of the bullet above, joined with a
            // space: the newline is the file's margin, not the sentence's.
            o.body.push(line.trim().to_string());
            continue;
        }
        close(&mut open, &mut sections);
        if line.trim().is_empty() {
            continue;
        }
        let Some(current) = sections.last() else {
            if headline.is_empty() {
                headline = plain(line);
                continue;
            }
            // A second paragraph before the first heading: the pre-lede
            // slot (template §3.1) or a straight mistake, neither of which
            // this generator understands.
            return Err(format!(
                "{version}.md has a second paragraph before the first \"## \" heading, \
                 which this generator does not understand: {}",
                json(line)
            ));
        };
        // A line under a heading that is neither a bullet nor a
        // continuation of one (the template's optional intro sentence, §3
        // Slot 4, say). Dropping it would lose words a reader was meant to
        // see, so it is refused.
        return Err(format!(
            "{version}.md has a line under \"{}\" that is neither a bullet nor a continuation of one: {}",
            current.title,
            json(line)
        ));
    }
    close(&mut open, &mut sections);

    if headline.is_empty() {
        return Err(format!("{version}.md has no headline paragraph"));
    }
    if sections.is_empty() {
        return Err(format!("{version}.md has no ## sections"));
    }
    if let Some(empty) = sections.iter().find(|s| s.items.is_empty()) {
        return Err(format!(
            "{version}.md's \"## {}\" heading has no bullets under it",
            empty.title
        ));
    }
    Ok(ReleaseNotes {
        version: version.to_string(),
        headline,
        sections,
    })
}

/// A Rust string literal holding exactly `s`.
fn rust_str(s: &str) -> String {
    format!("{s:?}")
}

/// rust/crates/sp-app/src/model/notes_release.rs: the same data as the
/// `RELEASE` constant sp-app's notes.rs declares the types for.
pub fn render_notes_rust(notes: &ReleaseNotes) -> String {
    let mut out = String::new();
    out.push_str("// GENERATED by `cargo xtask build-desktop-notes`: do not edit.\n");
    out.push_str(&format!("// Source: docs/releases/{}.md\n", notes.version));
    out.push_str("// Regenerate with: cargo xtask build-desktop-notes\n");
    out.push_str("// Included by notes.rs, which declares Release, Section and Item.\n");
    out.push('\n');
    out.push_str("pub const RELEASE: Release = Release {\n");
    out.push_str(&format!("    version: {},\n", rust_str(&notes.version)));
    out.push_str(&format!("    headline: {},\n", rust_str(&notes.headline)));
    out.push_str("    sections: &[\n");
    for section in &notes.sections {
        out.push_str("        Section {\n");
        out.push_str(&format!(
            "            title: {},\n",
            rust_str(&section.title)
        ));
        out.push_str("            items: &[\n");
        for item in &section.items {
            out.push_str("                Item {\n");
            out.push_str(&format!(
                "                    lead: {},\n",
                rust_str(&item.lead)
            ));
            out.push_str(&format!(
                "                    body: {},\n",
                rust_str(&item.body)
            ));
            out.push_str("                },\n");
        }
        out.push_str("            ],\n");
        out.push_str("        },\n");
    }
    out.push_str("    ],\n");
    out.push_str("};\n");
    out
}

/// The version the repository is at: the Rust workspace's.
pub fn current_version(repo: &Path) -> Result<String, String> {
    let cargo = repo.join("rust/Cargo.toml");
    let text = std::fs::read_to_string(&cargo).map_err(|e| format!("rust/Cargo.toml: {e}"))?;
    crate::bump_version::cargo_table_version(&text, "workspace.package").ok_or_else(|| {
        "rust/Cargo.toml has no version in its [workspace.package] table".to_string()
    })
}

fn run(args: &[String]) -> Result<Vec<String>, String> {
    let mut repo: PathBuf = crate::paths::repo_root();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--repo" => repo = PathBuf::from(it.next().ok_or("--repo needs a folder")?),
            "-h" | "--help" => {
                return Ok(vec![
                    "usage: cargo xtask build-desktop-notes [--repo <dir>]".to_string(),
                ]);
            }
            other => return Err(format!("unexpected argument: {other}")),
        }
    }
    let version = current_version(&repo)?;
    let source = format!("docs/releases/{version}.md");
    let markdown = std::fs::read_to_string(repo.join(&source))
        .map_err(|e| format!("cannot read {source}: {e}"))?;
    let notes = parse_release_notes(&version, &markdown)?;
    std::fs::write(repo.join(RUST_MODULE), render_notes_rust(&notes))
        .map_err(|e| format!("cannot write {RUST_MODULE}: {e}"))?;
    Ok(vec![format!("wrote {RUST_MODULE} from {source}")])
}

/// `cargo xtask build-desktop-notes`: its exit code.
pub fn main(args: &[String]) -> u8 {
    match run(args) {
        Ok(lines) => {
            for line in lines {
                println!("{line}");
            }
            0
        }
        Err(e) => {
            eprintln!("build-desktop-notes: {e}");
            1
        }
    }
}
