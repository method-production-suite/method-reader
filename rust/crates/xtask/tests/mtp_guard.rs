//! xtask keeps its own copy of sp-device's `MTP_HELPER_GUARD` (parity runs
//! and the bundle smoke set `METHOD_READER_MTP_HELPER` to it) rather than
//! depending on sp-device, which would bring rustix into the developer
//! tools for one string. This holds the copy to the original: if sp-device
//! moves its guard, the parity CLI and the smoked window would otherwise
//! find the real helper beside them and let a plugged-in Kindle in.

use xtask::parity::MTP_HELPER_GUARD;
use xtask::paths::workspace_root;

#[test]
fn the_guard_is_the_one_sp_device_defines() {
    let source = std::fs::read_to_string(workspace_root().join("crates/sp-device/src/mtp.rs"))
        .expect("read sp-device's mtp.rs");
    let line = source
        .lines()
        .find(|l| {
            l.trim_start()
                .starts_with("pub const MTP_HELPER_GUARD: &str = ")
        })
        .expect("sp-device still defines MTP_HELPER_GUARD as a &str");
    let value = line
        .split_once('"')
        .and_then(|(_, rest)| rest.rsplit_once('"'))
        .map(|(v, _)| v)
        .expect("a string literal");
    assert_eq!(MTP_HELPER_GUARD, value);
    // And it is still a path nothing can create.
    assert!(MTP_HELPER_GUARD.starts_with("/dev/null/"));
}
