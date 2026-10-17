//! The one adapter onto mtp-rs: `Session` over a real (or virtual) MtpDevice.
//!
//! mtp-rs is async and runtime-agnostic; this helper runs one operation at a
//! time, so each call is driven to completion with `block_on` and no runtime
//! is needed. tests/virtual_device.rs runs this file against mtp-rs's own
//! virtual device, so everything here but the USB transport is exercised
//! without hardware.

use crate::session::{DevError, Entry, Session, StorageSummary, UploadFailure};
use bytes::Bytes;
use futures::executor::block_on;
use mtp_rs::{MtpDevice, NewObjectInfo, ObjectHandle, Storage, StorageId};
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::Duration;

/// How much of the book is read and sent at a time.
const CHUNK: usize = 256 * 1024;

/// mtp-rs's error, narrowed to what the copy and the engine act on.
pub fn map_error(e: &mtp_rs::Error) -> DevError {
    use mtp_rs::Error as E;
    match e {
        E::NotFound | E::StaleHandle => DevError::NotFound,
        E::AccessDenied => DevError::AccessDenied,
        E::ExclusiveAccess => DevError::Exclusive,
        E::PermissionDenied => DevError::Permission,
        E::Unsupported => DevError::Unsupported,
        E::StorageFull => DevError::StorageFull,
        E::Disconnected | E::NoDevice => DevError::Disconnected,
        other => DevError::Other(other.to_string()),
    }
}

/// Open the Kindle with this serial. The vendor and product are passed as a
/// known device, so mtp-rs matches it at once rather than by opening it to
/// scan its descriptors (a Kindle's MTP interface is vendor-specific; mtp-rs
/// notes it is found that way otherwise).
pub fn open_by_serial(
    vendor_id: u16,
    product_id: u16,
    serial: &str,
    timeout: Duration,
) -> Result<MtpDevice, mtp_rs::Error> {
    block_on(
        MtpDevice::builder()
            .known_devices(&[(vendor_id, product_id)])
            .timeout(timeout)
            .open_by_serial(serial),
    )
}

pub struct RealSession {
    device: MtpDevice,
    storages: HashMap<u64, Storage>,
}

impl RealSession {
    pub fn new(device: MtpDevice) -> Self {
        RealSession {
            device,
            storages: HashMap::new(),
        }
    }

    /// Send CloseSession. Dropping the device does not.
    pub fn close(self) -> Result<(), DevError> {
        block_on(self.device.close()).map_err(|e| map_error(&e))
    }

    pub fn device(&self) -> &MtpDevice {
        &self.device
    }

    fn storage(&mut self, id: u64) -> Result<&Storage, DevError> {
        if !self.storages.contains_key(&id) {
            let s = block_on(self.device.storage(StorageId(id))).map_err(|e| map_error(&e))?;
            self.storages.insert(id, s);
        }
        Ok(&self.storages[&id])
    }
}

/// The book as a stream of chunks, read as the upload asks for them, so a
/// large file is never held whole in memory. A read error ends the stream
/// after reporting it.
fn file_stream(file: File) -> impl futures::Stream<Item = Result<Bytes, std::io::Error>> + Send {
    futures::stream::unfold(Some(file), |state| async move {
        let mut file = state?;
        let mut buf = vec![0u8; CHUNK];
        match file.read(&mut buf) {
            Ok(0) => None,
            Ok(n) => {
                buf.truncate(n);
                Some((Ok(Bytes::from(buf)), Some(file)))
            }
            Err(e) => Some((Err(e), None)),
        }
    })
}

impl Session for RealSession {
    fn storages(&mut self) -> Result<Vec<StorageSummary>, DevError> {
        let list = block_on(self.device.storages()).map_err(|e| map_error(&e))?;
        let mut out = Vec::new();
        for s in list {
            let info = s.info();
            out.push(StorageSummary {
                id: s.id().0,
                description: info.description.clone(),
                free_bytes: info.free_space,
                capacity_bytes: info.total_capacity,
                writable: info.is_writable,
            });
            self.storages.insert(s.id().0, s);
        }
        Ok(out)
    }

