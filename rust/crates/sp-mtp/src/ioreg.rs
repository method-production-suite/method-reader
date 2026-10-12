//! Who holds a USB device exclusively on macOS, read off IORegistry.
//!
//! mtp-rs reports `Error::ExclusiveAccess` when something else has the
//! device and its docs point at IORegistry's `UsbExclusiveOwner` for the
//! name (docs/device-map.md §7). The helper asks `ioreg` rather than linking
//! IOKit: one read-only spawn, only when a name is wanted, and a parser that
//! is plain text and therefore testable here without a Kindle.
//!
//! UNPROVEN: the exact text of `UsbExclusiveOwner` has not been seen on a
//! Kindle. The parser takes the process name after "pid N, " when the value
//! has that shape and the whole value otherwise, and `raw` is reported
//! beside it so hardware day can read what macOS actually said.

/// `ioreg -r -c IOUSBHostDevice -l -w0`: every USB device as a root, with
/// its children (the interfaces) and every property, unwrapped.
pub const IOREG_ARGV: &[&str] = &["ioreg", "-r", "-c", "IOUSBHostDevice", "-l", "-w0"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Owner {
    /// The process name, e.g. "ptpcamerad".
    pub name: String,
    /// The property's value as macOS printed it.
    pub raw: String,
}

/// One device's block of `ioreg -r` output: from its `+-o` line at column 0
/// to the next one.
fn device_blocks(text: &str) -> Vec<Vec<&str>> {
    let mut blocks: Vec<Vec<&str>> = Vec::new();
    for line in text.lines() {
        if line.starts_with("+-o ") {
            blocks.push(vec![line]);
        } else if let Some(block) = blocks.last_mut() {
            block.push(line);
        }
    }
    blocks
}

/// `"key" = value` on one line, the value unquoted when quoted.
fn property<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let quoted = format!("\"{key}\" = ");
    let at = line.find(&quoted)?;
    let value = line[at + quoted.len()..].trim();
    Some(
        value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .unwrap_or(value),
    )
}

fn first_property<'a>(block: &[&'a str], key: &str) -> Option<&'a str> {
    block.iter().find_map(|line| property(line, key))
}

/// "pid 412, ptpcamerad" -> "ptpcamerad"; anything else as it is.
pub fn owner_name(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("pid ")
        && let Some((pid, name)) = rest.split_once(',')
        && pid.trim().chars().all(|c| c.is_ascii_digit())
        && !name.trim().is_empty()
    {
        return name.trim().to_string();
    }
    trimmed.to_string()
}

/// The exclusive owner of the device with this vendor id and serial, from
/// `ioreg -r -c IOUSBHostDevice -l -w0` text. None when the device is not in
/// the text or nothing holds it. A device with no serial is matched by
/// vendor alone only when `serial` is None.
pub fn exclusive_owner(text: &str, vendor_id: u16, serial: Option<&str>) -> Option<Owner> {
    for block in device_blocks(text) {
        let vendor = first_property(&block, "idVendor").and_then(|v| v.parse::<u32>().ok());
        if vendor != Some(u32::from(vendor_id)) {
            continue;
        }
        let found_serial = first_property(&block, "USB Serial Number")
            .or_else(|| first_property(&block, "kUSBSerialNumberString"));
        if serial.is_some() && found_serial != serial {
            continue;
        }
        if let Some(raw) = first_property(&block, "UsbExclusiveOwner") {
            return Some(Owner {
                name: owner_name(raw),
                raw: raw.to_string(),
            });
        }
        return None;
    }
    None
}

/// Run ioreg and read the owner. macOS only; None anywhere else, and None if
/// ioreg cannot be run.
pub fn lookup(vendor_id: u16, serial: Option<&str>) -> Option<Owner> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let out = std::process::Command::new(IOREG_ARGV[0])
        .args(&IOREG_ARGV[1..])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    exclusive_owner(&String::from_utf8_lossy(&out.stdout), vendor_id, serial)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Shaped on real `ioreg -r -c IOUSBHostDevice -l -w0` output from this
    // Mac (2026-10-10), with an invented Kindle (vendor 6473 = 0x1949) and an
    // invented phone. The owner values are guesses at the shape: no Kindle
    // has been plugged in here.
    const SAMPLE: &str = r#"+-o Pixel 9 Pro XL@02100000  <class IOUSBHostDevice, id 0x100000a01, registered, matched, active, busy 0 (2 ms), retain 25>
  | {
  |   "idProduct" = 20193
  |   "USB Serial Number" = "PHONE0001"
  |   "idVendor" = 6353
  |   "UsbExclusiveOwner" = "pid 300, Android File Transfer Agent"
  | }
  |
