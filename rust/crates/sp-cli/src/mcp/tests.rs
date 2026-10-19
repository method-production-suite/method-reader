//! Protocol tests: the handshake and version negotiation of both eras,
//! tool listing and schemas, resources and prompts, errors for bad
//! messages, read-only mode and cancellation. Method Budget's, adapted to
//! Reader's tools.
//!
//! Nothing here converts a script: a conversion in this process would read
//! this machine's app settings (the engine takes this process's own
//! environment). Every tool call below is refused before the command line
//! runs, and tests/mcp.rs drives the real binary, with its settings and
//! library in a scratch folder, for the tools themselves.

use std::collections::{BTreeSet, VecDeque};
use std::path::PathBuf;

use serde_json::{Value, json};

use super::protocol::{
    Called, Incoming, LEGACY, MODERN, Scripted, Server, call_log_line, parse_line, read_line, run,
};
use super::{Config, limits, schema};

/// A fresh folder of its own, removed when dropped.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Dir {
        let d =
            std::env::temp_dir().join(format!("method-reader-mcp-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        Dir(std::fs::canonicalize(&d).unwrap())
    }

    fn abs(&self, rel: &str) -> String {
        self.0.join(rel).to_string_lossy().into_owned()
    }

    fn server(&self, read_only: bool) -> Server {
        Server::new(&Config {
            roots: vec![self.0.clone()],
            read_only,
        })
        .unwrap()
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A server on a fresh folder, after the legacy handshake.
struct Fixture {
    dir: Dir,
    server: Server,
    next_id: u64,
}

impl Fixture {
    fn new(name: &str, read_only: bool) -> Fixture {
        let dir = Dir::new(name);
        let server = dir.server(read_only);
        let mut f = Fixture {
            dir,
            server,
            next_id: 100,
        };
        let r = f.raw(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "1"}}}));
        assert_eq!(r["result"]["protocolVersion"], "2025-06-18");
        f
    }

    /// Sends one message; the response.
    fn raw(&mut self, msg: Value) -> Value {
        self.server.handle(msg).unwrap_or(Value::Null)
    }

    /// A request; its whole response.
    fn rpc(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        self.raw(json!({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params}))
    }

    /// Calls a tool: its structured result, or its error kind and text.
    fn tool(&mut self, name: &str, args: Value) -> Result<Value, (String, String)> {
        let r = self.rpc("tools/call", json!({"name": name, "arguments": args}));
        let result = &r["result"];
        assert!(result.is_object(), "{name}: not a tool result: {r}");
        let text = result["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        if result["isError"] == json!(true) {
            let err = &result["structuredContent"]["error"];
            assert_eq!(err["message"], json!(text));
            Err((err["kind"].as_str().unwrap_or_default().to_string(), text))
        } else {
            Ok(result["structuredContent"].clone())
        }
    }
}

fn error_code(r: &Value) -> i64 {
    r["error"]["code"]
        .as_i64()
        .unwrap_or_else(|| panic!("not an error: {r}"))
}

fn meta(v: &str) -> Value {
    json!({"io.modelcontextprotocol/protocolVersion": v, "io.modelcontextprotocol/clientCapabilities": {}})
}