    fn children(&mut self, storage: u64, parent: Option<u64>) -> Result<Vec<Entry>, DevError> {
        let s = self.storage(storage)?;
        let list = block_on(s.list_objects(parent.map(ObjectHandle))).map_err(|e| map_error(&e))?;
        Ok(list
            .into_iter()
            .map(|o| Entry {
                handle: o.handle.0,
                folder: o.is_folder(),
                name: o.filename,
            })
            .collect())
    }

    fn create_folder(
        &mut self,
        storage: u64,
        parent: Option<u64>,
        name: &str,
    ) -> Result<u64, DevError> {
        let s = self.storage(storage)?;
        block_on(s.create_folder(parent.map(ObjectHandle), name))
            .map(|h| h.0)
            .map_err(|e| map_error(&e))
    }

    fn upload(
        &mut self,
        storage: u64,
        parent: Option<u64>,
        name: &str,
        local: &Path,
        size: u64,
    ) -> Result<u64, UploadFailure> {
        let file = File::open(local).map_err(|e| UploadFailure {
            error: DevError::Other(format!("could not read {}: {e}", local.display())),
            partial: None,
        })?;
        let s = self.storage(storage).map_err(|error| UploadFailure {
            error,
            partial: None,
        })?;
        let stream = Box::pin(file_stream(file));
        block_on(s.upload(
            parent.map(ObjectHandle),
            NewObjectInfo::file(name, size),
            stream,
        ))
        .map(|h| h.0)
        .map_err(|f| UploadFailure {
            error: map_error(&f.source),
            partial: f.partial.map(|h| h.0),
        })
    }

    fn rename(&mut self, storage: u64, handle: u64, name: &str) -> Result<(), DevError> {
        let s = self.storage(storage)?;
        block_on(s.rename(ObjectHandle(handle), name)).map_err(|e| map_error(&e))
    }

    fn delete(&mut self, storage: u64, handle: u64) -> Result<(), DevError> {
        let s = self.storage(storage)?;
        block_on(s.delete(ObjectHandle(handle))).map_err(|e| map_error(&e))
    }

    fn supports_rename(&self) -> bool {
        self.device.supports_rename()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_errors_the_engine_acts_on_keep_their_meaning() {
        assert_eq!(
            map_error(&mtp_rs::Error::ExclusiveAccess),
            DevError::Exclusive
        );
        assert_eq!(
            map_error(&mtp_rs::Error::PermissionDenied),
            DevError::Permission
        );
        assert_eq!(
            map_error(&mtp_rs::Error::StorageFull),
            DevError::StorageFull
        );
        assert_eq!(
            map_error(&mtp_rs::Error::Disconnected),
            DevError::Disconnected
        );
        assert_eq!(map_error(&mtp_rs::Error::StaleHandle), DevError::NotFound);
        assert!(matches!(
            map_error(&mtp_rs::Error::Timeout),
            DevError::Other(_)
        ));
    }

    /// A temp file removed when dropped, on a failing assertion's unwind too.
    struct TempFile(std::path::PathBuf);

    impl TempFile {
        fn new(name: &str) -> Self {
            TempFile(std::env::temp_dir().join(format!("{name}-{}.bin", std::process::id())))
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn the_file_stream_yields_the_whole_file_in_chunks() {
        let file = TempFile::new("method-reader-mtp-stream");
        let path = &file.0;
        let data: Vec<u8> = (0..(CHUNK * 2 + 17)).map(|i| (i % 251) as u8).collect();
        std::fs::write(path, &data).unwrap();
        let chunks: Vec<Bytes> = block_on(futures::StreamExt::collect::<Vec<_>>(file_stream(
            File::open(path).unwrap(),
        )))
        .into_iter()
        .map(|c| c.unwrap())
        .collect();
        assert!(chunks.len() >= 3);
        assert_eq!(chunks.concat(), data);
    }
}
