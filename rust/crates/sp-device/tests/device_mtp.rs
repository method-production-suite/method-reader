//! Branch mtp-kindle's tests/device-mtp.test.ts, ported (BETA, off). The
//! engine's side of the MTP helper: finding it, reading its answers, and
//! the listing that never fails. Every helper here is a fake (an injected
//! runner, or a shell script in a scratch folder): no test lists or writes
//! to real USB. The listing checks are in device_list.rs.
//!
//! The TypeScript's "test guard" block pinned root .env.test's
//! METHOD_READER_MTP_HELPER / METHOD_READER_LAUNCHCTL / METHOD_READER_PKILL guard paths.
//! The Rust side needs no guard file: MTP is off unless METHOD_READER_MTP_HELPER
//! names a helper (mtp_enabled), and every runner here is injected.

mod common;
use common::{Recording, exit, strings};
use sp_device::mtp::{
    HelperLocation, MTP_HELPER_NAME, MTP_LIST_TIMEOUT, MtpPutAnswer, PutMethod, kindle_device_path,
    list_mtp_kindles, mtp_enabled, mtp_helper_path, parse_mtp_list, parse_put_answer,
    put_to_mtp_kindle,
};
use sp_device::process::{RunOutput, Runner};
use sp_device::{ConnectedDevice, DeviceKind, Platform, Transport, device_id};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

fn kindle_json() -> serde_json::Value {
    serde_json::json!({
        "vendorId": 6473, "productId": 39297, "vendorIdHex": "0x1949", "productIdHex": "0x9981",
        "serial": "G000INVENTED01", "manufacturer": "Amazon", "model": "Kindle Paperwhite", "kindle": true, "storages": null,
    })
}

fn listing(devices: Vec<serde_json::Value>) -> String {
    serde_json::json!({ "ok": true, "devices": devices }).to_string()
}

fn with(mut v: serde_json::Value, key: &str, value: serde_json::Value) -> serde_json::Value {
    v[key] = value;
    v
}

/// A runner that records argv and timeout.
struct Timed {
    calls: Mutex<Vec<(Vec<String>, Duration)>>,
    answer: RunOutput,
}

impl Runner for Timed {
    fn run(&self, argv: &[String], timeout: Duration) -> Result<RunOutput, String> {
        self.calls.lock().unwrap().push((argv.to_vec(), timeout));
        Ok(self.answer.clone())
    }
}

fn timed(answer: RunOutput) -> Timed {
    Timed {
        calls: Mutex::new(Vec::new()),
        answer,
    }
}

// mtpHelperPath

fn location<'a>(
    env: &'a dyn Fn(&str) -> Option<String>,
    exec: &str,
    platform: Platform,
    exists: &'a dyn Fn(&str) -> bool,
) -> HelperLocation<'a> {
    HelperLocation {
        env,
        exec_path: PathBuf::from(exec),
        platform,
        exists,
        dev_target_dir: Some(PathBuf::from("/repo/rust/target/release")),
    }
}

#[test]
fn a_shipped_build_never_looks_in_the_build_machines_folders() {
    // The dev fallback is a path baked in at compile time. In a release
    // built on a Windows runner it is D:\a\..., and D: on a person's
    // machine is often a removable drive that could carry that path.
    let host = sp_device::mtp::HelperLocation::host();
    assert_eq!(host.dev_target_dir.is_some(), cfg!(debug_assertions));
}

#[test]
fn the_override_wins_but_only_when_it_is_there() {
    let env = |k: &str| (k == "METHOD_READER_MTP_HELPER").then(|| "/x/helper".to_string());
    let there = |p: &str| p == "/x/helper";
    let none = |_: &str| false;
    assert_eq!(
        mtp_helper_path(&location(&env, "/a/b", Platform::Darwin, &there)),
        Some("/x/helper".into())
    );
    assert_eq!(
        mtp_helper_path(&location(&env, "/a/b", Platform::Darwin, &none)),
        None
    );
}

#[test]
fn beside_the_running_program_where_the_bundled_app_keeps_it() {
    let at = "/Applications/Method Reader.app/Contents/MacOS/method-reader-mtp";
    let env = |_: &str| None;
    let exists = |p: &str| p == at;
    let loc = location(
        &env,
        "/Applications/Method Reader.app/Contents/MacOS/method-reader",
        Platform::Darwin,
        &exists,
    );
    assert_eq!(mtp_helper_path(&loc), Some(at.into()));
}

#[test]
fn on_windows_it_is_the_exe() {
    let found = Mutex::new(Vec::new());
    let env = |_: &str| None;
    let exists = |p: &str| {
        found.lock().unwrap().push(p.to_string());
        false
    };
    mtp_helper_path(&location(
        &env,
        "C:\\Method Reader\\method-reader.exe",
        Platform::Win32,
        &exists,
    ));
    let found = found.lock().unwrap();
    assert!(
        found
            .iter()
            .any(|p| p.ends_with(&format!("{MTP_HELPER_NAME}.exe"))),
        "{found:?}"
    );
    assert!(
        found.contains(&"C:\\Method Reader\\method-reader-mtp.exe".to_string()),
        "{found:?}"
    );
}

