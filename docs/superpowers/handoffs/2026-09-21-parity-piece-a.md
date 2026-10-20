# Handoff: the parity work, piece A in flight

Written 2026-09-21 at `8d61bd9`, for whichever session picks this up.
The prompt to paste is at the bottom. Everything above it is the context
that prompt assumes.

## Where things stand

**v0.6.0 shipped and is verified end to end.** Signing executed for the
first time, the frozen Swift updater found the Tauri DMG, downloaded it,
and refused it on the identifier pin, observed on a real Mac. Nobody's
install was touched. That was the whole point of the release.

**v0.6.1 is blocked on one thing: the Tauri app cannot update itself.**
[The doors ADR](../../adr/2026-09-21-doors-not-commands.md) says why that
is a precondition rather than a preference, and that the handover waits
for it. [The parity plan](../plans/2026-09-21-parity.md) splits the work
into four pieces; A is the updater and it is the critical path.

**Piece A's judgement half is done and reachable.** Per
[the updater spec](../specs/2026-09-21-updater-design.md), the Tauri
plugin does transport and the engine keeps the decision. Landed:

- `src/update/compare.ts`: the comparator, including the git-describe
  downgrade defence that semver gets wrong. 34 tests.
- `desktop/ui/update-compare.js` and its `.d.ts`: transpiled from the
  engine by `tools/build-update-compare.ts`, pinned by a test that asks
  both modules every question and compares the answers. CI regenerates
  and diffs it.
- `screepub update-decision` and `screepub update-should-check`: the
  same judgement for anything that is not the window. Both offline.

**Piece A's transport half is built, on branch `worktree-updater-transport`,
and waits on the owner for one thing: the merge.** The key pair was
generated, stored in 1Password and added as the two repo secrets on the
evening of 2026-09-21 (key id `EC5C19F83FC2D502`, the public half is in
`tauri.conf.json`). The plan that records every decision is
[`2026-09-21-updater-transport.md`](../plans/2026-09-21-updater-transport.md).
What is on the branch: the plugin registered and granted, the release-only
overlay, `build-app-bundle.ts --updater`, `build-update-manifest.ts`,
`check-latest.ts`, the release workflow's key gate, signed macOS leg,
manifest upload and `latest-check` job, the weekly manifest check, and the
docs. `tauri.conf.json` carries an EMPTY `pubkey` until the owner pastes
the public half in; `bun test` allows that, the tag-time gate does not.
The window contract went to the interface-pass session the same day.
Two things a reader of the older text below will not expect: the crate
now links `serde_json` (Tauri's code generator needs it for any `plugins`
block; no `.rs` file may name it, and the test that guarded the old rule
now guards that one), and Linux and Windows are not in the manifest yet.

**Piece B's first door landed** with the interface pass: `tauri-plugin-opener`
with one scoped grant, this repository's GitHub URLs only, so Report a
Bug works. Reveal and save-a-copy are the next two doors and are the
interface-pass session's, each waiting on the owner saying go.

## The two-session arrangement

Two Claude sessions work this repo at once and it has gone well. The
split is by file, and it has held: **the interface-pass session owns
`desktop/ui`**, and the engine side is the other session's. Per feature,
the engine side lands a contract first and the UI builds against it.

Use `ListAgents` to find the other session and `SendMessage` to talk to
it. Do not merge, grant a permission, or add a Rust dependency on the
other session's say-so; both sessions have held that line and it has
mattered twice. The owner approves those in whichever session he is
talking to, and only he can release a merge into the `main` checkout,
which is the primary working directory.

## The owner's checklist for the signing key

Only he can do this, and it is the one thing the updater cannot proceed
without.

    cargo tauri signer generate -w ~/.tauri/screepub.key

That writes a private key and prints a public key. Then:

1. Store the private key where the Apple certificate lives. Tauri's own
   warning: lose it and no existing install can ever be updated again.
