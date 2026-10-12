//! What the window asks of the computer it runs on, apart from the engine:
//! native dialogs, opening a web page, the request service, its own small
//! storage, the clock, the platform, Reduce Motion and the updater. Real
//! ones for the window, fakes for the tests, so no test opens a dialog,
//! a browser or a socket.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::Value;

use crate::model::convert::Platform;
use crate::model::update::{Found, MemoryStorage, Storage};

pub trait Dialogs {
    /// The open dialog, filtered to screenplays (pdf, fountain, txt).
    fn pick_screenplay(&self) -> Option<String>;
    fn pick_folder(&self, start: &str) -> Option<String>;
    fn save(&self, name: &str, filter: &(String, Vec<String>)) -> Option<String>;
}

pub trait Opener {
    fn open_url(&self, url: &str) -> bool;
}

pub trait Poster: Send + Sync {
    /// POST JSON; the status and the parsed body, or None for no answer.
    fn post(&self, url: &str, body: &Value) -> Option<(u16, Option<Value>)>;
}

/// One of the typewriter's two sounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    Key,
    Ding,
}

pub trait Speaker {
    /// Play a sound now; a computer that cannot is quiet.
    fn play(&mut self, sound: Sound);
}

pub trait Updater {
    /// Whether this build can update itself at all.
    fn ready(&self) -> bool;
    fn check(&self) -> Result<Option<Found>, String>;
}

/// The pages the window may open, and nothing else (capabilities/
/// default.json's opener scope).
pub fn allowed_url(url: &str) -> bool {
    url.starts_with("https://github.com/ssandweiss/screepub/")
        || matches!(
            url,
            "https://calibre-ebook.com/download_osx"
                | "https://calibre-ebook.com/download_windows"
                | "https://kdp.amazon.com/en_US/help/topic/G202131170"
        )
}

pub struct Host {
    pub dialogs: Box<dyn Dialogs>,
    pub opener: Box<dyn Opener>,
    pub poster: Arc<dyn Poster>,
    pub storage: Box<dyn Storage>,
    pub clock: Box<dyn Fn() -> f64>,
    pub platform: Platform,
    pub reduced_motion: Arc<AtomicBool>,
    pub updater: Box<dyn Updater>,
    pub speaker: Box<dyn Speaker>,
}

impl Host {
    pub fn reduced_motion(&self) -> bool {
        self.reduced_motion.load(Ordering::Relaxed)
    }

    /// Open a page, if it is one the window may open.
    pub fn open(&self, url: &str) -> bool {
        allowed_url(url) && self.opener.open_url(url)
    }
}

// ------------------------------------------------------------------ real

pub struct NativeDialogs;

impl Dialogs for NativeDialogs {
    fn pick_screenplay(&self) -> Option<String> {
        rfd::FileDialog::new()
            .add_filter("Screenplay", &["pdf", "fountain", "txt"])
            .pick_file()
            .map(|p| p.to_string_lossy().into_owned())
    }

    fn pick_folder(&self, start: &str) -> Option<String> {
        rfd::FileDialog::new()
            .set_directory(start)
            .pick_folder()
            .map(|p| p.to_string_lossy().into_owned())
    }

    fn save(&self, name: &str, filter: &(String, Vec<String>)) -> Option<String> {
        let exts: Vec<&str> = filter.1.iter().map(String::as_str).collect();
        rfd::FileDialog::new()
            .set_file_name(name)
            .add_filter(&filter.0, &exts)
            .save_file()
            .map(|p| p.to_string_lossy().into_owned())
    }
}

pub struct Browser;

impl Opener for Browser {
    fn open_url(&self, url: &str) -> bool {
        webbrowser::open(url).is_ok()
    }
}

pub struct Http;

impl Poster for Http {
    fn post(&self, url: &str, body: &Value) -> Option<(u16, Option<Value>)> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_millis(
                crate::model::requests::TIMEOUT_MS,
            )))
            .http_status_as_error(false)
            .build()
            .into();
        let mut response = agent.post(url).send_json(body).ok()?;
        let status = response.status().as_u16();
        let parsed = response.body_mut().read_json::<Value>().ok();
        Some((status, parsed))
    }
}

