//! Ports of tests/export-formats.test.ts, tests/export-freshness.test.ts
//! and tests/export-kfx-setup.test.ts.

mod common;

use std::time::{Duration, SystemTime};

use common::Scratch;
use sp_engine::export::formats::{ExportFormat, ToolchainState, file_extension, format_label};
use sp_engine::export::freshness::needs_regeneration;
use sp_engine::export::kfx::KfxStatus;
use sp_engine::export::kfx_setup::{KFX_LINKS, KfxFix, KfxSetup, KfxStep, kfx_possible, kfx_setup};

fn st(calibre_available: bool, kfx_ready: bool) -> ToolchainState {
    ToolchainState {
        calibre_available,
        kfx_ready,
    }
}

// ---- export-formats ----

#[test]
fn epub_is_always_epub() {
    assert_eq!(file_extension(ExportFormat::Epub, st(false, false)), "epub");
    assert_eq!(file_extension(ExportFormat::Epub, st(true, true)), "epub");
}

#[test]
fn the_ladder_is_kfx_when_ready_and_our_azw3_otherwise() {
    assert_eq!(file_extension(ExportFormat::Kindle, st(true, true)), "kfx");
    assert_eq!(
        file_extension(ExportFormat::Kindle, st(true, false)),
        "azw3"
    );
    assert_eq!(
        file_extension(ExportFormat::Kindle, st(false, false)),
        "azw3"
    );
}

#[test]
fn the_kindle_label_names_its_format_and_says_azw3_is_beta() {
    for calibre in [true, false] {
        let label = format_label(ExportFormat::Kindle, st(calibre, false));
        assert!(label.contains("AZW3"));
        assert!(label.contains("(beta)"));
    }
    assert!(!format_label(ExportFormat::Kindle, st(true, true)).contains("beta"));
}

#[test]
fn only_the_kfx_rung_claims_best_quality() {
    assert!(format_label(ExportFormat::Kindle, st(true, true)).contains("best quality"));
    assert!(!format_label(ExportFormat::Kindle, st(true, false)).contains("best quality"));
}

#[test]
fn kfx_wins_even_without_calibre_detected_separately() {
    assert_eq!(file_extension(ExportFormat::Kindle, st(false, true)), "kfx");
    assert!(format_label(ExportFormat::Kindle, st(false, true)).contains("KFX"));
}

#[test]
fn a_named_file_type_is_its_own_extension_and_label() {
    let names = [
        (ExportFormat::Kfx, "kfx", "KFX"),
        (ExportFormat::Azw3, "azw3", "AZW3 (beta)"),
        (ExportFormat::Mobi, "mobi", "MOBI"),
        (ExportFormat::Fountain, "fountain", "Fountain"),
    ];
    for (format, ext, name) in names {
        for state in [st(false, false), st(true, true)] {
            assert_eq!(file_extension(format, state), ext);
            assert!(format_label(format, state).starts_with(&format!("{name}:")));
        }
    }
}

// ---- export-freshness ----

struct Pair {
    _s: Scratch,
    epub: String,
    artifact: String,
}

fn set_age(path: &str, age_s: u64) {
    let f = std::fs::File::options().write(true).open(path).unwrap();
    f.set_modified(SystemTime::now() - Duration::from_secs(age_s))
        .unwrap();
}

fn pair(artifact_age: Option<u64>, epub_age: u64) -> Pair {
    let s = Scratch::new("export-freshness");
    let epub = s.join("book.epub");
    let artifact = s.join("book.mobi");
    std::fs::write(&epub, "epub").unwrap();
    set_age(&epub, epub_age);
    if let Some(age) = artifact_age {
        std::fs::write(&artifact, "mobi").unwrap();
        set_age(&artifact, age);
    }
    Pair {
        _s: s,
        epub,
        artifact,
    }
}

#[test]
fn a_missing_artifact_needs_regeneration() {
    let p = pair(None, 10);
    assert!(needs_regeneration(&p.artifact, &p.epub));
}

