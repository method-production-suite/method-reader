//! The updater values a build bakes in (review item O10). Only a
//! handover-rehearsal build (`--features rehearsal`) reads the
//! METHOD_READER_UPDATE_* compile-time values; every other build has the
//! release endpoint, the release key and HTTPS, whatever was set.

#[cfg(not(feature = "rehearsal"))]
#[test]
fn a_build_without_the_rehearsal_feature_bakes_the_release_values() {
    const { assert!(!sp_update::REHEARSAL) };
    assert_eq!(sp_update::baked_endpoint(), sp_update::ENDPOINT);
    assert_eq!(sp_update::baked_pubkey(), sp_update::PUBKEY);
    assert!(!sp_update::baked_insecure());
    assert!(
        sp_update::ENDPOINT
            .starts_with("https://github.com/method-production-suite/method-reader/")
    );
}

#[test]
fn the_build_script_refuses_each_rehearsal_value_without_the_feature() {
    let script = include_str!("../build.rs");
    for name in [
        "METHOD_READER_UPDATE_ENDPOINT",
        "METHOD_READER_UPDATE_PUBKEY",
        "METHOD_READER_UPDATE_INSECURE",
    ] {
        assert!(script.contains(&format!("\"{name}\"")), "{name}");
        let lib = include_str!("../src/lib.rs");
        assert!(lib.contains(&format!("option_env!(\"{name}\")")), "{name}");
    }
    assert!(script.contains("CARGO_FEATURE_REHEARSAL"));
}
