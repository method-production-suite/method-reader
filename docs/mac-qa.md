# QA on a Mac: the gate nothing else can pass

**Read this first (2026-10-12, the Rust cutover).** Every checklist below
was written for a release of the TypeScript engine and the Tauri window, and
is kept as the record of what was checked then. Its commands are that era's
and no longer run: Bun, `tools/*.ts`, `tools/*.sh`, `src/` and `desktop/`
are gone. For the next QA pass, the same steps are, from `rust/`:

| Then | Now |
| --- | --- |
| `bun test`, `bunx tsc --noEmit` | `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings` |
| `bun test tests/<name>.test.ts` | `cargo test -p <crate>` (rust/PARITY.md says which crate holds each old test file) |
| `bun src/cli.ts <args>` | `cargo run -p sp-cli --bin screepub -- <args>` |
| `bun tools/build-sidecar.ts --host`, then `cargo run` in `desktop/src-tauri` | `cargo run -p sp-app` (the engine is linked in; there is no sidecar) |
| `bun tools/verify-signing.ts --dmg <dmg> --expect <coexist or handover>` | `cargo xtask verify-signing --dmg <dmg> --expect <coexist or handover>` |
| `bun tools/capture-screens.ts` | `cargo xtask capture-screens` |

The tap steps below have no replacement: the tap was retired on
2026-10-18, and people install from the release's own downloads.

The window's update memory moved too: the Rust window keeps it in its own
storage, and on its first run copies it once from the Tauri window's WebKit
folder named below (docs/superpowers/specs/2026-10-11-rust-release-design.md,
"What carries across").

## QA for the identifier release (F2)

Written 2026-10-01 for the release that hands the Mac over from the Swift
app to the window (plan
`docs/superpowers/plans/2026-10-01-f2-handover-amendment.md`). Everything
in this release is about ONE thing: an installed app becomes the window
without anyone reinstalling. Run it after the release is published, not
before: the old updater reads the real release page.

### [claude] after the tag, before anyone updates

- [ ] **Do:** list the release's assets. **See:** exactly one `.dmg`, named
  `Screepub-macOS.dmg`; `latest.json`; `Screepub-macOS.app.tar.gz` and its
  `.sig`. **Fail:** a second `.dmg`, or any `Screepub-Desktop-` name.
- [ ] **Do:** download the image and run
  `bun tools/verify-signing.ts --dmg Screepub-macOS.dmg --expect handover`.
  **See:** it passes: an installed Swift app would INSTALL this. **Fail:**
  anything else. Stop here; nobody updates until this is green.
- [ ] **Do:** `tools/check-tap.sh`. **See:** the formula serves the new
  version and the cask is deprecated. **Fail:** either one.

### [hands] the Swift app updates itself into the window

- [ ] **Do:** open the Swift app (`/Applications/Screepub.app`, 0.7.x) and
  Check for Updates. **See:** it finds the new version, downloads, installs
  and relaunches. The app that opens is the window: Convert, Read, Tune,
  Send. **Fail:** "failed signature verification" (that was the right
  answer before this release and the wrong one now), or it relaunches into
  the old app.
- [ ] **Do:** look in `/Applications`. **See:** `Screepub.app`, now the
  window. **Fail:** a second copy, or a `.staged` / `.old-` leftover.
- [ ] **Do:** convert a script, then send it to the Kindle over USB.
  **See:** it works; macOS may ask once for access to removable volumes.
  **Fail:** the copy fails after you allowed access.
- [ ] Known and accepted: anything you tuned in the Swift app starts from
  the defaults once. Books are untouched.

### [hands] the window updates itself and keeps its name

- [ ] **Do:** open `Screepub Desktop.app` and let it update (Settings, or
  the update label). **See:** it installs and restarts. **Fail:** an error,
  or it stays on the old version.
- [ ] Known and accepted, all four once: it asks the "check for updates
  automatically?" question again; macOS asks again for drive access; Finder
  still calls it "Screepub Desktop"; and you now have two copies of the
  same app. **Do:** delete `Screepub Desktop.app`. **See:** `Screepub.app`
  still opens and still updates.

### [hands] Homebrew

- [ ] **Do:** `brew info --cask <owner>/tap/screepub`. **See:** it says
  the cask is deprecated. `brew install --formula <owner>/tap/screepub`
  (the command-line formula; the cask shares its name, hence `--formula`)
  still
  installs the new version.

---

## QA for 0.7.3

Written 2026-09-24, from `origin/main` at 9e79a29, for the release after
0.7.2.

Two people share this page.

- **Claude** runs every item marked **[claude]**: tests, the engine from the
  command line, and the window's own page in a browser. All of it runs
  against scratch folders, never your real library or settings.
- **The owner** runs every item marked **[hands]**: the real window, a real
  Kindle, Apple Books, Mail, Calibre, and the update from 0.7.2.

The order: Claude's part first (it builds the app the owner tests), then the owner's
pass in one sitting, then the update from 0.7.2 last, once 0.7.3 is
published.

Each item has three lines: **Do**, **See**, and **Fail**. Tick the box when
it passes. When one fails, note the words on screen and carry on.

### What changed since 0.7.2

- **Updates.** One question under the drop area asks whether to check once
  a day. A found update shows as "Update to 0.7.3" beside the version, in
  place of the old dot. Installing restarts the window by itself, once
  running work has finished, any open file dialog has closed, and any
  settings change has been saved. The switch in the release notes shows its
  real state.
- **Moving the window.** Drag it by the strip along the top, or by the gaps
  in the tab bar.
- **Send.** Every way out in one list: readers over USB, Apple Books,
  Amazon's Send to Kindle, email to your Kindle, Save the EPUB, Save a
  Kindle file. The one you used last is marked in brass and remembered.
- **Best Kindle quality.** A checklist on the Send page with Get and
  Install buttons. It only shows when this computer cannot make KFX yet.
- **Where books go.** Change the folder from the Convert page, or reset
  it. Show in Finder follows it.
- **Settings page.** "Use these for new scripts", and a way back to
  Screepub's own defaults. A book keeps the settings it was built with.
- **Keep or follow.** "When a PDF is converted: Keep its settings / Follow
  the defaults" on the Settings page.
