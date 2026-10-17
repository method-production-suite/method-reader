//! Which pictures a capture run changed, compared with the last commit, and
//! a page that shows each one old beside new (was tools/capture/review.ts).
//! `cargo xtask review-screens` is the command; the /release skill runs it
//! at its first moment, so the owner checks the pictures as he checks the
//! notes: that they look right, and that nothing real is in frame (spec
//! 2026-09-22, part 5).
//!
//! Compared with HEAD, staged or not, because the release commit is what
//! they are about to join. Every picture the capture writes goes to two
//! places (assets/screens/ and site/img/): each is one picture to look at,
//! shown once with both paths.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use base64::Engine as _;

use super::shots::{SHOTS, all_outputs};

/// Every folder the capture tool writes a picture into, sorted.
pub fn picture_dirs() -> Vec<String> {
    let mut dirs: Vec<String> = all_outputs(SHOTS)
        .iter()
        .map(|p| p.rsplit_once('/').map_or(".", |(d, _)| d).to_string())
        .collect();
    dirs.sort();
    dirs.dedup();
    dirs
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangedPicture {
    /// Every path holding this picture, sorted.
    pub paths: Vec<String>,
    /// The bytes at HEAD, or None for a new picture.
    pub before: Option<Vec<u8>>,
    /// The bytes on disk now, or None for a removed one.
    pub after: Option<Vec<u8>>,
}

/// A picture's bytes at HEAD and on disk: the key a change is grouped by.
type Pair = (Option<Vec<u8>>, Option<Vec<u8>>);

fn git(repo: &Path, args: &[&str]) -> Result<(bool, Vec<u8>, String), String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .map_err(|e| format!("could not run git: {e}"))?;
    Ok((
        out.status.success(),
        out.stdout,
        String::from_utf8_lossy(&out.stderr).into_owned(),
    ))
}

/// The pictures under [`picture_dirs`] that differ from HEAD, grouped so
/// the same change in two places is listed once, sorted by first path.
pub fn changed_pictures(repo: &Path) -> Result<Vec<ChangedPicture>, String> {
    let dirs = picture_dirs();
    let mut args = vec![
        "status",
        "--porcelain=v1",
        "-z",
        "--untracked-files=all",
        "--",
    ];
    args.extend(dirs.iter().map(String::as_str));
    let (ok, out, err) = git(repo, &args)?;
    if !ok {
        return Err(format!("git status failed: {}", err.trim()));
    }
    // Records are "XY path"; a rename or copy is followed by its old path as
    // a record of its own. Both are looked at: after a rename the old path
    // is gone from disk and shows as removed, and after a copy it is
    // unchanged and drops out below.
    let text = String::from_utf8_lossy(&out);
    let records: Vec<&str> = text.split('\0').collect();
    let mut paths: Vec<String> = Vec::new();
    let mut i = 0;
    while i < records.len() {
        let record = records[i];
        i += 1;
        if record.len() < 4 {
            continue;
        }
        let mut found = vec![record[3..].to_string()];
        if record.starts_with('R') || record.starts_with('C') {
            found.push(records.get(i).copied().unwrap_or("").to_string());
            i += 1;
        }
        paths.extend(found.into_iter().filter(|p| p.ends_with(".png")));
    }
    paths.sort();

    // Keyed by (before, after), in first-seen order.
    let mut order: Vec<Pair> = Vec::new();
    let mut groups: BTreeMap<usize, ChangedPicture> = BTreeMap::new();
    for path in paths {
        let (ok, head, _) = git(repo, &["cat-file", "blob", &format!("HEAD:{path}")])?;
        let before = ok.then_some(head);
        let after = std::fs::read(repo.join(&path)).ok();
        if before.is_some() && before == after {
            continue;
        }
        let key = (before.clone(), after.clone());
        match order.iter().position(|k| *k == key) {
            Some(n) => groups.get_mut(&n).expect("grouped").paths.push(path),
            None => {
                groups.insert(
                    order.len(),
                    ChangedPicture {
                        paths: vec![path],
                        before,
                        after,
                    },
                );
                order.push(key);
            }
        }
    }
    let mut all: Vec<ChangedPicture> = groups.into_values().collect();
    all.sort_by(|a, b| a.paths[0].cmp(&b.paths[0]));
    Ok(all)
}

