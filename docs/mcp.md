# `method-reader mcp`: scripts for coding agents

`method-reader mcp` is a [Model Context Protocol](https://modelcontextprotocol.io)
server built into the command-line converter. It lets an agent (any MCP
client) look inside a screenplay PDF and make editable scripts and e-books
from it with typed tools, instead of composing shell commands.

It works on **files** named by their absolute paths. Every tool runs the
command line's own conversion in the same process, the code path the window
and `method-reader --json` use, so a tool's answer is the command line's
answer. No network, no other programs, nothing sent anywhere.

## Starting it

```sh
method-reader mcp [--root DIR]... [--read-only]
```

| Flag | Meaning |
|---|---|
| `--root DIR` | A folder the tools may read and write in. Repeat it for several. Default: the folder it was started in. Every path in a tool call must be absolute and inside a root. |
| `--read-only` | Offer only the tools that write nothing (`inspect_script`, `preview_page`). |

The server reads JSON-RPC messages from stdin, one per line, and writes one
response per line to stdout. stdout carries protocol messages only; a line
per tool call (its name, whether it worked, how long it took) goes to stderr,
with anything the conversion said there. It exits when stdin closes.

`screepub mcp` answers too, without the old name's notice.

## Registering it

The converter is a download of its own: see
[Install the command-line converter](developers.md#install-the-command-line-converter).
Use its full path below if it isn't on the `PATH` your client starts
programs with.

Claude Code, for every project, with your scripts under `~/Scripts`:

```sh
claude mcp add --scope user method-reader -- method-reader mcp --root ~/Scripts
```

Claude Desktop, in `claude_desktop_config.json`:

```json
{ "mcpServers": { "method-reader": { "command": "/path/to/method-reader", "args": ["mcp", "--root", "/Users/you/Scripts"] } } }
```

Any other MCP client: start `method-reader mcp --root <folder>` over stdio.

## Tools

Every tool takes `path`, the script, as an absolute path. A scanned PDF (no
text layer) is refused; so is a document that doesn't read as a screenplay,
unless `force` is true (as `--force` does). Each conversion uses the
script's saved settings when `<name>.method-reader.json` sits beside it,
else the format defaults chosen in the app, else Method Reader's own.

| Tool | Takes | Gives |
|---|---|---|
| `inspect_script` | `path` (.pdf), `force` | Title and author as detected, page count, every scene (heading, the scene number a shooting script prints, the page it starts on), every speaking character (name, number of speeches, first page), warnings. Writes nothing. |
| `preview_page` | `path` (.pdf), `page`, `force` | One page as plain text, and each element on it with its kind (scene, action, character, parenthetical, dialogue, transition...). Pages are the PDF's sheets counted from 1, which may differ from the numbers printed on them. Writes nothing. |
| `convert_to_script` | `path` (.pdf), `formats` (`fountain`, `fdx`; default both), `output_dir`, `overwrite`, `force` | Writes `<name>.fountain` and `<name>.fdx` (beta: a reconstruction, so revision marks and exact page breaks do not carry over). Pictures (beta) go in `<name>.images` beside the `.fountain`. Answers with the paths and the counts. |
| `make_ebook` | `path` (.pdf, .fountain or .txt), `formats` (`epub`, `azw3`, `mobi`; default `epub`), `output_dir`, `title`, `author`, `overwrite`, `force` | Writes `<name>.epub`, `<name>.azw3` (beta, for Kindles over USB) and `<name>.mobi` (the oldest Kindles; text only). Answers with the paths. |

Files are named as the command line names them, after the script, and go
into `output_dir` (an existing folder inside a root), or the script's own
folder. A file already there is refused unless `overwrite` is true. The
EPUB a conversion always makes is written to a scratch folder and removed
when `make_ebook` wasn't asked for one; `inspect_script` and `preview_page`
leave nothing behind.

A result is a one-line summary for a person and the same answer as JSON
(`structuredContent` from protocol revision 2025-06-18 on). A refusal comes
back as a tool error (`isError`) with a kind (`invalid_input`, `not_found`,
`path_refused`, `refused`, `io`) and the command line's own message and
error code, so an agent can read it and try again.

## Resources and prompts

- Resource `method-reader://formats`: what Method Reader reads and writes,
  and how it names what it writes.
- Prompt `editable_script` (`path`): inspect a PDF, then write its `.fountain`
  and `.fdx`.
- Prompt `ebook_for_reader` (`path`, `reader`): make the e-book a given
  reader takes.

## Safety

- Every path must be absolute and inside a root, after every symbolic link
  is followed. Nothing under a hidden folder (a name starting with `.`) is
  read or written, and nothing is written through a symbolic link.
- Inputs must be `.pdf` (or `.fountain`/`.txt` for `make_ebook`), at most
  512 MB. Outputs are only the formats above.
- Text from a script (titles, headings, names, dialogue) is returned as data.
  The server tells clients so in its instructions.

## Protocol

Revisions 2024-11-05 to 2025-11-25 through `initialize`, and 2026-07-28's
per-request `_meta` (with `server/discover`). The lists and reads that never
change while the server runs (`server/discover`, `tools/list`,
`prompts/list`, `resources/list`, `resources/templates/list`,
`resources/read`) carry 2026-07-28's caching hints: `ttlMs` 3600000 and
`cacheScope` "public". The protocol code is a copy of Method Budget's
(`rust/crates/sp-cli/src/mcp/`), kept close to it for the suite's shared
crate to come.

The tools gain `osf` among the script formats when Method Reader writes
that format.