- **Taking turns.** A settings change made while a send, a Kindle file or a
  conversion is using the same book waits for it and says so ("Waiting for
  the copy to finish…"). Opening Apple Books, Send to Kindle or Mail never
  makes it wait.
- **Safer copies to a reader.** The copy goes to a hidden file first and is
  renamed into place, so a failed copy leaves the old book.
- **Read.** The scene list no longer puts page numbers in front of scene
  names.
- **Tidier.** Kindle Previewer's temp folders are removed after each KFX
  build.
- **Fixed.** Side by side dual dialogue can be chosen again after a
  Sequential default.
- **Command line.** New: `routes`, `route`, `kfx-status`, `kfx-install`,
  `app-settings`, `reveal`.

### Before you start

- **Claude builds the candidate first** (Part 1). The owner's pass runs on that
  build, opened from where Claude put it. Leave 0.7.2 in `/Applications`:
  Part 3 updates it.
- **The candidate may still say rev 0.7.2** until the release is cut. That
  is expected.
- **Have ready:** your Kindle and its cable; `tests/fixtures/field-station.pdf`
  from the repo (the invented demo script); one of your own screenplay
  PDFs; your iPhone or iPad if you want to watch Books sync; a reMarkable
  only if you have one.
- **Two steps use Terminal** (Calibre, and the one-time question). The
  commands are written out for you.
- **What your pass changes, and how it goes back:** the library folder (you
  press Reset), the remembered way out (harmless), your Calibre (protected
  by a scratch folder), your default mail app (only if you choose to switch
  it, and you switch it back), and the window's update memory (moved
  aside, not deleted).

---

### Part 1: Claude's checks (no hands)

#### Tests and the candidate build

- [x] **[claude] The full test suite** (passed 2026-09-24)
  - Do: `bun test` from the repo root.
  - See: 0 fail. On 2026-09-24 at 3bc54b4, in a worktree without the
    private scripts: 2672 pass, 11 skip, 0 fail.
  - Fail: any failure, or a test that read or wrote the real settings file.
- [x] **[claude] Types** (passed 2026-09-24)
  - Do: `bunx tsc --noEmit`.
  - See: no output.
  - Fail: any error.
- [x] **[claude] Build the candidate window** (passed 2026-09-24)
  - Do: `bun tools/build-sidecar.ts --host`, then in `desktop/src-tauri`:
    `cargo tauri build --bundles app --config tauri.transition.conf.json`.
    Give the owner the path to `Screepub Desktop.app`. It lands at
    `desktop/src-tauri/target/release/bundle/macos/Screepub Desktop.app`
    inside the worktree it was built in. For this pass Claude built it at
    `/Users/CWP_MBP_SGS2/Documents/CODING_PROJECTS/Projects/02_Darkwell/Screepub/.claude/worktrees/qa-run/desktop/src-tauri/target/release/bundle/macos/Screepub Desktop.app`.
  - See: it builds; the engine inside answers `--version --json`; `git
    status` is clean afterwards.
  - Fail: the build fails, or it rewrote a tracked file.

#### Updates

- [x] **[claude] The one-time question** (passed 2026-09-24)
  - Do: run `tests/desktop-ui.test.ts`, then open the window's own page in
    headless Chrome with a stand-in updater and empty storage.
  - See: "Check for new versions once a day?" with Turn on and No thanks
    under the drop area. Either button removes it for good. No update
    request goes out before an answer. With no updater, or off a Mac, it
    never shows.
  - Fail: it comes back after an answer, shows where updates cannot
    happen, or a request goes out first.
- [x] **[claude] The label beside the version** (passed 2026-09-24)
  - Do: same page, with the stand-in offering a newer version; step it
    through a download.
  - See: "Update to X", then "Downloading X… 40%" (no percent when the
    server sends no size), "Installing…", "Restarting after this
    finishes…" while engine work runs, then "Restarting…". A failure reads
    "Update failed. Try again" and can be clicked. No brass dot. The offer
    survives a relaunch the same day. A check that finds nothing newer
    takes the label down.
  - Fail: other words, a label that looks clickable mid-download, or the
    dot.
