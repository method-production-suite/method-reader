# Releasing Method Reader

A release is a version tag, `vX.Y.Z`, on `main`. Pushing it starts
`.github/workflows/release.yml`, which checks the tagged commit, builds and
signs every download, and leaves a DRAFT release on GitHub. Nothing is
public until a person has checked the Mac image and published the draft.
Publishing starts `.github/workflows/release-published.yml`, which bumps the
Homebrew tap and reads the published release back.

Every Method app releases the same way. The suite's checklist is
[RELEASE-CHECKLIST.md](https://github.com/method-production-suite/.github/blob/main/RELEASE-CHECKLIST.md)
in the organisation's `.github` repository (the link goes live when that
repository is published). Tick it for every release; this page adds Method
Reader's own commands, in the order they run.

Every `cargo xtask` command runs from `rust/`. `0.13.1` stands for the new
version throughout.

## 1. The notes

Write `docs/releases/0.13.1.md` from
[release-notes-template.md](release-notes-template.md). The facts it is
drafted from:

```bash
cargo xtask release-notes v0.13.0     # the previous release's tag
```

The notes come first because the version bump reads them: the window's
What's New is made from that file.

## 2. The version

```bash
cargo xtask bump-version 0.13.1 --check   # can the window show these notes?
cargo xtask bump-version 0.13.1           # rust/Cargo.toml, Cargo.lock, the window's notes
```

The version lives in one place, `[workspace.package]` in `rust/Cargo.toml`.
The release's `checks` job refuses a tag that disagrees with it.

## 3. The pictures

Retake every README and site picture from the invented Field Station
script, then look at each one that changed, old beside new:

```bash
cargo xtask capture-screens                             # needs Google Chrome (macOS)
cargo xtask review-screens --out /tmp/screens.html      # then open the page
```

It takes the Method suite's four shots in light and dark: the Read page,
the before and after, the drop well with the result beside it, and the
command line converting Field Station. Each is written to `assets/screens/`
for the README and to `site/img/` for the site.

Every picture must match the build being released. Commit the changed ones
with the release.

## 4. Checks, commit, tag, push

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
git add rust/Cargo.toml rust/Cargo.lock rust/crates/sp-app/src/model/notes_release.rs \
  docs/releases/0.13.1.md assets/screens site/img
git commit -m "Method Reader 0.13.1"
git tag -a v0.13.1 -m "Method Reader 0.13.1"
git push --atomic origin main v0.13.1
```

Commit and tag in two commands: the `require-release-notes` hook refuses a
tag whose notes are not committed yet. `--atomic` pushes `main` and the tag
together or not at all.

**A pushed tag is permanent.** People download from it and the workflow has
built from it. Never move, delete or force-push a public tag; fix a problem
with the next patch version.

## 5. Wait for the draft

The release run takes 15 to 25 minutes. Its last job, `draft-check`, reads
the draft back and fails loudly, naming what is missing, unless the draft
carries every file: exactly one `Method-Reader-macOS.dmg`, the update
archive and its signature, `latest.json` (not for a prerelease), every
other installer and every command-line archive. A red `draft-check` means
do not publish: re-run the failed jobs from the run's page
(`gh run rerun <id> --failed`), or run the whole workflow again against the
tag (`gh workflow run release.yml --ref v0.13.1`); every upload replaces
what is there.

## 6. A person checks the draft and publishes it

On a Mac with Apple silicon: download `Method-Reader-macOS.dmg` from the
draft, open it, drag Method Reader to Applications and open it. It must
open without a warning (signed, notarized, stapled). Open the sample script
and convert it once. Then publish the draft on the repository's Releases
page.

Only a person publishes. No workflow does it, and that is also why the next
step runs: GitHub starts no workflow from a release that a workflow's own
token published.

## 7. The Homebrew tap

Publishing runs `release-published.yml`: `tap` bumps
`ssandweiss/homebrew-tap` (with the `TAP_TOKEN` secret), `tap-check` reads
the tap back from GitHub, and `latest-check` reads the update manifest back
from the address every installed copy reads. Prereleases are skipped. A red
job there means the release is public and that part does not serve it yet.

To do the tap by hand (no `TAP_TOKEN`, or a failed bump; the release run's
summary carries a crib sheet):

```bash
cargo xtask bump-tap 0.13.1 /path/to/homebrew-tap-checkout   # then commit and push there
cargo xtask check-tap                                       # does the tap serve the newest release
```

[release-secrets.md](release-secrets.md) has every secret and the tap's
history.

## 8. The website and the README

If the release changes Method Reader's status or its platforms, update the
platform table in the README, and ask MOrg (the suite's organisation
session) to change `status` and `platforms` for Method Reader in the
website's `apps.toml`, or open that pull request in the suite's website
repository.