/// The window's own string storage (localStorage in the webview): one JSON
/// file beside the app's settings, written on every change.
pub struct FileStorage {
    path: std::path::PathBuf,
    data: MemoryStorage,
}

impl FileStorage {
    pub fn open(path: std::path::PathBuf) -> Self {
        let data = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| {
                serde_json::from_str::<std::collections::HashMap<String, String>>(&s).ok()
            })
            .map(MemoryStorage)
            .unwrap_or_default();
        Self { path, data }
    }

    /// Where the window keeps it: the platform's per-user config folder.
    pub fn default_path() -> Option<std::path::PathBuf> {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
        let home = std::path::PathBuf::from(home);
        let base = if cfg!(target_os = "macos") {
            home.join("Library/Application Support")
        } else if cfg!(target_os = "windows") {
            std::env::var_os("APPDATA").map(Into::into).unwrap_or(home)
        } else {
            std::env::var_os("XDG_CONFIG_HOME")
                .map(Into::into)
                .unwrap_or(home.join(".config"))
        };
        Some(
            base.join("com.darkwell.screepub")
                .join("window-storage.json"),
        )
    }
}

impl Storage for FileStorage {
    fn get_item(&self, key: &str) -> Option<String> {
        self.data.get_item(key)
    }
    fn set_item(&mut self, key: &str, value: &str) {
        self.data.set_item(key, value);
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string(&self.data.0) {
            let _ = std::fs::write(&self.path, text);
        }
    }
}

/// No updater in this build: wave 3 brings the transport.
pub struct NoUpdater;

impl Updater for NoUpdater {
    fn ready(&self) -> bool {
        false
    }
    fn check(&self) -> Result<Option<Found>, String> {
        Err("this build cannot update itself".into())
    }
}

impl Host {
    /// The window's own: native dialogs, the browser, HTTP, a storage file,
    /// a monotonic clock, Reduce Motion as the OS reports it.
    pub fn native(reduced_motion: Arc<AtomicBool>) -> Self {
        let start = std::time::Instant::now();
        let storage: Box<dyn Storage> = match FileStorage::default_path() {
            Some(path) => Box::new(FileStorage::open(path)),
            None => Box::new(MemoryStorage::default()),
        };
        Self {
            dialogs: Box::new(NativeDialogs),
            opener: Box::new(Browser),
            poster: Arc::new(Http),
            storage,
            clock: Box::new(move || start.elapsed().as_secs_f64() * 1000.0),
            platform: Platform::current(),
            reduced_motion,
            updater: Box::new(NoUpdater),
            speaker: Box::new(Typewriter::default()),
        }
    }
}

/// The real speaker, opened at the first sound. If opening fails it is
/// not tried again (typewriter.js's `broken`).
#[derive(Default)]
struct Typewriter {
    speaker: Option<super::sound::Speaker>,
    broken: bool,
}

impl Speaker for Typewriter {
    fn play(&mut self, sound: Sound) {
        if self.broken {
            return;
        }
        if self.speaker.is_none() {
            self.speaker = super::sound::Speaker::open();
            self.broken = self.speaker.is_none();
        }
        if let Some(speaker) = self.speaker.as_mut() {
            match sound {
                Sound::Key => speaker.key(),
                Sound::Ding => speaker.ding(),
            }
        }
    }
}

// ------------------------------------------------------------------ fakes

/// A speaker that remembers what it was asked to play.
#[derive(Clone, Default)]
pub struct FakeSpeaker {
    pub played: Arc<Mutex<Vec<Sound>>>,
}

impl Speaker for FakeSpeaker {
    fn play(&mut self, sound: Sound) {
        self.played.lock().expect("fake").push(sound);
    }
}

/// Dialogs that answer from a script, and remember being asked.
#[derive(Clone, Default)]
pub struct FakeDialogs {
    pub answers: Arc<Mutex<Vec<Option<String>>>>,
    pub asked: Arc<Mutex<Vec<String>>>,
}

impl FakeDialogs {
    fn next(&self, what: String) -> Option<String> {
        self.asked.lock().expect("fake").push(what);
        let mut answers = self.answers.lock().expect("fake");
        if answers.is_empty() {
            None
        } else {
            answers.remove(0)
        }
    }
}