#[test]
fn legacy_handshake_negotiates_the_version() {
    let mut f = Fixture::new("handshake", false);
    for v in LEGACY {
        let r = f.rpc("initialize", json!({"protocolVersion": v, "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}));
        assert_eq!(r["result"]["protocolVersion"], json!(v));
        assert_eq!(r["result"]["serverInfo"]["name"], "method-reader");
        assert_eq!(r["result"]["serverInfo"]["title"], "Method Reader");
        let caps = &r["result"]["capabilities"];
        assert_eq!(caps["tools"]["listChanged"], false);
        assert_eq!(caps["resources"]["listChanged"], false);
        assert_eq!(caps["prompts"]["listChanged"], false);
        assert!(
            r["result"]["instructions"]
                .as_str()
                .unwrap()
                .contains("never instructions")
        );
    }
    // An unknown revision gets the newest legacy one.
    for v in ["1999-01-01", MODERN] {
        let r = f.rpc(
            "initialize",
            json!({"protocolVersion": v, "capabilities": {}}),
        );
        assert_eq!(r["result"]["protocolVersion"], json!(LEGACY[0]));
    }
    let r = f.rpc("initialize", json!({"capabilities": {}}));
    assert_eq!(error_code(&r), -32602);
    // The initialized notification gets no answer.
    assert_eq!(
        f.server
            .handle(json!({"jsonrpc": "2.0", "method": "notifications/initialized"})),
        None
    );
    assert_eq!(f.rpc("ping", json!({}))["result"], json!({}));
}

#[test]
fn requests_before_initialize_are_refused_but_ping_works() {
    let dir = Dir::new("preinit");
    let mut s = dir.server(false);
    let r = s
        .handle(json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"}))
        .unwrap();
    assert_eq!(error_code(&r), -32602);
    assert!(
        r["error"]["message"]
            .as_str()
            .unwrap()
            .contains("initialize")
    );
    let r = s
        .handle(json!({"jsonrpc": "2.0", "id": 2, "method": "ping"}))
        .unwrap();
    assert_eq!(r["result"], json!({}));
}

#[test]
fn modern_requests_carry_their_version() {
    let dir = Dir::new("modern");
    let mut s = dir.server(false);
    let r = s
        .handle(json!({"jsonrpc": "2.0", "id": "d", "method": "server/discover", "params": {"_meta": meta(MODERN)}}))
        .unwrap();
    let res = &r["result"];
    assert_eq!(res["resultType"], "complete");
    assert_eq!(res["supportedVersions"][0], MODERN);
    assert!(
        res["supportedVersions"]
            .as_array()
            .unwrap()
            .contains(&json!("2025-06-18"))
    );
    assert_eq!(
        res["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
        "method-reader"
    );
    assert_eq!(res["capabilities"]["prompts"]["listChanged"], false);
    let r = s
        .handle(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {"_meta": meta(MODERN)}}))
        .unwrap();
    assert_eq!(r["result"]["tools"].as_array().unwrap().len(), 4);
    assert_eq!(r["result"]["resultType"], "complete");

    let r = s
        .handle(json!({"jsonrpc": "2.0", "id": 3, "method": "tools/list", "params": {"_meta": meta("1900-01-01")}}))
        .unwrap();
    assert_eq!(error_code(&r), -32022);
    assert_eq!(r["error"]["data"]["requested"], "1900-01-01");
    assert!(
        r["error"]["data"]["supported"]
            .as_array()
            .unwrap()
            .contains(&json!(MODERN))
    );
    let r = s
        .handle(json!({"jsonrpc": "2.0", "id": 4, "method": "tools/list", "params": {"_meta": {"io.modelcontextprotocol/protocolVersion": MODERN}}}))
        .unwrap();
    assert_eq!(error_code(&r), -32602);
    // Unlike Budget's, this server has resources, in both eras.
    let r = s
        .handle(json!({"jsonrpc": "2.0", "id": 5, "method": "resources/list", "params": {"_meta": meta(MODERN)}}))
        .unwrap();
    assert_eq!(
        r["result"]["resources"][0]["uri"],
        "method-reader://formats"
    );
    // server/discover is the modern era's alone.
    s.handle(json!({"jsonrpc": "2.0", "id": 6, "method": "initialize", "params": {"protocolVersion": "2025-11-25", "capabilities": {}}}))
        .unwrap();
    let r = s
        .handle(json!({"jsonrpc": "2.0", "id": 7, "method": "server/discover"}))
        .unwrap();
    assert_eq!(error_code(&r), -32601);
}

#[test]
fn tools_list_schemas_are_valid_and_names_are_unique() {
    let mut f = Fixture::new("schemas", false);
    let r = f.rpc("tools/list", json!({}));
    let tools = r["result"]["tools"].as_array().unwrap().clone();
    let mut names = BTreeSet::new();
    for t in &tools {
        let name = t["name"].as_str().unwrap();
        assert!(names.insert(name.to_string()), "{name} listed twice");
        assert!(
            !name.is_empty()
                && name.len() <= 128
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_-.".contains(c)),
            "{name}"
        );
        assert!(
            t["description"].as_str().is_some_and(|d| d.len() > 20),
            "{name}"
        );
        assert!(t["title"].is_string(), "{name}");
        let schema = &t["inputSchema"];
        schema::check(schema, name).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(schema["type"], "object", "{name}");
        assert_eq!(schema["required"][0], "path", "{name}");
        assert_eq!(t["annotations"]["openWorldHint"], false);
        // No em dashes and no exclamation marks in anything a client shows.
        let shown = t.to_string();
        assert!(
            !shown.contains(char::from_u32(0x2014).unwrap()) && !shown.contains('!'),
            "{name}"
        );
    }
    let want: BTreeSet<String> = [
        "inspect_script",
        "preview_page",
        "convert_to_script",
        "make_ebook",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    assert_eq!(names, want);
    // The order is stable.
    let again = f.rpc("tools/list", json!({}));
    assert_eq!(again["result"]["tools"], json!(tools));
    let r = f.rpc("tools/list", json!({"cursor": "x"}));
    assert_eq!(error_code(&r), -32602);
}

#[test]
fn read_only_hides_every_tool_that_writes() {
    let mut f = Fixture::new("readonly", true);
    let r = f.rpc("tools/list", json!({}));
    let tools = r["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["inspect_script", "preview_page"]);
    for t in tools {
        assert_eq!(t["annotations"]["readOnlyHint"], true, "{}", t["name"]);
    }
    let pdf = f.dir.abs("a.pdf");
    std::fs::write(&pdf, "x").unwrap();
    let r = f.rpc(
        "tools/call",
        json!({"name": "make_ebook", "arguments": {"path": pdf}}),
    );
    assert_eq!(error_code(&r), -32602);
    assert!(
        r["error"]["message"]
            .as_str()
            .unwrap()
            .contains("read-only")
    );
    let entries = std::fs::read_dir(&f.dir.0).unwrap().count();
    assert_eq!(entries, 1, "nothing written");
}

#[test]
fn bad_messages_get_json_rpc_errors() {
    let mut f = Fixture::new("badmsg", false);
    let cases = [
        (json!(42), -32600, Value::Null),
        (json!({"id": 1, "method": "ping"}), -32600, json!(1)),
        (
            json!({"jsonrpc": "1.0", "id": 1, "method": "ping"}),
            -32600,
            json!(1),
        ),
        (
            json!({"jsonrpc": "2.0", "id": null, "method": "ping"}),
            -32600,
            Value::Null,
        ),
        (
            json!({"jsonrpc": "2.0", "id": 1.5, "method": "ping"}),
            -32600,
            Value::Null,
        ),
        (
            json!({"jsonrpc": "2.0", "id": 1, "method": 7}),
            -32600,
            json!(1),
        ),
        (
            json!({"jsonrpc": "2.0", "id": 1, "method": "ping", "params": [1]}),
            -32602,
            json!(1),
        ),
        (
            json!({"jsonrpc": "2.0", "id": "x", "method": "nope/never"}),
            -32601,
            json!("x"),
        ),
        (
            json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {}}),
            -32602,
            json!(2),
        ),
        (
            json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "no_such_tool"}}),
            -32602,
            json!(3),
        ),
        (
            json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "inspect_script", "arguments": [1]}}),
            -32602,
            json!(4),
        ),
        (json!([]), -32600, Value::Null),
    ];
    for (msg, code, id) in cases {
        let r = f.raw(msg.clone());
        assert_eq!(error_code(&r), code, "{msg}: {r}");
        assert_eq!(r["id"], id, "{msg}");
        assert_eq!(r["jsonrpc"], "2.0");
    }
    // Notifications and responses get no answer, even unknown ones.
    assert_eq!(
        f.server
            .handle(json!({"jsonrpc": "2.0", "method": "notifications/whatever"})),
        None
    );
    assert_eq!(
        f.server
            .handle(json!({"jsonrpc": "2.0", "id": 9, "result": {}})),
        None
    );
    // A batch answers each request in it.
    let r = f.raw(json!([
        {"jsonrpc": "2.0", "id": 1, "method": "ping"},
        {"jsonrpc": "2.0", "method": "notifications/initialized"},
        {"jsonrpc": "2.0", "id": 2, "method": "nope"}
    ]));
    assert_eq!(r.as_array().unwrap().len(), 2);
    // Arguments that don't fit the schema are a tool error the model can read.
    let (kind, e) = f
        .tool("preview_page", json!({"path": "/a.pdf"}))
        .unwrap_err();
    assert_eq!(kind, "invalid_input");
    assert!(e.contains("`page` is required"), "{e}");
    let (_, e) = f
        .tool("preview_page", json!({"path": "/a.pdf", "page": 0}))
        .unwrap_err();
    assert!(e.contains("page: must be at least 1"), "{e}");
    let (_, e) = f
        .tool("inspect_script", json!({"path": "/a.pdf", "bogus": 1}))
        .unwrap_err();
    assert!(e.contains("unknown property `bogus`"), "{e}");
    let (_, e) = f
        .tool("make_ebook", json!({"path": "/a.pdf", "formats": ["kfx"]}))
        .unwrap_err();
    assert!(e.contains("formats[0]: must be one of"), "{e}");
    let (_, e) = f
        .tool(
            "convert_to_script",
            json!({"path": "/a.pdf", "formats": []}),
        )
        .unwrap_err();
    assert!(e.contains("needs at least 1 item"), "{e}");
}

