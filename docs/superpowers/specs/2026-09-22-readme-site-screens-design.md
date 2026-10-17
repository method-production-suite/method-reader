# Design: a new README, a truthful site, and pictures that retake themselves

Date: 2026-09-22 · Status: approved in conversation, section by section.
Owner decisions are recorded below as they were made.

Built so far (2026-09-29): part 4 shipped with 0.7.2; part 3 is on main;
parts 2 and 5 are on branch `site-and-release-skill`. Part 5's durable
half is `tools/bump-version.ts` and `tools/review-screens.ts`; the skill
text that calls them lives outside git, as this spec says it must. Part 1
waits for the identifier release.

2026-10-01: part 1 built ahead of the identifier release, adapted to it not
having happened. The README's Download button still gives the SwiftUI DMG
and the Install section says which Mac download to pick; the platform badge
names all three platforms; "who has verified what" moved to
`docs/verification-ledger.md`, and the developer material to
`docs/developers.md`.

2026-10-16: brought to the Method suite's picture standard
(SUITE-STANDARDS.md section 7.1), which took this design as its model. The
shots are now the suite's four, in its order, every one in light and dark
and every one written to both `assets/screens/` and `site/img/` (the site
has a dark mode now): (1) `read`, the Read page, is the README's top
picture and the site hero's; (2) `compare`, the before and after, keeps the
old hero's drawing (renamed from `hero`, captured by
`tools/capture/compare.html` at 1280 by 800, with the site's sticky header
hidden again) and sits under the README's opening paragraph; the site keeps
its live section instead; (3) `drop` with `result` beside it, unchanged;
(4) `cli`, new: the real `method-reader` binary converts a copy of Field
Station into the scratch library, and `tools/capture/terminal.html` sets
that output, the book's folder and the head of its .fountain in a terminal
framed like the window. Under "Yours to keep" on both pages. Same command.

## The problem

- **The README does two jobs badly at once.** About 3,100 words: a third is
  for screenwriters, the rest is install hedging for two Mac apps, a privacy
  section, and a full developer manual. A screenwriter decides in the first
  screen, and the first screen is fine; the problem is everything they have
  to scroll past to reach Install.
- **Its three pictures are of the Swift app at 0.4.1**, from July. The
  Swift app retires at the identifier release. The invented script they
  show is no longer in the repository, so they cannot be retaken.
- **The site and the README say the same things in different words** (the
  pitch, the device status, "your script never leaves your machine") and
  drift apart.
- **The site never shows the real app.** Its drag-and-drop section is a
  drawing.
- **Both pages hand-list the download files, and both are stale.** They
  name the 0.6.0 Linux and Windows files, because those files carry the
  version in their names.

## Decisions (owner, 2026-09-22)

1. **The front page shows the cross-platform window**, not the Swift app. It
   becomes the Mac app at the identifier release, and it is the only one
   whose pictures can be taken by a script.
2. **Readers first, developers one link away.**
3. **The site is the tour, the README is the front door.** One set of
   pictures, from one tool, used by both.
4. **The hero picture is the before-and-after**: the site's existing section
   that scrolls a PDF page against the same scene reflowed on an e-reader,
   captured as a still. One drawing, two places.
5. **The invented script is "Field Station"**, grown from the scene the
   site already shows (MARA, DELACROIX) into a full committed script.
6. **Pictures are taken by headless Chrome over the window's own files, fed
   real engine answers**, not by capturing the running app. Unattended, no
   display, no permissions, same pixels every run.
7. **The README shows four pictures**: the hero, the drop-and-result pair,
   and reading with the scene index.
8. **The site's drag-and-drop drawing is replaced** by the same real
   drop-and-result pair. The drawing's small animation is the accepted loss.
9. **Download filenames lose their version numbers**, the way the Mac
   downloads already have. Chosen over versioned files behind redirect pages
   and over rewriting both pages every release.
10. **Only the Mac download gets a top button.** Linux and Windows are
    listed with a plain status: nobody has installed either by hand yet, and
    a test already forbids the README claiming Windows for the window.

## Part 1: the README

Target about 1,100 words. In order:

1. Name, one-line pitch, badges. The platform badge says **macOS** only.
2. One **Download for Mac** button, an **Other platforms** link to the
   install table, and **Take the tour at screepub.com**.
3. **The hero**: the before-and-after still.
4. The opening paragraph, kept as it is today.
5. **What it does**: the five existing bullets, tightened.
6. **The drop-and-result pair**, light and dark variants through
   `<picture>`, so GitHub shows the one matching the reader's theme.
7. **Which readers?**: the device table, kept as it is. The prose around it
   cut to two short paragraphs.
