//! Every workflow in .github/workflows, read as the YAML GitHub will run
//! (was tests/workflows.test.ts, tests/workflow-shell.test.ts,
//! tests/weekly-toolchain.test.ts and the pin rule of
//! tests/release-artifacts.test.ts).
//!
//! Things only GitHub would otherwise find out, and only after a push: a
//! `needs:` naming a job that is gone (GitHub rejects the whole file, so
//! every job in it stops running, silently), a step that still builds the
//! Swift app or runs the Bun engine after both were deleted, an action that
//! rides a mutable tag, a token with more than it needs, and a bash idiom
//! that dies on the macOS runner's bash 3.2.

mod repo_common;

use regex::Regex;
use repo_common::*;
use serde_yaml::Value;
use std::collections::{BTreeMap, BTreeSet};

fn all() -> Vec<(String, Value)> {
    workflow_files()
        .into_iter()
        .map(|f| {
            let wf = workflow(&f);
            (f, wf)
        })
        .collect()
}

#[test]
fn every_workflow_is_found_and_parsed() {
    // A wrong folder or a parse that yields nothing would make every test
    // below pass over an empty list.
    let names = workflow_files();
    for f in [
        "manifest-freshness.yml",
        "release-published.yml",
        "release.yml",
        "checks.yml",
        "full.yml",
        "weekly-toolchain.yml",
    ] {
        assert!(names.contains(&f.to_string()), "{f} is missing: {names:?}");
    }
    // rust.yml built and tested every change; checks.yml and full.yml
    // replaced it (SUITE-STANDARDS section 6, CI ArtCraft-style).
    assert!(!names.contains(&"rust.yml".to_string()));
    for (f, wf) in all() {
        assert!(!jobs(&wf).is_empty(), "{f} has no jobs");
    }
}

#[test]
fn every_needs_names_a_job_in_the_same_workflow() {
    let mut dangling = vec![];
    for (f, wf) in all() {
        for (name, job) in jobs(&wf) {
            for n in needs(job) {
                if !has_job(&wf, &n) {
                    dangling.push(format!("{f}: {name} needs {n}"));
                }
            }
        }
    }
    assert_eq!(dangling, Vec::<String>::new());
}

#[test]
fn no_job_builds_or_runs_the_swift_app_which_is_gone() {
    let workdir = Regex::new(r"^app(/|$)").unwrap();
    let swift = Regex::new(r"\bswift\s+(build|run|test|package)\b").unwrap();
    let mut found = vec![];
    for (f, wf) in all() {
        for (name, job) in jobs(&wf) {
            let mut dirs = vec![
                job.get("defaults")
                    .and_then(|d| d.get("run"))
                    .map(|r| s(r, "working-directory").to_string())
                    .unwrap_or_default(),
            ];
            dirs.extend(
                steps(job)
                    .iter()
                    .map(|st| s(st, "working-directory").to_string()),
            );
            for d in dirs.iter().filter(|d| workdir.is_match(d)) {
                found.push(format!("{f}: {name} works in {d}"));
            }
            for st in steps(job) {
                if swift.is_match(s(st, "run")) {
                    found.push(format!("{f}: {name} runs {}", s(st, "run").trim()));
                }
            }
        }
    }
    assert_eq!(found, Vec::<String>::new());
}

/// A command word, at the start of a command: after the line start, a
/// separator or a subshell, so "the Bun engine" in prose does not count.
fn runs_a_js_tool(line: &str) -> bool {
    let re = Regex::new(r"(^|[\s;&|(`])(bun|bunx|node|npm|npx|pnpm|yarn)(\s|$)").unwrap();
    re.is_match(line)
}

