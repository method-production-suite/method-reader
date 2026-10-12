//! What counts as a Kindle the helper may write to. Pure, so it is tested
//! without a device.
//!
//! Amazon's USB vendor id is 0x1949 on every Kindle (Calibre's Kindle MTP
//! driver matches on the vendor alone; docs/device-map.md §2.1). The only
//! product id on record is 0x9981 (Scribe 32 GB, libmtp), so matching on the
//! product would miss every other model. Two things share the vendor and are
//! not what this route is for:
//!
//! - a Kindle of the mass-storage era, which mounts as a drive and is
//!   already the engine's volume route (an interface of class 8, or on
//!   Windows a whole device bound to USBSTOR);
//! - a Fire tablet, an Android tablet whose USB product string says "Fire".
//!   Its Kindle app does not read a book copied into documents/.

/// Amazon's USB vendor id.
pub const AMAZON_VENDOR_ID: u16 = 0x1949;

/// The one Kindle product id libmtp lists (Scribe 32 GB). Passed to mtp-rs as
/// a known device so it matches without opening the device to read
/// descriptors; any other Kindle is found by vendor.
pub const KNOWN_KINDLE_PRODUCT_IDS: &[u16] = &[0x9981];

/// USB interface class of a mass-storage device.
pub const MASS_STORAGE_CLASS: u8 = 0x08;

/// A Kindle this helper may copy to: Amazon's vendor, and not a Fire tablet.
pub fn is_kindle(vendor_id: u16, product: Option<&str>) -> bool {
    vendor_id == AMAZON_VENDOR_ID
        && !product.is_some_and(|p| p.to_ascii_lowercase().contains("fire"))
}

/// A device that presents itself as a drive: any interface of the
/// mass-storage class, or (Windows, where interface details are hidden for a
/// device bound whole to one driver) the USB storage driver.
pub fn is_mass_storage(interface_classes: &[u8], windows_driver: Option<&str>) -> bool {
    interface_classes.contains(&MASS_STORAGE_CLASS)
        || windows_driver.is_some_and(|d| d.eq_ignore_ascii_case("USBSTOR"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_amazon_product_is_a_kindle_unless_it_is_a_fire() {
        assert!(is_kindle(0x1949, Some("Kindle")));
        assert!(is_kindle(0x1949, Some("Amazon Kindle Paperwhite")));
        assert!(is_kindle(0x1949, None));
        assert!(!is_kindle(0x1949, Some("Fire")));
        assert!(!is_kindle(0x1949, Some("Amazon FIRE HD 10")));
    }

    #[test]
    fn a_phone_or_a_camera_is_never_a_kindle() {
        assert!(!is_kindle(0x18d1, Some("Pixel 9 Pro XL")));
        assert!(!is_kindle(0x04e8, Some("Kindle"))); // a name is not a vendor
        assert!(!is_kindle(0xffff, Some("Virtual Kindle")));
    }

    #[test]
    fn mass_storage_by_interface_or_by_windows_driver() {
        assert!(is_mass_storage(&[0x08], None));
        assert!(is_mass_storage(&[0x06, 0x08], None));
        assert!(!is_mass_storage(&[0x06], None));
        assert!(!is_mass_storage(&[0xff], None));
        assert!(!is_mass_storage(&[], None));
        assert!(is_mass_storage(&[], Some("USBSTOR")));
        assert!(!is_mass_storage(&[], Some("WUDFWpdMtp")));
    }
}
