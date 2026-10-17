//! What the window asks of the computer it runs on, apart from the engine:
//! native dialogs, opening a web page, the request service, its own small
//! storage, the clock, the platform, Reduce Motion and the updater. Real
//! ones for the window, fakes for the tests, so no test opens a dialog,
//! a browser or a socket.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::Value;

use sp_update::flow::{Event as UpdateEvent, Outcome, Steps};
use sp_update::gate::{ENGINE_QUIET, EngineGate, RESTART_WAIT_CAP};

use crate::model::convert::Platform;
use crate::model::update::{Found, MemoryStorage, Storage};
use crate::model::update_flow::{Offer, RunOutcome};

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

pub trait Updater: Send + Sync {
    /// Whether this build can update itself at all.
    fn ready(&self) -> bool;
    fn check(&self) -> Result<Option<Found>, String>;
    /// Download and verify, ahead of any click, the update the last check
    /// found, and keep it for [`Updater::install`], so "Update ready" means
    /// what it says. The version kept, or why not. On a worker thread.
    fn prepare(&self) -> Result<String, String> {
        Err("this build cannot update itself".into())
    }
    /// update.js installAndRestart, on a worker thread: download, verify,
    /// swap the bundle, retire `gate`, wait for 500 ms of quiet (at most
    /// 120 s), restart. `live`: the offer still holds the update its check
    /// returned; otherwise the run checks afresh first.
    fn install(
        &self,
        _live: bool,
        _gate: &EngineGate,
        emit: &mut dyn FnMut(UpdateEvent),
    ) -> RunOutcome {
        let message = "this build cannot update itself".to_string();
        emit(UpdateEvent::Failed {
            version: None,
            message,
        });
        RunOutcome::Error { offer: None }
    }
    /// update.js restartInstalled ("Restart now"): true when the new copy
    /// started and this one should close.
    fn restart(&self, version: &str, emit: &mut dyn FnMut(UpdateEvent)) -> bool {
        emit(UpdateEvent::Installed {
            version: version.into(),
        });
        false
    }
}

/// sp-update's outcome as the flow reads it.
pub fn run_outcome(outcome: Outcome) -> RunOutcome {
    let offer = |u: sp_update::Update| Offer {
        version: u.version,
        body: u.body.unwrap_or_default(),
        live: true,
    };
    match outcome {
        Outcome::Restarting { .. } => RunOutcome::Restarting,
        Outcome::Stalled { update, .. } => RunOutcome::Stalled {
            offer: Some(offer(update)),
        },
        Outcome::Installed { update, .. } => RunOutcome::Installed {
            offer: Some(offer(update)),
        },
        Outcome::Current => RunOutcome::Current,
        Outcome::Error { update, .. } => RunOutcome::Error {
            offer: update.map(offer),
        },
    }
}

/// The pages the window may open, and nothing else (capabilities/
/// default.json's opener scope, and the Help menu's two websites).
pub fn allowed_url(url: &str) -> bool {
    use crate::model::help::{SUITE, WEBSITE};
    url.starts_with("https://github.com/method-production-suite/method-reader/")
        || url.starts_with("https://github.com/method-production-suite/method-reader#")
        || url == WEBSITE
        || url == SUITE
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
    pub updater: Arc<dyn Updater>,
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

    /// The first run after the handover: no storage file yet, so start from
    /// the update consent the Tauri window kept in WebKit's localStorage
    /// (sp_update::webkit), and write it down so this happens once.
    pub fn open_or_import(
        path: std::path::PathBuf,
        website_data: Option<&std::path::Path>,
    ) -> Self {
        let first_run = !path.exists();
        let mut storage = Self::open(path);
        if first_run && let Some(dir) = website_data {
            let imported = sp_update::webkit::import_update_consent(dir);
            let mut keys: Vec<_> = imported.into_iter().collect();
            keys.sort();
            for (k, v) in keys {
                storage.set_item(&k, &v);
            }
        }
        storage
    }

    /// Where the window keeps it: beside the engine's settings.json, in
    /// the app-data folder named after the bundle identifier, or in
    /// METHOD_READER_CONFIG_DIR (else the LEGACY SCREEPUB_CONFIG_DIR) when
    /// that is set (the engine's own override, which tests and scratch runs
    /// point elsewhere, so the window's storage goes with them). Until the
    /// one-time move has carried it over (sp_engine::legacy), the 0.12
    /// window's own file, under com.darkwell.screepub, is used where it is,
    /// so the update answer stays the person's. A copy built under another
    /// identifier (a rehearsal) keeps to that identifier's folder.
    pub fn default_path() -> Option<std::path::PathBuf> {
        const FILE: &str = "window-storage.json";
        if let Some(dir) = config_override() {
            return Some(dir.join(FILE));
        }
        let running = running_identifier();
        if running == sp_update::IDENTIFIER {
            Some(sp_cli::app_data_file(FILE))
        } else {
            Some(sp_cli::app_data_folder_named(&running).join(FILE))
        }
    }
}

