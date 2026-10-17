//! Port of tests/export-kfx.test.ts: the KFX toolchain probe, the KFX
//! conversion (against stand-in tools), the plugin listing parse, and the
//! plugin install (its Python run for real under python3 against stub
//! calibre modules).
//!
//! CALIBRE SPAWNS: [`real_status`] runs `calibre-customize --list-plugins`
//! ONCE per run of this file, and the header check runs it once more, both
//! with CALIBRE_CONFIG_DIRECTORY pointed at the scratch copy
//! (common::calibre_env). Nothing else here starts Calibre.

#![cfg_attr(not(unix), allow(dead_code, unused_imports))]

mod common;

use std::sync::OnceLock;

use common::kfx_stubs::{SnippetRun, scenario, system_python};
use common::{Scratch, calibre_env, guarded_env};
use serde_json::json;
use sp_engine::export::calibre::{CALIBRE_FORMAT_GUARDS, TimedRun, calibre_tool, run_with_timeout};
use sp_engine::export::kfx::{
    KfxDeps, KfxInstallResult, KfxStatus, PLUGIN_INSTALL_TIMEOUT_MS, PLUGIN_LIST_TIMEOUT_MS,
    compute_ready, debug_runner, install_kfx_plugin, kfx_scratch_path, kfx_sibling, kfx_status,
    lists_kfx_output, plugin_installed, plugin_table_header, previewer_path, to_kfx,
};

/// This machine's KFX status, asked once, of the scratch COPY of Calibre's
/// settings.
fn real_status() -> KfxStatus {
    static STATUS: OnceLock<KfxStatus> = OnceLock::new();
    *STATUS.get_or_init(|| kfx_status(&calibre_env()))
}

const READY: KfxStatus = KfxStatus {
    calibre: true,
    previewer: true,
    plugin_installed: true,
    ready: true,
};

#[test]
fn previewer_is_never_found_on_linux() {
    if cfg!(target_os = "linux") {
        assert_eq!(previewer_path(&guarded_env()), None);
    }
}

#[test]
fn status_ready_requires_all_three_pieces() {
    let s = real_status();
    assert_eq!(s.ready, s.calibre && s.previewer && s.plugin_installed);
}

#[test]
fn compute_ready_is_the_three_way_and() {
    assert!(compute_ready(true, true, true));
    assert!(!compute_ready(false, true, true));
    assert!(!compute_ready(true, false, true));
    assert!(!compute_ready(true, true, false));
}

#[test]
fn status_calibre_agrees_with_discovery() {
    assert_eq!(
        real_status().calibre,
        calibre_tool("calibre-customize", &calibre_env()).is_some()
    );
}

#[test]
fn plugin_installed_is_false_whenever_calibre_is_absent() {
    let s = real_status();
    if !s.calibre {
        assert!(!s.plugin_installed);
    }
}

#[test]
fn on_linux_the_kfx_rung_is_never_ready() {
    if cfg!(target_os = "linux") {
        assert!(!real_status().ready);
    }
}

#[test]
fn to_kfx_refuses_outright_when_calibre_is_absent() {
    let deps = KfxDeps {
        tool: Some(Box::new(|| None)),
        ..Default::default()
    };
    let err = to_kfx(
        "/nowhere/nonexistent.epub",
        &mut |_| {},
        &deps,
        &guarded_env(),
    )
    .unwrap_err();
    assert!(err.message().contains("ebook-convert was not found"));
}

#[test]
fn kfx_sibling_is_same_folder_same_stem() {
    assert_eq!(kfx_sibling("/tmp/book.epub"), "/tmp/book.kfx");
    assert_eq!(kfx_sibling("/tmp/Book.EPUB"), "/tmp/Book.kfx");
}

#[cfg(unix)]
#[test]
fn the_scratch_path_is_hidden_same_directory_and_keeps_kfx() {
    assert_eq!(
        kfx_scratch_path("/tmp/dir/Book.EPUB"),
        "/tmp/dir/.Book.partial.kfx"
    );
}

