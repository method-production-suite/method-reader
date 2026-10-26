//! Where the server may read and write: inside its roots only.
//!
//! Method Budget's `src/mcp/paths.rs`, with one rule changed: every path a
//! tool is given must be absolute. Budget takes a relative path from its
//! first root; Reader refuses it, so a script is never looked for, or a
//! book written, anywhere but where the caller said.
//!
//! - An absolute path must be inside some root.
//! - An existing file is resolved with every symlink followed
//!   (`canonicalize`), and the real path must still be inside a root, so a
//!   link that leads out is refused.
//! - A file to write must be in an existing folder inside a root, must not
//!   be a symlink or a folder, and must have one of the extensions the tool
//!   writes (`.fountain`, `.epub`...), so the server can't be pointed at a
//!   script, a config file or a git hook.
//! - Nothing below a root whose name starts with `.` (`.git`, `.ssh`, a
//!   hidden file) is read or written.

use std::path::{Component, Path, PathBuf};

use super::limits;
use super::tools::ToolError;

/// Screenplay PDFs.
pub const PDF_EXTS: &[&str] = &["pdf"];
/// What a conversion reads: a PDF, or a Fountain script (`.txt` too, as the
/// command line takes it).
pub const SCRIPT_EXTS: &[&str] = &["pdf", "fountain", "txt"];

/// The folders the server works in, each canonical (symlinks resolved).
#[derive(Clone, Debug)]
pub struct Roots {
    dirs: Vec<PathBuf>,
}

impl Roots {
    /// Canonicalizes `dirs`; each must be an existing folder.
    pub fn new(dirs: &[PathBuf]) -> Result<Roots, String> {
        if dirs.is_empty() {
            return Err("at least one root folder is needed".into());
        }
        let mut out: Vec<PathBuf> = Vec::new();
        for d in dirs {
            let real = std::fs::canonicalize(d).map_err(|e| format!("{}: {e}", d.display()))?;
            if !real.is_dir() {
                return Err(format!("{}: not a folder", d.display()));
            }
            if !out.contains(&real) {
                out.push(real);
            }
        }
        Ok(Roots { dirs: out })
    }

    pub fn dirs(&self) -> &[PathBuf] {
        &self.dirs
    }

    /// `typed` as a path, after basic checks: it must be absolute.
    fn join(&self, typed: &str) -> Result<PathBuf, ToolError> {
        if typed.trim().is_empty() {
            return Err(ToolError::path("the path is empty"));
        }
        if typed.len() > limits::MAX_PATH || typed.contains('\0') {
            return Err(ToolError::path("that is not a usable path"));
        }
        let p = Path::new(typed);
        if !p.is_absolute() {
            return Err(ToolError::path(format!(
                "{typed}: not an absolute path. Give the whole path, from the top of the disk"
            )));
        }
        Ok(p.to_path_buf())
    }

    /// The root `real` (canonical) is inside, refusing hidden names below it.
    fn inside(&self, real: &Path, typed: &str) -> Result<&Path, ToolError> {
        let root = self
            .dirs
            .iter()
            .filter(|r| real.starts_with(r))
            .max_by_key(|r| r.as_os_str().len())
            .ok_or_else(|| {
                ToolError::path(format!(
                    "{typed}: outside the folders this server may use ({})",
                    self.list()
                ))
            })?;
        let rest = real.strip_prefix(root).unwrap_or(real);
        for c in rest.components() {
            match c {
                Component::Normal(n) if n.to_string_lossy().starts_with('.') => {
                    return Err(ToolError::path(format!(
                        "{typed}: hidden files and folders (names starting with `.`) are off limits"
                    )));
                }
                Component::Normal(_) => {}
                _ => return Err(ToolError::path(format!("{typed}: not a plain path"))),
            }
        }
        Ok(root)
    }

