//! Just enough HTTP/1.1 for the capture: a server for the pages Chrome
//! draws, and a client for Chrome's /json endpoints (and the tests).
//!
//! Every exchange is one request on one connection (`Connection: close`).
//! The server answers each connection on a thread of its own, apart from
//! the thread that drives Chrome: the TypeScript tool once hung for good
//! because the process serving the pages was blocked waiting on Chrome,
//! which was waiting on the pages.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// The most a request's head or body may be: the pages ask for files and
/// /engine carries an argv, so anything bigger is not the window.
const MAX_HEAD: usize = 64 * 1024;
const MAX_BODY: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Default)]
pub struct Request {
    pub method: String,
    /// The request target as sent: path and query, still percent-encoded.
    pub target: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    /// A GET for `target` naming `host`, as a test builds one.
    pub fn get(host: &str, target: &str) -> Request {
        Request {
            method: "GET".into(),
            target: target.into(),
            headers: vec![("host".into(), host.into())],
            body: Vec::new(),
        }
    }

    pub fn with_header(mut self, name: &str, value: &str) -> Request {
        self.headers.retain(|(n, _)| !n.eq_ignore_ascii_case(name));
        self.headers.push((name.into(), value.into()));
        self
    }

    /// A header's value, by case-insensitive name.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// The path, before any `?`, still encoded.
    pub fn path(&self) -> &str {
        self.target.split(['?', '#']).next().unwrap_or("")
    }

    /// A query parameter, decoded as URLSearchParams decodes one.
    pub fn query(&self, name: &str) -> Option<String> {
        let q = self.target.split_once('?')?.1;
        let q = q.split('#').next().unwrap_or("");
        q.split('&').find_map(|pair| {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            (form_decode(k) == name).then(|| form_decode(v))
        })
    }
}

fn form_decode(s: &str) -> String {
    let bytes = percent_decode_lossy(&s.replace('+', " "));
    String::from_utf8_lossy(&bytes).into_owned()
}

/// `%XX` decoded; a malformed escape is kept as it is (URLSearchParams).
fn percent_decode_lossy(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let Some(x) = b.get(i + 1..i + 3).and_then(hex_pair)
        {
            out.push(x);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

fn hex_pair(two: &[u8]) -> Option<u8> {
    let s = std::str::from_utf8(two).ok()?;
    u8::from_str_radix(s, 16).ok()
}

/// decodeURIComponent: every `%XX` decoded, and the whole must be UTF-8.
/// A malformed escape or bad UTF-8 is an error, as it throws in JavaScript.
pub fn decode_uri_component(s: &str) -> Result<String, String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let x = b
                .get(i + 1..i + 3)
                .and_then(hex_pair)
                .ok_or("a malformed escape")?;
            out.push(x);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "not UTF-8 once decoded".to_string())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
}

impl Response {
    pub fn new(status: u16, content_type: &str, body: Vec<u8>) -> Response {
        Response {
            status,
            content_type: content_type.into(),
            body,
        }
    }

    /// Plain text, as the capture's own answers are.
    pub fn say(body: &str, status: u16) -> Response {
        Response::new(
            status,
            "text/plain; charset=utf-8",
            body.as_bytes().to_vec(),
        )
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Status",
    }
}

/// Reads one request off a connection. None when the peer sent nothing
/// usable (a closed socket, a malformed head).
pub fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    let mut head_len = 0;
    reader.read_line(&mut line).ok()?;
    head_len += line.len();
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();
    let mut headers = Vec::new();
    loop {
        let mut h = String::new();
        let n = reader.read_line(&mut h).ok()?;
        head_len += n;
        if n == 0 || head_len > MAX_HEAD {
            return None;
        }
        let h = h.trim_end_matches(['\r', '\n']);
        if h.is_empty() {
            break;
        }
        let (name, value) = h.split_once(':')?;
        headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
    }
    let length: usize = headers
        .iter()
        .find(|(n, _)| n == "content-length")
        .and_then(|(_, v)| v.parse().ok())
        .unwrap_or(0);
    if length > MAX_BODY {
        return None;
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    Some(Request {
        method,
        target,
        headers,
        body,
    })
}

pub fn write_response(stream: &mut TcpStream, r: &Response) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {} {}\r\ncontent-type: {}\r\ncontent-length: {}\r\ncache-control: no-store\r\nconnection: close\r\n\r\n",
        r.status,
        reason(r.status),
        r.content_type,
        r.body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(&r.body)?;
    stream.flush()
}

/// A running server: its address, and how to stop it.
pub struct Server {
    pub addr: SocketAddr,
    stop: Arc<AtomicBool>,
    accept: Option<JoinHandle<()>>,
}