fn ready_deps(tool: String) -> KfxDeps<'static> {
    KfxDeps {
        tool: Some(Box::new(move || Some(tool.clone()))),
        status: Some(Box::new(|| READY)),
        timeout_ms: None,
    }
}

#[cfg(unix)]
fn fake_ebook_convert(s: &Scratch) -> (String, String) {
    let tool = s.join("ebook-convert");
    let log = s.join("argv.log");
    common::executable(
        &tool,
        format!("#!/bin/sh\nfor a in \"$@\"; do echo \"$a\"; done > \"{log}\"\ntouch \"$2\"\n"),
    );
    (tool, log)
}

#[cfg(unix)]
#[test]
fn to_kfx_writes_a_kfx_scratch_and_renames_it_onto_the_sibling() {
    let s = Scratch::new("export-kfx");
    let (tool, log) = fake_ebook_convert(&s);
    let epub = s.join("book.epub");
    std::fs::write(&epub, "fake epub bytes").unwrap();
    std::fs::write(kfx_sibling(&epub), "an older conversion").unwrap();
    let mut stages = Vec::new();
    let out = to_kfx(
        &epub,
        &mut |st| stages.push(st.to_owned()),
        &ready_deps(tool),
        &guarded_env(),
    )
    .unwrap();
    assert_eq!(out, kfx_sibling(&epub));
    assert_eq!(common::read(&out), "");
    let argv: Vec<String> = common::read(&log).lines().map(str::to_owned).collect();
    assert_eq!(argv[0], epub);
    assert!(argv[1].ends_with(".kfx"));
    assert_eq!(argv[1], kfx_scratch_path(&epub));
    assert_eq!(argv[2..], CALIBRE_FORMAT_GUARDS.map(str::to_owned));
    assert!(!std::path::Path::new(&kfx_scratch_path(&epub)).exists());
    assert_eq!(
        stages,
        vec!["converting to KFX (Kindle Previewer can take ~20s to start)…"]
    );
}

#[cfg(unix)]
#[test]
fn to_kfx_refuses_when_not_ready_naming_what_is_missing() {
    let s = Scratch::new("export-kfx");
    let (tool, _) = fake_ebook_convert(&s);
    let epub = s.join("book.epub");
    std::fs::write(&epub, "fake epub bytes").unwrap();
    let deps = KfxDeps {
        tool: Some(Box::new(move || Some(tool.clone()))),
        status: Some(Box::new(|| KfxStatus {
            calibre: true,
            previewer: false,
            plugin_installed: false,
            ready: false,
        })),
        timeout_ms: None,
    };
    let err = to_kfx(&epub, &mut |_| {}, &deps, &guarded_env()).unwrap_err();
    assert_eq!(err.name(), "KfxToolchainNotReadyError");
    assert_eq!(
        err.message(),
        "KFX conversion needs Kindle Previewer and the KFX plugin."
    );
    assert!(!std::path::Path::new(&kfx_sibling(&epub)).exists());
}

/// A fake that does to whatever $TMPDIR it gets what Kindle Previewer does,
/// and writes down which folder that was.
#[cfg(unix)]
fn leaky(s: &Scratch, exit: i32) -> (String, String, String) {
    let tool = s.join("ebook-convert");
    let seen = s.join("tmpdir.txt");
    let leftover = format!("leftover-{}", std::process::id());
    let last = if exit == 0 {
        "touch \"$2\"".to_owned()
    } else {
        format!("echo 'Kindle Previewer failed' >&2; exit {exit}")
    };
    common::executable(
        &tool,
        format!(
            "#!/bin/sh\ndir=\"${{TMPDIR:-/tmp}}\"\nprintf '%s' \"$dir\" > \"{seen}\"\nmkdir -p \"$dir/{leftover}/conv_out\"\necho '\"Type\",\"Description\"' > \"$dir/{leftover}/conversionLog.csv\"\n{last}\n"
        ),
    );
    let epub = s.join("book.epub");
    std::fs::write(&epub, "fake epub bytes").unwrap();
    (tool, epub, seen)
}

