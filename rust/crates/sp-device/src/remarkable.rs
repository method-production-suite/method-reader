//! src/device/remarkable.ts. reMarkable tablets never mount as a volume (no
//! mass storage, no MTP). With "USB web interface" on in the tablet's
//! storage settings, the device serves plain HTTP on its fixed USB-ethernet
//! address, and files are added with a multipart POST. EPUB and PDF only.
//!
//! The HTTP client sits behind [`HttpClient`], so tests can use a fake; the
//! real one is ureq, pure Rust, with no TLS compiled in (the interface is
//! plain HTTP).

use sp_fsutil::NodeIoError;
use sp_fsutil::nodepath::host;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::path::Path;
use std::time::Duration;

/// The tablet's fixed address on the USB link, the same on every unit.
pub const REMARKABLE_USB_ADDRESS: &str = "10.11.99.1";
pub const REMARKABLE_ENDPOINT: &str = "http://10.11.99.1";

/// Paper Pro's web interface caps uploads here.
pub const REMARKABLE_MAX_UPLOAD_BYTES: u64 = 100 * 1024 * 1024;

/// How long the presence probe waits. One constant: list_devices always
/// passes an explicit value.
pub const REMARKABLE_PROBE_TIMEOUT: Duration = Duration::from_millis(1500);

/// How long the listing taken before an upload may take.
pub const LISTING_TIMEOUT: Duration = Duration::from_secs(10);
/// How long the upload itself may take.
pub const UPLOAD_TIMEOUT: Duration = Duration::from_secs(30);

/// What Bun's fetch says when it cannot reach the tablet at all (refused,
/// no route, no such host). A send's failure line shows it as it is.
pub const UNABLE_TO_CONNECT: &str = "Unable to connect. Is the computer able to access the url?";
/// What Bun's fetch says when its AbortSignal timeout fires.
pub const TIMED_OUT: &str = "The operation timed out.";

/// One file in a multipart form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilePart {
    pub field: String,
    pub filename: String,
    pub mime: String,
    pub bytes: Vec<u8>,
}

/// The two requests the tablet's interface needs. Each answers the HTTP
/// status, or the words a failed fetch would have thrown.
pub trait HttpClient: Send + Sync {
    fn get(&self, url: &str, timeout: Duration) -> Result<u16, String>;
    fn post_file(&self, url: &str, part: &FilePart, timeout: Duration) -> Result<u16, String>;
}

/// `RemarkableUploadError` and the failures before it: the message is what
/// `send` reports as `send-failed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemarkableUploadError {
    pub message: String,
}

impl std::fmt::Display for RemarkableUploadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for RemarkableUploadError {}

fn failed(message: impl Into<String>) -> RemarkableUploadError {
    RemarkableUploadError {
        message: message.into(),
    }
}

/// `new URL(rel, endpoint.replace(/\/$/, '') + '/')` for the two relative
/// paths used here.
fn url_for(endpoint: &str, rel: &str) -> String {
    let base = endpoint.strip_suffix('/').unwrap_or(endpoint);
    format!("{base}/{rel}")
}

/// The root-folder listing URL. Listing is also STATE on the tablet: /upload
/// has no destination parameter and writes into whichever folder the
/// interface listed last, so this GET doubles as the aim taken immediately
/// before every shot.
pub fn documents_url(endpoint: &str) -> String {
    url_for(endpoint, "documents/")
}

/// True when the USB web interface answers 200: the tablet is docked over
/// USB with the interface on. Cheap enough to poll; never fails.
pub fn probe_remarkable(client: &dyn HttpClient, endpoint: &str, timeout: Duration) -> bool {
    matches!(client.get(&documents_url(endpoint), timeout), Ok(200))
}

fn ext_of(file: &str) -> String {
    host::extname(file).trim_start_matches('.').to_lowercase()
}

/// The two formats the tablet's interface accepts. `send` asks this BEFORE
/// it uploads, rather than matching the upload error's words.
pub fn remarkable_accepts(file: &str) -> bool {
    let ext = ext_of(file);
    ext == "pdf" || ext == "epub"
}

/// The refusal for a file the tablet cannot take, word for word (`send`
/// reports the same sentence as unsupported-file).
pub fn not_accepted_message(file: &str) -> String {
    format!("reMarkable accepts PDF and EPUB, not .{}.", ext_of(file))
}

