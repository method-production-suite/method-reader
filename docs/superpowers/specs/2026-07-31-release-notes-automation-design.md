# Release notes: draft, review, publish — design (v2)

Date: 2026-07-31
Status: revised after a five-persona review; awaiting the owner's approval
Branch: release-notes-automation (off main)

v1 of this spec was reviewed by a release engineer, a DX designer, a
security reviewer, a technical writer and a YAGNI skeptic. Four findings
changed the design materially and are called out inline as **[v1 was
wrong]**. Everything each reviewer proved with measurements has been
re-verified here before adoption.

## Goal

Every release ships notes a screenwriter can read, in a fixed voice,
reviewed by the owner before anything publishes, carried into the GitHub release
automatically. Make the failure states unreachable, not merely detected.

Today `release.yml` hardcodes a five-line body. Four releases shipped that
way.

## What changed from v1, in order of severity

### 1. The `.claude/` narrowing is CUT entirely **[v1 was wrong]**

v1 proposed tracking `.claude/skills/` and `.claude/settings.json`.

Two independent problems, both verified:

- `.gitignore:14` is `.claude/` with a trailing slash. Git does not descend
  into an excluded directory, so a `!.claude/skills/` re-include never
  matches. The natural fix fails silently, and the natural response to that
  (`git add -f`) overrides every remaining deny.
- `.claude/worktrees/` currently holds FOUR complete repo checkouts. The
  `/fixtures/` rule is root-anchored on purpose (so `tests/fixtures/` stays
  committed), so it does NOT protect
  `.claude/worktrees/<name>/fixtures/<real script>.pdf`. Verified with
  `git check-ignore -v`: that path is ignored **solely** by the `.claude/`
  line this spec proposed to narrow. This is the same shape as the
  near-miss caught at publication.

**Decision: do not touch `.gitignore` in this work.** The `/release` skill
lives at `.claude/skills/release/` untracked. Everything durable and
portable (template, tool, hook body, CI checks) is committed elsewhere.
The only loss is that the skill file is not versioned; the template it
reads is, and it carries the judgment. If versioning the skill is wanted
later it is its own reviewed change, using the `.claude/*` form with an
explicit `.claude/worktrees/` deny after the negations, plus
`git check-ignore` evidence for all four paths.

### 2. The tag can point at an orphaned commit **[v1 was wrong]**

v1's central claim was "the tag cannot exist without the notes because one
command makes both." That proves notes exist at the tagged commit. It does
NOT prove the tagged commit is the commit being released.

Defeat sequence, two keystrokes a solo maintainer types daily: `/release`
commits and tags at A; the owner fixes a typo with `git commit --amend`, making
B; he pushes. The tag points at A, orphaned and not on main. All four v1
layers pass. The build ships from A.

**Fix, three parts:** the printed push command is
`git push --atomic origin main v<version>` (named refs, never `--tags`,
atomic so main and tag land together or not at all); the `checks` job
asserts the tagged commit is an ancestor of `origin/main`; the hook also
blocks `git commit --amend` and `git reset` when HEAD carries a `v*` tag.

**Amended 2026-07-31: the third part was never built, on purpose.** The
hook does not special-case `git commit --amend` or `git reset` on a
tagged commit. That trade-off was accepted rather than fixed, because
the `checks` job's ancestor assertion (part two, above) already catches
this UNIVERSALLY -- an amend or reset that orphans a tagged commit fails
the same way whether it happened through this hook's blind spot, a tag
typed straight into a terminal, or a tag created on another machine
entirely. Adding a local, Claude-Code-only special case for one of many
ways to orphan a commit would have been redundant with a check that
already covers all of them.

### 3. The honesty section must DIFF the registry, not grep it **[v1 was wrong]**

