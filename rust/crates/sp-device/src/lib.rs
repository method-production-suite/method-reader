//! Method Reader's readers, ported from src/device/, src/reveal.ts and the
//! device verbs' handlers (src/cli-devices.ts, src/cli-eject.ts,
//! src/cli-reveal.ts).
//!
//! - Finding readers: [`volumes`] lists what is mounted, [`classify`] and
//!   [`kindle`] say which vendor a volume is, [`remarkable`] probes the
//!   tablet's USB web interface, and [`list::list_devices`] puts them
//!   together.
//! - Sending: [`transfer`] copies to where each vendor indexes a book,
//!   through sp-fsutil's flushed replace; [`remarkable`] uploads.
//! - The verbs: [`commands`] (`devices`, `send`), [`eject`], [`reveal`].
//!   Each returns a serde value whose JSON is the TypeScript's field for
//!   field, or a [`error::CliError`] with the same code and words.
//! - BETA and off until hardware day: [`mtp`] (Kindles that connect over
//!   MTP, through the `method-reader-mtp` helper in sp-mtp) and
//!   [`camera_import`] (macOS's ptpcamerad, paused for one copy).
//!
//! Every child process goes through [`process::Runner`] or
//! [`process::Opener`] and every HTTP request through
//! [`remarkable::HttpClient`], so the tests use fakes and nothing real is
//! ever ejected, revealed, paused or written to.

pub mod camera_import;
pub mod classify;
pub mod commands;
pub mod eject;
pub mod error;
pub mod kindle;
pub mod list;
pub mod mtp;
pub mod paths;
pub mod process;
pub mod remarkable;
pub mod reveal;
pub mod transfer;
pub mod types;
pub mod volumes;

pub use error::{CliError, ErrorCode};
pub use list::{ListDevicesOptions, list_devices};
pub use process::Platform;
pub use types::{ConnectedDevice, DeviceKind, Transport, device_id};
