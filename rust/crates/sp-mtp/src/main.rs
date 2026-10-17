//! method-reader-mtp's entry point: parse, run, print one JSON object, exit.
//! Every decision lives in the library modules; this file only wires them to
//! the real USB stack.

use mtp_rs::MtpDevice;
use nusb::MaybeFuture;
use serde_json::{Map, Value, json};
use sp_mtp::args::{self, Command};
use sp_mtp::ioreg;
use sp_mtp::kindle::{AMAZON_VENDOR_ID, is_kindle, is_mass_storage};
use sp_mtp::output::{Code, Failure, render, success};
use sp_mtp::paths::parse_device_path;
use sp_mtp::put::{device_failure, list_folder, put};
use sp_mtp::real::{RealSession, map_error, open_by_serial};
use sp_mtp::session::Session;
use std::time::{Duration, Instant};

/// How long one USB transfer may take before mtp-rs gives up on it.
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(30);
/// Between attempts to open a device something else holds.
const RETRY_EVERY: Duration = Duration::from_millis(250);

/// One USB device as the listing reports it.
struct Found {
    vendor_id: u16,
    product_id: u16,
    serial: Option<String>,
    manufacturer: Option<String>,
    model: Option<String>,
}

impl Found {
    fn kindle(&self) -> bool {
        is_kindle(self.vendor_id, self.model.as_deref())
    }

    fn json(&self) -> Map<String, Value> {
        let mut m = Map::new();
        m.insert("vendorId".into(), json!(self.vendor_id));
        m.insert("productId".into(), json!(self.product_id));
        m.insert(
            "vendorIdHex".into(),
            json!(format!("0x{:04x}", self.vendor_id)),
        );
        m.insert(
            "productIdHex".into(),
            json!(format!("0x{:04x}", self.product_id)),
        );
        m.insert("serial".into(), json!(self.serial));
        m.insert("manufacturer".into(), json!(self.manufacturer));
        m.insert("model".into(), json!(self.model));
        m.insert("kindle".into(), json!(self.kindle()));
        m
    }
}

/// Every Amazon device that is not a drive, from USB descriptors alone.
/// Opens nothing: this is what the engine polls every two seconds.
fn amazon_devices() -> Result<Vec<Found>, Failure> {
    let list = nusb::list_devices()
        .wait()
        .map_err(|e| Failure::new(Code::Internal, format!("could not list USB devices: {e}")))?;
    let mut out = Vec::new();
    for d in list {
        if d.vendor_id() != AMAZON_VENDOR_ID {
            continue;
        }
        let classes: Vec<u8> = d.interfaces().map(|i| i.class()).collect();
        #[cfg(target_os = "windows")]
        let driver = d.driver().map(|s| s.to_string());
        #[cfg(not(target_os = "windows"))]
        let driver: Option<String> = None;
        if is_mass_storage(&classes, driver.as_deref()) {
            continue;
        }
        out.push(Found {
            vendor_id: d.vendor_id(),
            product_id: d.product_id(),
            serial: d.serial_number().map(str::to_string),
            manufacturer: d.manufacturer_string().map(str::to_string),
            model: d.product_string().map(str::to_string),
        });
    }
    Ok(out)
}

/// Every device mtp-rs recognises as MTP, plus the Amazon devices it might
/// not (a Kindle's MTP interface is vendor-specific).
fn all_devices() -> Result<Vec<Found>, Failure> {
    let mut out: Vec<Found> = MtpDevice::list_devices()
        .map_err(|e| Failure::new(Code::Internal, format!("could not list MTP devices: {e}")))?
        .into_iter()
        .map(|d| Found {
            vendor_id: d.vendor_id,
            product_id: d.product_id,
            serial: d.serial_number,
            manufacturer: d.manufacturer,
            model: d.product,
        })
        .collect();
    for a in amazon_devices()? {
        let seen = out.iter().any(|f| {
            f.vendor_id == a.vendor_id && f.product_id == a.product_id && f.serial == a.serial
        });
        if !seen {
            out.push(a);
        }
    }
    Ok(out)
}

fn owner_json(found: &Found) -> Value {
    match ioreg::lookup(found.vendor_id, found.serial.as_deref()) {
        Some(o) => json!({ "name": o.name, "raw": o.raw }),
        None => Value::Null,
    }
}

