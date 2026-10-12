//! `src/device/types.ts`: the shape of a connected reader, which the route
//! catalog lists. Device DETECTION (src/device/*) is ported separately (the
//! sp-device branch); this is only the type it hands over, so the routes
//! can be built and tested now. When device detection lands, it produces
//! these, or this module re-exports its own.

use serde::{Deserialize, Serialize};

/// `DeviceKind`: a supported e-reader family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceKind {
    Kindle,
    Kobo,
    Tolino,
    Remarkable,
}

impl DeviceKind {
    /// Every kind, in `DEVICE_DISPLAY_NAMES`' key order.
    pub const ALL: [DeviceKind; 4] = [
        DeviceKind::Kindle,
        DeviceKind::Kobo,
        DeviceKind::Tolino,
        DeviceKind::Remarkable,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            DeviceKind::Kindle => "kindle",
            DeviceKind::Kobo => "kobo",
            DeviceKind::Tolino => "tolino",
            DeviceKind::Remarkable => "remarkable",
        }
    }

    /// `DEVICE_DISPLAY_NAMES[kind]`.
    pub fn display_name(self) -> &'static str {
        match self {
            DeviceKind::Kindle => "Kindle",
            DeviceKind::Kobo => "Kobo",
            DeviceKind::Tolino => "tolino",
            DeviceKind::Remarkable => "reMarkable",
        }
    }
}

/// `ConnectedDevice`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectedDevice {
    pub kind: DeviceKind,
    pub name: String,
    /// Mounted volume root; null for reMarkable (network route).
    pub volume: Option<String>,
}

/// `deviceId(device)`: the volume, or the kind when there is none.
pub fn device_id(device: &ConnectedDevice) -> String {
    device
        .volume
        .clone()
        .unwrap_or_else(|| device.kind.as_str().to_owned())
}
