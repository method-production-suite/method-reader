//! `cargo xtask require-release-notes`: was tools/hooks/require-release-notes.sh.
//!
//! A Claude Code PreToolUse hook body for Bash tool calls: it reads the
//! pending tool call's JSON on stdin and blocks (exit 2, the reason on
//! stderr) creating or pushing a version tag (`git tag`, tag-moving
//! `git push` forms, `gh release create`) when docs/releases/<version>.md
//! is not committed at HEAD. Any other answer is exit 0.
//!
//! THIS IS AN ERGONOMICS GUARD, NOT A SECURITY BOUNDARY. A tag typed into a
//! terminal, made by another tool or pushed from another machine is never
//! seen; the checks that gate a release are CI's (ci.yml and release.yml's
//! `checks` job). Forms it knowingly does NOT catch, which CI does:
//! `git -C . tag`, double-space spellings, `git update-ref refs/tags/...`,
//! `git push --mirror`, and `gh api` calls that make a tag or release.
//!
//! The logic is the shell script's, step for step, with its comments:
//!
//! 1. Nothing on stdin (a terminal, or empty input): nothing to block.
//! 2. The command is `.tool_input.command` of the JSON. When the JSON does
//!    not parse (the shell's "jq missing or failed" case), the raw payload
//!    itself is scanned, so a broken payload degrades to "match on raw
//!    text", never to "crash" and never to "allow everything".
//! 3. Cheap bail-out: a command with no `git tag`, no `gh release create`,
//!    and no tag-moving `git push` anywhere in it is waved through. A
//!    WHOLE-STRING test on purpose, so text inside `$(...)` still counts.
//! 4. Recovery forms (`git tag -d`/`--delete`, `git push --delete`/`-d`,
//!    pushing `:refs/tags/...`) and read-only listing are never blocked.
//!    Matching is per shell SEGMENT (split on `;`, `&`, `|` and newline,
//!    each stripped of leading `FOO=bar ` assignments and the keywords
//!    `do then else elif while until if`). "Compound" (a metacharacter AND
//!    more than one `git tag`/`git push`/`gh release create` anywhere) is
//!    exempt only when every invocation is a recovery form. The listing
//!    allowlist applies only when no version is named at all. A command
//!    whose tag-shaped text is never in command position is inert, unless
//!    it has `$(...)` or a backtick that could hide one. In the raw
//!    (unparsed) mode, recovery falls back to a bare substring test,
//!    because never blocking recovery outranks precision there.
//! 5. Every `vX.Y.Z(-pre)?` token is extracted (release.yml's own regex,
//!    leading v stripped). None: fail CLOSED.
//! 6. Each must have docs/releases/<version>.md committed at HEAD
//!    (`git cat-file -e`, never the working tree).
//!
//! The command is only ever MATCHED against: never run, never handed to a
//! shell.

use std::collections::BTreeSet;
use std::io::{IsTerminal, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::LazyLock;

use regex::Regex;

static VERSION_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"v[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.]+)?").expect("regex"));
static INVOCATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"git tag|git push|gh release create").expect("regex"));
static KEYWORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(do|then|else|elif|while|until|if)[[:space:]]+(.*)$").expect("regex")
});
static ASSIGNMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*=[^[:space:]]*[[:space:]]+(.*)$").expect("regex")
});
static TAG_DELETE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^git[[:space:]]+tag[[:space:]]+-d([[:space:]]|$)").expect("regex")
});
static TAG_DELETE_LONG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^git[[:space:]]+tag[[:space:]]+--delete([[:space:]]|$)").expect("regex")
});
static PUSH_DELETE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^git[[:space:]]+push[[:space:]]+(--delete|-d)([[:space:]]|$)").expect("regex")
});
static GIT_TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^git[[:space:]]+tag([[:space:]]|$)").expect("regex"));
static GH_RELEASE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^gh[[:space:]]+release[[:space:]]+create([[:space:]]|$)").expect("regex")
});
static GIT_PUSH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^git[[:space:]]+push([[:space:]]|$)").expect("regex"));
static GIT_TAG_ANY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^git[[:space:]]+tag([[:space:]]+.*)?$").expect("regex"));
static GIT_TAG_BARE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^git[[:space:]]+tag[[:space:]]*$").expect("regex"));
static SHORT_LIST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[[:space:]]-l([[:space:]]|$)").expect("regex"));

/// The hook's answer: the exit code and what goes to stderr.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub code: u8,
    pub stderr: String,
}