#[test]
fn a_dev_checkout_finds_its_own_release_build() {
    let dev = sp_fsutil::nodepath::host::join(&["/repo/rust/target/release", "method-reader-mtp"]);
    let env = |_: &str| None;
    let d = dev.clone();
    let exists = move |p: &str| p == d;
    assert_eq!(
        mtp_helper_path(&location(
            &env,
            "/usr/local/bin/cargo",
            Platform::Linux,
            &exists
        )),
        Some(dev)
    );
}

#[test]
fn none_anywhere_is_none() {
    let env = |_: &str| None;
    let none = |_: &str| false;
    assert_eq!(
        mtp_helper_path(&location(&env, "/usr/local/bin/x", Platform::Darwin, &none)),
        None
    );
}

#[test]
fn mtp_is_off_unless_a_helper_is_named() {
    assert!(!mtp_enabled(&|_| None));
    assert!(!mtp_enabled(&|_| Some("  ".into())));
    assert!(mtp_enabled(
        &|k| (k == "METHOD_READER_MTP_HELPER").then(|| "/x/method-reader-mtp".into())
    ));
}

// parseMtpList

fn mtp_kindle(name: &str, serial: &str) -> ConnectedDevice {
    ConnectedDevice {
        kind: DeviceKind::Kindle,
        name: name.into(),
        volume: None,
        transport: Some(Transport::Mtp),
        serial: Some(serial.into()),
    }
}

#[test]
fn a_kindle_becomes_a_kindle_device_with_no_volume_addressed_by_serial() {
    let devices = parse_mtp_list(&listing(vec![kindle_json()]));
    assert_eq!(devices, [mtp_kindle("Kindle Paperwhite", "G000INVENTED01")]);
    assert_eq!(device_id(&devices[0]), "mtp:G000INVENTED01");
    assert_eq!(
        serde_json::to_value(&devices[0]).unwrap(),
        serde_json::json!({ "kind": "kindle", "name": "Kindle Paperwhite", "volume": null, "transport": "mtp", "serial": "G000INVENTED01" })
    );
}

#[test]
fn anything_not_a_kindle_or_with_no_serial_is_left_out() {
    let phone = with(
        with(
            with(
                with(kindle_json(), "vendorId", 6353.into()),
                "serial",
                "PHONE".into(),
            ),
            "model",
            "Pixel".into(),
        ),
        "kindle",
        false.into(),
    );
    let no_serial = with(kindle_json(), "serial", serde_json::Value::Null);
    let blank = with(kindle_json(), "serial", "  ".into());
    assert!(parse_mtp_list(&listing(vec![phone, no_serial, blank])).is_empty());
}

#[test]
fn a_kindle_with_no_model_string_is_called_kindle() {
    let devices = parse_mtp_list(&listing(vec![with(
        kindle_json(),
        "model",
        serde_json::Value::Null,
    )]));
    assert_eq!(devices[0].name, "Kindle");
}

#[test]
fn the_same_serial_twice_is_one_device() {
    assert_eq!(
        parse_mtp_list(&listing(vec![kindle_json(), kindle_json()])).len(),
        1
    );
}

#[test]
fn anything_that_is_not_the_contract_is_no_devices() {
    for out in [
        "",
        "not json",
        "[]",
        r#"{"ok":false,"error":{"code":"internal","message":"x"}}"#,
        r#"{"ok":true}"#,
        "null",
    ] {
        assert!(parse_mtp_list(out).is_empty(), "{out}");
    }
}

// listMtpKindles

#[test]
fn asks_for_kindles_only_from_descriptors_under_a_short_timeout() {
    let run = timed(exit(0, &listing(vec![kindle_json()]), ""));
    let devices = list_mtp_kindles(Some("/h/method-reader-mtp"), &run, MTP_LIST_TIMEOUT);
    assert_eq!(devices.len(), 1);
    assert_eq!(
        *run.calls.lock().unwrap(),
        [(
            strings(&["/h/method-reader-mtp", "list", "--kindles"]),
            Duration::from_millis(3000)
        )]
    );
}

#[test]
fn no_helper_a_crash_a_timeout_or_a_runner_that_fails_is_an_empty_list() {
    assert!(list_mtp_kindles(None, &timed(exit(0, "", "")), MTP_LIST_TIMEOUT).is_empty());
    assert!(list_mtp_kindles(Some("/h"), &timed(exit(1, "", "")), MTP_LIST_TIMEOUT).is_empty());
    assert!(
        list_mtp_kindles(Some("/h"), &timed(RunOutput::default()), MTP_LIST_TIMEOUT).is_empty()
    );
    let fails = Recording::new(|_| Err("ENOENT".into()));
    assert!(list_mtp_kindles(Some("/h"), &*fails, MTP_LIST_TIMEOUT).is_empty());
}