#[cfg(unix)]
#[test]
fn previewer_gets_its_own_temp_folder_and_it_is_removed() {
    let s = Scratch::new("export-kfx");
    let (tool, epub, seen) = leaky(&s, 0);
    let out = to_kfx(&epub, &mut |_| {}, &ready_deps(tool), &guarded_env()).unwrap();
    assert!(std::path::Path::new(&out).exists());
    let dir = common::read(&seen);
    assert!(!dir.is_empty());
    assert!(!std::path::Path::new(&dir).exists());
}

#[cfg(unix)]
#[test]
fn a_failed_conversion_removes_previewers_temp_folder_too() {
    let s = Scratch::new("export-kfx");
    let (tool, epub, seen) = leaky(&s, 1);
    let err = to_kfx(&epub, &mut |_| {}, &ready_deps(tool), &guarded_env()).unwrap_err();
    assert!(err.message().contains("Kindle Previewer failed"));
    assert!(!std::path::Path::new(&common::read(&seen)).exists());
}

// ---- listsKfxOutput ----

struct Table {
    type_width: usize,
    name_width: usize,
}

impl Table {
    fn line(&self, kind: &str, name: &str, version: &str, disabled: &str) -> String {
        format!(
            "{kind:<tw$}{name:<nw$}{version:<15}{disabled:<15}",
            tw = self.type_width,
            nw = self.name_width
        )
    }
    fn header(&self) -> String {
        format!(
            "{:<tw$}{:<nw$}{:<15}{:<15}Site Customization",
            "Type",
            "Name",
            "Version",
            "Disabled",
            tw = self.type_width,
            nw = self.name_width
        )
    }
    fn kfx_output(&self) -> Vec<String> {
        vec![
            self.line("Conversion output", "KFX Output", "(2, 20, 1)", "False"),
            "\t Convert e-books to the KFX format".into(),
            String::new(),
        ]
    }
    fn companion(&self) -> Vec<String> {
        vec![
            self.line(
                "Metadata writer",
                "Set KFX metadata (from KFX Output)",
                "(2, 20, 1)",
                "False",
            ),
            "\t Set metadata in KFX files".into(),
            String::new(),
        ]
    }
    fn epub_output(&self) -> Vec<String> {
        vec![
            self.line("Conversion output", "EPUB Output", "(1, 0, 0)", "False"),
            "\t Convert e-books to the EPUB format".into(),
            String::new(),
        ]
    }
    fn listing(&self, rows: &[Vec<String>]) -> String {
        let mut lines = vec![self.header(), String::new()];
        for r in rows {
            lines.extend(r.iter().cloned());
        }
        lines.join("\n")
    }
}

const MEASURED: Table = Table {
    type_width: 22,
    name_width: 54,
};

#[test]
fn the_companion_alone_is_not_the_plugin() {
    let m = &MEASURED;
    assert!(!lists_kfx_output(
        &m.listing(&[m.epub_output(), m.companion()])
    ));
}

#[test]
fn both_plugins_is_installed_and_the_converter_alone_is_too() {
    let m = &MEASURED;
    assert!(lists_kfx_output(&m.listing(&[
        m.kfx_output(),
        m.companion(),
        m.epub_output()
    ])));
    assert!(lists_kfx_output(
        &m.listing(&[m.epub_output(), m.kfx_output()])
    ));
}

#[test]
fn the_columns_are_read_off_the_header() {
    let n = Table {
        type_width: 18,
        name_width: 36,
    };
    assert!(lists_kfx_output(
        &n.listing(&[n.companion(), n.kfx_output()])
    ));
    assert!(!lists_kfx_output(&n.listing(&[n.companion()])));
}

