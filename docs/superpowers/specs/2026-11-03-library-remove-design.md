# Removing a script from the library

2026-11-03. The library view's next slice (ROADMAP.md, Now: "Next:
removing a script from the library"). Design for the first slice:
docs/superpowers/specs/2026-11-02-library-view-design.md.

## The problem

The Library tab lists every script in the library folder, and there is no
way to take one off it short of finding the folder by hand. A script
converted by mistake, or twice under two names, stays on the list for good.

## The decision: to the Trash, never deleted

A script's folder holds more than files the app can make again: its own
settings file is the person's work, and when the PDF has moved or gone the
`.fountain` and the book are the only copy left. So removing a script moves
its whole folder to the system Trash (the Recycle Bin on Windows, the
freedesktop trash on Linux), where the person can put it back. Nothing is
deleted outright.

Built on that recommendation and marked so in the roadmap's "Waiting on
the owner". To reverse: the engine's one trash call becomes a removal.

The Trash comes from the `trash` crate (5.2.9, MIT), which reuses the
objc2-foundation 0.3.2 and windows 0.62 already in the tree. The workspace
forbids unsafe code, and moving a file to the Trash needs a system call
that no safe standard-library function makes.

## 1. The engine and the command line: `method-reader library-remove <folder>`

- **What it takes:** one script folder, exactly as the `library` listing
  gives it in `folder`.
- **What it refuses**, each with its own message and nothing moved:
  - a folder that is not directly under the library root (compared with
    links resolved on both sides, so `..` and a linked root cannot reach
    outside it);
  - a folder that is itself a link;
  - a folder the listing would not list (no source.json naming a source,
    or no `<stem>.fountain` beside it): removing only what the tab shows
    keeps a stray folder in the root from going by mistake;
  - the root itself.
- **What it answers**, with `--json`:
  `{"ok": true, "removed": "<folder>", "name": "<stem>"}`. Without
  `--json`, `moved <folder> to the Trash`.
- **On a Mac** the move uses the file manager's own call, not the Finder:
  asking the Finder needs permission to drive another app, which the
  signed app does not hold.
- **The recent list** is left alone: it names the PDF you chose, which is
  still where it was.
- **The seam:** `METHOD_READER_TRASH`, when set, is a folder that stands
  for the Trash: the script's folder is moved into it (a rename, under a
  fresh name when one is taken). The folder must already be there: it is
  never made, so a guard path cannot become a real folder. Every test sets it. sp-engine's
  `guarded_env()` sets it to a path under /dev/null, so a test that forgets
  fails rather than filling the owner's Trash.

## 2. The window: Remove on each row

- Each row gets **Remove from library**, a quiet button under the row's
  details (Open's column has room for one button only).
- Pressing it asks first, the way taking a book off a reader does: "Move
  <title> to the Trash?" with "Its folder, book and settings go to the
  Trash, where you can put them back. The PDF stays where it is." and the
  buttons **Move to Trash** and **Cancel**. The keyboard lands on Cancel.
- After it goes, the tab asks for `library` again and says
  "<title> is in the Trash." A refusal says the engine's message.
- Removing is disabled while a conversion runs, and while another removal
  is under way.
- When the removed script is the one open in the window, the window keeps
  it open: the preview is already drawn, and nothing reads the folder
  again until the next conversion.

## Not in this slice

- Renaming a script.
- Removing several at once.
- Undo inside the app: the Trash is the undo.

## Tests

- **sp-engine, `tests/library_remove.rs`:** each refusal moves nothing; a
  real removal leaves the folder in the stand-in Trash and off the
  listing; a second folder of the same name in the Trash does not
  collide; `guarded_env()` refuses to move anything.
- **sp-cli:** the verb through a sandbox with `METHOD_READER_LIBRARY` and
  `METHOD_READER_TRASH`; `--help`; the `VERBS` list; a PARITY.md row.
- **sp-app:** the argv contract; Remove asks first and Cancel sends
  nothing; Move to Trash sends exactly one `library-remove` with the row's
  folder, then asks for `library` again; the row labels and the question's
  words; disabled while busy.
