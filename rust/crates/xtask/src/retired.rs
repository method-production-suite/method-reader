//! Tools the Rust port retires, kept as commands that say so.

/// `cargo xtask build-sidecar`: the Rust app links sp-engine directly, so
/// there is no engine sidecar to compile (rust/PARITY.md, "The engine
/// sidecar"). Exit 2, the code a refused tool answers, with the reason.
pub fn build_sidecar_main(_args: &[String]) -> u8 {
    eprintln!(
        "xtask build-sidecar: retired. The Rust app links the engine directly, so there is no sidecar to build. Build the CLI with `cargo xtask build-cli`."
    );
    2
}
