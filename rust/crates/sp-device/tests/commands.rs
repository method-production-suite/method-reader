//! The device handlers in tests/cli-devices.test.ts (devicesCommand,
//! selectDevice, sendCommand) and branch mtp-kindle's
//! tests/cli-mtp-send.test.ts ("send to an MTP Kindle"), ported. Argv
//! dispatch, the spawned CLI and the settings file a send remembers into go
//! with sp-cli and sp-engine; here `remember` is a recording callback.

mod common;
use common::{Recording, Stub, exit, s, scratch, strings};
use serde_json::json;
use sp_device::camera_import::{CameraImportDeps, camera_import_marker};
use sp_device::commands::{
    DeviceSummary, KINDLE_USB_TYPES, MtpSend, SendOptions, SendResult, SentTo, devices_command,
    select_device, send_command,
};
use sp_device::list::ListDevicesOptions;
use sp_device::process::Runner;
use sp_device::remarkable::UreqClient;
use sp_device::{ConnectedDevice, DeviceKind, ErrorCode, Platform, Transport};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn kobo() -> ConnectedDevice {
    ConnectedDevice::mounted(
        DeviceKind::Kobo,
        "KOBOeReader",
        "/run/media/sam/KOBOeReader",
    )
}
fn kindle() -> ConnectedDevice {
    ConnectedDevice::mounted(DeviceKind::Kindle, "Kindle", "/run/media/sam/Kindle")
}

// devicesCommand

#[test]
fn devices_command_reports_each_device_with_the_id_send_accepts() {
    let scan = |_: Option<&[String]>| vec![kobo()];
    let probe = |_: &str, _: Duration| Ok(true);
    let result = devices_command(
        &ListDevicesOptions {
            scan: Some(&scan),
            probe: Some(&probe),
            skip_mtp: true,
            ..Default::default()
        },
        Platform::Darwin,
    );
    assert_eq!(
        serde_json::to_value(&result).unwrap(),
        json!({ "devices": [
            { "id": "/run/media/sam/KOBOeReader", "kind": "kobo", "name": "KOBOeReader", "volume": "/run/media/sam/KOBOeReader" },
            { "id": "remarkable", "kind": "remarkable", "name": "reMarkable", "volume": null },
        ]})
    );
}