- [x] **[claude] The restart waits for work** (passed 2026-09-24)
  - Do: the waiting tests in `tests/desktop-ui.test.ts`.
  - See: the restart fires only once the engine has been quiet for half a
    second. The background checks (the Send page's route list and KFX
    status, the Convert page's library line, and Show in Finder) do not
    hold it off. A settings write does, and so do an open file dialog and
    a Settings knob still settling.
  - Fail: any of those the other way round.
- [x] **[claude] The release-notes switch shows its real state** (passed
  2026-09-24)
  - Do: on the stand-in page, press Turn on under the drop area, then open
    the release notes.
  - See: the "once a day" box is ticked, without a relaunch. Untick it,
    close, reopen: still unticked.
  - Fail: the box disagrees with the answer.
- [x] **[claude] What the window is allowed to do** (passed 2026-09-24)
  - Do: `bun test tests/desktop-shell.test.ts`.
  - See: restart and window dragging granted; quitting itself not granted;
    folder picker and Save box granted; the old fixed-folder reveal
    permission gone; the three KFX download pages are the only new links.
  - Fail: any other grant.

#### Where books go, and what new scripts start from

- [x] **[claude] The library folder rules** (passed 2026-09-24)
  - Do: with the settings file in a scratch folder, run `screepub
    app-settings --json`, then `--set` a good folder, a relative one, one
    under `/System`, and `{"libraryPath": null}`; then once more with
    `SCREEPUB_LIBRARY` set.
  - See: the default is `~/Documents/Screepub`. A chosen folder wins over
    it. `SCREEPUB_LIBRARY` wins over both and is reported as such. A bad
    folder is refused with a sentence and nothing is stored. `null` goes
    back to the default.
  - Fail: a bad folder is stored, or the wrong folder is used.
- [x] **[claude] The library line on the Convert page** (passed 2026-09-24)
  - Do: stand-in page, three cases: the default, a chosen folder, and
    `SCREEPUB_LIBRARY` set.
  - See: "Books are saved in ~/Documents/Screepub." with Change…. A chosen
    folder adds Reset. The variable case ends "set by SCREEPUB_LIBRARY."
    and has no buttons. A refused folder shows the engine's sentence on a
    second line and keeps the buttons.
  - Fail: wrong words, or buttons that vanish after a refusal.
- [x] **[claude] Defaults for new scripts** (passed 2026-09-24)
  - Do: stand-in page, a converted script's Settings page: move one knob
    (Dialogue margins, say), wait for "Saved.", then press "Use these for
    new scripts", then "Reset new scripts to Screepub's defaults". Check
    the same through `screepub app-settings`.
  - See: the foot first says "New scripts start from Screepub's own
    defaults." After Use: the note "New scripts will start from these
    settings.", the caption "New scripts start from your own defaults.",
    and the Reset button. Use on an untouched script shows the note but no
    Reset, because nothing differs. Reset puts it back and hides itself.
    Who wins, highest first: a one-off flag, the script's own settings,
    your defaults, Screepub's defaults.
  - Fail: wrong caption, Reset offered when nothing differs, or a
    different order.
- [x] **[claude] A book keeps what it was built with** (passed 2026-09-24)
  - Do: convert Field Station into a scratch library, then change the
    defaults for new scripts.
  - See: `field-station.screepub.json` appears with the first conversion.
    After the change, `screepub settings` for that script answers as
    before. A one-off `--options` is not saved into it. A `.fountain`
    input saves nothing.
  - Fail: the old book follows the new defaults, or a one-off flag sticks.
- [x] **[claude] Keep its settings / Follow the defaults** (passed
  2026-09-24)
  - Do: on the Settings page, under "When a PDF is converted", choose Keep
    its settings, convert a new PDF, change the defaults for new scripts;
    then again with Follow the defaults.
  - See: Keep: that book stays as it was built. Follow: it takes the new
    defaults. The choice is still there after a relaunch. A script
    following the defaults gets settings of its own the first time one of
    its knobs moves.
  - Fail: both choices behave the same, or the choice is forgotten.
- [x] **[claude] Side by side can win again** (passed 2026-09-24)
  - Do: make Sequential the dual dialogue default, then set one script to
    side by side: `screepub settings <script>.fountain --set
    '{"dualDialogue":"sideBySide"}'`.
  - See: that script answers side by side.
  - Fail: it stays Sequential (the bug fixed 2026-09-23).

#### Send routes

- [x] **[claude] The list on this Mac** (passed 2026-09-24)
  - Do: `screepub routes <book>.epub --json`, settings in a scratch folder.
  - See: Apple Books, Send to Kindle web, Save the EPUB, Save a Kindle
    file; then dimmed: Kindle, Kobo, tolino ("plug in over USB to send"),
    reMarkable ("dock over USB to send"), and Send to Kindle email ("needs
    Apple Mail as the default mail app…"). Chosen: Apple Books, when
    nothing is remembered. Seen exactly so on 2026-09-24.
  - Fail: another order, or a dimmed row with no fix.
- [x] **[claude] Saving a copy, and the memory** (passed 2026-09-24)
  - Do: `screepub route save-epub <book>.epub --out <scratch>/x.epub`, then
    again with a relative path, and with a `.mobi` name.
  - See: the first writes the file, answers "Saved to …", and the scratch
    settings file now says `"lastRoute": "save-epub"`. The other two are
    refused before anything is written.
  - Fail: a refused save writes anything, or a failed route is remembered.
- [x] **[claude] Refusals come before anything opens** (passed 2026-09-24)
  - Do: `bun test tests/cli-routes.test.ts tests/routes.test.ts
    tests/route-facts.test.ts tests/send-routes-ui.test.ts`.
  - See: all pass. A reader key, an unknown key, a save with no `--out`,
    and the Amazon setup page given a book are all refused before
    anything opens. A remembered route that can never work here falls
    back to the first one that can.
  - Fail: any failure.
- [x] **[claude] The Send page draws what the engine says** (passed
  2026-09-24)
  - Do: stand-in page, fed a routes answer; change which one is chosen.
  - See: rows keep the engine's order. Only the chosen row's button is
    brass in the list. The KFX block's Install button is brass too, when
    that block shows. Dimmed rows show their fix and have no button. The email row
    carries "First time? Amazon needs your sender address approved…" and
    "Open Amazon's page", even when dimmed.
  - Fail: rows move, or a dimmed row gets a button.

#### Best Kindle quality (KFX)

- [x] **[claude] The checklist on this Mac** (passed 2026-09-24)
  - Do: `screepub kfx-status` with `CALIBRE_CONFIG_DIRECTORY` pointing at a
    copy of `~/Library/Preferences/calibre`, then at an empty scratch
    folder.
  - See: first: "Kindles get KFX, the best quality Screepub can make."
    Second: Calibre installed, Kindle Previewer installed, KFX plugin "not
    installed: run screepub kfx-install". Seen exactly so on 2026-09-24.
    The plugin counts as installed when Calibre lists "KFX Output" or a
    renamed "KFX Output (…)" copy; the companion "Set KFX metadata" plugin
    alone does not.
  - Fail: the scratch run still sees the real plugin. Then the protection
    in Part 2 does not work, and the owner should skip the install.
- [x] **[claude] The links, and the installer's refusals** (passed
  2026-09-24)
  - Do: `bun test tests/export-kfx-setup.test.ts tests/cli-kfx.test.ts`;
    fetch the three download pages.
  - See: all pass. Calibre's Mac and Windows pages and Amazon's Kindle
    Previewer page all answer. The install is refused on Linux before
    anything downloads.
  - Fail: a dead link, or a test that could reach the real installer.

#### Reading

- [x] **[claude] The scene list has no page numbers** (passed 2026-09-24)
  - Do: stand-in page, convert Field Station, open Read. Then the same on
    the private test scripts, reported as counts only.
  - See: 11 scenes, none starting with a page number (9 of the 11 did
    before 2026-09-22).
  - Fail: a scene name starting with a number its heading does not have.

#### Temp folders

- [x] **[claude] A Kindle file leaves nothing behind** (passed 2026-09-24)
  - Do: list the system temp folder, run `screepub export <book>.epub
    --for kindle` on Field Station (KFX on this Mac) with
    `CALIBRE_CONFIG_DIRECTORY` on a copy of your Calibre folder, list it
    again.
  - See: no new `screepub-kfx-…` folder and no new Kindle Previewer
    folder.
  - Fail: anything new left behind.
- [x] **[claude] Tests and tools leave nothing behind** (passed 2026-09-24)
  - Do: list the temp folder before and after the full `bun test`.
  - See: `tests/temp-hygiene.test.ts` passes, and nothing new named
    `screepub-…` is left.
  - Fail: leftovers.

#### Pictures and docs

- [x] **[claude] The pictures retake cleanly** (passed 2026-09-24)
  - Do: `bun tools/capture-screens.ts`.
  - See: all nine print `unchanged`.
  - Fail: a refused call or a changed picture.
- [x] **[claude] The docs match the window** (passed 2026-09-24)
  - Do: read README.md's network paragraph and `docs/send-to-kindle.md`
    beside the Send page.
  - See: nothing says the window has no Send to Kindle page, and every
    web page the Send page opens is listed.
  - Fail: README says the window has no Send to Kindle page, or leaves out
    a page the Send page opens.

---

### Part 2: the owner's pass (hands, one sitting)

Work top to bottom. Kindle unplugged until the Send section says so.

#### Open and convert

- [YES ] **[hands] Open the candidate**
  - Do: double-click the `Screepub Desktop.app` Claude built (not the one
    in `/Applications`).
  - See: the window, the drop area, and a version stamp at the foot, with
    no line above it saying the engine could not start.
  - Fail: no window, or a line saying the engine could not start.
- [x] **[hands] Convert Field Station** (passed 2026-09-28)
  - Do: drag `tests/fixtures/field-station.pdf` onto the window.
  - See: Field Station, 19 pages, 11 scenes, 4 speaking characters: the
    same numbers the command line gave Claude.
  - Fail: different numbers, or an error.

#### Moving the window

- [x] **[hands] Drag by the top strip** (passed 2026-09-28)
  - Do: press in the blank band above the tabs and drag.
  - See: the whole window moves with the pointer.
  - Fail: nothing moves, or text gets selected instead.
- [x] **[hands] Double-click the top strip** (passed 2026-09-28)
  - Do: double-click the same band, then double-click again.
  - See: the window zooms to fill the screen, then goes back.
  - Fail: nothing happens.
- [x] **[hands] Drag by the gaps in the tab bar** (passed 2026-09-28)
  - Do: press between two tab names, or just right of Send, and drag.
  - See: the window moves.
  - Fail: nothing moves.
- [x] **[hands] The tabs still switch** (passed 2026-09-28)
  - Do: click Convert, Read, Settings and Send in turn; then use the Left
    and Right arrow keys on the tabs.
  - See: each one opens its page.
  - Fail: a click on a tab moves the window, or does nothing.
- [x] **[hands] A scrolled page** (passed 2026-09-28)
  - Do: on Read, scroll well down, then click whatever sits near the top
    edge of the window.
  - See: the click works. The strip has scrolled away with the paper; it
    comes back when you scroll to the top.
  - Fail: a control near the top edge cannot be clicked.

#### Reading

- [x] **[hands] The scene list has room at the top** (passed on the rebuild; the owner approved the candidate)
  - Do: open Read and show the scene list, scrolled to the top.
  - See: the count ("11 SCENES" for Field Station) sits clear of the
    list's top edge, with the same room as below the last scene.
  - Fail: the count touches the top edge.

#### Where books go

- [x] **[hands] The library line** (passed 2026-09-28)
  - Do: press Convert another to get back to the drop area.
  - See: under it, "Books are saved in ~/Documents/Screepub." and
    Change….
  - Fail: no line, or a different folder.
- [x] **[hands] Change the folder** (passed 2026-09-28)
  - Do: Change…, make a new folder such as `~/Desktop/Screepub QA`, choose
    it.
  - See: the line names the new folder, a Reset button appears, and a
    second line says "New books go here. Books already converted stay
    where they are."
  - Fail: no folder picker, the line does not change, or an error.
- [x] **[hands] New books land there** (passed 2026-09-28)
  - Do: convert Field Station again.
  - See: the path under the result is inside `Screepub QA`. The earlier
    copy in `~/Documents/Screepub` is still there, untouched.
  - Fail: it lands in the old folder, or the old copy moved or vanished.
- [x] **[hands] Show in Finder** (passed 2026-09-28)
  - Do: press Show in Finder on the result.
  - See: Finder opens on the new folder with the `.epub` selected.
  - Fail: nothing opens, the wrong folder, or an error line under the
    path.
- [x] **[hands] Reset** (passed 2026-09-28)
  - Do: Convert another, then Reset.
  - See: "Books are saved in ~/Documents/Screepub." again, no Reset
    button, and no second line.
  - Fail: it still names `Screepub QA`.

#### Release notes

- [x] **[hands] The switch shows your answer** (passed 2026-09-28)
  - Do: click the version stamp at the foot.
  - See: in the release notes, "Look for a new version once a day" is
    ticked (you turned it on in 0.7.1 or 0.7.2).
  - Fail: it is unticked.

#### Send

Field Station open, Kindle still unplugged.

- [x] **[hands] The list** (passed 2026-09-28)
  - Do: open Send.
  - See: Apple Books, Send to Kindle web, Save the EPUB, Save a Kindle
    file. Below them, dimmed with their fix: Kindle, Kobo, tolino,
    reMarkable, and Send to Kindle email (dimmed while Mail is not your
    default mail app). Apple Books is brass. No "Best Kindle quality"
    block: this Mac already makes KFX.
  - Fail: rows shuffle as you watch, a dimmed row has a button, or the
    KFX block shows.
- [x] **[hands] Save the EPUB** (passed 2026-09-28)
  - Do: Save the EPUB…, save to the Desktop. Then press it again and
    cancel.
  - See: the Save box suggests `field-station.epub`; the status says
    "Saved to …/Desktop/field-station.epub."; the file opens in Books.
    The brass moves to Save the EPUB, and no row moves. Cancelling
    changes nothing and says nothing.
  - Fail: no file, a wrong name, the rows reorder, or cancel shows an
    error.
- [x] **[hands] Apple Books** (passed 2026-09-28)
  - Do: Add to Apple Books.
  - See: Books opens with Field Station in it; the status says "Added to
    Apple Books. It syncs to your iPhone and iPad when Books uses
    iCloud." Later it shows on your iPhone or iPad. Brass moves to Apple
    Books.
  - Fail: Books does not open, the book is missing, or an error line.
- [x] **[hands] Send to Kindle web** (passed 2026-09-28)
  - Do: press Send to Kindle web.
  - See: your browser opens Amazon's Send to Kindle page, and Finder shows
    the book selected so you can drag it in. The status says so.
    (Sending it through Amazon is up to you.)
  - Fail: the page or Finder does not appear, or an error.
- [x] **[hands] Amazon's settings page** (passed 2026-09-28)
  - Do: on the email row, press Open Amazon's page.
  - See: your browser opens Amazon's Personal Document Settings, where the
    Kindle's email address and the approved senders list live.
  - Fail: nothing opens, or a different page.
- [ ] **[hands] Email to your Kindle** (optional, not run)
  - Do: only if you are willing to switch for a minute: Mail > Settings >
    General > Default email reader: Mail. Back in Send, press Send to
    Kindle email.
  - See: within a couple of seconds the email row is no longer dimmed.
    Mail opens a new message with the EPUB attached; the status says to
    address it to your Kindle's email address. Switch the default back
    afterwards.
  - Fail: a message with no attachment, or the row stays dimmed.
- [x] **[hands] The Send page opens at once** (passed on the rebuild; the owner approved the candidate)
  - Do: open Send.
  - See: every row is there straight away, in well under a second. The
    reMarkable row says "checking whether one is docked…" for a moment,
    then "dock over USB to send". Buttons that hand over a Kindle file
    name its type: "Save a Kindle file (.kfx)…", and "Copy to Kindle
    (.kfx)" when your Kindle is plugged in. The Kindle's own name stays
    plain.
  - Fail: a blank page for a second or more, or a Kindle button that names
    no type on this Mac.
- [x] **[hands] The Kindle file builds by itself, and says so** (passed on the rebuild; the owner approved the candidate)
  - Do: in Settings move one knob and wait for "Saved.", then open Send.
    (It builds ahead only when Kindle is likely: your last way out was a
    Kindle or Save a Kindle file, or a Kindle is plugged in. Yours is.)
  - See: without pressing anything, "Building the Kindle file. This takes
    about half a minute." with a moving bar. Save the EPUB pressed now
    starts at once, without waiting. When the bar goes, press Save a Kindle
    file (.kfx)…: the Save box opens straight away and suggests
    `field-station.kfx`.
  - Fail: nothing shows while it builds, Save the EPUB waits for the build,
    or the Save box offers AZW3 or MOBI on this Mac.
- [x] **[hands] A knob moved during a build waits its turn** (passed on the rebuild; the owner approved the candidate)
  - Do: move a knob and wait for "Saved.", open Send, and while the bar
    moves go to Settings and move another knob.
  - See: "Waiting for the Kindle file to finish building…", then "Saved."
    once the bar is gone.
  - Fail: the knob change is lost, or it saves while the bar is still
    moving.
- [x] **[hands] Pressing during a build waits for it** (passed on the rebuild; the owner approved the candidate)
  - Do: start a build as above and, while the bar moves, press Save a
    Kindle file (.kfx)….
  - See: the building line stays up until the build ends, then the Save
    box opens. Only one build runs.
  - Fail: a second build starts, or an error.
- [x] **[hands] Kindle over USB** (passed 2026-09-28)
  - Do: plug in the Kindle and wait a moment, then press Copy to Kindle.
  - See: a Kindle row appears with its drive. Then "Building the Kindle
    file…" with a moving bar if the file is out of date, "Copying it to
    Kindle…", and a sent line that names
    the file and asks you to eject first, plus a line naming KFX. Eject:
    the book is in the Kindle's library, opens, and character names stay
    with their lines. Brass moves to the Kindle.
  - Fail: no Kindle row, an error, or the book is missing or will not
    open.
- [x] **[hands] An unplugged Kindle stays chosen** (passed 2026-09-28)
  - Do: unplug the Kindle and watch Send; then plug it back in.
  - See: unplugged, the Kindle row dims and no button is brass. Plugged
    back in, Copy to Kindle is brass again.
  - Fail: the brass jumps to another row.
- [ ] **[hands] reMarkable** (not run: no reMarkable)
  - Do: dock it over USB, press Upload to reMarkable.
  - See: a reMarkable row appears; the upload finishes; the book is on the
    tablet.
  - Fail: anything else. Either way, this is the first real reMarkable
    report, so note what happened.

#### Best Kindle quality, with your Calibre protected

Calibre keeps its plugins in its settings folder. Setting
`CALIBRE_CONFIG_DIRECTORY` points it at a different folder for that launch
only. The window hands its surroundings to the engine, and the engine hands
them to Calibre unchanged, so the install lands in the scratch folder and
your real Calibre is not touched. Checked 2026-09-24: with an empty scratch
folder, Calibre lists no KFX plugin and `screepub kfx-status` says it is
not installed, while your real Calibre still has KFX Output 2.20.1.

- [x] **[hands] Open with a scratch Calibre folder** (passed 2026-09-28)
  - Do: quit Screepub fully (Cmd-Q), then in Terminal:
    ```
    mkdir -p ~/Desktop/calibre-qa
    open --env CALIBRE_CONFIG_DIRECTORY="$HOME/Desktop/calibre-qa" "<the path Claude gave you>/Screepub Desktop.app"
    ```
    Convert Field Station and open Send.
  - See: a "Best Kindle quality" block: "Kindles get AZW3 for now. KFX
    looks better, and needs the three free tools below." Calibre:
    Installed. Kindle Previewer: Installed. KFX plugin: an Install button.
    Its Install button is brass, beside the brass on your last way out.
  - Fail: no block. Most likely Screepub was still running, so the
    setting never reached it: quit and try again.
- [x] **[hands] Install the plugin** (passed 2026-09-28)
  - Do: press Install.
  - See: the button reads Installing…; the line says "Downloading and
    installing the KFX plugin. This takes a few seconds."; then
    "Installed the KFX plugin 2.20.1. Kindles now get KFX." (the version
    may be newer).
  - Fail: an error line. Copy its words.
- [x] **[hands] The new plugin makes KFX** (passed 2026-09-28)
  - Do: convert one of your own PDFs (a book not yet built as KFX), then
    Save a Kindle file….
  - See: the Save box suggests a `.kfx` file.
  - Fail: AZW3 is offered, or the build fails.
- [x] **[hands] Back to your own Calibre** (passed 2026-09-28)
  - Do: quit, open the candidate normally (double-click), open Send.
  - See: no "Best Kindle quality" block. The brass is still on the last
    way out you used (Save a Kindle file, from the step before).
  - Fail: the block appears, which would mean your real Calibre lost its
    plugin; or the brass has gone back to Apple Books.

#### The one-time question

The window remembers your answer in its own web storage, and on this Mac
it already says you answered yes. Moving that folder aside makes it ask
again. It holds nothing else the window uses.

- [x] **[hands] See the question** (passed 2026-09-28)
  - Do: quit Screepub (Cmd-Q), then in Terminal:
    ```
    mv ~/Library/WebKit/com.darkwell.screepub.desktop/WebsiteData/Default ~/Library/WebKit/com.darkwell.screepub.desktop/WebsiteData/Default.before-qa
    ```
    Open the candidate again.
  - See: under the drop area, "Check for new versions once a day?" with
    Turn on and No thanks, above the library line. Nothing beside the
    version stamp.
  - Fail: no question.
- [x] **[hands] Answer it** (passed 2026-09-28)
  - Do: press Turn on. Open the release notes. Quit and reopen.
  - See: the question goes at once. In the release notes the "once a day"
    box is ticked. After reopening, no question.
  - Fail: the question comes back, or the box is unticked.

#### Put things back

- [x] **[hands] Tidy up** (passed 2026-09-28)
  - Do: if you switched the default mail app, switch it back. When happy,
    delete `~/Desktop/calibre-qa`, `~/Desktop/Screepub QA` and the
    `Default.before-qa` folder. Quit the candidate.
  - See: the Convert page names `~/Documents/Screepub`.
  - Fail: it still names the QA folder.

---

### Part 3: the update from 0.7.2 (superseded)

**Superseded by 0.7.4.** 0.7.3 and then 0.7.4 shipped; the update from 0.7.2
to 0.7.3 went through the window's updater on the owner's Mac (see
[the verification ledger](verification-ledger.md)). The boxes below are left
as written and are not a to-do. The last item, the automatic restart, is
the one still worth seeing on the next update.

Only once 0.7.3 is published. 0.7.2 cannot restart itself, so this update
still ends with you quitting and reopening once.

- [ ] **[claude] 0.7.3 is published** (after 0.7.3 is published)
  - Do: fetch the latest release's `latest.json`.
  - See: it names 0.7.3, and the release carries the Mac download.
  - Fail: it still names 0.7.2.
- [ ] **[hands] Open 0.7.2**
  - Do: make sure the candidate is quit, then open Screepub Desktop from
    `/Applications`.
  - See: rev 0.7.2 at the foot.
  - Fail: any other version.
- [ ] **[hands] Find 0.7.3**
  - Do: click rev 0.7.2 to open the release notes; press Check for
    updates. (A brass dot may already sit before the version: 0.7.2
    checks once a day.)
  - See: "Screepub 0.7.3 is available.", its notes, and an Install 0.7.3
    button.
  - Fail: "is the newest there is", or an error.
- [ ] **[hands] Install**
  - Do: press Install 0.7.3.
  - See: Downloading…, Installing…, then "Update installed. Quit and
    reopen Screepub to use 0.7.3." It does not restart by itself: 0.7.2
    cannot.
  - Fail: any error, including one about the signature.
- [ ] **[hands] Quit and reopen**
  - Do: Cmd-Q, then open Screepub Desktop from `/Applications`.
  - See: rev 0.7.3; "Books are saved in ~/Documents/Screepub." under the
    drop area; no question (you answered it in Part 2).
  - Fail: still rev 0.7.2.
- [ ] **[hands] Nothing newer**
  - Do: open the release notes; press Check for updates.
  - See: the "once a day" box matches your answer; "Screepub 0.7.3 is the
    newest there is."; nothing beside the version stamp.
  - Fail: an offer, a label, or an error.
- [ ] **[claude] The app on disk** (after 0.7.3 is published)
  - Do: read the installed app's version, and check its signature with
    `spctl`.
  - See: 0.7.3, accepted, notarized Developer ID.
  - Fail: anything else.
- [ ] **[hands] The automatic restart** (the release after 0.7.3)
  - Do: when the next version is out, open 0.7.3, click "Update to …"
    beside the version, and while it downloads, drop a long script on the
    window.
  - See: "Downloading … N%", "Installing…", then "Restarting after this
    finishes…" for as long as the conversion runs, then "Restarting…",
    and the window comes back by itself on the new version.
  - Fail: it restarts in the middle of the conversion, or ends on "Quit
    and reopen". This cannot be seen any sooner: 0.7.3 arrives the old
    way.

---

### Where things live on a Mac

`<script>` below is the PDF's name without `.pdf`, for example
`field-station`.

| What | Where |
| --- | --- |
| Your books | `~/Documents/Screepub/<script>/`, or the folder chosen with Change…. `SCREEPUB_LIBRARY` beats both. |
| In each script's folder | `<script>.epub` (the book), `<script>.fountain` (the text the book is built from), `<script>.screepub.json` (this script's own settings), `source.json` (which PDF it came from), and once made, the Kindle file: `<script>.kfx`, `.azw3` or `.mobi`. |
| Two PDFs with the same name | The second gets its own folder with a short code: `<script>-1a2b3c4d`. |
| App settings | `~/Library/Application Support/Screepub/settings.json`: the chosen folder (`libraryPath`), the defaults for new scripts (`formatDefaults`), and the last way out you used (`lastRoute`). Made the first time one of them is saved. `SCREEPUB_CONFIG_DIR` moves it. |
| The window's update memory | `~/Library/WebKit/com.darkwell.screepub.desktop/WebsiteData/`: whether you answered the question, your answer, the day of the last check, and a version a check found. Not in `settings.json`. |
| Copies you save | Wherever the Save box said. Screepub keeps no record of them. |
| KFX temp work | A `screepub-kfx-…` folder in the system temp folder, removed when each build ends. |
| Books from the old Swift app | Flat in `~/Documents/Screepub` (`Draft.epub`). The window leaves them alone. |