#[test]
fn the_plugin_or_a_renamed_copy_counts_and_look_alikes_do_not() {
    let m = &MEASURED;
    let named = |name: &str| {
        lists_kfx_output(&m.listing(&[vec![
            m.line("Conversion output", name, "(2, 12, 0)", "False"),
            String::new(),
        ]]))
    };
    assert!(named("KFX Output"));
    assert!(named("KFX Output (Fix Traditional Chinese)"));
    assert!(named("KFX Output (fork)"));
    assert!(!named("Set KFX metadata (from KFX Output)"));
    assert!(!named("Legacy KFX Output"));
    assert!(!named("KFX Outputs"));
    assert!(!named("KFX Output(fork)"));
    assert!(!named(
        "Set KFX metadata (from KFX Output (Fix Traditional Chinese))"
    ));
}

#[test]
fn a_disabled_plugin_still_counts() {
    let m = &MEASURED;
    for disabled in ["True", "False"] {
        let row = vec![
            m.line("Conversion output", "KFX Output", "(2, 20, 1)", disabled),
            String::new(),
        ];
        assert!(lists_kfx_output(&m.listing(&[m.companion(), row])));
    }
}

#[test]
fn a_description_line_that_mentions_it_is_not_a_plugin_row() {
    let m = &MEASURED;
    let quoting = vec![
        m.line("User interface action", "KFX Helper", "(1, 0, 0)", "False"),
        "\t Works alongside KFX Output".into(),
        format!("\t{}KFX Output", " ".repeat(21)),
        String::new(),
    ];
    assert!(!lists_kfx_output(&m.listing(&[quoting])));
}

#[test]
fn windows_line_endings_read_the_same() {
    let m = &MEASURED;
    let crlf = m
        .listing(&[m.companion(), m.kfx_output()])
        .split('\n')
        .collect::<Vec<_>>()
        .join("\r\n");
    assert!(lists_kfx_output(&crlf));
}

#[test]
fn a_listing_with_no_header_names_nothing() {
    let m = &MEASURED;
    let mut rows = vec![String::new()];
    rows.extend(m.kfx_output());
    assert!(!lists_kfx_output(&rows.join("\n")));
    assert!(!lists_kfx_output(""));
}

/// CALIBRE SPAWN: `calibre-customize --list-plugins`, with the scratch copy.
#[test]
fn this_machines_calibre_still_prints_the_header() {
    let env = calibre_env();
    let Some(customize) = calibre_tool("calibre-customize", &env) else {
        return;
    };
    let run = run_with_timeout(
        &[customize, "--list-plugins".into()],
        &env,
        PLUGIN_LIST_TIMEOUT_MS,
    )
    .unwrap();
    let header = plugin_table_header(&run.stdout).expect("a header row");
    let words: Vec<&str> = header.split_whitespace().collect();
    assert_eq!(
        words,
        [
            "Type",
            "Name",
            "Version",
            "Disabled",
            "Site",
            "Customization"
        ]
    );
    assert!(header.starts_with("Type "));
}

#[test]
fn the_header_is_found_wherever_it_is() {
    let m = &MEASURED;
    let table = m.listing(&[m.kfx_output()]);
    assert_eq!(plugin_table_header(&table), Some(m.header().as_str()));
    let warned = format!("calibre: a warning printed first\n{table}");
    assert_eq!(plugin_table_header(&warned), Some(m.header().as_str()));
    assert!(lists_kfx_output(&warned));
    assert_eq!(plugin_table_header(&m.kfx_output().join("\n")), None);
}

// ---- pluginInstalled against a fake calibre-customize ----

#[cfg(unix)]
fn fake_customize(s: &Scratch, listing: &str) -> String {
    let fake = s.join("calibre-customize");
    common::executable(&fake, format!("#!/bin/sh\ncat <<'EOF'\n{listing}\nEOF\n"));
    fake
}

#[cfg(unix)]
#[test]
fn plugin_installed_reads_a_fake_listing() {
    let m = &MEASURED;
    let cases = [
        (m.listing(&[m.kfx_output(), m.companion()]), true),
        (m.listing(&[m.epub_output()]), false),
        (m.listing(&[m.epub_output(), m.companion()]), false),
    ];
    for (listing, want) in cases {
        let s = Scratch::new("kfx-customize");
        let fake = fake_customize(&s, &listing);
        assert_eq!(plugin_installed(&fake, &guarded_env(), 10_000), want);
    }
}