    fn list(&self) -> String {
        self.dirs
            .iter()
            .map(|d| d.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// An existing file with one of `exts`, as its real path.
    pub fn existing(&self, typed: &str, exts: &[&str]) -> Result<PathBuf, ToolError> {
        let joined = self.join(typed)?;
        let real = std::fs::canonicalize(&joined).map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ToolError::not_found(format!("{typed}: no such file"))
            } else {
                ToolError::io(format!("{typed}: {e}"))
            }
        })?;
        self.inside(&real, typed)?;
        if !real.is_file() {
            return Err(ToolError::path(format!("{typed}: not a file")));
        }
        check_ext(&real, exts, typed)?;
        Ok(real)
    }

    /// A file to write, with one of `exts`: its folder must exist inside a
    /// root; the file itself may exist (a plain file, not a symlink).
    pub fn target(&self, typed: &str, exts: &[&str]) -> Result<PathBuf, ToolError> {
        let joined = self.join(typed)?;
        let name = joined
            .file_name()
            .ok_or_else(|| ToolError::path(format!("{typed}: needs a file name")))?
            .to_os_string();
        let parent = joined
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let parent_real = std::fs::canonicalize(parent).map_err(|e| {
            ToolError::not_found(format!("{typed}: the folder doesn't exist ({e})"))
        })?;
        if !parent_real.is_dir() {
            return Err(ToolError::path(format!("{typed}: not in a folder")));
        }
        let real = parent_real.join(&name);
        self.inside(&real, typed)?;
        check_ext(&real, exts, typed)?;
        match std::fs::symlink_metadata(&real) {
            Ok(m) if m.file_type().is_symlink() => Err(ToolError::path(format!(
                "{typed}: is a symbolic link; refusing to write through it"
            ))),
            Ok(m) if !m.is_file() => Err(ToolError::path(format!("{typed}: not a file"))),
            _ => Ok(real),
        }
    }

    /// A folder inside a root.
    pub fn folder(&self, typed: &str) -> Result<PathBuf, ToolError> {
        let joined = self.join(typed)?;
        let real = std::fs::canonicalize(&joined)
            .map_err(|e| ToolError::not_found(format!("{typed}: {e}")))?;
        self.inside(&real, typed)?;
        if !real.is_dir() {
            return Err(ToolError::path(format!("{typed}: not a folder")));
        }
        Ok(real)
    }

    /// How a real path is shown back: absolute, as the system writes it, so
    /// it can be passed back as is.
    pub fn show(&self, real: &Path) -> String {
        real.display().to_string()
    }
}

