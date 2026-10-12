//! What survives of tests/sidecar-targets.test.ts and
//! tests/build-sidecar.test.ts. The engine sidecar is retired (the Rust app
//! links the engine), so its file names, musl swap, rustc host lookup and
//! output folder retire with it, and so did `cargo xtask build-sidecar`,
//! which answered "retired" until the cutover removed it; the Rust triples
//! it held now name the CLI's release targets, and those are pinned here.

use xtask::cli_build::binfmt::BinaryFormat as F;
use xtask::cli_build::targets::{MAC_TARGETS, TARGETS, Target};

fn all() -> impl Iterator<Item = &'static Target> {
    TARGETS.iter().chain(MAC_TARGETS)
}

#[test]
fn every_release_target_maps_to_exactly_one_rust_triple() {
    // Pinned whole: a wrong triple builds cleanly for the wrong machine.
    let map: Vec<(&str, &str)> = all().map(|t| (t.id, t.rust_triple)).collect();
    assert_eq!(
        map,
        [
            ("linux-x64", "x86_64-unknown-linux-gnu"),
            ("linux-arm64", "aarch64-unknown-linux-gnu"),
            ("windows-x64", "x86_64-pc-windows-msvc"),
            // Apple silicon only: Intel Macs are not supported after 0.9.0.
            ("macos-arm64", "aarch64-apple-darwin"),
        ]
    );
}

#[test]
fn no_two_targets_share_a_triple_and_no_triple_is_empty() {
    let mut triples: Vec<&str> = all().map(|t| t.rust_triple).collect();
    assert!(triples.iter().all(|t| !t.is_empty()));
    triples.sort();
    triples.dedup();
    assert_eq!(triples.len(), TARGETS.len() + MAC_TARGETS.len());
}

#[test]
fn each_target_names_the_binary_format_its_output_must_have() {
    let map: Vec<(&str, F)> = all().map(|t| (t.rust_triple, t.format)).collect();
    assert_eq!(
        map,
        [
            ("x86_64-unknown-linux-gnu", F::ElfX8664),
            ("aarch64-unknown-linux-gnu", F::ElfAarch64),
            ("x86_64-pc-windows-msvc", F::PeX8664),
            ("aarch64-apple-darwin", F::MachoArm64),
        ]
    );
}

#[test]
fn only_the_windows_target_carries_exe() {
    for t in all() {
        let windows = t.rust_triple.contains("windows");
        assert_eq!(t.binary_name.ends_with(".exe"), windows, "{}", t.id);
    }
}

#[test]
fn every_triple_agrees_with_the_format_it_is_checked_against() {
    // Was "the two target tables agree": with one table the drift between
    // two copies cannot happen, but a triple and its format can still
    // disagree within a row, and then every build of it fails verification.
    for t in all() {
        let (os, arch) = match t.format {
            F::ElfX8664 => ("linux", "x86_64"),
            F::ElfAarch64 => ("linux", "aarch64"),
            F::PeX8664 => ("windows", "x86_64"),
            F::MachoArm64 => ("apple-darwin", "aarch64"),
            other => panic!("{} names {other}, which no release builds", t.id),
        };
        assert!(t.rust_triple.starts_with(arch), "{}", t.id);
        assert!(t.rust_triple.contains(os), "{}", t.id);
    }
}