v1 said: registry entries "whose body still contains `Device verdict:
pending`". Two reviewers independently measured the same result, which I
re-verified:

| ref | pending entries |
|---|---|
| `v0.4.2` | #5a, #16, #8c, #10b |
| `HEAD` | #16, #17, #8c, #10b |

The approved 0.5.0 draft cites exactly one (#17). Three of v1's four were
already pending before this cycle and would reappear in every release
forever, training the reader to skip the section. Meanwhile the entry that
actually *changed* (#5a, pending → `BINDS`) is the source of the draft's
best paragraph, and v1's rule cannot see it.

**Fix:** the tool emits `verdictsOpenedInRange` and
`verdictsResolvedInRange` by parsing the registry at both refs. Opened
feeds "Good to know". Resolved feeds the improvements and the closer.
Entries pending at both refs feed neither. Headline unit test: over
`v0.4.2..HEAD`, assert #17 opened, #5a resolved, and #16/#8c/#10b in
neither.

### 4. `optionChanges` is the wrong change signal **[v1 was wrong]**

Over the 0.5.0 range `src/options.ts` gained exactly one key, which
produced one of six bullets. The other five came from behaviors with no
option key at all. The registry, which CLAUDE.md already requires updating
whenever a formatting behavior changes, captured five of six.

**Fix:** `registryChanges` (entries added or materially edited between
refs, grouped by `### N.` heading) becomes the primary signal.
`optionChanges` stays as a secondary hint that a bullet needs a
default-and-why sentence.

### 5. The empty-release path contradicted the spec's own rejection **[v1 was wrong]**

v1 rejects auto-generated placeholder notes in bold, then instructs Claude
to "offer a one-line maintenance note" when nothing is user-visible. That
is the placeholder, re-entering with Claude's endorsement.

**Fix:** hard stop. Claude reports the range is not user-visible and writes
nothing. If the owner is releasing anyway (a rebuild, a signing fix), they state
the reason and the note is written from their words.

## Deliverables

### 1. `docs/release-notes-template.md`

The technical writer's template, adopted substantially as written. Its
load-bearing parts, each measured against the approved 0.5.0 draft:

- **Caps, with 0.5.0's actuals as calibration:** 350 words total (0.5.0:
  325), one-sentence lede under 25 words (12), two change headings and six
  bullets total (2 and 6), three caveats max (2), optional closer under 70
  words (48).
- **The cut ratio, stated:** 37 commits produced 6 bullets. The default
  disposition for a commit is NO bullet. Entire merged branches produce
  nothing.
- **Slots, not fixed headings [v1 was wrong].** v1 specified New /
  Improved / Fixed / Good to know. The approved draft uses none of those:
  it leads with a theme sentence, groups new-and-improved under a heading
  named for the reader's outcome ("Your scripts break better across
  pages"), names the fix heading for who was affected ("Fixed for older
  Kindles"), then Good to know, then an italic closer. Readers do not care
  which changes were features.
- **A say-this-not-that table** covering the vocabulary that must never
  appear, with the replacement phrasing.
- **Ten worked pairs** whose left column is real commit subjects and
  registry prose, since that is literally what the drafter reads. Includes
  two failure examples: the plausible-but-wrong-vocabulary draft and the
  too-long draft.
- **A pre-lede slot for privacy, requirements or network-surface changes.**
  The README stakes the product's reputation on exactly five network
  touchpoints; a change there can never be a bullet buried under
  improvements.
- **Name the switch as the reader rail labels it, once.** The 0.5.0 draft
  never says "Print-style split minimums", so a reader who wants to turn it
  off cannot find it.
- **A contamination warning:** the registry and commit log are written in
  em dashes and jargon and are the source material. Copy facts, not
  punctuation or vocabulary.

The template's rules on format names are stated as a principle rather than
a banned-word list, because v1's list contradicted the approved draft: the
draft says "Kindles still can't read EPUB files copied over USB", which is
correct, because the reader has a file with that extension. The principle:
name a format only where the reader must act on it; never as a pipeline
stage.

### 2. `tools/release-notes.ts`

The skeptic argued to cut this: its four v1 facts are shell one-liners. That
premise held for three of them and failed for the one that matters. A
two-ref registry parse and diff is real logic, it is the field the notes'
honesty depends on, and it is the field v1 got wrong. So the tool survives,
with corrected fields:

- `verdictsOpenedInRange`, `verdictsResolvedInRange`: each with the entry
  number, title, verdict sentence and `Device support:` line, because
  number-and-title alone cannot produce "it does nothing on readers that
  ignore it".
