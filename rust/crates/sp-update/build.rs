//! A build that is not a handover rehearsal refuses the rehearsal's
//! compile-time values (review item O10): a throwaway key, a local endpoint
//! or plain HTTP must never reach a release because a variable was left in
//! someone's shell. src/lib.rs ignores them outside a rehearsal as well.

const REHEARSAL_ONLY: [&str; 3] = [
    "METHOD_READER_UPDATE_ENDPOINT",
    "METHOD_READER_UPDATE_PUBKEY",
    "METHOD_READER_UPDATE_INSECURE",
];

fn main() {
    let rehearsal = std::env::var_os("CARGO_FEATURE_REHEARSAL").is_some();
    for name in REHEARSAL_ONLY {
        println!("cargo:rerun-if-env-changed={name}");
        if !rehearsal && std::env::var_os(name).is_some() {
            panic!(
                "{name} is set, but this is not a handover-rehearsal build. It would bake a \
                 rehearsal's updater setting into the app. Unset it, or build with \
                 `--features rehearsal` (cargo xtask rehearse-handover prepare does)."
            );
        }
    }
}