8. **Reading, with the scene index**, light and dark.
9. **Install**: one table of computer, file and status. Status says
   "verified by a person" or "built and checked automatically, never
   installed by a person yet, reports welcome". Homebrew in one line.
   The Windows warning in two sentences. The command-line downloads in one
   line with a link.
10. **Your script stays on your machine**: the privacy bullets kept, and the
    window's network touchpoints. The Swift app's touchpoint paragraph goes,
    because at the identifier release there is no Swift app to describe.
11. **For developers**: three lines and a link to a new `docs/developers.md`.
12. **License**, unchanged.

**What moves out, not deleted:** the CLI reference, the library layout, the
device commands, development commands and architecture go to
`docs/developers.md`. "Who has installed what, and by whom" goes to
`desktop/README.md`, which already keeps that ledger.

**Tests:** about a dozen tests pin README sentences today
(`tests/release-artifacts.test.ts`). Each pinned fact either stays on the
page in the new wording or its test follows the text to the new file. None
is deleted without saying which fact it guarded and where that fact went.

**When:** the rewrite is prepared on a branch and merges with the
identifier release, because it describes the window as the Mac app.

## Part 2: the site

Changed:

- **Drag and drop**: the drawing becomes the real drop-and-result pair,
  light variants (the site has no dark mode).
- **Every download**: version-free links with the same status column as the
  README.
- **The Download for macOS buttons** point at the window's Mac download from
  the identifier release on. Until then they keep pointing at the Swift
  app, so the site and the README switch together.

