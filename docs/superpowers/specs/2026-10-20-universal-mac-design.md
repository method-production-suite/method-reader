# Universal Mac builds from 1.0.0

Status: design, 2026-10-20. Nothing here is built yet.

The owner decided, 2026-10-19: every suite app ships a universal Mac build from its
next release. For Method Reader that is 1.0.0. 0.13.1 stays Apple silicon only.

## What changes for people

- An Intel Mac can install the current version again and update it. Until now
  0.9.0 was the last version for Intel.
- Apple silicon Macs get the same app as before. The download is bigger
  (roughly double the executable), and nothing else changes for them.
- The command-line tool is one universal file for every Mac.

## The one decision this needs: Intel copies of 0.9.0

Every installed Screepub 0.9.0 on an Intel Mac checks
`ssandweiss/screepub/releases/latest/download/latest.json` (its
`tauri.conf.json` at v0.9.0), and GitHub forwards that address to this repo's
latest release. Today that file has no `darwin-x86_64` entry, so those copies are
told that Intel is retired (`is_intel_mac_retired`). Once 1.0.0's file lists
`darwin-x86_64`, they will be offered 1.0.0 directly.

0.9.0 to 1.0.0 is a jump no Apple silicon copy has made. The Apple silicon path
went through the identifier release, then the move to the Rust app at 0.12.0, and
each step has a proof in the handover gate (`handover-proof` for the 0.11.1 Tauri
updater, `update-proof-0.13` for 0.13.0's). Nothing proves that 0.9.0's updater
can install a 1.0.0 bundle, or that the bundle starts cleanly on 0.9.0's
leftovers (its settings folders, its WebKit data, the old bundle name).

Options:

1. **Offer it, proven (recommended).** Add a third handover-gate step: 0.9.0's
   own updater (built from the v0.9.0 tag's lock, like `update-proof-0.13`)
   installs this release from a local `latest.json` on an Intel runner, or on an
   Apple silicon runner under Rosetta. Then check that the installed app starts
   headless, reads 0.9.0's settings, and renames itself (`OLD_BUNDLE_NAMES`
   already covers `Screepub.app` and `Screepub Desktop.app`). Intel users come
   back onto the update path with no action from them.
2. **Don't offer it to 0.9.0.** Only possible if the new updater asks for a key
   0.9.0's never asks for (for example `darwin-x86_64-method`). That needs a
   change in method-libs, which is frozen until 2026-11-02 except for security,
   and it strands every Intel 0.9.0 copy on 0.9.0 for good.
3. **Hold the Intel key back for one release.** Ship 1.0.0 universal but list
   only `darwin-aarch64`, and add `darwin-x86_64` in 1.0.1 once option 1's proof
   exists. Intel users can install 1.0.0 by hand from the download page, and they
   would get updates from 1.0.1.

The recommendation is 1: it is the only option that brings existing Intel users
back without asking anything of them. If the 0.9.0 proof turns out to be hard
to build, option 3 lets 1.0.0 ship on time.

## The build (release.yml)

All on the existing `macos-15` runner. Both targets are built before any secret
enters the job, as now.

- `rustup target add x86_64-apple-darwin`, then build `sp-cli` and `sp-app` for
  `aarch64-apple-darwin` and for `x86_64-apple-darwin`.
- `lipo -create` each pair into one universal binary: the window, `method-reader`
  and the `screepub` alias.
- The arch check stays exact. `lipo -archs "$B"` must print exactly
  `x86_64 arm64`, and anything else fails the job. This replaces today's
  `= arm64` checks at the CLI (line 199) and window (line 401) steps.
- Signing, notarising and stapling are unchanged. They already go inside out over
  `Contents/MacOS/*`, and a universal Mach-O signs as one file.
- Before signing, run each binary's `--version` once under
  `arch -x86_64` (the runner has Rosetta) and once natively, so a broken
  slice fails the build and not a user's install.
- The weekly full build (`full.yml`) stays Apple silicon only, so the weekly
  runs don't cost more.

## The update file (sp-release)

- `UPDATER_KINDS` in `names.rs`: the one Mac archive answers
  `darwin-aarch64` and `darwin-x86_64` (and the `-app` variants the old updater
  tries first, if option 1 is chosen).
- `check_latest.rs` currently refuses an Intel key that points at the
  Apple silicon archive, because that archive was arm64 only. That rule is right
  for 0.10 to 0.13 and wrong from 1.0.0. It changes to: an Intel key must point
  at an archive whose release carries the universal CLI. Its tests keep the old
  case (an arm64-only release with an Intel key is still refused).
- `manifest.rs`, `xtask` `build-update-manifest`, `rehearse.rs` and their tests:
  `darwin-aarch64` alone becomes both keys.
- The method-update pin moves to method-libs `25c80f8` (method-libs#7, which
  lets an Intel Mac ask for `darwin-x86_64`), written as the full 40-character
  SHA. It is in the workspace `Cargo.toml`, line 58.

## The window

- `is_intel_mac_retired` and its message go. An Intel copy of 1.0.0 finds its
  key, so the message can only appear for a copy whose manifest has lost the key,
  which is a release bug, and the ordinary "couldn't check" message covers it.
- `host.rs`'s update `target` (`darwin-aarch64`, line 769) comes from the
  running architecture.

## The command-line tool

Today's asset is `method-reader-cli-macos-arm64.tar.gz`. Options:

1. **A new name, `method-reader-cli-macos.tar.gz` (recommended)**, and stop
   making the arm64 one. The name says what it is, and nothing installs the old
   name automatically (the Homebrew tap is retired). The README, the site and the
   release-notes template name the file, and this updates all three.
2. Keep the old name for the universal file. That's misleading, and it stays
   misleading for good.

## Proof before the tag

In the release PR's "Tested locally", on this Mac on power:

- `cargo test --workspace --locked --target x86_64-apple-darwin` (under Rosetta);
- `arch -x86_64 <binary> --version` for the window and both CLI names;
- one headless launch of the x86_64 window (`--capture-screens` or the
  control interface), with no window on screen.

Then the rehearsal on main, which runs both handover proofs and, under option 1,
the 0.9.0 one.

## Public text

Each of these goes in the 1.0.0 release PR, not before. The site follows
the latest published release.

- README "Downloads": Apple silicon and Intel, macOS 13 or later; drop the
  "On an Intel Mac, 0.9.0 is the last version" paragraph.
- The site's download line, through MOrg.
- `docs/releases/1.0.0.md`: one line for Intel users. If option 1 is chosen, it
  says 0.9.0 updates itself. Under option 3, it says to download 1.0.0 once.

## Also in 1.0.0, separate PRs

- `SHA256SUMS.minisig`: SHA256SUMS signed with the update key (MOrg,
  2026-10-19).
- The MCP server (#62), the device library's engine half (#67) and the FDX
  scene pages, which wait for the 0.13.1 tag.
- Launcher 0.1.2, which accepts universal bundles, is already published.