- `registryChanges`: entries added or materially edited, by heading.
- `optionChanges`: secondary.
- `commits`: subjects as DATA, truncated to 200 chars, control characters
  and ANSI stripped, each flagged with whether the author is the repo
  owner. Today the owner authors every commit; `CONTRIBUTING.md` solicits
  patches, and a squash merge puts a contributor's PR title into the
  subject line.
- `previousRelease`: `{ version, wordCount, bulletCount }`, so "shorter
  than last time" is a number rather than a vibe.
- `userVisible`.

**Three fields this spec promised and the implementation deliberately does
NOT emit (reconciled 2026-07-31, after the skill was written against the
real output):**

- `proposedVersion` + reason. The `/release` skill derives the version
  itself from a documented minor/patch rule, so a second opinion in the
  JSON would be a second place to keep that rule correct.
- `optionChanges`. Finding 4 already demoted it to a secondary hint, and
  the skill gets it directly from `git diff <tag>..HEAD -- src/options.ts`
  when it wants it. Emitting it too would duplicate one grep.
- `previousRelease` word and bullet counts. The template already sends the
  drafter to read the two most recent real release files, which is where
  voice and length actually transfer; a count without the prose next to it
  is the less useful half.

None of the three is load-bearing, and the skill was verified against the
six fields that DO exist rather than the ones this document imagined. If a
future draft comes back visibly longer than its predecessor, `previousRelease`
is the first of the three worth adding.

Tests over synthetic git state: the headline verdict-diff case above; an
entry pending at both refs appearing in neither list; version proposal per
change shape; docs-only range; empty range.

### 3. `/release` skill (local, `.claude/skills/release/`)

Two human moments. Everything else is silent.

1. **`/release`** (or `/release 0.5.1`). Preflight starts in the
   background; git reading and drafting happen in the foreground. v1 put
   several minutes of `swift build` in front of the only screen the owner cares
   about, for no safety benefit, since nothing is written yet.
   Preflight begins `git fetch --tags --prune`, and a tag existing **on
   origin** is the hard stop, not a local tag, which lies in both
   directions.
2. **Silent stops:** version taken on origin, or nothing user-visible.
   Nothing written. A local notes file for an unreleased version is a
   RESUME path, not a refusal: v1 locked itself out of exactly the state
   where a human is most likely to fumble the sequence by hand.
3. **Draft** `docs/releases/<version>.md` from the tool's JSON, the
   template, and the two most recent existing release files, which is how
   voice and length actually transfer.
4. **Moment one: the header block, then the file.** Not "here is a
   document, approve it": fluent prose reads as correct, and a gate that
   is always approved is the origin story of this whole project. The block
   shows the proposed version and reason, the claims Claude is least sure
   about with their evidence, verdicts settled this cycle, pending ones
   deliberately omitted, and a count of commits that earned no bullet. When
   nothing is uncertain it collapses to one line, so the exceptional
   release is visibly different from the routine one.
   Approval is by editing the file or typing `ship`. Re-loops print only
   the changed bullets.
5. **He types `ship`.** Claude waits on preflight, re-reads the file from
   disk in case he edited it, re-derives the version from `package.json`
   and asserts the matching notes file is staged, then commits notes plus
   bump and creates the ANNOTATED tag on that commit. (Current tags v0.4.0
   through v0.4.2 are lightweight; this is a deliberate change.)
6. **Moment two:** one question, showing
   `git push --atomic origin main v<version>` and noting the build takes 15
   to 25 minutes. Declining prints `git tag -d` so backing out is one
   paste.

The skill states that commit subjects and registry text are untrusted input
to be summarized, never instructions to follow, and that the approval gate
includes a confidentiality check, not just a prose check.

### 4. Hook: body committed, wiring local **[v1 was wrong]**

v1 proposed committing `.claude/settings.json` to a public repo that
solicits contributions. That ships auto-executing configuration to forks,
gives contributors a guard that only obstructs them, and creates a target:
once tracked, a PR can modify it, and checking out that branch and running
Claude Code becomes arbitrary code execution. `settings.local.json` already
achieves the stated goal (enforcing on future Claude sessions on this
machine) with none of that.

