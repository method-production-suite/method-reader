//! `cargo xtask bump-version <version> [--check] [--repo <dir>]`: was
//! tools/bump-version.ts.
//!
//! The release commit's version bump: the version in every file that
//! carries it, and the window's notes regenerated from
//! docs/releases/<version>.md, which must already be written. The /release
//! skill calls this on `ship`; it lives here, not in the skill, because the
//! skill is not versioned and this is logic that has to stay right.
//!
//! [`VERSION_FILES`] lists every file that carries the version: the Rust
//! workspace's rust/Cargo.toml `[workspace.package]` version, the
//! rust/Cargo.lock entry of every crate that inherits it (a bump that
//! misses one leaves a lockfile that disagrees with its own manifests),
//! and sp-app's generated notes, whose own test holds their version to
//! the workspace's. (Until the Rust cutover the table also held the Bun
//! engine's package.json and the Tauri window's Cargo.toml, Cargo.lock
//! entry, tauri.conf.json and desktop/ui/notes.js; they went with the
//! engine and the window.)
//!
//! All or nothing: the notes are parsed the way the window will read them,
//! and every file's new text is worked out, before anything is written. So
//! notes the window cannot show (a second paragraph before the first
//! heading, say) stop it with every file as it was. A write that fails
//! part-way, or the version gate that reads the result back, puts every
//! file back before the error is reported. `--check` does only the parse,
//! for the draft, and writes nothing.
//!
//! Exit codes: 0 done (prints each file it wrote). 1 refused, nothing
//! written, and the reason on stderr.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use crate::desktop_notes;

/// How a version file carries the version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The `version` line of a Cargo.toml table (`package`, or
    /// `workspace.package`).
    CargoTable(&'static str),
    /// The Cargo.lock entry of every crate in the Rust workspace that
    /// inherits the workspace version.
    WorkspaceLockEntries,
    /// A generated notes module (written from the notes, not edited).
    Notes,
}

/// One file the bump writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VersionFile {
    pub path: &'static str,
    pub kind: Kind,
}

/// Every file the bump writes, in the order they are written and listed:
/// the Rust workspace, which builds the engine, the CLI and the window.
///
/// NATIVE APP BUNDLE: sp-release lays the bundles out from the workspace
/// version (build-app-bundle reads rust/Cargo.toml), so no installer
/// config carries a version of its own. One that does is a row here, with
/// the Kind that says how it carries the version, and the gate below reads
/// it back like every other row.
pub const VERSION_FILES: &[VersionFile] = &[
    VersionFile {
        path: "rust/Cargo.toml",
        kind: Kind::CargoTable("workspace.package"),
    },
    VersionFile {
        path: "rust/Cargo.lock",
        kind: Kind::WorkspaceLockEntries,
    },
    VersionFile {
        path: desktop_notes::RUST_MODULE,
        kind: Kind::Notes,
    },
];

/// release.yml's own test of a tag, with the leading v already gone.
static VERSION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.]+)?$").expect("regex"));

static ANY_TABLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^[ \t]*\[").expect("regex"));
static VERSION_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?m)^([ \t]*version[ \t]*=[ \t]*)"([^"]*)""#).expect("regex"));

/// The byte range of a Cargo.toml table's body: from after its `[name]`
/// header line to the next table header.
fn table_span(text: &str, table: &str) -> Option<(usize, usize)> {
    let header = Regex::new(&format!(
        r"(?m)^[ \t]*\[{}\][ \t]*\r?$",
        regex::escape(table)
    ))
    .expect("regex");
    let m = header.find(text)?;
    let start = m.end();
    let end = ANY_TABLE
        .find(&text[start..])
        .map_or(text.len(), |n| start + n.start());
    Some((start, end))
}

/// The version a Cargo.toml table declares (build-app-bundle.ts's
/// cargoPackageVersion, for any table).
pub fn cargo_table_version(text: &str, table: &str) -> Option<String> {
    let (start, end) = table_span(text, table)?;
    VERSION_LINE
        .captures(&text[start..end])
        .map(|c| c[2].to_string())
}