/// Every refusal of a path comes before the command line runs, with a
/// message that says what to do.
#[test]
fn paths_are_refused_before_anything_runs() {
    let mut f = Fixture::new("paths", false);
    std::fs::create_dir_all(f.dir.0.join("sub")).unwrap();
    std::fs::write(f.dir.abs("notes.docx"), "x").unwrap();
    let pdf = f.dir.abs("script.pdf");
    std::fs::write(&pdf, "x").unwrap();
    std::fs::write(f.dir.abs("script.fountain"), "x").unwrap();
    let elsewhere = Dir::new("paths-elsewhere");
    let outside = elsewhere.0.join("elsewhere.pdf");
    std::fs::write(&outside, "x").unwrap();
    let cases = [
        (
            "inspect_script",
            json!({"path": "script.pdf"}),
            "path_refused",
            "not an absolute path",
        ),
        (
            "preview_page",
            json!({"path": "sub/../script.pdf", "page": 1}),
            "path_refused",
            "not an absolute path",
        ),
        (
            "inspect_script",
            json!({"path": f.dir.abs("missing.pdf")}),
            "not_found",
            "no such file",
        ),
        (
            "inspect_script",
            json!({"path": f.dir.abs("notes.docx")}),
            "path_refused",
            "only works with .pdf files",
        ),
        (
            "convert_to_script",
            json!({"path": f.dir.abs("script.fountain")}),
            "path_refused",
            "only works with .pdf",
        ),
        (
            "make_ebook",
            json!({"path": f.dir.abs("notes.docx")}),
            "path_refused",
            ".pdf, .fountain, .txt",
        ),
        (
            "inspect_script",
            json!({"path": outside.to_string_lossy()}),
            "path_refused",
            "outside the folders",
        ),
        (
            "make_ebook",
            json!({"path": pdf, "output_dir": "sub"}),
            "path_refused",
            "not an absolute path",
        ),
        (
            "make_ebook",
            json!({"path": pdf, "output_dir": f.dir.abs("nowhere")}),
            "not_found",
            "nowhere",
        ),
        (
            "convert_to_script",
            json!({"path": pdf, "output_dir": pdf}),
            "path_refused",
            "not a folder",
        ),
    ];
    for (tool, args, kind, want) in cases {
        let (k, e) = f.tool(tool, args.clone()).unwrap_err();
        assert_eq!(k, kind, "{tool} {args}: {e}");
        assert!(e.contains(want), "{tool} {args}: {e}");
    }
    // Nothing is replaced unless the call says so, and that too is refused
    // before anything runs.
    std::fs::write(f.dir.abs("script.epub"), "mine").unwrap();
    std::fs::write(f.dir.abs("script.fdx"), "mine").unwrap();
    let (k, e) = f.tool("make_ebook", json!({"path": pdf})).unwrap_err();
    assert_eq!(k, "refused");
    assert!(
        e.contains("script.epub already exists; pass overwrite: true"),
        "{e}"
    );
    let (_, e) = f
        .tool(
            "convert_to_script",
            json!({"path": pdf, "formats": ["fdx"]}),
        )
        .unwrap_err();
    assert!(e.contains("script.fdx already exists"), "{e}");
    let (_, e) = f
        .tool(
            "convert_to_script",
            json!({"path": pdf, "formats": ["fountain"]}),
        )
        .unwrap_err();
    assert!(e.contains("script.fountain already exists"), "{e}");
    std::fs::remove_file(f.dir.abs("script.fountain")).unwrap();
    std::fs::create_dir_all(f.dir.0.join("script.images")).unwrap();
    let (_, e) = f
        .tool(
            "convert_to_script",
            json!({"path": pdf, "formats": ["fountain"]}),
        )
        .unwrap_err();
    assert!(e.contains("script.images already exists"), "{e}");
    assert_eq!(std::fs::read(f.dir.abs("script.epub")).unwrap(), b"mine");
}

