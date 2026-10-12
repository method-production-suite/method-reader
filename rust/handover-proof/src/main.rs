//! The handover, proved with the window's own code and no window.
//!
//!   handover-proof --bundle <a COPY of a Tauri-built Screepub.app>
//!                  --endpoint http://127.0.0.1:<port>/latest.json
//!                  --pubkey <base64 key box> [--current 0.11.1]
//!
//! Builds a Tauri app on tauri's mock runtime with tauri-plugin-updater
//! 2.12.0 configured exactly as tauri.conf.json configures every window
//! (only the key, the endpoint and the http allowance differ), points the
//! updater at the given bundle as if it were running from it, and runs the
//! same calls the window's JavaScript makes: check(), then
//! downloadAndInstall(). Then it reads the swapped bundle the way tauri
//! 2.11.5's restart does (CFBundleExecutable from the NEW Info.plist) and
//! says what a relaunch would start. It never starts it.

use std::path::PathBuf;

use tauri_plugin_updater::UpdaterExt;

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn read_plist(bundle: &std::path::Path) -> plist::Dictionary {
    plist::from_file(bundle.join("Contents").join("Info.plist")).expect("Info.plist")
}

fn get(d: &plist::Dictionary, k: &str) -> String {
    d.get(k)
        .and_then(plist::Value::as_string)
        .unwrap_or("<none>")
        .to_string()
}

fn main() {
    let bundle = PathBuf::from(arg("--bundle").expect("--bundle"));
    let endpoint = arg("--endpoint").expect("--endpoint");
    let pubkey = arg("--pubkey").expect("--pubkey");
    let current = arg("--current").unwrap_or_else(|| "0.11.1".into());

    let before = read_plist(&bundle);
    println!(
        "before: {} {} ({}), runs {}",
        get(&before, "CFBundleName"),
        get(&before, "CFBundleShortVersionString"),
        get(&before, "CFBundleIdentifier"),
        get(&before, "CFBundleExecutable")
    );
    let running = bundle
        .join("Contents")
        .join("MacOS")
        .join(get(&before, "CFBundleExecutable"));

    let mut context = tauri::test::mock_context(tauri::test::noop_assets());
    context.package_info_mut().version = current.parse().expect("--current is semver");
    context.config_mut().plugins.0.insert(
        "updater".into(),
        serde_json::json!({
            "pubkey": pubkey,
            "endpoints": [endpoint],
            "dangerousInsecureTransportProtocol": true
        }),
    );
    let app = tauri::test::mock_builder()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .build(context)
        .expect("a mock Tauri app with the updater plugin");

    tauri::async_runtime::block_on(async {
        let updater = app
            .updater_builder()
            .executable_path(&running)
            .build()
            .expect("the updater");
        let update = match updater.check().await {
            Ok(Some(u)) => u,
            Ok(None) => {
                eprintln!("FAIL: check() found nothing newer than {current}");
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("FAIL: check(): {e}");
                std::process::exit(1);
            }
        };
        println!(
            "check(): {} -> {} for {} from {}",
            update.current_version, update.version, update.target, update.download_url
        );
        let mut received = 0usize;
        if let Err(e) = update
            .download_and_install(|n, _| received += n, || println!("downloaded"))
            .await
        {
            eprintln!("FAIL: downloadAndInstall(): {e}");
            std::process::exit(1);
        }
        println!("downloadAndInstall(): {received} bytes, verified, the bundle swapped in place");
    });

    let after = read_plist(&bundle);
    let exe = bundle
        .join("Contents")
        .join("MacOS")
        .join(get(&after, "CFBundleExecutable"));
    println!(
        "after: {} {} ({}), runs {}",
        get(&after, "CFBundleName"),
        get(&after, "CFBundleShortVersionString"),
        get(&after, "CFBundleIdentifier"),
        get(&after, "CFBundleExecutable")
    );
    #[cfg(unix)]
    let executable = {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(&exe)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    };
    #[cfg(not(unix))]
    let executable = exe.is_file();
    if !executable {
        eprintln!(
            "FAIL: the relaunch target {} is not an executable file",
            exe.display()
        );
        std::process::exit(1);
    }
    println!("relaunch() would start {}", exe.display());
}