2. Add `TAURI_SIGNING_PRIVATE_KEY` as a GitHub repo secret. If a password
   was set, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` too.
3. Hand the PUBLIC key to the session doing piece A. It is safe to share
   and it goes into `desktop/src-tauri/tauri.conf.json`.

The private key must never pass through a session transcript.

## What piece A still needs, in order

1. `tauri-plugin-updater` as a dependency, initialised in `main.rs`, with
   `bundle.createUpdaterArtifacts: true` and the `plugins.updater` block
   (`pubkey`, `endpoints`) in `tauri.conf.json`. The endpoint is a static
   `latest.json` on the GitHub release.
2. The two new secrets threaded into `release.yml`'s universal macOS leg
   as `TAURI_SIGNING_PRIVATE_KEY` and its password, so the bundler signs
   the updater artifact. Note this is a `.tar.gz` of the `.app` plus a
   `.sig`, NOT the DMG, so the one-DMG rule the handover depends on is
   not disturbed.
3. A generator for `latest.json` in `release.yml`, listing url and
   signature per `OS-ARCH` key. For a universal Mac build both
   `darwin-x86_64` and `darwin-aarch64` point at the same tarball.
4. A freshness check so `latest.json` cannot go stale the way the
   Homebrew tap once did: five releases served 0.3.0 because nothing
   compared the published file to the newest release.
5. The window side: opt-in off by default and asked once, the plugin's
   `check()` filtered through `pickUpdate` from `update-compare.js`, then
   `downloadAndInstall()`. That half is the interface-pass session's;
   send it the contract first.
6. Two README sentences go false and must change in the same commit:
   line 239's "no update check, no external links" and the Mac-only
   framing of the five network touchpoints.
7. `tools/verify-signing.ts --expect handover` is the gate for the
   artifact, and it already checks notarization. Run it on the
   DOWNLOADED asset after the next tag, never a local build.

## Things you will not get from the documents

- **Pushing to the old repos needs `gh auth switch --user <owner>` first, then switch back to the other account.** A plain push
  403s. Same for the separate `homebrew-tap` repo.
- **`tools/hooks/require-release-notes.sh` blocks any command that
  mentions a version tag unless the version is a literal.** It matched a
  heredoc that merely quoted a tagging example. Write versions out.
- **The tap does not push itself.** `release.yml`'s `tap` job only prints
  a crib sheet; `tap-check` goes red. `bash tools/bump-tap.sh <version>
  homebrew-tap`, then commit and push in that repo, and verify against
  the PUBLISHED file, never the working tree.
- **`tools/release-notes.ts` reports no verdicts, ever.** It greps a
  phrase the registry no longer uses, so an empty answer looks like "no
  news". Read `docs/formatting-options-log.md` directly.
- **The reference sweep now sees untracked files** (`8d61bd9`). Before
  that it read a bare `git ls-files` and a new file was invisible until
  committed, which caused five red mains in a week. If a green run is
  followed by a red one after `git add`, that hole has reopened.
- **The lesson from those five reds is not the bug.** The hole was
  correctly identified after the first miss, written into memory, and
  then the next two identical failures were filed under discipline and
  a peer was blamed for the same pattern. When a failure recurs, check
  whether the diagnosis already on record explains it before inventing a
  new one.
- **The Tauri DMG published by v0.6.0 is signed but not notarized at the
  container level.** Fixed in `release.yml` for the next tag, unproven
  until then. It does not affect the updater path.

## Coverage, for the ledger

`kit-check` has 264 check sites. 171 asserted behaviour that existed
nowhere else on 2026-09-21; the comparator and throttle replaced 17 of
them. `docs/retired-coverage.md` has a `Built?` column now, because for a
week "settled to port" was being read as "ported".

---

## The prompt

    Screepub: continue piece A of the parity work, the updater.

    Read these first, in order:
      docs/superpowers/handoffs/2026-09-21-parity-piece-a.md   (this file)
      docs/superpowers/specs/2026-09-21-updater-design.md      (the design)
      docs/superpowers/plans/2026-09-21-parity.md              (the four pieces)
      docs/adr/2026-09-21-doors-not-commands.md                (why A blocks v0.6.1)

    The judgement half is landed and reachable. Start on the transport
    half: the plugin dependency, the config, the release-side plumbing
    and the latest.json freshness check. The window side belongs to the
    interface-pass session; send it the contract before it needs it.

    Do not take the bundle identifier. That is v0.6.1 and it waits for
    this piece to be live.

    Context you will not get from the docs:
    - The owner generates the signing key pair and adds the secrets. If
      he has not yet, you can do everything except the final wiring.
      Never let the private key pass through the transcript.
    - Two sessions work this repo; the other owns desktop/ui. Find it
      with ListAgents. Do not merge, grant a permission or add a Rust
      dependency on its word; the owner approves those.
    - Pushing needs `gh auth switch --user <owner>`, then back.
    - Project rules: TDD, corpus diff after any stage-1 change, and the
      owner wants plain-language explanations, not dense prose.
