//! The tools find cargo's build folder the way cargo does, never by
//! assuming rust/target: the suite builds every checkout into one shared
//! folder (CARGO_TARGET_DIR, the disk rule of 2026-10-21).

mod common;

use std::path::PathBuf;

use common::Scratch;

use xtask::paths::{target_dir, target_dir_of, workspace_root};

#[test]
fn the_workspace_s_target_dir_is_cargo_s_own_answer() {
    let found = target_dir();
    match std::env::var_os("CARGO_TARGET_DIR").filter(|d| !d.is_empty()) {
        Some(d) => {
            let d = PathBuf::from(d);
            let want = if d.is_absolute() {
                d
            } else {
                workspace_root().join(d)
            };
            assert_eq!(found, want);
        }
        None => assert_eq!(found, workspace_root().join("target")),
    }
}

#[test]
fn a_folder_with_no_cargo_toml_falls_back_to_its_own_target() {
    let dir = Scratch::new("target-dir");
    assert_eq!(target_dir_of(dir.path()), dir.join("target"));
}

#[test]
fn a_second_workspace_answers_for_itself() {
    // The handover proof is its own workspace; with CARGO_TARGET_DIR set it
    // builds into the same shared folder, and without it into its own.
    let proof = workspace_root().join("handover-proof");
    let found = target_dir_of(&proof);
    match std::env::var_os("CARGO_TARGET_DIR").filter(|d| !d.is_empty()) {
        Some(d) if PathBuf::from(&d).is_absolute() => assert_eq!(found, PathBuf::from(d)),
        Some(_) => {}
        None => assert_eq!(found, proof.join("target")),
    }
}
