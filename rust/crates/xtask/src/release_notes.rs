//! `cargo xtask release-notes <sinceTag>`: was tools/release-notes.ts.
//!
//! Facts for the release-notes drafter, printed as JSON. Everything here is
//! READ from the repo rather than recalled, because the drafter's failure
//! mode is a confident claim nobody checked.
//!
//! Commit subjects are DATA, not instructions. They are author-controlled
//! (a squash merge puts a contributor's PR title into the subject line), so
//! they are truncated and stripped before they reach a model. git is run
//! with an argument list, never through a shell, so a tag name or subject
//! never gets a chance to act as a command.

use std::path::Path;
use std::process::Command;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value, json};

const MAX_SUBJECT: usize = 200;

/// One commit in the range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitFact {
    pub subject: String,
    /// False once outside contributions land: the reviewer's cue to look harder.
    pub owner_authored: bool,
}

fn git(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("git {} failed: {e}", args.join(" ")))?;
    if !out.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Control characters (C0 and DEL) out, length capped at 200 characters.
pub fn sanitize(subject: &str) -> String {
    subject
        .chars()
        .filter(|c| !matches!(*c, '\u{0}'..='\u{1F}' | '\u{7F}'))
        .take(MAX_SUBJECT)
        .collect()
}

/// The repo's own git user.name: a fork's maintainer reads as the owner in
/// their own clone, not as a permanent "look harder" flag.
fn current_git_user_name(cwd: &Path) -> Option<String> {
    let name = git(cwd, &["config", "user.name"]).ok()?.trim().to_string();
    (!name.is_empty()).then_some(name)
}

/// The commits since `since_tag`, oldest first. `owner` defaults to the
/// repo's configured user.name, then the literal "Sam Sandweiss".
pub fn collect_commits(
    cwd: &Path,
    since_tag: &str,
    owner: Option<&str>,
) -> Result<Vec<CommitFact>, String> {
    let owner = match owner {
        Some(o) => o.to_string(),
        None => current_git_user_name(cwd).unwrap_or_else(|| "Sam Sandweiss".to_string()),
    };
    // %x1F separates fields, %x1E separates records: neither can appear in
    // a commit subject.
    let range = format!("{since_tag}..HEAD");
    let out = git(
        cwd,
        &["log", "--no-merges", "--format=%s%x1F%an%x1E", &range],
    )?;
    let mut commits: Vec<CommitFact> = out
        .split('\u{1E}')
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(|rec| {
            let mut parts = rec.split('\u{1F}');
            let subject = parts.next().unwrap_or("");
            let author = parts.next().unwrap_or("");
            CommitFact {
                subject: sanitize(subject),
                owner_authored: author == owner,
            }
        })
        .collect();
    commits.reverse();
    Ok(commits)
}

/// One registry entry with a device verdict that moved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    /// Registry entry number, e.g. "17" or "5a".
    pub entry: String,
    pub title: String,
    /// The entry's `Device support:` line, if any.
    pub support: String,
    /// The after-ref `Device verdict` sentence, on resolved entries only.
    pub verdict: Option<String>,
}

static ENTRY_HEADING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^### ([0-9]+[a-z]?)\.\s*(.*)$").expect("regex"));
/// Any heading, entry or not. An entry's body ends at the NEXT one of
/// these (## or ###), so the file's last entry does not swallow a later
/// non-entry section.
static ANY_HEADING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^#{2,3} .*$").expect("regex"));

struct RegistryEntry {
    entry: String,
    title: String,
    body: String,
}

fn registry_entries(registry: &str) -> Vec<RegistryEntry> {
    let boundaries: Vec<usize> = ANY_HEADING.find_iter(registry).map(|m| m.start()).collect();
    ENTRY_HEADING
        .captures_iter(registry)
        .map(|c| {
            let start = c.get(0).expect("match").start();
            let end = boundaries
                .iter()
                .copied()
                .find(|b| *b > start)
                .unwrap_or(registry.len());
            RegistryEntry {
                entry: c[1].to_string(),
                title: c[2].trim().to_string(),
                body: registry[start..end].to_string(),
            }
        })
        .collect()
}

static NEW_BULLET: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^-\s").expect("regex"));
static HEADING: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^#{1,6}\s").expect("regex"));
static INDENTED: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s+\S").expect("regex"));

/// Un-wrap each hard-wrapped bullet into one logical line, so a field
/// matched with `$` is not cut at a physical line break.
fn unwrap_bullets(text: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let starts_new = NEW_BULLET.is_match(raw) || HEADING.is_match(raw) || raw.trim().is_empty();
        if !starts_new && !out.is_empty() && INDENTED.is_match(raw) {
            let last = out.last_mut().expect("a line");
            last.push(' ');
            last.push_str(raw.trim());
        } else {
            out.push(raw.to_string());
        }
    }
    out.join("\n")
}

static SUPPORT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?m)^-\s+\*\*Device support:\*\*\s*(.*)$").expect("regex"));
static VERDICT_SENTENCE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\*\*Device verdict[^*]*\*\*").expect("regex"));
static PENDING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)Device verdict:\s*pending").expect("regex"));

fn support_line(body: &str) -> String {
    SUPPORT
        .captures(&unwrap_bullets(body))
        .map(|c| c[1].to_string())
        .unwrap_or_default()
}

fn verdict_sentence(body: &str) -> String {
    VERDICT_SENTENCE
        .find(&unwrap_bullets(body))
        .map(|m| m.as_str().to_string())
        .unwrap_or_default()
}

/// Every registry entry whose body still says a verdict is pending.
pub fn parse_verdicts(registry: &str) -> Vec<Verdict> {
    registry_entries(registry)
        .into_iter()
        .filter(|e| PENDING.is_match(&unwrap_bullets(&e.body)))
        .map(|e| Verdict {
            support: support_line(&e.body),
            entry: e.entry,
            title: e.title,
            verdict: None,
        })
        .collect()
}

