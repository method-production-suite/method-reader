//! A pre-filled bug report (was `desktop/ui/feedback.js`).

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
            redact("in /data/sam/x.pdf", Some("/data/sam")),
            "in <path>.pdf"
        );
    }

    #[test]
    fn an_unknown_platform_says_so() {
        assert_eq!(os_label("  "), "unknown platform");
        assert_eq!(os_label("macOS"), "macOS");
    }
}
