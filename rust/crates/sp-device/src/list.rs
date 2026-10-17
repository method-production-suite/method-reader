//! src/device/list.ts: the one list of everything reachable right now.
//! Volume-mounted vendors (by on-disk signature), Kindles that connect over
//! MTP (beta, off: [`crate::mtp`]), and a reMarkable, which never mounts and
//! is only there if its USB web interface answers.

use crate::mtp::{list_mtp_kindles_here, mtp_enabled, process_env};
use crate::remarkable::{
    REMARKABLE_ENDPOINT, REMARKABLE_PROBE_TIMEOUT, UreqClient, probe_remarkable,
};
use crate::types::ConnectedDevice;
use crate::volumes::mounted_devices;
use std::time::Duration;

/// The mount scan's seam.
pub type ScanFn<'a> = &'a (dyn Fn(Option<&[String]>) -> Vec<ConnectedDevice> + Sync);
/// The reMarkable probe's seam: `Err` is a probe that threw, which is a
/// tablet that is not there.
pub type ProbeFn<'a> = &'a (dyn Fn(&str, Duration) -> Result<bool, String> + Sync);
/// The MTP listing's seam: `Err` is a listing that threw, which is no MTP
/// Kindles.
pub type MtpFn<'a> = &'a (dyn Fn() -> Result<Vec<ConnectedDevice>, String> + Sync);

/// `ListDevicesOptions`. Every field defaults to production.
#[derive(Default, Clone)]
pub struct ListDevicesOptions<'a> {
    /// Mount parents to scan; defaults to this platform's.
    pub roots: Option<Vec<String>>,
    /// reMarkable base URL; defaults to the fixed USB address.
    pub remarkable_endpoint: Option<String>,
    pub probe_timeout: Option<Duration>,
    /// List only what the mount scan (and MTP) sees, and never start the
    /// reMarkable probe. The probe waits out its whole timeout on every
    /// machine with no tablet docked, so a caller that has to answer at once
    /// (`routes --quick`, the send panel's first answer) leaves the tablet
    /// out and asks again in full.
    pub skip_remarkable: bool,
    /// Leave MTP Kindles out, and never run the helper.
    pub skip_mtp: bool,
    pub scan: Option<ScanFn<'a>>,
    pub probe: Option<ProbeFn<'a>>,
    pub mtp: Option<MtpFn<'a>>,
}

impl ListDevicesOptions<'_> {
    /// `deviceSeams()` in cli.ts: `METHOD_READER_VOLUME_ROOTS` (the platform's
    /// path delimiter between roots) and `METHOD_READER_REMARKABLE_ENDPOINT`, the
    /// overrides the CLI honors so a test can point it at fake readers.
    pub fn from_env() -> ListDevicesOptions<'static> {
        let delimiter = if cfg!(windows) { ';' } else { ':' };
        let roots = std::env::var("METHOD_READER_VOLUME_ROOTS")
            .ok()
            .filter(|r| !r.is_empty())
            .map(|r| {
                r.split(delimiter)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            });
        let remarkable_endpoint = std::env::var("METHOD_READER_REMARKABLE_ENDPOINT")
            .ok()
            .filter(|e| !e.is_empty());
        ListDevicesOptions {
            roots,
            remarkable_endpoint,
            ..Default::default()
        }
    }
}

/// Every recognised reader that is reachable right now: the mounted ones,
/// then MTP Kindles, then a reMarkable that answered.
///
/// The probe's timeout is paid on every machine with no reMarkable, so the
/// scan, the MTP listing and the probe all run at once: the call's wall
/// clock is the probe's timeout, never timeout + scan.
pub fn list_devices(options: &ListDevicesOptions) -> Vec<ConnectedDevice> {
    let endpoint = options
        .remarkable_endpoint
        .as_deref()
        .unwrap_or(REMARKABLE_ENDPOINT);
    let timeout = options.probe_timeout.unwrap_or(REMARKABLE_PROBE_TIMEOUT);
    let roots = options.roots.as_deref();

    std::thread::scope(|s| {
        let probing = (!options.skip_remarkable).then(|| {
            s.spawn(move || match options.probe {
                Some(probe) => probe(endpoint, timeout).unwrap_or(false),
                None => probe_remarkable(&UreqClient, endpoint, timeout),
            })
        });
        let mtp_listing = (!options.skip_mtp).then(|| {
            s.spawn(move || match options.mtp {
                Some(mtp) => mtp().unwrap_or_default(),
                None if mtp_enabled(&process_env) => list_mtp_kindles_here(),
                None => Vec::new(),
            })
        });

        let mut devices = match options.scan {
            Some(scan) => scan(roots),
            None => mounted_devices(roots),
        };
        // A seam that panicked is the same as one that threw: nothing.
        if let Some(listing) = mtp_listing {
            devices.extend(listing.join().unwrap_or_default());
        }
        if let Some(probing) = probing
            && probing.join().unwrap_or(false)
        {
            devices.push(ConnectedDevice::remarkable());
        }
        devices
    })
}
