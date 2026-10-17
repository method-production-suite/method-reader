//! The handful of MTP operations a copy needs, behind one small trait, so the
//! copy (put.rs) is tested against a fake without hardware and the real
//! adapter (real.rs) is the only code that talks to mtp-rs.

use std::path::Path;

/// One storage on the device ("Internal Storage" on a Kindle).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageSummary {
    pub id: u64,
    pub description: String,
    pub free_bytes: u64,
    pub capacity_bytes: u64,
    pub writable: bool,
}

/// One child of a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub handle: u64,
    pub name: String,
    pub folder: bool,
}

/// mtp-rs's errors, narrowed to the ones a copy treats differently.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DevError {
    NotFound,
    AccessDenied,
    /// Another process holds the device.
    Exclusive,
    /// The system would not let this process open the device.
    Permission,
    Unsupported,
    StorageFull,
    Disconnected,
    Other(String),
}

impl DevError {
    /// A clause for a sentence: "the Kindle said <this>".
    pub fn describe(&self) -> String {
        match self {
            DevError::NotFound => "it could not find that file or folder".into(),
            DevError::AccessDenied => "it refused to write there".into(),
            DevError::Exclusive => "another program is using it".into(),
            DevError::Permission => "this computer did not allow Method Reader to open it".into(),
            DevError::Unsupported => "it does not support that".into(),
            DevError::StorageFull => "it is full".into(),
            DevError::Disconnected => "it was unplugged or stopped answering".into(),
            DevError::Other(detail) => detail.clone(),
        }
    }
}

/// A failed upload, and the half-written object it may have left behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadFailure {
    pub error: DevError,
    pub partial: Option<u64>,
}

/// An open MTP session on one device. `parent: None` is a storage's root.
pub trait Session {
    fn storages(&mut self) -> Result<Vec<StorageSummary>, DevError>;
    fn children(&mut self, storage: u64, parent: Option<u64>) -> Result<Vec<Entry>, DevError>;
    fn create_folder(
        &mut self,
        storage: u64,
        parent: Option<u64>,
        name: &str,
    ) -> Result<u64, DevError>;
    fn upload(
        &mut self,
        storage: u64,
        parent: Option<u64>,
        name: &str,
        local: &Path,
        size: u64,
    ) -> Result<u64, UploadFailure>;
    fn rename(&mut self, storage: u64, handle: u64, name: &str) -> Result<(), DevError>;
    fn delete(&mut self, storage: u64, handle: u64) -> Result<(), DevError>;
    fn supports_rename(&self) -> bool;
}

/// An in-memory device for tests: a tree of named objects, and switches that
/// make one operation fail.
#[cfg(test)]
pub mod fake {
    use super::*;
    use std::collections::BTreeMap;

    #[derive(Debug, Clone)]
    pub struct Object {
        pub storage: u64,
        pub parent: Option<u64>,
        pub name: String,
        pub folder: bool,
        pub bytes: Vec<u8>,
    }

    #[derive(Debug, Default)]
    pub struct FakeSession {
        pub storages: Vec<StorageSummary>,
        pub objects: BTreeMap<u64, Object>,
        pub next: u64,
        pub can_rename: bool,
        /// Make the next upload fail, leaving a partial object when true.
        pub fail_upload: Option<(DevError, bool)>,
        pub fail_rename: Option<DevError>,
        pub fail_delete_of: Option<String>,
        /// Every operation, in order, for asserting the sequence.
        pub log: Vec<String>,
    }

    impl FakeSession {
        pub fn kindle() -> Self {
            FakeSession {
                storages: vec![StorageSummary {
                    id: 0x10001,
                    description: "Internal Storage".into(),
                    free_bytes: 1_000_000,
                    capacity_bytes: 8_000_000,
                    writable: true,
                }],
                next: 1,
                can_rename: true,
                ..Default::default()
            }
        }

