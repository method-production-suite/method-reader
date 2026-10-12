//! Reading the engine's answers. They keep the CLI's `--json` shape (see
//! `crate::engine`), and the window trusts them for their SHAPE only, field
//! by field, the way `desktop/ui` did with `typeof` checks. These helpers are
//! those checks.

use serde_json::Value;

/// Something a person could read: a string with more than spaces in it.
/// `send-routes.js` `isText`.
pub fn is_text(value: Option<&Value>) -> bool {
    matches!(value, Some(Value::String(s)) if !s.trim().is_empty())
}

/// The string at `key`, if it is one.
pub fn str_at<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

/// The string at `key`, trimmed, if it has words in it.
pub fn text_at(value: &Value, key: &str) -> Option<String> {
    str_at(value, key)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// `answer.ok === true`, exactly: not truthy, true.
pub fn ok(value: &Value) -> bool {
    value.get("ok") == Some(&Value::Bool(true))
}

/// The engine's own sentence for a refusal (`answer.error.message`), trimmed,
/// or None when it sent none.
pub fn error_message(value: &Value) -> Option<String> {
    value.get("error").and_then(|e| text_at(e, "message"))
}

/// A finite number at `key` (JSON has no NaN, so any number is finite).
pub fn number_at(value: &Value, key: &str) -> Option<f64> {
    value.get(key).and_then(Value::as_f64)
}

/// A boolean at `key`, if it is exactly one.
pub fn bool_at(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn text_is_a_string_with_words_in_it() {
        assert!(is_text(Some(&json!("a"))));
        assert!(!is_text(Some(&json!("   "))));
        assert!(!is_text(Some(&json!(1))));
        assert!(!is_text(None));
    }

    #[test]
    fn ok_means_true_and_nothing_else() {
        assert!(ok(&json!({"ok": true})));
        assert!(!ok(&json!({"ok": "true"})));
        assert!(!ok(&json!({"ok": 1})));
        assert!(!ok(&json!({})));
    }

    #[test]
    fn the_refusal_sentence_is_trimmed_or_absent() {
        assert_eq!(
            error_message(&json!({"error": {"message": " no "}})),
            Some("no".into())
        );
        assert_eq!(error_message(&json!({"error": {"message": "  "}})), None);
        assert_eq!(error_message(&json!({"error": "x"})), None);
    }
}
