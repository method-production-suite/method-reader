//! src/device/volumes.ts: the ONLY part of the device layer that cannot be
//! tested against reality, because what is mounted depends on what is
//! plugged in. Kept deliberately dumb, with injectable roots, so that the
//! device knowledge lives in [`crate::classify`].

use crate::classify::{classify, volume_name};
use crate::paths::{is_directory, is_windows_drive_root};
use crate::process::Platform;
use crate::types::ConnectedDevice;
use sp_fsutil::nodepath::host;

/// Where this OS mounts removable media. On Windows these are drive roots,
/// each itself a volume; C: is skipped, it is the system disk. On Linux the
/// person's own udisks folders come first, then `/media`.
pub fn volume_roots(platform: Platform) -> Vec<String> {
    match platform {
        Platform::Darwin => vec!["/Volumes".into()],
        Platform::Win32 => ('D'..='Z').map(|letter| format!("{letter}:\\")).collect(),
        Platform::Linux => {
            let user = username();
            vec![
                format!("/run/media/{user}"),
                format!("/media/{user}"),
                "/media".into(),
            ]
        }
    }
}

/// `os.userInfo().username`: the passwd entry for this process's uid, else
/// the login name the environment gives.
fn username() -> String {
    #[cfg(unix)]
    {
        let uid = rustix::process::getuid().as_raw();
        if let Ok(passwd) = std::fs::read_to_string("/etc/passwd") {
            for line in passwd.lines() {
                let fields: Vec<&str> = line.split(':').collect();
                if fields.len() > 2 && fields[2].parse::<u32>().ok() == Some(uid) {
                    return fields[0].to_string();
                }
            }
        }
    }
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default()
}

/// True when `root` is itself a mountable volume rather than a folder that
/// CONTAINS volumes: the Windows drive-letter case. The platform is an
/// argument so the branch is testable everywhere.
pub fn root_is_itself_a_volume(root: &str, platform: Platform) -> bool {
    platform == Platform::Win32 && is_windows_drive_root(root)
}

/// Every mounted volume path under the given roots (default: this
/// platform's). On Windows a root IS a volume; elsewhere a root CONTAINS
/// volumes. Entries come in the order the system lists them, as Bun's
/// readdirSync gives them (unsorted).
pub fn enumerate_volumes(roots: Option<&[String]>) -> Vec<String> {
    let defaults;
    let roots = match roots {
        Some(r) => r,
        None => {
            defaults = volume_roots(Platform::host());
            &defaults
        }
    };
    let mut found = Vec::new();
    for root in roots {
        if !is_directory(root) {
            continue;
        }
        if root_is_itself_a_volume(root, Platform::host()) {
            found.push(root.clone());
            continue;
        }
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = host::join(&[root, &name]);
            if is_directory(&path) {
                found.push(path);
            }
        }
    }
    found
}

/// All recognized readers among the mounted volumes.
pub fn mounted_devices(roots: Option<&[String]>) -> Vec<ConnectedDevice> {
    enumerate_volumes(roots)
        .into_iter()
        .filter_map(|volume| {
            classify(&volume)
                .map(|kind| ConnectedDevice::mounted(kind, volume_name(&volume), volume))
        })
        .collect()
}
