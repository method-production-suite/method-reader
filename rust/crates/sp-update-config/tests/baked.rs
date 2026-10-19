//! The updater values a build bakes in (review item O10). Only a
//! handover-rehearsal build (`--features rehearsal`) reads the
//! METHOD_READER_UPDATE_* compile-time values; every other build has the
//! release endpoint, the release key and HTTPS, whatever was set.

#[cfg(not(feature = "rehearsal"))]
#[test]
fn a_build_without_the_rehearsal_feature_bakes_the_release_values() {
    const { assert!(!sp_update_config::REHEARSAL) };
    let baked = sp_update_config::baked();
    assert_eq!(
        baked,
        method_update::Baked::release(sp_update_config::ENDPOINT, sp_update_config::PUBKEY)
    );
    assert!(!baked.insecure && !baked.rehearsal);
    assert!(
        sp_update_config::ENDPOINT
            .starts_with("https://github.com/method-production-suite/method-reader/")
    );
    let config = sp_update_config::update_config();
    assert_eq!(config.endpoint, sp_update_config::ENDPOINT);
    assert_eq!(config.pubkey, sp_update_config::PUBKEY);
    assert!(!config.allow_insecure);
}

/// `cargo xtask rehearse-handover prepare`'s build: the local endpoint, the
/// throwaway key and plain HTTP it was compiled with, each only if set.
#[cfg(feature = "rehearsal")]
#[test]
fn a_rehearsal_build_bakes_what_it_was_compiled_with() {
    let baked = sp_update_config::baked();
    assert!(baked.rehearsal);
    assert_eq!(
        baked.endpoint,
        option_env!("METHOD_READER_UPDATE_ENDPOINT").unwrap_or(sp_update_config::ENDPOINT)
    );
    assert_eq!(
        baked.pubkey,
        option_env!("METHOD_READER_UPDATE_PUBKEY").unwrap_or(sp_update_config::PUBKEY)
    );
    assert_eq!(
        baked.insecure,
        option_env!("METHOD_READER_UPDATE_INSECURE") == Some("1")
    );
    assert_eq!(
        sp_update_config::update_config().allow_insecure,
        baked.insecure
    );
}

#[test]
fn the_build_script_refuses_each_rehearsal_value_without_the_feature() {
    let script = include_str!("../build.rs");
    let lib = include_str!("../src/lib.rs");
    for suffix in method_update::config::REHEARSAL_VARIABLES {
        let name = format!("{}{suffix}", sp_update_config::REHEARSAL_PREFIX);
        assert!(script.contains(&format!("\"{name}\"")), "{name}");
    }
    assert!(script.contains("CARGO_FEATURE_REHEARSAL"));
    // The values are read through method-update's baked!, under Reader's
    // prefix and Reader's own feature.
    assert!(lib.contains(&format!(
        "prefix = \"{}\"",
        sp_update_config::REHEARSAL_PREFIX
    )));
    assert!(lib.contains("cfg!(feature = \"rehearsal\")"));
    assert!(lib.contains("rehearsal = REHEARSAL"));
}