Unchanged: the hero words, the before-and-after section (it is the source of
the README's hero), the live settings panel, the audience section.

Images live in `site/img/`, because the Pages workflow deploys `site/` alone.

## Part 3: the capture tool

Read this with [Amended while planning](#amended-while-planning-2026-09-22-from-measurements) below: it replaces the `tools/screens/` pages and the recorded replay described here.

One command: `bun tools/capture-screens.ts`. No new dependencies.

1. **The script.** `tests/fixtures/field-station.pdf`, committed, made by a
   new `demo` kind in `tools/make-fixture.py`. About 25 pages and a dozen
   scenes. Scene one is the site's scene word for word. It joins the
   byte-for-byte fixture stability test with the other committed fixtures.
   Every name in it is invented, as the project rule requires of anything
   that can reach a picture.
2. **The real engine, recorded.** The tool runs the real CLI on the script
   with the exact arguments the window builds, by calling the window's own
   `argv` function from `desktop/ui/app.js`, not a copy. It runs with
   `SCREEPUB_LIBRARY` pointed at a scratch folder whose path ends in
   `Documents/Screepub`, so the real library is never touched and any path
   in frame looks like the default one. Every answer is recorded for this
   run only.
3. **The real window, replayed.** One small page per picture in
   `tools/screens/`, served by `Bun.serve` because ES modules do not load
   from `file://`. Each page installs a stand-in for `window.__TAURI__`
   covering exactly what the window uses: `core.invoke` for `run_engine`
   and `pick_file`, and `event.listen`, including firing the
   `tauri://drag-drop` event that a real drop fires. `opener` and `updater`
   are absent, so the update control stays hidden, as it would in a build
   without the plugin. Then it loads the window's own `main.js` and steps
   into the state the way a person does.
   **The replay never answers a question it did not record.** An argument
   list it has not seen throws, and the capture fails naming it, so a
   change to how the window calls the engine can never produce a picture of
   a state the engine did not really produce.
4. **The picture.** Headless Chrome at 2x, the same way `tools/make-og.sh`
   makes `og.png`. The window frame (rounded corners, shadow, the three
   traffic lights sitting on the paper, as the Overlay title bar puts them)
   is drawn by the page in CSS. Light and dark for window pictures. The
   window's version stamp is hidden, so a picture changes only when the
   window's look changes (see Part 5).
5. **The hero.** The site's own page, loaded and held at the scroll
   position where the before-and-after is fully in view, then captured.
6. **Written only if changed.** Output to `assets/screens/` for the README
   and copied to `site/img/`. A file is replaced only when its bytes differ,
   so a release where nothing changed commits no images.

Nothing goes into `desktop/ui/`. No demo mode ships in the product, and the
interface-pass session's files are untouched.

**Tests** (`bun test`, no Chrome): the replay throws on an unrecorded
argument list; the replay answers a recorded one verbatim; each picture's
step list is well-formed; the Field Station fixture is stable. The
picture-taking itself runs in the release skill, not on every push.

**Honest limits.** It is Chrome drawing the window, not the Mac's own
webview. The fonts are bundled, so text matches, but a rendering difference
between the two engines would not be caught here. The frame is drawn, not
captured.

### Amended while planning (2026-09-22), from measurements

Each of these was found by running something, not by reasoning, before
the plan was written. None changes what the owner approved; each changes
how.

- **Live engine answers, not recorded ones.** The capture server answers
  each engine call the window makes by running the real CLI with exactly
  those arguments, so every answer is real by construction. The "never
  answers what it did not record" rule becomes an allow-list: the version
  check, a conversion of the demo script, and settings or re-renders
  inside the demo library. Anything else (devices, send, export, any other
  path) is refused and fails the capture, naming the call.
- **Chrome is driven over its remote-control protocol**, not the
  `--screenshot` flag. The flag photographs whatever is on screen when its
  time runs out, including a failed state; the protocol lets the tool wait
  for the page to report `ready` or `failed`. No dependency: Bun's built-in
  WebSocket. Dark mode through the protocol's media emulation (the obvious
  command-line flag, `--force-prefers-color-scheme`, does nothing).
- **Two passes per window picture.** The window pins its binding and brads
  with fixed positioning, so it cannot be drawn inside a smaller frame on
  one page. Pass one captures the window at its own 860 by 620 size; pass
  two places that image in a rounded, shadowed frame with the traffic
  lights and captures again on a transparent background.
- **The window's own `index.html` is served, not copied.** The server
  inserts the capture scripts into it on the way out, so a stylesheet the
  window adds later is picked up without anyone touching the capture tool.
- **The site's scene sits on pages 14 to 18**, where the site's own page
  markers put it, not as scene one. It ends on FADE OUT, so Field Station
  is about 18 pages, not 25.
- **The demo library is `/Users/Shared/Documents/Screepub`**, because the
  result screen prints the book's full path. It needs no username and
  reads naturally. `/Users/Shared/Documents` is NOT on every Mac (it was
  missing on the one the tool was built on), so the tool creates it if
  needed and removes only what it created: the library, which it marks as
  its own, and the Documents folder only if the run made it and it is
  empty. It refuses to run if an unmarked library is already there.

## Part 4: version-free download names

In `tools/build-app-bundle.ts`'s `releasedName`:

| What | Today | After |
| --- | --- | --- |
| Linux, Debian or Ubuntu | `Screepub_<v>_amd64.deb` | `Screepub-linux-amd64.deb` |
| Linux, Fedora or openSUSE | `Screepub-<v>-1.x86_64.rpm` | `Screepub-linux-x86_64.rpm` |
| Windows | `Screepub-<v>-setup.exe` | `Screepub-windows-x64-setup.exe` |

An arm64 `.deb` built by hand becomes `Screepub-linux-arm64.deb`. The
version still lives inside each package, so apt, dnf and Windows show it.
The Mac window's DMG name is not touched: whether it takes the Swift app's
`Screepub-macOS.dmg` name is a handover question. The command-line
archives are already version-free.

**When:** first, so the new links point at files a real release has
published before either page uses them.

## Part 5: the release skill

- **The capture runs in the background** next to the preflight, so it adds
  no wait. Without Chrome at the path `make-og.sh` uses, it warns and skips:
  stale pictures are worth a warning, not a blocked release.
- **Moment one shows notes and pictures together.** Any changed picture is
  shown old beside new; if none changed, one line says so. The owner checks
  them as he checks the notes, including that nothing real is in frame.
  "ship" approves both.
- **On ship**, changed pictures join the release commit with the notes and
  the version bump.
- **Why the version stamp is hidden.** Left in, every window picture changes
  every release and the repository grows by a few megabytes each time,
  forever.
- **Stale instructions fixed in the same pass**, all found cutting 0.7.0
  and 0.7.1: the version lives in `package.json`, `Cargo.toml`,
  `Cargo.lock` and `tauri.conf.json`, and `desktop/ui/notes.js` is
  regenerated from the notes; commit and tag must be separate commands or
  the release-notes guard blocks the tag; and the tap now bumps itself
  (`TAP_TOKEN`, added 2026-09-22), so the skill checks the `tap-check` job
  instead of bumping by hand.

The skill stays outside git, as it always has. The durable logic is the
tool, so the skill only calls it.

## Order of work

1. Part 4, the filenames, then a release, so the new names exist.
2. Part 3, the fixture and the capture tool.
3. Part 2, the site's pictures and download list.
4. Part 5, the release skill.
5. Part 1, the README, on a branch that merges with the identifier release,
   together with the site's Mac button switch.

## What this does not do

- It does not change anyone's verification status. Linux and Windows say
  "never installed by a person" until a person installs one.
- It does not rename the Mac window's DMG.
- It gives the site no dark mode.
- It takes no pictures of the Swift app.
- It does not touch `desktop/ui/`.

## Coordination

`README.md` has been edited by both sessions. The interface-pass session is
told before the rewrite starts, and the rewrite merges only on the owner's
word.
