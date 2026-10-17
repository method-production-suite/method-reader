# Release secrets

The `release` workflow (`.github/workflows/release.yml`) needs these repo
**Actions secrets** — GitHub → repo **Settings → Secrets and variables →
Actions → New repository secret**. Instructions only; never commit the values.

The signing identity is **Clockwork Post Production LLC** (the Apple
Developer account). Until every secret below exists, the release job fails
loudly and normal CI is unaffected.

The release steps themselves, from the notes to the person who publishes
the draft, are in [RELEASING.md](RELEASING.md). A tag builds a DRAFT; the
Homebrew tap and the read-back checks run from
`.github/workflows/release-published.yml` once a person publishes it.

## 1. Developer ID Application certificate

You need a **Developer ID Application** certificate (not "Apple
Development"). If you don't have one yet: Xcode → Settings → Accounts →
your team → Manage Certificates → **+** → Developer ID Application. Then in
**Keychain Access**, find "Developer ID Application: Clockwork Post
Production LLC …", right-click → **Export** → save a `.p12` and set an
export password.

```bash
base64 -i Certificates.p12 | pbcopy    # paste as DEVELOPER_ID_CERT_P12_BASE64
```

- **`DEVELOPER_ID_CERT_P12_BASE64`** — the base64 blob copied above
- **`CERT_PASSWORD`** — the password you set when exporting the `.p12`
- **`KEYCHAIN_PASSWORD`** — any strong random string (CI's throwaway keychain);
  generate one with `openssl rand -base64 24`

## 2. App Store Connect API key (for notarization)

[App Store Connect](https://appstoreconnect.apple.com) → **Users and
Access → Integrations → App Store Connect API** → generate a key with the
**Developer** role. Download the `.p8` (offered **once** — keep it safe).

```bash
base64 -i AuthKey_XXXXXXXXXX.p8 | pbcopy    # paste as AC_API_KEY_P8_BASE64
```

- **`AC_API_KEY_P8_BASE64`** — the base64 blob copied above
- **`AC_API_KEY_ID`** — the key ID (the `XXXXXXXXXX` in the filename)
- **`AC_API_ISSUER_ID`** — the Issuer ID shown at the top of that page

## 3. `TAP_TOKEN` — OPTIONAL, and the only one whose absence is not fatal

Without it the release still succeeds; the `tap` job
(`release-published.yml`) just says so and exits clean, leaving the crib
sheet in the release run's summary and the tap to be bumped by hand. With
it, the tap bumps itself on every non-prerelease release, once a person
publishes it.

GitHub → **Settings → Developer settings → Personal access tokens →
Fine-grained tokens → Generate new token**:

- **Resource owner:** `ssandweiss`
- **Repository access:** *Only select repositories* → **`ssandweiss/homebrew-tap`**
- **Permissions:** Repository permissions → **Contents: Read and write**.
  Nothing else. That is the entire blast radius: it cannot touch this repo.
- **Expiration:** a fine-grained token cannot be non-expiring beyond a year.
  When it lapses, the `tap` job starts reporting "TAP_TOKEN is not set" and
  `tap-freshness` goes red at the next release. That is a visible,
  self-correcting failure, not a silent one.

Paste as **`TAP_TOKEN`**.

## 4. `TAURI_SIGNING_PRIVATE_KEY`: the updater's signing key

Added 2026-09-21 for piece A of the parity plan. The app updates itself
through its updater (`rust/crates/sp-update`, which reads the same
`latest.json` and the same minisign signatures the Tauri windows' updater
plugin read), and every download is checked against a minisign signature.
The release signs the updater archive with a private key only CI holds, and
the app carries the matching public key. The secret keeps its Tauri-era
name so nothing in the repository settings had to change.

**The pair already exists, and it must never change.** Its key id is
`E4D0668D54F6EB34`, and every window since 0.7.1 and the Rust app trust
only that key: **lose the private key or its password and no installed copy
of the app can ever be updated again.** Store the private key where the
Apple certificate lives.

It was generated on the owner's own machine, never in a session transcript
and never on a runner, with the Tauri CLI the repository pinned then
(tauri-cli 2.11.4; the repository no longer pins it, and `sp-release keygen`
makes throwaway keys for rehearsals and tests only):

```bash
cargo install tauri-cli --version 2.11.4 --locked
```

```bash
cargo tauri signer generate -w ~/.tauri/screepub.key
```

It asked for a password (optional; recommended), wrote the private key to
`~/.tauri/screepub.key` and the public key beside it as
`~/.tauri/screepub.key.pub`, and printed only the two paths.

The three pieces:

- **`TAURI_SIGNING_PRIVATE_KEY`**: the CONTENTS of `~/.tauri/screepub.key`
  (`pbcopy < ~/.tauri/screepub.key`), pasted as a repo secret. It is one
  line of base64 and the release tools accept it as is.
- **`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`**: the password, if you set one.
  Leave this secret out if you did not: the workflow then passes an empty
  string, which is what a password-less key expects.
- **The public key** has one home: the `PUBKEY` constant in
  `rust/crates/sp-update/src/lib.rs` (the contents of
  `~/.tauri/screepub.key.pub` as base64, one line, committed). It is safe
  to share and safe to commit. The app is built with it, and the release
  tools check against it by default.

What holds this together, in the order it runs:

- The macOS leg of `release.yml`'s `app-bundles` job refuses to START
  without the private key rather than failing after the full build, then
  signs the update archive with `sp-release sign-update` (the signature
  records the version, which the Rust app requires) and publishes
  `Screepub-macOS.app.tar.gz` and its `.sig`.
- `app-upload` writes `latest.json` from what arrived (`sp-release
  manifest`), checks every archive it names against `PUBKEY` and the
  signed version (`sp-release verify-update`), and uploads it LAST, so
  nothing it names is ever missing or unverified when it is read.
- `latest-check` (`release-published.yml`, once a person publishes the
  draft) reads the manifest back from the endpoint the app uses
  (`sp-release check-latest --deep`) and compares it with the newest
  release: the version, every platform's asset, every signature, and the
  key id against `PUBKEY`; with `--deep` it also downloads each archive and
  verifies it as the app will. `tap-freshness.yml` repeats that weekly.

Unlike `TAP_TOKEN`, absence IS fatal to a release, on purpose:
[ADR 2026-09-21](adr/2026-09-21-doors-not-commands.md) makes the updater
the precondition for v0.6.1, and a release that cannot sign an update is
the thing it forbids.

## Sanity check

**Updated 2026-07-31: a throwaway tag no longer works here.** Both the
`require-release-notes` hook and the `checks` job now require a
committed `docs/releases/<version>.md` for whatever version the tag
names, and nothing is committed for a one-off smoke-test tag like
`v0.0.2-rc`. Rather than writing throwaway notes for a throwaway
version, use `release.yml`'s `workflow_dispatch` trigger (see its
"Re-run entry point" comment) to run the SAME workflow again against a
tag that already has real notes — the most recent real release tag,
e.g. `v0.4.2`:

```bash
gh workflow run release.yml --ref v0.4.2
gh run watch --exit-status
```

Green run → the existing GitHub Release for that tag gets its assets
re-uploaded (the window's `Screepub-macOS.dmg`, Apple silicon only, from
`app-upload`, and `screepub-cli-macos-arm64.tar.gz` from the `release`
job): every upload step
is idempotent (`gh release upload --clobber`), so this rebuilds and
re-notarizes the real release rather than minting a fake one. Nothing to
delete afterward.

Then do the real acceptance test: download the DMG on a **different Mac**,
open it, drag to Applications, double-click, and convert a PDF — that's
what confirms notarization and signing are correct.

## The Homebrew tap: flipped to per-arch, DONE 2026-08-07 at v0.5.2

**Superseded in shape** at the release after 0.9.0, when
Intel Macs were dropped: the formula serves one tarball,
`screepub-cli-macos-arm64.tar.gz`, says `depends_on arch: :arm64`, and is
written whole by `cargo xtask bump-tap`. The per-arch record below is kept
for its lessons, which still hold.

**Status: done and pushed** (`ssandweiss/homebrew-tap@a2034f3`). The
section below is kept for the reasoning; the paste-ready block in it was
WRONG and the correction is recorded at the end. The tap installs
`v0.5.2` on both architectures, verified with `brew style` and
`brew audit --strict` on the formula and the cask, and with all three
SHA-256s checked against the digests GitHub reports for the release
assets rather than against the release prose that quotes them.

**Why it went stale for FIVE releases, and the real lesson:** the
per-arch flip was written and locally verified at v0.4.2, and this file
recorded it as done. It was never committed. `homebrew-tap/` is a
separate repo checked out inside this one and hidden from `git status`
by `.git/info/exclude`, which is local-only, so an uncommitted change in
it looks exactly like no change at all. Every `brew install` from v0.3.0
through v0.5.2 therefore served **0.3.0**, from a single
`screepub-macOS` asset the release workflow had long since stopped
producing.

So the failure was not "the files never got edited." They were edited,
and the edit was verified by a real `brew install`. The failure was that
nothing outside one laptop could see whether that edit had ever shipped,
and this file was written from the working tree instead of from the
remote. Check the PUBLISHED state, never the local one.

### The original note (reasoning still correct)


The release now ships the CLI as `screepub-cli-macos-{arm64,x64}.tar.gz`
(~25MB each) instead of one raw 133MB universal binary — bun embeds its
runtime per slice, so universal doubled every download for no benefit. The
DMG stays universal on purpose: a browser can't detect the visitor's CPU,
and Homebrew can.

Once a tag has produced those assets, replace the formula's single
`url`/`sha256` with:

```ruby
  on_arm do
    url "https://github.com/ssandweiss/screepub/releases/download/v#{version}/screepub-cli-macos-arm64.tar.gz"
    sha256 "ARM64_TARBALL_SHA"
  end
  on_intel do
    url "https://github.com/ssandweiss/screepub/releases/download/v#{version}/screepub-cli-macos-x64.tar.gz"
    sha256 "X64_TARBALL_SHA"
  end
```

and change `bin.install "screepub-macOS" => "screepub"` to
`bin.install "screepub"` — the binary inside the tarballs is already
named plain `screepub`. Do NOT push this before the assets exist; the
0.3.0 formula keeps working until then.

### Correction: the block above does not pass `brew style`

Two things the block got wrong, both caught by Homebrew itself:

1. **`on_arm do url ... end` is CASK syntax.** In a formula, `brew style`
   rejects `url` and `sha256` inside `on_arm`/`on_intel`
   (`FormulaAudit/ComponentsOrder`). Formulae use a plain conditional:

   ```ruby
   if Hardware::CPU.arm?
     url ".../v0.4.2/screepub-cli-macos-arm64.tar.gz"
     sha256 "..."
   else
     url ".../v0.4.2/screepub-cli-macos-x64.tar.gz"
     sha256 "..."
   end
   ```

2. **No `version` stanza, and no `#{version}` in the urls.** Homebrew
   scans the version out of the `v0.4.2` in the url path, so an explicit
   `version` audits as "redundant with version scanned from URL". But
   dropping it while keeping `#{version}` in the url is circular. So the
   tag is written LITERALLY in both urls, and a bump edits both.

See `homebrew-tap/Formula/screepub.rb` for the working form.

### Bumping the tap by hand

**From 0.13 the tap's files are `method-reader`.** The first bump after
the rename moves `Formula/screepub.rb` and `Casks/screepub.rb` to the new
names and writes `formula_renames.json` and `cask_renames.json`, which is
how Homebrew moves anyone who installed `screepub` over to `method-reader`.
`cargo xtask bump-tap` does all of that itself; `check-tap` fails if either
renames file is missing.

One SHA is needed, for the one command-line tarball: Apple silicon only
since the release after 0.9.0, so the formula has one
url and says `depends_on arch: :arm64`. The cask is retired (deprecated
at the identifier release, disabled from the release after it) and is
never bumped again. The release's "Tap bump crib sheet" step writes the
SHA into the run's summary. To get it after the fact:

```bash
gh release download vX.Y.Z --dir /tmp/tap --pattern 'method-reader-cli-macos-*.tar.gz' && shasum -a 256 /tmp/tap/*
```

Then run `cargo xtask bump-tap X.Y.Z <tap-checkout>` from `rust/`, which writes
`homebrew-tap/Formula/method-reader.rb` whole from that digest and disables the
cask. By hand: the FIRST bump after 0.9.0 starts from
its two-arch tap, so besides the url and the SHA, replace the formula's
whole if/else on the CPU with the one url and sha256, add
`depends_on arch: :arm64` on the line before `depends_on :macos`, and in
`homebrew-tap/Casks/method-reader.rb` change `deprecate!` to `disable!`, keeping
its date and its `because:` (Homebrew compares the date with the user's
local day, so it must already be past). Later bumps change the url and the SHA only.
`cargo xtask bump-tap` refuses a release that still carries
`screepub-cli-macos-x64.tar.gz`: bump the identifier release or anything
older with that tag's own copy of the tool (then `tools/bump-tap.sh`).
Verify before pushing:

```bash
brew style Formula/method-reader.rb
brew audit --strict --formula ssandweiss/tap/method-reader
```

Note `brew audit` reads the TAPPED clone at
`$(brew --repository ssandweiss/tap)`, not this checkout, so copy the
files there to test and `git checkout --` them afterward to leave that
clone pristine for `brew update`.

### Keeping it from going stale again

All three pieces LANDED 2026-08-07. Two do the work, one keeps them
honest:

- **The bumper: `release-published.yml`'s `tap` job, running `cargo xtask bump-tap`.**
  It runs when a person publishes the release (until 0.13 it was a job in
  `release.yml`, after one that published automatically), so it cannot
  start before the assets exist. It clones
  the tap, retires the cask for good (its `deprecate!` becomes `disable!`,
  once: the last Swift app it installs would offer an Intel Mac an app it
  cannot open), writes the formula whole (one url, one SHA,
  `depends_on arch: :arm64`), runs `brew style`, and commits. Checksums come from
  the digests GitHub reports per asset, so nothing is downloaded and
  nothing is re-hashed. Prereleases are skipped: a hyphen in the tag must
  never become what `brew install` hands out.

  Needs `TAP_TOKEN` (§3 above). **Without it the job does not fail the
  release** — it says so and exits clean, leaving the crib sheet and the
  manual path intact.

- **The floor: `release.yml`'s "Tap bump crib sheet" step**, writing the
  version and the formula's SHA into `$GITHUB_STEP_SUMMARY`, with what
  the first hand bump must change besides it. This is the
  fallback for a missing token or a failed bump, so the hand edit is
  paste-and-commit rather than a download-and-hash chore.

- **The alarm, in two places, both running `cargo xtask check-tap`.** It
  reads the tap's cask and formula STRAIGHT FROM GITHUB and fails when
  the formula pins something other than the newest release or carries a
  SHA that does not match that release's assets, or when the cask is not
  retired the way that release wants (deprecated while a release still
  carries the Intel tarball, disabled after). No secret needed, both repos
  being public.

  - `release-published.yml`'s **`tap-check`** job, `needs: tap`, is the
    per-release check. A red job there means the release published fine and the tap
    does not serve it yet.
  - `.github/workflows/tap-freshness.yml` is weekly plus
    `workflow_dispatch`, and catches drift BETWEEN releases: a tap edited
    by hand, a release deleted, a formula half-bumped.

  Keep the alarm even though the bumper exists. The bumper can be skipped
  for want of a token, and a bumper that silently no-ops is precisely the
  failure that hid five releases.

  **The trap that cost a version to find:** `tap-freshness.yml` originally
  also carried `on: release: [published]`, and at v0.5.3 that trigger did
  not fire even once. GitHub does not start new workflow runs from events
  raised by a workflow using the default `GITHUB_TOKEN`, and `release.yml`
  creates the release with exactly that token. The trigger read as
  coverage while providing none, which is the same shape as the tap
  problem itself. That was true while `release.yml` published the release
  itself. **Since the suite's release standard a person publishes the
  draft**, and an event a person raises does start workflows, so the tap
  and its checks moved to `release-published.yml` on that trigger. It
  holds only while no workflow publishes: `xtask/tests/release_workflow.rs`
  fails if one does.

**Why the logic lives in `cargo xtask` commands and not inline in the
YAML:** these are the steps that have already failed silently once. Inline,
they can only be tested by cutting a release. As commands they can be run
against the real repos, which is how `bump-tap` was checked for the
properties that matter: bumping to the version the tap already serves produces an
EMPTY diff, bumping a tap that is a version behind restores it
byte-for-byte, a restructured tap fails loudly instead of no-opping, and
a tag with no release fails before touching anything. That last one found
a real bug: `gh api` prints its 404 body on **stdout**, so an emptiness
check accepted `{"message":"Not Found"...}` as a checksum. Both tools
now check the SHAPE of a digest, not merely that something came back.
(They were shell scripts then, `tools/bump-tap.sh` and
`tools/check-tap.sh`; the xtask commands are their ports, with the same
checks.)