/// The bundle identifier this copy runs under: its own Info.plist's inside
/// a Mac bundle (so a rehearsal built under another identifier keeps to
/// its own storage), else the shipped one, [`sp_update::IDENTIFIER`].
pub fn running_identifier() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|exe| sp_update::install::bundle_of_exe(&exe))
        .and_then(|b| sp_update::install::bundle_identifier(&b))
        .unwrap_or_else(|| sp_update::IDENTIFIER.to_string())
}

/// METHOD_READER_CONFIG_DIR, or the LEGACY SCREEPUB_CONFIG_DIR when the
/// new one is unset, when set to something: the engine's own rule.
fn config_override() -> Option<std::path::PathBuf> {
    use crate::model::names::{CONFIG_DIR_VAR, LEGACY_CONFIG_DIR_VAR};
    [CONFIG_DIR_VAR, LEGACY_CONFIG_DIR_VAR]
        .into_iter()
        .find_map(|n| std::env::var_os(n).filter(|d| !d.is_empty()))
        .map(Into::into)
}

/// The Tauri window's WebKit data for this copy, on a Mac.
fn tauri_website_data() -> Option<std::path::PathBuf> {
    // A scratch run reads nothing of the real Tauri window's.
    if !cfg!(target_os = "macos") || config_override().is_some() {
        return None;
    }
    let home = std::env::var_os("HOME")?;
    let home = std::path::Path::new(&home);
    webkit_identifiers(&running_identifier())
        .iter()
        .map(|id| sp_update::webkit::website_data(home, id))
        .find(|dir| dir.is_dir())
}

/// Whose WebKit storage may hold this copy's update answer, first found
/// first: its own identifier's; then, for Method Reader's identifier (or a
/// rehearsal's, which extends it), the same under the 0.12.x identifier
/// the Tauri windows had, and for the shipped identifier the 0.7.x
/// window's too, since a copy that skipped every release since updates
/// straight to this one.
pub fn webkit_identifiers(running: &str) -> Vec<String> {
    let mut ids = vec![running.to_owned()];
    if let Some(rest) = running.strip_prefix(sp_update::IDENTIFIER) {
        ids.push(format!("{}{rest}", sp_update::LEGACY_IDENTIFIER));
        if rest.is_empty() {
            ids.push(sp_update::LEGACY_TAURI_IDENTIFIER.to_owned());
        }
    }
    ids
}

/// The updater: latest.json through sp-update, with the key every Tauri
/// window trusts. Ready only in a Mac app bundle (inventory row U1: the
/// manifest lists darwin only, and a development build has no bundle to
/// swap).
pub struct ManifestUpdater {
    /// The bundle this copy started from.
    pub bundle: Option<std::path::PathBuf>,
    /// Where the last install put the new version, when it did: the same
    /// path, or `Method Reader.app` beside a `Screepub.app` (the one-time
    /// rename). The restart, "Restart now" and any later install use it,
    /// since the old path may be gone.
    installed: Mutex<Option<std::path::PathBuf>>,
    /// The update the last check returned: the offer's live handle.
    last: Mutex<Option<sp_update::Update>>,
    /// The update [`Updater::prepare`] downloaded and verified: its version
    /// and its bytes, which the next install of that version uses instead
    /// of downloading again.
    prepared: Mutex<Option<(String, Vec<u8>)>>,
}