// ---- installKfxPlugin ----

const TOOL: &str = "/nowhere/calibre-debug";

fn answer(
    stdout: String,
) -> impl Fn(&[String]) -> Result<TimedRun, sp_engine::errors::EngineError> {
    move |_argv: &[String]| {
        Ok(TimedRun {
            code: 0,
            stdout: stdout.clone(),
            stderr: String::new(),
            timed_out: false,
        })
    }
}

fn result_line(v: serde_json::Value) -> String {
    format!("METHOD_READER_RESULT {v}\n")
}

#[test]
fn reports_the_version_calibre_installed() {
    let r = install_kfx_plugin(
        &answer(format!(
            "some calibre chatter\n{}",
            result_line(json!({ "ok": true, "version": "2.20.1" }))
        )),
        Some(TOOL),
    );
    assert!(r.ok);
    assert_eq!(r.version.as_deref(), Some("2.20.1"));
}

#[test]
fn a_conflicting_fork_is_removed_and_named() {
    let r = install_kfx_plugin(
        &answer(result_line(json!({ "ok": true, "version": "2.20.1",
            "removed": ["KFX Output (Fix Traditional Chinese)"] }))),
        Some(TOOL),
    );
    assert!(r.ok);
    assert_eq!(
        r.removed,
        Some(vec!["KFX Output (Fix Traditional Chinese)".to_owned()])
    );
}

#[test]
fn a_plain_upgrade_removes_nothing() {
    let r = install_kfx_plugin(
        &answer(result_line(
            json!({ "ok": true, "version": "2.20.1", "removed": [] }),
        )),
        Some(TOOL),
    );
    assert!(r.ok);
    assert_eq!(r.removed, Some(vec![]));
}

#[test]
fn no_calibre_is_a_named_reason() {
    let r = install_kfx_plugin(&answer(String::new()), None);
    assert!(!r.ok);
    assert!(r.reason.unwrap().to_lowercase().contains("calibre"));
}

#[test]
fn a_failed_install_carries_calibres_own_words() {
    let run = |_: &[String]| {
        Ok(TimedRun {
            code: 1,
            stdout: String::new(),
            stderr: "urlopen error timed out".into(),
            timed_out: false,
        })
    };
    let r = install_kfx_plugin(&run, Some(TOOL));
    assert!(!r.ok);
    assert!(r.reason.unwrap().contains("timed out"));
}

#[test]
fn output_with_no_result_line_is_refused() {
    let r = install_kfx_plugin(&answer("hello\n".into()), Some(TOOL));
    assert!(!r.ok);
    assert_eq!(r.reason.as_deref(), Some("hello"));
}

#[test]
fn a_result_line_saying_failure_is_honoured() {
    let r = install_kfx_plugin(
        &answer(result_line(
            json!({ "ok": false, "error": "size mismatch" }),
        )),
        Some(TOOL),
    );
    assert!(!r.ok);
    assert!(r.reason.unwrap().contains("size mismatch"));
}

#[test]
fn a_failure_after_a_copy_was_cleared_names_it() {
    let r = install_kfx_plugin(
        &answer(result_line(
            json!({ "ok": false, "error": "add_plugin failed",
            "removed": ["KFX Output (Fix Traditional Chinese)"] }),
        )),
        Some(TOOL),
    );
    assert_eq!(
        r,
        KfxInstallResult {
            ok: false,
            version: None,
            reason: Some("add_plugin failed".into()),
            removed: Some(vec!["KFX Output (Fix Traditional Chinese)".into()]),
        }
    );
}

#[test]
fn a_failure_before_anything_was_cleared_reports_empty() {
    let r = install_kfx_plugin(
        &answer(result_line(
            json!({ "ok": false, "error": "size mismatch", "removed": [] }),
        )),
        Some(TOOL),
    );
    assert!(!r.ok);
    assert_eq!(r.removed, Some(vec![]));
}