#[test]
fn resources_and_prompts_are_served() {
    let mut f = Fixture::new("catalog", false);
    let r = f.rpc("resources/list", json!({}));
    let list = r["result"]["resources"].as_array().unwrap();
    assert_eq!(list.len(), 1);
    let uri = list[0]["uri"].as_str().unwrap().to_string();
    let r = f.rpc("resources/read", json!({"uri": uri}));
    let c = &r["result"]["contents"][0];
    assert_eq!(c["uri"], json!(uri));
    assert_eq!(c["mimeType"], "text/markdown");
    let text = c["text"].as_str().unwrap();
    for word in ["PDF", "Fountain", ".fdx", "EPUB", "AZW3", "MOBI"] {
        assert!(text.contains(word), "{word}");
    }
    let r = f.rpc("resources/read", json!({"uri": "method-reader://nothing"}));
    assert_eq!(error_code(&r), -32002);
    assert_eq!(error_code(&f.rpc("resources/read", json!({}))), -32602);
    let r = f.rpc("resources/templates/list", json!({}));
    assert_eq!(r["result"]["resourceTemplates"], json!([]));

    let r = f.rpc("prompts/list", json!({}));
    let prompts = r["result"]["prompts"].as_array().unwrap().clone();
    let names: Vec<&str> = prompts
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["editable_script", "ebook_for_reader"]);
    for p in &prompts {
        assert_eq!(p["arguments"][0]["name"], "path");
        assert_eq!(p["arguments"][0]["required"], true);
    }
    let r = f.rpc(
        "prompts/get",
        json!({"name": "editable_script", "arguments": {"path": "/scripts/field-station.pdf"}}),
    );
    let m = &r["result"]["messages"][0];
    assert_eq!(m["role"], "user");
    let text = m["content"]["text"].as_str().unwrap();
    assert!(text.contains("/scripts/field-station.pdf"), "{text}");
    assert!(text.contains("inspect_script") && text.contains("convert_to_script"));
    let r = f.rpc(
        "prompts/get",
        json!({"name": "ebook_for_reader", "arguments": {"path": "/s.pdf", "reader": "a Kobo"}}),
    );
    assert!(
        r["result"]["messages"][0]["content"]["text"]
            .as_str()
            .unwrap()
            .contains("It is for: a Kobo.")
    );
    for bad in [
        json!({"name": "editable_script"}),
        json!({"name": "editable_script", "arguments": {"path": ""}}),
        json!({"name": "editable_script", "arguments": {"path": "/a.pdf", "x": "y"}}),
        json!({"name": "editable_script", "arguments": {"path": 3}}),
        json!({"name": "nope", "arguments": {}}),
        json!({}),
    ] {
        assert_eq!(
            error_code(&f.rpc("prompts/get", bad.clone())),
            -32602,
            "{bad}"
        );
    }
    for list in ["prompts/list", "resources/list", "resources/templates/list"] {
        let r = f.rpc(list, json!({"cursor": "x"}));
        assert_eq!(error_code(&r), -32602, "{list}");
    }
}

