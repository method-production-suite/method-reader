//! The engine finds Calibre where it is installed.
//!
//! Every test that runs the real Calibre skips on a machine without it, so a
//! Calibre the engine could no longer FIND would leave the weekly Calibre
//! job (.github/workflows/weekly-toolchain.yml) green while testing nothing.
//! That job sets METHOD_READER_REQUIRE_CALIBRE=1 (or, until the workflow
//! moves to the new name, SCREEPUB_REQUIRE_CALIBRE=1) after installing
//! Calibre, and
//! this fails unless the engine's own discovery rule finds each tool. It
//! only looks for the files: nothing here starts Calibre.

mod common;

#[test]
fn the_engine_finds_each_calibre_tool_where_calibre_is_required() {
    if std::env::var_os("METHOD_READER_REQUIRE_CALIBRE").is_none()
        && std::env::var_os("SCREEPUB_REQUIRE_CALIBRE").is_none()
    {
        eprintln!("skipped: METHOD_READER_REQUIRE_CALIBRE is not set");
        return;
    }
    let env = common::guarded_env();
    for tool in ["ebook-convert", "calibre-customize", "calibre-debug"] {
        let found = sp_engine::export::calibre::calibre_tool(tool, &env);
        assert!(found.is_some(), "the engine cannot find {tool}");
        eprintln!("{tool}: {}", found.unwrap());
    }
}
