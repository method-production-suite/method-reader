//! Copying one file onto a Kindle, replacing an earlier copy of it.
//!
//! MTP cannot overwrite, and cannot rename one object over another. So:
//!
//!  1. find or make each folder of the path (`documents`);
//!  2. remove a temp file an earlier, failed copy left behind;
//!  3. upload under the temp name (paths::temp_name);
//!  4. only then delete the old copy, and rename the temp file into place.
//!
//! A failed upload costs nothing: its partial object is deleted and the old
//! book is untouched. The old copy goes only once the new one is wholly on
//! the device.
//!
//! A device that cannot rename gets delete-then-upload under the real name
//! instead ("replace"). There a failed upload DOES cost the old copy, which
//! is why it is the fallback and why the answer says which way it went.
//! Whether a Kindle can rename is one of hardware day's questions.

use crate::output::{Code, Failure};
use crate::paths::{DevicePath, temp_name};
use crate::session::{DevError, Session, StorageSummary};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Rename,
    Replace,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Rename => "rename",
            Method::Replace => "replace",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// Where it landed, e.g. "documents/Title.kfx".
    pub path: String,
    pub bytes: u64,
    pub method: Method,
    /// The storage's description, e.g. "Internal Storage".
    pub storage: String,
    /// True when an earlier copy was there and is now replaced.
    pub replaced: bool,
}

/// A device failure in words, with the code the engine switches on.
pub fn device_failure(doing: &str, error: &DevError) -> Failure {
    let code = match error {
        DevError::Exclusive => Code::Busy,
        DevError::Permission => Code::Permission,
        DevError::StorageFull => Code::StorageFull,
        _ => Code::CopyFailed,
    };
    Failure::new(
        code,
        format!("could not {doing}: the Kindle said {}", error.describe()),
    )
}

fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.0)
}

/// The storage a book goes on: the first one that can be written to. A Kindle
/// has exactly one ("Internal Storage").
fn choose_storage(storages: &[StorageSummary]) -> Result<StorageSummary, Failure> {
    storages.iter().find(|s| s.writable).cloned().ok_or_else(|| {
        Failure::new(
            Code::NoStorage,
            if storages.is_empty() {
                "the Kindle offers no storage to copy to. Unlock it, and check it is not in charging-only mode".to_string()
            } else {
                "the Kindle's storage is read-only".to_string()
            },
        )
    })
}

/// The folder called `name` under `parent`, made if it is not there. An
/// exact match wins; otherwise one that differs only in case (a Kindle's
/// file system does not care, and a second "Documents" beside "documents"
/// would be invisible to it).
fn folder(
    session: &mut dyn Session,
    storage: u64,
    parent: Option<u64>,
    name: &str,
    whole: &str,
) -> Result<u64, Failure> {
    let kids = session
        .children(storage, parent)
        .map_err(|e| device_failure(&format!("look inside the folder for {whole}"), &e))?;
    if let Some(e) = kids.iter().find(|e| e.folder && e.name == name) {
        return Ok(e.handle);
    }
    if let Some(e) = kids
        .iter()
        .find(|e| e.folder && e.name.eq_ignore_ascii_case(name))
    {
        return Ok(e.handle);
    }
    if kids
        .iter()
        .any(|e| !e.folder && e.name.eq_ignore_ascii_case(name))
    {
        return Err(Failure::new(
            Code::BadPath,
            format!("\"{name}\" on the Kindle is a file, not a folder"),
        ));
    }
    session
        .create_folder(storage, parent, name)
        .map_err(|e| device_failure(&format!("make the folder \"{name}\""), &e))
}

/// Delete, ignoring failure: for clean-up after something else already
/// failed, whose error is the one worth reporting.
fn tidy(session: &mut dyn Session, storage: u64, handle: Option<u64>) {
    if let Some(h) = handle {
        let _ = session.delete(storage, h);
    }
}