### Known limits, not failures

- Kobo, tolino and reMarkable have never run on real hardware. The Send
  page says so.
- The top strip scrolls away with the page. Scroll to the top to drag.
- Books converted before 0.7.3 have no settings of their own yet, so they
  follow the defaults for new scripts until one of their knobs is moved
  once.
- Changing the library folder moves nothing. A script converted again
  after a change starts fresh in the new folder, without its old tuning.
- Email to your Kindle needs Apple Mail as the default mail app. Any other
  mail app drops the attachment, which is why the row is dimmed.
- While a copy to a reader is in flight the reader holds both books, so a
  reader with room for only one refuses the send and keeps the old book.
- A copy killed outright can leave a hidden `.screepub-<8 hex>.partial` on
  the reader; the next send of that book writes over it.

### Found while writing this (2026-09-24)

1. **The picture tool will stop at the first window picture.** The Convert
   page now asks the engine where books go every time the drop area draws
   (`desktop/ui/convert.js:538`, called from `:583`). The capture tool's
   allow-list has no rule for that call (`tools/capture/gate.ts:51-81`;
   checked: it refuses `app-settings --json`), the stand-in marks the
   capture failed (`tools/capture/bridge.js:44`), and the run throws
   (`tools/capture/run.ts:333`). The tests stay green because none of them
   runs a real capture. Allowing the call would also add a line to the
   drop picture ("Books are saved in /Users/Shared/Documents/Screepub, set
   by SCREEPUB_LIBRARY."), so it is a picture decision as well as a fix.
   **Fixed on 2026-09-24:** the picture tool allows the settings read and
   hides the library line.
2. **README.md:257-258** still says the window "has no Send-to-Kindle page
   yet", and the paragraph it ends (from README.md:243) lists the window's
   network touchpoints without the Amazon pages the Send page now opens.
   **Fixed on 2026-09-24:** README updated.
3. **docs/send-to-kindle.md:22-24 and :44-46** describe the old Swift
   app's Settings and its Save a Copy, not the window's Send page.
   **Fixed on 2026-09-24:** docs/send-to-kindle.md updated.
4. **A restart can land on an open Save box** (from the next release on).
   The restart waits for engine work only (`desktop/ui/app.js:239`). Save
   a Kindle file builds the file, then opens the Save box
   (`desktop/ui/send.js:1002`) with no engine work running, so a restart
   waiting on the build fires half a second later, under the open box.
   The same goes for the folder picker (`desktop/ui/convert.js:504`).
   **Fixed on 2026-09-24:** a restart waits while any dialog is open.
5. **Not a bug, but it shapes the pass:** on this Mac the window's storage
   already records the question as answered (yes), so it will not appear
   until that memory is moved aside ("The one-time question", above).

### What to report back

1. Every [hands] item that failed, with the words on screen. A screenshot
   is fine.
2. Anything the window told you that was not true. That is the failure
   this project cares about most.
3. The Kindle: did the book arrive, which format did the Send page name,
   and did it open and show in the library.
4. Apple Books: did it land, and did it reach your iPhone or iPad.
5. The KFX install: the version it named, and whether the next Kindle file
   was a `.kfx`.
6. The update: did 0.7.2 find 0.7.3, and did quit and reopen land on rev
   0.7.3.
7. Anything that took much longer than the page said it would.
8. A reMarkable, Kobo or tolino, if you tried one. It is the first hardware
   report for it, either way.

---

## Earlier gates, kept as a record

Everything below is the gate 1 checklist from 2026-09-14, with what was
learned up to 2026-09-21. Those gates are passed. Nothing below is part of
the 0.7.3 pass.

This is the checklist for **gate 1** of retiring the SwiftUI app: someone
mounts the new Mac build, opens it, and converts a script. Until that
happened, the Swift app was not deleted.

Everything in the Tauri app has been exercised on Linux, where it builds,
launches and converts. As of 2026-09-14 CI has also built it on macOS for the
first time: run 34876329117 compiled the shell on `macos-15`, produced
`Screepub_0.6.0_aarch64.dmg`, opened it without installing, and ran the engine
out of it.

**That is a build, not a person.** Nobody has mounted that DMG, cleared
Gatekeeper, seen the window, or converted a script on a Mac — which is the
whole reason this page exists.

You are not looking for polish. You are answering one question: **does it
work at all on a Mac, and does it lie about anything.**

---

## Before you start

```bash
cd ~/Projects/personal/screepub   # or wherever it lives on the Mac
git pull                          # main moved a long way; this is the whole program
bun install
bun test                          # expect 1445 pass / 3 skip / 0 fail
```

If `bun test` is not green on the Mac, stop and tell me — the suite has only
ever been run on Linux, and a macOS-only failure is itself a finding.

---

## Gate 1b is PASSED as of 2026-09-14

The universal DMG was mounted, dragged to `/Applications`, launched from
there, and used to convert two real feature scripts. §2 and §3 below are
answered. What is still open on this page is §4 (check nothing landed beside
the PDF), §5 (both apps installed, and the updater), and §6 (send to a
reader) — plus gate 1c, which needs a Windows machine.

## Already answered, 2026-09-14 — do not redo these

A session on a Mac got this far, so the checklist below is shorter than it
looks. What is confirmed:

- **A local build works.** `bun tools/build-sidecar.ts --host` then, from
  `desktop/src-tauri`, `cargo run` builds and launches in ~82s (the Tauri CLI
  is NOT needed for the dev loop). `cargo tauri build --bundles app,dmg
  --config tauri.transition.conf.json` produced `Screepub Desktop.app` and a
  28 MB `Screepub Desktop_0.6.0_aarch64.dmg`.
- **It is adhoc/linker-signed, i.e. unsigned** — exactly as §1 predicts for a
  non-tagged build. Not a defect.
- **The window opens and shows `ENGINE 0.5.4`.** The sidecar-resolution
  failure §2 calls the most likely macOS-specific problem did not occur.
- **The numbers agree with the CLI.** The engine *inside the bundle* returns
  byte-identical JSON to `bun src/cli.ts` on three real scripts (only
  `epubPath`/`fountainPath` differ, by construction). And
  `desktop/ui/convert.js` reads `answer.pages`/`scenes`/`characters` straight
  off the engine JSON behind `Number.isFinite` guards, so the surface omits a
  clause rather than inventing one. **§3 is now answered too**: two real scripts were converted through the app
  on 2026-09-14, and the app's `.fountain` for one of them is byte-for-byte
  identical to a fresh CLI run over the same PDF.
- **§5's updater question is settled from source.** The Swift app's
  installer pins `identifier "com.darkwell.screepub"` exactly and the Tauri app is
  `com.darkwell.screepub.desktop`, so the refusal is structural, not
  probable. But it is a *late, repeating* refusal, and the hazard is dormant
  only because `release.yml` uploads the Swift DMG first. See
  [ADR 2026-09-14](adr/2026-09-14-swift-app-update-path.md), which changes
  what F2 should ship. Observing the refusal on the running app is still
  worth doing; predicting it is no longer necessary.

Note for whoever drives this: an unbundled `cargo run` binary claims no
bundle identifier, so screenshot tooling cannot find its window. Build the
`.app` if you need to capture it.

## 1. Get a DMG, and know whether it is signed

Look at the newest `desktop` workflow run:

```bash
gh run list --workflow=desktop.yml --limit 5
gh run view <id> --log | grep -iE 'signing|notariz|codesign|bundling'
```

**What matters now is signing, not bundling** — bundling is settled, CI did it.
The signing and notarization path was written by reading `tauri-bundler`'s
source and has still never executed, because those secrets only reach a
tagged release. So expect the push-triggered runs to produce an **unsigned**
DMG: Gatekeeper will object, and that is the expected state today, not a
defect. The signed path is first exercised by cutting `v0.6.0`.

To get an actual DMG in your hands you need a tagged release (`v0.6.0`), or
you can build one locally:

```bash
cargo install tauri-cli --version 2.11.4    # if you do not have it
bun tools/build-sidecar.ts --host
cd desktop/src-tauri
cargo tauri build --bundles app,dmg --config tauri.transition.conf.json
```

The `--config` overlay is what renames it to **Screepub Desktop** so it
cannot collide with your installed `Screepub.app`. **Do not build without
it** during the transition.

---

## 2. Does it open

Mount the DMG, drag to Applications, open it.

- **Gatekeeper.** If it is signed and notarized, it should open with no
  warning. If it warns, note the exact wording — right-click → Open is the
  workaround, and whether that is needed tells us whether notarization
  actually worked.
- **Does a window appear at all.** On Linux the window maps with the Convert
  page rendered and `ENGINE 0.5.4` in the bottom corner. That version stamp is
  the shell proving it found and spawned its sidecar engine. **If you see the
  window but no engine version, the sidecar is not being found** — that is the
  single most likely macOS-specific failure, because the sidecar is resolved
  by a platform-specific path.

---

## 3. Convert a real script

Drop one of your own scripts on the drop well — a real one, not the test
fixture. Watch for:

- The progress bar moves and the read-out names a stage.
- The result names the title, page count, scene count and speaking characters.
- **Check those numbers.** Run the same file through the CLI and compare:
  ```bash
  bun src/cli.ts ~/path/to/script.pdf --json -o /tmp/qa.epub
  ```
  They must agree exactly. A window showing plausible invented numbers would
  pass a screenshot and fail this.

Then open **Read**. The script should keep its shape — sluglines in caps,
character cues centred over indented dialogue, transitions ranged right. The
window renders the engine's own preview, so if this looks wrong the engine and
the app disagree, which is worth knowing.

---

## 4. Where your files went, and this one matters

(From the identifier release on there is one app, the window, and it
writes one folder per script. The flat files below are what an old Swift
install left behind; nothing reads them.)

**Your books now live in `~/Documents/Screepub`, and the two apps use it
differently:**

- The SwiftUI app writes **flat**: `~/Documents/Screepub/Draft.epub`
- The new app writes **one folder per script**:
  `~/Documents/Screepub/Draft/Draft.epub`

They coexist without collision — the new app never writes into a folder it
did not create. But **the new app does not list, reuse, or inherit tuning
from books the old app left flat there.** If you have tuned settings on an
existing script, the new app will not see them; it starts from defaults.

Check: after converting, confirm nothing new appeared beside the PDF you
dropped. Everything should be in the library.

---

## 5. Both apps installed at once

**HISTORY from the identifier release on.** Everything below describes
two apps with different identifiers. Since that release the window carries
the Swift app's identifier, and the old updater is supposed to INSTALL it:
see "QA for the identifier release" at the top of this page.

**OBSERVED 2026-09-21, and it behaved.** This section used to say "nobody
has tried it" and "the refusal has only been reasoned about, never
observed". Both are now false, so here is what happened, on a real Mac
against the real published v0.6.0 release.

`Screepub.app` 0.5.4 (`com.darkwell.screepub`) and `Screepub Desktop.app`
0.6.0 (`com.darkwell.screepub.desktop`) were both in `/Applications` and
both launched. The old app had `updateOptIn = 1` already. Check for
Updates found 0.6.0, downloaded, and then showed, in the window's footer:

> the update failed signature verification and was not installed. Get it
> from the release page instead

The app stayed at rev 0.5.4. Nothing was replaced.

Three things worth keeping from that:

- **It picked the TAURI image, not the Swift one.** The spec predicted the
  opposite, on the theory that upload order put `Screepub-macOS.dmg`
  first. GitHub returns release assets ordered by NAME, and
  `Screepub-Desktop-macOS-universal.dmg` sorts first. So the refusal path
  was exercised for real rather than skipped.
- **The refusal is LATE, as designed.** `dmgRequirement` pins no
  identifier, so the whole image downloads and mounts before the app
  inside fails the pin. Expect a pause, not an instant rejection.
- **The message is legible and names a way forward.** It comes from
  exactly one branch, `UpdateInstallError.verificationFailed`, which only
  `UpdateInstaller.verify` throws. A download failure, a mount failure, a
  missing app and a version mismatch each have their own separate
  wording, so this message cannot be produced by any of them.

Still true, and still the thing to watch at v0.6.1: **if the old app ever
OFFERS to update you to a Tauri build before the identifier is taken
deliberately, stop and tell me.** `bun tools/verify-signing.ts --dmg <dmg>
--expect coexist` asks that question of a downloaded file and fails if the
answer is yes, so it can be checked without installing anything.

---

## 6. Try to send to a reader

Convert a script. The result asks what you read on: press **Kindle**. The
question is not asked again (a quiet **Different device** brings it back).

Plug in your Kindle. Its card, or the brass button, says so with a green
dot, and the brass button becomes **Copy to Kindle**, naming the file type.

A Kindle on a Mac is the one combination that has been tried on real
hardware; every other reader carries an asterisk that says it has not.

So this step is the most valuable thing you can do: press Copy to Kindle and
see what happens. Whether it works or fails, it is real hardware evidence. If
the file lands, the brass button becomes **Eject Kindle** with a line saying
to eject before you unplug. Press it, check the Kindle can be unplugged, and
that the book opens on the device in the format you expected (KFX if you have
the Calibre toolchain, AZW3 otherwise, MOBI as the floor).

---

## What to report back

Short is fine. The three things I most want to know:

1. **Did the window open, and did it show an engine version.**
2. **Did the numbers match the CLI.**
3. **Anything the app told you that was not true.** That is the failure mode
   this project cares most about, and it is the one I am least able to check
   from here.

Everything else — rough edges, wording, layout at odd window sizes — is worth
noting but is not what gate 1 is asking.

---

## Two decisions that do not need a Mac

Separately from the QA, `docs/retired-coverage.md` has ten recommendations
waiting on you. One question decides 59 of the 171 unported checks:

**Does the app ever tell a user a newer version exists?**

The new app has no updater. The old one does, and 59 assertions cover it. If
the answer is yes, it gets ported before the Swift app is deleted. If no, that is a
capability the Mac app has and the new one will not — worth deciding
deliberately rather than by omission.

The second: which of **Apple Books, Send-to-Kindle, mail, save-a-copy, and
Cancel during conversion** come back. The new app has none of them.
