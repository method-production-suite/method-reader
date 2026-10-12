//! src/device/transfer.ts: copy a book to where its vendor indexes it.

use crate::kindle::{copy_to_kindle_volume, make_dir_all};
use crate::types::{ConnectedDevice, DeviceKind};
use sp_fsutil::NodeIoError;
use sp_fsutil::nodepath::host;
use std::fmt;
use std::path::Path;

/// Why a copy did not happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferError {
    /// `NoVolumeError`: the device has no mounted volume (a reMarkable, or a
    /// reader reached over MTP).
    NoVolume,
    /// The file system refused, in Node's words.
    Io(NodeIoError),
}

impl fmt::Display for TransferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransferError::NoVolume => f.write_str("Device has no mounted volume."),
            TransferError::Io(e) => f.write_str(&e.message),
        }
    }
}

impl std::error::Error for TransferError {}

impl From<NodeIoError> for TransferError {
    fn from(e: NodeIoError) -> Self {
        TransferError::Io(e)
    }
}

/// Copy a book to its vendor's expected location: Kindle -> documents/,
/// Kobo -> the volume root, tolino -> Books/ at the root (subfolders of it
/// are not reliably indexed; the folder is made if missing). Replaces any
/// previous copy. Returns the destination path.
pub fn copy_to_device(file: &str, device: &ConnectedDevice) -> Result<String, TransferError> {
    let Some(volume) = device.volume.as_deref() else {
        return Err(TransferError::NoVolume);
    };
    let dest_dir = match device.kind {
        DeviceKind::Kindle => return Ok(copy_to_kindle_volume(file, volume)?),
        DeviceKind::Kobo => volume.to_string(),
        DeviceKind::Tolino => host::join(&[volume, "Books"]),
        DeviceKind::Remarkable => return Err(TransferError::NoVolume),
    };
    make_dir_all(&dest_dir)?;
    let dest = host::join(&[&dest_dir, &host::basename(file)]);
    sp_fsutil::replace_file(Path::new(file), Path::new(&dest))?;
    Ok(dest)
}
