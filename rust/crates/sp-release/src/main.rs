//! `sp-release <command> [--flag value]...`: see USAGE.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use sp_release::{archive, check_latest, linux, mac, manifest, names, nsis, serve, sign, version};

const USAGE: &str = "sp-release: Method Reader's release tools

  version-check   --version V [--cargo-toml rust/Cargo.toml]
  mac-app         --version V --window BIN --icon ICNS [--helper BIN]... [--resource FILE]...
                  [--identifier ID] --out DIR
  update-archive  --app \"Method Reader.app\" --out Method-Reader-macOS.app.tar.gz
  sign-update     --file F --version V      (key: TAURI_SIGNING_PRIVATE_KEY, its password:
                                             TAURI_SIGNING_PRIVATE_KEY_PASSWORD; writes F.sig)
  keygen          --out KEYFILE [--password P]   (THROWAWAY keys: rehearsals and tests)
  manifest        --dir ARRIVALS --version V --out latest.json [--repo OWNER/NAME]
                  [--asset-base URL]   (rehearsal only: files served from URL/<name>)
  verify-update   --dir ARRIVALS --manifest latest.json [--pubkey BASE64]
  cli-archive     --binary BIN [--alias BIN] --kind macos-arm64|linux-x64|linux-arm64|windows-x64
                  --out DIR   (--alias: the `screepub` binary, packed beside method-reader)
  deb | rpm       --version V --window BIN --icon SIZE=PNG... --resource FILE... --arch A
                  [--depends LINE] --out DIR
  appdir          --version V --window BIN --icon SIZE=PNG... --resource FILE... --out DIR
  nsis            --version V --src DIR --setup OUT.exe --script OUT.nsi
  check-latest    [--version V] [--repo OWNER/NAME] [--pubkey BASE64] [--deep]
  serve           --dir DIR [--port N]
";

struct Args {
    values: BTreeMap<String, Vec<String>>,
    flags: Vec<String>,
}

impl Args {
    fn parse(argv: &[String]) -> Result<Self, String> {
        let mut values: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut flags = Vec::new();
        let mut i = 0;
        while i < argv.len() {
            let a = &argv[i];
            let Some(key) = a.strip_prefix("--") else {
                return Err(format!("unexpected argument {a:?}"));
            };
            if key == "deep" {
                flags.push(key.to_string());
                i += 1;
                continue;
            }
            let v = argv
                .get(i + 1)
                .ok_or_else(|| format!("--{key} needs a value"))?;
            values.entry(key.into()).or_default().push(v.clone());
            i += 2;
        }
        Ok(Self { values, flags })
    }
    fn one(&self, k: &str) -> Result<String, String> {
        self.opt(k).ok_or_else(|| format!("--{k} is required"))
    }
    fn opt(&self, k: &str) -> Option<String> {
        self.values.get(k).and_then(|v| v.last().cloned())
    }
    fn many(&self, k: &str) -> Vec<String> {
        self.values.get(k).cloned().unwrap_or_default()
    }
    fn path(&self, k: &str) -> Result<PathBuf, String> {
        self.one(k).map(PathBuf::from)
    }
    fn version(&self) -> Result<String, String> {
        let v = self.one("version")?;
        let v = v.trim_start_matches('v').to_string();
        if !names::is_release_version(&v) {
            return Err(format!(
                "--version must be MAJOR.MINOR.PATCH (got {v:?}); a branch name must never reach a file name"
            ));
        }
        Ok(v)
    }
    fn icons(&self) -> Result<Vec<(String, PathBuf)>, String> {
        self.many("icon")
            .iter()
            .map(|s| {
                s.split_once('=')
                    .map(|(a, b)| (a.to_string(), PathBuf::from(b)))
                    .ok_or_else(|| format!("--icon wants SIZE=PNG, got {s:?}"))
            })
            .collect()
    }
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    println!("wrote {} ({} bytes)", path.display(), bytes.len());
    Ok(())
}

