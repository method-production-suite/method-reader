//! `method-reader mcp`, end to end: the real binary over stdio, in a scratch
//! folder with every seam pinned (tests/common), each tool on the committed
//! invented fixtures. The protocol's own cases are src/mcp/tests.rs.
//!
//! Each tool is held to the command line's own answer for the same script,
//! since the tools run that same code: a count the server reports that the
//! command line doesn't is a second implementation, which is the bug.

mod common;

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use common::{Sandbox, zip_entries};
use serde_json::{Value, json};

/// A running server and the lines it writes.
struct Session {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: mpsc::Receiver<String>,
    next_id: u64,
}

impl Session {
    /// `exe mcp <args>` in the sandbox, with only its pins in the
    /// environment (as `Sandbox::run` starts the command line).
    fn start(sb: &Sandbox, exe: &str, args: &[&str]) -> Session {
        let mut c = Command::new(exe);
        c.arg("mcp").args(args).current_dir(sb.root());
        c.env_clear();
        for k in [
            "PATH",
            "HOME",
            "TMPDIR",
            "SystemRoot",
            "USERPROFILE",
            "APPDATA",
        ] {
            if let Some(v) = std::env::var_os(k) {
                c.env(k, v);
            }
        }
        for (k, v) in &sb.env {
            c.env(k, v);
        }
        c.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = c.spawn().expect("method-reader mcp starts");
        let stdout = child.stdout.take().expect("stdout");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let stdin = child.stdin.take();
        Session {
            child,
            stdin,
            lines: rx,
            next_id: 0,
        }
    }

    fn send(&mut self, msg: &Value) {
        let stdin = self.stdin.as_mut().expect("stdin open");
        writeln!(stdin, "{msg}").expect("write");
        stdin.flush().expect("flush");
    }

    /// The next line the server writes, as JSON.
    fn read(&mut self) -> Value {
        let line = self
            .lines
            .recv_timeout(Duration::from_secs(300))
            .expect("an answer");
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON ({e}): {line}"))
    }

    fn rpc(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        let r = self.read();
        assert_eq!(r["id"], json!(id), "{r}");
        r
    }

    fn initialize(&mut self) -> Value {
        let r = self.rpc(
            "initialize",
            json!({"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "test", "version": "1"}}),
        );
        self.send(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}));
        r
    }

    /// A tool's structured result, or its error kind and text.
    fn tool(&mut self, name: &str, args: Value) -> Result<Value, (String, String)> {
        let r = self.rpc("tools/call", json!({"name": name, "arguments": args}));
        let result = &r["result"];
        assert!(result.is_object(), "{name}: not a tool result: {r}");
        let text = result["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        if result["isError"] == json!(true) {
            let kind = result["structuredContent"]["error"]["kind"]
                .as_str()
                .unwrap_or_default()
                .to_owned();
            return Err((kind, text));
        }
        // The JSON is in the content too, for clients without structured results.
        let mirrored: Value =
            serde_json::from_str(result["content"][1]["text"].as_str().expect("JSON text"))
                .expect("JSON");
        assert_eq!(mirrored, result["structuredContent"], "{name}");
        Ok(result["structuredContent"].clone())
    }