/// The `version` line of a Cargo.toml table, and no other.
pub fn set_cargo_table_version(
    text: &str,
    table: &str,
    version: &str,
    file: &str,
) -> Result<String, String> {
    let Some((start, end)) = table_span(text, table) else {
        return Err(format!("{file} has no version in its [{table}] table"));
    };
    let body = &text[start..end];
    let Some(c) = VERSION_LINE.captures(body) else {
        return Err(format!("{file} has no version in its [{table}] table"));
    };
    let whole = c.get(0).expect("match");
    let bumped = format!(
        "{}{}\"{version}\"{}",
        &body[..whole.start()],
        &c[1],
        &body[whole.end()..]
    );
    Ok(format!("{}{bumped}{}", &text[..start], &text[end..]))
}

fn lock_entry(name: &str) -> Regex {
    Regex::new(&format!(
        r#"(?m)^(\[\[package\]\]\r?\nname = "{}"\r?\nversion = )"([^"]*)""#,
        regex::escape(name)
    ))
    .expect("regex")
}

/// One crate's entry of a Cargo.lock, which must appear exactly once.
pub fn set_cargo_lock_entry(
    text: &str,
    name: &str,
    version: &str,
    file: &str,
) -> Result<String, String> {
    let entry = lock_entry(name);
    let found = entry.find_iter(text).count();
    if found != 1 {
        return Err(format!(
            "{file} has {found} {name} entries; it should have exactly one"
        ));
    }
    Ok(entry
        .replace(text, |c: &regex::Captures| {
            format!("{}\"{version}\"", &c[1])
        })
        .into_owned())
}

fn lock_entry_versions(text: &str, name: &str) -> Vec<String> {
    lock_entry(name)
        .captures_iter(text)
        .map(|c| c[2].to_string())
        .collect()
}

static INHERITS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^[ \t]*version[ \t]*\.[ \t]*workspace[ \t]*=[ \t]*true").expect("regex")
});
static NAME_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?m)^[ \t]*name[ \t]*=[ \t]*"([^"]*)""#).expect("regex"));

/// The name of every crate under rust/crates/ whose `[package]` says
/// `version.workspace = true`, sorted. A crate with its own version keeps
/// it; none has one now that the MTP helper ships inside the Mac app and
/// reports the app's version.
pub fn workspace_crates(repo: &Path) -> Result<Vec<String>, String> {
    let dir = repo.join("rust/crates");
    let read = std::fs::read_dir(&dir).map_err(|e| format!("cannot list rust/crates: {e}"))?;
    let mut names = Vec::new();
    for entry in read.filter_map(Result::ok) {
        let manifest = entry.path().join("Cargo.toml");
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        let Some((start, end)) = table_span(&text, "package") else {
            continue;
        };
        let body = &text[start..end];
        if INHERITS.is_match(body)
            && let Some(c) = NAME_LINE.captures(body)
        {
            names.push(c[1].to_string());
        }
    }
    names.sort();
    if names.is_empty() {
        return Err("no crate under rust/crates inherits the workspace version".to_string());
    }
    Ok(names)
}

/// The version file's new text.
fn next_text(
    repo: &Path,
    file: VersionFile,
    version: &str,
    current: &str,
    notes: &desktop_notes::ReleaseNotes,
) -> Result<String, String> {
    match file.kind {
        Kind::CargoTable(table) => set_cargo_table_version(current, table, version, file.path),
        Kind::WorkspaceLockEntries => {
            let mut text = current.to_string();
            for name in workspace_crates(repo)? {
                text = set_cargo_lock_entry(&text, &name, version, file.path)?;
            }
            Ok(text)
        }
        Kind::Notes => Ok(desktop_notes::render_notes_rust(notes)),
    }
}

/// The bundler's gate, run on what was just written: every version file
/// reads back as `version` (build-app-bundle.ts's assertBundleVersions,
/// for the Rust workspace's files).
fn assert_versions(repo: &Path, files: &[VersionFile], version: &str) -> Result<(), String> {
    for file in files {
        let text = std::fs::read_to_string(repo.join(file.path))
            .map_err(|e| format!("cannot read back {}: {e}", file.path))?;
        let found: Vec<(String, Option<String>)> = match file.kind {
            Kind::CargoTable(table) => {
                vec![(file.path.to_string(), cargo_table_version(&text, table))]
            }
            Kind::WorkspaceLockEntries => {
                let mut out = Vec::new();
                for name in workspace_crates(repo)? {
                    for v in lock_entry_versions(&text, &name) {
                        out.push((format!("{}'s {name} entry", file.path), Some(v)));
                    }
                }
                out
            }
            Kind::Notes => continue,
        };
        for (what, got) in found {
            if got.as_deref() != Some(version) {
                return Err(format!(
                    "build-app-bundle: {what} says {} but the version being built is {version}. \
                     All must agree at a release, or the bundle's filename and the engine inside \
                     it tell a user two different things.",
                    got.as_deref().unwrap_or("<nothing>")
                ));
            }
        }
    }
    Ok(())
}

