//! Method Reader's file rules, shared by sp-device and sp-engine.
//!
//! - [`atomic`]: write-then-rename for every file the engine keeps
//!   (src/atomic-write.ts).
//! - [`replace`]: the flushed replace a reader's USB copy goes through
//!   (src/replace-file.ts).
//! - [`error`]: a failure in the words Node gave it, because those words
//!   reach a person and the codes are switched on.
//! - [`nodepath`]: Node's `path` functions, because the paths the engine
//!   prints are built by them.

pub mod atomic;
pub mod error;
pub mod nodepath;
pub mod replace;

pub use atomic::{copy_file_atomic, temp_path_for, write_file_atomic};
pub use error::NodeIoError;
pub use replace::{partial_path_for, replace_file, replace_file_with};