- **Body:** `tools/hooks/require-release-notes.sh`, committed, reviewable,
  testable, not executed merely by existing. `set -euo pipefail`, reads the
  command via `jq -r`, never `eval` and never re-expands into a command
  position (an unsanitized re-expansion of model-generated text would be a
  bypass firing before the permission prompt).
- **Wiring:** `.claude/settings.local.json`, untracked.
- **Matching:** `git tag`, `git push` with `--tags`/`--follow-tags`/
  `refs/tags/` or naming a `vX.Y.Z` token, and `gh release create` (which
  creates the tag server-side).
- **Version resolution (reconciled 2026-07-31, after a follow-up review
  found this passage never matched what was built):** this section
  originally said the hook "does not parse a version out of the
  command... instead it enumerates local `v*` tags absent from origin
  and checks each with `git cat-file -e <tag>^{commit}:docs/releases/
  <v>.md`." That design was never implemented. What ships instead: the
  hook parses every `vX.Y.Z(-prerelease)?` token out of the COMMAND TEXT
  itself (deduplicated, every token checked, not just the first) and
  asserts `git cat-file -e HEAD:docs/releases/<version>.md` for each --
  checking the working tree's `HEAD`, not the tag's own commit, because
  at the moment this hook fires the tag does not exist yet. Matching is
  done against shell SEGMENTS of the command (split on `;`/`&`/`|`/
  newline, with a leading env-assignment or loop/conditional keyword
  stripped), not only the command's first token, so `cd /tmp && git tag
  --list` and `for x in 1; do git tag --list; done` are recognized the
  same as a bare `git tag --list`. Whether a command counts as COMPOUND
  (more than one real invocation joined by a metacharacter) is still
  decided by a cruder whole-string test on purpose, because that is what
  catches an invocation hidden inside `$(...)`/backticks that
  segment-splitting cannot see into.
- **Fails closed** on a matched-but-unresolvable command; exits 0
  immediately on non-matching Bash so unrelated commands are not hostage to
  whether `jq` is installed.
- **Recovery is guaranteed for the forms it recognizes** (reconciled
  2026-07-31): `git tag -d`/`--delete`, `git push --delete`/`-d`, and
  pushing a tag deletion (`:refs/tags/...`), matched per invocation so a
  compound command made ENTIRELY of recognized deletes also passes (the
  standard `git tag -d v1 && git push origin :refs/tags/v1` undo). This
  section originally said "anything containing `tag -d` or `:refs/tags/`
  is explicitly allowed," full stop; the shipped guarantee is narrower
  and says so: an unrecognized spelling of a delete fails closed like
  anything else the hook cannot resolve, and a delete outnumbering a real
  create in the same compound command does not exempt the create.
- Documented in plain words as an ergonomics guard, not a security
  boundary. Anyone can tag from a terminal.

### 5. `release.yml`

**A `checks` step at position 1, before `setup-bun` and Rosetta**,
validating at the tagged commit: the version matches
`^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.]+)?$` (which also converts the
`workflow_dispatch`-against-a-branch case from a confusing
`docs/releases/main.md` error into an immediate clear one); the notes file
exists via `git cat-file`; its tree mode is `100644`, not a symlink, which
would otherwise be followed and published; size is non-zero and under
60,000 bytes (GitHub's body cap near 125,000 would otherwise fail AFTER
notarization); valid UTF-8; no embedded checksum, since the workflow
appends the real one and two conflicting SHAs on one page both look
official. Plus `package.json`'s version equals the tag, which today is
guarded at `app/release.sh:18-23` but only fires minutes in with
certificates already on disk. Plus the ancestor check from finding 2.

**Publish step made idempotent and atomic:**
`gh release view || gh release create --draft`, then
`gh release upload --clobber`, then `gh release edit --draft=false`.
This fixes two things v1 missed. `gh release create` fails on an existing
release, so v1's cheap-recovery story (`workflow_dispatch`) was the one
path that did not compose. And publishing before streaming three assets
means `GET /releases/latest` briefly returns a release with no DMG:
`UpdateCheck.latest` throws `noDownloadableAsset`, `checkIfDue` has already
stamped `lastChecked` and swallows the throw, so a user who checks in that
window is told "no update" for another 24 hours. Drafts are excluded from
`/releases/latest` entirely.

**Body composition:** `cp` the file and append; content reaches `printf`
only as an argument (`printf '%s\n' "$X"`), never as a format string, since
"100%" in release prose would silently corrupt the public page.
`${{ github.ref_name }}` stays in `env:`, never inline in `run:`. Append
SHA-256 for all three assets after a blank line, not just the DMG: the
tarballs are the ones consumed by scripts and the ones with no notarization
story.

### 6. `ci.yml`

**Unconditional, not diff-based [v1 was wrong].** v1 said "if the version
changed, notes must exist", which does not fire when the version did not
change, and needs history that `actions/checkout`'s depth-1 clone does not
have. The stronger and simpler invariant: `docs/releases/<package.json
version>.md` must exist and be non-empty on every push. No history needed.
Never `|| true`.

Cost: main currently says 0.4.2 with no notes file. Seed a three-line
`docs/releases/0.4.2.md` stub saying notes begin at 0.5.0. That is not the
backfilling this spec rejects; it is one stub for the current version,
bought in exchange for an invariant with no exceptions.

### 7. `docs/releases/0.5.0.md`

From the approved draft. Delete `docs/release-notes-0.5.0-draft.md`.

## Testing

- `tools/release-notes.ts`: the cases listed in deliverable 2.
- `tools/hooks/require-release-notes.sh`: matched-and-blocked,
  matched-but-unresolvable-blocks, non-matching-exits-0, `tag -d` allowed.
- Both guards demonstrated failing and then passing, with output recorded:
  a tag with no notes blocked by the hook, and a branch bumping the version
  without notes failing CI.
- A `bun test` over `docs/releases/*.md` failing on the absolute bans: the
  em dash character, `kepub`, `sideload`, `ragged-right`, `keep-together`,
  `rendering engine`, and any real title, author or character name. Seven
  strings, no judgment, runs without Claude. Deliberately excludes EPUB,
  MOBI and "orphan", which the approved draft uses correctly.

  **Corrected 2026-07-31, during implementation:** the "any real title,
  author or character name" clause above was NOT built as a mechanical
  ban, and should not be. Hardcoding a real script title, author or
  character name in a committed test file would BE the leak it exists to
  prevent, sitting in the one file a stranger auditing this public repo
  is most likely to open. The seven strings actually shipped are all
  generic jargon, none of them anyone's private information. The
  confidentiality check for real names is human-only, by design, at the
  `/release` skill's approval gate (spec deliverable 3, moment one) --
  the reviewer, not a grep, is the only thing that can tell a real name
  from a plausible-looking placeholder.

## Out of scope

- **The Homebrew tap, which is BROKEN and is being tracked separately.**
  `homebrew-tap/Formula/screepub.rb` installs `screepub-macOS`, an asset
  the build no longer produces, and points at v0.3.0. It has been skipped
  at three consecutive releases, and `docs/release-secrets.md:62-87`
  contains the written TODO that was skipped each time. The directory is
  hidden from `git status` by `.git/info/exclude`, which is local-only and
  is why nobody sees it. This is a real user-facing break and deserves its
  own fix; it is not release-notes work.
- Website link and in-app notes list. The committed path keeps both cheap;
  the updater already carries `releaseNotesURL` and would need `body` added
  to its `GitHubRelease` decode.
- Backfilling 0.1.0 through 0.4.2 beyond the one stub above.
- Versioning the `/release` skill (see finding 1).

## Note for the publication checklist

`v0.2.0` resolves to `033ca99`, the root commit of main. The tree there is
clean now. Recorded because it is the same structural shape as the original
near-miss: tags survive history rewrites and keep old trees browsable. If
history is ever rewritten again, re-check every tag, not only the branch.
`BORN-SECURE.md` is the natural home for that line.