pub fn kind_of(p: &ChangedPicture) -> &'static str {
    match (&p.before, &p.after) {
        (None, _) => "new",
        (_, None) => "removed",
        _ => "changed",
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn side(label: &str, bytes: Option<&[u8]>, missing: &str) -> String {
    let body = match bytes {
        None => format!("<p class=\"none\">{missing}</p>"),
        Some(b) => format!(
            "<img alt=\"{label}\" src=\"data:image/png;base64,{}\">",
            base64::engine::general_purpose::STANDARD.encode(b)
        ),
    };
    format!("<figure><figcaption>{label}</figcaption>{body}</figure>")
}

/// One self-contained page: every picture is inside it, so it opens from
/// any folder, in any viewer, with nothing else beside it.
pub fn review_page(pictures: &[ChangedPicture]) -> String {
    let sections: Vec<String> = pictures
        .iter()
        .map(|p| {
            let paths: Vec<String> = p.paths.iter().map(|x| escape(x)).collect();
            format!(
                "\n<section>\n  <h2>{} <span>{}</span></h2>\n  <div class=\"pair\">\n    {}\n    {}\n  </div>\n</section>",
                paths.join("<br>"),
                kind_of(p),
                side("Before", p.before.as_deref(), "No picture before. This one is new."),
                side("Now", p.after.as_deref(), "Removed. The capture no longer takes this picture."),
            )
        })
        .collect();
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Pictures to check</title>
<style>
body{{margin:0;padding:24px;font:15px/1.5 -apple-system,system-ui,sans-serif;background:#f4f2ec;color:#1d1b16}}
h1{{font-size:20px;margin:0 0 6px}}
.lead{{max-width:44em;margin:0 0 24px}}
section{{margin:0 0 36px}}
h2{{font-size:13px;font-family:ui-monospace,monospace;font-weight:600;margin:0 0 10px}}
h2 span{{font-family:-apple-system,system-ui,sans-serif;font-weight:400;color:#6d6960;margin-left:8px}}
.pair{{display:grid;grid-template-columns:1fr 1fr;gap:16px}}
figure{{margin:0;background:#d9d6ce;padding:10px;border-radius:6px}}
figcaption{{font-size:12px;color:#4a453a;margin-bottom:6px}}
img{{display:block;width:100%;height:auto}}
.none{{margin:40px 0;text-align:center;color:#6d6960}}
</style>
</head>
<body>
<h1>Pictures to check</h1>
<p class="lead">The release will commit these with the notes. Before on the left, now on the right.
Look for anything real in frame: a real script's title, author or character name.
Every picture should show Field Station, the invented script, or no script at all.</p>
{}
</body>
</html>
"#,
        sections.join("\n")
    )
}

/// `cargo xtask review-screens --out <page.html> [--repo <dir>]`: prints
/// "No picture changed." and writes nothing, or writes the page and prints
/// one line per picture. Exit codes: 0 done, 1 refused or failed.
pub fn main(args: &[String], default_repo: &Path) -> u8 {
    match review(args, default_repo) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("review-screens: {e}");
            1
        }
    }
}

fn review(args: &[String], default_repo: &Path) -> Result<(), String> {
    let flags = super::parse_flags(args, &["out", "repo"])?;
    let out = flags
        .get("out")
        .ok_or("usage: cargo xtask review-screens --out <page.html>")?;
    let repo = flags.get("repo").map_or(default_repo, |r| Path::new(r));
    let pictures = changed_pictures(repo)?;
    if pictures.is_empty() {
        println!("No picture changed.");
        return Ok(());
    }
    std::fs::write(out, review_page(&pictures))
        .map_err(|e| format!("could not write {out}: {e}"))?;
    let n = pictures.len();
    println!(
        "{n} picture{} changed. Old beside new: {out}",
        if n == 1 { "" } else { "s" }
    );
    for p in &pictures {
        println!("  {:<7}  {}", kind_of(p), p.paths.join(" "));
    }
    Ok(())
}
