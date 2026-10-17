//! How the CLI speaks: stdout for the answer, stderr for everything a
//! person reads beside it, and the one place an early exit is printed.

use std::io::Write;
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value};
use sp_engine::jsjson;

/// Why a run stopped before its natural end.
#[derive(Debug)]
pub enum Stop {
    /// `fail(error)`: a contract error, exit 1.
    Fail { code: String, message: String },
    /// Already printed; exit with this code.
    Exit(u8),
    /// An error nobody anticipated: the catch-all, exit 1. The message is
    /// `String(err)`.
    Internal(String),
}

impl Stop {
    pub fn fail(code: &str, message: impl Into<String>) -> Stop {
        Stop::Fail {
            code: code.to_owned(),
            message: message.into(),
        }
    }

    pub fn usage(message: impl Into<String>) -> Stop {
        Stop::fail("usage", message)
    }

    /// A throw that is not a contract error, as `String(err)` writes a
    /// plain `Error`.
    pub fn internal(message: impl std::fmt::Display) -> Stop {
        Stop::Internal(format!("Error: {message}"))
    }
}

pub type Run = Result<(), Stop>;

/// Where the two streams go: the process's own stdout and stderr for the
/// binary, anything else for a caller in the same process (the window),
/// which hears each line as it is written, exactly as a parent reading the
/// pipes would.
pub trait Sink: Send {
    /// One whole line for stdout.
    fn out(&mut self, line: &str);
    /// One whole line for stderr.
    fn err(&mut self, line: &str);
}

/// The process's own streams, each line flushed before anything else
/// happens (`sayLine`: a piped answer must never be cut short).
pub struct Std;

impl Sink for Std {
    fn out(&mut self, line: &str) {
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(line.as_bytes());
        let _ = out.write_all(b"\n");
        let _ = out.flush();
    }

    fn err(&mut self, line: &str) {
        let mut e = std::io::stderr().lock();
        let _ = e.write_all(line.as_bytes());
        let _ = e.write_all(b"\n");
        let _ = e.flush();
    }
}

type Shared = Arc<Mutex<Box<dyn Sink>>>;

/// A handle that writes stderr lines, for the conversion's callbacks.
#[derive(Clone)]
pub struct ErrLine(Shared);

impl ErrLine {
    pub fn line(&self, text: &str) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).err(text);
    }
}

pub struct Io {
    pub json: bool,
    sink: Shared,
}

impl Io {
    pub fn with_sink(json: bool, sink: Box<dyn Sink>) -> Io {
        Io {
            json,
            sink: Arc::new(Mutex::new(sink)),
        }
    }

    /// One line on stdout.
    pub fn say(&mut self, text: &str) {
        self.sink
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .out(text);
    }

    /// One line on stderr.
    pub fn err(&mut self, text: &str) {
        self.err_line().line(text);
    }

    pub fn err_line(&self) -> ErrLine {
        ErrLine(self.sink.clone())
    }

    /// `JSON.stringify({ ok: true, ...answer })` on stdout.
    pub fn ok(&mut self, answer: Value) {
        let mut m = Map::new();
        m.insert("ok".into(), Value::Bool(true));
        if let Value::Object(fields) = answer {
            for (k, v) in fields {
                m.insert(k, v);
            }
        }
        self.say(&jsjson::stringify(&Value::Object(m)));
    }

    /// `showHelp(usage)`: the text, or `{ ok, usage }` in --json; exit 0.
    pub fn help(&mut self, usage: &str) -> Stop {
        if self.json {
            self.ok(serde_json::json!({ "usage": usage }));
        } else {
            self.say(usage);
        }
        Stop::Exit(0)
    }

    /// Print a stop and give the exit code.
    pub fn finish(&mut self, stop: Stop) -> u8 {
        match stop {
            Stop::Exit(code) => code,
            Stop::Fail { code, message } => {
                if self.json {
                    let error = serde_json::json!({ "ok": false, "error": { "code": code, "message": message } });
                    self.say(&jsjson::stringify(&error));
                } else {
                    self.err(&format!("method-reader: {message}"));
                }
                1
            }
            Stop::Internal(message) => {
                if self.json {
                    let error = serde_json::json!({ "ok": false, "error": { "code": "internal", "message": message } });
                    self.say(&jsjson::stringify(&error));
                } else {
                    self.err(&message);
                }
                1
            }
        }
    }
}