#[test]
fn an_older_artifact_needs_regeneration() {
    let p = pair(Some(100), 10);
    assert!(needs_regeneration(&p.artifact, &p.epub));
}

#[test]
fn a_newer_artifact_is_fresh() {
    let p = pair(Some(10), 100);
    assert!(!needs_regeneration(&p.artifact, &p.epub));
}

#[test]
fn identical_mtimes_count_as_stale() {
    let p = pair(Some(50), 50);
    let when = std::fs::metadata(&p.epub).unwrap().modified().unwrap();
    std::fs::File::options()
        .write(true)
        .open(&p.artifact)
        .unwrap()
        .set_modified(when)
        .unwrap();
    assert!(needs_regeneration(&p.artifact, &p.epub));
}

#[test]
fn an_unreadable_epub_makes_a_present_artifact_stale() {
    let p = pair(Some(10), 100);
    assert!(needs_regeneration(
        &p.artifact,
        &p._s.join("does-not-exist.epub")
    ));
}

// ---- export-kfx-setup ----

fn status(calibre: bool, previewer: bool, plugin_installed: bool) -> KfxStatus {
    KfxStatus {
        calibre,
        previewer,
        plugin_installed,
        ready: calibre && previewer && plugin_installed,
    }
}

fn all() -> Vec<KfxStatus> {
    let mut out = Vec::new();
    for c in [false, true] {
        for p in [false, true] {
            for g in [false, true] {
                out.push(status(c, p, g));
            }
        }
    }
    out
}

fn step<'a>(setup: &'a KfxSetup, id: &str) -> &'a KfxStep {
    setup
        .steps
        .iter()
        .find(|s| serde_json::to_value(s.id).unwrap() == id)
        .unwrap()
}

fn link(label: &str, url: &str) -> Option<KfxFix> {
    Some(KfxFix::Link {
        label: label.into(),
        url: url.into(),
    })
}

#[test]
fn always_three_steps_in_order() {
    for platform in ["darwin", "win32", "linux"] {
        for s in all() {
            let setup = kfx_setup(&s, platform);
            let ids: Vec<String> = setup
                .steps
                .iter()
                .map(|x| {
                    serde_json::to_value(x.id)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned()
                })
                .collect();
            assert_eq!(ids, ["calibre", "previewer", "plugin"]);
            let names: Vec<&str> = setup.steps.iter().map(|x| x.name.as_str()).collect();
            assert_eq!(names, ["Calibre", "Kindle Previewer", "KFX plugin"]);
        }
    }
}

#[test]
fn a_step_has_a_fix_exactly_when_not_installed() {
    for platform in ["darwin", "win32", "linux"] {
        for s in all() {
            for x in kfx_setup(&s, platform).steps {
                assert_eq!(x.fix.is_some(), !x.installed, "{platform} {:?}", x.id);
            }
        }
    }
}

#[test]
fn ready_passes_through_and_a_ready_machine_says_kfx() {
    let setup = kfx_setup(&status(true, true, true), "darwin");
    assert!(setup.ready);
    assert_eq!(
        setup.summary,
        "Kindles get KFX, the best quality Method Reader can make."
    );
    assert!(setup.steps.iter().all(|s| s.installed));
}

#[test]
fn ready_is_exactly_status_ready() {
    for platform in ["darwin", "win32", "linux"] {
        for s in all() {
            assert_eq!(kfx_setup(&s, platform).ready, s.ready);
        }
    }
}

#[test]
fn missing_calibre_links_to_this_platforms_page() {
    assert_eq!(
        step(&kfx_setup(&status(false, true, false), "darwin"), "calibre").fix,
        link("Get Calibre", "https://calibre-ebook.com/download_osx")
    );
    assert_eq!(
        step(&kfx_setup(&status(false, true, false), "win32"), "calibre").fix,
        link("Get Calibre", "https://calibre-ebook.com/download_windows")
    );
    assert_eq!(
        step(&kfx_setup(&status(false, false, false), "linux"), "calibre").fix,
        link("Get Calibre", "https://calibre-ebook.com/download")
    );
}

