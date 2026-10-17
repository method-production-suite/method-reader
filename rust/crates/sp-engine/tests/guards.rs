//! The Rust form of tests/calibre-config-isolation.test.ts and of the
//! preload checks in tests/app-settings.test.ts: no test reaches the real
//! settings file or the real Calibre settings folder.
//!
//! The TypeScript's guard was an environment mutated in a preload plus root
//! `.env.test`. Here the engine takes its environment as a value, so the
//! guard is what tests hand it (common::guarded_env, common::calibre_env)
//! and a check that no test reaches for this process's own environment.
//!
//! Not ported: "a bare Calibre spawn on the guard path fails loudly". It
//! starts calibre-customize against a folder that is NOT a scratch copy,
//! which this port's rules forbid; the guard value itself is checked below.

mod common;

use std::path::Path;

use common::{
    CALIBRE_GUARD, SETTINGS_GUARD, calibre_config_copy, calibre_env, real_calibre_config,
};

/// A guard is a path under /dev/null: nothing can be created there, so a
/// settings write or a Calibre start that reaches it fails loudly instead of
/// touching a real folder. (The TypeScript's guards lived in root .env.test
/// and tools/calibre-config.ts, with these same values.)
fn assert_uncreatable(guard: &str) {
    assert!(guard.starts_with("/dev/null/"), "{guard}");
    #[cfg(unix)]
    assert!(
        std::fs::create_dir_all(guard).is_err(),
        "{guard} could be made"
    );
}

#[test]
fn the_settings_guard_is_a_path_nothing_can_create() {
    assert_eq!(SETTINGS_GUARD, "/dev/null/method-reader-test-guard");
    assert_uncreatable(SETTINGS_GUARD);
}

#[test]
fn the_calibre_guard_is_a_path_nothing_can_create() {
    assert_eq!(CALIBRE_GUARD, "/dev/null/method-reader-test-calibre-guard");
    assert_uncreatable(CALIBRE_GUARD);
}

fn canonical(p: &Path) -> std::path::PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

#[test]
fn calibre_gets_a_scratch_copy_in_the_temp_folder_not_the_real_one() {
    let env = calibre_env();
    let live = env.get("CALIBRE_CONFIG_DIRECTORY").unwrap();
    assert_ne!(live, CALIBRE_GUARD);
    assert!(Path::new(live).exists());
    let live = canonical(Path::new(live));
    assert!(live.starts_with(canonical(&std::env::temp_dir())));
    let real = canonical(&real_calibre_config());
    assert!(!live.starts_with(&real));
    assert!(!real.starts_with(&live));
}

#[test]
fn the_copy_holds_everything_the_real_folder_holds_caches_aside() {
    let real = real_calibre_config();
    if !real.exists() {
        return;
    }
    let copy = calibre_config_copy();
    let in_copy = common::names(&copy.to_string_lossy());
    for name in common::names(&real.to_string_lossy()) {
        if name != "caches" {
            assert!(in_copy.contains(&name), "{name}");
        }
    }
    assert!(!copy.join("caches").exists());
    let plugins = real.join("plugins");
    if plugins.exists() {
        let copied = common::names(&copy.join("plugins").to_string_lossy());
        for name in common::names(&plugins.to_string_lossy()) {
            assert!(copied.contains(&name), "{name}");
        }
    }
}

/// CALIBRE SPAWN: `calibre-customize --list-plugins`, with the scratch copy.
/// It also proves the probe's spawn reaches the copy: on the guard it would
/// fail and read as "not installed".
#[test]
fn a_kfx_plugin_in_the_real_folder_is_found_in_the_copy() {
    let env = calibre_env();
    let plugins = real_calibre_config().join("plugins");
    if sp_engine::export::calibre::calibre_tool("calibre-customize", &env).is_none()
        || !plugins.exists()
        || !common::names(&plugins.to_string_lossy())
            .iter()
            .any(|n| n.starts_with("KFX Output"))
    {
        return;
    }
    assert!(sp_engine::export::kfx::kfx_status(&env).plugin_installed);
}

#[test]
fn the_guarded_env_carries_both_guards_and_nothing_of_this_process_but_path() {
    let env = common::guarded_env();
    assert_eq!(env.get("METHOD_READER_CONFIG_DIR"), Some(SETTINGS_GUARD));
    assert_eq!(env.get("CALIBRE_CONFIG_DIRECTORY"), Some(CALIBRE_GUARD));
    let names: Vec<&String> = env.vars().map(|(k, _)| k).collect();
    for name in names {
        assert!(
            [
                "PATH",
                "METHOD_READER_CONFIG_DIR",
                "CALIBRE_CONFIG_DIRECTORY"
            ]
            .contains(&name.as_str()),
            "{name}"
        );
    }
}

/// The structural half: no test file hands the engine this process's own
/// environment, which is where the real settings file and the real Calibre
/// folder are.
#[test]
fn no_test_reaches_for_this_processs_environment() {
    let dir = common::repo_root().join("rust/crates/sp-engine/tests");
    let mut files = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        let p = entry.path();
        if p.extension().is_some_and(|e| e == "rs") {
            files.push(p);
        }
    }
    let forbidden = ["Env::", "process()"].concat();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(
            !text.contains(&forbidden),
            "{} hands the engine this process's environment",
            file.display()
        );
    }
}
