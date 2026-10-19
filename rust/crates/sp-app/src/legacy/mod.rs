//! What the window reads, once, of the Tauri window it replaced: the
//! update answer that window kept in WebKit's localStorage ([`webkit`]),
//! read without SQLite ([`sqlite`]). Moved here from sp-update when the
//! updater became the suite's method-update: only Method Reader ever had a
//! Tauri window to carry an answer over from.

pub mod sqlite;
pub mod webkit;