/// Upload a PDF or EPUB to the tablet's root folder. "Root" is made true,
/// not assumed: /upload writes into the last-listed folder (server-side
/// state), so root is listed first and a failed listing stops the send
/// rather than fire blind into the wrong folder. A file that cannot be
/// read, or is over the cap, stops it before any request.
pub fn upload_to_remarkable(
    client: &dyn HttpClient,
    file: &str,
    endpoint: &str,
) -> Result<(), RemarkableUploadError> {
    if !remarkable_accepts(file) {
        return Err(failed(not_accepted_message(file)));
    }
    let size = std::fs::metadata(file)
        .map_err(|e| failed(NodeIoError::from_io(&e, "stat", Some(Path::new(file)), None).message))?
        .len();
    if size > REMARKABLE_MAX_UPLOAD_BYTES {
        return Err(failed(format!(
            "this file is {} MB; the tablet's USB web interface accepts up to 100 MB.",
            size / (1024 * 1024)
        )));
    }

    let listing = client
        .get(&documents_url(endpoint), LISTING_TIMEOUT)
        .map_err(failed)?;
    if listing != 200 {
        return Err(failed(format!(
            "couldn't open the tablet's root folder (HTTP {listing}); nothing was uploaded."
        )));
    }

    let bytes = std::fs::read(file).map_err(|e| {
        failed(NodeIoError::from_io(&e, "open", Some(Path::new(file)), None).message)
    })?;
    let mime = if ext_of(file) == "pdf" {
        "application/pdf"
    } else {
        "application/epub+zip"
    };
    let part = FilePart {
        field: "file".into(),
        filename: host::basename(file),
        mime: mime.into(),
        bytes,
    };
    let status = client
        .post_file(&url_for(endpoint, "upload"), &part, UPLOAD_TIMEOUT)
        .map_err(failed)?;
    if !(200..300).contains(&status) {
        return Err(failed(format!("reMarkable upload failed (HTTP {status}).")));
    }
    Ok(())
}

/// The multipart body Bun's FormData sends: one part, CRLF lines, and the
/// filename written as it is (Bun does not escape it).
pub fn multipart_body(part: &FilePart, boundary: &str) -> Vec<u8> {
    let mut body = Vec::with_capacity(part.bytes.len() + 256);
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\nContent-Type: {}\r\n\r\n",
            part.field, part.filename, part.mime
        )
        .as_bytes(),
    );
    body.extend_from_slice(&part.bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

/// A boundary in Bun's shape: `----WebKitFormBoundary` and 32 random hex.
pub fn new_boundary() -> String {
    let mut hex = String::from("----WebKitFormBoundary");
    for _ in 0..2 {
        let mut h = RandomState::new().build_hasher();
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
        );
        hex.push_str(&format!("{:016x}", h.finish()));
    }
    hex
}

/// The real client: ureq, plain HTTP, every status an answer (a 404 or a
/// 500 is the tablet's word, not a failed request).
#[derive(Debug, Clone, Copy, Default)]
pub struct UreqClient;

impl UreqClient {
    fn agent(timeout: Duration) -> ureq::Agent {
        ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .http_status_as_error(false)
            .build()
            .into()
    }
}

fn words(err: ureq::Error) -> String {
    match err {
        ureq::Error::Timeout(_) => TIMED_OUT.into(),
        ureq::Error::Io(e) if e.kind() == std::io::ErrorKind::TimedOut => TIMED_OUT.into(),
        _ => UNABLE_TO_CONNECT.into(),
    }
}

impl HttpClient for UreqClient {
    fn get(&self, url: &str, timeout: Duration) -> Result<u16, String> {
        let response = UreqClient::agent(timeout).get(url).call().map_err(words)?;
        Ok(response.status().as_u16())
    }

    fn post_file(&self, url: &str, part: &FilePart, timeout: Duration) -> Result<u16, String> {
        let boundary = new_boundary();
        let body = multipart_body(part, &boundary);
        let response = UreqClient::agent(timeout)
            .post(url)
            .header(
                "Content-Type",
                &format!("multipart/form-data; boundary={boundary}"),
            )
            .send(&body[..])
            .map_err(words)?;
        Ok(response.status().as_u16())
    }
}
