# Verification ledger

Who has checked what, kept in one place so no page has to infer it from a
green checkmark. Three lists. An item moves up only when somebody does the
thing, and the README, the site (methodreader.com) and the release notes say no
more than this page does. `rust/crates/xtask/tests/site.rs` and
`rust/crates/xtask/tests/readme_and_docs.rs` hold the README and the site to it.

The window is Method Reader on every platform, and on a Mac the one app,
`Method-Reader-macOS.dmg`. From the Rust cutover on it is the native Rust window
(`rust/crates/sp-app`); every release through 0.11.1 shipped the Tauri
window that came before it, and the checks by a person below were made on
that one. The older Mac app was the Swift
app, deleted from the source at piece F3. It is no longer published, and
its own updater, where it is switched on, installs the window in its place.

**Verified on a real machine, by a person**

- **The older Mac app**, before it retired: installed from its notarized
  download, converting scripts and sending them to a Kindle, over USB and by
  email.
- **The window on a Mac, from a release.** The owner's Mac updated the window
  from 0.7.2 to 0.7.3 through its own updater: a bundle that `release.yml`
  built, signed and notarized, downloaded, checked and installed by the
  window itself.
- **The window on a Mac, by hand.** A `.dmg` built on the owner's Mac
  (universal then) was mounted, dragged to `/Applications`, opened past
  Gatekeeper and used to convert two real feature scripts; its `.fountain`
  matched a fresh command-line run byte for byte. The 0.7.3 candidate then
  passed a full
  hands-on pass ([mac-qa.md](mac-qa.md)): converting, the library folder,
  Settings, the Send page's routes, Apple Books, Send to Kindle on the web, a
  Kindle over USB, and installing the KFX plugin into a scratch Calibre. The
  0.7.4 candidate passed the items that pass rechecked. Each Mac that ran
  these is Apple silicon, the only kind the Mac download is built for since
  Intel Macs were dropped after 0.9.0.
- **A Kindle**, firmware 5.19.2, over USB (KFX, AZW3 and MOBI) and by email.
- **Apple Books** on a Mac: the book arrives in the library.
- **The window on Linux, ARM.** `Screepub_0.6.0_arm64.deb` and
  `Screepub-0.6.0-1.aarch64.rpm`, built on an aarch64 Linux machine, hold the
  engine, `LICENSE`, `THIRD-PARTY-NOTICES.md`, four icon sizes and a launcher
  entry. The engine runs out of both and converts a fixture, and the window
  runs out of the unpacked `.deb`. Nothing was installed. These are the
  packages an ARM machine makes; the Intel packages a release publishes have
  not been run by a person.

**Verified only by CI, and only as far as CI can reach**

- `rust.yml`, on every push: formatting, lints and every crate's tests on
  macOS, Windows and Linux; the release command line converting the
  fixtures on a Mac, checked with epubcheck; and every installer built
  unsigned and opened on its own system (the `.deb` installed and removed,
  the `.rpm` listed, the AppImage unpacked, the Windows installer run
  silently and uninstalled), with the Mac app's headless mode converting a
  fixture. No runner has a display, so nothing here opens the window.
  Until the Rust cutover, `ci.yml` and `desktop.yml` did this for the
  TypeScript engine and the Tauri window.
- `release.yml`, on every tag: the same bundles, built again. Since 0.6.0 it
  has built and signed the Mac window, and since 0.7.0 it has notarized it.
  Every bundle is opened and checked for the files and the version it
  claims before anything is uploaded (until the Rust cutover, its engine was
  run too; `rust.yml` runs the Mac app's headless conversion), and once a
  person publishes the draft, `release-published.yml` reads the update
  manifest (`latest.json`, Mac only) back from GitHub. The Mac `.dmg` is built for Apple silicon only, the
  runner's own kind, so all of it runs; of 0.9.0's universal `.dmg`, the
  last, only the ARM half ran.
- Weekly: the Homebrew formula and the update manifest still match the
  newest release, the retired Homebrew cask still says it is retired
  (deprecated, and disabled from the release after 0.9.0), and
  the Calibre tests (KEPUB, the KFX plugin listing, and Calibre's
  own reader opening Method Reader's AZW3) still pass against Calibre's newest
  version.

**Verified by nobody**

- Installing the `.deb`, the `.rpm` or the `.exe` on a real machine.
- The window on Windows: no runner has a display and no person has opened it.
- The Windows installer itself, run by a person, and the SmartScreen screen
  the README describes.
- The Intel SLICE of 0.9.0's universal `.dmg`, the last Mac download to
  carry one. It was built, checked as a container, signed and
  published, and never ran. Every Mac download after it is Apple silicon
  only.
- The window's launcher entry, opened from a Linux desktop menu.
- Dragging a file onto the window on Linux or Windows. On a Mac it is
  verified: the 0.7.3 pass converted a script dropped on the window.
- The window restarting itself after an update, from 0.7.3 on: written and
  tested in code, not yet recorded by a person.
- The 0.13 update behaviour on a real Mac: the quiet check two minutes
  after launch and every six hours, the update downloaded before **Update
  ready, restart to update** appears, the Help menu's switch, a previous no
  kept off, and What's New opening once after the update. Written and
  tested in code against a fake updater, not yet seen by a person.
- The move to one Mac app, by a person: the older Mac app updating itself
  into the window, and a *Screepub Desktop* install updating into Method Reader.
  Both are written and tested in code.
- A Tauri window updating itself into the Rust window on a real Mac, by a
  person. `release.yml`'s handover gate proves it headless, with 0.11.1's
  own updater, before a release goes public.
- The Linux AppImage, new with the Rust window, run by a person.
- Kobo, tolino and reMarkable on real hardware. They are tested against
  injected mounts and a stub tablet.
- Any e-reader connected on Linux or Windows.