        pub fn add(&mut self, parent: Option<u64>, name: &str, folder: bool, bytes: &[u8]) -> u64 {
            let h = self.next;
            self.next += 1;
            self.objects.insert(
                h,
                Object {
                    storage: 0x10001,
                    parent,
                    name: name.into(),
                    folder,
                    bytes: bytes.to_vec(),
                },
            );
            h
        }

        /// The path of every object, sorted, for whole-tree assertions.
        pub fn tree(&self) -> Vec<String> {
            let mut out: Vec<String> = self
                .objects
                .keys()
                .map(|h| {
                    let mut parts = vec![];
                    let mut at = Some(*h);
                    while let Some(id) = at {
                        let o = &self.objects[&id];
                        parts.push(o.name.clone());
                        at = o.parent;
                    }
                    parts.reverse();
                    let path = parts.join("/");
                    if self.objects[h].folder {
                        format!("{path}/")
                    } else {
                        path
                    }
                })
                .collect();
            out.sort();
            out
        }

        pub fn content(&self, path: &str) -> Option<Vec<u8>> {
            let tree = self.objects.keys().copied().collect::<Vec<_>>();
            for h in tree {
                let mut parts = vec![];
                let mut at = Some(h);
                while let Some(id) = at {
                    parts.push(self.objects[&id].name.clone());
                    at = self.objects[&id].parent;
                }
                parts.reverse();
                if parts.join("/") == path {
                    return Some(self.objects[&h].bytes.clone());
                }
            }
            None
        }
    }

    impl Session for FakeSession {
        fn storages(&mut self) -> Result<Vec<StorageSummary>, DevError> {
            self.log.push("storages".into());
            Ok(self.storages.clone())
        }

        fn children(&mut self, _storage: u64, parent: Option<u64>) -> Result<Vec<Entry>, DevError> {
            self.log.push(format!("children {parent:?}"));
            Ok(self
                .objects
                .iter()
                .filter(|(_, o)| o.parent == parent)
                .map(|(h, o)| Entry {
                    handle: *h,
                    name: o.name.clone(),
                    folder: o.folder,
                })
                .collect())
        }

        fn create_folder(
            &mut self,
            _storage: u64,
            parent: Option<u64>,
            name: &str,
        ) -> Result<u64, DevError> {
            self.log.push(format!("mkdir {name}"));
            Ok(self.add(parent, name, true, &[]))
        }

        fn upload(
            &mut self,
            _storage: u64,
            parent: Option<u64>,
            name: &str,
            local: &Path,
            size: u64,
        ) -> Result<u64, UploadFailure> {
            self.log.push(format!("upload {name}"));
            if let Some((error, partial)) = self.fail_upload.take() {
                let partial = if partial {
                    Some(self.add(parent, name, false, b"half"))
                } else {
                    None
                };
                return Err(UploadFailure { error, partial });
            }
            let bytes = std::fs::read(local).map_err(|e| UploadFailure {
                error: DevError::Other(e.to_string()),
                partial: None,
            })?;
            assert_eq!(
                bytes.len() as u64,
                size,
                "the size handed to upload must be the file's"
            );
            Ok(self.add(parent, name, false, &bytes))
        }

        fn rename(&mut self, _storage: u64, handle: u64, name: &str) -> Result<(), DevError> {
            self.log
                .push(format!("rename {} -> {name}", self.objects[&handle].name));
            if let Some(e) = self.fail_rename.take() {
                return Err(e);
            }
            self.objects
                .get_mut(&handle)
                .ok_or(DevError::NotFound)?
                .name = name.into();
            Ok(())
        }

        fn delete(&mut self, _storage: u64, handle: u64) -> Result<(), DevError> {
            let name = self
                .objects
                .get(&handle)
                .map(|o| o.name.clone())
                .ok_or(DevError::NotFound)?;
            self.log.push(format!("delete {name}"));
            if self.fail_delete_of.as_deref() == Some(name.as_str()) {
                return Err(DevError::AccessDenied);
            }
            self.objects.remove(&handle);
            Ok(())
        }

        fn supports_rename(&self) -> bool {
            self.can_rename
        }
    }
}
