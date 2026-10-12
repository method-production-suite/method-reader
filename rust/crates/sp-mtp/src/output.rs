//! The one JSON object every run prints, in the engine's contract: success is
//! `{"ok":true,...}` and exit 0; failure is
//! `{"ok":false,"error":{"code":"...","message":"..."}}` and exit 1. The
//! message is a plain sentence a person can act on; the code is what the
//! engine switches on, never the message (src/cli-errors.ts's rule).

use serde_json::{Map, Value, json};

/// Every way a run can fail, named the way the engine's codes are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    /// The command line was not one this helper takes.
    Usage,
    /// No device with that serial is plugged in.
    NoDevice,
    /// The device is not a Kindle, so nothing is written to it.
    NotAKindle,
    /// Another program holds the device (macOS's camera import, Calibre...).
    Busy,
    /// The system would not let this program open the device (Linux udev).
    Permission,
    /// The local file to copy cannot be read.
    Unreadable,
    /// The device offers no storage that can be written to.
    NoStorage,
    /// Not enough room on the device.
    StorageFull,
    /// The path on the device is not one this helper will write.
    BadPath,
    /// The copy started and did not finish.
    CopyFailed,
    /// A bug of ours.
    Internal,
}

impl Code {
    pub fn as_str(self) -> &'static str {
        match self {
            Code::Usage => "usage",
            Code::NoDevice => "no-device",
            Code::NotAKindle => "not-a-kindle",
            Code::Busy => "busy",
            Code::Permission => "permission",
            Code::Unreadable => "unreadable",
            Code::NoStorage => "no-storage",
            Code::StorageFull => "storage-full",
            Code::BadPath => "bad-path",
            Code::CopyFailed => "copy-failed",
            Code::Internal => "internal",
        }
    }
}

/// A failure that already knows its code and its sentence, plus any extra
/// fields the caller needs (`holder` on `busy`).
#[derive(Debug, Clone, PartialEq)]
pub struct Failure {
    pub code: Code,
    pub message: String,
    pub extra: Map<String, Value>,
}

impl Failure {
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Failure {
            code,
            message: message.into(),
            extra: Map::new(),
        }
    }

    pub fn with(mut self, key: &str, value: Value) -> Self {
        self.extra.insert(key.to_string(), value);
        self
    }

    /// The error object, as printed.
    pub fn to_json(&self) -> Value {
        let mut error = Map::new();
        error.insert("code".into(), json!(self.code.as_str()));
        error.insert("message".into(), json!(self.message));
        for (k, v) in &self.extra {
            error.insert(k.clone(), v.clone());
        }
        json!({ "ok": false, "error": Value::Object(error) })
    }
}

/// A success: `ok: true` first, then the answer's own fields.
pub fn success(fields: Map<String, Value>) -> Value {
    let mut out = Map::new();
    out.insert("ok".into(), json!(true));
    for (k, v) in fields {
        out.insert(k, v);
    }
    Value::Object(out)
}

/// What to print and the exit code to leave with.
pub fn render(result: Result<Value, Failure>) -> (String, i32) {
    match result {
        Ok(value) => (value.to_string(), 0),
        Err(failure) => (failure.to_json().to_string(), 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_success_starts_with_ok_true_and_exits_zero() {
        let mut fields = Map::new();
        fields.insert("devices".into(), json!([]));
        let (text, code) = render(Ok(success(fields)));
        assert_eq!(code, 0);
        assert_eq!(text, r#"{"ok":true,"devices":[]}"#);
    }

    #[test]
    fn a_failure_carries_its_code_and_sentence_and_exits_one() {
        let (text, code) = render(Err(Failure::new(
            Code::NoDevice,
            "no Kindle with serial X is plugged in",
        )));
        assert_eq!(code, 1);
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["ok"], json!(false));
        assert_eq!(v["error"]["code"], json!("no-device"));
        assert_eq!(
            v["error"]["message"],
            json!("no Kindle with serial X is plugged in")
        );
    }

    #[test]
    fn extra_fields_ride_inside_the_error_object() {
        let f = Failure::new(Code::Busy, "Calibre is using the Kindle")
            .with("holder", json!("calibre"));
        let v = f.to_json();
        assert_eq!(v["error"]["holder"], json!("calibre"));
        assert_eq!(v["error"]["code"], json!("busy"));
    }

    #[test]
    fn every_code_has_a_distinct_kebab_case_name() {
        let all = [
            Code::Usage,
            Code::NoDevice,
            Code::NotAKindle,
            Code::Busy,
            Code::Permission,
            Code::Unreadable,
            Code::NoStorage,
            Code::StorageFull,
            Code::BadPath,
            Code::CopyFailed,
            Code::Internal,
        ];
        let names: std::collections::HashSet<_> = all.iter().map(|c| c.as_str()).collect();
        assert_eq!(names.len(), all.len());
        for name in names {
            assert!(
                name.chars().all(|c| c.is_ascii_lowercase() || c == '-'),
                "{name}"
            );
        }
    }
}
