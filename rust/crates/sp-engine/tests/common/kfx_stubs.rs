//! Port of tests/kfx-snippet-stubs.ts: runs the KFX plugin install snippet
//! for real, under the system python3, against stand-ins for the few
//! `calibre.*` names it imports. The real Calibre is never involved: the
//! stand-in package is first on PYTHONPATH, user site-packages are off, and
//! the two network fetches are answered from a scenario file.

use std::cell::RefCell;
use std::process::Command;

use serde_json::{Value, json};
use sp_engine::errors::EngineError;
use sp_engine::export::calibre::TimedRun;

/// python3, when the machine has one that runs.
pub fn system_python() -> Option<&'static str> {
    let out = Command::new("python3")
        .args(["-c", "import bz2, json, zipfile; print(\"ok\")"])
        .output()
        .ok()?;
    (out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "ok")
        .then_some("python3")
}

const CALIBRE_INIT: &str = r#"
import json, os, urllib.request

LOG = os.environ['METHOD_READER_STUB_LOG']
with open(os.environ['METHOD_READER_STUB_SCENARIO']) as f:
    SCENARIO = json.load(f)

def log(event):
    with open(LOG, 'a') as f:
        f.write(event + '\n')

def zip_bytes():
    import io, zipfile
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, 'w') as z:
        if SCENARIO['zip'] != 'not-plugin':
            z.writestr('__init__.py', 'x = 1\n')
        z.writestr('kfxlib/stub.py', 'y = 2\n')
    return buf.getvalue()

class _Response:
    def __init__(self, data):
        self._data = data
    def read(self):
        return self._data

def _urlopen(url, timeout=None):
    log('zip ' + url)
    if SCENARIO['zip'] == 'offline':
        raise OSError('<urlopen error [Errno 8] nodename nor servname provided>')
    return _Response(zip_bytes())

urllib.request.urlopen = _urlopen
"#;

const HTTPS: &str = r#"
import bz2, json
from calibre import SCENARIO, log, zip_bytes

def get_https_resource_securely(url):
    log('index ' + url)
    if SCENARIO['index'] == 'offline':
        raise OSError('<urlopen error [Errno 8] nodename nor servname provided>')
    entry = {'version': [2, 20, 1], 'size': len(zip_bytes()) + SCENARIO.get('sizeDelta', 0)}
    if SCENARIO['index'] != 'no-file':
        entry['file'] = 'kfx_output.zip'
    return bz2.compress(json.dumps({'KFX Output': entry}).encode('utf-8'))
"#;

const PLUGIN_UPDATER: &str = r#"
from calibre import SCENARIO
if not SCENARIO.get('noIndexUrl'):
    INDEX_URL = 'https://stub.invalid/plugins/index.json.bz2'
"#;

const CUSTOMIZE_UI: &str = r#"
import os
from calibre import SCENARIO, log

class _Plugin:
    def __init__(self, name):
        self.name = name

_installed = [_Plugin(p['name']) for p in SCENARIO['plugins']]

def initialized_plugins():
    return iter(_installed)

def output_format_plugins():
    return iter([p for p, s in zip(_installed, SCENARIO['plugins']) if s['output']])

def remove_plugin(p):
    log('remove ' + p.name)

def add_plugin(path):
    log('add ' + ('zip-present' if os.path.exists(path) else 'zip-missing'))
    if SCENARIO.get('addFails'):
        raise RuntimeError(SCENARIO['addFails'])
"#;

/// A scenario (see the TypeScript's SnippetScenario), as the JSON the stubs
/// read.
pub fn scenario(index: &str, zip: &str, plugins: &[(&str, bool)]) -> Value {
    json!({
        "index": index,
        "zip": zip,
        "plugins": plugins.iter().map(|(n, o)| json!({ "name": n, "output": o })).collect::<Vec<_>>(),
    })
}

/// The stub package for one scenario, in `dir`.
pub struct SnippetRun {
    dir: String,
    python: &'static str,
    last: RefCell<(String, String)>,
}

impl SnippetRun {
    pub fn new(dir: &str, python: &'static str, scenario: &Value) -> Self {
        let pkg = format!("{dir}/calibre");
        for sub in ["", "utils", "gui2", "gui2/dialogs", "customize"] {
            std::fs::create_dir_all(format!("{pkg}/{sub}")).unwrap();
            if !sub.is_empty() {
                std::fs::write(format!("{pkg}/{sub}/__init__.py"), "").unwrap();
            }
        }
        std::fs::write(format!("{pkg}/__init__.py"), CALIBRE_INIT).unwrap();
        std::fs::write(format!("{pkg}/utils/https.py"), HTTPS).unwrap();
        std::fs::write(
            format!("{pkg}/gui2/dialogs/plugin_updater.py"),
            PLUGIN_UPDATER,
        )
        .unwrap();
        std::fs::write(format!("{pkg}/customize/ui.py"), CUSTOMIZE_UI).unwrap();
        std::fs::write(format!("{dir}/scenario.json"), scenario.to_string()).unwrap();
        SnippetRun {
            dir: dir.to_owned(),
            python,
            last: RefCell::new((String::new(), String::new())),
        }
    }

    /// The runner to hand installKfxPlugin: runs argv[2] (the snippet)
    /// under python3 with the stubs, ignoring the calibre-debug path.
    pub fn run(&self, argv: &[String]) -> Result<TimedRun, EngineError> {
        let log = format!("{}/events.log", self.dir);
        let _ = std::fs::remove_file(&log);
        let out = Command::new(self.python)
            .args(["-c", &argv[2]])
            .env("PYTHONPATH", &self.dir)
            .env("PYTHONNOUSERSITE", "1")
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .env("METHOD_READER_STUB_LOG", &log)
            .env(
                "METHOD_READER_STUB_SCENARIO",
                format!("{}/scenario.json", self.dir),
            )
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        *self.last.borrow_mut() = (stdout.clone(), stderr.clone());
        Ok(TimedRun {
            code: out.status.code().unwrap_or(1),
            stdout,
            stderr,
            timed_out: false,
        })
    }

    /// Every stub call, in order.
    pub fn events(&self) -> Vec<String> {
        std::fs::read_to_string(format!("{}/events.log", self.dir))
            .map(|t| {
                t.lines()
                    .filter(|l| !l.is_empty())
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The raw stdout and stderr of the last run.
    pub fn last(&self) -> (String, String) {
        self.last.borrow().clone()
    }
}