#[test]
fn lines_are_framed_and_bounded() {
    let mut input: &[u8] = b"{\"a\":1}\r\n\n   \nnot json\n";
    let mut buf = Vec::new();
    assert_eq!(read_line(&mut input, 100, &mut buf).unwrap(), Some(true));
    assert_eq!(
        parse_line(&buf, true),
        Some(Incoming::Message(json!({"a": 1})))
    );
    assert_eq!(read_line(&mut input, 100, &mut buf).unwrap(), Some(true));
    assert_eq!(parse_line(&buf, true), None);
    assert_eq!(read_line(&mut input, 100, &mut buf).unwrap(), Some(true));
    assert_eq!(parse_line(&buf, true), None);
    assert_eq!(read_line(&mut input, 100, &mut buf).unwrap(), Some(true));
    assert!(
        matches!(parse_line(&buf, true), Some(Incoming::Invalid(e)) if e.starts_with("Parse error"))
    );
    assert_eq!(read_line(&mut input, 100, &mut buf).unwrap(), None);

    // A line over the limit is refused, and the next one still reads.
    let long = format!("{}\n{{\"b\":2}}\n", "x".repeat(50));
    let mut input = std::io::BufReader::with_capacity(8, long.as_bytes());
    assert_eq!(read_line(&mut input, 20, &mut buf).unwrap(), Some(false));
    assert!(
        matches!(parse_line(&buf, false), Some(Incoming::Invalid(e)) if e.contains(&limits::MAX_MESSAGE_BYTES.to_string()))
    );
    assert_eq!(read_line(&mut input, 20, &mut buf).unwrap(), Some(true));
    assert_eq!(
        parse_line(&buf, true),
        Some(Incoming::Message(json!({"b": 2})))
    );
    // A last line without a newline still counts.
    let mut input: &[u8] = b"{\"c\":3}";
    assert_eq!(read_line(&mut input, 100, &mut buf).unwrap(), Some(true));
}

