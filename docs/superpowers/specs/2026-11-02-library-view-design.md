# The library view

2026-11-02. The owner's pick for what Reader builds next ("Start the library
view"). ROADMAP.md, Later: "list your books, reopen one without converting
its PDF again, and be the obvious home for each script's settings."

## The problem

Every conversion the window makes lands in the library folder
(`~/Documents/Method Reader` unless the person chose another), one folder per
script. The window has no way to see what is in there. Reopening a script
means finding its PDF and converting it again, and a script whose PDF has
moved or gone cannot be reopened at all, although its `.fountain` and book
are sitting in the library.

## The first slice

Two pull requests, engine first.

### 1. The engine and the command line: `method-reader library`

A new verb, `library`, that takes no arguments and answers, with `--json`:

```json
{
  "ok": true,
  "root": "/Users/x/Documents/Method Reader",
  "scripts": [
    {
      "folder": "/Users/x/Documents/Method Reader/Field Station",
      "name": "Field Station",
      "title": "FIELD STATION",
      "source": "/Users/x/Scripts/Field Station.pdf",
      "sourceExists": true,
      "fountain": ".../Field Station/Field Station.fountain",
      "epub": ".../Field Station/Field Station.epub",
      "pages": 18,
      "scenes": 24,
      "converted": "2026-11-02T18:00:00Z",
      "others": [".../Field Station/Field Station.azw3"],
      "settings": ".../Field Station/Field Station.method-reader.json"
    }
  ]
}
```

- **What counts as a script:** a folder directly under the library root
  whose `source.json` names a source and which holds `<stem>.fountain`,
  where the stem comes from that source's file name. Anything else in the
  root is left out silently: no error for a stray folder or file. Links are
  not followed.
- **`name`** is the stem, and **`title`** is the `.fountain` title page's
  Title, with its emphasis and notes stripped (sp-fountain), or null.
- **`pages`** is the highest `= pg N` marker in the `.fountain`
  (sp-fountain's pages.rs). **`scenes`** is the number of scene headings the
  tokenizer finds, counting mini-slugs out (sp-fountain's slug.rs). The
  window needs both to show a reopened book the way it shows a converted
  one, so they come from the `.fountain` itself, never a separate cache.
- **`epub`** is null when the folder has no book yet. **`others`** lists the
  `.azw3`, `.kfx`, `.mobi` and `.fdx` beside it, sorted. **`settings`** is
  the per-script settings file, or null.
- **`converted`** is the `.fountain`'s modified time, as ISO 8601 UTC.
  Scripts come newest first, then by name.
- **The root** comes from `library_root`, the same lookup a conversion
  uses. A root that does not exist yet answers `scripts: []`, not an error.

The listing reads only. It never creates the root, rewrites source.json or
touches recent.json.

### 2. The window: a Library tab

- **`Surface::Library`** goes after Convert, and is labelled "Library". It
  is available with no script open.
- **Showing the tab** asks the engine for `library` each time, and drops a
  stale answer (an ask counter, as the reader's book list does).
- **Each row** shows the title (or the name), then pages, scenes and when
  it was converted. A row whose PDF has gone says so: its book still opens.
- **Open** reopens the book without converting the PDF. The window sends
  `convert-to` with the library `.fountain` as the input and its `.epub` as
  the output, the way the sample reopens, and lands on Preview with the
  row's pages and scenes. A row with no `.epub` yet makes one the same way.
- **An empty library** says where the library folder is and that
  conversions land there.

## Not in this slice

- Removing a script from the library, or renaming one.
- Per-script settings from a row: the row says whether a script has any,
  and Settings stays where it is.
- Counting a reopen as "opened" for the launcher's recent list.
- Search or sorting controls.
- Watching the folder for changes: the tab asks again each time it is
  shown.

## Tests

- **sp-engine, `tests/library_list.rs`:** a scratch root with guarded_env.
  Cover two scripts newest first; the name collision (a hashed folder); a
  folder with no source.json, which is skipped; a source that has gone
  (`sourceExists: false`); no `.epub`; a missing root, which answers `[]`;
  and a link, which is not followed.
- **sp-cli:** the verb through a sandbox with `METHOD_READER_LIBRARY`;
  `--help`; the `VERBS` list; and a PARITY.md row.
- **sp-app:**
  - the argv contract;
  - the tab exists with no script open;
  - the rows' accessible labels;
  - Open sends exactly one `convert-to` from the library `.fountain`, and
    no `convert`, then lands on Preview;
  - a stale answer is dropped;
  - the empty library's words.
