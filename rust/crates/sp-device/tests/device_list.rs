//! tests/device-list.test.ts, ported, with branch mtp-kindle's listing
//! checks from tests/device-mtp.test.ts ("listDevices with MTP").

mod common;
use common::{Stub, s, scratch};
use sp_device::list::{ListDevicesOptions, list_devices};
use sp_device::{ConnectedDevice, DeviceKind, Transport};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

fn mount_root_with_kobo(scratch: &Path) -> String {
    let root = tempfile::Builder::new()
        .prefix("mounts-")
        .tempdir_in(scratch)
        .unwrap()
        .keep();
    std::fs::create_dir_all(root.join("KOBOeReader").join(".kobo")).unwrap();
    s(&root)
}

fn kinds(devices: &[ConnectedDevice]) -> Vec<&'static str> {
    devices.iter().map(|d| d.kind.as_str()).collect()
}

#[test]
fn mounted_vendors_are_listed_when_the_tablet_does_not_answer() {
    let sc = scratch("method-reader-device-list-");
    let silent = Stub::start(404);
    let devices = list_devices(&ListDevicesOptions {
        roots: Some(vec![mount_root_with_kobo(sc.path())]),
        remarkable_endpoint: Some(silent.url.clone()),
        skip_mtp: true,
        ..Default::default()
    });
    assert_eq!(kinds(&devices), ["kobo"]);
    assert!(
        devices[0]
            .volume
            .as_deref()
            .unwrap()
            .ends_with("KOBOeReader")
    );
}

#[test]
fn a_remarkable_that_answers_is_appended_after_the_mounted_devices() {
    let sc = scratch("method-reader-device-list-");
    let answering = Stub::start(200);
    let devices = list_devices(&ListDevicesOptions {
        roots: Some(vec![mount_root_with_kobo(sc.path())]),
        remarkable_endpoint: Some(answering.url.clone()),
        skip_mtp: true,
        ..Default::default()
    });
    // Order is asserted with a mounted device PRESENT, so an implementation
    // that prepended the tablet fails rather than passing on an empty list.
    assert_eq!(kinds(&devices), ["kobo", "remarkable"]);
    assert_eq!(devices[1].volume, None);
    assert_eq!(devices[1].name, "reMarkable");
}

#[test]
fn nothing_connected_is_an_empty_list_not_an_error() {
    let sc = scratch("method-reader-device-list-");
    let silent = Stub::start(404);
    let empty = tempfile::Builder::new()
        .prefix("mounts-")
        .tempdir_in(sc.path())
        .unwrap();
    let devices = list_devices(&ListDevicesOptions {
        roots: Some(vec![s(empty.path())]),
        remarkable_endpoint: Some(silent.url.clone()),
        skip_mtp: true,
        ..Default::default()
    });
    assert!(devices.is_empty());
}

#[test]
fn the_mount_scan_and_the_probe_run_concurrently_not_one_after_the_other() {
    // Each seam records when it STARTED; sequential running would put the
    // probe's start 200 ms after the scan's.
    let scan_started = Mutex::new(None::<Instant>);
    let probe_started = Mutex::new(None::<Instant>);
    let scan = |_: Option<&[String]>| {
        *scan_started.lock().unwrap() = Some(Instant::now());
        std::thread::sleep(Duration::from_millis(200));
        vec![ConnectedDevice::mounted(
            DeviceKind::Kindle,
            "Kindle",
            "/v/Kindle",
        )]
    };
    let probe = |_: &str, _: Duration| {
        *probe_started.lock().unwrap() = Some(Instant::now());
        std::thread::sleep(Duration::from_millis(200));
        Ok(true)
    };
    let devices = list_devices(&ListDevicesOptions {
        scan: Some(&scan),
        probe: Some(&probe),
        skip_mtp: true,
        ..Default::default()
    });
    assert_eq!(kinds(&devices), ["kindle", "remarkable"]);
    let (a, b) = (
        scan_started.lock().unwrap().unwrap(),
        probe_started.lock().unwrap().unwrap(),
    );
    let apart = if a > b { a - b } else { b - a };
    assert!(apart < Duration::from_millis(20), "{apart:?} apart");
}

