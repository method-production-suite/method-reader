# Release secrets

The `release` workflow (`.github/workflows/release.yml`) needs these repo
**Actions secrets** — GitHub → repo **Settings → Secrets and variables →
Actions → New repository secret**. Instructions only; never commit the values.

The signing identity is the company on the Apple Developer account (team
`XSRB3D643J`). Until every secret below exists, the release job fails
loudly and normal CI is unaffected.

The release steps themselves, from the notes to the person who publishes
the draft, are in [RELEASING.md](RELEASING.md). A tag builds a DRAFT; the
read-back check runs from `.github/workflows/release-published.yml` once a
person publishes it.

## 1. Developer ID Application certificate

You need a **Developer ID Application** certificate (not "Apple
Development"). If you don't have one yet: Xcode → Settings → Accounts →
your team → Manage Certificates → **+** → Developer ID Application. Then in
**Keychain Access**, find "Developer ID Application: <the account's
company> (XSRB3D643J)", right-click → **Export** → save a `.p12` and set
an export password.

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

## 3. Retired

This section described the token that pushed the package-manager tap,
retired on 2026-10-18 along with the tap. The numbers of the sections
around it are kept so links to them still work.

## 4. `TAURI_SIGNING_PRIVATE_KEY`: the updater's signing key

Added 2026-09-21 for piece A of the parity plan. The app updates itself
through its updater (the suite's `method-update`, set up by
`rust/crates/sp-update-config`, which reads the same
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
cargo tauri signer generate -w ~/.tauri/method-reader-updater.key
```

It asked for a password (optional; recommended), wrote the private key and
the public key beside it, and printed only the two paths. On the release
Mac they now live at `~/.tauri/method-reader-updater.key` and
`~/.tauri/method-reader-updater.key.pub` (mode 600; renamed from
`screepub.key` on 2026-10-18, contents unchanged, key ID E4D0668D54F6EB34),
and the pair is kept in the team's password manager as "Method Reader -
Update signing key".

The three pieces:

- **`TAURI_SIGNING_PRIVATE_KEY`**: the CONTENTS of `~/.tauri/method-reader-updater.key`
  (`pbcopy < ~/.tauri/method-reader-updater.key`), pasted as a repo secret. It is one
  line of base64 and the release tools accept it as is.
- **`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`**: the password, if you set one.
  Leave this secret out if you did not: the workflow then passes an empty
  string, which is what a password-less key expects.
- **The public key** has one home: the `PUBKEY` constant in
  `rust/crates/sp-update-config/src/lib.rs` (the contents of
  `~/.tauri/method-reader-updater.key.pub` as base64, one line, committed). It is safe
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
  verifies it as the app will. `manifest-freshness.yml` repeats that weekly.

Its absence is fatal to a release, on purpose:
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
