//! The `devices` and `send` handlers from src/cli-devices.ts, minus the
//! two things that are not device knowledge: argv dispatch (sp-cli) and
//! remembering the route that worked (sp-engine's settings, handed in as a
//! callback). Handlers RETURN values and never print; sp-cli owns stdout.

use crate::camera_import::{CAMERA_IMPORT_NOTE, CameraImportDeps, with_camera_import_paused};
use crate::error::{CliError, ErrorCode};
use crate::list::{ListDevicesOptions, list_devices};
use crate::mtp::{
    HelperLocation, MTP_PUT_TIMEOUT, MtpPutAnswer, mtp_helper_path, put_to_mtp_kindle,
};
use crate::process::{Platform, Runner, SpawnRunner};
use crate::remarkable::{
    HttpClient, REMARKABLE_ENDPOINT, UreqClient, not_accepted_message, remarkable_accepts,
    upload_to_remarkable,
};
use crate::transfer::copy_to_device;
use crate::types::{ConnectedDevice, DeviceKind, Transport, device_id};
use serde::Serialize;
use sp_fsutil::nodepath::host;
use std::time::Duration;

/// One device as the CLI and the window see it. `id` is what
/// `send --device` accepts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSummary {
    pub id: String,
    pub kind: DeviceKind,
    pub name: String,
    pub volume: Option<String>,
    /// `mtp` on a Kindle that connects over MTP; absent on everything else.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transport: Option<Transport>,
    /// On an MTP Kindle on a Mac: what the window shows while it copies.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub copy_note: Option<String>,
}

/// `{ "devices": [...] }`; sp-cli adds `"ok": true` in front.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DevicesResult {
    pub devices: Vec<DeviceSummary>,
}

/// One device's summary. The MTP fields appear only on an MTP Kindle.
pub fn summarize(device: &ConnectedDevice, platform: Platform) -> DeviceSummary {
    let mtp = device.transport == Some(Transport::Mtp);
    DeviceSummary {
        id: device_id(device),
        kind: device.kind,
        name: device.name.clone(),
        volume: device.volume.clone(),
        transport: mtp.then_some(Transport::Mtp),
        copy_note: (mtp && platform == Platform::Darwin).then(|| CAMERA_IMPORT_NOTE.to_string()),
    }
}

/// Everything plugged in. An empty list is an answer, not an error.
pub fn devices_command(options: &ListDevicesOptions, platform: Platform) -> DevicesResult {
    DevicesResult {
        devices: list_devices(options)
            .iter()
            .map(|d| summarize(d, platform))
            .collect(),
    }
}

/// Which device gets the book. The precedence is deliberate:
///   1. Nothing connected: no-devices, EVEN IF an id was given (the
///      actionable fact is that nothing is plugged in).
///   2. An id given: the exact id, or unknown-device. It wins over the
///      ambiguity check: the person has already answered that question.
///   3. Exactly one connected: that one.
///   4. Several: ambiguous-device, listing the ids. Never a guess.
pub fn select_device(
    devices: &[ConnectedDevice],
    requested_id: Option<&str>,
) -> Result<ConnectedDevice, CliError> {
    let ids: Vec<String> = devices.iter().map(device_id).collect();
    if devices.is_empty() {
        return Err(CliError::new(
            ErrorCode::NoDevices,
            "no reader is connected: plug one in over USB and try again",
        ));
    }
    if let Some(requested) = requested_id {
        return match ids.iter().position(|id| id == requested) {
            Some(i) => Ok(devices[i].clone()),
            None => Err(CliError::new(
                ErrorCode::UnknownDevice,
                format!(
                    "no connected reader has the id \"{requested}\". Connected: {}",
                    ids.join(", ")
                ),
            )),
        };
    }
    if devices.len() == 1 {
        return Ok(devices[0].clone());
    }
    Err(CliError::new(
        ErrorCode::AmbiguousDevice,
        format!(
            "several readers are connected. Pick one with --device: {}",
            ids.join(", ")
        ),
    ))
}

/// The device a send went to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SentTo {
    pub id: String,
    pub kind: DeviceKind,
    pub name: String,
}

/// `send`'s answer. `destination` is absent for a reMarkable, which has no
/// path; on an MTP Kindle it is the path on the device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendResult {
    pub device: SentTo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub destination: Option<String>,
    /// True for a reMarkable, which reports no destination.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uploaded: Option<bool>,
    /// True for an MTP Kindle: nothing was mounted, nothing to eject.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safe_to_unplug: Option<bool>,
    /// On an MTP Kindle: whether macOS's camera import was paused for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera_import_paused: Option<bool>,
}

/// What a Kindle opens when it is copied over USB: AZW3, MOBI, KFX, PDF and
/// TXT, with the older AZW and PRC names of MOBI. Never an EPUB.
pub const KINDLE_USB_TYPES: &[&str] = &["kfx", "azw3", "azw", "mobi", "prc", "pdf", "txt"];

/// How long the helper keeps trying to open a Kindle the camera-import
/// daemon was just told to let go of.
pub const RELEASE_WAIT: Duration = Duration::from_secs(5);

/// The MTP copy's seams.
pub struct MtpSend<'a> {
    /// The helper; None: look for it ([`mtp_helper_path`]).
    pub helper: Option<Option<String>>,
    pub run: &'a dyn Runner,
    pub timeout: Duration,
}

impl Default for MtpSend<'_> {
    fn default() -> Self {
        MtpSend {
            helper: None,
            run: &SpawnRunner,
            timeout: MTP_PUT_TIMEOUT,
        }
    }
}