pub fn put(
    session: &mut dyn Session,
    local: &Path,
    size: u64,
    dest: &DevicePath,
) -> Result<Outcome, Failure> {
    let whole = dest.display();
    let storages = session
        .storages()
        .map_err(|e| device_failure("read the Kindle's storage", &e))?;
    let storage = choose_storage(&storages)?;
    if storage.free_bytes < size {
        return Err(Failure::new(
            Code::StorageFull,
            format!(
                "the Kindle has {} free and the book is {}. Remove something from it and try again",
                megabytes(storage.free_bytes),
                megabytes(size)
            ),
        ));
    }
    let sid = storage.id;

    let mut parent: Option<u64> = None;
    for name in &dest.folders {
        parent = Some(folder(session, sid, parent, name, &whole)?);
    }

    let kids = session
        .children(sid, parent)
        .map_err(|e| device_failure(&format!("look inside the folder for {whole}"), &e))?;
    if kids.iter().any(|e| e.folder && e.name == dest.name) {
        return Err(Failure::new(
            Code::BadPath,
            format!("\"{whole}\" on the Kindle is a folder, not a file"),
        ));
    }
    let existing: Vec<u64> = kids
        .iter()
        .filter(|e| !e.folder && e.name == dest.name)
        .map(|e| e.handle)
        .collect();
    let temp = temp_name(&dest.name);
    for leftover in kids.iter().filter(|e| !e.folder && e.name == temp) {
        session
            .delete(sid, leftover.handle)
            .map_err(|e| device_failure("remove a copy an earlier attempt left unfinished", &e))?;
    }

    let done = |method: Method| Outcome {
        path: whole.clone(),
        bytes: size,
        method,
        storage: storage.description.clone(),
        replaced: !existing.is_empty(),
    };

    if session.supports_rename() {
        let new = session
            .upload(sid, parent, &temp, local, size)
            .map_err(|f| {
                tidy(session, sid, f.partial);
                device_failure(
                    &format!("copy the book to {whole}. Nothing on the Kindle was changed"),
                    &f.error,
                )
            })?;
        for old in &existing {
            if let Err(e) = session.delete(sid, *old) {
                tidy(session, sid, Some(new));
                return Err(device_failure(
                    &format!("replace the copy already at {whole}. It is still there, unchanged"),
                    &e,
                ));
            }
        }
        match session.rename(sid, new, &dest.name) {
            Ok(()) => return Ok(done(Method::Rename)),
            Err(_) => {
                // Advertised and then refused: fall back to a plain upload
                // under the real name, then remove the temp copy.
                return match session.upload(sid, parent, &dest.name, local, size) {
                    Ok(_) => {
                        tidy(session, sid, Some(new));
                        Ok(done(Method::Replace))
                    }
                    Err(f) => {
                        tidy(session, sid, f.partial);
                        Err(device_failure(
                            &format!(
                                "finish the copy to {whole}. The earlier copy was removed and the new one is on the Kindle under the hidden name {temp}"
                            ),
                            &f.error,
                        ))
                    }
                };
            }
        }
    }

    for old in &existing {
        session.delete(sid, *old).map_err(|e| {
            device_failure(
                &format!("replace the copy already at {whole}. It is still there, unchanged"),
                &e,
            )
        })?;
    }
    session
        .upload(sid, parent, &dest.name, local, size)
        .map_err(|f| {
            tidy(session, sid, f.partial);
            let lost = if existing.is_empty() {
                ""
            } else {
                ". The earlier copy had already been removed"
            };
            device_failure(&format!("copy the book to {whole}{lost}"), &f.error)
        })?;
    Ok(done(Method::Replace))
}

