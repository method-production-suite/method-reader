//! The Linux build's system headers. Two crates in the window's tree find a
//! system library through pkg-config when they build on Linux: alsa-sys
//! (cpal's ALSA, libasound2-dev) and yeslogic-fontconfig-sys (fontique's
//! fontconfig, libfontconfig1-dev). release.yml once built the window
//! without libasound2-dev, and fontconfig's headers only arrived as a side
//! effect of GTK's. Every apt step that installs headers for a Linux build
//! names both, and a new `-sys` crate in Cargo.lock has to be classified
//! here before it can slip in unnoticed.

mod repo_common;

use repo_common::read;

/// Every `-sys` crate in rust/Cargo.lock, and what a Linux build of the
/// workspace needs installed for it.
const SYS_CRATES: &[(&str, Option<&str>)] = &[
    // pkg-config at build time on Linux:
    ("alsa-sys", Some("libasound2-dev")),
    ("yeslogic-fontconfig-sys", Some("libfontconfig1-dev")),
    // Built with `dlopen` (wgpu-hal and smithay ask for it): links nothing,
    // probes nothing; the library is opened at run time (sp-release's
    // LOADED_AT_RUN_TIME declares it in the packages).
    ("wayland-sys", None),
    // GTK's, only through muda, which sp-app uses on macOS alone.
    ("atk-sys", None),
    ("gdk-pixbuf-sys", None),
    ("gdk-sys", None),
    ("gio-sys", None),
    ("glib-sys", None),
    ("gobject-sys", None),
    ("gtk-sys", None),
    ("pango-sys", None),
    // Other systems', or no system library at all.
    ("core-foundation-sys", None),
    ("fsevent-sys", None),
    ("inotify-sys", None),
    ("io-kit-sys", None),
    ("jni-sys", None),
    ("js-sys", None),
    ("kqueue-sys", None),
    ("linux-raw-sys", None),
    ("ndk-sys", None),
    ("objc-sys", None),
    ("renderdoc-sys", None),
    ("web-sys", None),
    ("windows-sys", None),
    ("worker-sys", None),
];

fn lock_sys_crates() -> Vec<String> {
    let lock = read("rust/Cargo.lock");
    let mut names: Vec<String> = lock
        .lines()
        .filter_map(|l| l.strip_prefix("name = \""))
        .map(|n| n.trim_end_matches('"').to_string())
        .filter(|n| n.ends_with("-sys"))
        .collect();
    names.sort();
    names.dedup();
    names
}

#[test]
fn every_sys_crate_in_the_lock_file_is_classified() {
    let known: Vec<&str> = SYS_CRATES.iter().map(|(n, _)| *n).collect();
    let unknown: Vec<String> = lock_sys_crates()
        .into_iter()
        .filter(|n| !known.contains(&n.as_str()))
        .collect();
    assert!(
        unknown.is_empty(),
        "new -sys crates: does a Linux build need a system package for them? {unknown:?}"
    );
}

/// Every `apt-get install` line that installs headers.
fn header_installs() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (file, text) in [
        (
            ".github/workflows/full.yml",
            read(".github/workflows/full.yml"),
        ),
        (
            ".github/workflows/release.yml",
            read(".github/workflows/release.yml"),
        ),
        ("CONTRIBUTING.md", read("CONTRIBUTING.md")),
    ] {
        for line in text.lines() {
            if line.contains("apt-get install") && line.contains("-dev") {
                out.push((file.to_string(), line.trim().to_string()));
            }
        }
    }
    out
}

#[test]
fn every_linux_header_install_names_what_pkg_config_looks_for() {
    let lines = header_installs();
    // full.yml's test and bundle jobs, release.yml's build, CONTRIBUTING.
    assert!(lines.len() >= 4, "{lines:?}");
    for (file, line) in &lines {
        for (_, pkg) in SYS_CRATES {
            if let Some(pkg) = pkg {
                assert!(
                    line.split_whitespace().any(|w| w == *pkg),
                    "{file}: {line} lacks {pkg}"
                );
            }
        }
    }
}

#[test]
fn contributing_lists_what_ci_installs() {
    let contributing = read("CONTRIBUTING.md");
    let line = contributing
        .lines()
        .find(|l| l.starts_with("sudo apt-get install"))
        .expect("CONTRIBUTING's apt line");
    let full_yml = read(".github/workflows/full.yml");
    assert!(
        full_yml.lines().any(|l| l.trim() == line.trim()),
        "CONTRIBUTING says {line:?}; full.yml's test job must install the same"
    );
}