#[test]
fn no_workflow_runs_bun_node_or_npm() {
    // The cutover deleted the TypeScript engine and its tools. A step that
    // still ran one would fail on a Monday, or worse, pass by running
    // nothing; a setup action for one would be a supply-chain surface
    // with nothing to supply.
    let mut found = vec![];
    for (f, wf) in all() {
        for (name, job) in jobs(&wf) {
            for st in steps(job) {
                let uses = s(st, "uses");
                if uses.contains("setup-bun") || uses.contains("setup-node") {
                    found.push(format!("{f}: {name} uses {uses}"));
                }
                for line in commands(s(st, "run")).lines() {
                    if runs_a_js_tool(line) || line.contains("tools/") && line.contains(".ts") {
                        found.push(format!("{f}: {name} runs {}", line.trim()));
                    }
                }
            }
        }
    }
    assert_eq!(found, Vec::<String>::new());
    // The rule itself, so a regex that matches nothing cannot pass above.
    assert!(runs_a_js_tool("bun test"));
    assert!(runs_a_js_tool("  (cd x && npm ci)"));
    assert!(runs_a_js_tool("V=\"$(node -p 1)\""));
    assert!(!runs_a_js_tool("# the Bun engine"));
    assert!(!runs_a_js_tool("cargo build -p sp-cli --bin screepub"));
}

/// Every `uses:` line in the workflows and the repository's own actions.
/// A cargo command that names more than one package with `-p` and also
/// limits the build to a binary with `--bin`. Cargo applies `--bin` to every
/// package named, so the other packages' binaries are never built: v0.12.0's
/// release built `-p sp-cli --bin screepub -p sp-release` and then found no
/// sp-release to run (only release.yml runs that line, and only on a tag).
fn bin_with_two_packages(line: &str) -> bool {
    let words: Vec<&str> = line.split_whitespace().collect();
    if !words.contains(&"cargo") {
        return false;
    }
    let packages = words
        .iter()
        .filter(|w| **w == "-p" || **w == "--package")
        .count();
    let bins = words
        .iter()
        .any(|w| *w == "--bin" || w.starts_with("--bin="));
    packages > 1 && bins
}

#[test]
fn no_cargo_command_limits_two_packages_to_one_bin() {
    let mut found = vec![];
    for (f, wf) in all() {
        for (name, job) in jobs(&wf) {
            for st in steps(job) {
                for line in commands(s(st, "run")).lines() {
                    if bin_with_two_packages(line) {
                        found.push(format!("{f}: {name}: {}", line.trim()));
                    }
                }
            }
        }
    }
    assert_eq!(found, Vec::<String>::new());
    // The rule itself, so a check that matches nothing cannot pass above.
    assert!(bin_with_two_packages(
        "cargo build --release --locked -p sp-cli --bin screepub -p sp-release"
    ));
    assert!(!bin_with_two_packages(
        "cargo build --release --locked -p sp-cli --bin screepub"
    ));
    assert!(!bin_with_two_packages("cargo build -p a -p b"));
}

fn uses_lines() -> Vec<(String, String)> {
    let mut files: Vec<String> = workflow_files()
        .into_iter()
        .map(|f| format!(".github/workflows/{f}"))
        .collect();
    for entry in std::fs::read_dir(root().join(".github/actions")).expect(".github/actions") {
        let dir = entry.expect("an entry").path();
        for name in ["action.yml", "action.yaml"] {
            if dir.join(name).is_file() {
                let rel = dir.join(name);
                let rel = rel.strip_prefix(root()).unwrap().to_string_lossy();
                files.push(rel.replace('\\', "/"));
            }
        }
    }
    let re = Regex::new(r"^\s*-?\s*uses:\s*(.+?)\s*$").unwrap();
    let mut out = vec![];
    for f in files {
        for line in read(&f).lines() {
            if let Some(c) = re.captures(line) {
                out.push((f.clone(), c[1].to_string()));
            }
        }
    }
    out
}

#[test]
fn every_action_is_pinned_to_a_full_sha_with_its_version_beside_it() {
    // A tag can be moved; a commit cannot. The comment is what pinact and
    // the suite-wide pin moves keep honest, and what a reader checks the
    // SHA against.
    let pinned = Regex::new(r"^[\w.-]+/[\w./-]+@[0-9a-f]{40} # v\d+(\.\d+)*$").unwrap();
    let all = uses_lines();
    assert!(all.len() > 10, "found only {} uses: lines", all.len());
    let offenders: Vec<String> = all
        .iter()
        .filter(|(_, u)| !u.starts_with("./") && !pinned.is_match(u))
        .map(|(f, u)| format!("{f}: {u}"))
        .collect();
    assert_eq!(offenders, Vec::<String>::new());
}