/// What is in one folder on the device, for a person checking a copy by
/// hand (`ls`). Reads only: an absent folder is `bad-path`, never made.
/// `folder` is a path like "documents", or None for the storage's top level.
pub fn list_folder(
    session: &mut dyn Session,
    folder: Option<&str>,
) -> Result<(String, Vec<crate::session::Entry>), Failure> {
    let storages = session
        .storages()
        .map_err(|e| device_failure("read the Kindle's storage", &e))?;
    let storage = storages
        .first()
        .cloned()
        .ok_or_else(|| Failure::new(Code::NoStorage, "the Kindle offers no storage"))?;
    let mut parent: Option<u64> = None;
    let parts: Vec<&str> = folder
        .map(|f| f.split('/').filter(|p| !p.is_empty()).collect())
        .unwrap_or_default();
    for part in &parts {
        let kids = session
            .children(storage.id, parent)
            .map_err(|e| device_failure("look inside a folder", &e))?;
        let found = kids
            .iter()
            .find(|e| e.folder && e.name == *part)
            .or_else(|| {
                kids.iter()
                    .find(|e| e.folder && e.name.eq_ignore_ascii_case(part))
            })
            .ok_or_else(|| {
                Failure::new(
                    Code::BadPath,
                    format!("there is no folder \"{part}\" on the Kindle"),
                )
            })?;
        parent = Some(found.handle);
    }
    let mut kids = session
        .children(storage.id, parent)
        .map_err(|e| device_failure("look inside a folder", &e))?;
    kids.sort_by(|a, b| a.name.cmp(&b.name));
    Ok((storage.description, kids))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::parse_device_path;
    use crate::session::fake::FakeSession;
    use std::io::Write;

    struct Book {
        path: std::path::PathBuf,
        size: u64,
    }

    impl Drop for Book {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }

    fn book(bytes: &[u8]) -> Book {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "method-reader-mtp-put-{}-{n}.kfx",
            std::process::id()
        ));
        std::fs::File::create(&path)
            .unwrap()
            .write_all(bytes)
            .unwrap();
        Book {
            path,
            size: bytes.len() as u64,
        }
    }

    fn dest(s: &str) -> DevicePath {
        parse_device_path(s).unwrap()
    }

    #[test]
    fn a_new_book_goes_up_under_the_temp_name_and_is_renamed_into_place() {
        let mut s = FakeSession::kindle();
        let docs = s.add(None, "documents", true, &[]);
        let b = book(b"NEW");
        let out = put(&mut s, &b.path, b.size, &dest("documents/Title.kfx")).unwrap();
        assert_eq!(out.method, Method::Rename);
        assert!(!out.replaced);
        assert_eq!(out.path, "documents/Title.kfx");
        assert_eq!(out.storage, "Internal Storage");
        assert_eq!(s.tree(), vec!["documents/", "documents/Title.kfx"]);
        assert_eq!(s.content("documents/Title.kfx").unwrap(), b"NEW");
        assert!(
            s.objects
                .values()
                .all(|o| o.parent != Some(docs) || o.name == "Title.kfx")
        );
        assert_eq!(
            s.log
                .iter()
                .filter(|l| l.starts_with("upload") || l.starts_with("rename"))
                .collect::<Vec<_>>(),
            vec![
                "upload .Title.kfx.method-reader-part",
                "rename .Title.kfx.method-reader-part -> Title.kfx"
            ]
        );
    }

    #[test]
    fn the_old_copy_is_deleted_only_after_the_new_one_is_wholly_up() {
        let mut s = FakeSession::kindle();
        let docs = s.add(None, "documents", true, &[]);
        s.add(Some(docs), "Title.kfx", false, b"OLD");
        let b = book(b"NEW");
        let out = put(&mut s, &b.path, b.size, &dest("documents/Title.kfx")).unwrap();
        assert!(out.replaced);
        assert_eq!(s.content("documents/Title.kfx").unwrap(), b"NEW");
        let upload = s.log.iter().position(|l| l.starts_with("upload")).unwrap();
        let delete_old = s.log.iter().position(|l| l == "delete Title.kfx").unwrap();
        assert!(upload < delete_old, "{:?}", s.log);
    }

    #[test]
    fn a_failed_upload_leaves_the_old_book_and_no_partial() {
        let mut s = FakeSession::kindle();
        let docs = s.add(None, "documents", true, &[]);
        s.add(Some(docs), "Title.kfx", false, b"OLD");
        s.fail_upload = Some((DevError::Disconnected, true));
        let b = book(b"NEW");
        let err = put(&mut s, &b.path, b.size, &dest("documents/Title.kfx")).unwrap_err();
        assert_eq!(err.code, Code::CopyFailed);
        assert!(
            err.message.contains("Nothing on the Kindle was changed"),
            "{}",
            err.message
        );
        assert_eq!(s.tree(), vec!["documents/", "documents/Title.kfx"]);
        assert_eq!(s.content("documents/Title.kfx").unwrap(), b"OLD");
    }

    #[test]
    fn a_missing_documents_folder_is_made() {
        let mut s = FakeSession::kindle();
        let b = book(b"NEW");
        put(&mut s, &b.path, b.size, &dest("documents/Title.kfx")).unwrap();
        assert_eq!(s.tree(), vec!["documents/", "documents/Title.kfx"]);
    }

    #[test]
    fn a_folder_that_differs_only_in_case_is_used_not_doubled() {
        let mut s = FakeSession::kindle();
        s.add(None, "Documents", true, &[]);
        let b = book(b"NEW");
        put(&mut s, &b.path, b.size, &dest("documents/Title.kfx")).unwrap();
        assert_eq!(s.tree(), vec!["Documents/", "Documents/Title.kfx"]);
    }

    #[test]
    fn a_leftover_temp_file_from_a_crash_is_removed_first() {
        let mut s = FakeSession::kindle();
        let docs = s.add(None, "documents", true, &[]);
        s.add(Some(docs), ".Title.kfx.method-reader-part", false, b"half");
        let b = book(b"NEW");
        put(&mut s, &b.path, b.size, &dest("documents/Title.kfx")).unwrap();
        assert_eq!(s.tree(), vec!["documents/", "documents/Title.kfx"]);
    }

    #[test]
    fn without_rename_it_is_delete_then_upload_and_says_so() {
        let mut s = FakeSession::kindle();
        s.can_rename = false;
        let docs = s.add(None, "documents", true, &[]);
        s.add(Some(docs), "Title.kfx", false, b"OLD");
        let b = book(b"NEW");
        let out = put(&mut s, &b.path, b.size, &dest("documents/Title.kfx")).unwrap();
        assert_eq!(out.method, Method::Replace);
        assert_eq!(s.content("documents/Title.kfx").unwrap(), b"NEW");
        assert!(
            !s.log.iter().any(|l| l.contains("method-reader-part")),
            "{:?}",
            s.log
        );
    }

    #[test]
    fn without_rename_a_failed_upload_says_the_old_copy_is_gone() {
        let mut s = FakeSession::kindle();
        s.can_rename = false;
        let docs = s.add(None, "documents", true, &[]);
        s.add(Some(docs), "Title.kfx", false, b"OLD");
        s.fail_upload = Some((DevError::Other("I/O error: pipe".into()), true));
        let b = book(b"NEW");
        let err = put(&mut s, &b.path, b.size, &dest("documents/Title.kfx")).unwrap_err();
        assert!(
            err.message
                .contains("earlier copy had already been removed"),
            "{}",
            err.message
        );
        assert_eq!(s.tree(), vec!["documents/"]);
    }

    #[test]
    fn a_refused_rename_falls_back_to_a_plain_upload() {
        let mut s = FakeSession::kindle();
        s.fail_rename = Some(DevError::Unsupported);
        let b = book(b"NEW");
        let out = put(&mut s, &b.path, b.size, &dest("documents/Title.kfx")).unwrap();
        assert_eq!(out.method, Method::Replace);
        assert_eq!(s.tree(), vec!["documents/", "documents/Title.kfx"]);
    }

    #[test]
    fn an_old_copy_that_cannot_be_deleted_stays_and_the_new_temp_goes() {
        let mut s = FakeSession::kindle();
        let docs = s.add(None, "documents", true, &[]);
        s.add(Some(docs), "Title.kfx", false, b"OLD");
        s.fail_delete_of = Some("Title.kfx".into());
        let b = book(b"NEW");
        let err = put(&mut s, &b.path, b.size, &dest("documents/Title.kfx")).unwrap_err();
        assert!(
            err.message.contains("still there, unchanged"),
            "{}",
            err.message
        );
        assert_eq!(s.tree(), vec!["documents/", "documents/Title.kfx"]);
        assert_eq!(s.content("documents/Title.kfx").unwrap(), b"OLD");
    }

    #[test]
    fn too_little_room_is_refused_before_anything_is_written() {
        let mut s = FakeSession::kindle();
        s.storages[0].free_bytes = 2;
        let b = book(b"NEW");
        let err = put(&mut s, &b.path, b.size, &dest("documents/Title.kfx")).unwrap_err();
        assert_eq!(err.code, Code::StorageFull);
        assert!(s.objects.is_empty());
    }

    #[test]
    fn no_storage_or_only_read_only_storage() {
        let mut s = FakeSession::kindle();
        s.storages.clear();
        let b = book(b"NEW");
        assert_eq!(
            put(&mut s, &b.path, b.size, &dest("a.kfx"))
                .unwrap_err()
                .code,
            Code::NoStorage
        );
        let mut s = FakeSession::kindle();
        s.storages[0].writable = false;
        let err = put(&mut s, &b.path, b.size, &dest("a.kfx")).unwrap_err();
        assert_eq!(err.code, Code::NoStorage);
        assert!(err.message.contains("read-only"));
    }

    #[test]
    fn a_file_where_a_folder_should_be_or_a_folder_where_the_book_goes() {
        let mut s = FakeSession::kindle();
        s.add(None, "documents", false, b"x");
        let b = book(b"NEW");
        assert_eq!(
            put(&mut s, &b.path, b.size, &dest("documents/Title.kfx"))
                .unwrap_err()
                .code,
            Code::BadPath
        );
        let mut s = FakeSession::kindle();
        let docs = s.add(None, "documents", true, &[]);
        s.add(Some(docs), "Title.kfx", true, &[]);
        assert_eq!(
            put(&mut s, &b.path, b.size, &dest("documents/Title.kfx"))
                .unwrap_err()
                .code,
            Code::BadPath
        );
    }

    #[test]
    fn ls_reads_a_folder_and_never_makes_one() {
        let mut s = FakeSession::kindle();
        let docs = s.add(None, "documents", true, &[]);
        s.add(Some(docs), "B.kfx", false, b"b");
        s.add(Some(docs), "A.azw3", false, b"a");
        let (storage, kids) = list_folder(&mut s, Some("documents")).unwrap();
        assert_eq!(storage, "Internal Storage");
        assert_eq!(
            kids.iter().map(|k| k.name.as_str()).collect::<Vec<_>>(),
            vec!["A.azw3", "B.kfx"]
        );
        let (_, top) = list_folder(&mut s, None).unwrap();
        assert_eq!(top.len(), 1);
        assert_eq!(
            list_folder(&mut s, Some("system")).unwrap_err().code,
            Code::BadPath
        );
        assert!(
            !s.log.iter().any(|l| l.starts_with("mkdir")
                || l.starts_with("upload")
                || l.starts_with("delete"))
        );
    }

    #[test]
    fn device_errors_map_to_the_engine_s_codes() {
        assert_eq!(device_failure("x", &DevError::Exclusive).code, Code::Busy);
        assert_eq!(
            device_failure("x", &DevError::Permission).code,
            Code::Permission
        );
        assert_eq!(
            device_failure("x", &DevError::StorageFull).code,
            Code::StorageFull
        );
        assert_eq!(
            device_failure("x", &DevError::Disconnected).code,
            Code::CopyFailed
        );
        assert!(
            device_failure("copy it", &DevError::Disconnected)
                .message
                .contains("unplugged")
        );
    }
}
