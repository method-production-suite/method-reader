//! method-reader-mtp: list Kindles that connect over MTP, and copy one file to one.
//!
//! Everything that can be decided without a device lives in a module of its
//! own and is unit-tested: the JSON envelope (`output`), argv (`args`), the
//! device path and temp name (`paths`), what counts as a Kindle (`kindle`),
//! macOS's IORegistry text (`ioreg`), and the copy itself (`put`), which runs
//! against the `Session` trait so a fake can drive every branch. `real` is
//! the one adapter onto mtp-rs; tests/virtual_device.rs runs it against
//! mtp-rs's virtual device. `main.rs` only wires them together.

pub mod args;
pub mod ioreg;
pub mod kindle;
pub mod output;
pub mod paths;
pub mod put;
pub mod real;
pub mod session;