#[test]
fn a_probe_that_throws_is_a_tablet_that_is_not_there() {
    let scan = |_: Option<&[String]>| Vec::new();
    let probe = |_: &str, _: Duration| Err("ECONNREFUSED".to_string());
    let devices = list_devices(&ListDevicesOptions {
        scan: Some(&scan),
        probe: Some(&probe),
        skip_mtp: true,
        ..Default::default()
    });
    assert!(devices.is_empty());
}

#[test]
fn skip_remarkable_lists_what_the_mount_scan_sees_and_never_starts_the_probe() {
    // The probe here would never answer, so a listing that still waited for
    // it would hang.
    let probes = Mutex::new(0);
    let scan = |_: Option<&[String]>| {
        vec![ConnectedDevice::mounted(
            DeviceKind::Kindle,
            "Kindle",
            "/v/Kindle",
        )]
    };
    let probe = |_: &str, _: Duration| {
        *probes.lock().unwrap() += 1;
        loop {
            std::thread::sleep(Duration::from_secs(3600));
        }
    };
    let devices = list_devices(&ListDevicesOptions {
        scan: Some(&scan),
        probe: Some(&probe),
        skip_remarkable: true,
        skip_mtp: true,
        ..Default::default()
    });
    assert_eq!(kinds(&devices), ["kindle"]);
    assert_eq!(*probes.lock().unwrap(), 0);
}

// From branch mtp-kindle: listDevices with MTP.

fn kobo() -> ConnectedDevice {
    ConnectedDevice::mounted(DeviceKind::Kobo, "KOBOeReader", "/Volumes/KOBOeReader")
}

fn mtp_kindle() -> ConnectedDevice {
    ConnectedDevice {
        kind: DeviceKind::Kindle,
        name: "Kindle".into(),
        volume: None,
        transport: Some(Transport::Mtp),
        serial: Some("S1".into()),
    }
}

#[test]
fn mtp_kindles_come_after_the_mounted_readers_with_quick_too() {
    let scan = |_: Option<&[String]>| vec![kobo()];
    let mtp = || Ok(vec![mtp_kindle()]);
    let quick = list_devices(&ListDevicesOptions {
        scan: Some(&scan),
        mtp: Some(&mtp),
        skip_remarkable: true,
        ..Default::default()
    });
    assert_eq!(quick, [kobo(), mtp_kindle()]);
    let probe = |_: &str, _: Duration| Ok(false);
    let full = list_devices(&ListDevicesOptions {
        scan: Some(&scan),
        mtp: Some(&mtp),
        probe: Some(&probe),
        ..Default::default()
    });
    assert_eq!(full, [kobo(), mtp_kindle()]);
}

#[test]
fn skip_mtp_never_asks_and_a_listing_that_throws_is_no_mtp_devices() {
    let asked = Mutex::new(false);
    let scan = |_: Option<&[String]>| vec![kobo()];
    let mtp = || {
        *asked.lock().unwrap() = true;
        Ok(vec![mtp_kindle()])
    };
    let skipped = list_devices(&ListDevicesOptions {
        scan: Some(&scan),
        mtp: Some(&mtp),
        skip_mtp: true,
        skip_remarkable: true,
        ..Default::default()
    });
    assert_eq!(skipped, [kobo()]);
    assert!(!*asked.lock().unwrap());
    let throws = || Err("boom".to_string());
    let thrown = list_devices(&ListDevicesOptions {
        scan: Some(&scan),
        mtp: Some(&throws),
        skip_remarkable: true,
        ..Default::default()
    });
    assert_eq!(thrown, [kobo()]);
}

#[test]
fn the_default_with_mtp_unshipped_and_no_helper_named_adds_nothing() {
    // MTP is off for users: with no METHOD_READER_MTP_HELPER the helper is never
    // even looked for.
    let scan = |_: Option<&[String]>| Vec::new();
    if std::env::var("METHOD_READER_MTP_HELPER").is_err() {
        assert!(
            list_devices(&ListDevicesOptions {
                scan: Some(&scan),
                skip_remarkable: true,
                ..Default::default()
            })
            .is_empty()
        );
    }
}
