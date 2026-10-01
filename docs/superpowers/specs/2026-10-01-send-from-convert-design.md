# Design: send from the result screen, device first

Date: 2026-10-01 · Status: for the owner's review. Built from four
mockups (3-send, 4-send, 5-live and its two revisions). Second layout design
for the window, after the layout pass (2026-09-29).

## What

The Send tab goes. Sending happens on Convert's result screen, and it starts
from the reader's device, not from a list of routes: "What do you read on?"
is asked once and remembered. While a script converts, its first page types
itself out and takes its formatting in a wave. After a copy over USB there is
an Eject button. A reader whose device is missing can ask for it from inside
the app, with no account.

## Why

The owner's notes, 2026-10-01: the Send page is a separate place for what is
really the last step of converting; the result screen's options felt
"confusing and overwhelming"; the routes are named in Screepub's vocabulary
(KFX, AZW3, the Send to Kindle app) rather than the reader's ("I have a
Kindle"). He asked for the device to come first, for Send to Kindle on the
web to lead for Kindles because email is clunky, for an asterisk on what has
never been tried on hardware, for every file type to stay reachable "for
nerds", and for the conversion itself to be a moment worth watching.

## Decisions

### The result screen

1. **The tabs are Convert, Read and Settings.** The Send surface and its tab
   are removed. Everything it did is reachable from the result screen.
2. **The result screen is a page and a panel.** Left: the book's first page,
   in its own typesetting (the same page the conversion animation ends on).
   Right: "<Title> is ready", one line of counts ("19 pages, 11 scenes"), then
   the send panel, then three quiet links: Read it, Settings, Convert another.
3. **First time: "What do you read on?"** The panel shows device cards:
   Kindle; iPhone, iPad or Mac (Apple Books); Kobo; tolino; reMarkable; and a
   last card, "My device isn't here". The choice is stored app-wide (the app
   settings file, key `readerDevice`, written by the engine like `lastRoute`)
   and the question is not asked again.
4. **After that: the calm screen.** One brass button for the way that fits
   right now, one plain "Other ways" button, and a quiet "Different device"
   link that brings the cards back.
5. **A device the computer can see is never asked about.** The window already
   polls `devices`. A plugged-in reader of the chosen kind turns its card's
   edge brass with a green dot, and makes "Copy to <name>" the brass button.
   A plugged-in reader of another kind is offered in "Other ways".

### What each device offers

6. **Kindle.** Plugged in: "Copy to Kindle" (the best Kindle file this
   computer makes, KFX when the tools are there), then Send to Kindle on the
   web, then email. Not plugged in: **Send to Kindle on the web leads**
   (owner, 2026-10-01: the most seamless; Amazon's own conversion keeps
   scenes together), then email, then "plug it in to copy over USB". The web
   route opens Amazon's page beside the book's folder and the reader drags
   the book in; the panel says so in one line, because Screepub cannot upload
   it for them. On a Mac with Amazon's Send to Kindle app installed, the app
   replaces the web page, as the route catalog already decides.
7. **iPhone, iPad or Mac.** On a Mac: "Add to Apple Books" (it syncs through
   iCloud). Elsewhere: "Save the EPUB", with one line on getting it to the
   device.
8. **Kobo, tolino, reMarkable.** Their existing routes (copy over USB; the
   reMarkable's USB web interface). Each carries an asterisk. Pressing or
   hovering it says: "Built and tested in code, never tried on a real <name>
   yet. If it works for you, tell us." with a link to report it (see 14).
   The proven set stays the one `send.js`'s PROVEN already records (a Kindle,
   on a Mac); everything else gets the asterisk, including a Kindle on
   Windows or Linux.
9. **The Kindle file builds ahead, as today.** When the chosen device is a
   Kindle (or one is plugged in), the KFX or AZW3 build starts as the result
   appears, with today's one line and moving bar under the brass button.
   "Copy to Kindle (.kfx)" keeps naming the file type on the button.

### Every way to send

10. **"Other ways" opens one list in place, in two groups.** *Send it*: every
    route the engine lists today, asterisks included. *Save a file*: every
    file type Screepub makes, each with one line on who it is for:
    EPUB (most readers, Apple Books, email), KFX (modern Kindles over USB,
    the best layout; greyed with a "set it up" link when the tools are
    missing, which opens today's KFX checklist), AZW3 (older Kindles over
    USB), MOBI (the oldest Kindles), Fountain (the script as plain text, for
    screenwriting apps). The KFX setup checklist lives here now.
11. **The engine learns to make any one of them on request:**
    `screepub export <book.epub> --for epub|kindle|kfx|azw3|mobi|fountain`.
    `kindle` keeps today's meaning (the best rung this computer reaches);
    `kfx` refuses with the existing toolchain error when the tools are
    missing; `fountain` copies the script's `.fountain`. `--out` and
    `--check` work for every value.

