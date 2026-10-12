//! src/device/types.ts: the reader families and one connected reader.

use serde::{Deserialize, Serialize};

/// A supported e-reader family. Volume-mounted vendors are detected by their
/// on-disk signatures; reMarkable never mounts and is reached over its USB
/// web interface instead (see [`crate::remarkable`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceKind {
    Kindle,
    Kobo,
    Tolino,
    Remarkable,
}

impl DeviceKind {
    /// The word the engine writes for the kind (`kindle`, `kobo`, ...).
    pub fn as_str(self) -> &'static str {
        match self {
            DeviceKind::Kindle => "kindle",
            DeviceKind::Kobo => "kobo",
            DeviceKind::Tolino => "tolino",
            DeviceKind::Remarkable => "remarkable",
        }
    }

    /// DEVICE_DISPLAY_NAMES: the kind as a person reads it.
    pub fn display_name(self) -> &'static str {
        match self {
            DeviceKind::Kindle => "Kindle",
            DeviceKind::Kobo => "Kobo",
            DeviceKind::Tolino => "tolino",
            DeviceKind::Remarkable => "reMarkable",
        }
    }
}

/// How a reader that does not mount is reached over USB. Only MTP so far: a
/// Kindle from October 2024 on ([`crate::mtp`], BETA and off). A reMarkable
/// carries none; it is reached over its USB web interface, which its kind
/// already says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    Mtp,
}

/// One reader that is reachable right now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectedDevice {
    pub kind: DeviceKind,
    pub name: String,
    /// Mounted volume root; None for a reMarkable (network route) and for an
    /// MTP Kindle, neither of which ever mounts.
    pub volume: Option<String>,
    /// Present only on a reader reached over MTP.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<Transport>,
    /// The USB serial an MTP reader is addressed by. Present with transport.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
}

impl ConnectedDevice {
    /// A reader that mounts as a volume.
    pub fn mounted(
        kind: DeviceKind,
        name: impl Into<String>,
        volume: impl Into<String>,
    ) -> ConnectedDevice {
        ConnectedDevice {
            kind,
            name: name.into(),
            volume: Some(volume.into()),
            transport: None,
            serial: None,
        }
    }

    /// The docked reMarkable: no volume, its display name.
    pub fn remarkable() -> ConnectedDevice {
        ConnectedDevice {
            kind: DeviceKind::Remarkable,
            name: DeviceKind::Remarkable.display_name().into(),
            volume: None,
            transport: None,
            serial: None,
        }
    }

    /// What `send --device` takes: the volume path for a reader that mounts,
    /// `mtp:<serial>` for an MTP Kindle (two of them are two ids), and the
    /// kind for a reMarkable, of which there is only ever one.
    pub fn id(&self) -> String {
        device_id(self)
    }
}

/// `deviceId`: see [`ConnectedDevice::id`].
pub fn device_id(device: &ConnectedDevice) -> String {
    if let Some(volume) = &device.volume {
        return volume.clone();
    }
    if device.transport == Some(Transport::Mtp)
        && let Some(serial) = &device.serial
    {
        return format!("mtp:{serial}");
    }
    device.kind.as_str().to_string()
}