#[test]
fn an_unreachable_site_gets_a_friendly_sentence() {
    let r = install_kfx_plugin(
        &answer(result_line(json!({ "ok": false,
            "error": "<urlopen error [Errno 8] nodename nor servname provided>", "offline": true }))),
        Some(TOOL),
    );
    assert!(!r.ok);
    assert_eq!(
        r.reason.as_deref(),
        Some(
            "Calibre's plugin site could not be reached. Check the internet connection, then try again."
        )
    );
}

// ---- the install snippet, run under python3 against stub modules ----

const FORK: (&str, bool) = ("KFX Output (Fix Traditional Chinese)", true);
const COMPANION: (&str, bool) = ("Set KFX metadata (from KFX Output)", false);
const REAL: (&str, bool) = ("KFX Output", true);

fn install(sc: serde_json::Value) -> Option<(KfxInstallResult, Vec<String>)> {
    let python = system_python()?;
    let s = Scratch::new("kfx-snippet");
    let stubs = SnippetRun::new(&s.str(), python, &sc);
    let result = install_kfx_plugin(&|argv: &[String]| stubs.run(argv), Some(TOOL));
    let (stdout, stderr) = stubs.last();
    assert!(
        stdout.contains("METHOD_READER_RESULT "),
        "no result line; stderr was:\n{stderr}"
    );
    Some((result, stubs.events()))
}

fn kinds(events: &[String]) -> Vec<&str> {
    events
        .iter()
        .map(|e| e.split(' ').next().unwrap_or(""))
        .collect()
}

fn all_plugins() -> Vec<(&'static str, bool)> {
    vec![REAL, FORK, COMPANION]
}

#[test]
fn snippet_installs_reports_the_version_and_clears_only_the_fork() {
    let Some((result, events)) = install(scenario("ok", "ok", &all_plugins())) else {
        return;
    };
    assert_eq!(
        result,
        KfxInstallResult {
            ok: true,
            version: Some("2.20.1".into()),
            reason: None,
            removed: Some(vec![FORK.0.into()]),
        }
    );
    assert_eq!(kinds(&events), ["index", "zip", "remove", "add"]);
    assert!(events.contains(&format!("remove {}", FORK.0)));
    assert!(events.contains(&"add zip-present".to_owned()));
    assert_eq!(
        events[1],
        "zip https://plugins.calibre-ebook.com/kfx_output.zip"
    );
}

#[test]
fn snippet_unreachable_index_is_offline_and_leaves_every_plugin() {
    let Some((result, events)) = install(scenario("offline", "ok", &all_plugins())) else {
        return;
    };
    assert_eq!(
        result,
        KfxInstallResult {
            ok: false,
            version: None,
            reason: Some("Calibre's plugin site could not be reached. Check the internet connection, then try again.".into()),
            removed: Some(vec![]),
        }
    );
    assert_eq!(kinds(&events), ["index"]);
}

#[test]
fn snippet_unreachable_zip_is_offline_and_leaves_every_plugin() {
    let Some((result, events)) = install(scenario("ok", "offline", &all_plugins())) else {
        return;
    };
    assert!(!result.ok);
    assert!(result.reason.unwrap().contains("could not be reached"));
    assert_eq!(result.removed, Some(vec![]));
    assert_eq!(kinds(&events), ["index", "zip"]);
}

#[test]
fn snippet_size_mismatch_is_refused_before_any_fork_is_cleared() {
    let mut sc = scenario("ok", "ok", &all_plugins());
    sc["sizeDelta"] = json!(7);
    let Some((result, events)) = install(sc) else {
        return;
    };
    assert!(!result.ok);
    let reason = result.reason.unwrap();
    assert!(reason.starts_with("downloaded ") && reason.contains(" bytes, index says "));
    assert_eq!(result.removed, Some(vec![]));
    assert_eq!(kinds(&events), ["index", "zip"]);
}