fn run_script(server: Server, msgs: Vec<Incoming>) -> Vec<Value> {
    let mut out = Vec::new();
    run(server, Scripted(VecDeque::from(msgs)), &mut out).unwrap();
    String::from_utf8(out)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn the_loop_answers_parse_errors_and_skips_cancelled_requests() {
    let dir = Dir::new("cancel");
    // A script that isn't there: had the cancelled call run, it would have
    // answered (not_found) without converting anything.
    let missing = dir.abs("missing.pdf");
    let msg = Incoming::Message;
    let out = run_script(
        dir.server(false),
        vec![
            msg(
                json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-11-25", "capabilities": {}}}),
            ),
            Incoming::Invalid("Parse error: nope".into()),
            msg(
                json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {"name": "make_ebook", "arguments": {"path": missing}}}),
            ),
            msg(
                json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 5, "reason": "user"}}),
            ),
            msg(json!({"jsonrpc": "2.0", "id": 6, "method": "ping"})),
            // A cancellation for nothing in flight is ignored.
            msg(
                json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 99}}),
            ),
            msg(json!({"jsonrpc": "2.0", "id": 99, "method": "ping"})),
        ],
    );
    let ids: Vec<Value> = out.iter().map(|r| r["id"].clone()).collect();
    assert_eq!(
        ids,
        vec![json!(1), Value::Null, json!(6), json!(99)],
        "{out:?}"
    );
    assert_eq!(out[1]["error"]["code"], -32700);
}