fn check_ext(real: &Path, exts: &[&str], typed: &str) -> Result<(), ToolError> {
    let ok = real
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| exts.iter().any(|x| e.eq_ignore_ascii_case(x)));
    if ok {
        Ok(())
    } else {
        let list: Vec<String> = exts.iter().map(|e| format!(".{e}")).collect();
        Err(ToolError::path(format!(
            "{typed}: this tool only works with {} files",
            list.join(", ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch folder, removed when dropped.
    pub fn temp(name: &str) -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix(&format!("method-reader-mcp-paths-{name}-"))
            .tempdir()
            .unwrap()
    }

    #[test]
    fn paths_stay_inside_the_roots() {
        let scratch = temp("inside");
        let base = scratch.path().to_path_buf();
        let root = base.join("root");
        let outside = base.join("outside");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(root.join("a.pdf"), "x").unwrap();
        std::fs::write(root.join("sub/b.pdf"), "x").unwrap();
        std::fs::write(root.join(".git/c.pdf"), "x").unwrap();
        std::fs::write(root.join("notes.txt"), "x").unwrap();
        std::fs::write(outside.join("o.pdf"), "x").unwrap();
        let roots = Roots::new(std::slice::from_ref(&root)).unwrap();
        let real_root = std::fs::canonicalize(&root).unwrap();
        let abs = |rel: &str| real_root.join(rel).to_string_lossy().into_owned();
        let p = PDF_EXTS;

        assert!(roots.existing(&abs("a.pdf"), p).unwrap().ends_with("a.pdf"));
        assert!(roots.existing(&abs("sub/b.pdf"), p).is_ok());
        assert_eq!(
            roots.show(&roots.existing(&abs("sub/b.pdf"), p).unwrap()),
            abs("sub/b.pdf")
        );

        let refused = |r: Result<PathBuf, ToolError>, want: &str| {
            let e = r.unwrap_err();
            assert!(e.message.contains(want), "{}", e.message);
        };
        // Budget would take these from its first root; Reader never does.
        refused(roots.existing("a.pdf", p), "not an absolute path");
        refused(roots.existing("sub/b.pdf", p), "not an absolute path");
        refused(
            roots.target("new.fountain", &["fountain"]),
            "not an absolute",
        );
        refused(roots.folder("sub"), "not an absolute path");
        refused(
            roots.existing(&abs("../outside/o.pdf"), p),
            "outside the folders",
        );
        refused(
            roots.existing(outside.join("o.pdf").to_str().unwrap(), p),
            "outside",
        );
        refused(roots.existing(&abs(".git/c.pdf"), p), "hidden");
        refused(
            roots.existing(&abs("notes.txt"), p),
            "only works with .pdf files",
        );
        refused(roots.existing(&abs("missing.pdf"), p), "no such file");
        refused(roots.existing("", p), "empty");
        refused(roots.existing(&abs("a\0.pdf"), p), "not a usable path");
        let f = &["fountain"];
        refused(roots.target(&abs("../outside/new.fountain"), f), "outside");
        refused(
            roots.target(&abs("nofolder/new.fountain"), f),
            "folder doesn't exist",
        );
        refused(roots.target(&abs("evil.sh"), f), "only works with");
        refused(roots.target(&abs(".hidden.fountain"), f), "hidden");
        refused(roots.target(&abs("sub"), f), "only works with");
        assert!(
            roots
                .target(&abs("sub/new.fountain"), f)
                .unwrap()
                .ends_with("sub/new.fountain")
        );
        assert!(roots.folder(&abs("sub")).is_ok());
        refused(roots.folder(&abs("../outside")), "outside");
        refused(roots.folder(&abs("a.pdf")), "not a folder");
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_that_leave_are_refused() {
        use std::os::unix::fs::symlink;
        let scratch = temp("links");
        let base = scratch.path().to_path_buf();
        let root = base.join("root");
        let outside = base.join("outside");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("o.pdf"), "x").unwrap();
        std::fs::write(root.join("in.pdf"), "x").unwrap();
        std::fs::write(root.join("in.epub"), "x").unwrap();
        symlink(outside.join("o.pdf"), root.join("link.pdf")).unwrap();
        symlink(&outside, root.join("dirlink")).unwrap();
        symlink(root.join("in.pdf"), root.join("inner.pdf")).unwrap();
        symlink(root.join("in.epub"), root.join("inner.epub")).unwrap();
        symlink(base.join("nowhere.epub"), root.join("dangling.epub")).unwrap();
        let roots = Roots::new(std::slice::from_ref(&root)).unwrap();
        let real_root = std::fs::canonicalize(&root).unwrap();
        let abs = |rel: &str| real_root.join(rel).to_string_lossy().into_owned();
        let p = PDF_EXTS;
        for rel in ["link.pdf", "dirlink/o.pdf"] {
            let e = roots.existing(&abs(rel), p).unwrap_err();
            assert!(e.message.contains("outside"), "{rel}: {}", e.message);
        }
        let e = &["epub"];
        assert!(
            roots
                .target(&abs("dirlink/new.epub"), e)
                .unwrap_err()
                .message
                .contains("outside")
        );
        // A link that stays inside resolves to the real file, which is fine to
        // read, but nothing is ever written through a link.
        assert!(
            roots
                .existing(&abs("inner.pdf"), p)
                .unwrap()
                .ends_with("in.pdf")
        );
        for rel in ["inner.epub", "dangling.epub"] {
            let err = roots.target(&abs(rel), e).unwrap_err();
            assert!(
                err.message.contains("symbolic link"),
                "{rel}: {}",
                err.message
            );
        }
    }
}