impl Dialogs for FakeDialogs {
    fn pick_screenplay(&self) -> Option<String> {
        self.next("open".into())
    }
    fn pick_folder(&self, start: &str) -> Option<String> {
        self.next(format!("folder {start}"))
    }
    fn save(&self, name: &str, _filter: &(String, Vec<String>)) -> Option<String> {
        self.next(format!("save {name}"))
    }
}

#[derive(Clone, Default)]
pub struct FakeOpener {
    pub opened: Arc<Mutex<Vec<String>>>,
}

impl Opener for FakeOpener {
    fn open_url(&self, url: &str) -> bool {
        self.opened.lock().expect("fake").push(url.into());
        true
    }
}

#[derive(Default)]
pub struct FakePoster {
    pub sent: Mutex<Vec<(String, Value)>>,
    pub answer: Mutex<Option<(u16, Option<Value>)>>,
}

impl Poster for FakePoster {
    fn post(&self, url: &str, body: &Value) -> Option<(u16, Option<Value>)> {
        self.sent
            .lock()
            .expect("fake")
            .push((url.into(), body.clone()));
        self.answer.lock().expect("fake").clone()
    }
}

pub struct FakeUpdater {
    pub ready: bool,
    pub found: Result<Option<Found>, String>,
}

impl Updater for FakeUpdater {
    fn ready(&self) -> bool {
        self.ready
    }
    fn check(&self) -> Result<Option<Found>, String> {
        self.found.clone()
    }
}

/// A clock the test turns by hand.
#[derive(Clone, Default)]
pub struct FakeClock(pub Arc<Mutex<f64>>);

impl FakeClock {
    pub fn advance(&self, ms: f64) {
        *self.0.lock().expect("clock") += ms;
    }
    pub fn now(&self) -> f64 {
        *self.0.lock().expect("clock")
    }
}

/// Everything a test needs to see what the window did outside itself.
#[derive(Clone, Default)]
pub struct Fakes {
    pub dialogs: FakeDialogs,
    pub opener: FakeOpener,
    pub poster: Arc<FakePoster>,
    pub clock: FakeClock,
    pub reduced_motion: Arc<AtomicBool>,
    pub speaker: FakeSpeaker,
}

impl Fakes {
    pub fn host(&self, platform: Platform) -> Host {
        let clock = self.clock.clone();
        Host {
            dialogs: Box::new(self.dialogs.clone()),
            opener: Box::new(self.opener.clone()),
            poster: self.poster.clone(),
            storage: Box::new(MemoryStorage::default()),
            clock: Box::new(move || clock.now()),
            platform,
            reduced_motion: self.reduced_motion.clone(),
            updater: Box::new(FakeUpdater {
                ready: platform == Platform::Mac,
                found: Ok(None),
            }),
            speaker: Box::new(self.speaker.clone()),
        }
    }

    /// Queue what the next native dialog answers.
    pub fn answer_dialog(&self, path: Option<&str>) {
        self.dialogs
            .answers
            .lock()
            .expect("fake")
            .push(path.map(str::to_string));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_opens_only_the_pages_it_was_granted() {
        assert!(allowed_url(
            "https://github.com/ssandweiss/screepub/issues/new?body=x"
        ));
        assert!(allowed_url("https://calibre-ebook.com/download_osx"));
        assert!(allowed_url(
            "https://kdp.amazon.com/en_US/help/topic/G202131170"
        ));
        assert!(!allowed_url("https://github.com/someone-else/repo"));
        assert!(!allowed_url("https://calibre-ebook.com/download_linux"));
        assert!(!allowed_url("file:///etc/passwd"));
    }

    #[test]
    fn a_page_outside_the_grant_is_refused_before_the_browser_hears_of_it() {
        let fakes = Fakes::default();
        let host = fakes.host(Platform::Mac);
        assert!(!host.open("https://example.com"));
        assert!(host.open("https://calibre-ebook.com/download_windows"));
        assert_eq!(fakes.opener.opened.lock().unwrap().len(), 1);
    }
}