#[test]
fn older_revisions_get_text_only_results() {
    let mut f = Fixture::new("oldrev", false);
    let r = f.rpc(
        "initialize",
        json!({"protocolVersion": "2024-11-05", "capabilities": {}}),
    );
    assert_eq!(r["result"]["protocolVersion"], "2024-11-05");
    let r = f.rpc(
        "tools/call",
        json!({"name": "inspect_script", "arguments": {"path": f.dir.abs("missing.pdf")}}),
    );
    assert!(r["result"].get("structuredContent").is_none());
    assert_eq!(r["result"]["isError"], true);
    assert!(
        r["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("no such file")
    );
}

/// The shape of every cacheable result in each era: revision 2026-07-28
/// requires `resultType` and the caching hints `ttlMs` (an integer, at least
/// 0) and `cacheScope` ("public" or "private") on the result itself, not on
/// each entry; the older revisions have none of them.
#[test]
fn cacheable_results_carry_caching_hints() {
    let dir = Dir::new("cache");
    let cacheable = [
        ("server/discover", json!({})),
        ("tools/list", json!({})),
        ("prompts/list", json!({})),
        ("resources/list", json!({})),
        ("resources/templates/list", json!({})),
        ("resources/read", json!({"uri": "method-reader://formats"})),
    ];
    for read_only in [false, true] {
        let mut s = dir.server(read_only);
        for (id, (method, params)) in cacheable.iter().enumerate() {
            let mut params = params.clone();
            params["_meta"] = meta(MODERN);
            let r = s
                .handle(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
                .unwrap();
            let res = r["result"].as_object().unwrap_or_else(|| panic!("{r}"));
            assert_eq!(res["resultType"], "complete", "{method}");
            assert_eq!(res["ttlMs"], json!(3_600_000), "{method}");
            assert!(
                res["ttlMs"].is_u64(),
                "{method}: ttlMs must be a non-negative integer"
            );
            assert_eq!(res["cacheScope"], "public", "{method}");
            if *method == "tools/list" {
                for tool in res["tools"].as_array().unwrap() {
                    assert!(tool.get("ttlMs").is_none() && tool.get("cacheScope").is_none());
                }
            }
        }
        // Calls aren't cacheable: no hints.
        for (method, params) in [
            ("ping", json!({})),
            (
                "prompts/get",
                json!({"name": "editable_script", "arguments": {"path": "/a.pdf"}}),
            ),
            (
                "tools/call",
                json!({"name": "inspect_script", "arguments": {"path": "a.pdf"}}),
            ),
        ] {
            let mut params = params.clone();
            params["_meta"] = meta(MODERN);
            let r = s
                .handle(json!({"jsonrpc": "2.0", "id": 9, "method": method, "params": params}))
                .unwrap();
            assert!(r["result"].get("ttlMs").is_none(), "{method}: {r}");
            assert_eq!(r["result"]["resultType"], "complete", "{method}");
        }
    }
    // The legacy handshake's revisions know nothing of them.
    let mut s = dir.server(false);
    s.handle(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-11-25", "capabilities": {}}}))
        .unwrap();
    for method in ["tools/list", "prompts/list", "resources/list"] {
        let r = s
            .handle(json!({"jsonrpc": "2.0", "id": 2, "method": method}))
            .unwrap();
        assert!(
            r["result"].get("ttlMs").is_none() && r["result"].get("cacheScope").is_none(),
            "{method}"
        );
    }
}

#[test]
fn mcp_is_a_subcommand_only_where_a_verb_would_be() {
    let argv = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert_eq!(
        super::command(&argv(&["mcp", "--root", "/x"])),
        Some(&argv(&["--root", "/x"])[..])
    );
    assert_eq!(super::command(&argv(&["script.pdf", "mcp"])), None);
    assert_eq!(super::command(&argv(&["--json", "mcp"])), None);
    assert_eq!(super::command(&argv(&[])), None);
}

/// Every tools/call writes one stderr line, however it ended: a tool that
/// panics included (method-budget #55; it used to return before the line).
#[test]
fn every_call_ending_has_its_log_line() {
    assert_eq!(
        call_log_line("inspect_script", Called::Ok, 12),
        "method-reader mcp: inspect_script ok (12 ms)"
    );
    assert_eq!(
        call_log_line("make_ebook", Called::Refused("invalid"), 3),
        "method-reader mcp: make_ebook refused: invalid (3 ms)"
    );
    assert_eq!(
        call_log_line("preview_page", Called::Panicked, 7),
        "method-reader mcp: preview_page panicked (7 ms)"
    );
}