#[test]
fn each_action_has_one_pin_everywhere() {
    // weekly-toolchain.yml once had to be checked against ci.yml by hand
    // after every bump. One action at two SHAs is a bump that missed a file.
    let mut pins: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (_, u) in uses_lines() {
        if let Some((action, rest)) = u.split_once('@') {
            pins.entry(action.to_string())
                .or_default()
                .insert(rest.to_string());
        }
    }
    let split: Vec<String> = pins
        .iter()
        .filter(|(_, v)| v.len() > 1)
        .map(|(k, v)| format!("{k}: {v:?}"))
        .collect();
    assert_eq!(split, Vec::<String>::new());
}

fn permissions(v: &Value) -> Option<BTreeMap<String, String>> {
    v.get("permissions").map(|p| {
        p.as_mapping()
            .unwrap_or_else(|| panic!("permissions must be a table, not {p:?}"))
            .iter()
            .map(|(k, v)| {
                (
                    k.as_str().unwrap().to_string(),
                    v.as_str().unwrap().to_string(),
                )
            })
            .collect()
    })
}

fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// A job that reads the release while it is still a DRAFT, which GitHub
/// shows only to a token that can write: release.yml's draft-check.
/// full.yml's check job: it reads the workflow's own past runs, which needs
/// actions: read and nothing else.
fn reads_its_runs(file: &str, name: &str, job: &Value) -> bool {
    file == "full.yml"
        && name == "check"
        && commands(&run_text(job)).contains("actions/workflows/full.yml/runs")
}

fn reads_the_draft(file: &str, name: &str, job: &Value) -> bool {
    file == "release.yml"
        && name == "draft-check"
        && commands(&run_text(job)).contains("gh release view")
}

#[test]
fn every_workflow_starts_read_only_and_grants_write_only_where_it_publishes() {
    let release_write = Regex::new(r"gh release (upload|create|edit)").unwrap();
    let mut wrong = vec![];
    for (f, wf) in all() {
        // Top level: contents: read. (The site deploys from the suite's
        // website repo now, so no workflow here needs a Pages grant.)
        let expected = map(&[("contents", "read")]);
        if permissions(&wf).as_ref() != Some(&expected) {
            wrong.push(format!(
                "{f}: top-level permissions {:?}, want {expected:?}",
                permissions(&wf)
            ));
        }
        for (name, job) in jobs(&wf) {
            let writes = release_write.is_match(&commands(&run_text(job)))
                || reads_the_draft(&f, &name, job);
            match permissions(job) {
                None if writes => wrong.push(format!(
                    "{f}: {name} changes a release with a read-only token"
                )),
                None => {}
                Some(p) if p == map(&[("contents", "write")]) && writes => {}
                Some(p) if p == map(&[("actions", "read")]) && reads_its_runs(&f, &name, job) => {}
                Some(p) => wrong.push(format!(
                    "{f}: {name} asks for {p:?} but {}",
                    if writes {
                        "needs only contents: write"
                    } else {
                        "changes no release"
                    }
                )),
            }
        }
    }
    assert_eq!(wrong, Vec::<String>::new());
}

#[test]
fn every_run_step_that_can_reach_windows_runs_under_bash() {
    // Without it a `run:` block on windows-latest goes to PowerShell, where
    // `set -euo pipefail` and `ARGS=(...)` are syntax errors, and the
    // Windows leg is the one nobody here can try first. A one-line cargo
    // or rustup call means the same in either shell.
    let mut wrong = vec![];
    for (f, wf) in all() {
        for (name, job) in jobs(&wf) {
            let reaches_windows = serde_yaml::to_string(job.get("runs-on").unwrap_or(&Value::Null))
                .unwrap()
                .contains("windows")
                || serde_yaml::to_string(job.get("strategy").unwrap_or(&Value::Null))
                    .unwrap()
                    .contains("windows");
            if !reaches_windows {
                continue;
            }
            let default_bash = job
                .get("defaults")
                .and_then(|d| d.get("run"))
                .is_some_and(|r| s(r, "shell") == "bash");
            for st in steps(job) {
                let run = s(st, "run").trim();
                if run.is_empty() || default_bash || s(st, "shell") == "bash" {
                    continue;
                }
                let guard = s(st, "if");
                if guard.contains("runner.os == 'Linux'") || guard.contains("runner.os == 'macOS'")
                {
                    continue;
                }
                if !run.contains('\n') && (run.starts_with("cargo ") || run.starts_with("rustup "))
                {
                    continue;
                }
                wrong.push(format!("{f}: {name}: {}", run.lines().next().unwrap_or("")));
            }
        }
    }
    assert_eq!(wrong, Vec::<String>::new());
}