fn allow() -> Decision {
    Decision {
        code: 0,
        stderr: String::new(),
    }
}

/// Leading and trailing whitespace off, like the shell's `trim`.
fn trim(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_ascii_whitespace() || c == '\u{b}')
}

/// Strips leading `FOO=bar ` assignments and the shell keywords, so the
/// checks see `git tag --list` however the segment began.
fn normalize_segment(segment: &str) -> String {
    let mut s = segment.to_string();
    loop {
        if let Some(c) = KEYWORD.captures(&s) {
            s = c[2].to_string();
            continue;
        }
        if let Some(c) = ASSIGNMENT.captures(&s) {
            s = c[1].to_string();
            continue;
        }
        return s;
    }
}

/// A normalized segment is a recognized recovery form.
fn is_recovery_form(s: &str) -> bool {
    TAG_DELETE.is_match(s)
        || TAG_DELETE_LONG.is_match(s)
        || s.contains(":refs/tags/")
        || PUSH_DELETE.is_match(s)
}

fn names_tags(s: &str, has_version: bool) -> bool {
    s.contains("--tags") || s.contains("--follow-tags") || s.contains("refs/tags/") || has_version
}

/// The command a Claude Code tool call carries, as `jq -r
/// '.tool_input.command // empty'` would print it; None when the payload
/// is not JSON jq could read.
fn extract_command(raw: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    let command = match &value {
        serde_json::Value::Null => &serde_json::Value::Null,
        serde_json::Value::Object(o) => match o.get("tool_input") {
            None | Some(serde_json::Value::Null) => &serde_json::Value::Null,
            Some(serde_json::Value::Object(t)) => {
                t.get("command").unwrap_or(&serde_json::Value::Null)
            }
            // jq cannot index a string, number or array with a name.
            Some(_) => return None,
        },
        _ => return None,
    };
    let text = match command {
        serde_json::Value::Null | serde_json::Value::Bool(false) => String::new(),
        serde_json::Value::String(s) => s.clone(),
        other => serde_json::to_string_pretty(other).unwrap_or_default(),
    };
    // `$(...)` drops trailing newlines.
    Some(text.trim_end_matches('\n').to_string())
}

const NO_VERSION_TAIL: &str = "
Failing closed: this guard cannot tell whether release notes exist for
an unresolved version, so it blocks rather than risk a tag shipping
without docs/releases/<version>.md. This is an ergonomics guard, not a
security boundary -- CI (.github/workflows/ci.yml and the release.yml
checks job) is what actually gates a release.

Run /release to create the tag and its release notes together.

