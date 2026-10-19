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
        "rust.yml",
        "weekly-toolchain.yml",
    ] {
        assert!(names.contains(&f.to_string()), "{f} is missing: {names:?}");
    }
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
    // Dependabot keep honest, and what a reader checks the SHA against.
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
/// nightly.yml's check job: it reads the workflow's own past runs, which
/// needs actions: read and nothing else.
fn reads_its_runs(file: &str, name: &str, job: &Value) -> bool {
    file == "nightly.yml"
        && name == "check"
        && commands(&run_text(job)).contains("actions/workflows/nightly.yml/runs")
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
fn weekly_pins_the_same_actions_rust_yml_does() {
    let pins = |f: &str| -> BTreeSet<String> {
        let wf = workflow(f);
        jobs(&wf)
            .into_iter()
            .flat_map(|(_, j)| steps(j))
            .map(|st| s(st, "uses").to_string())
            .filter(|u| !u.is_empty())
            .collect()
    };
    let rust = pins("rust.yml");
    for u in pins("weekly-toolchain.yml") {
        assert!(
            u.contains('@') && rust.contains(&u),
            "{u} differs from rust.yml"
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

#[test]
fn ci_runs_once_per_change_and_a_newer_push_replaces_an_older_run() {
    // The suite's runners are shared (SUITE-STANDARDS.md section 6): a
    // branch with a pull request must not run twice, once for the push and
    // once for the pull request, and a new push to a pull request cancels
    // the run it replaces. main is never cancelled, so every commit on it
    // keeps a full result. release.yml runs its own checks on the tag and
    // never cancels; it is not this workflow.
    let wf = workflow("rust.yml");
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
    assert!(on.get("pull_request").is_some());
    assert!(on.get("workflow_dispatch").is_some());
    let c = &wf["concurrency"];
    assert_eq!(
        c["group"].as_str(),
        Some("ci-${{ github.workflow }}-${{ github.ref }}")
    );
    assert_eq!(
        c["cancel-in-progress"].as_str(),
        Some("${{ github.ref != 'refs/heads/main' }}")
    );
    let release = workflow("release.yml");
    assert_ne!(
        release["concurrency"]["cancel-in-progress"].as_bool(),
        Some(true),
        "a release run is never cancelled"
    );
}

#[test]
fn every_ci_and_release_job_has_a_time_limit() {
    // A stuck job holds one of the suite's shared runners until GitHub's
    // six-hour default. Each limit is about twice the job's normal time
    // (Windows tests run about 37 minutes, so they get 75).
    for f in ["rust.yml", "release.yml"] {
        let wf = workflow(f);
        for (name, job) in jobs(&wf) {
            assert!(
                job.get("timeout-minutes").is_some(),
                "{f}: job {name} has no timeout-minutes"
            );
        }
    }
}

#[test]
fn only_main_saves_a_cache() {
    // GitHub keeps 10 GB of caches per repository and evicts the rest; with
    // every pull request saving its own copy, main's caches never survived
    // and most runs rebuilt from nothing (SUITE-STANDARDS section 6). So no
    // step uses the save-on-exit actions/cache, and every actions/cache/save
    // runs only on main. release.yml is exempt: it runs on tags, untouched.
    for f in workflow_files() {
        if f == "release.yml" {
            continue;
        }
        let wf = workflow(&f);
        for (name, job) in jobs(&wf) {
            for st in steps(job) {
                let uses = s(st, "uses");
                assert!(
                    !uses.starts_with("actions/cache@"),
                    "{f}: {name} uses actions/cache, which saves on every ref"
                );
                if uses.starts_with("actions/cache/save@") {
                    assert!(
                        s(st, "if").contains("github.ref == 'refs/heads/main'"),
                        "{f}: {name} saves a cache off main"
                    );
                }
            }
        }
    }
}

/// The bundles job's steps, and the index of its decide step.
fn bundles_decide() -> (Vec<Value>, usize) {
    let wf = workflow("rust.yml");
    let all: Vec<Value> = steps(job(&wf, "bundles")).into_iter().cloned().collect();
    let i = all
        .iter()
        .position(|st| s(st, "id") == "decide")
        .expect("bundles has a step with id: decide");
    (all, i)
}

const PACKAGING_GUARD: &str = "steps.decide.outputs.packaging == 'true'";

#[test]
fn bundles_decides_first_and_guards_every_later_step() {
    // The job is skipped only as every job is (a draft, or documentation
    // alone); the packaging answer is a step, and only the expensive steps
    // wait on it. Checkout comes first because the answer is a git diff,
    // and it must reach main (the merge commit's first parent).
    let wf = workflow("rust.yml");
    assert_eq!(job_if(job(&wf, "bundles")), "");
    let (all, i) = bundles_decide();
    assert_eq!(i, 1, "the decide step runs right after checkout");
    assert!(s(&all[0], "uses").starts_with("actions/checkout@"));
    assert_eq!(all[0]["with"]["fetch-depth"].as_u64(), Some(2));
    assert!(s(&all[0], "if").is_empty() && s(&all[i], "if").is_empty());
    let unguarded: Vec<String> = all[i + 1..]
        .iter()
        .filter(|st| !s(st, "if").contains(PACKAGING_GUARD))
        .map(|st| format!("{}{}", s(st, "name"), s(st, "uses")))
        .collect();
    assert_eq!(unguarded, Vec::<String>::new());
    assert!(all.len() > i + 2, "the guarded steps are still there");
}

/// Run the decide step's own bash for `event`, with a `git` stub on PATH
/// that answers `diff` with `changed`, and return what it wrote to
/// GITHUB_OUTPUT.
#[cfg(unix)]
fn decide(event: &str, changed: &[&str]) -> String {
    let (all, i) = bundles_decide();
    run_decision(s(&all[i], "run"), event, changed)
}

/// Run a decide step's bash for `event`, with a `git` stub on PATH that
/// answers `diff` with `changed`; what it wrote to GITHUB_OUTPUT.
#[cfg(unix)]
fn run_decision(script: &str, event: &str, changed: &[&str]) -> String {
    let dir = Scratch::new("decide");
    let list = dir.write("changed.txt", changed.join("\n") + "\n");
    dir.script(
        "bin/git",
        &format!(
            "case \"$1\" in rev-parse) exit 0 ;; diff) cat '{}' ;; *) exit 9 ;; esac\n",
            list.display()
        ),
    );
    let out_file = dir.join("output.txt");
    let path = format!("{}:/usr/bin:/bin", dir.join("bin").display());
    let out = bash(
        script,
        dir.path(),
        &[
            ("PATH", path.as_str()),
            ("EVENT", event),
            ("GITHUB_OUTPUT", out_file.to_str().expect("a UTF-8 path")),
        ],
    );
    assert!(out.status.success(), "{}", said(&out));
    std::fs::read_to_string(out_file).unwrap_or_default()
}

#[cfg(unix)]
#[test]
fn bundles_always_builds_on_main_and_on_a_manual_run() {
    for event in ["push", "workflow_dispatch"] {
        assert_eq!(decide(event, &["README.md"]), "packaging=true\n", "{event}");
    }
}

#[cfg(unix)]
#[test]
fn bundles_builds_a_pull_request_only_when_packaging_changed() {
    for f in [
        "rust/crates/sp-release/src/nsis.rs",
        "rust/crates/sp-release/Cargo.toml",
        "rust/crates/xtask/src/packaging/native.rs",
        "rust/crates/xtask/src/dmg.rs",
        "rust/crates/sp-app/Cargo.toml",
        "rust/crates/sp-app/icons/icon.icns",
        "rust/Cargo.lock",
        ".github/workflows/release.yml",
        ".github/workflows/rust.yml",
    ] {
        assert_eq!(
            decide("pull_request", &["README.md", f]),
            "packaging=true\n",
            "{f}"
        );
    }
    for f in [
        "README.md",
        "rust/crates/sp-parser/src/classify.rs",
        "rust/crates/sp-app/src/app/mod.rs",
        "docs/ROADMAP.md",
        // Near misses: a prefix is not the folder.
        "rust/crates/sp-release-notes/x.rs",
        "rust/crates/xtask/src/packaging.rs",
    ] {
        assert_eq!(decide("pull_request", &[f]), "packaging=false\n", "{f}");
    }
}

const CODE_GUARD: &str = "needs.changes.outputs.code == 'true'";
const DRAFT_GUARD: &str = "github.event_name != 'pull_request' || !github.event.pull_request.draft";

#[test]
fn a_draft_builds_nothing_and_marking_it_ready_starts_the_run() {
    // SUITE-STANDARDS.md section 6: a draft pull request runs no job, and
    // ready_for_review is among the triggers, so marking it ready starts the
    // full run. The changes job carries the draft test and every other job
    // needs it, so a skipped changes job skips them all (and a skipped job
    // still counts as passing for main's required checks).
    let wf = workflow("rust.yml");
    let types: Vec<&str> = wf["on"]["pull_request"]["types"]
        .as_sequence()
        .expect("pull_request lists its types")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(
        types,
        ["opened", "synchronize", "reopened", "ready_for_review"]
    );
    assert_eq!(job_if(job(&wf, "changes")), DRAFT_GUARD);
    for (name, j) in jobs(&wf) {
        if name == "changes" {
            continue;
        }
        assert_eq!(s(j, "needs"), "changes", "{name} needs the changes job");
        let want = match name.as_str() {
            "docs" => "needs.changes.outputs.code == 'false'",
            // The Mac-only job: nightly's alone (the next tests).
            "parity" => "inputs.full && needs.changes.outputs.code == 'true'",
            // Matrix jobs a ruleset has required: see the next test. (cli
            // is required by none, so it is skipped like any other job.)
            "rust" | "bundles" => "",
            _ => CODE_GUARD,
        };
        assert_eq!(job_if(j), want, "{name}");
    }
}

/// The required checks that come from a matrix ("rust (ubuntu-latest)").
const MATRIX_JOBS: [&str; 3] = ["rust", "cli", "bundles"];

#[test]
fn a_matrix_job_is_skipped_step_by_step_so_its_required_checks_report() {
    // A matrix job skipped at the job level reports one check under its bare
    // name, and main's ruleset waits for "rust (ubuntu-latest)" and the
    // rest. So rust is never skipped at the job level for documentation:
    // every step is. (bundles' steps wait on its packaging answer, which
    // documentation never changes; the test above holds that.)
    let wf = workflow("rust.yml");
    for name in MATRIX_JOBS {
        assert!(
            !job(&wf, name)["strategy"]["matrix"].is_null(),
            "{name} is a matrix job"
        );
    }
    // cli is required by no ruleset, so a job-level skip is harmless there.
    let unguarded: Vec<String> = steps(job(&wf, "rust"))
        .into_iter()
        .filter(|st| !s(st, "if").starts_with(CODE_GUARD))
        .map(|st| format!("{}{}{}", s(st, "name"), s(st, "uses"), s(st, "run")))
        .collect();
    assert_eq!(unguarded, Vec::<String>::new());
    for (name, j) in jobs(&wf) {
        if !MATRIX_JOBS.contains(&name.as_str()) {
            assert!(
                j["strategy"]["matrix"].is_null(),
                "{name} has a matrix: skip it step by step, like rust"
            );
        }
    }
}

#[cfg(unix)]
fn changes(event: &str, changed: &[&str]) -> String {
    let wf = workflow("rust.yml");
    let all = steps(job(&wf, "changes"));
    let st = all
        .iter()
        .find(|st| s(st, "id") == "decide")
        .expect("changes has a step with id: decide");
    run_decision(s(st, "run"), event, changed)
}

#[cfg(unix)]
#[test]
fn main_and_a_manual_run_check_everything() {
    for event in ["push", "workflow_dispatch"] {
        assert_eq!(changes(event, &["README.md"]), "code=true\n", "{event}");
    }
}

#[cfg(unix)]
#[test]
fn documentation_alone_skips_the_build_but_not_what_the_build_reads() {
    assert_eq!(
        changes(
            "pull_request",
            &[
                "README.md",
                "docs/device-map.md",
                "rust/PARITY.md",
                "docs/img/x.png"
            ]
        ),
        "code=false\n"
    );
    for f in [
        // Compiled into the window, or carried by every installer.
        "THIRD-PARTY-NOTICES.md",
        "docs/releases/0.13.1.md",
        "LICENSE-MIT",
        // Read by the tests: the window's colours, the fonts and the scene.
        "brand/tokens.css",
        "site/index.html",
        "rust/crates/sp-app/src/app/mod.rs",
        ".github/workflows/rust.yml",
    ] {
        assert_eq!(
            changes("pull_request", &["README.md", f]),
            "code=true\n",
            "{f}"
        );
    }
}

/// The matrix's `os:` expression: `fromJSON(inputs.full && FULL || '["ubuntu-latest"]')`.
fn os_lists(wf: &Value, name: &str) -> (Vec<String>, Vec<String>) {
    let os = s(&job(wf, name)["strategy"]["matrix"], "os").to_string();
    let re =
        Regex::new(r"^\$\{\{ fromJSON\(inputs\.full && '(\[[^']*\])' \|\| '(\[[^']*\])'\) \}\}$")
            .unwrap();
    let c = re
        .captures(&os)
        .unwrap_or_else(|| panic!("{name}: os is not the full-or-Linux expression: {os}"));
    let list = |i: usize| -> Vec<String> {
        serde_json::from_str(&c[i]).unwrap_or_else(|e| panic!("{name}: {e}"))
    };
    (list(1), list(2))
}

#[test]
fn each_change_runs_on_linux_alone_and_the_nightly_on_every_system() {
    // SUITE-STANDARDS.md section 6, Lean CI: a pull request and a push to
    // main run Linux only; nightly.yml calls this workflow with full: true
    // for macOS and Windows too. main's ruleset requires rust
    // (ubuntu-latest), worker and deny, which every change still reports.
    let wf = workflow("rust.yml");
    let every = ["ubuntu-latest", "macos-15", "windows-latest"];
    for name in ["rust", "bundles"] {
        let (full, change) = os_lists(&wf, name);
        assert_eq!(full, every, "{name}");
        assert_eq!(change, ["ubuntu-latest"], "{name}");
    }
    let (full, change) = os_lists(&wf, "cli");
    assert_eq!(full, ["ubuntu-latest", "macos-15"]);
    assert_eq!(change, ["ubuntu-latest"]);
    // Every other job is on Linux, except parity, which only nightly runs.
    for (name, j) in jobs(&wf) {
        let on = s(j, "runs-on");
        if MATRIX_JOBS.contains(&name.as_str()) {
            assert_eq!(on, "${{ matrix.os }}", "{name}");
        } else if name == "parity" {
            assert!(job_if(j).starts_with("inputs.full && "), "{name}");
        } else {
            assert_eq!(on, "ubuntu-latest", "{name}");
        }
    }
    let input = &wf["on"]["workflow_call"]["inputs"]["full"];
    assert_eq!(s(input, "type"), "boolean");
    assert_eq!(input["default"].as_bool(), Some(false));
}

#[test]
fn the_nightly_is_rust_yml_in_full_on_main() {
    let wf = workflow("nightly.yml");
    let on = &wf["on"];
    assert!(on["schedule"].as_sequence().is_some_and(|s| s.len() == 1));
    assert!(on.get("workflow_dispatch").is_some());
    assert!(on.get("push").is_none() && on.get("pull_request").is_none());
    let full = job(&wf, "full");
    assert_eq!(s(full, "uses"), "./.github/workflows/rust.yml");
    assert_eq!(full["with"]["full"].as_bool(), Some(true));
    assert_eq!(s(full, "needs"), "check");
    assert_eq!(job_if(full), "needs.check.outputs.run == 'true'");
    // rust.yml's own group is ci-<this workflow's name>-<ref>: this one must
    // differ, or the call would wait on itself.
    let group = s(&wf["concurrency"], "group");
    assert!(!group.starts_with("ci-"), "{group}");
    assert_ne!(
        wf["concurrency"]["cancel-in-progress"].as_bool(),
        Some(true)
    );
}

/// Run nightly's check step with a `gh` stub that answers `last` as the
/// last green nightly's commit; what it wrote to GITHUB_OUTPUT.
#[cfg(unix)]
fn nightly_check(event: &str, sha: &str, last: &str) -> String {
    let wf = workflow("nightly.yml");
    let all = steps(job(&wf, "check"));
    let st = all
        .iter()
        .find(|st| s(st, "id") == "check")
        .expect("check has a step with id: check");
    let dir = Scratch::new("nightly");
    dir.script(
        "bin/gh",
        &format!(
            "case \"$1 $2\" in \"api repos/o/r/actions/workflows/nightly.yml/runs?branch=main&status=success&per_page=1\") echo '{last}' ;; *) exit 9 ;; esac\n"
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
fn the_nightly_skips_itself_only_when_main_is_the_last_green_commit() {
    assert_eq!(nightly_check("schedule", "abc", "abc"), "run=false\n");
    assert_eq!(nightly_check("schedule", "abc", "def"), "run=true\n");
    // No green nightly yet.
    assert_eq!(nightly_check("schedule", "abc", ""), "run=true\n");
    // A manual run always runs, even on the same commit.
    assert_eq!(
        nightly_check("workflow_dispatch", "abc", "abc"),
        "run=true\n"
    );
}