#[test]
fn devices_command_on_an_empty_list_is_an_empty_array() {
    let scan = |_: Option<&[String]>| Vec::new();
    let probe = |_: &str, _: Duration| Ok(false);
    let result = devices_command(
        &ListDevicesOptions {
            scan: Some(&scan),
            probe: Some(&probe),
            skip_mtp: true,
            ..Default::default()
        },
        Platform::Linux,
    );
    assert_eq!(serde_json::to_string(&result).unwrap(), r#"{"devices":[]}"#);
}

#[test]
fn an_mtp_kindle_carries_transport_and_on_a_mac_only_the_camera_note() {
    let scan = |_: Option<&[String]>| Vec::new();
    let mtp = || Ok(vec![mtp_kindle("Kindle", "G000INVENTED01")]);
    let options = ListDevicesOptions {
        scan: Some(&scan),
        mtp: Some(&mtp),
        skip_remarkable: true,
        ..Default::default()
    };
    let mac = devices_command(&options, Platform::Darwin);
    assert_eq!(
        serde_json::to_value(&mac.devices[0]).unwrap(),
        json!({
            "id": "mtp:G000INVENTED01", "kind": "kindle", "name": "Kindle", "volume": null, "transport": "mtp",
            "copyNote": sp_device::camera_import::CAMERA_IMPORT_NOTE,
        })
    );
    for platform in [Platform::Linux, Platform::Win32] {
        let other = devices_command(&options, platform);
        assert_eq!(other.devices[0].transport, Some(Transport::Mtp));
        assert_eq!(other.devices[0].copy_note, None);
    }
    let _: &DeviceSummary = &mac.devices[0];
}

// selectDevice

#[test]
fn one_device_connected_and_no_id_picks_it() {
    assert_eq!(select_device(&[kobo()], None).unwrap(), kobo());
}

#[test]
fn an_explicit_id_picks_that_device_even_when_several_are_connected() {
    assert_eq!(
        select_device(&[kobo(), kindle()], Some("/run/media/sam/Kindle")).unwrap(),
        kindle()
    );
    let tablet = ConnectedDevice::remarkable();
    assert_eq!(
        select_device(&[kobo(), kindle(), tablet.clone()], Some("remarkable")).unwrap(),
        tablet
    );
}

#[test]
fn several_connected_and_no_id_is_ambiguous_listing_the_ids() {
    let err = select_device(&[kobo(), kindle()], None).unwrap_err();
    assert_eq!(err.code, ErrorCode::AmbiguousDevice);
    assert_eq!(
        err.message,
        "several readers are connected. Pick one with --device: /run/media/sam/KOBOeReader, /run/media/sam/Kindle"
    );
}

#[test]
fn nothing_connected_is_no_devices_even_when_an_id_was_given() {
    let err = select_device(&[], Some("/run/media/sam/Kindle")).unwrap_err();
    assert_eq!(err.code, ErrorCode::NoDevices);
    assert_eq!(
        err.message,
        "no reader is connected: plug one in over USB and try again"
    );
}

#[test]
fn nothing_connected_and_no_id_is_also_no_devices() {
    assert_eq!(
        select_device(&[], None).unwrap_err().code,
        ErrorCode::NoDevices
    );
}

#[test]
fn an_id_that_matches_nothing_is_unknown_device_listing_what_is() {
    let err = select_device(&[kobo(), kindle()], Some("/run/media/sam/Nope")).unwrap_err();
    assert_eq!(err.code, ErrorCode::UnknownDevice);
    assert_eq!(
        err.message,
        "no connected reader has the id \"/run/media/sam/Nope\". Connected: /run/media/sam/KOBOeReader, /run/media/sam/Kindle"
    );
}

#[test]
fn selection_matches_on_the_id_never_a_prefix_or_the_name() {
    assert_eq!(
        select_device(&[kindle()], Some("/run/media/sam"))
            .unwrap_err()
            .code,
        ErrorCode::UnknownDevice
    );
    assert_eq!(
        select_device(&[kindle()], Some("Kindle")).unwrap_err().code,
        ErrorCode::UnknownDevice
    );
}

// sendCommand

fn book(scratch: &Path, name: &str) -> String {
    let dir = tempfile::Builder::new()
        .prefix("send-")
        .tempdir_in(scratch)
        .unwrap()
        .keep();
    let path = dir.join(name);
    std::fs::write(&path, "book-bytes").unwrap();
    s(&path)
}

fn kindle_volume(scratch: &Path) -> ConnectedDevice {
    let dir = tempfile::Builder::new()
        .prefix("vol-")
        .tempdir_in(scratch)
        .unwrap()
        .keep()
        .join("Kindle");
    std::fs::create_dir_all(dir.join("documents")).unwrap();
    ConnectedDevice::mounted(DeviceKind::Kindle, "Kindle", s(&dir))
}

struct Remembered(Mutex<Vec<String>>);

impl Remembered {
    fn new() -> Remembered {
        Remembered(Mutex::new(Vec::new()))
    }
    fn all(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

#[test]
fn sending_to_a_volume_device_copies_it_where_that_vendor_indexes_and_remembers_the_kind() {
    let sc = scratch("method-reader-cli-devices-");
    let device = kindle_volume(sc.path());
    let file = book(sc.path(), "Script.epub");
    let remembered = Remembered::new();
    let remember = |r: &str| remembered.0.lock().unwrap().push(r.into());
    let d = device.clone();
    let scan = move |_: Option<&[String]>| vec![d.clone()];
    let probe = |_: &str, _: Duration| Ok(false);
    let mut options = SendOptions::new(&file, &remember);
    options.list = ListDevicesOptions {
        scan: Some(&scan),
        probe: Some(&probe),
        skip_mtp: true,
        ..Default::default()
    };
    let result = send_command(&options).unwrap();
    let volume = device.volume.clone().unwrap();
    assert_eq!(
        result.device,
        SentTo {
            id: volume.clone(),
            kind: DeviceKind::Kindle,
            name: "Kindle".into()
        }
    );
    let dest = result.destination.clone().unwrap();
    assert_eq!(
        dest,
        s(&Path::new(&volume).join("documents").join("Script.epub"))
    );
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "book-bytes");
    assert_eq!(result.uploaded, None);
    // A volume send remembers the device KIND, never its volume.
    assert_eq!(remembered.all(), ["device:kindle"]);
    assert_eq!(
        serde_json::to_value(&result).unwrap(),
        json!({ "device": { "id": volume, "kind": "kindle", "name": "Kindle" }, "destination": dest })
    );
}

#[test]
fn sending_to_a_remarkable_uploads_reports_no_destination_and_remembers_remarkable() {
    let sc = scratch("method-reader-cli-devices-");
    let stub = Stub::start(200);
    let remembered = Remembered::new();
    let remember = |r: &str| remembered.0.lock().unwrap().push(r.into());
    let scan = |_: Option<&[String]>| Vec::new();
    let probe = |_: &str, _: Duration| Ok(true);
    let file = book(sc.path(), "Script.epub");
    let mut options = SendOptions::new(&file, &remember);
    options.list = ListDevicesOptions {
        scan: Some(&scan),
        probe: Some(&probe),
        skip_mtp: true,
        remarkable_endpoint: Some(stub.url.clone()),
        ..Default::default()
    };
    options.http = &UreqClient;
    let result = send_command(&options).unwrap();
    assert_eq!(
        serde_json::to_value(&result).unwrap(),
        json!({ "device": { "id": "remarkable", "kind": "remarkable", "name": "reMarkable" }, "uploaded": true })
    );
    assert!(stub.requests().contains(&"POST /upload".to_string()));
    assert_eq!(remembered.all(), ["remarkable"]);
}

#[test]
fn a_remarkable_refuses_an_extension_it_cannot_read_before_any_request() {
    let sc = scratch("method-reader-cli-devices-");
    let stub = Stub::start(200);
    let remembered = Remembered::new();
    let remember = |r: &str| remembered.0.lock().unwrap().push(r.into());
    let scan = |_: Option<&[String]>| Vec::new();
    let probe = |_: &str, _: Duration| Ok(true);
    let file = book(sc.path(), "Script.azw3");
    let mut options = SendOptions::new(&file, &remember);
    options.list = ListDevicesOptions {
        scan: Some(&scan),
        probe: Some(&probe),
        skip_mtp: true,
        remarkable_endpoint: Some(stub.url.clone()),
        ..Default::default()
    };
    let err = send_command(&options).unwrap_err();
    assert_eq!(err.code, ErrorCode::UnsupportedFile);
    assert_eq!(err.message, "reMarkable accepts PDF and EPUB, not .azw3.");
    assert!(stub.requests().is_empty());
    assert!(remembered.all().is_empty());
}

#[test]
fn a_failed_copy_is_send_failed_carrying_the_underlying_message_and_remembers_nothing() {
    // A volume whose PARENT is a regular file: making the folder fails
    // ENOTDIR on every platform.
    let sc = scratch("method-reader-cli-devices-");
    let blocker = sc.path().join("not-a-dir");
    std::fs::write(&blocker, "x").unwrap();
    let device =
        ConnectedDevice::mounted(DeviceKind::Kobo, "KOBOeReader", s(&blocker.join("Kobo")));
    let remembered = Remembered::new();
    let remember = |r: &str| remembered.0.lock().unwrap().push(r.into());
    let d = device.clone();
    let scan = move |_: Option<&[String]>| vec![d.clone()];
    let probe = |_: &str, _: Duration| Ok(false);
    let file = book(sc.path(), "Script.epub");
    let mut options = SendOptions::new(&file, &remember);
    options.device_id = device.volume.clone();
    options.list = ListDevicesOptions {
        scan: Some(&scan),
        probe: Some(&probe),
        skip_mtp: true,
        ..Default::default()
    };
    let err = send_command(&options).unwrap_err();
    assert_eq!(err.code, ErrorCode::SendFailed);
    assert!(err.message.contains("ENOTDIR"), "{}", err.message);
    assert!(remembered.all().is_empty());
}

#[test]
fn a_failed_upload_is_send_failed() {
    let sc = scratch("method-reader-cli-devices-");
    let refusing = Stub::start(500);
    let remember = |_: &str| panic!("a failed upload remembers nothing");
    let scan = |_: Option<&[String]>| Vec::new();
    let probe = |_: &str, _: Duration| Ok(true);
    let file = book(sc.path(), "Script.epub");
    let mut options = SendOptions::new(&file, &remember);
    options.list = ListDevicesOptions {
        scan: Some(&scan),
        probe: Some(&probe),
        skip_mtp: true,
        remarkable_endpoint: Some(refusing.url.clone()),
        ..Default::default()
    };
    let err = send_command(&options).unwrap_err();
    assert_eq!(err.code, ErrorCode::SendFailed);
    assert_eq!(
        err.message,
        "couldn't open the tablet's root folder (HTTP 500); nothing was uploaded."
    );
}

#[test]
fn a_file_that_is_not_there_is_unreadable_and_no_device_is_touched() {
    let sc = scratch("method-reader-cli-devices-");
    let scanned = Mutex::new(0);
    let scan = |_: Option<&[String]>| {
        *scanned.lock().unwrap() += 1;
        Vec::new()
    };
    let remember = |_: &str| {};
    let gone = s(&sc.path().join("no-such-file.epub"));
    let mut options = SendOptions::new(&gone, &remember);
    options.list = ListDevicesOptions {
        scan: Some(&scan),
        skip_remarkable: true,
        skip_mtp: true,
        ..Default::default()
    };
    let err = send_command(&options).unwrap_err();
    assert_eq!(err.code, ErrorCode::Unreadable);
    assert_eq!(err.message, format!("cannot read the file to send: {gone}"));
    assert_eq!(*scanned.lock().unwrap(), 0);
}

#[test]
fn a_directory_given_as_the_file_is_unreadable_not_send_failed() {
    let sc = scratch("method-reader-cli-devices-");
    let device = kindle_volume(sc.path());
    let scan = move |_: Option<&[String]>| vec![device.clone()];
    let remember = |_: &str| {};
    let dir = s(sc.path());
    let mut options = SendOptions::new(&dir, &remember);
    options.list = ListDevicesOptions {
        scan: Some(&scan),
        skip_remarkable: true,
        skip_mtp: true,
        ..Default::default()
    };
    assert_eq!(
        send_command(&options).unwrap_err().code,
        ErrorCode::Unreadable
    );
}

#[test]
fn the_error_json_always_carries_a_message() {
    let err = sp_device::CliError::new(ErrorCode::SendFailed, "boom");
    assert_eq!(
        serde_json::to_value(&err).unwrap(),
        json!({ "code": "send-failed", "message": "boom" })
    );
}

// Send to an MTP Kindle (branch mtp-kindle, BETA and off).

fn mtp_kindle(name: &str, serial: &str) -> ConnectedDevice {
    ConnectedDevice {
        kind: DeviceKind::Kindle,
        name: name.into(),
        volume: None,
        transport: Some(Transport::Mtp),
        serial: Some(serial.into()),
    }
}

const DONE: &str =
    r#"{"ok":true,"path":"documents/Field Station.kfx","bytes":3,"method":"rename"}"#;

fn helper(answer: &str) -> Arc<Recording> {
    let answer = answer.to_string();
    let ok = answer.contains("\"ok\":true");
    Recording::new(move |_| Ok(exit(if ok { 0 } else { 1 }, &answer, "")))
}

fn camera(platform: Platform, config: &Path, run: Arc<Recording>) -> CameraImportDeps {
    CameraImportDeps {
        platform,
        uid: 501,
        run,
        marker: camera_import_marker(config),
        launchctl: "/fake/launchctl".into(),
        pkill: "/fake/pkill".into(),
        pid: std::process::id(),
        is_alive: Arc::new(|_| false),
        now: Arc::new(|| 1_000),
    }
}

struct MtpCase {
    _scratch: tempfile::TempDir,
    config: std::path::PathBuf,
    file: String,
}

fn mtp_case(name: &str) -> MtpCase {
    let sc = scratch("method-reader-mtp-send-");
    let config = sc.path().join("config");
    std::fs::create_dir_all(&config).unwrap();
    let file = book(sc.path(), name);
    MtpCase {
        _scratch: sc,
        config,
        file,
    }
}

fn send_mtp(
    case: &MtpCase,
    put: &dyn Runner,
    cam: Option<CameraImportDeps>,
    devices: Vec<ConnectedDevice>,
    device_id: Option<&str>,
    remember: &dyn Fn(&str),
) -> Result<SendResult, sp_device::CliError> {
    let scan = |_: Option<&[String]>| Vec::new();
    let mtp = move || Ok(devices.clone());
    let probe = |_: &str, _: Duration| Ok(false);
    let mut options = SendOptions::new(&case.file, remember);
    options.device_id = device_id.map(str::to_string);
    options.list = ListDevicesOptions {
        scan: Some(&scan),
        mtp: Some(&mtp),
        probe: Some(&probe),
        ..Default::default()
    };
    options.mtp = MtpSend {
        helper: Some(Some("/fake/method-reader-mtp".into())),
        run: put,
        timeout: Duration::from_secs(600),
    };
    options.camera_import = cam;
    send_command(&options)
}

#[test]
fn mtp_on_a_mac_pause_copy_with_a_wait_restore_safe_to_unplug_remembered() {
    let case = mtp_case("Field Station.kfx");
    let put = helper(DONE);
    let cam = Recording::ok();
    let remembered = Remembered::new();
    let remember = |r: &str| remembered.0.lock().unwrap().push(r.into());
    let result = send_mtp(
        &case,
        &*put,
        Some(camera(Platform::Darwin, &case.config, cam.clone())),
        vec![mtp_kindle("Kindle Paperwhite", "G000INVENTED01")],
        None,
        &remember,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&result).unwrap(),
        json!({
            "device": { "id": "mtp:G000INVENTED01", "kind": "kindle", "name": "Kindle Paperwhite" },
            "destination": "documents/Field Station.kfx",
            "safeToUnplug": true,
            "cameraImportPaused": true,
        })
    );
    assert_eq!(
        put.calls(),
        [strings(&[
            "/fake/method-reader-mtp",
            "put",
            "G000INVENTED01",
            &case.file,
            "documents/Field Station.kfx",
            "--wait-ms",
            "5000"
        ])]
    );
    let verbs: Vec<String> = cam
        .calls()
        .iter()
        .map(|c| c.get(1).cloned().unwrap_or_else(|| c[0].clone()))
        .collect();
    assert_eq!(verbs, ["print-disabled", "disable", "-9", "enable"]);
    assert!(!camera_import_marker(&case.config).exists());
    assert_eq!(remembered.all(), ["device:kindle"]);
}

#[test]
fn mtp_off_a_mac_no_pause_no_wait() {
    let case = mtp_case("Field Station.kfx");
    let put = helper(DONE);
    let cam = Recording::ok();
    let remember = |_: &str| {};
    let result = send_mtp(
        &case,
        &*put,
        Some(camera(Platform::Linux, &case.config, cam.clone())),
        vec![mtp_kindle("Kindle Paperwhite", "G000INVENTED01")],
        None,
        &remember,
    )
    .unwrap();
    assert_eq!(result.camera_import_paused, Some(false));
    assert!(cam.calls().is_empty());
    assert!(!put.calls()[0].contains(&"--wait-ms".to_string()));
}

#[test]
fn mtp_a_failed_copy_is_send_failed_in_the_helper_s_words_restores_and_remembers_nothing() {
    let case = mtp_case("Field Station.kfx");
    let put = helper(
        r#"{"ok":false,"error":{"code":"copy-failed","message":"could not copy the book: the Kindle said it was unplugged or stopped answering"}}"#,
    );
    let cam = Recording::ok();
    let remember = |_: &str| panic!("a failed copy remembers nothing");
    let err = send_mtp(
        &case,
        &*put,
        Some(camera(Platform::Darwin, &case.config, cam.clone())),
        vec![mtp_kindle("Kindle Paperwhite", "G000INVENTED01")],
        None,
        &remember,
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::SendFailed);
    assert_eq!(
        err.message,
        "could not copy the book: the Kindle said it was unplugged or stopped answering"
    );
    assert_eq!(cam.calls().last().unwrap()[1], "enable");
}

#[test]
fn mtp_another_program_holding_it_is_named() {
    let case = mtp_case("Field Station.kfx");
    let put = helper(
        r#"{"ok":false,"error":{"code":"busy","message":"Calibre is using the Kindle. Close it and try again","holder":"Calibre"}}"#,
    );
    let remember = |_: &str| {};
    let err = send_mtp(
        &case,
        &*put,
        Some(camera(Platform::Darwin, &case.config, Recording::ok())),
        vec![mtp_kindle("K", "S")],
        None,
        &remember,
    )
    .unwrap_err();
    assert_eq!(
        err.message,
        "Calibre is using the Kindle. Close it and try again"
    );
}

#[test]
fn mtp_camera_import_that_will_not_let_go_even_when_paused_says_what_to_do() {
    let case = mtp_case("Field Station.kfx");
    let put = helper(
        r#"{"ok":false,"error":{"code":"busy","message":"macOS's camera import (ptpcamerad) is holding the Kindle","holder":"ptpcamerad"}}"#,
    );
    let remember = |_: &str| {};
    let err = send_mtp(
        &case,
        &*put,
        Some(camera(Platform::Darwin, &case.config, Recording::ok())),
        vec![mtp_kindle("K", "S")],
        None,
        &remember,
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::SendFailed);
    assert_eq!(
        err.message,
        "macOS's camera import would not let go of the Kindle. Unplug it, plug it in again, and try again"
    );
}

#[test]
fn mtp_an_epub_or_anything_a_kindle_cannot_open_is_refused_before_anything_runs() {
    for (name, words) in [
        (
            "Field Station.epub",
            "a Kindle does not read an EPUB copied to it. Send the Kindle file instead: method-reader export --for kindle makes it",
        ),
        (
            "notes.docx",
            "a Kindle cannot open .docx copied over USB. Send a KFX, AZW3, MOBI, PDF or TXT",
        ),
    ] {
        let case = mtp_case(name);
        let put = helper(DONE);
        let cam = Recording::ok();
        let remember = |_: &str| {};
        let err = send_mtp(
            &case,
            &*put,
            Some(camera(Platform::Darwin, &case.config, cam.clone())),
            vec![mtp_kindle("K", "S")],
            None,
            &remember,
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::UnsupportedFile);
        assert_eq!(err.message, words);
        assert!(put.calls().is_empty());
        assert!(cam.calls().is_empty());
    }
    assert_eq!(
        KINDLE_USB_TYPES,
        ["kfx", "azw3", "azw", "mobi", "prc", "pdf", "txt"]
    );
}

#[test]
fn mtp_every_kindle_file_the_ladder_makes_is_accepted() {
    for ext in ["kfx", "azw3", "mobi"] {
        let case = mtp_case(&format!("A.{ext}"));
        let put = helper(&format!(
            r#"{{"ok":true,"path":"documents/A.{ext}","bytes":3,"method":"rename"}}"#
        ));
        let remember = |_: &str| {};
        let result = send_mtp(
            &case,
            &*put,
            Some(camera(Platform::Linux, &case.config, Recording::ok())),
            vec![mtp_kindle("K", "S")],
            None,
            &remember,
        )
        .unwrap();
        assert_eq!(result.destination, Some(format!("documents/A.{ext}")));
    }
}

#[test]
fn mtp_two_kindles_are_two_ids_and_an_id_picks_one() {
    let case = mtp_case("Field Station.kfx");
    let both = vec![
        mtp_kindle("Kindle Paperwhite", "G000INVENTED01"),
        mtp_kindle("Kindle Scribe", "G000INVENTED02"),
    ];
    let put = helper(DONE);
    let remember = |_: &str| {};
    let err = send_mtp(&case, &*put, None, both.clone(), None, &remember).unwrap_err();
    assert_eq!(err.code, ErrorCode::AmbiguousDevice);
    assert!(err.message.contains("mtp:G000INVENTED01"));
    let sent = send_mtp(
        &case,
        &*put,
        None,
        both,
        Some("mtp:G000INVENTED02"),
        &remember,
    )
    .unwrap();
    assert_eq!(sent.device.name, "Kindle Scribe");
    assert_eq!(put.calls()[0][2], "G000INVENTED02");
}