#[cfg(unix)]
#[test]
fn a_real_spawn_of_a_fake_helper_script() {
    use common::scratch;
    use sp_device::process::SpawnRunner;
    use std::os::unix::fs::PermissionsExt;
    let sc = scratch("method-reader-mtp-");
    let helper = sc.path().join("list-helper");
    std::fs::write(
        &helper,
        format!(
            "#!/bin/sh\nprintf '%s\\n' '{}'\n",
            listing(vec![kindle_json()])
        ),
    )
    .unwrap();
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let devices = list_mtp_kindles(Some(&common::s(&helper)), &SpawnRunner, MTP_LIST_TIMEOUT);
    assert_eq!(
        devices.iter().map(device_id).collect::<Vec<_>>(),
        ["mtp:G000INVENTED01"]
    );
}

// put

#[test]
fn the_book_goes_to_documents_under_its_own_name() {
    assert_eq!(
        kindle_device_path("/lib/Field Station.kfx"),
        "documents/Field Station.kfx"
    );
}

#[test]
fn argv_serial_file_device_path_and_wait_ms_only_when_asked() {
    let ok = r#"{"ok":true,"path":"documents/A.kfx","bytes":3,"method":"rename"}"#;
    let a = timed(exit(0, ok, ""));
    assert_eq!(
        put_to_mtp_kindle(
            Some("/h"),
            &a,
            "S1",
            "/lib/A.kfx",
            None,
            Duration::from_secs(600)
        ),
        MtpPutAnswer::Done {
            path: "documents/A.kfx".into(),
            bytes: 3,
            method: PutMethod::Rename
        }
    );
    assert_eq!(
        a.calls.lock().unwrap()[0].0,
        strings(&["/h", "put", "S1", "/lib/A.kfx", "documents/A.kfx"])
    );
    let b = timed(exit(0, ok, ""));
    put_to_mtp_kindle(
        Some("/h"),
        &b,
        "S1",
        "/lib/A.kfx",
        Some(Duration::from_secs(5)),
        Duration::from_secs(600),
    );
    assert_eq!(
        b.calls.lock().unwrap()[0].0,
        strings(&[
            "/h",
            "put",
            "S1",
            "/lib/A.kfx",
            "documents/A.kfx",
            "--wait-ms",
            "5000"
        ])
    );
}

#[test]
fn a_refusal_comes_back_in_the_helper_s_own_code_and_words_with_its_holder() {
    let busy = r#"{"ok":false,"error":{"code":"busy","message":"Calibre is using the Kindle. Close it and try again","holder":"calibre"}}"#;
    assert_eq!(
        put_to_mtp_kindle(
            Some("/h"),
            &timed(exit(1, busy, "")),
            "S1",
            "/lib/A.kfx",
            None,
            Duration::from_secs(1)
        ),
        MtpPutAnswer::Failed {
            code: "busy".into(),
            message: "Calibre is using the Kindle. Close it and try again".into(),
            holder: Some("calibre".into()),
        }
    );
}

fn code_of(answer: MtpPutAnswer) -> String {
    match answer {
        MtpPutAnswer::Failed { code, .. } => code,
        MtpPutAnswer::Done { .. } => "done".into(),
    }
}

#[test]
fn no_helper_no_answer_a_timeout_and_a_helper_that_will_not_start() {
    let t = Duration::from_secs(1);
    assert_eq!(
        code_of(put_to_mtp_kindle(
            None,
            &timed(exit(0, "", "")),
            "S1",
            "/a.kfx",
            None,
            t
        )),
        "helper-missing"
    );
    assert_eq!(
        code_of(put_to_mtp_kindle(
            Some("/h"),
            &timed(exit(1, "garbage", "")),
            "S1",
            "/a.kfx",
            None,
            t
        )),
        "helper-failed"
    );
    assert_eq!(
        code_of(put_to_mtp_kindle(
            Some("/h"),
            &timed(RunOutput::default()),
            "S1",
            "/a.kfx",
            None,
            t
        )),
        "timeout"
    );
    let fails = Recording::new(|_| Err("EACCES".into()));
    assert_eq!(
        code_of(put_to_mtp_kindle(
            Some("/h"),
            &*fails,
            "S1",
            "/a.kfx",
            None,
            t
        )),
        "helper-missing"
    );
}

#[test]
fn parse_put_answer_reads_the_replace_method_too() {
    let answer =
        parse_put_answer(r#"{"ok":true,"path":"documents/A.kfx","bytes":1,"method":"replace"}"#);
    assert!(matches!(
        answer,
        MtpPutAnswer::Done {
            method: PutMethod::Replace,
            ..
        }
    ));
}

#[test]
fn the_helper_name_is_sp_mtp_s_binary() {
    let manifest = include_str!("../../sp-mtp/Cargo.toml");
    assert!(manifest.contains(&format!("name = \"{MTP_HELPER_NAME}\"")));
}