/// Open a session and read what a person checking a Kindle wants to see.
fn storages_json(found: &Found) -> Map<String, Value> {
    let mut m = Map::new();
    let Some(serial) = found.serial.as_deref() else {
        m.insert("storages".into(), Value::Null);
        m.insert(
            "storageError".into(),
            json!("it reports no serial number, so it cannot be opened by one"),
        );
        return m;
    };
    match open_by_serial(found.vendor_id, found.product_id, serial, TRANSFER_TIMEOUT) {
        Err(e) => {
            m.insert("storages".into(), Value::Null);
            m.insert("busy".into(), json!(e.is_exclusive_access()));
            m.insert("storageError".into(), json!(e.to_string()));
        }
        Ok(device) => {
            let info = device.device_info();
            m.insert(
                "deviceInfo".into(),
                json!({
                    "manufacturer": info.manufacturer, "model": info.model,
                    "serial": info.serial_number, "version": info.device_version,
                }),
            );
            let caps = device.capabilities();
            m.insert(
                "capabilities".into(),
                json!({
                    "upload": caps.can_upload, "delete": caps.can_delete, "rename": caps.can_rename,
                    "createFolder": caps.can_create_folder,
                }),
            );
            let mut session = RealSession::new(device);
            match session.storages() {
                Ok(list) => {
                    let list: Vec<Value> = list
                        .iter()
                        .map(|s| {
                            json!({
                                "id": s.id, "description": s.description, "freeBytes": s.free_bytes,
                                "capacityBytes": s.capacity_bytes, "writable": s.writable,
                            })
                        })
                        .collect();
                    m.insert("storages".into(), Value::Array(list));
                }
                Err(e) => {
                    m.insert("storages".into(), Value::Null);
                    m.insert("storageError".into(), json!(e.describe()));
                }
            }
            m.insert("sessionClosed".into(), json!(session.close().is_ok()));
        }
    }
    m
}

fn list(kindles_only: bool, storages: bool) -> Result<Value, Failure> {
    let found = if kindles_only {
        amazon_devices()?
            .into_iter()
            .filter(Found::kindle)
            .collect()
    } else {
        all_devices()?
    };
    let devices: Vec<Value> = found
        .iter()
        .map(|f| {
            let mut m = f.json();
            if !kindles_only {
                m.insert("exclusiveOwner".into(), owner_json(f));
            }
            if storages {
                m.extend(storages_json(f));
            } else {
                m.insert("storages".into(), Value::Null);
            }
            Value::Object(m)
        })
        .collect();
    let mut fields = Map::new();
    fields.insert("devices".into(), Value::Array(devices));
    Ok(success(fields))
}

fn busy(found: &Found) -> Failure {
    let owner = ioreg::lookup(found.vendor_id, found.serial.as_deref());
    let message = match owner.as_ref().map(|o| o.name.as_str()) {
        Some("ptpcamerad") => {
            "macOS's camera import (ptpcamerad) is holding the Kindle".to_string()
        }
        Some(name) => format!("{name} is using the Kindle. Close it and try again"),
        None => "another program is using the Kindle. Close it and try again".to_string(),
    };
    let mut f = Failure::new(Code::Busy, message);
    if let Some(o) = owner {
        f = f
            .with("holder", json!(o.name))
            .with("holderRaw", json!(o.raw));
    } else {
        f = f.with("holder", Value::Null);
    }
    f
}

fn put_command(
    serial: &str,
    local: &std::path::Path,
    device_path: &str,
    wait_ms: u64,
    keep_session: bool,
) -> Result<Value, Failure> {
    let size = match std::fs::metadata(local) {
        Ok(m) if m.is_file() => m.len(),
        _ => {
            return Err(Failure::new(
                Code::Unreadable,
                format!("cannot read the file to copy: {}", local.display()),
            ));
        }
    };
    if std::fs::File::open(local).is_err() {
        return Err(Failure::new(
            Code::Unreadable,
            format!("cannot read the file to copy: {}", local.display()),
        ));
    }
    let dest = parse_device_path(device_path)?;

    // Looked up from descriptors first, so a device that is not a Kindle is
    // refused before anything opens it.
    let found = find_kindle(serial)?;

    let started = Instant::now();
    let device = loop {
        match open_by_serial(found.vendor_id, found.product_id, serial, TRANSFER_TIMEOUT) {
            Ok(d) => break d,
            Err(e) if e.is_exclusive_access() => {
                if started.elapsed() >= Duration::from_millis(wait_ms) {
                    return Err(busy(&found));
                }
                std::thread::sleep(RETRY_EVERY);
            }
            Err(e) => return Err(device_failure("open the Kindle", &map_error(&e))),
        }
    };

    let model = device.device_info().model.clone();
    let mut session = RealSession::new(device);
    let outcome = put(&mut session, local, size, &dest);
    let closed = if keep_session {
        false
    } else {
        session.close().is_ok()
    };
    let outcome = outcome?;

    let mut fields = Map::new();
    fields.insert("device".into(), json!({
        "serial": serial, "model": model, "vendorId": found.vendor_id, "productId": found.product_id,
    }));
    fields.insert("path".into(), json!(outcome.path));
    fields.insert("bytes".into(), json!(outcome.bytes));
    fields.insert("method".into(), json!(outcome.method.as_str()));
    fields.insert("storage".into(), json!(outcome.storage));
    fields.insert("replaced".into(), json!(outcome.replaced));
    fields.insert("sessionClosed".into(), json!(closed));
    Ok(success(fields))
}