Recovery is not blocked by this: \"git tag -d ...\", \"git tag --delete
...\", \"git push --delete ...\"/\"git push -d ...\", and pushing a tag
deletion (\":refs/tags/...\") are always allowed, and so is read-only tag
listing (\"git tag\", \"git tag --list\", ...). An unrecognized spelling of
any of those still fails closed like anything else this guard cannot
resolve.
";

const MISSING_TAIL: &str = "
Tagging or pushing a version without release notes ships a version
nobody can read the changelog for. This is an ergonomics guard, not a
security boundary -- CI (.github/workflows/ci.yml and the release.yml
checks job) is what actually gates a release.

Run /release to write the missing notes and create the tag together.

Recovery is not blocked by this: \"git tag -d ...\", \"git tag --delete
...\", \"git push --delete ...\"/\"git push -d ...\", and pushing a tag
deletion (\":refs/tags/...\") are always allowed, and so is read-only tag
listing (\"git tag\", \"git tag --list\", ...). An unrecognized spelling of
any of those still fails closed like anything else this guard cannot
resolve.
";

/// Whether `path` is committed at HEAD in the repository at `cwd`.
fn committed_at_head(cwd: &Path, path: &str) -> bool {
    Command::new("git")
        .args(["cat-file", "-e", &format!("HEAD:{path}")])
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// The hook's decision for one payload. `stdin` is None when stdin is a
/// terminal; `cwd` is where git looks for the notes.
pub fn decide(stdin: Option<&str>, cwd: &Path) -> Decision {
    // Step 1: nothing piped in, or nothing at all, blocks nothing.
    let Some(input) = stdin else {
        return allow();
    };
    let raw = input.trim_end_matches('\n');
    if raw.is_empty() {
        return allow();
    }

    // Step 2: the command, or the raw payload when it cannot be read out.
    let (cmd, extracted) = match extract_command(raw) {
        Some(c) => (c, true),
        None => (raw.to_string(), false),
    };

    // Step 3: not tag-touching at all.
    let has_version_token = VERSION_TOKEN.is_match(&cmd);
    let is_tag_touching = cmd.contains("git tag")
        || cmd.contains("gh release create")
        || (cmd.contains("git push") && names_tags(&cmd, has_version_token));
    if !is_tag_touching {
        return allow();
    }

    // Step 4: recovery forms and read-only listing.
    let meta = cmd.contains(';')
        || cmd.contains('&')
        || cmd.contains('|')
        || cmd.contains("$(")
        || cmd.contains('`')
        || cmd.contains('\n');
    let n_invocations = INVOCATION.find_iter(&cmd).count();
    let is_compound = meta && n_invocations > 1;

    if extracted {
        let mut touching: Vec<String> = Vec::new();
        for line in cmd.split([';', '&', '|', '\n']) {
            let seg = trim(line);
            if seg.is_empty() {
                continue;
            }
            let norm = normalize_segment(seg);
            let pushes_tags =
                GIT_PUSH.is_match(&norm) && names_tags(&norm, VERSION_TOKEN.is_match(&norm));
            if GIT_TAG.is_match(&norm) || GH_RELEASE.is_match(&norm) || pushes_tags {
                touching.push(norm);
            }
        }

        if touching.is_empty() {
            // Every occurrence of tag-shaped text is inert, unless a
            // $(...) or backtick could be hiding a real invocation.
            let has_exec_risk = cmd.contains("$(") || cmd.contains('`');
            if !has_exec_risk {
                return allow();
            }
        }

        if is_compound {
            // Exempt only when EVERY invocation is a recovery form.
            if !touching.is_empty() && touching.iter().all(|s| is_recovery_form(s)) {
                return allow();
            }
        } else if touching.len() == 1 {
            let seg = &touching[0];
            if is_recovery_form(seg) {
                return allow();
            }
            // No legitimate listing names a version, so a create whose
            // MESSAGE mentions "--list" is still checked.
            if !has_version_token && GIT_TAG_ANY.is_match(seg) {
                if GIT_TAG_BARE.is_match(seg) {
                    return allow();
                }
                if [
                    "--list",
                    "--contains",
                    "--points-at",
                    "--sort",
                    "--merged",
                    "--no-merged",
                    "--format",
                ]
                .iter()
                .any(|f| seg.contains(f))
                    || SHORT_LIST.is_match(seg)
                {
                    return allow();
                }
            }
        }
    } else if [
        "tag -d",
        "tag --delete",
        ":refs/tags/",
        "push --delete",
        "push -d",
    ]
    .iter()
    .any(|f| cmd.contains(f))
    {
        // The raw payload: recovery by bare substring, never blocked.
        return allow();
    }

    // Step 5: every version token, deduplicated and sorted.
    let versions: BTreeSet<&str> = VERSION_TOKEN.find_iter(&cmd).map(|m| m.as_str()).collect();
    if versions.is_empty() {
        let truncated = if cmd.chars().count() > 200 {
            format!(
                "{}...[truncated]",
                cmd.chars().take(200).collect::<String>()
            )
        } else {
            cmd.clone()
        };
        return Decision {
            code: 2,
            stderr: format!(
                "require-release-notes: blocked -- this command touches version tags but\n\
                 no vX.Y.Z version could be found in it:\n\n  {truncated}\n{NO_VERSION_TAIL}"
            ),
        };
    }

    // Step 6: git, not the filesystem, for EVERY version found.
    let missing: Vec<String> = versions
        .iter()
        .map(|v| format!("docs/releases/{}.md", &v[1..]))
        .filter(|path| !committed_at_head(cwd, path))
        .collect();
    if missing.is_empty() {
        return allow();
    }
    Decision {
        code: 2,
        stderr: format!(
            "require-release-notes: blocked -- release notes are not committed at\n\
             HEAD for: {}\n{MISSING_TAIL}",
            missing.join(", ")
        ),
    }
}

/// `cargo xtask require-release-notes`: its exit code.
pub fn main(_args: &[String]) -> u8 {
    let stdin = std::io::stdin();
    let input = if stdin.is_terminal() {
        None
    } else {
        let mut bytes = Vec::new();
        // An unreadable stdin is treated as empty, like `cat || true`.
        let _ = stdin.lock().read_to_end(&mut bytes);
        Some(String::from_utf8_lossy(&bytes).into_owned())
    };
    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let decision = decide(input.as_deref(), &cwd);
    eprint!("{}", decision.stderr);
    decision.code
}