/// latest.json, asked now: a newer release for this copy, or None.
fn check_now() -> Result<Option<sp_update::Update>, String> {
    let current = sp_update::manifest::parse_version(env!("CARGO_PKG_VERSION"))?;
    sp_update::check(&config(), &current).map_err(|e| e.to_string())
}

fn config() -> sp_update::Config {
    sp_update::Config::method_reader(Some(sp_update::Installer::App))
}

/// The run's side effects, for real: sp-update's transport, its swap, and
/// the gate the window's busy count mirrors into.
struct RealSteps<'a> {
    /// The bundle to swap, and after the swap the one it reported
    /// (`Installed::bundle`), which the restart starts.
    bundle: std::path::PathBuf,
    gate: &'a EngineGate,
    /// What a quiet check already downloaded and verified, if anything.
    prepared: Option<(String, Vec<u8>)>,
}

impl Steps for RealSteps<'_> {
    fn fresh_check(&mut self) -> Result<Option<sp_update::Update>, String> {
        check_now()
    }
    fn download(
        &mut self,
        update: &sp_update::Update,
        progress: &mut dyn FnMut(usize, Option<u64>),
        finished: &mut dyn FnMut(),
    ) -> Result<Vec<u8>, String> {
        if let Some((version, bytes)) = self.prepared.take()
            && version == update.version
        {
            // Verified when it was downloaded; nothing to fetch again.
            progress(bytes.len(), Some(bytes.len() as u64));
            finished();
            return Ok(bytes);
        }
        sp_update::download(&config(), update, progress, finished).map_err(|e| e.to_string())
    }
    fn install(&mut self, _update: &sp_update::Update, bytes: &[u8]) -> Result<(), String> {
        let installed = sp_update::install::install_app(bytes, &self.bundle, &|bundle, new| {
            sp_update::install::admin_swap(bundle, new)
        })
        .map_err(|e| e.to_string())?;
        self.bundle = installed.bundle;
        Ok(())
    }
    fn retire(&mut self) {
        self.gate.retire();
    }
    fn restart_ready(&self) -> bool {
        sp_update::install::bundle_executable(&self.bundle).is_ok()
    }
    fn busy(&self) -> bool {
        self.gate.busy()
    }
    fn wait_quiet(&mut self) -> bool {
        self.gate.wait_quiet(ENGINE_QUIET, RESTART_WAIT_CAP)
    }
    fn restart(&mut self) -> Result<(), String> {
        sp_update::restart::relaunch(&self.bundle).map_err(|e| e.to_string())
    }
}

impl ManifestUpdater {
    pub fn for_this_copy() -> Self {
        let bundle = if cfg!(target_os = "macos") {
            std::env::current_exe()
                .ok()
                .and_then(|exe| sp_update::install::bundle_of_exe(&exe))
        } else {
            None
        };
        Self {
            bundle,
            installed: Mutex::new(None),
            last: Mutex::new(None),
            prepared: Mutex::new(None),
        }
    }

    /// The bundle as it stands now: where the last install put it, else
    /// the one this copy started from.
    fn current_bundle(&self) -> Option<std::path::PathBuf> {
        self.installed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .or_else(|| self.bundle.clone())
    }
}

