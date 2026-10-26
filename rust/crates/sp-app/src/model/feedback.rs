//! A pre-filled bug report (was `desktop/ui/feedback.js`), as a GitHub
//! issue or, for people without an account, an email ([`ProblemReport`]).

pub const NEW_ISSUE_BASE: &str =
    "https://github.com/method-production-suite/method-reader/issues/new";
/// About the longest body a report carries.
pub const MAX_BODY: usize = 2000;

/// `application/x-www-form-urlencoded`, as URLSearchParams writes it.
pub fn form_encode(value: &str) -> String {
    let mut out = String::new();
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// A pre-filled issue: an instruction, the context cut to fit, and the
/// versions, which are never cut.
pub fn new_issue_url(
    app_version: &str,
    os_version: &str,
    context: Option<&str>,
    home: Option<&str>,
) -> String {
    let head = "<!-- Describe the issue or suggestion. -->\n\n";
    let foot = format!(
        "\n---\n{} {app_version} · {os_version}\n",
        super::names::PRODUCT
    );
    let mut block = String::new();
    if let Some(context) = context.filter(|c| !c.trim().is_empty()) {
        let open = "\n**What happened:**\n";
        // Lengths in UTF-16 units, as JavaScript counts them.
        let units = |s: &str| s.encode_utf16().count();
        let room = MAX_BODY.saturating_sub(units(head) + units(&foot) + units(open) + 1);
        let mut said = redact(context, home);
        if units(&said) > room {
            let keep = room.saturating_sub(1);
            let utf16: Vec<u16> = said.encode_utf16().take(keep).collect();
            said = format!("{}…", String::from_utf16_lossy(&utf16));
        }
        block = format!("{open}{said}\n");
    }
    let body = format!("{head}{block}{foot}");
    format!("{NEW_ISSUE_BASE}?body={}", form_encode(&body))
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Replace every match of a home-folder pattern (`/Users/name`,
/// `/home/name`, `C:\Users\name`) not preceded by a word char or `~`.
fn fold_homes(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let free = i == 0 || !(is_word(chars[i - 1]) || chars[i - 1] == '~');
        if free && let Some(end) = home_at(&chars, i) {
            out.push('~');
            i = end;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Where a home-folder pattern starting at `i` ends, if one does.
fn home_at(chars: &[char], i: usize) -> Option<usize> {
    let rest: String = chars[i..].iter().take(12).collect();
    let (prefix_len, sep, stops): (usize, char, &[char]) = if rest.starts_with("/Users/") {
        (7, '/', &['/', '\'', '"', '`'])
    } else if rest.starts_with("/home/") {
        (6, '/', &['/', '\'', '"', '`'])
    } else if chars.len() > i + 9
        && chars[i].is_ascii_alphabetic()
        && chars[i + 1..i + 9].iter().collect::<String>() == ":\\Users\\"
    {
        (9, '\\', &['\\', '\'', '"', '`'])
    } else {
        return None;
    };
    let _ = sep;
    let start = i + prefix_len;
    let mut end = start;
    while end < chars.len() && !chars[end].is_whitespace() && !stops.contains(&chars[end]) {
        end += 1;
    }
    (end > start).then_some(end)
}

/// The length of the file name at the start of `rest`: up to its extension
/// when it has one (a name may hold spaces), else up to the first space.
fn name_length(rest: &[char]) -> usize {
    for (i, &c) in rest.iter().enumerate() {
        if matches!(c, '\\' | '/' | '\r' | '\n') {
            break;
        }
        if c != '.' {
            continue;
        }
        let k = rest[i + 1..]
            .iter()
            .take_while(|c| c.is_ascii_alphanumeric())
            .count();
        if !(1..=8).contains(&k) {
            continue;
        }
        let after = i + 1 + k;
        let ok = match rest.get(after) {
            None => true,
            Some(c) if c.is_whitespace() || ",;:)]".contains(*c) => true,
            Some('.') => rest.get(after + 1).is_none_or(|c| c.is_whitespace()),
            _ => false,
        };
        if ok {
            return after;
        }
    }
    rest.iter().take_while(|c| !c.is_whitespace()).count()
}

/// The engine's sentence, safe to put in a public URL.
pub fn redact(message: &str, home: Option<&str>) -> String {
    let mut out = message.to_string();
    if let Some(home) = home.filter(|h| h.trim().chars().count() > 1) {
        let h = home.trim_end_matches(['/', '\\']);
        if !h.is_empty() {
            out = out.split(h).collect::<Vec<_>>().join("~");
        }
    }
    let out = fold_homes(&out);
    let chars: Vec<char> = out.chars().collect();
    let mut result = String::new();
    let mut i = 0;
    while i < chars.len() {
        let free = i == 0 || chars[i - 1].is_whitespace() || "'\"`([".contains(chars[i - 1]);
        let mut j = i;
        if chars[j] == '~' {
            j += 1;
        } else if chars.len() > j + 1 && chars[j].is_ascii_alphabetic() && chars[j + 1] == ':' {
            j += 2;
        }
        if free && j < chars.len() && matches!(chars[j], '/' | '\\') {
            let mut end = j;
            while end < chars.len() && !"\r\n'\"`<>|".contains(chars[end]) {
                end += 1;
            }
            let path = &chars[i..end];
            let at = path
                .iter()
                .rposition(|c| matches!(c, '/' | '\\'))
                .unwrap_or(0);
            let rest = &path[at + 1..];
            let n = name_length(rest);
            let name: String = rest[..n].iter().collect();
            let ext = name
                .rfind('.')
                .map(|d| &name[d..])
                .filter(|e| {
                    (2..=9).contains(&e.len()) && e[1..].chars().all(|c| c.is_ascii_alphanumeric())
                })
                .unwrap_or("");
            result.push_str("<path>");
            result.push_str(ext);
            result.extend(&rest[n..]);
            i = end;
            continue;
        }
        result.push(chars[i]);
        i += 1;
    }
    result
}

/// What the window knows about the machine, for the footer.
pub fn os_label(platform: &str) -> String {
    let said = platform.trim();
    if said.is_empty() {
        "unknown platform".into()
    } else {
        said.into()
    }
}

// ---------------------------------------------------------- by email

/// Where a report by email goes.
pub const REPORT_ADDRESS: &str = "hello@methodproduction.com";

/// Every report's subject starts with this, the product's name and a
/// space. A FILTER DEPENDS ON IT: the suite's inbox sorts Method Reader's
/// reports by this exact prefix (trailing space included), so changing it
/// sends every report somewhere nobody reads. The whole subject is
/// "Method Reader <version>: <code>", or "...: a problem" with no code.
pub const SUBJECT_PREFIX: &str = "Method Reader ";

/// What a subject says when there is no refusal code.
pub const NO_CODE: &str = "a problem";

/// The most a person may type: as much as the issue body holds.
pub const MAX_TYPED: usize = MAX_BODY;

/// The longest `mailto:` URL the window hands the mail app, in bytes.
/// Mail apps differ on how much they take; this much they all do.
pub const MAX_MAILTO: usize = 2000;

/// A script's shape as counts, from the engine's `shape` verb: integers
/// only, so no word of the script can ride in it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shape {
    /// Absent when the script carries no page markers.
    pub pages: Option<u64>,
    pub scene_headings: u64,
    pub cues: u64,
    pub dialogue_blocks: u64,
    pub dual_exchanges: u64,
    pub pictures: u64,
    pub transitions: u64,
}

impl Shape {
    /// The `shape --json` answer, if it is one. Anything but integers
    /// where the counts belong is no shape at all.
    pub fn from_answer(answer: &serde_json::Value) -> Option<Shape> {
        if answer.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            return None;
        }
        let n = |key: &str| answer.get(key).and_then(|v| v.as_u64());
        let pages = match answer.get("pages") {
            None => None,
            Some(v) => Some(v.as_u64()?),
        };
        Some(Shape {
            pages,
            scene_headings: n("sceneHeadings")?,
            cues: n("cues")?,
            dialogue_blocks: n("dialogueBlocks")?,
            dual_exchanges: n("dualExchanges")?,
            pictures: n("pictures")?,
            transitions: n("transitions")?,
        })
    }

    /// One line a count, as the report carries them.
    pub fn lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(pages) = self.pages {
            out.push(format!("pages: {pages}"));
        }
        out.push(format!("scene headings: {}", self.scene_headings));
        out.push(format!("cues: {}", self.cues));
        out.push(format!("dialogue blocks: {}", self.dialogue_blocks));
        out.push(format!("dual-dialogue exchanges: {}", self.dual_exchanges));
        out.push(format!("pictures: {}", self.pictures));
        out.push(format!("transitions: {}", self.transitions));
        out
    }
}

/// A problem report: what the person wrote, and only what the window
/// already knows (docs/superpowers/specs/2026-10-21-report-without-github-design.md).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProblemReport {
    /// What the person wrote. Required; at most [`MAX_TYPED`] characters.
    pub typed: String,
    pub app_version: String,
    /// [`os_label`]'s answer.
    pub os: String,
    /// A failed conversion's code and message: the app's own words, with
    /// paths already redacted.
    pub refusal: Option<(String, String)>,
    /// The script's shape, when the person ticked the box.
    pub shape: Option<Shape>,
}

/// RFC 6068: UTF-8, every byte but the unreserved ones percent-encoded,
/// and every line break as CRLF (`%0D%0A`), whatever the text used.
pub fn mailto_encode(value: &str) -> String {
    let text = value.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::new();
    for b in text.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            b'\n' => out.push_str("%0D%0A"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

impl ProblemReport {
    pub fn new(typed: &str, app_version: &str, os: &str) -> Self {
        ProblemReport {
            typed: typed.chars().take(MAX_TYPED).collect(),
            app_version: app_version.into(),
            os: os.into(),
            refusal: None,
            shape: None,
        }
    }

    /// Whether there is anything to send: the person has to say something.
    pub fn can_send(&self) -> bool {
        !self.typed.trim().is_empty()
    }

    /// "Method Reader <version>: <code>", or "...: a problem".
    pub fn subject(&self) -> String {
        let code = self
            .refusal
            .as_ref()
            .map(|(code, _)| code.as_str())
            .filter(|c| !c.trim().is_empty())
            .unwrap_or(NO_CODE);
        format!("{SUBJECT_PREFIX}{}: {code}", self.app_version)
    }

    /// Everything after what the person wrote: never cut.
    fn footer(&self) -> String {
        let mut lines = vec![
            "---".to_string(),
            format!(
                "{} {} · {}",
                super::names::PRODUCT,
                self.app_version,
                self.os
            ),
        ];
        if let Some((code, message)) = &self.refusal {
            lines.push(format!("What the app said: {code}: {message}"));
        }
        if let Some(shape) = &self.shape {
            lines.push("Counts of the script's shape (no words from it):".into());
            lines.extend(shape.lines());
        }
        lines.join("\n")
    }

    fn body_with(&self, typed: &str) -> String {
        format!("{}\n\n{}\n", typed.trim_end(), self.footer())
    }

    fn url_with(&self, typed: &str) -> String {
        format!(
            "mailto:{REPORT_ADDRESS}?subject={}&body={}",
            mailto_encode(&self.subject()),
            mailto_encode(&self.body_with(typed))
        )
    }

    /// What the person wrote, cut (with an ellipsis) until the whole URL
    /// fits in [`MAX_MAILTO`] bytes. The versions and counts never are.
    fn fitted_typed(&self) -> String {
        let typed = self.typed.trim_end();
        if self.url_with(typed).len() <= MAX_MAILTO {
            return typed.to_string();
        }
        let chars: Vec<char> = typed.chars().collect();
        let cut = |n: usize| format!("{}…", chars[..n].iter().collect::<String>());
        // The longest prefix that fits: the URL grows with every character.
        let (mut lo, mut hi) = (0usize, chars.len());
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            if self.url_with(&cut(mid)).len() <= MAX_MAILTO {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        cut(lo)
    }

    /// The body exactly as the mail will carry it, cut and all: what
    /// "Show what's sent" shows.
    pub fn body(&self) -> String {
        self.body_with(&self.fitted_typed())
    }

    /// The pre-filled message to [`REPORT_ADDRESS`].
    pub fn mailto_url(&self) -> String {
        self.url_with(&self.fitted_typed())
    }

    /// The same report as a pre-filled GitHub issue, for people with an
    /// account: what the person wrote, then the app's words and counts.
    pub fn issue_url(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if !self.typed.trim().is_empty() {
            parts.push(self.typed.trim_end().to_string());
        }
        if let Some((code, message)) = &self.refusal {
            parts.push(format!("{code}: {message}"));
        }
        if let Some(shape) = &self.shape {
            parts.push(shape.lines().join("\n"));
        }
        let context = parts.join("\n\n");
        new_issue_url(&self.app_version, &self.os, Some(&context), None)
    }
}

/// The report sheet as the person has it so far.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReportDraft {
    pub typed: String,
    /// A failed conversion's code and redacted message.
    pub refusal: Option<(String, String)>,
    /// The `.fountain` whose shape was asked for, if any.
    pub asked: Option<String>,
    /// Its counts, once the engine has answered.
    pub shape: Option<Shape>,
    /// The box: off until the person ticks it.
    pub add_shape: bool,
    pub show_sent: bool,
    /// The mail app did not open.
    pub mail_failed: bool,
    pub copied: bool,
}

impl ReportDraft {
    /// A draft for a failed conversion (its code and message, redacted
    /// with `home`), or for Help's menu (`None`).
    pub fn new(refusal: Option<(&str, &str)>, home: Option<&str>) -> Self {
        ReportDraft {
            refusal: refusal.map(|(code, message)| (code.to_string(), redact(message, home))),
            ..Default::default()
        }
    }

    /// The report this draft makes: the counts only when the box is ticked.
    pub fn report(&self, app_version: &str, os: &str) -> ProblemReport {
        ProblemReport {
            refusal: self.refusal.clone(),
            shape: self.shape.filter(|_| self.add_shape),
            ..ProblemReport::new(&self.typed, app_version, os)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_encoding_matches_url_search_params() {
        assert_eq!(form_encode("C++ a/b"), "C%2B%2B+a%2Fb");
        assert_eq!(form_encode("·"), "%C2%B7");
        assert_eq!(form_encode("a-b_c.d*"), "a-b_c.d*");
    }

    #[test]
    fn the_report_carries_both_versions_and_the_context() {
        let url = new_issue_url("0.11.1", "MacIntel", Some("scanned: no text"), None);
        assert!(url.starts_with(
            "https://github.com/method-production-suite/method-reader/issues/new?body="
        ));
        assert!(url.contains("Method+Reader+0.11.1+%C2%B7+MacIntel"));
        assert!(url.contains("scanned%3A+no+text"));
        let none = new_issue_url("0.11.1", "x", Some("  "), None);
        assert!(!none.contains("What+happened"));
    }

    #[test]
    fn a_long_context_is_cut_and_the_footer_never_is() {
        let long = "x".repeat(5000);
        let url = new_issue_url("0.11.1", "Linux", Some(&long), None);
        assert!(url.contains("%E2%80%A6"), "an ellipsis where it was cut");
        assert!(url.ends_with("Method+Reader+0.11.1+%C2%B7+Linux%0A"));
    }

    #[test]
    fn every_path_becomes_path_and_its_extension() {
        assert_eq!(
            redact("cannot read /Users/ann/Scripts/My Film.pdf (EACCES)", None),
            "cannot read <path>.pdf (EACCES)"
        );
        assert_eq!(
            redact("wrote C:\\Users\\ann\\Books\\Big Title.epub, done", None),
            "wrote <path>.epub, done"
        );
        assert_eq!(redact("in /srv/library", None), "in <path>");
        assert_eq!(redact("and/or https://x.y/z", None), "and/or https://x.y/z");
        assert_eq!(redact("'/a/b.kfx'", None), "'<path>.kfx'");
    }

    #[test]
    fn the_home_folder_becomes_a_tilde_first() {
        assert_eq!(redact("home is /Users/ann", None), "home is ~");
        assert_eq!(
            redact("home is /Users/ann", Some("/Users/ann/")),
            "home is ~"
        );
        assert_eq!(
            redact("in /data/ann/x.pdf", Some("/data/ann")),
            "in <path>.pdf"
        );
    }

    #[test]
    fn an_unknown_platform_says_so() {
        assert_eq!(os_label("  "), "unknown platform");
        assert_eq!(os_label("macOS"), "macOS");
    }
    // ------------------------------------------------------ by email

    fn report(typed: &str) -> ProblemReport {
        ProblemReport::new(typed, "0.14.0", "macOS")
    }

    fn field_station() -> Shape {
        Shape {
            pages: Some(18),
            scene_headings: 11,
            cues: 103,
            dialogue_blocks: 103,
            dual_exchanges: 0,
            pictures: 0,
            transitions: 3,
        }
    }

    #[test]
    fn the_subject_is_the_contract_the_inbox_filters_on() {
        assert_eq!(SUBJECT_PREFIX, format!("{} ", crate::model::names::PRODUCT));
        let plain = report("x");
        assert_eq!(plain.subject(), "Method Reader 0.14.0: a problem");
        let refused = ProblemReport {
            refusal: Some(("scanned".into(), "This PDF is a scan.".into())),
            ..report("x")
        };
        assert_eq!(refused.subject(), "Method Reader 0.14.0: scanned");
        for r in [plain, refused] {
            assert!(r.subject().starts_with("Method Reader "), "{}", r.subject());
            assert!(r.subject().starts_with(SUBJECT_PREFIX));
            let url = r.mailto_url();
            assert!(
                url.contains("?subject=Method%20Reader%200.14.0%3A%20"),
                "{url}"
            );
        }
    }

    #[test]
    fn the_mailto_encoding_is_rfc_6068() {
        assert_eq!(mailto_encode("a b+c&d=e?"), "a%20b%2Bc%26d%3De%3F");
        assert_eq!(mailto_encode("a-b_c.d~"), "a-b_c.d~");
        assert_eq!(
            mailto_encode("one\ntwo\r\nthree\rfour"),
            "one%0D%0Atwo%0D%0Athree%0D%0Afour"
        );
        assert_eq!(mailto_encode("é·"), "%C3%A9%C2%B7");
        let url = report("It stopped.").mailto_url();
        assert!(url.starts_with("mailto:hello@methodproduction.com?subject="));
        assert!(url.contains("&body=It%20stopped.%0D%0A%0D%0A---%0D%0AMethod%20Reader%200.14.0%20%C2%B7%20macOS%0D%0A"), "{url}");
    }

    #[test]
    fn non_ascii_and_line_breaks_survive_as_utf_8() {
        let r = report("Página dos\r\nline two ñ 字");
        let url = r.mailto_url();
        assert!(
            url.contains("P%C3%A1gina%20dos%0D%0Aline%20two%20%C3%B1%20%E5%AD%97"),
            "{url}"
        );
        assert!(!url.contains('\n') && !url.contains('\r') && !url.contains(' '));
        assert!(url.is_ascii());
    }

    #[test]
    fn a_long_report_is_cut_in_the_typed_text_never_the_versions() {
        let r = ProblemReport {
            shape: Some(field_station()),
            refusal: Some(("unreadable".into(), "cannot read <path>.pdf".into())),
            ..report(&"é".repeat(1500))
        };
        let url = r.mailto_url();
        assert!(url.len() <= MAX_MAILTO, "{}", url.len());
        assert!(url.len() > MAX_MAILTO - 12, "cut no more than it has to");
        assert!(
            url.contains("%E2%80%A6%0D%0A%0D%0A---"),
            "an ellipsis where it was cut"
        );
        assert!(url.contains("Method%20Reader%200.14.0%20%C2%B7%20macOS"));
        assert!(url.contains("unreadable%3A%20cannot%20read%20%3Cpath%3E.pdf"));
        assert!(url.ends_with("transitions%3A%203%0D%0A"));
        assert!(r.body().contains("…\n\n---\n"));
        // A short report is not cut at all.
        assert!(!report("short").mailto_url().contains("%E2%80%A6"));
    }

    #[test]
    fn the_typed_text_is_held_to_two_thousand_characters() {
        let r = report(&"x".repeat(5000));
        assert_eq!(r.typed.chars().count(), MAX_TYPED);
        assert!(!report("   \n ").can_send());
        assert!(report("it broke").can_send());
    }

    #[test]
    fn the_counts_ride_only_when_the_box_is_ticked() {
        let mut draft = ReportDraft {
            typed: "Cues went missing.".into(),
            shape: Some(field_station()),
            ..ReportDraft::default()
        };
        let off = draft.report("0.14.0", "macOS");
        assert_eq!(off.shape, None);
        assert!(!off.body().contains("cues:"));
        draft.add_shape = true;
        let on = draft.report("0.14.0", "macOS");
        assert!(on.body().contains("Counts of the script's shape (no words from it):\npages: 18\nscene headings: 11\ncues: 103\n"));
        assert!(on.issue_url().contains("cues%3A+103"));
    }

    #[test]
    fn a_refusal_s_paths_are_gone_before_the_report_is_built() {
        let draft = ReportDraft::new(
            Some(("unreadable", "cannot read /Users/ann/Secret Title.pdf")),
            Some("/Users/ann"),
        );
        let r = draft.report("0.14.0", "macOS");
        assert!(!r.body().contains("Secret"));
        assert!(
            r.body()
                .contains("What the app said: unreadable: cannot read <path>.pdf")
        );
        assert!(!r.issue_url().contains("Secret"));
    }

    #[test]
    fn the_issue_link_is_today_s_when_nothing_was_typed() {
        let r = ProblemReport {
            refusal: Some(("scanned".into(), "no text".into())),
            ..report("")
        };
        assert_eq!(
            r.issue_url(),
            new_issue_url("0.14.0", "macOS", Some("scanned: no text"), None)
        );
        assert_eq!(
            report("").issue_url(),
            new_issue_url("0.14.0", "macOS", None, None)
        );
        assert!(report("It froze.").issue_url().contains("It+froze."));
    }

    #[test]
    fn a_shape_is_integers_or_nothing() {
        let v = serde_json::json!({"ok": true, "pages": 18, "sceneHeadings": 11, "cues": 103,
            "dialogueBlocks": 103, "dualExchanges": 0, "pictures": 0, "transitions": 3});
        assert_eq!(Shape::from_answer(&v), Some(field_station()));
        let mut no_pages = v.clone();
        no_pages.as_object_mut().unwrap().remove("pages");
        assert_eq!(Shape::from_answer(&no_pages).unwrap().pages, None);
        let mut words = v.clone();
        words["cues"] = serde_json::json!("DELACROIX");
        assert_eq!(Shape::from_answer(&words), None);
        assert_eq!(Shape::from_answer(&serde_json::json!({"ok": false})), None);
    }
}
