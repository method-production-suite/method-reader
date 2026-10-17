//! The licence split: every crate is MIT OR Apache-2.0 except sp-azw3, the
//! calibre port, which is GPL-3.0-only. The crates other projects take on
//! their own (the PDF reader, the parser, Fountain and Final Draft) must
//! never come to depend on it, or they would carry the GPL with them.

mod repo_common;

use repo_common::*;
use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

/// The crates that turn a PDF into an editable script.
const IMPORT_CRATES: [&str; 5] = ["sp-model", "sp-pdf", "sp-parser", "sp-fountain", "sp-fdx"];

/// The whole graph, every platform's packages included: not `--offline`,
/// because a fresh CI runner has fetched only its own platform's crates and
/// `cargo metadata` needs them all. `--locked` keeps it to Cargo.lock.
fn metadata() -> serde_json::Value {
    let out = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--locked"])
        .current_dir(root().join("rust"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn every_crate_but_the_calibre_port_is_mit_or_apache() {
    let m = metadata();
    let members: BTreeSet<&str> = m["workspace_members"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let mut seen = 0;
    for p in m["packages"].as_array().unwrap() {
        if !members.contains(p["id"].as_str().unwrap()) {
            continue;
        }
        seen += 1;
        let name = p["name"].as_str().unwrap();
        let want = if name == "sp-azw3" {
            "GPL-3.0-only"
        } else {
            "MIT OR Apache-2.0"
        };
        assert_eq!(p["license"].as_str(), Some(want), "{name}");
    }
    assert!(seen > 15, "only {seen} workspace crates read");
}

#[test]
fn the_import_crates_never_depend_on_the_gpl_crate() {
    let m = metadata();
    let names: BTreeMap<&str, &str> = m["packages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["id"].as_str().unwrap(), p["name"].as_str().unwrap()))
        .collect();
    let deps: BTreeMap<&str, Vec<&str>> = m["resolve"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| {
            let id = n["id"].as_str().unwrap();
            let ds = n["dependencies"]
                .as_array()
                .unwrap()
                .iter()
                .map(|d| d.as_str().unwrap())
                .collect();
            (id, ds)
        })
        .collect();
    for krate in IMPORT_CRATES {
        let root = names
            .iter()
            .find(|(_, n)| **n == krate)
            .map(|(id, _)| *id)
            .unwrap_or_else(|| panic!("{krate} is not in the workspace"));
        let mut stack = vec![root];
        let mut seen = BTreeSet::new();
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            assert_ne!(names[id], "sp-azw3", "{krate} reaches sp-azw3");
            stack.extend(deps[id].iter().copied());
        }
    }
}

#[test]
fn the_three_licence_files_are_where_the_bundles_take_them() {
    assert!(read("LICENSE-MIT").starts_with("MIT License"));
    assert!(read("LICENSE-APACHE").contains("Apache License"));
    assert!(read("rust/crates/sp-azw3/LICENSE-GPL-3.0").contains("Version 3, 29 June 2007"));
    assert!(!root().join("LICENSE").exists(), "the AGPL file is gone");
}