#[test]
fn snippet_a_download_that_is_not_a_plugin_is_refused() {
    let Some((result, events)) = install(scenario("ok", "not-plugin", &all_plugins())) else {
        return;
    };
    assert_eq!(
        result,
        KfxInstallResult {
            ok: false,
            version: None,
            reason: Some("downloaded file is not a calibre plugin".into()),
            removed: Some(vec![]),
        }
    );
    assert_eq!(kinds(&events), ["index", "zip"]);
}

#[test]
fn snippet_an_add_plugin_failure_reports_what_was_removed() {
    let mut sc = scenario("ok", "ok", &all_plugins());
    sc["addFails"] = json!("plugin rejected by calibre");
    let Some((result, events)) = install(sc) else {
        return;
    };
    assert_eq!(
        result,
        KfxInstallResult {
            ok: false,
            version: None,
            reason: Some("plugin rejected by calibre".into()),
            removed: Some(vec![FORK.0.into()]),
        }
    );
    assert_eq!(kinds(&events), ["index", "zip", "remove", "add"]);
}

#[test]
fn snippet_an_index_entry_with_no_file_is_a_plain_failure() {
    let Some((result, events)) = install(scenario("no-file", "ok", &all_plugins())) else {
        return;
    };
    assert!(!result.ok);
    assert!(!result.reason.unwrap().contains("could not be reached"));
    assert_eq!(result.removed, Some(vec![]));
    assert_eq!(kinds(&events), ["index"]);
}

#[test]
fn snippet_a_failed_calibre_import_still_ends_in_a_result_line() {
    let mut sc = scenario("ok", "ok", &all_plugins());
    sc["noIndexUrl"] = json!(true);
    let Some((result, events)) = install(sc) else {
        return;
    };
    assert!(!result.ok);
    assert!(result.reason.unwrap().contains("INDEX_URL"));
    assert_eq!(result.removed, Some(vec![]));
    assert!(events.is_empty());
}

// ---- every Calibre spawn has a timeout ----

#[cfg(unix)]
fn sleeper(s: &Scratch) -> String {
    let tool = s.join("calibre-tool");
    common::executable(&tool, "#!/bin/sh\nsleep 5\n");
    tool
}

#[test]
fn the_limits_a_minute_to_list_five_to_install() {
    assert_eq!(PLUGIN_LIST_TIMEOUT_MS, 60 * 1000);
    assert_eq!(PLUGIN_INSTALL_TIMEOUT_MS, 5 * 60 * 1000);
}

#[cfg(unix)]
#[test]
fn a_plugin_listing_that_hangs_reads_as_not_installed() {
    let s = Scratch::new("sleeper");
    let started = std::time::Instant::now();
    assert!(!plugin_installed(&sleeper(&s), &guarded_env(), 200));
    assert!(started.elapsed().as_millis() < 4000);
}

#[cfg(unix)]
#[test]
fn an_install_that_hangs_is_stopped_and_reported() {
    let s = Scratch::new("sleeper");
    let tool = sleeper(&s);
    let started = std::time::Instant::now();
    let runner = debug_runner(guarded_env(), 200);
    let result = install_kfx_plugin(&runner, Some(&tool));
    assert!(started.elapsed().as_millis() < 4000);
    assert!(!result.ok);
    assert!(result.reason.unwrap().contains("was stopped"));
}

#[cfg(unix)]
#[test]
fn a_kfx_conversion_that_hangs_is_stopped_with_its_scratch() {
    let s = Scratch::new("kfx-hang");
    let epub = s.join("book.epub");
    std::fs::write(&epub, "fake epub bytes").unwrap();
    let tool = sleeper(&s);
    let deps = KfxDeps {
        tool: Some(Box::new(move || Some(tool.clone()))),
        status: Some(Box::new(|| READY)),
        timeout_ms: Some(200),
    };
    let err = to_kfx(&epub, &mut |_| {}, &deps, &guarded_env()).unwrap_err();
    assert_eq!(err.name(), "CalibreTimedOutError");
    assert!(!std::path::Path::new(&kfx_scratch_path(&epub)).exists());
    assert!(!std::path::Path::new(&kfx_sibling(&epub)).exists());
}