### After a copy: Eject

12. **A copy to a reader over USB ends with "Eject <name>" as the brass
    button** and one line: "<Title> is on your <name>. Eject it before you
    unplug." New engine verb `screepub eject <volume>`: macOS
    `diskutil eject`, Linux `udisksctl unmount` then `power-off`, Windows the
    shell's Eject verb. It refuses anything that is not a mounted reader
    volume the engine itself would list, so the window cannot eject an
    arbitrary disk. A volume that will not eject (something has a file open)
    says so and keeps the button.
13. **The result of a send survives leaving the page.** The status line and
    the Eject button are kept in module state and drawn again on return, so
    the eject reminder is never lost (an audit finding against today's Send
    page, 2026-10-01).

### "My device isn't here"

14. **One small field, then Send.** The device's name (up to 80 characters)
    goes, with the app's version and platform and nothing else, to a request
    service: a Cloudflare Worker on the free plan (owner: free services only,
    Cloudflare preferred) that stores it in a free D1 table, adding to a
    count when the same device is asked for again. The owner reads the list
    in Cloudflare's dashboard. The Worker accepts only that one form, caps
    lengths, and limits each sender to a few requests an hour. Its address is
    the free `*.workers.dev` one; screepub.com stays at Porkbun for now.
    The "tell us" links on asterisked routes use the same service with a
    "worked" or "did not work" answer. The README's network list gains one
    line for it. The Worker's code and its tests live in this repository;
    the owner signs in with `bunx wrangler login` and runs the deploy
    commands; no key passes through a chat.
15. **Offline or refused:** the field keeps its text and says it could not
    send, with Try again. Nothing is queued silently.

### A Kindle plugged in while a book is open

16. **A short line slides in at the top of whatever page is showing:**
    "Kindle plugged in. Copy <Title>?" with one button and Not now. It goes
    away by itself after eight seconds, never sends without a press, and
    appears once per plug-in. It uses the same `devices` poll, which now runs
    whenever a book is open, not only on the Send page.

### The page writes itself

17. **While converting, the first real page types out**, replacing today's
    progress bar (a slim "reading page N of M" line stays under it). The
    engine sends that page's text early: under `--progress`, as soon as it
    has read the first page that is not a title page, it writes one progress
    line carrying that page's paragraphs (blank-line separated, plain text).
    The page is typed plain, left-aligned, in the book's own font, size and
    width, so lines wrap where they will end up.
18. **The formatting rolls in two blocks behind the typing** once the book
    exists: each block takes its form (the heading bold, a character name to
    its place, dialogue tucked in) with a brief brass glow. If the engine
    finishes first, the wave starts straight away; if it is still working,
    the typing finishes and the wave waits for the book, then sweeps. The
    page that results is the result screen's preview page (decision 2).
19. **Timing:** typing takes about four seconds whatever the script; the
    show is never longer than about six seconds plus whatever the engine
    still needs. Nothing waits on the show: a click anywhere finishes it at
    once, and the result's buttons work as soon as they appear.
20. **Typewriter is the style** (the owner chose it over ink). Key sounds are
    generated in the window with Web Audio (no recorded files to license).
21. **Two app-wide switches on the Settings page:** "Animate the page" (on by
    default) and "Typewriter sounds" (off by default). The system's Reduce
    Motion setting turns the animation off regardless, and the text is in
    the page for screen readers from the first moment.

## What this does not do

- It does not change the conversion itself, or what any route sends.
- It does not move screepub.com to Cloudflare (a separate, later step for
  the site's security headers).
- It does not build a library view or reopen earlier books after a relaunch.

## Build order

Each part is its own plan, built and reviewed in turn:

1. **Engine:** `export --for` every type, `eject`, the early first page under
   `--progress`, `readerDevice` in the app settings.
2. **Send from Convert:** the result screen (page and panel, device cards,
   calm screen, other ways, eject, asterisks), the Send tab removed, the
   Kindle heads-up. send.js's route logic is kept and re-homed rather than
   rewritten.
3. **The page writes itself:** the typing, the wave, sounds, the two
   switches.
4. **Device requests:** the Worker, its tests, the window's field, the
   README line; deployed by the owner from the commands the plan gives.

## Coordination

Screeepub 2's images branch (ships after the identifier release) also edits
desktop/ui/convert.js's progress code and adds a nineteenth Settings knob;
part 3 rebases on whichever lands first. The identifier release renames the
app; nothing here depends on the name.

## Checks

Test-first for every engine verb and every pure decision (which button is
brass, what a device card offers, the wave's order and timing with an
injected clock). The measured-layout test (headless Chrome) covers the
result screen at 900, 1280 and 1800 wide. A live check in the browser pane
with the real engine on scratch folders, as in the layout pass. The owner's
hands on a test build, including a real Eject of his Kindle.