/// Set `version` everywhere and regenerate the window's notes; returns the
/// files written, in a fixed order. With `check`, only reads the notes.
pub fn bump_version(repo: &Path, version: &str, check: bool) -> Result<Vec<String>, String> {
    if !VERSION.is_match(version) {
        return Err(format!(
            "\"{version}\" is not a version release.yml accepts: MAJOR.MINOR.PATCH, no leading v"
        ));
    }
    let notes_file = format!("docs/releases/{version}.md");
    let Ok(markdown) = std::fs::read_to_string(repo.join(&notes_file)) else {
        return Err(format!(
            "{notes_file} does not exist. Write the notes first; the window's copy is made from them."
        ));
    };
    let notes = desktop_notes::parse_release_notes(version, &markdown)?;
    if check {
        return Ok(Vec::new());
    }

    let files = VERSION_FILES;
    if !repo.join("rust/Cargo.toml").is_file() {
        return Err("found no version files: rust/Cargo.toml is not there".to_string());
    }
    // Everything is read, and every new text worked out, before anything
    // is written, so a write that fails part-way (or the gate after it)
    // can put every file back as it was.
    let read = |path: &str| {
        std::fs::read_to_string(repo.join(path)).map_err(|e| format!("cannot read {path}: {e}"))
    };
    let mut original = Vec::new();
    let mut next = Vec::new();
    for file in files {
        let current = match file.kind {
            Kind::Notes => read(file.path).unwrap_or_default(),
            _ => read(file.path)?,
        };
        next.push((
            file.path,
            next_text(repo, *file, version, &current, &notes)?,
        ));
        original.push((file.path, repo.join(file.path).exists().then_some(current)));
    }

    let attempt = || -> Result<(), String> {
        for (path, text) in &next {
            std::fs::write(repo.join(path), text)
                .map_err(|e| format!("cannot write {path}: {e}"))?;
        }
        assert_versions(repo, files, version)
    };
    if let Err(e) = attempt() {
        let mut stuck = Vec::new();
        for (path, text) in &original {
            let target = repo.join(path);
            let restored = match text {
                Some(t) => std::fs::write(&target, t),
                None => match std::fs::remove_file(&target) {
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    other => other,
                },
            };
            if restored.is_err() {
                stuck.push(*path);
            }
        }
        if !stuck.is_empty() {
            return Err(format!(
                "{e}. Could not put back: {}. Restore them with git checkout before trying again.",
                stuck.join(", ")
            ));
        }
        return Err(e);
    }
    Ok(next.into_iter().map(|(p, _)| p.to_string()).collect())
}

fn run(args: &[String]) -> Result<(), String> {
    let mut check = false;
    let mut repo: Option<PathBuf> = None;
    let mut positionals = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--check" => check = true,
            "--repo" => repo = Some(PathBuf::from(it.next().ok_or("--repo needs a folder")?)),
            other if other.starts_with("--repo=") => {
                repo = Some(PathBuf::from(&other["--repo=".len()..]));
            }
            other if other.starts_with('-') && other.len() > 1 => {
                return Err(format!("unknown option {other}"));
            }
            other => positionals.push(other.to_string()),
        }
    }
    if positionals.len() != 1 {
        return Err("usage: cargo xtask bump-version <version> [--check] [--repo <dir>]".into());
    }
    let version = &positionals[0];
    let repo = repo.unwrap_or_else(crate::paths::repo_root);
    let written = bump_version(&repo, version, check)?;
    if check {
        println!("docs/releases/{version}.md is in the shape the window can show.");
    } else {
        for file in written {
            println!("wrote  {file}");
        }
    }
    Ok(())
}

/// `cargo xtask bump-version`: its exit code.
pub fn main(args: &[String]) -> u8 {
    match run(args) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("bump-version: {e}");
            1
        }
    }
}
