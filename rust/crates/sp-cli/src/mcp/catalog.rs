//! What the server offers besides tools: one resource that describes the
//! formats Method Reader reads and writes, and two prompts that walk a
//! client through the usual jobs. Both are fixed text, the same for every
//! caller, so their lists and reads carry the protocol's caching hints.
//!
//! Method Budget's server has neither yet; these follow the specification's
//! shapes, so the shared crate to come can take them as they are.

use serde_json::{Map, Value, json};

/// The formats resource's address.
pub const FORMATS_URI: &str = "method-reader://formats";

const FORMATS_TEXT: &str = "# What Method Reader reads and writes

Method Reader works on files on this computer and sends nothing anywhere.

## Reads

- **PDF**: a screenplay saved or printed as a PDF with a text layer. A
  scanned PDF (pictures of pages, no text) is refused: run text recognition
  on it first. A document that doesn't read as a screenplay is refused too,
  unless the call passes force.
- **Fountain** (`.fountain`, or `.txt`): a plain-text screenplay. Only
  make_ebook takes one. It is partly supported: the settings that act while a
  PDF is read do nothing here.

## Writes

- **Fountain** (`.fountain`): the script as plain text, to keep editing in
  any Fountain editor. Pictures carried out of the PDF (beta) go in a
  `<name>.images` folder beside it, and the `.fountain` names each one in an
  `[[img: ...]]` note.
- **.fdx** (beta): the script as a file for screenwriting apps that open
  .fdx. A reconstruction: revision marks and exact page breaks do not carry
  over.
- **EPUB**: a reflowable e-book for most readers and reading apps.
- **AZW3**: for Kindles over USB, made by Method Reader from the
  EPUB, with nothing else installed.
- **MOBI**: for the oldest Kindles over USB. Text only for now: no pictures
  and no cover.

Every file is named after the script (`<name>.fountain`, `<name>.epub`...)
and written into the script's own folder unless the call names another.
Nothing already there is replaced unless the call passes overwrite.

## Settings

A conversion uses the script's saved settings when a
`<name>.method-reader.json` sits beside it (the app and
`method-reader settings` write one), and otherwise the format defaults
chosen in the app, or Method Reader's own.
";

/// `resources/list`'s entries.
pub fn resources() -> Vec<Value> {
    vec![json!({
        "uri": FORMATS_URI,
        "name": "formats",
        "title": "Formats",
        "description": "What Method Reader reads and writes, and how it names what it writes.",
        "mimeType": "text/markdown",
    })]
}

/// The contents of the resource at `uri`, if there is one.
pub fn read(uri: &str) -> Option<Value> {
    (uri == FORMATS_URI).then(|| {
        json!({
            "uri": FORMATS_URI,
            "mimeType": "text/markdown",
            "text": FORMATS_TEXT,
        })
    })
}

/// One prompt: its name, title, description and arguments (name,
/// description, required).
struct Prompt {
    name: &'static str,
    title: &'static str,
    description: &'static str,
    arguments: &'static [(&'static str, &'static str, bool)],
    text: fn(&Map<String, Value>) -> String,
}

const PROMPTS: &[Prompt] = &[
    Prompt {
        name: "editable_script",
        title: "Turn a PDF into an editable script",
        description: "Inspect a screenplay PDF, then write it out as a .fountain and a .fdx.",
        arguments: &[("path", "The screenplay PDF, as an absolute path.", true)],
        text: |a| {
            format!(
                "Turn the screenplay PDF at {} into an editable script with Method Reader. \
                 First call inspect_script on it and tell me its title, how many pages it \
                 has, and how many scenes and speaking characters were found. Then call \
                 convert_to_script to write a .fountain and a .fdx beside it, and tell me \
                 where they went and any warnings. If a file is already there, ask me before \
                 passing overwrite.",
                arg(a, "path")
            )
        },
    },
    Prompt {
        name: "ebook_for_reader",
        title: "Make an e-book for a reader",
        description: "Make an e-book of a screenplay in the format a given reader takes.",
        arguments: &[
            (
                "path",
                "The screenplay, as an absolute path to a .pdf or a .fountain.",
                true,
            ),
            (
                "reader",
                "The reader or app it is for (a Kindle, a Kobo, a phone app...).",
                false,
            ),
        ],
        text: |a| {
            let reader = match a.get("reader").and_then(Value::as_str) {
                Some(r) if !r.trim().is_empty() => format!(" It is for: {}.", r.trim()),
                _ => String::new(),
            };
            format!(
                "Make an e-book of the screenplay at {} with Method Reader's make_ebook \
                 tool.{reader} For a Kindle over USB, ask for azw3, and add mobi for \
                 the oldest Kindles; for any other reader or app, ask for epub. If you \
                 can't tell which reader it is for, ask me. Tell me where the book went and \
                 any warnings.",
                arg(a, "path")
            )
        },
    },
];

fn arg(a: &Map<String, Value>, k: &str) -> String {
    a.get(k)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// `prompts/list`'s entries.
pub fn prompts() -> Vec<Value> {
    PROMPTS
        .iter()
        .map(|p| {
            let args: Vec<Value> = p
                .arguments
                .iter()
                .map(|(name, description, required)| {
                    json!({"name": name, "description": description, "required": required})
                })
                .collect();
            json!({
                "name": p.name,
                "title": p.title,
                "description": p.description,
                "arguments": args,
            })
        })
        .collect()
}

/// `prompts/get`'s result for `name`, or why it can't be given.
pub fn prompt(name: &str, args: &Map<String, Value>) -> Result<Value, String> {
    let p = PROMPTS
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| format!("unknown prompt: {name}"))?;
    for (k, v) in args {
        if !p.arguments.iter().any(|(n, _, _)| n == k) {
            return Err(format!("{name} takes no argument `{k}`"));
        }
        if !v.is_string() {
            return Err(format!("{name}: the argument `{k}` must be text"));
        }
    }
    for (n, _, required) in p.arguments {
        let given = args
            .get(*n)
            .and_then(Value::as_str)
            .is_some_and(|s| !s.trim().is_empty());
        if *required && !given {
            return Err(format!("{name} needs the argument `{n}`"));
        }
    }
    Ok(json!({
        "description": p.description,
        "messages": [{"role": "user", "content": {"type": "text", "text": (p.text)(args)}}],
    }))
}