fn run(cmd: &str, a: &Args) -> Result<(), String> {
    let mtime = archive::build_time();
    match cmd {
        "version-check" => {
            let v = a.version()?;
            let file = a
                .opt("cargo-toml")
                .unwrap_or_else(|| "rust/Cargo.toml".into());
            let text = std::fs::read_to_string(&file).map_err(|e| format!("{file}: {e}"))?;
            match version::workspace_version(&text) {
                Some(found) if found == v => {
                    println!("{file} says {v}, as the tag does");
                    Ok(())
                }
                found => Err(format!(
                    "{file} says {} but the tag says {v}. Run the version bump and re-tag.",
                    found.as_deref().unwrap_or("<nothing>")
                )),
            }
        }
        "mac-app" => {
            let v = a.version()?;
            let window = a.path("window")?;
            let icon = a.path("icon")?;
            let helpers: Vec<PathBuf> = a.many("helper").into_iter().map(PathBuf::from).collect();
            let resources: Vec<PathBuf> =
                a.many("resource").into_iter().map(PathBuf::from).collect();
            let identifier = a
                .opt("identifier")
                .unwrap_or_else(|| sp_update::IDENTIFIER.into());
            let bundle = mac::build_app(
                &mac::MacApp {
                    version: &v,
                    identifier: &identifier,
                    window: &window,
                    helpers: &helpers,
                    icon: &icon,
                    resources: &resources,
                },
                &a.path("out")?,
            )
            .map_err(|e| e.to_string())?;
            println!("laid out {}", bundle.display());
            Ok(())
        }
        "update-archive" => {
            let bytes = archive::app_archive(&a.path("app")?, mtime).map_err(|e| e.to_string())?;
            write(&a.path("out")?, &bytes)
        }
        "sign-update" => {
            let file = a.path("file")?;
            let v = a.version()?;
            let key = std::env::var("TAURI_SIGNING_PRIVATE_KEY").map_err(|_| {
                "TAURI_SIGNING_PRIVATE_KEY is not set: nothing can be signed, and an unsigned update is one no copy of Method Reader will install".to_string()
            })?;
            let password = std::env::var("TAURI_SIGNING_PRIVATE_KEY_PASSWORD").unwrap_or_default();
            let sk = sign::load_secret_key(&key, &password)?;
            let data = std::fs::read(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            let name = file
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let sig = sign::sign(&sk, &data, &name, &v, now)?;
            let mut out = file.into_os_string();
            out.push(names::SIGNATURE_EXT);
            write(Path::new(&out), sig.as_bytes())
        }
        "keygen" => {
            let out = a.path("out")?;
            let mut pub_path = out.clone().into_os_string();
            pub_path.push(".pub");
            if out.exists() || Path::new(&pub_path).exists() {
                return Err(format!(
                    "{} already exists; keygen never overwrites a key",
                    out.display()
                ));
            }
            let (sk, pk) = sign::keygen(&a.opt("password").unwrap_or_default())?;
            write(&out, sk.as_bytes())?;
            write(Path::new(&pub_path), pk.as_bytes())
        }
        "manifest" => {
            let v = a.one("version")?;
            let repo = a.opt("repo").unwrap_or_else(|| names::REPO.into());
            let artifacts = manifest::collect_signed_artifacts(&a.path("dir")?)?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            let mut m = manifest::render_manifest(&v, &artifacts, &repo, &manifest::iso_utc(now))?;
            if let Some(base) = a.opt("asset-base") {
                manifest::rebase(&mut m, &base);
            }
            write(&a.path("out")?, manifest::to_text(&m).as_bytes())
        }
        "verify-update" => {
            let text = std::fs::read_to_string(a.path("manifest")?).map_err(|e| e.to_string())?;
            let m: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
            let pubkey = a.opt("pubkey").unwrap_or_else(|| sp_update::PUBKEY.into());
            let ok = manifest::verify_dir(&m, &a.path("dir")?, &pubkey)?;
            println!(
                "every platform verifies with key {}: {}",
                sp_update::verify::key_id(&pubkey).map_err(|e| e.to_string())?,
                ok.join(", ")
            );
            Ok(())
        }
        "cli-archive" => {
            let bin = a.path("binary")?;
            let alias = a.opt("alias").map(PathBuf::from);
            let alias = alias.as_deref();
            let out = a.path("out")?;
            let tarball = || archive::cli_tarball(&bin, alias, mtime);
            let (name, bytes) = match a.one("kind")?.as_str() {
                "macos-arm64" => (names::CLI_MAC_ARM64, tarball()),
                "linux-x64" => (names::CLI_LINUX_X64, tarball()),
                "linux-arm64" => (names::CLI_LINUX_ARM64, tarball()),
                "windows-x64" => (names::CLI_WINDOWS_X64, archive::cli_zip(&bin, alias)),
                other => return Err(format!("no command-line archive is defined for {other}")),
            };
            write(&out.join(name), &bytes.map_err(|e| e.to_string())?)
        }
        "deb" | "rpm" | "appdir" => {
            let v = a.version()?;
            let window = a.path("window")?;
            let icons = a.icons()?;
            let resources: Vec<PathBuf> =
                a.many("resource").into_iter().map(PathBuf::from).collect();
            let depends = a.opt("depends").unwrap_or_default();
            let app = linux::LinuxApp {
                version: &v,
                window: &window,
                icons: &icons,
                resources: &resources,
                depends: &depends,
            };
            let out = a.path("out")?;
            match cmd {
                "deb" => {
                    let arch = a.one("arch")?;
                    let name = match arch.as_str() {
                        "amd64" => names::LINUX_DEB_AMD64.to_string(),
                        other => format!("{}-linux-{other}.deb", names::INSTALLER_STEM),
                    };
                    let bytes = linux::build_deb(&app, &arch, mtime).map_err(|e| e.to_string())?;
                    write(&out.join(name), &bytes)
                }
                "rpm" => {
                    let arch = a.one("arch")?;
                    let name = match arch.as_str() {
                        "x86_64" => names::LINUX_RPM_X86_64.to_string(),
                        other => format!("{}-linux-{other}.rpm", names::INSTALLER_STEM),
                    };
                    let bytes = linux::build_rpm(&app, &arch, mtime as u32)?;
                    write(&out.join(name), &bytes)
                }
                _ => {
                    let dir = linux::build_appdir(&app, &out).map_err(|e| e.to_string())?;
                    println!("laid out {}", dir.display());
                    Ok(())
                }
            }
        }
        "nsis" => {
            let v = a.version()?;
            let text = nsis::script(&v, &a.one("src")?, &a.one("setup")?);
            write(&a.path("script")?, text.as_bytes())
        }
        "check-latest" => {
            let repo = a.opt("repo").unwrap_or_else(|| names::REPO.into());
            let pubkey = a.opt("pubkey").unwrap_or_else(|| sp_update::PUBKEY.into());
            let deep = a.flags.iter().any(|f| f == "deep");
            match check_latest::check_latest(&repo, &pubkey, a.opt("version").as_deref(), deep) {
                Ok((v, platforms)) => {
                    println!(
                        "latest.json serves {v} for {}, signed by the key the app trusts.",
                        platforms.join(", ")
                    );
                    Ok(())
                }
                Err(problems) => {
                    for p in &problems {
                        eprintln!("::error::{p}");
                    }
                    Err("A red latest-check means the release published and the app's updater cannot use it. app-upload writes latest.json from what arrived; see docs/release-secrets.md §4.".into())
                }
            }
        }
        "serve" => {
            let dir = a.path("dir")?;
            let port: u16 = a
                .opt("port")
                .map(|p| p.parse().map_err(|_| format!("--port {p} is not a port")))
                .transpose()?
                .unwrap_or(0);
            serve::serve(&dir, port, |addr| {
                println!("serving {} at {addr}", dir.display())
            })
            .map_err(|e| e.to_string())
        }
        _ => Err(format!("unknown command {cmd:?}\n\n{USAGE}")),
    }
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let Some((cmd, rest)) = argv.split_first() else {
        eprint!("{USAGE}");
        std::process::exit(2);
    };
    if cmd == "--help" || cmd == "-h" {
        print!("{USAGE}");
        return;
    }
    let result = Args::parse(rest).and_then(|a| run(cmd, &a));
    if let Err(e) = result {
        eprintln!("sp-release {cmd}: {e}");
        std::process::exit(1);
    }
}