/// `send`'s inputs and seams.
pub struct SendOptions<'a> {
    /// An EXISTING file: `send` never converts.
    pub file: String,
    /// The id `devices` reports. None: the single connected device.
    pub device_id: Option<String>,
    pub list: ListDevicesOptions<'a>,
    pub http: &'a dyn HttpClient,
    pub mtp: MtpSend<'a>,
    /// The camera-import pause for an MTP copy on a Mac. sp-engine supplies
    /// [`CameraImportDeps::host`] with its config folder; None pauses nothing.
    pub camera_import: Option<CameraImportDeps>,
    /// Remember the route that worked (`device:<kind>` or `remarkable`), so
    /// the send panel chooses it first next time. sp-engine writes it to the
    /// app settings, and a settings file that cannot be written never fails
    /// a send. Called only after a send that worked.
    pub remember: &'a dyn Fn(&str),
}

impl<'a> SendOptions<'a> {
    pub fn new(file: impl Into<String>, remember: &'a dyn Fn(&str)) -> SendOptions<'a> {
        SendOptions {
            file: file.into(),
            device_id: None,
            list: ListDevicesOptions::default(),
            http: &UreqClient,
            mtp: MtpSend::default(),
            camera_import: None,
            remember,
        }
    }
}

fn ext_of(file: &str) -> String {
    host::extname(file).trim_start_matches('.').to_lowercase()
}

/// Send an existing file to a connected reader.
///
/// The file is checked FIRST, before the device list is built: a typo in
/// the filename must not be reported as "no devices", and must not pay the
/// reMarkable probe's timeout to find that out.
pub fn send_command(options: &SendOptions) -> Result<SendResult, CliError> {
    let is_file = std::fs::metadata(&options.file)
        .map(|m| m.is_file())
        .unwrap_or(false);
    if !is_file {
        return Err(CliError::new(
            ErrorCode::Unreadable,
            format!("cannot read the file to send: {}", options.file),
        ));
    }

    let devices = list_devices(&options.list);
    let device = select_device(&devices, options.device_id.as_deref())?;
    let identity = SentTo {
        id: device_id(&device),
        kind: device.kind,
        name: device.name.clone(),
    };

    if device.kind == DeviceKind::Remarkable {
        // Asked before the upload, not inferred from its error, and in the
        // SAME sentence the upload would have used.
        if !remarkable_accepts(&options.file) {
            return Err(CliError::new(
                ErrorCode::UnsupportedFile,
                not_accepted_message(&options.file),
            ));
        }
        let endpoint = options
            .list
            .remarkable_endpoint
            .as_deref()
            .unwrap_or(REMARKABLE_ENDPOINT);
        upload_to_remarkable(options.http, &options.file, endpoint)
            .map_err(|e| CliError::new(ErrorCode::SendFailed, e.message))?;
        (options.remember)("remarkable");
        return Ok(SendResult {
            device: identity,
            destination: None,
            uploaded: Some(true),
            safe_to_unplug: None,
            camera_import_paused: None,
        });
    }

    if device.transport == Some(Transport::Mtp) {
        return send_to_mtp_kindle(options, &device, identity);
    }

    let destination = copy_to_device(&options.file, &device)
        .map_err(|e| CliError::new(ErrorCode::SendFailed, e.to_string()))?;
    (options.remember)(&format!("device:{}", device.kind.as_str()));
    Ok(SendResult {
        device: identity,
        destination: Some(destination),
        uploaded: None,
        safe_to_unplug: None,
        camera_import_paused: None,
    })
}

/// BETA. Copy to a Kindle that connects over MTP, through the helper, with
/// macOS's camera import paused for the length of the copy (and only on a
/// Mac). Refusals that need no device come first.
fn send_to_mtp_kindle(
    options: &SendOptions,
    device: &ConnectedDevice,
    identity: SentTo,
) -> Result<SendResult, CliError> {
    let ext = ext_of(&options.file);
    if !KINDLE_USB_TYPES.contains(&ext.as_str()) {
        let message = if ext == "epub" {
            "a Kindle does not read an EPUB copied to it. Send the Kindle file instead: method-reader export --for kindle makes it".to_string()
        } else {
            format!(
                "a Kindle cannot open .{ext} copied over USB. Send a KFX, AZW3, MOBI, PDF or TXT"
            )
        };
        return Err(CliError::new(ErrorCode::UnsupportedFile, message));
    }
    let serial = device.serial.clone().unwrap_or_default();
    let helper = match &options.mtp.helper {
        Some(h) => h.clone(),
        None => mtp_helper_path(&HelperLocation::host()),
    };
    let copy = |paused: bool| {
        put_to_mtp_kindle(
            helper.as_deref(),
            options.mtp.run,
            &serial,
            &options.file,
            paused.then_some(RELEASE_WAIT),
            options.mtp.timeout,
        )
    };
    let (answer, paused) = match &options.camera_import {
        Some(deps) => with_camera_import_paused(deps, |pause| (copy(pause.paused), pause.paused)),
        None => (copy(false), false),
    };
    match answer {
        MtpPutAnswer::Failed {
            code,
            message,
            holder,
        } => {
            if code == "unreadable" {
                return Err(CliError::new(ErrorCode::Unreadable, message));
            }
            if code == "busy" && holder.as_deref() == Some("ptpcamerad") && paused {
                return Err(CliError::new(
                    ErrorCode::SendFailed,
                    "macOS's camera import would not let go of the Kindle. Unplug it, plug it in again, and try again",
                ));
            }
            // The helper's sentence as it wrote it.
            Err(CliError::new(ErrorCode::SendFailed, message))
        }
        MtpPutAnswer::Done { path, .. } => {
            (options.remember)("device:kindle");
            Ok(SendResult {
                device: identity,
                destination: Some(path),
                uploaded: None,
                safe_to_unplug: Some(true),
                camera_import_paused: Some(paused),
            })
        }
    }
}
