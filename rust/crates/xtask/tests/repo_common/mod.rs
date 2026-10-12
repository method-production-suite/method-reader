//! Shared by the repo-level checks (workflows.rs, release_workflow.rs,
//! release_steps.rs, site.rs, readme_and_site.rs and their neighbours):
//! the repository's files, the workflows read as the YAML GitHub runs, and
//! a scratch folder of stub executables for running a step's own bash.
//! Not every test file uses every helper.
#![allow(dead_code)]

use serde_yaml::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

/// The repository root.
pub fn root() -> PathBuf {
    xtask::paths::repo_root()
}

/// A repository file as text.
pub fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// Runs of whitespace as one space: the pages hard-wrap, and a phrase split
/// across two lines is still the phrase a reader sees.
pub fn flat(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Every workflow file name in .github/workflows, sorted.
pub fn workflow_files() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root().join(".github/workflows"))
        .expect(".github/workflows")
        .map(|e| {
            e.expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|f| f.ends_with(".yml") || f.ends_with(".yaml"))
        .collect();
    names.sort();
    names
}

/// One workflow, parsed.
pub fn workflow(file: &str) -> Value {
    let text = read(&format!(".github/workflows/{file}"));
    serde_yaml::from_str(&text).unwrap_or_else(|e| panic!("{file}: {e}"))
}

/// The workflow's jobs, by name, in file order.
pub fn jobs(wf: &Value) -> Vec<(String, &Value)> {
    wf.get("jobs")
        .and_then(Value::as_mapping)
        .expect("a jobs: table")
        .iter()
        .map(|(k, v)| (k.as_str().expect("a job name").to_string(), v))
        .collect()
}

pub fn job<'a>(wf: &'a Value, name: &str) -> &'a Value {
    wf.get("jobs")
        .and_then(|j| j.get(name))
        .unwrap_or_else(|| panic!("no job named {name}"))
}

pub fn has_job(wf: &Value, name: &str) -> bool {
    wf.get("jobs").and_then(|j| j.get(name)).is_some()
}

pub fn steps(job: &Value) -> Vec<&Value> {
    job.get("steps")
        .and_then(Value::as_sequence)
        .map(|s| s.iter().collect())
        .unwrap_or_default()
}

/// A string field, or "".
pub fn s<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

/// The job's `needs:`, as a list.
pub fn needs(job: &Value) -> Vec<String> {
    match job.get("needs") {
        None => vec![],
        Some(Value::String(one)) => vec![one.clone()],
        Some(Value::Sequence(many)) => many
            .iter()
            .map(|n| n.as_str().expect("a job name").to_string())
            .collect(),
        Some(other) => panic!("needs: {other:?}"),
    }
}

/// Every `run:` block of the job, joined.
pub fn run_text(job: &Value) -> String {
    steps(job)
        .iter()
        .map(|st| s(st, "run"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The lines that execute: comment lines dropped.
pub fn commands(text: &str) -> String {
    text.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The `run:` of the step with this name in this job.
pub fn step_named(wf: &Value, job_name: &str, step: &str) -> String {
    steps(job(wf, job_name))
        .into_iter()
        .find(|st| s(st, "name") == step)
        .map(|st| s(st, "run").to_string())
        .filter(|r| !r.is_empty())
        .unwrap_or_else(|| panic!("{job_name} has no step named {step:?} with a run block"))
}

/// The index of the first step matching `pred`.
pub fn step_index(job: &Value, pred: impl Fn(&Value) -> bool) -> Option<usize> {
    steps(job).into_iter().position(pred)
}

/// The job-level `if:`.
pub fn job_if(job: &Value) -> String {
    s(job, "if").to_string()
}

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A folder under the system temp folder, removed when dropped.
pub struct Scratch(PathBuf);

impl Scratch {
    pub fn new(tag: &str) -> Self {
        let n = NEXT.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "screepub-xtask-repo-{tag}-{}-{n}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch folder");
        Scratch(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn join(&self, p: impl AsRef<Path>) -> PathBuf {
        self.0.join(p)
    }

    /// Write a file, making its folders.
    pub fn write(&self, rel: &str, text: impl AsRef<[u8]>) -> PathBuf {
        let path = self.0.join(rel);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("folders");
        std::fs::write(&path, text).expect("write");
        path
    }

    /// An executable shell script at `rel`.
    #[cfg(unix)]
    pub fn script(&self, rel: &str, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = self.write(rel, format!("#!/bin/sh\n{body}"));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        path
    }

    /// A recorder at `rel` standing in for a real executable: it appends
    /// one tab-joined line per call to `<rel>.argv` and exits 0, so a
    /// step's own command lines are captured rather than run.
    #[cfg(unix)]
    pub fn recorder(&self, rel: &str) -> PathBuf {
        let log = self.join(format!("{rel}.argv"));
        self.script(
            rel,
            &format!(
                "printf '%s\\t' \"$@\" >> '{0}'\nprintf '\\n' >> '{0}'\nexit 0\n",
                log.display()
            ),
        )
    }

    /// The lines a recorder at `rel` wrote, trailing tab gone.
    pub fn calls(&self, rel: &str) -> Vec<String> {
        std::fs::read_to_string(self.join(format!("{rel}.argv")))
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.strip_suffix('\t').unwrap_or(l).to_string())
            .collect()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Run `script` with bash -c in `cwd`, with exactly `env` (PATH included:
/// the caller decides what is reachable) and HOME in `cwd`.
pub fn bash(script: &str, cwd: &Path, env: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new("/bin/bash");
    cmd.arg("-c").arg(script).current_dir(cwd).env_clear();
    cmd.env("HOME", cwd);
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.output().expect("bash runs")
}

/// stdout and stderr together, for messages.
pub fn said(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}