    /// Closes stdin; the exit code and everything on stderr.
    fn finish(mut self) -> (i32, String) {
        drop(self.stdin.take());
        let out = self.child.wait_with_output().expect("exits");
        (
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    }
}

fn reader(sb: &Sandbox) -> Session {
    let root = sb.p("");
    Session::start(sb, env!("CARGO_BIN_EXE_method-reader"), &["--root", &root])
}

/// Every file under `dir`, relative, sorted.
fn files(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).expect("a folder") {
            let p = e.expect("an entry").path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(
                    p.strip_prefix(dir)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    out.sort();
    out
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

#[test]
fn the_binary_serves_over_stdio_and_inspects_like_the_command_line() {
    let sb = Sandbox::new();
    let pdf = sb.copy_fixture("field-station.pdf", "scripts/field-station.pdf");
    let before = files(&sb.root());
    let mut m = reader(&sb);
    let init = m.initialize();
    assert_eq!(init["result"]["serverInfo"]["name"], "method-reader");
    assert_eq!(init["result"]["protocolVersion"], "2025-11-25");
    let tools = m.rpc("tools/list", json!({}));
    assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 4);

    let got = m
        .tool("inspect_script", json!({"path": s(&pdf)}))
        .expect("inspects");

    // The command line's own answer for the same script.
    let cli = sb.run(&[&s(&pdf), "--json", "--no-fountain", "-o", &sb.p("cli.epub")]);
    assert_eq!(cli.code, 0, "{}", cli.stderr);
    let cli = cli.json();
    assert_eq!(got["title"], cli["title"]);
    assert_eq!(got["pages"], cli["pages"]);
    assert_eq!(got["scene_count"], cli["scenes"]);
    assert_eq!(got["character_count"], cli["characters"]);
    let names: Vec<&Value> = got["characters"]
        .as_array()
        .unwrap()
        .iter()
        .take(5)
        .map(|c| &c["name"])
        .collect();
    let top: Vec<&Value> = cli["topCharacters"].as_array().unwrap().iter().collect();
    assert_eq!(names, top, "the same speakers, most speeches first");
    assert_eq!(got["path"], json!(s(&common::canonical(&pdf))));

    let pages = got["pages"].as_u64().unwrap();
    assert!(pages > 1);
    let scenes = got["scenes"].as_array().unwrap();
    assert!(!scenes.is_empty());
    let mut last_page = 0;
    for sc in scenes {
        let heading = sc["heading"].as_str().unwrap();
        assert!(!heading.is_empty());
        let page = sc["page"].as_u64().unwrap();
        assert!(page >= last_page && page <= pages, "{sc}");
        last_page = page;
    }
    let characters = got["characters"].as_array().unwrap();
    let mut last = u64::MAX;
    for c in characters {
        let speeches = c["speeches"].as_u64().unwrap();
        assert!(speeches <= last, "most speeches first: {c}");
        last = speeches;
        let first = c["first_page"].as_u64().unwrap();
        assert!((1..=pages).contains(&first), "{c}");
    }

    // Nothing was written beside the script, or anywhere else in the
    // sandbox but the command line's own run above.
    let mut after = files(&sb.root());
    after.retain(|f| f != "cli.epub");
    assert_eq!(after, before);

    let (code, stderr) = m.finish();
    assert_eq!(code, 0, "{stderr}");
    assert!(stderr.contains("inspect_script ok"), "{stderr}");
}

#[test]
fn preview_page_gives_one_page_as_plain_text() {
    let sb = Sandbox::new();
    let pdf = s(&sb.copy_fixture("field-station.pdf", "field-station.pdf"));
    let mut m = reader(&sb);
    m.initialize();

    // Page 2 is the first page after the title page: it opens on the
    // script's first scene heading, which the .fountain has too.
    let page = m
        .tool("preview_page", json!({"path": pdf, "page": 2}))
        .expect("a page");
    assert_eq!(page["page"], 2);
    let text = page["text"].as_str().unwrap();
    let elements = page["elements"].as_array().unwrap();
    assert!(elements.len() > 3, "{page}");
    assert_eq!(elements[0]["type"], "scene");
    assert!(text.starts_with(elements[0]["text"].as_str().unwrap()));
    for bad in ["**", "[[", "\t"] {
        assert!(!text.contains(bad), "{bad:?} in {text}");
    }
    let kinds: Vec<&str> = elements
        .iter()
        .map(|e| e["type"].as_str().unwrap())
        .collect();
    assert!(
        kinds.contains(&"character") && kinds.contains(&"dialogue"),
        "{kinds:?}"
    );
    // A cue's speech follows it on the next line, as a .fountain lays it out.
    let cue = elements
        .iter()
        .position(|e| e["type"] == "character")
        .unwrap();
    let pair = format!(
        "{}\n{}",
        elements[cue]["text"].as_str().unwrap(),
        elements[cue + 1]["text"].as_str().unwrap()
    );
    assert!(text.contains(&pair), "{text}");

    let fountain = sb.p("check.fountain");
    let cli = sb.run(&[
        &pdf,
        "--json",
        "-o",
        &sb.p("check.epub"),
        "--fountain",
        &fountain,
    ]);
    assert_eq!(cli.code, 0);
    let fountain = std::fs::read_to_string(&fountain).unwrap();
    let first_heading = fountain
        .lines()
        .find(|l| l.starts_with("INT") || l.starts_with("EXT"))
        .unwrap();
    assert_eq!(elements[0]["text"], first_heading);

    // Every page reads, and the one past the end is refused.
    let pages = page["pages"].as_u64().unwrap();
    let last = m
        .tool("preview_page", json!({"path": pdf, "page": pages}))
        .expect("the last page");
    assert_eq!(last["pages"], json!(pages));
    let (kind, e) = m
        .tool("preview_page", json!({"path": pdf, "page": pages + 1}))
        .unwrap_err();
    assert_eq!(kind, "invalid_input");
    assert!(e.contains(&format!("this script has {pages} pages")), "{e}");
    m.finish();
}

#[test]
fn convert_to_script_writes_the_fountain_and_fdx_the_command_line_writes() {
    let sb = Sandbox::new();
    let pdf = s(&sb.copy_fixture("screenplay.pdf", "in/screenplay.pdf"));
    let out = sb.mkdir("out");
    let mut m = reader(&sb);
    m.initialize();
    let got = m
        .tool(
            "convert_to_script",
            json!({"path": pdf, "output_dir": s(&out)}),
        )
        .expect("converts");
    let fountain = PathBuf::from(got["fountain_path"].as_str().unwrap());
    let fdx = PathBuf::from(got["fdx_path"].as_str().unwrap());
    assert_eq!(
        fountain,
        common::canonical(&out).join("screenplay.fountain")
    );
    assert_eq!(fdx, common::canonical(&out).join("screenplay.fdx"));
    // Only what was asked for, and nothing beside the PDF.
    assert_eq!(files(&out), ["screenplay.fdx", "screenplay.fountain"]);
    assert_eq!(files(&sb.path("in")), ["screenplay.pdf"]);

    // Byte for byte what the command line writes for the same PDF.
    let cli = sb.run(&[
        &pdf,
        "--json",
        "-o",
        &sb.p("cli/x.epub"),
        "--fountain",
        &sb.p("cli/x.fountain"),
        "--fdx",
        &sb.p("cli/x.fdx"),
    ]);
    assert_eq!(cli.code, 1, "the folder must exist first");
    sb.mkdir("cli");
    let cli = sb.run(&[
        &pdf,
        "--json",
        "-o",
        &sb.p("cli/x.epub"),
        "--fountain",
        &sb.p("cli/x.fountain"),
        "--fdx",
        &sb.p("cli/x.fdx"),
    ]);
    assert_eq!(cli.code, 0, "{}", cli.stderr);
    let answer = cli.json();
    assert_eq!(
        std::fs::read(&fountain).unwrap(),
        std::fs::read(sb.path("cli/x.fountain")).unwrap()
    );
    assert_eq!(
        std::fs::read(&fdx).unwrap(),
        std::fs::read(sb.path("cli/x.fdx")).unwrap()
    );
    for (k, cli_k) in [("pages", "pages"), ("scenes", "scenes"), ("title", "title")] {
        assert_eq!(got[k], answer[cli_k], "{k}");
    }
    let fdx_text = std::fs::read_to_string(&fdx).unwrap();
    assert!(fdx_text.starts_with("<?xml"), "{fdx_text}");

    // A second call refuses to replace them, and overwrite replaces them.
    let (kind, e) = m
        .tool(
            "convert_to_script",
            json!({"path": pdf, "output_dir": s(&out)}),
        )
        .unwrap_err();
    assert_eq!(kind, "refused");
    assert!(e.contains("already exists"), "{e}");
    std::fs::write(&fdx, "old").unwrap();
    m.tool(
        "convert_to_script",
        json!({"path": pdf, "output_dir": s(&out), "formats": ["fdx"], "overwrite": true}),
    )
    .expect("replaces");
    assert_eq!(std::fs::read_to_string(&fdx).unwrap(), fdx_text);

    // By default the scripts go beside the PDF.
    let only = m
        .tool(
            "convert_to_script",
            json!({"path": pdf, "formats": ["fountain"]}),
        )
        .expect("converts");
    assert_eq!(only["fdx_path"], Value::Null);
    assert_eq!(
        files(&sb.path("in")),
        ["screenplay.fountain", "screenplay.pdf"]
    );
    m.finish();
}

#[test]
fn make_ebook_writes_each_format_asked_for() {
    let sb = Sandbox::new();
    let pdf = s(&sb.copy_fixture("field-station.pdf", "field-station.pdf"));
    let out = sb.mkdir("books");
    let mut m = reader(&sb);
    m.initialize();
    let got = m
        .tool(
            "make_ebook",
            json!({"path": pdf, "output_dir": s(&out), "formats": ["epub", "azw3", "mobi"]}),
        )
        .expect("makes books");
    assert_eq!(
        files(&out),
        [
            "field-station.azw3",
            "field-station.epub",
            "field-station.mobi"
        ]
    );
    let book = |ext: &str| common::canonical(&out).join(format!("field-station.{ext}"));
    assert_eq!(got["epub_path"], json!(s(&book("epub"))));
    assert_eq!(got["azw3_path"], json!(s(&book("azw3"))));
    assert_eq!(got["mobi_path"], json!(s(&book("mobi"))));
    let names: Vec<String> = zip_entries(&book("epub"))
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    assert_eq!(names[0], "mimetype");
    for ext in ["azw3", "mobi"] {
        let bytes = std::fs::read(book(ext)).unwrap();
        assert_eq!(&bytes[60..68], b"BOOKMOBI", "{ext}");
    }

    // A MOBI alone still lands where it was asked for, with no EPUB beside it.
    let alone = sb.mkdir("mobi-only");
    let got = m
        .tool(
            "make_ebook",
            json!({"path": pdf, "output_dir": s(&alone), "formats": ["mobi"]}),
        )
        .expect("a MOBI");
    assert_eq!(files(&alone), ["field-station.mobi"]);
    assert_eq!(got["epub_path"], Value::Null);
    assert_eq!(
        got["mobi_path"],
        json!(s(&common::canonical(&alone).join("field-station.mobi")))
    );
    let bytes = std::fs::read(alone.join("field-station.mobi")).unwrap();
    assert_eq!(&bytes[60..68], b"BOOKMOBI");

    // The command line's EPUB for the same script and title, entry for entry.
    let titled = sb.mkdir("titled");
    m.tool(
        "make_ebook",
        json!({"path": pdf, "output_dir": s(&titled), "title": "A Title Given", "author": "Someone Invented"}),
    )
    .expect("an EPUB");
    let cli = sb.run(&[
        &pdf,
        "--json",
        "--no-fountain",
        "--title",
        "A Title Given",
        "--author",
        "Someone Invented",
        "-o",
        &sb.p("cli.epub"),
    ]);
    assert_eq!(cli.code, 0);
    let ours = zip_entries(&titled.join("field-station.epub"));
    let theirs = zip_entries(&sb.path("cli.epub"));
    assert_eq!(ours.len(), theirs.len());
    for ((n1, d1), (n2, d2)) in ours.iter().zip(&theirs) {
        assert_eq!(n1, n2);
        // The package records when it was made; everything else matches.
        if !n1.ends_with(".opf") {
            assert_eq!(d1, d2, "{n1}");
        }
    }
    let opf = ours
        .iter()
        .find(|(n, _)| n.ends_with(".opf"))
        .map(|(_, d)| String::from_utf8_lossy(d).into_owned())
        .unwrap();
    assert!(opf.contains("A Title Given") && opf.contains("Someone Invented"));

    // A Fountain script makes a book too.
    let fountain = sb.p("check.fountain");
    sb.run(&[
        &pdf,
        "--json",
        "-o",
        &sb.p("check.epub"),
        "--fountain",
        &fountain,
    ]);
    let fbooks = sb.mkdir("from-fountain");
    let got = m
        .tool(
            "make_ebook",
            json!({"path": fountain, "output_dir": s(&fbooks)}),
        )
        .expect("a book from Fountain");
    assert_eq!(files(&fbooks), ["check.epub"]);
    assert!(got["title"].is_string());
    m.finish();
}

#[test]
fn refusals_come_back_as_tool_errors() {
    let sb = Sandbox::new();
    let scanned = s(&sb.copy_fixture("blank-pages.pdf", "blank-pages.pdf"));
    let prose = s(&sb.copy_fixture("prose.pdf", "prose.pdf"));
    let mut m = reader(&sb);
    m.initialize();
    let (kind, e) = m
        .tool("inspect_script", json!({"path": scanned}))
        .unwrap_err();
    assert_eq!(kind, "refused");
    assert!(e.contains("(method-reader error: scanned)"), "{e}");
    let (kind, e) = m.tool("make_ebook", json!({"path": prose})).unwrap_err();
    assert_eq!(kind, "refused");
    assert!(e.contains("(method-reader error: not-screenplay)"), "{e}");
    // Nothing was left behind by the refusal.
    assert_eq!(files(&sb.root()), ["blank-pages.pdf", "prose.pdf"]);
    // force reads it anyway, as --force does.
    let got = m
        .tool("inspect_script", json!({"path": prose, "force": true}))
        .expect("forced");
    assert!(got["pages"].as_u64().unwrap() >= 1);
    // A path outside the root is refused before anything is read.
    let (kind, e) = m
        .tool(
            "inspect_script",
            json!({"path": "/no/such/root/script.pdf"}),
        )
        .unwrap_err();
    assert!(kind == "not_found" || kind == "path_refused", "{kind}: {e}");
    m.finish();
}

#[test]
fn the_old_name_serves_too_and_says_nothing_extra() {
    let sb = Sandbox::new();
    let root = sb.p("");
    let mut m = Session::start(&sb, env!("CARGO_BIN_EXE_screepub"), &["--root", &root]);
    let init = m.initialize();
    assert_eq!(init["result"]["serverInfo"]["name"], "method-reader");
    let (code, stderr) = m.finish();
    assert_eq!(code, 0);
    assert!(!stderr.contains("screepub is now called"), "{stderr}");
}

#[test]
fn bad_command_lines_are_refused() {
    let sb = Sandbox::new();
    let help = sb.run(&["mcp", "--help"]);
    assert_eq!(help.code, 0);
    assert!(help.stdout.contains("method-reader mcp [--root DIR]..."));
    let r = sb.run(&["mcp", "extra"]);
    assert_eq!(r.code, 1);
    assert!(
        r.stderr.contains("unexpected argument `extra`"),
        "{}",
        r.stderr
    );
    let r = sb.run(&["mcp", "--root"]);
    assert_eq!(r.code, 1);
    let r = sb.run(&["mcp", "--root", "/no/such/folder/anywhere"]);
    assert_eq!(r.code, 1);
    assert!(
        r.stderr.contains("/no/such/folder/anywhere"),
        "{}",
        r.stderr
    );
    // A file named mcp is a script to convert, as a file named after any
    // verb is.
    sb.write("mcp", "x");
    let r = sb.run(&["mcp", "--json"]);
    assert_eq!(r.error_code(), "unsupported-type");
}
