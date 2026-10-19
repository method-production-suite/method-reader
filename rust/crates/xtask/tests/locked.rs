//! No pull request builds on GitHub any more (SUITE-STANDARDS.md section 6,
//! CI ArtCraft-style), so the full local check on a Mac is where a stale or
//! crossed Cargo.lock has to fail. Every cargo build xtask runs itself, and
//! the `cargo xtask` alias, pass --locked.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).expect("readable") {
        let p = e.expect("an entry").path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn the_xtask_alias_is_locked() {
    let config = std::fs::read_to_string(root().join(".cargo/config.toml")).expect("config.toml");
    let alias = config
        .lines()
        .find(|l| l.trim_start().starts_with("xtask ="))
        .expect("the xtask alias");
    assert!(alias.contains("--locked"), "{alias}");
}

#[test]
fn every_cargo_build_xtask_runs_is_locked() {
    let mut files = Vec::new();
    rust_files(&root().join("crates/xtask/src"), &mut files);
    let mut found = 0;
    let mut wrong = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(&f).expect("readable");
        let mut from = 0;
        // An argument list whose first word is "build": `["build", ...]` or
        // `vec![..., "build".into(), ...]` (cargo first, then build).
        while let Some(i) = text[from..].find("\"build\"") {
            let at = from + i;
            from = at + 1;
            let open = text[..at].rfind('[').expect("inside a list");
            let between = text[open + 1..at].trim();
            let first = between.is_empty() || between.ends_with("cargo.clone(),");
            if !first {
                continue;
            }
            let close = at + text[at..].find(']').expect("a closed list");
            found += 1;
            let list = &text[open..close];
            let before: String = text[..open].lines().rev().take(4).collect();
            if !list.contains("\"--locked\"") && !before.contains("not --locked:") {
                let line = text[..at].lines().count();
                wrong.push(format!("{}:{line}", f.display()));
            }
        }
    }
    assert!(
        found >= 6,
        "found only {found} cargo builds: the scan is broken"
    );
    assert_eq!(wrong, Vec::<String>::new());
}