+-o Kindle@01100000  <class IOUSBHostDevice, id 0x100000b02, registered, matched, active, busy 0 (5 ms), retain 30>
  | {
  |   "idProduct" = 39297
  |   "USB Product Name" = "Kindle"
  |   "USB Serial Number" = "G000INVENTED01"
  |   "idVendor" = 6473
  | }
  |
  +-o IOUSBHostInterface@0  <class IOUSBHostInterface, id 0x100000b03, registered, matched, active, busy 0 (1 ms), retain 8>
    | {
    |   "bInterfaceClass" = 255
    |   "UsbExclusiveOwner" = "pid 412, ptpcamerad"
    | }
    |
"#;

    #[test]
    fn the_kindle_s_holder_is_found_on_its_interface() {
        let owner = exclusive_owner(SAMPLE, 0x1949, Some("G000INVENTED01")).unwrap();
        assert_eq!(owner.name, "ptpcamerad");
        assert_eq!(owner.raw, "pid 412, ptpcamerad");
    }

    #[test]
    fn another_device_s_holder_is_never_reported_for_the_kindle() {
        let owner = exclusive_owner(SAMPLE, 0x18d1, Some("PHONE0001")).unwrap();
        assert_eq!(owner.name, "Android File Transfer Agent");
        assert_eq!(exclusive_owner(SAMPLE, 0x1949, Some("SOMEONE-ELSE")), None);
    }

    #[test]
    fn nothing_holding_it_is_none() {
        let free = SAMPLE.replace(
            "    |   \"UsbExclusiveOwner\" = \"pid 412, ptpcamerad\"\n",
            "",
        );
        assert_eq!(exclusive_owner(&free, 0x1949, Some("G000INVENTED01")), None);
        assert_eq!(exclusive_owner("", 0x1949, None), None);
    }

    #[test]
    fn an_owner_that_is_not_pid_shaped_is_reported_whole() {
        assert_eq!(owner_name("pid 9, calibre-parallel"), "calibre-parallel");
        assert_eq!(owner_name("ptpcamerad"), "ptpcamerad");
        assert_eq!(owner_name("pid x, thing"), "pid x, thing");
        assert_eq!(owner_name("  pid 12,  OpenMTP Helper "), "OpenMTP Helper");
    }

    // `-r` prints a device behind a hub twice: as its own root block, and
    // again inside the hub's block. The hub's block must answer for the hub
    // (its own properties come first), never for the Kindle nested in it.
    // Measured on this Mac 2026-10-10: a hub's own owner is its driver,
    // "AppleUSB20Hub", not a pid.
    const BEHIND_A_HUB: &str = r#"+-o USB2.0 Hub@01100000  <class IOUSBHostDevice, id 0x1000770d4, registered, matched, active, busy 0 (114 ms), retain 34>
  |   "USB Product Name" = "USB2.0 Hub"
  |   "UsbExclusiveOwner" = "AppleUSB20Hub"
  |   "idVendor" = 8584
  +-o AppleUSB20Hub@01100000  <class AppleUSB20Hub, id 0x1000770d6, registered, matched, active, busy 0 (108 ms), retain 27>
  | |   |   "USB Product Name" = "Kindle"
  | |   |   "USB Serial Number" = "G000INVENTED01"
  | |   |   "idVendor" = 6473
  | |   |   "UsbExclusiveOwner" = "pid 412, ptpcamerad"
+-o Kindle@01110000  <class IOUSBHostDevice, id 0x100000b02, registered, matched, active, busy 0 (5 ms), retain 30>
  |   "USB Product Name" = "Kindle"
  |   "USB Serial Number" = "G000INVENTED01"
  |   "idVendor" = 6473
  |   "UsbExclusiveOwner" = "pid 412, ptpcamerad"
"#;

    #[test]
    fn a_hub_never_answers_for_the_kindle_behind_it() {
        assert_eq!(
            exclusive_owner(BEHIND_A_HUB, 8584, None).unwrap().name,
            "AppleUSB20Hub"
        );
        assert_eq!(
            exclusive_owner(BEHIND_A_HUB, 0x1949, Some("G000INVENTED01"))
                .unwrap()
                .name,
            "ptpcamerad"
        );
    }

    #[test]
    fn with_no_serial_the_first_device_of_the_vendor_answers() {
        assert_eq!(
            exclusive_owner(SAMPLE, 0x1949, None).unwrap().name,
            "ptpcamerad"
        );
    }
}
