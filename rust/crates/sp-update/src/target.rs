//! Which `platforms` key a build answers to: tauri-plugin-updater 2.12.0's
//! `updater_os`, `updater_arch` and `get_urls`, which try
//! `<os>-<arch>-<installer>` first and then `<os>-<arch>`.

use crate::manifest::{Platform, Release};

/// How the running copy was installed, as the plugin names it
/// (`Installer::name`). Method Reader's Mac app is `app`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Installer {
    AppImage,
    Deb,
    Rpm,
    App,
    Msi,
    Nsis,
}

impl Installer {
    pub fn name(self) -> &'static str {
        match self {
            Self::AppImage => "appimage",
            Self::Deb => "deb",
            Self::Rpm => "rpm",
            Self::App => "app",
            Self::Msi => "msi",
            Self::Nsis => "nsis",
        }
    }
}

/// The plugin's OS word for this build: `darwin`, not `macos`.
pub fn updater_os() -> Option<&'static str> {
    if cfg!(target_os = "linux") {
        Some("linux")
    } else if cfg!(target_os = "macos") {
        Some("darwin")
    } else if cfg!(target_os = "windows") {
        Some("windows")
    } else {
        None
    }
}

/// The plugin's architecture word for this build.
pub fn updater_arch() -> Option<&'static str> {
    if cfg!(target_arch = "x86") {
        Some("i686")
    } else if cfg!(target_arch = "x86_64") {
        Some("x86_64")
    } else if cfg!(target_arch = "arm") {
        Some("armv7")
    } else if cfg!(target_arch = "aarch64") {
        Some("aarch64")
    } else if cfg!(target_arch = "riscv64") {
        Some("riscv64")
    } else {
        None
    }
}

/// The keys one build looks for, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetSet {
    pub os: String,
    pub arch: String,
    pub keys: Vec<String>,
}

impl TargetSet {
    pub fn new(os: &str, arch: &str, installer: Option<Installer>) -> Self {
        let mut keys = Vec::new();
        if let Some(i) = installer {
            keys.push(format!("{os}-{arch}-{}", i.name()));
        }
        keys.push(format!("{os}-{arch}"));
        Self {
            os: os.into(),
            arch: arch.into(),
            keys,
        }
    }

    /// This build, installed as `installer`.
    pub fn current(installer: Option<Installer>) -> Result<Self, String> {
        let os = updater_os()
            .ok_or("Unsupported OS, expected one of `linux`, `darwin` or `windows`.")?;
        let arch = updater_arch().ok_or(
            "Unsupported application architecture, expected one of `x86`, `x86_64`, `arm` or `aarch64`.",
        )?;
        Ok(Self::new(os, arch, installer))
    }

    /// `get_urls`: the first key the manifest has, or the plugin's
    /// `TargetsNotFound` words, which sp-app reads to tell an Intel Mac why
    /// it is offered nothing (`model::update::is_intel_mac_retired`).
    pub fn pick<'a>(&self, release: &'a Release) -> Result<&'a Platform, String> {
        for key in &self.keys {
            if let Ok(p) = release.platform(key) {
                return Ok(p);
            }
        }
        Err(format!(
            "None of the fallback platforms `{:?}` were found in the response `platforms` object",
            self.keys
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(keys: &[&str]) -> Release {
        let platforms: serde_json::Map<String, serde_json::Value> = keys
            .iter()
            .map(|k| {
                (
                    k.to_string(),
                    serde_json::json!({"url": format!("https://x/{k}"), "signature": "s"}),
                )
            })
            .collect();
        Release::from_value(serde_json::json!({"version": "1.0.0", "platforms": platforms}))
            .unwrap()
    }

    #[test]
    fn the_installer_key_comes_first_then_the_plain_one() {
        let t = TargetSet::new("darwin", "aarch64", Some(Installer::App));
        assert_eq!(t.keys, ["darwin-aarch64-app", "darwin-aarch64"]);
        let both = release(&["darwin-aarch64", "darwin-aarch64-app"]);
        assert_eq!(t.pick(&both).unwrap().url, "https://x/darwin-aarch64-app");
        let plain = release(&["darwin-aarch64"]);
        assert_eq!(t.pick(&plain).unwrap().url, "https://x/darwin-aarch64");
    }

    #[test]
    fn an_intel_mac_gets_the_words_sp_app_recognises() {
        let t = TargetSet::new("darwin", "x86_64", Some(Installer::App));
        let msg = t.pick(&release(&["darwin-aarch64"])).unwrap_err();
        assert_eq!(
            msg,
            "None of the fallback platforms `[\"darwin-x86_64-app\", \"darwin-x86_64\"]` were found in the response `platforms` object"
        );
    }

    #[test]
    fn this_build_has_a_key() {
        let t = TargetSet::current(None).unwrap();
        assert_eq!(t.keys.len(), 1);
        assert!(t.keys[0].contains('-'));
    }
}