/// The Kindle with this serial, from USB descriptors alone. Anything that is
/// not a Kindle, or a Kindle connected as a drive, is refused here, before
/// anything opens it: the helper never writes to (or opens) a phone.
fn find_kindle(serial: &str) -> Result<Found, Failure> {
    let list = nusb::list_devices()
        .wait()
        .map_err(|e| Failure::new(Code::Internal, format!("could not list USB devices: {e}")))?;
    for d in list {
        if d.serial_number() != Some(serial) {
            continue;
        }
        let classes: Vec<u8> = d.interfaces().map(|i| i.class()).collect();
        #[cfg(target_os = "windows")]
        let driver = d.driver().map(|s| s.to_string());
        #[cfg(not(target_os = "windows"))]
        let driver: Option<String> = None;
        let found = Found {
            vendor_id: d.vendor_id(),
            product_id: d.product_id(),
            serial: Some(serial.to_string()),
            manufacturer: d.manufacturer_string().map(str::to_string),
            model: d.product_string().map(str::to_string),
        };
        if !found.kindle() {
            return Err(Failure::new(
                Code::NotAKindle,
                format!(
                    "the device with the serial {serial} is not a Kindle ({}), so Method Reader will not touch it",
                    found.model.as_deref().unwrap_or("unnamed")
                ),
            ));
        }
        if is_mass_storage(&classes, driver.as_deref()) {
            return Err(Failure::new(
                Code::NotAKindle,
                "this Kindle is connected as a drive, not over MTP: copy to it as a drive",
            ));
        }
        return Ok(found);
    }
    Err(Failure::new(
        Code::NoDevice,
        format!(
            "no Kindle with the serial {serial} is plugged in. Plug it in, unlock it, and try again"
        ),
    ))
}

/// `ls`: read one folder, then close the session.
fn ls_command(serial: &str, folder: Option<&str>) -> Result<Value, Failure> {
    let found = find_kindle(serial)?;
    let device = match open_by_serial(found.vendor_id, found.product_id, serial, TRANSFER_TIMEOUT) {
        Ok(d) => d,
        Err(e) if e.is_exclusive_access() => return Err(busy(&found)),
        Err(e) => return Err(device_failure("open the Kindle", &map_error(&e))),
    };
    let mut session = RealSession::new(device);
    let listed = list_folder(&mut session, folder);
    let _ = session.close();
    let (storage, kids) = listed?;
    let mut fields = Map::new();
    fields.insert("storage".into(), json!(storage));
    fields.insert("folder".into(), json!(folder.unwrap_or("")));
    fields.insert(
        "entries".into(),
        Value::Array(
            kids.iter()
                .map(|k| json!({ "name": k.name, "folder": k.folder }))
                .collect(),
        ),
    );
    Ok(success(fields))
}

fn run(argv: &[String]) -> Result<Value, Failure> {
    match args::parse(argv)? {
        Command::Version => {
            let mut fields = Map::new();
            fields.insert("version".into(), json!(env!("CARGO_PKG_VERSION")));
            fields.insert("mtpRs".into(), json!("0.33.0"));
            Ok(success(fields))
        }
        Command::Help => {
            let mut fields = Map::new();
            fields.insert("usage".into(), json!(args::USAGE));
            Ok(success(fields))
        }
        Command::List {
            kindles_only,
            storages,
        } => list(kindles_only, storages),
        Command::Put {
            serial,
            local,
            device_path,
            wait_ms,
            keep_session,
        } => put_command(&serial, &local, &device_path, wait_ms, keep_session),
        Command::Ls { serial, folder } => ls_command(&serial, folder.as_deref()),
    }
}

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    // A panic still answers in the contract: one JSON object, code internal.
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(|| run(&argv)).unwrap_or_else(|panic| {
        let detail = panic
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_else(|| "unknown".into());
        Err(Failure::new(
            Code::Internal,
            format!("the MTP helper stopped unexpectedly: {detail}"),
        ))
    });
    let (text, code) = render(result);
    println!("{text}");
    std::process::exit(code);
}
