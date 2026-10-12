//! src/device/classify.ts: the pure vendor predicate. Given a mounted
//! volume's path, which e-reader family is it? All device knowledge lives
//! here, so it is testable with temp folders on every platform. Enumerating
//! what is actually mounted is [`crate::volumes`]'s job.

use crate::kindle::is_kindle_volume;
use crate::paths::{is_directory, is_windows_drive_root};
use crate::types::DeviceKind;
use sp_fsutil::nodepath::host;

/// A volume's name: its last path component, except that a bare Windows
/// drive root has none, so it is named by its drive designator (`D:\` ->
/// `D:`) rather than surfacing an empty device name.
pub fn volume_name(volume: &str) -> String {
    if is_windows_drive_root(volume) {
        return volume[..volume.len() - 1].to_string();
    }
    host::basename(volume)
}

/// Vendor signature of a mounted volume, or None for a plain drive. Kobo
/// firmware keeps a `.kobo` folder at the root; tolino mounts under its
/// brand name; Kindle keeps the documents/ check. The order decides an
/// ambiguous volume: Kindle, then Kobo, then the tolino name.
///
/// The tolino name cannot be seen on a Windows drive root ("D:" carries no
/// vendor), and this layer deliberately does no volume-label lookup.
pub fn classify(volume: &str) -> Option<DeviceKind> {
    if is_kindle_volume(volume) {
        return Some(DeviceKind::Kindle);
    }
    if is_directory(&host::join(&[volume, ".kobo"])) {
        return Some(DeviceKind::Kobo);
    }
    if volume_name(volume).to_lowercase().contains("tolino") {
        return Some(DeviceKind::Tolino);
    }
    None
}
