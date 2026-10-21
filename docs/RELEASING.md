# Releasing Method Reader

A release is a version tag, `vX.Y.Z`, on `main`. Pushing it starts
`.github/workflows/release.yml`, which checks the tagged commit, builds and
signs every download, and leaves a DRAFT release on GitHub. Nothing is
public until the draft has been checked and published. The owner's one yes
covers both the tag and the publish (2026-10-19): the session that asked
checks the draft, reports to MOrg and publishes it, unless the owner has
said stop, and a failed check stops it there. Publishing starts
`.github/workflows/release-published.yml`, which reads the published
release back.

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

It takes the Method suite's four shots in light and dark: the Preview page,
the before and after, the drop well with the result beside it, and the
command line converting Field Station. Each is written to `assets/screens/`
for the README and to `site/img/` for the site.

Every picture must match the build being released. Commit the changed ones
with the release.

## 4. The release PR, then the tag

The notes, the version bump and any changed pictures go to main as one
pull request, opened ready with its "Tested locally" note, and reviewed by
MReview before merge (a release is on its pre-merge list). Then the
rehearsal runs on the merge commit (below), and once it is green and the
owner has said yes, the tag goes on that commit:

```bash
git fetch origin
git tag -a v0.13.1 <merge commit> -m "Method Reader 0.13.1"
git push origin v0.13.1
```

The tag is made under the neutral identity, like every public commit.

## Before the tag: the rehearsal

The release commit is on main now, so the rehearsal runs there first. It is the only full build before a release (2026-10-19); the weekly
build is not run by hand as well. It makes no tag and no release.

1. A rehearsal of the release itself: Actions, **release**, Run workflow, on
   `main`, with `rehearse_version` set to the workspace version in
   `rust/Cargo.toml`. Or from a terminal:

   ```bash
   gh workflow run release.yml --ref main -f rehearse_version=0.13.1
   ```

   It runs every check, build, signature, notarization and staple, every
   bundle check and the old window's update into the new build. It skips
   every step that would write to a release: the draft, the uploads, the
   checksums, `latest.json` and the draft check. A rehearsal honours the
   crate-age list's age-out entries, as main does; the real tag run does
   not. It runs only from main, since it uses the real signing keys.
2. Delete the rehearsal's artifacts as soon as it is green. They are signed
   builds that are not a release, and GitHub would otherwise keep them for
   a day (`RUN` is the rehearsal's run id):

   ```bash
   for id in $(gh api repos/method-production-suite/method-reader/actions/runs/RUN/artifacts --jq '.artifacts[].id'); do gh api -X DELETE repos/method-production-suite/method-reader/actions/artifacts/$id; done
   ```

Ask for the tag only when the rehearsal is green, naming the version, the
commit and the green run.

**A pushed tag is permanent.** People download from it and the workflow has
built from it. Never move, delete or force-push a public tag; fix a problem
with the next patch version.

## 5. Wait for the draft

The release run takes 15 to 25 minutes. Its `checksums` job reads every
download back from the draft and writes ONE `SHA256SUMS` over them (the
suite standard: bare names, sorted, never listing itself), signs it with the
update key as `SHA256SUMS.minisig` (from 1.0.0), and removes the
`SHA256SUMS-app` an older run left. A rehearsal never reaches the `checksums` job,
so the first real signing of `SHA256SUMS` is 1.0.0's tag: watch that job
on that run (the rehearsal's probe signature in the Mac step only proves
the key). Its last job, `draft-check`, reads
the draft back and fails loudly, naming what is missing, unless the draft
carries every file: exactly one `Method-Reader-macOS.dmg`, the update
archive and its signature, `latest.json` (not for a prerelease), every
other installer, every command-line archive, `SHA256SUMS` and `SHA256SUMS.minisig`, and no
`SHA256SUMS-app`. A red `draft-check` means
do not publish: re-run the failed jobs from the run's page
(`gh run rerun <id> --failed`), or run the whole workflow again against the
tag (`gh workflow run release.yml --ref v0.13.1`); every upload replaces
what is there.

## 6. Check the draft, then publish it

The session that asked for the tag checks the draft from the command line,
with nothing opened on screen:

- the Mac image: `gh release download v0.13.1 --pattern Method-Reader-macOS.dmg`,
  then `hdiutil attach -nobrowse -readonly`, `spctl --assess --type execute -vv`
  on the app inside (Gatekeeper must accept it), `codesign --verify --deep
  --strict`, `xcrun stapler validate` on the image and the app, and
  `hdiutil detach`; and from 1.0.0, `lipo -archs` on the app's
  `Contents/MacOS/method-reader-app` says `x86_64 arm64`;
- the Mac command line: `codesign --verify --strict` on both binaries in
  `method-reader-cli-macos.tar.gz`, and `lipo -archs` on each says `x86_64 arm64`;
- every download against `SHA256SUMS`, and every size against the run's own
  artifacts (none empty);
- `SHA256SUMS.minisig` (from 1.0.0): `sp-release verify-file --file SHA256SUMS`
  from `rust/`, or `minisign -Vm SHA256SUMS -P <the public key>`;
- `latest.json`: `sp-release verify-update` against the update archive and
  its signature.

Report the result to MOrg, then publish, unless the owner has said stop:

```bash
gh release edit v0.13.1 --draft=false
```

Publish with a person's token (the suite's own gh account), never a
workflow's: GitHub starts no workflow from a release a workflow's own token
published, and step 7 has to run. A failed check stops here: fix it, or
re-run the failed jobs, and check again.

## 7. The published read-back

Publishing runs `release-published.yml`: `latest-check` reads the update
manifest back from the address every installed copy reads. Prereleases are
skipped. A red job there means the release is public and updates cannot use
it yet. `manifest-freshness.yml` repeats the check weekly.

There is no package-manager step: the Homebrew tap was retired on
2026-10-18, and people install from the release's own downloads.

[release-secrets.md](release-secrets.md) has every secret.

## 8. The website and the README

If the release changes Method Reader's status or its platforms, update the
platform table in the README, and ask MOrg (the suite's organisation
session) to change `status` and `platforms` for Method Reader in the
website's `apps.toml`, or open that pull request in the suite's website
repository.
