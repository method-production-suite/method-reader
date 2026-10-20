# ADR: the window gets doors, not commands; and the updater comes first

Date: 2026-09-21 · Status: accepted (user-approved)
Refines: [ADR 2026-09-12](2026-09-12-cross-platform-tauri.md)'s governing rule
Amends: [the handover plan](../superpowers/plans/2026-09-20-swift-to-tauri-handover.md)'s v0.6.1
Follows from: [the parity audit](https://github.com/method-production-suite/method-reader/blob/7eb40e5/docs/parity-audit.md)

## Two decisions

**1. The OS-launch shims are built by GRANTING PERMISSIONS, not by adding
Rust commands.** `desktop/src-tauri/capabilities/default.json` gains the
narrow `opener` and `dialog` (save) permissions. The shell keeps exactly
two commands, `run_engine` and `pick_file`.

**2. The automatic handover does not happen until the new app can update
itself.** v0.6.1 takes the bundle identifier only after the updater
exists. The other missing features may follow afterwards.

## Why permissions rather than commands

The ADR's governing rule is that **Rust is a window, not a brain**, and it
was enforced as recently as 2026-09-21, when Cancel was refused because a
kill handle meant a third Rust command.

A permission is a different kind of thing from a command, and the
distinction is the whole basis of this decision. A command is code this
project writes and maintains, and code is where decisions hide: a
`send_to_route` command would have to know which route, in what order,
with what fallback, and that is a brain. A permission is a line in a
manifest saying the window may ask the OS to open a link or show a save
box. It adds a door. It adds no judgement, no branching and no lines of
Rust.

So the rule survives literally: the shell still registers two commands,
and "Rust is a window" stays true in the strongest available sense, which
is that there is almost no Rust.

**Corrected 2026-09-21, before this was acted on.** A first draft said the
grant gives the window "access to plugins that exist, not new plugins".
True of `dialog`, which is already a dependency and already initialised in
`main.rs`. NOT true of the reveal: `tauri-plugin-opener` is a separate
crate and is the only one that "reveals" a file in the system file
explorer, where `tauri-plugin-shell`'s open would open the containing
folder without selecting the file.

So Show in Finder costs one new dependency, and this ADR says so rather
than hiding it behind "a line in a manifest". The decision does not
change; one word of the argument does. The shims cost **one new crate and
some scoped permissions**, not zero crates. Still no new commands, still
no new logic in Rust, and the crate is a first-party Tauri plugin whose
whole job is handing a path or a URL to the OS, which is the definition
of a door.

Raised by the interface-pass session, which checked `Cargo.toml` and
found two plugin dependencies where this ADR implied three. Verified here
against the plugin's own documentation before the wording changed.

**The grants are SCOPED, which is what keeps this cheap.** The opener
plugin takes allow-lists: `opener:allow-open-url` can name the hosts it
may reach and `opener:allow-open-path` the paths. The window is not
granted "open anything"; it is granted the bug tracker, Amazon's two
pages, and the library. Write the scopes with the permissions, not
afterwards.

**Amended 2026-09-23 (parity piece D).** `opener:allow-open-url` gained
three exact URLs: Calibre's macOS and Windows download pages and Amazon's
Kindle Previewer page, so the Send page's KFX checklist can link to what it
says is missing. Approved by the owner the same day. Same test as every
grant here: a door, not an opinion. What to link is decided by
`src/export/kfx-setup.ts`, and `tests/desktop-shell.test.ts` holds that
file's links and this grant to the same three strings.

**Amended 2026-09-23 (parity piece B, send routes).** Apple Books, Amazon's
Send to Kindle app and its own web page, Amazon's Personal Document
Settings page (the email route's one-time setup) and Apple Mail are all
opened by the ENGINE, not the window. So is the file manager, already in
this piece: the Send to Kindle web route opens the book's folder beside
Amazon's page. (Show in Finder moves to the engine with piece C.) The
owner's reasoning: the routes open a file that lives in the Screepub
library, piece C is what makes that library's location movable, and a
window permission is a fixed path written into a manifest that cannot
follow it there. So these are four `screepub route <key>` calls the engine
performs (`apple-books`; `send-to-kindle`, which opens the folder too;
`email-to-kindle`; and `kindle-email-setup`, which takes no book), through
the same door `run_engine` already is, and they cost the window no new
permission at all. The window's own doors gain exactly one new entry for
this piece: `dialog:allow-save`, so the reader can choose where a saved
copy of the EPUB or the Kindle file goes. It has no allow-list to write,
for the same reason `process:allow-restart` does not: the user picks the
path in the native box, and the window only ever hands that one string
back to the engine, which is what writes there. Approved by the owner the
same day ("yes go for it"). Same test as every grant here: a door, not an
opinion.

**Amended 2026-09-23 (the update notice and window drag).** One new crate:
`tauri-plugin-process`, linked for exactly one reason, restarting the
window after an update once the engine is idle. Only
`process:allow-restart` is granted; the plugin's own `exit` is not. A
second grant, `core:window:allow-start-dragging`, costs no new crate at
all: it widens a permission `core` already provides, so the strip along
the top of the window, and the gaps in the tab bar, can move it. Neither
grant has an allow-list to write, unlike the opener grants above:
`process:allow-restart` restarts THIS app and reaches nothing else to
scope, and `core:window:allow-start-dragging` only lets the page ask to
move the one window it is already inside, which is not a resource a list
of allowed values would narrow. Approved by the owner the same day. Still
two commands.

**Amended 2026-09-23 (parity piece C, the gear).** The library folder can
now be chosen (`screepub app-settings`), so the window's one path-scoped
door, `opener:allow-reveal-item-in-dir` fixed to `$DOCUMENT/Screepub/**`,
could no longer follow it, and it is removed. Show in Finder is the
engine's `screepub reveal <file>` instead, through `run_engine`, the same
answer piece B gave for the routes above. The window gains one door in its
place: `dialog:allow-open`, so the reader can choose the folder in the
OS's own picker. Like `dialog:allow-save`, it has no allow-list to write:
the user picks the folder, and the window only hands that one path to the
engine, which checks it, creates it and stores it. Approved by the owner
the same day ("yes go for it"). Still two commands.

What this buys, and it is six of the nine gaps the audit found: Apple
Books, Send-to-Kindle, email-to-Kindle, save-a-copy, Report a Bug, and
Show in Finder.

**What it does NOT license.** Any future permission is this same
decision again, made deliberately, with the same test: does the window
gain a door, or does it gain an opinion? `shell:execute` in particular is
not covered here. It is the ability to run arbitrary programs, which is
a brain wearing a permission's clothes, and the engine is already the
only thing allowed to be spawned.

## Why the updater comes first

The [migration ADR](2026-09-20-swift-app-migrates-itself.md) accepted the
automatic handover by knocking down four objections. The second was "the
payload would remove the updater", and its resolution reads:

> **RESOLVED by decision.** The owner chose a full ported self-update
> rather than notify-only or nothing … The new app will have an updater,
> so the upgrade no longer ends the user's ability to receive upgrades.

Resolved by a decision to port. **The port did not happen.** The parity
audit found the updater at zero lines in the Tauri app on 2026-09-21,
five weeks of documents later.

So v0.6.1, as planned, would do the exact thing objection 2 says must not
be done, and it would do it through a **one-way door**: once a user is
moved onto an app that cannot update itself, no future release can reach
them, because the app that would have carried the fix is the one being
replaced. The only remaining route is asking them to download something
by hand, which is what the automatic upgrade exists to avoid.

This is not a parity preference. It is the migration ADR's own stated
precondition, unmet, and the decision here is only to stop treating it as
met.

**The other eight gaps are different in kind and may follow.** Apple
Books, Send-to-Kindle, email, save-a-copy, Show in Finder, feedback, the
KFX install surface and the gear are all things a later update can
deliver, PROVIDED the user can receive a later update. That proviso is
the whole of decision 2.

## Consequences

- v0.6.1 is blocked on one piece, and that piece is the largest single
  item in the retirement: 85 of the 171 unreplaced checks.
- `app/` keeps being maintained for longer, which is a real cost and is
  accepted. F3 was already blocked by the audit.
- The updater needs a design decision before it can be planned, and this
  ADR does not make it: **Tauri's updater plugin, or a port of
  `UpdateInstall.swift`'s behaviour?** The plugin is cross-platform and
  wants a key pair and a published update manifest, which is a new
  secret and a new artifact. The port keeps the frozen app's
  codesign-pinning behaviour, which is the thing `docs/retired-coverage.md`
  says must survive "whatever shape" the updater takes, but an in-place
  bundle swap is native work on each platform. That question is the
  first thing the updater's own spec has to answer.
- Nothing here changes v0.6.0, which shipped and is verified.