/// The arrays a run block assigns a non-empty literal: `NAME=(word ...)`.
fn non_empty_arrays(run: &str) -> BTreeSet<String> {
    let re = Regex::new(r"\b([A-Za-z_][A-Za-z0-9_]*)=\(\s*[^)\s]").unwrap();
    re.captures_iter(run).map(|c| c[1].to_string()).collect()
}

/// What is wrong with one run block under bash 3.2 and `set -u`.
fn empty_array_problems(run: &str) -> Vec<String> {
    // Bare `"${NAME[@]}"`, not preceded by the ${NAME[@]+ guard. On macOS's
    // bash 3.2 expanding an EMPTY array under `set -u` is an unbound
    // variable error. v0.5.0 died on exactly this, AFTER notarization:
    // `"${PRE[@]}"` where PRE=() for every non-prerelease. A bare
    // expansion is safe only of an array this same block fills with a
    // literal list and never empties.
    let bare = Regex::new(r#"(^|[^+])"\$\{([A-Za-z_][A-Za-z0-9_]*)\[@\]\}""#).unwrap();
    let filled = non_empty_arrays(run);
    let emptied: BTreeSet<String> = Regex::new(r"\b([A-Za-z_][A-Za-z0-9_]*)=\(\s*\)")
        .unwrap()
        .captures_iter(run)
        .map(|c| c[1].to_string())
        .collect();
    let mut out = vec![];
    for c in bare.captures_iter(run) {
        let name = &c[2];
        if !filled.contains(name) || emptied.contains(name) {
            out.push(format!("\"${{{name}[@]}}\""));
        }
    }
    out
}

#[test]
fn no_possibly_empty_array_is_expanded_bare() {
    let mut found = vec![];
    for (f, wf) in all() {
        for (name, job) in jobs(&wf) {
            for st in steps(job) {
                for p in empty_array_problems(s(st, "run")) {
                    found.push(format!("{f}: {name}: {p}"));
                }
            }
        }
    }
    assert_eq!(found, Vec::<String>::new());
    // The rule against the shapes that motivated it.
    assert_eq!(
        empty_array_problems("PRE=()\ngh x ${PRE[@]+\"${PRE[@]}\"}"),
        Vec::<String>::new()
    );
    assert_eq!(
        empty_array_problems("PRE=()\n[ x ] && PRE=(--prerelease)\ngh x \"${PRE[@]}\""),
        vec!["\"${PRE[@]}\"".to_string()]
    );
    assert_eq!(
        empty_array_problems("gh x \"${UNSEEN[@]}\""),
        vec!["\"${UNSEEN[@]}\"".to_string()]
    );
    assert_eq!(
        empty_array_problems("RES=(--resource LICENSE)\nsp \"${RES[@]}\""),
        Vec::<String>::new()
    );
}

#[cfg(unix)]
#[test]
fn the_guarded_spelling_survives_an_empty_array_under_the_real_bash() {
    // /bin/bash: on macOS that IS 3.2, the genuine article.
    let out = std::process::Command::new("/bin/bash")
        .args([
            "-c",
            r#"set -euo pipefail; PRE=(); args=(x ${PRE[@]+"${PRE[@]}"}); echo "${#args[@]}""#,
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", said(&out));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "1");
}

#[cfg(unix)]
#[test]
fn and_still_passes_the_flag_through_when_the_array_is_not_empty() {
    let out = std::process::Command::new("/bin/bash")
        .args([
            "-c",
            r#"set -euo pipefail; PRE=(--prerelease); args=(x ${PRE[@]+"${PRE[@]}"}); echo "${args[1]}""#,
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", said(&out));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "--prerelease");
}

// weekly-toolchain.yml's engine-calibre job can only be run by GitHub,
// weekly. These are the parts of it that can be checked here, so a typo or
// a lost guard is caught on the next push instead of on a Monday.

#[test]
fn weekly_the_calibre_job_exists_on_macos_in_the_workspace() {
    let wf = workflow("weekly-toolchain.yml");
    let job = job(&wf, "engine-calibre");
    assert!(s(job, "runs-on").starts_with("macos-"), "{job:?}");
    let dir = job
        .get("defaults")
        .and_then(|d| d.get("run"))
        .map(|r| s(r, "working-directory"));
    assert_eq!(dir, Some("rust"));
    for st in steps(job) {
        assert!(
            !s(st, "run").contains("swift") && !s(st, "working-directory").starts_with("app"),
            "{st:?}"
        );
    }
}

#[test]
fn weekly_installs_calibre_then_tests_with_calibre_required() {
    // The Calibre tests skip without Calibre, so a Calibre the engine can
    // no longer FIND would leave the job green while testing nothing.
    // METHOD_READER_REQUIRE_CALIBRE turns that skip into a failure.
    let wf = workflow("weekly-toolchain.yml");
    let job = job(&wf, "engine-calibre");
    let install = step_index(job, |st| {
        s(st, "run").contains("brew install --cask calibre")
    })
    .expect("a Calibre install step");
    let test = step_index(job, |st| {
        s(st, "run").trim() == "cargo test --workspace --locked"
    })
    .expect("a cargo test --workspace --locked step");
    assert!(install < test, "Calibre must be installed before the tests");
    let env = steps(job)[test].get("env").expect("the test step's env");
    assert_eq!(s(env, "METHOD_READER_REQUIRE_CALIBRE"), "1");
    // The test that reads it exists and reads it.
    let found = read("rust/crates/sp-engine/tests/calibre_found.rs");
    assert!(found.contains("METHOD_READER_REQUIRE_CALIBRE"));
}

#[test]
fn weekly_pins_the_same_actions_full_yml_does() {
    let pins = |f: &str| -> BTreeSet<String> {
        let wf = workflow(f);
        jobs(&wf)
            .into_iter()
            .flat_map(|(_, j)| steps(j))
            .map(|st| s(st, "uses").to_string())
            .filter(|u| !u.is_empty())
            .collect()
    };
    let full = pins("full.yml");
    for u in pins("weekly-toolchain.yml") {
        assert!(
            u.contains('@') && full.contains(&u),
            "{u} differs from full.yml"
        );
    }
}

#[test]
fn the_standing_check_watches_the_updater_manifest() {
    let wf = workflow("manifest-freshness.yml");
    let texts: Vec<String> = jobs(&wf).into_iter().map(|(_, j)| run_text(j)).collect();
    assert!(
        texts
            .iter()
            .any(|t| t.contains("sp-release check-latest --deep"))
    );
    // Never on `release: published`: that is release-published.yml's
    // trigger, and it fires only because a PERSON publishes (GitHub starts
    // no run from an event a workflow raised with the default token, which
    // is why this file's own copy of it never fired at v0.5.3).
    let on = serde_yaml::to_string(wf.get("on").expect("an on: table")).unwrap();
    assert!(!on.contains("release"), "{on}");
}

const DRAFT_GUARD: &str = "github.event_name != 'pull_request' || !github.event.pull_request.draft";

#[test]
fn each_change_runs_only_the_quick_dependency_checks() {
    // SUITE-STANDARDS.md section 6, CI ArtCraft-style: a pull request and a
    // push to main run cargo deny and the crate-age check, and nothing that
    // builds or tests; main's ruleset requires exactly those two. A draft
    // runs neither, and ready_for_review starts the run. A newer push
    // cancels the run it replaces, on main too (the owner, 2026-10-19). release.yml
    // runs its own checks on the tag and never cancels.
    let wf = workflow("checks.yml");
    let on = &wf["on"];
    let push_branches: Vec<&str> = on["push"]["branches"]
        .as_sequence()
        .expect("push is limited to named branches")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(push_branches, ["main"]);
    assert!(
        on["push"].get("tags").is_none(),
        "a tag's checks are release.yml's"
    );
    let types: Vec<&str> = on["pull_request"]["types"]
        .as_sequence()
        .expect("pull_request lists its types")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(
        types,
        ["opened", "synchronize", "reopened", "ready_for_review"]
    );
    assert!(on.get("workflow_dispatch").is_some());
    let c = &wf["concurrency"];
    assert_eq!(c["group"].as_str(), Some("checks-${{ github.ref }}"));
    assert_eq!(c["cancel-in-progress"].as_bool(), Some(true));
    let names: Vec<String> = jobs(&wf).into_iter().map(|(n, _)| n).collect();
    assert_eq!(names, ["deny", "cooldown"]);
    let builds = Regex::new(r"cargo (build|test|clippy|xtask|run)\b").unwrap();
    for (name, j) in jobs(&wf) {
        assert_eq!(job_if(j), DRAFT_GUARD, "{name}");
        assert_eq!(s(j, "runs-on"), "ubuntu-latest", "{name}");
        let run = commands(&run_text(j));
        assert!(!builds.is_match(&run), "{name} builds or tests: {run}");
    }
    let release = workflow("release.yml");
    assert_ne!(
        release["concurrency"]["cancel-in-progress"].as_bool(),
        Some(true),
        "a release run is never cancelled"
    );
}

#[test]
fn the_weekly_full_build_runs_everything_on_every_system() {
    let wf = workflow("full.yml");
    let on = &wf["on"];
    let cron: Vec<&str> = on["schedule"]
        .as_sequence()
        .expect("a schedule")
        .iter()
        .filter_map(|c| c["cron"].as_str())
        .collect();
    assert_eq!(cron.len(), 1);
    assert!(cron[0].ends_with(" * * 1"), "Mondays: {}", cron[0]);
    assert!(on.get("workflow_dispatch").is_some());
    assert!(on.get("push").is_none() && on.get("pull_request").is_none());
    assert_ne!(
        wf["concurrency"]["cancel-in-progress"].as_bool(),
        Some(true)
    );
    let names: BTreeSet<String> = jobs(&wf).into_iter().map(|(n, _)| n).collect();
    let want: BTreeSet<String> = [
        "check", "rust", "parity", "cli", "worker", "deny", "bundles",
    ]
    .map(String::from)
    .into();
    assert_eq!(names, want);
    for (name, j) in jobs(&wf) {
        if name == "check" {
            assert_eq!(job_if(j), "");
            continue;
        }
        assert_eq!(s(j, "needs"), "check", "{name}");
        assert_eq!(job_if(j), "needs.check.outputs.run == 'true'", "{name}");
    }
    let every: Vec<String> = ["ubuntu-latest", "macos-15", "windows-latest"]
        .map(String::from)
        .into();
    for name in ["rust", "bundles"] {
        let os: Vec<String> = job(&wf, name)["strategy"]["matrix"]["os"]
            .as_sequence()
            .unwrap_or_else(|| panic!("{name} has an os matrix"))
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        assert_eq!(os, every, "{name}");
    }
    // The frozen answers are a Mac's.
    assert_eq!(s(job(&wf, "parity"), "runs-on"), "macos-15");
    // Every installer is built: no step waits on a packaging answer.
    for st in steps(job(&wf, "bundles")) {
        assert!(!s(st, "if").contains("decide"), "{}", s(st, "name"));
    }
}

#[test]
fn only_the_weekly_full_build_saves_a_cache() {
    // checks.yml restores (cargo-deny's) and never saves, so each change adds
    // nothing to the repository's 10 GB; full.yml saves every cache, on main.
    for f in workflow_files() {
        let wf = workflow(&f);
        for (name, j) in jobs(&wf) {
            for st in steps(j) {
                if s(st, "uses").starts_with("actions/cache/save@") {
                    assert!(
                        f == "full.yml" || f == "release.yml",
                        "{f}: {name} saves a cache"
                    );
                }
            }
        }
    }
    let wf = workflow("full.yml");
    for name in ["rust", "parity", "cli", "deny", "bundles"] {
        assert!(
            steps(job(&wf, name))
                .iter()
                .any(|st| s(st, "uses").starts_with("actions/cache/save@")),
            "full.yml: {name} saves no cache"
        );
    }
}

/// Run full.yml's check step with a `gh` stub that answers `last` as the
/// last green run's commit; what it wrote to GITHUB_OUTPUT.
#[cfg(unix)]
fn full_check(event: &str, sha: &str, last: &str) -> String {
    let wf = workflow("full.yml");
    let all = steps(job(&wf, "check"));
    let st = all
        .iter()
        .find(|st| s(st, "id") == "check")
        .expect("check has a step with id: check");
    let dir = Scratch::new("full-check");
    dir.script(
        "bin/gh",
        &format!(
            "case \"$1 $2\" in \"api repos/o/r/actions/workflows/full.yml/runs?branch=main&status=success&per_page=1\") echo '{last}' ;; *) exit 9 ;; esac\n"
        ),
    );
    let out_file = dir.join("output.txt");
    let path = format!("{}:/usr/bin:/bin", dir.join("bin").display());
    let out = bash(
        s(st, "run"),
        dir.path(),
        &[
            ("PATH", path.as_str()),
            ("EVENT", event),
            ("REPO", "o/r"),
            ("SHA", sha),
            ("GITHUB_OUTPUT", out_file.to_str().expect("a UTF-8 path")),
        ],
    );
    assert!(out.status.success(), "{}", said(&out));
    std::fs::read_to_string(out_file).unwrap_or_default()
}

#[cfg(unix)]
#[test]
fn the_full_build_skips_itself_only_when_main_is_the_last_green_commit() {
    assert_eq!(full_check("schedule", "abc", "abc"), "run=false\n");
    assert_eq!(full_check("schedule", "abc", "def"), "run=true\n");
    // No green run yet.
    assert_eq!(full_check("schedule", "abc", ""), "run=true\n");
    // A manual run always runs, even on the same commit: the pre-tag check.
    assert_eq!(full_check("workflow_dispatch", "abc", "abc"), "run=true\n");
}

#[test]
fn every_job_in_every_workflow_has_a_time_limit() {
    // A stuck job holds one of the suite's shared runners until GitHub's
    // six-hour default. Each limit is about twice the job's normal time
    // (Windows tests run about 37 minutes, so they get 75).
    for f in workflow_files() {
        let wf = workflow(&f);
        for (name, job) in jobs(&wf) {
            assert!(
                job.get("timeout-minutes").is_some(),
                "{f}: job {name} has no timeout-minutes"
            );
        }
    }
}

#[test]
fn the_whole_lock_is_checked_for_crate_age_on_main_and_weekly() {
    // A pull request checks only what it adds or bumps, so a crate that
    // slipped in young would otherwise be trusted forever: main checks the
    // whole lock on every push, and weekly-toolchain.yml every Monday.
    let run_of = |file: &str, job: &str| -> String {
        let wf = workflow(file);
        wf["jobs"][job]["steps"]
            .as_sequence()
            .unwrap()
            .iter()
            .filter_map(|s| s["run"].as_str())
            .find(|r| r.contains("check-crate-ages.py"))
            .unwrap_or_else(|| panic!("{file}: {job} runs no crate-age check"))
            .to_owned()
    };
    let rust = run_of("checks.yml", "cooldown");
    let main_branch = rust
        .split("elif")
        .find(|b| b.contains("\"$GITHUB_REF\" = refs/heads/main"))
        .expect("a branch for a push to main");
    assert!(main_branch.contains("base=\"\""), "{main_branch}");
    let weekly = run_of("weekly-toolchain.yml", "crate-ages");
    assert!(weekly.contains("--lock rust/Cargo.lock"));
    assert!(!weekly.contains("--only-changed"));
}