#[test]
fn missing_previewer_links_where_amazon_makes_it() {
    for platform in ["darwin", "win32"] {
        assert_eq!(
            step(
                &kfx_setup(&status(true, false, true), platform),
                "previewer"
            )
            .fix,
            link(
                "Get Kindle Previewer",
                "https://kdp.amazon.com/en_US/help/topic/G202131170"
            )
        );
    }
    assert_eq!(
        step(&kfx_setup(&status(true, false, true), "linux"), "previewer").fix,
        Some(KfxFix::Unavailable {
            why: "Amazon does not make it for Linux".into()
        })
    );
    assert_eq!(
        step(
            &kfx_setup(&status(true, false, true), "freebsd"),
            "previewer"
        )
        .fix,
        Some(KfxFix::Unavailable {
            why: "Amazon does not make it for this system".into()
        })
    );
}

#[test]
fn the_plugin_installs_with_calibre_and_waits_without() {
    assert_eq!(
        step(&kfx_setup(&status(true, true, false), "darwin"), "plugin").fix,
        Some(KfxFix::Install {
            label: "Install".into()
        })
    );
    assert_eq!(
        step(&kfx_setup(&status(false, true, false), "darwin"), "plugin").fix,
        Some(KfxFix::After {
            why: "Install Calibre first".into()
        })
    );
}

#[test]
fn the_plugin_is_unavailable_where_kfx_cannot_happen() {
    for s in [status(true, false, false), status(false, false, false)] {
        assert_eq!(
            step(&kfx_setup(&s, "linux"), "plugin").fix,
            Some(KfxFix::Unavailable {
                why: "Of no use without Kindle Previewer".into()
            })
        );
    }
}

#[test]
fn plugin_installed_means_nothing_without_calibre() {
    let s = KfxStatus {
        calibre: false,
        previewer: true,
        plugin_installed: true,
        ready: false,
    };
    assert!(!step(&kfx_setup(&s, "darwin"), "plugin").installed);
}

#[test]
fn the_summary_names_the_rung_a_kindle_gets_today() {
    let today = "Kindles get AZW3 (beta) for now, which Method Reader makes itself. KFX looks better, and needs the three free tools below.";
    assert_eq!(
        kfx_setup(&status(true, false, false), "darwin").summary,
        today
    );
    assert_eq!(
        kfx_setup(&status(false, false, false), "win32").summary,
        today
    );
    let linux = "Amazon does not make Kindle Previewer for Linux, so Kindles get AZW3 (beta), which Method Reader makes itself.";
    assert_eq!(
        kfx_setup(&status(true, false, false), "linux").summary,
        linux
    );
    assert_eq!(
        kfx_setup(&status(false, false, false), "linux").summary,
        linux
    );
}

#[test]
fn possible_is_exactly_darwin_and_win32() {
    assert!(kfx_possible("darwin"));
    assert!(kfx_possible("win32"));
    for p in ["linux", "freebsd", "openbsd", "sunos", "aix", ""] {
        assert!(!kfx_possible(p), "{p}");
    }
    assert!(kfx_setup(&status(true, true, true), "darwin").possible);
    assert!(!kfx_setup(&status(true, false, true), "linux").possible);
}

#[test]
fn every_link_drawn_where_kfx_is_possible_is_listed_and_used() {
    let mut emitted = std::collections::BTreeSet::new();
    for platform in ["darwin", "win32"] {
        for s in all() {
            for x in kfx_setup(&s, platform).steps {
                if let Some(KfxFix::Link { url, .. }) = x.fix {
                    emitted.insert(url);
                }
            }
        }
    }
    let listed: std::collections::BTreeSet<String> =
        KFX_LINKS.iter().map(|s| (*s).to_owned()).collect();
    assert_eq!(emitted, listed);
}

#[test]
fn no_string_a_person_reads_carries_an_em_dash() {
    for platform in ["darwin", "win32", "linux", "freebsd"] {
        for s in all() {
            let text = serde_json::to_string(&kfx_setup(&s, platform)).unwrap();
            assert!(!text.contains('\u{2014}'), "{platform}");
        }
    }
}
