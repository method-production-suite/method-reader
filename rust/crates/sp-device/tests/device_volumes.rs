//! tests/device-volumes.test.ts, ported. Enumeration is tested by injecting
//! roots, never against whatever is really plugged into this machine.

mod common;
use common::{join, s, scratch};
use sp_device::DeviceKind;
use sp_device::Platform;
use sp_device::volumes::{
    enumerate_volumes, mounted_devices, root_is_itself_a_volume, volume_roots,
};
use std::path::Path;

fn mount_root(scratch: &Path, volumes: &[(&str, &[&str])]) -> String {
    let root = tempfile::Builder::new()
        .prefix("mounts-")
        .tempdir_in(scratch)
        .unwrap()
        .keep();
    for (name, subdirs) in volumes {
        std::fs::create_dir_all(root.join(name)).unwrap();
        for sub in *subdirs {
            std::fs::create_dir_all(root.join(name).join(sub)).unwrap();
        }
    }
    s(&root)
}

fn base(p: &str) -> String {
    Path::new(p)
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned()
}

#[test]
fn enumerate_lists_the_directories_under_an_injected_root() {
    let sc = scratch("method-reader-device-volumes-");
    let root = mount_root(
        sc.path(),
        &[
            ("Kindle", &["documents"]),
            ("KOBOeReader", &[".kobo"]),
            ("USB STICK", &[]),
        ],
    );
    let mut found: Vec<String> = enumerate_volumes(Some(&[root]))
        .iter()
        .map(|p| base(p))
        .collect();
    found.sort();
    assert_eq!(found, ["KOBOeReader", "Kindle", "USB STICK"]);
}

#[test]
fn enumerate_ignores_files_and_missing_roots() {
    let sc = scratch("method-reader-device-volumes-");
    let root = mount_root(sc.path(), &[("Kindle", &["documents"])]);
    std::fs::write(join(&root, "loose-file.txt"), "x").unwrap();
    let found = enumerate_volumes(Some(&[root, join(&s(sc.path()), "no-such-root")]));
    assert_eq!(
        found.iter().map(|p| base(p)).collect::<Vec<_>>(),
        ["Kindle"]
    );
}

#[test]
fn mounted_devices_classifies_what_it_finds_and_drops_plain_drives() {
    let sc = scratch("method-reader-device-volumes-");
    let root = mount_root(
        sc.path(),
        &[
            ("Kindle", &["documents"]),
            ("KOBOeReader", &[".kobo"]),
            ("USB STICK", &["documents"]),
        ],
    );
    let devices = mounted_devices(Some(&[root]));
    let mut kinds: Vec<&str> = devices.iter().map(|d| d.kind.as_str()).collect();
    kinds.sort();
    assert_eq!(kinds, ["kindle", "kobo"]);
    assert_eq!(
        devices
            .iter()
            .find(|d| d.kind == DeviceKind::Kobo)
            .unwrap()
            .name,
        "KOBOeReader"
    );
    assert!(devices.iter().all(|d| d.volume.is_some()));
}

#[test]
fn mounted_devices_on_an_empty_root_finds_nothing() {
    let sc = scratch("method-reader-device-volumes-");
    assert!(mounted_devices(Some(&[mount_root(sc.path(), &[])])).is_empty());
}

#[test]
fn the_default_roots_are_the_right_ones_for_each_platform() {
    assert_eq!(volume_roots(Platform::Darwin), ["/Volumes"]);
    let linux = volume_roots(Platform::Linux);
    assert!(linux.iter().any(|r| r.starts_with("/run/media")));
    assert!(linux.contains(&"/media".to_string()));
    let win = volume_roots(Platform::Win32);
    assert!(win.iter().all(|r| r.len() == 3 && r.ends_with(":\\")));
    assert_eq!(
        win.first().map(String::as_str),
        Some("D:\\"),
        "C: is the system disk and is skipped"
    );
    assert_eq!(win.len(), 23);
}

#[test]
fn a_drive_root_is_itself_a_volume_only_on_win32() {
    assert!(root_is_itself_a_volume("D:\\", Platform::Win32));
}

#[test]
fn a_lowercase_drive_root_is_recognised_too() {
    assert!(root_is_itself_a_volume("d:\\", Platform::Win32));
}

#[test]
fn a_drive_root_shaped_string_is_not_itself_a_volume_off_windows() {
    assert!(!root_is_itself_a_volume("D:\\", Platform::Linux));
}

#[test]
fn a_non_drive_root_path_is_never_itself_a_volume_even_on_win32() {
    assert!(!root_is_itself_a_volume("/Volumes", Platform::Darwin));
    assert!(!root_is_itself_a_volume("/Volumes", Platform::Win32));
}