impl Server {
    /// Serves on 127.0.0.1 at a port the OS picks, calling `handle` for
    /// every request on a thread per connection.
    pub fn start<F>(handle: F) -> std::io::Result<Server>
    where
        F: Fn(Request) -> Response + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let addr = listener.local_addr()?;
        let stop = Arc::new(AtomicBool::new(false));
        let handle = Arc::new(handle);
        let stopping = Arc::clone(&stop);
        let accept = std::thread::Builder::new()
            .name("capture-server".into())
            .spawn(move || {
                for conn in listener.incoming() {
                    if stopping.load(Ordering::SeqCst) {
                        break;
                    }
                    let Ok(mut stream) = conn else { continue };
                    let handle = Arc::clone(&handle);
                    let _ = std::thread::Builder::new()
                        .name("capture-request".into())
                        .spawn(move || {
                            let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
                            if let Some(req) = read_request(&mut stream) {
                                let r = handle(req);
                                let _ = write_response(&mut stream, &r);
                            }
                            let _ = stream.shutdown(Shutdown::Both);
                        });
                }
            })?;
        Ok(Server {
            addr,
            stop,
            accept: Some(accept),
        })
    }

    pub fn port(&self) -> u16 {
        self.addr.port()
    }

    /// Stops accepting. Answers false if the accept loop did not end within
    /// `within`. Requests already being answered finish on their own.
    pub fn stop(&mut self, within: Duration) -> bool {
        self.stop.store(true, Ordering::SeqCst);
        // Wakes the accept loop, which is blocked in accept().
        let _ = TcpStream::connect_timeout(&self.addr, Duration::from_secs(1));
        let Some(accept) = self.accept.take() else {
            return true;
        };
        let start = Instant::now();
        while !accept.is_finished() {
            if start.elapsed() > within {
                return false;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = accept.join();
        true
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop(Duration::from_secs(5));
    }
}

/// One request to `http://host:port/target`, answered as (status, body).
/// `headers` are added to Host and Content-Length.
pub fn fetch(
    method: &str,
    url: &str,
    headers: &[(&str, &str)],
    body: &[u8],
    timeout: Duration,
) -> Result<(u16, Vec<u8>), String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| format!("not an http:// URL: {url}"))?;
    let (host, target) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let addr: SocketAddr = host
        .parse()
        .map_err(|_| format!("not an address this client can reach: {host}"))?;
    let mut s = TcpStream::connect_timeout(&addr, timeout).map_err(|e| e.to_string())?;
    s.set_read_timeout(Some(timeout))
        .map_err(|e| e.to_string())?;
    s.set_write_timeout(Some(timeout))
        .map_err(|e| e.to_string())?;
    let mut head = format!(
        "{method} {target} HTTP/1.1\r\nhost: {host}\r\nconnection: close\r\ncontent-length: {}\r\n",
        body.len()
    );
    for (n, v) in headers {
        head.push_str(&format!("{n}: {v}\r\n"));
    }
    head.push_str("\r\n");
    s.write_all(head.as_bytes()).map_err(|e| e.to_string())?;
    s.write_all(body).map_err(|e| e.to_string())?;
    // Read to the end of the body the head promises, not to the end of the
    // connection: a server may keep it open whatever this asked for.
    let mut raw = Vec::new();
    let mut buf = [0u8; 16 * 1024];
    loop {
        if let Some(split) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
            let head = String::from_utf8_lossy(&raw[..split]).to_ascii_lowercase();
            let length = head.lines().find_map(|l| {
                l.strip_prefix("content-length:")
                    .and_then(|v| v.trim().parse::<usize>().ok())
            });
            if length.is_some_and(|n| raw.len() >= split + 4 + n) {
                raw.truncate(split + 4 + length.unwrap_or(0));
                break;
            }
        }
        match s.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => raw.extend_from_slice(&buf[..n]),
            Err(e) => return Err(e.to_string()),
        }
    }
    parse_response(&raw)
}

fn parse_response(raw: &[u8]) -> Result<(u16, Vec<u8>), String> {
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("an HTTP answer with no end to its head")?;
    let head = String::from_utf8_lossy(&raw[..split]);
    let status: u16 = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or("an HTTP answer with no status")?;
    let mut body = raw[split + 4..].to_vec();
    let chunked = head.lines().any(|l| {
        l.split_once(':').is_some_and(|(n, v)| {
            n.trim().eq_ignore_ascii_case("transfer-encoding")
                && v.trim().eq_ignore_ascii_case("chunked")
        })
    });
    if chunked {
        body = dechunk(&body)?;
    }
    Ok((status, body))
}

fn dechunk(mut b: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    loop {
        let eol = b
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or("a chunk with no size line")?;
        let size_text = String::from_utf8_lossy(&b[..eol]);
        let size = usize::from_str_radix(size_text.split(';').next().unwrap_or("").trim(), 16)
            .map_err(|_| "a chunk size that is not hex")?;
        b = &b[eol + 2..];
        if size == 0 {
            return Ok(out);
        }
        if b.len() < size + 2 {
            return Err("a chunk shorter than it said".into());
        }
        out.extend_from_slice(&b[..size]);
        b = &b[size + 2..];
    }
}