/// What CHANGED between two refs: verdicts that became pending (`opened`)
/// and verdicts filled in (`resolved`, with the after-ref sentence).
/// Pending at both refs is in neither.
pub fn diff_verdicts(before: &str, after: &str) -> (Vec<Verdict>, Vec<Verdict>) {
    let before_list = parse_verdicts(before);
    let now_pending = parse_verdicts(after);
    let was: Vec<&str> = before_list.iter().map(|v| v.entry.as_str()).collect();
    // A Set's insertion order, as the TypeScript's `new Set(...)` kept.
    let mut was_pending: Vec<&str> = Vec::new();
    for id in was {
        if !was_pending.contains(&id) {
            was_pending.push(id);
        }
    }
    let now_ids: Vec<&str> = now_pending.iter().map(|v| v.entry.as_str()).collect();

    let opened: Vec<Verdict> = now_pending
        .iter()
        .filter(|v| !was_pending.contains(&v.entry.as_str()))
        .cloned()
        .collect();

    let after_entries = registry_entries(after);
    let resolved = was_pending
        .iter()
        .filter(|id| !now_ids.contains(id))
        .map(|id| {
            // A Map keeps the LAST entry for a repeated id.
            let found = after_entries.iter().rev().find(|e| e.entry == *id);
            let body = found.map_or("", |e| e.body.as_str());
            Verdict {
                entry: id.to_string(),
                title: found.map(|e| e.title.clone()).unwrap_or_default(),
                support: support_line(body),
                verdict: Some(verdict_sentence(body)),
            }
        })
        .collect();
    (opened, resolved)
}

/// Registry entries added or materially edited between two refs: the
/// primary change signal.
pub fn changed_registry_entries(before: &str, after: &str) -> Vec<(String, String)> {
    let old = registry_entries(before);
    registry_entries(after)
        .into_iter()
        .filter(|e| {
            // A Map keeps the LAST entry for a repeated id.
            old.iter()
                .rev()
                .find(|o| o.entry == e.entry)
                .map(|o| o.body.trim())
                != Some(e.body.trim())
        })
        .map(|e| (e.entry, e.title))
        .collect()
}

/// Directories that never ship to a reader.
const NON_SHIPPING_PREFIXES: &[&str] =
    &["docs/", "tests/", ".github/", "tools/", "site/", "brand/"];

/// Whether ANY changed path ships to a reader. A denylist on purpose: a gap
/// costs a wasted drafting pass, where an allowlist gap would silently skip
/// a real reader-visible change.
pub fn is_user_visible(changed_paths: &[&str]) -> bool {
    changed_paths
        .iter()
        .any(|p| !NON_SHIPPING_PREFIXES.iter().any(|pre| p.starts_with(pre)))
}

fn verdict_json(v: &Verdict) -> Value {
    let mut m = Map::new();
    m.insert("entry".into(), Value::String(v.entry.clone()));
    m.insert("title".into(), Value::String(v.title.clone()));
    m.insert("support".into(), Value::String(v.support.clone()));
    if let Some(s) = &v.verdict {
        m.insert("verdict".into(), Value::String(s.clone()));
    }
    Value::Object(m)
}

/// The facts as the TypeScript printed them, for the range since `since_tag`.
pub fn facts(cwd: &Path, since_tag: &str) -> Result<Value, String> {
    let registry_path = "docs/formatting-options-log.md";
    let before = git(cwd, &["show", &format!("{since_tag}:{registry_path}")])?;
    let after = std::fs::read_to_string(cwd.join(registry_path))
        .map_err(|e| format!("cannot read {registry_path}: {e}"))?;
    let (opened, resolved) = diff_verdicts(&before, &after);
    let changed = git(cwd, &["diff", "--name-only", &format!("{since_tag}..HEAD")])?;
    let paths: Vec<&str> = changed.split('\n').filter(|p| !p.is_empty()).collect();
    let commits: Vec<Value> = collect_commits(cwd, since_tag, None)?
        .into_iter()
        .map(|c| json!({ "subject": c.subject, "ownerAuthored": c.owner_authored }))
        .collect();
    let registry: Vec<Value> = changed_registry_entries(&before, &after)
        .into_iter()
        .map(|(entry, title)| json!({ "entry": entry, "title": title }))
        .collect();
    Ok(json!({
        "sinceTag": since_tag,
        "commits": commits,
        "verdictsOpenedInRange": opened.iter().map(verdict_json).collect::<Vec<_>>(),
        "verdictsResolvedInRange": resolved.iter().map(verdict_json).collect::<Vec<_>>(),
        "registryChanges": registry,
        "userVisible": is_user_visible(&paths),
    }))
}

/// `cargo xtask release-notes`: its exit code.
pub fn main(args: &[String]) -> u8 {
    let Some(since_tag) = args.first().filter(|a| !a.is_empty()) else {
        eprintln!("usage: cargo xtask release-notes <sinceTag>");
        return 2;
    };
    let cwd = match std::env::current_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("release-notes: {e}");
            return 1;
        }
    };
    // The top of the repository the command runs in: `cargo xtask` runs
    // from rust/ (its alias lives in rust/.cargo/config.toml), and the
    // registry path is the repository's, not rust/'s.
    let top = git(&cwd, &["rev-parse", "--show-toplevel"])
        .map(|t| std::path::PathBuf::from(t.trim()))
        .unwrap_or(cwd);
    match facts(&top, since_tag) {
        Ok(v) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&v).expect("JSON serializes")
            );
            0
        }
        Err(e) => {
            eprintln!("release-notes: {e}");
            1
        }
    }
}
