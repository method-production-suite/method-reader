# Plan: Screepub in pure Rust

## Status (2026-10-12): done on branch `rust/final`, waiting on the owner

**Done.** All of Screepub is the Cargo workspace in `rust/`: the engine
crates, the `screepub` CLI, the egui window (AccessKit for screen readers),
the updater, the release tools, the device-request Worker and the
developer tools (`cargo xtask`). The TypeScript engine, Bun, package.json,
the Tauri window and desktop/ui are deleted. `rust/final` is the cutover
(`rust/cutover`) with every branch the port left open merged into it: the
whole-port review's fixes (`rust-port`), the xtask, capture, docs, notices,
release-skill and repo-checks branches. The one JavaScript runtime call
left is `cargo xtask rehearse-handover prepare`, which builds the old
v0.11.1 window from its tag so the handover can be rehearsed.

**Verified, on the owner's Mac (Apple silicon), 2026-10-12:**

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D
  warnings` and `cargo test --workspace`: clean, 3,576 tests in 218 test
  binaries, none failed.
- `cargo xtask parity`: the release CLI against the TypeScript's frozen
  answers, clean: 7 committed fixtures in 494 runs and 7 real scripts in
  root fixtures/ in 189 runs (rust/oracle-real, counts only), all identical.
- A release build of the window: it did not compile until the
  integration (egui's `Style::debug` exists only in debug builds), which
  CI's bundle job would have hit; fixed, and `cargo clippy --release -p
  sp-app` is clean too.
- Every row of rust/PARITY.md is done or retired, but two that say why:
  cli-mtp-send's route rows (the MTP route is off until hardware day) and
  W12 (no sidecar exists to be missing).
- The release-built window's headless `--smoke` converts
  tests/fixtures/field-station.pdf with scratch settings and library.
- `cargo deny check`: advisories, licenses and sources clean against
  rust/deny.toml.

**Not verified here, because only CI or the owner can:**

- The first CI run of rust.yml and release.yml on this branch: the Linux
  apt lines (libasound2-dev, libfontconfig1-dev), the .deb and .rpm
  dependency read-backs, the new deny job, and Windows and Linux test runs.
- A signed, notarized release, and the updater handover on a real 0.11.x
  install.

**Owner actions left:**

1. Review `rust/final`, push it, and watch the first rust.yml run; if every
   Linux job passes, `libgtk-3-dev` can come off the apt lines (nothing on
   Linux links GTK; PARITY.md, Decisions).
2. Copy docs/superpowers/plans/2026-10-11-release-skill-after-rust.md over
   .claude/skills/release/SKILL.md (a worktree cannot edit the shared
   skill), deleting its header comment.
3. Run `cargo xtask rehearse-handover prepare`, `prove`, `run` and `verify`
   on the Mac (needs bun 1.3.14 and the Tauri CLI 2.11.4 for the tagged
   window) to watch a 0.11.1 window update itself into the Rust app.
4. Merge to main and cut the release with /release (the version bump,
   notes, pictures and the tag); the tag's release.yml signs, notarizes and
   publishes, and the Homebrew tap is bumped with `cargo xtask bump-tap`.
5. Device passes: the AZW3 and FDX on a Kindle (registry #20), the
   AppImage, .deb, .rpm and Windows installer run by a person, so the
   README's and site's status column can move up.
6. Decide the open owner questions in PARITY.md's Decisions: rustls on
   ring or the pure-Rust provider; the follow-ups O3 (an uncapped inflate in
   hayro-syntax) and O4 (one slow JPEG 2000 picture can outrun the picture
   budget); deploying the device-request Worker.

Owner's ask (2026-10-11): a version of Screepub that is 100% Rust, no other
code, converted completely, giving the same results on the control scripts,
with 1:1 feature parity. Maximize parallel work.

## Decisions (made in the owner's absence, his words "sure, let's do it")

- **Screens are native Rust**, not HTML in a webview. Candidates are iced
  and egui. Slint is out because its markup is a language of its own. The
  UI spike picks one, and **screen-reader support (AccessKit or equivalent)
  is a hard requirement**: the current window is keyboard- and
  screen-reader-usable and parity includes that.
- **A separate branch, `rust-port`, switches over in ONE release** once the
  definition of done below holds. TypeScript releases keep shipping from
  `main` until then.
- **Stays non-Rust because it is not app code:** the website (`site/`,
  HTML), CI workflow YAML, docs, and the outside tools other people make
  (Calibre, the KFX plugin, Kindle Previewer).
- Speed is not the goal (a 99-page script converts in 0.24 s today). The
  gains are one program, a smaller download, and no JavaScript runtime.

## Architecture: `rust/` Cargo workspace

| Crate | Ports | Depends on |
|---|---|---|
| `sp-model` | src/parser/types.ts, src/options.ts, fountain-js token shape | none |
| `sp-pdf` | what pdf.js gives the parser: per-page text items (str, transform, width, height, font, hasEOL), font style facts, painted images with their pixels | pure-Rust PDF stack (spike decides: hayro family, lopdf, pdf-rs) |
| `sp-parser` | src/parser/* | sp-model, sp-pdf |
| `sp-fountain` | src/fountain/* (serialize, notes, slug, cover) plus a fountain-js-compatible tokenizer | sp-model |
| `sp-epub` | src/epub/* | sp-fountain |
| `sp-mobi` | src/mobi/* | sp-fountain |
| `sp-azw3` | src/azw3/* | sp-mobi (palmdb) |
| `sp-fdx` | src/fdx/* | sp-fountain |
| `sp-fsutil` | atomic-write.ts, replace-file.ts, Node's path functions | none |
| `sp-device` | src/device/*, eject, reveal, the MTP route and macOS camera-import pause (off until hardware day) | sp-fsutil, sp-mtp's JSON |
| `sp-engine` | convert, library, library-images, settings, update/compare, export, routes | the above |
| `sp-cli` | src/cli*.ts: binary `screepub`, the same flags and the same `--json` contract | sp-engine |
| `sp-app` | desktop/ (window, every screen, updater, device watching, page show, sounds) | sp-engine directly, no sidecar |
| `sp-mtp` | native/mtp-helper from branch `mtp-kindle` (binary `screepub-mtp`) | mtp-rs |
| `xtask` | tools/*.ts the project needs (fixtures stay Python-generated until ported; capture-screens, review-screens, bump-version, release-notes, smoke) | |

Every crate is `unsafe_code = "forbid"` unless a crate documents why not.

## How parity is proved (the TypeScript engine is the answer key)

1. **Oracle dumps** (`tools/oracle/dump.ts`, transitional TypeScript, deleted
   at cutover): for any PDF or .fountain, write JSON of every stage: pdf.js
   text items per page, parser elements, the .fountain, fountain-js tokens,
   and the EPUB/MOBI/AZW3/FDX bytes. Committed fixture dumps live in
   `rust/oracle/` and are regenerated by one command.
2. **Stage parity tests** in each crate compare against those dumps, so a
   crate can be finished before the crates upstream of it exist.
3. **The parity harness** (`cargo xtask parity`) runs the TypeScript CLI and
   the Rust CLI on every committed fixture AND every real script in root
   `/fixtures/`, for every command, and diffs: the `--json` answers (paths and
   times normalized), the .fountain byte for byte, the EPUB entry by entry,
   and MOBI/AZW3/FDX byte for byte. Real-script results are reported as counts
   only, never titles or names.
4. **Behavior tests are ported**: every `tests/*.ts` file maps to a Rust test
   file (tracked in `rust/PARITY.md`), so the 3,900 checks keep guarding the
   Rust code after the TypeScript is gone.

## Definition of done

- `cargo xtask parity` is clean on all committed fixtures and all real
  scripts, for every CLI command and flag in today's `--help`.
- Every row in `rust/PARITY.md` (TS test file -> Rust tests; window feature
  -> sp-app screen) is ported and green.
- sp-app reproduces every screen and flow in desktop/ui, keyboard and screen
  reader included, checked against a feature inventory the UI wave writes
  first.
- The release pipeline builds, signs, notarizes and publishes the Rust app
  and CLI for Apple-silicon Mac, Windows x64 and Linux x64; the updater
  handover is proved (a 0.11.x window updates itself into the Rust app with
  the same identifier, settings and library).
- TypeScript, Bun, package.json and desktop/ui are deleted on the branch.

## Waves

Wave 1 (parallel, starts now):
- **Foundation:** sp-model, the oracle dump tool and committed dumps, the
  xtask parity harness skeleton, the CI job for `rust/`, `rust/PARITY.md`.
- **PDF spike, then sp-pdf:** reproduce pdf.js text items and images in pure
  Rust, measured against the oracle dumps on every fixture and real script.
  The riskiest piece; it starts first.
- **UI spike:** pick the toolkit (accessibility is the deciding test), write
  the feature inventory of desktop/ui, and build the shell, theme and fonts.

Wave 2 (after Foundation lands; parallel):
- sp-fountain, sp-epub, sp-mobi + sp-azw3, sp-fdx, sp-parser (against the
  oracle's text-item dumps, so it does not wait for sp-pdf), sp-engine
  (settings, library, device, export, update), sp-app screens.

Wave 3: sp-cli assembly and full parity runs; sp-app wired to sp-engine;
packaging, signing, updater handover, Homebrew; xtask tools; sp-mtp merged.

Wave 4: parity sweep to zero, independent reviews, docs (README,
CONTRIBUTING, CLAUDE.md, developers.md), delete the TypeScript, cut over.

## Rules for every helper

- Work in your own worktree on a branch cut from `rust-port`, and only in
  your crate's folder plus the files your brief names. Merge conflicts in
  `rust/Cargo.lock` are expected and resolved by regenerating it.
- TDD against the oracle. Never weaken a parity check to make it pass; a
  difference is either a bug or a documented, owner-visible decision in
  `rust/PARITY.md`.
- Never run Calibre, calibre-debug, ebook-convert, calibre-customize or
  Kindle Previewer outside a test that isolates `CALIBRE_CONFIG_DIRECTORY`
  in a scratch copy, not even `--version` (it rewrites the owner's
  settings). Calibre's version is in its Info.plist.
- Never touch the real app settings or library; never let a real script's
  title, author or character name reach code, tests, docs or commits.
- No push, merge to main, tag or release.