impl Updater for ManifestUpdater {
    fn ready(&self) -> bool {
        self.bundle.is_some()
    }
    fn check(&self) -> Result<Option<Found>, String> {
        let found = check_now()?;
        *self.last.lock().unwrap_or_else(|e| e.into_inner()) = found.clone();
        Ok(found.map(|u| Found {
            version: u.version,
            current_version: u.current_version,
            body: u.body,
        }))
    }
    fn prepare(&self) -> Result<String, String> {
        if self.current_bundle().is_none() {
            return Err("this copy of Method Reader cannot update itself".into());
        }
        let update = self
            .last
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .ok_or("no update to download")?;
        let held = self
            .prepared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .is_some_and(|(v, _)| *v == update.version);
        if !held {
            let bytes = sp_update::download(&config(), &update, |_, _| {}, || {})
                .map_err(|e| e.to_string())?;
            *self.prepared.lock().unwrap_or_else(|e| e.into_inner()) =
                Some((update.version.clone(), bytes));
        }
        Ok(update.version)
    }
    fn install(
        &self,
        live: bool,
        gate: &EngineGate,
        emit: &mut dyn FnMut(UpdateEvent),
    ) -> RunOutcome {
        let Some(bundle) = self.current_bundle() else {
            emit(UpdateEvent::Failed {
                version: None,
                message: "this copy of Method Reader cannot update itself".into(),
            });
            return RunOutcome::Error { offer: None };
        };
        let update = if live {
            self.last.lock().unwrap_or_else(|e| e.into_inner()).take()
        } else {
            None
        };
        let mut steps = RealSteps {
            bundle: bundle.clone(),
            gate,
            prepared: self
                .prepared
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take(),
        };
        let outcome = sp_update::flow::install_and_restart(update, &mut steps, emit);
        if steps.bundle != bundle {
            *self.installed.lock().unwrap_or_else(|e| e.into_inner()) = Some(steps.bundle);
        }
        run_outcome(outcome)
    }
    fn restart(&self, version: &str, emit: &mut dyn FnMut(UpdateEvent)) -> bool {
        let Some(bundle) = self.current_bundle() else {
            return false;
        };
        let gate = EngineGate::new();
        let mut steps = RealSteps {
            bundle,
            gate: &gate,
            prepared: None,
        };
        sp_update::flow::restart_installed(version, &mut steps, emit)
            == sp_update::flow::RestartOutcome::Restarting
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
            let _ = sp_fsutil::write_file_atomic(&self.path, text);
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
            Some(path) => Box::new(FileStorage::open_or_import(
                path,
                tauri_website_data().as_deref(),
            )),
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
            updater: Arc::new(ManifestUpdater::for_this_copy()),
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

/// A Save box's filter: the type's name and its extensions.
pub type SaveFilter = (String, Vec<String>);

/// Dialogs that answer from a script, and remember being asked.
#[derive(Clone, Default)]
pub struct FakeDialogs {
    pub answers: Arc<Mutex<Vec<Option<String>>>>,
    pub asked: Arc<Mutex<Vec<String>>>,
    /// Each Save box's filter: the type's name and its extensions.
    pub filters: Arc<Mutex<Vec<SaveFilter>>>,
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
    fn save(&self, name: &str, filter: &(String, Vec<String>)) -> Option<String> {
        self.filters.lock().expect("fake").push(filter.clone());
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

/// An updater whose run is sp-update's real flow over fake side effects:
/// the download and the swap succeed at once, and the wait for quiet is
/// the real gate's, with a cap a test can shorten.
pub struct FakeUpdater {
    pub ready: bool,
    pub found: Result<Option<Found>, String>,
    pub can_restart: bool,
    pub quiet: std::time::Duration,
    pub cap: std::time::Duration,
    /// How many times the new copy was started.
    pub restarted: Arc<std::sync::atomic::AtomicUsize>,
    /// How many times an update was downloaded ahead of the click, and
    /// what that download answers (None: the found version, kept).
    pub prepared: Arc<std::sync::atomic::AtomicUsize>,
    pub prepare_fails: Option<String>,
}

impl Default for FakeUpdater {
    fn default() -> Self {
        Self {
            ready: true,
            found: Ok(None),
            can_restart: true,
            quiet: ENGINE_QUIET,
            cap: RESTART_WAIT_CAP,
            restarted: Arc::default(),
            prepared: Arc::default(),
            prepare_fails: None,
        }
    }
}

struct FakeSteps<'a> {
    fake: &'a FakeUpdater,
    gate: &'a EngineGate,
}

impl Steps for FakeSteps<'_> {
    fn fresh_check(&mut self) -> Result<Option<sp_update::Update>, String> {
        Ok(self.fake.found.clone()?.map(fake_update))
    }
    fn download(
        &mut self,
        _update: &sp_update::Update,
        progress: &mut dyn FnMut(usize, Option<u64>),
        finished: &mut dyn FnMut(),
    ) -> Result<Vec<u8>, String> {
        progress(50, Some(100));
        progress(50, Some(100));
        finished();
        Ok(vec![0; 100])
    }
    fn install(&mut self, _update: &sp_update::Update, _bytes: &[u8]) -> Result<(), String> {
        Ok(())
    }
    fn retire(&mut self) {
        self.gate.retire();
    }
    fn restart_ready(&self) -> bool {
        self.fake.can_restart
    }
    fn busy(&self) -> bool {
        self.gate.busy()
    }
    fn wait_quiet(&mut self) -> bool {
        self.gate.wait_quiet(self.fake.quiet, self.fake.cap)
    }
    fn restart(&mut self) -> Result<(), String> {
        self.fake
            .restarted
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }
}

fn fake_update(f: Found) -> sp_update::Update {
    sp_update::Update {
        current_version: f.current_version,
        version: f.version,
        body: f.body,
        date: None,
        target: "darwin-aarch64".into(),
        download_url: "https://example.invalid/Method-Reader-macOS.app.tar.gz".into(),
        signature: String::new(),
        raw_json: Value::Null,
    }
}

impl Updater for FakeUpdater {
    fn ready(&self) -> bool {
        self.ready
    }
    fn check(&self) -> Result<Option<Found>, String> {
        self.found.clone()
    }
    fn prepare(&self) -> Result<String, String> {
        self.prepared
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if let Some(message) = &self.prepare_fails {
            return Err(message.clone());
        }
        match self.found.clone()? {
            Some(f) => Ok(f.version),
            None => Err("no update to download".into()),
        }
    }
    fn install(
        &self,
        live: bool,
        gate: &EngineGate,
        emit: &mut dyn FnMut(UpdateEvent),
    ) -> RunOutcome {
        let update = if live {
            self.found.clone().ok().flatten().map(fake_update)
        } else {
            None
        };
        let mut steps = FakeSteps { fake: self, gate };
        run_outcome(sp_update::flow::install_and_restart(
            update, &mut steps, emit,
        ))
    }
    fn restart(&self, version: &str, emit: &mut dyn FnMut(UpdateEvent)) -> bool {
        let gate = EngineGate::new();
        let mut steps = FakeSteps {
            fake: self,
            gate: &gate,
        };
        sp_update::flow::restart_installed(version, &mut steps, emit)
            == sp_update::flow::RestartOutcome::Restarting
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
            updater: Arc::new(FakeUpdater {
                ready: platform == Platform::Mac,
                ..FakeUpdater::default()
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
            "https://github.com/method-production-suite/method-reader/issues/new?body=x"
        ));
        assert!(allowed_url("https://calibre-ebook.com/download_osx"));
        assert!(allowed_url(
            "https://kdp.amazon.com/en_US/help/topic/G202131170"
        ));
        assert!(!allowed_url("https://github.com/someone-else/repo"));
        // The old repository redirects, but nothing the window opens is
        // there any more.
        assert!(!allowed_url(
            "https://github.com/ssandweiss/screepub/issues/new?body=x"
        ));
        assert!(!allowed_url("https://calibre-ebook.com/download_linux"));
        // The Help menu's pages.
        assert!(allowed_url("https://methodreader.com"));
        assert!(allowed_url("https://methodproduction.com"));
        assert!(allowed_url(crate::model::help::USER_GUIDE));
        assert!(allowed_url(crate::model::help::RELEASES));
        assert!(!allowed_url("https://methodreader.com.example.net"));
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
