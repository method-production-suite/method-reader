//! The environment and the platform, as parameters.
//!
//! The TypeScript reads `process.env` and `process.platform` through
//! default arguments, so a test can hand any system's answer to a function
//! from any host. Rust tests run in threads of one process, where changing
//! the real environment is both unsafe and racy, so here the environment is
//! always a value: [`Env::process`] for a real run, a map for a test.

use std::collections::BTreeMap;

/// A snapshot of environment variables (`process.env`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Env(BTreeMap<String, String>);

impl Env {
    /// An empty environment.
    pub fn new() -> Self {
        Env(BTreeMap::new())
    }

    /// This process's environment, now. Variables whose names or values
    /// are not UTF-8 are left out, as nothing the engine reads can be one.
    pub fn process() -> Self {
        Env(std::env::vars_os()
            .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
            .collect())
    }

    /// `env.NAME`, `undefined` as `None`. On Windows, as Node's
    /// `process.env` there, the name's case does not matter: Windows calls
    /// the search path `Path`, and `env.PATH` still finds it.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.get_on(name, cfg!(windows))
    }

    /// [`Env::get`] with the platform's rule given: `windows` reads names
    /// without regard to ASCII case (an exact match first).
    pub fn get_on(&self, name: &str, windows: bool) -> Option<&str> {
        self.0
            .get(name)
            .or_else(|| {
                windows
                    .then(|| {
                        self.0
                            .iter()
                            .find(|(k, _)| k.eq_ignore_ascii_case(name))
                            .map(|(_, v)| v)
                    })
                    .flatten()
            })
            .map(String::as_str)
    }

    /// `env.NAME || ''`-style truthiness: set and not empty.
    pub fn non_empty(&self, name: &str) -> Option<&str> {
        self.get(name).filter(|v| !v.is_empty())
    }

    /// `env.NAME = value`. On Windows a variable of the same name in
    /// another case is replaced, not doubled (a child given both `Path` and
    /// `PATH` would see one of them, unpredictably).
    pub fn set(&mut self, name: impl Into<String>, value: impl Into<String>) -> &mut Self {
        self.set_on(name, value, cfg!(windows))
    }

    /// [`Env::set`] with the platform's rule given.
    pub fn set_on(
        &mut self,
        name: impl Into<String>,
        value: impl Into<String>,
        windows: bool,
    ) -> &mut Self {
        let name = name.into();
        if windows {
            self.0
                .retain(|k, _| k == &name || !k.eq_ignore_ascii_case(&name));
        }
        self.0.insert(name, value.into());
        self
    }

    pub fn remove(&mut self, name: &str) -> &mut Self {
        self.0.remove(name);
        self
    }

    /// `{ ...env, NAME: value }`.
    pub fn with(&self, name: &str, value: &str) -> Self {
        let mut copy = self.clone();
        copy.set(name, value);
        copy
    }

    /// Every variable, for handing to a child process.
    pub fn vars(&self) -> impl Iterator<Item = (&String, &String)> {
        self.0.iter()
    }
}

impl<K: Into<String>, V: Into<String>> FromIterator<(K, V)> for Env {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        Env(iter
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect())
    }
}

/// `process.platform` for this build: `darwin`, `win32`, `linux`, or the
/// OS's own name elsewhere (`freebsd`, `openbsd`, ...), as Node names them.
pub fn host_platform() -> &'static str {
    match std::env::consts::OS {
        "macos" => "darwin",
        "windows" => "win32",
        other => other,
    }
}

/// `os.homedir()`.
pub fn os_home() -> String {
    std::env::home_dir()
        .and_then(|p| p.to_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Whether `c` is whitespace as JavaScript's `String.prototype.trim` sees
/// it (WhiteSpace and LineTerminator: Unicode spaces, U+FEFF, but not
/// U+0085).
fn js_space(c: char) -> bool {
    c == '\u{feff}' || (c.is_whitespace() && c != '\u{85}')
}

/// `s.trim()`.
pub fn js_trim(s: &str) -> &str {
    s.trim_matches(js_space)
}

/// `s.trimStart()`.
pub fn js_trim_start(s: &str) -> &str {
    s.trim_start_matches(js_space)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_reads_and_replaces_a_name_in_any_case() {
        let env: Env = [("Path", r"C:\\Calibre2")].into_iter().collect();
        assert_eq!(env.get_on("PATH", true), Some(r"C:\\Calibre2"));
        assert_eq!(env.get_on("PATH", false), None, "elsewhere case matters");
        let mut env = env;
        env.set_on("PATH", "X", true);
        assert_eq!(env.vars().count(), 1);
        assert_eq!(env.get_on("path", true), Some("X"));
    }
}
