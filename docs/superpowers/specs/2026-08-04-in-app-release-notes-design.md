# In-app release notes: design

Date: 2026-08-04
Status: approved by the owner in brainstorming; awaiting spec review
Branch: worktree-release-notes-in-app (off design-system-parity, because both
change `ContentView.swift` and this one builds on `Theme.inkMuted`)

## Goal

When the app says a new revision is ready, a screenwriter can read what
changed without leaving the app. Today that button opens a browser.

## What is true today

`UpdatePopover` in `ContentView.swift` shows the version, the install consent
sentence, `INSTALL AND RELAUNCH`, and `VIEW RELEASE NOTES`. That last button
calls `NSWorkspace.shared.open(update.releaseNotesURL)` and hands the reader
to GitHub. Dismissing the popover is "later".

The notes are not merely unrendered, they are never requested.
`UpdateCheck.swift` calls `/releases/latest` and its private `GitHubRelease`
decodes `tag_name`, `html_url` and `assets`. GitHub returns the notes in
`body` on that same response and the decoder does not name the field, so it
is discarded before anything could use it.

Meanwhile `release.yml` publishes `docs/releases/<version>.md` into the
release body and guards it at 60000 bytes, and `ci.yml` fails a version bump
whose notes file is missing or empty. The content is already guaranteed to
exist, in a fixed shape, on the response the app already fetches.

## Two decisions taken in brainstorming

**The notes stay a deliberate third choice, on a sheet.** The prompt keeps
its shape: version, what installing does, install, release notes. Picking the
notes opens a sheet over the main window rather than growing the popover or
opening a second window. The popover is 300pt wide, which is too narrow for
prose; a second window would carry the notes away from the decision they
exist to inform. A sheet uses the full 460pt and keeps
`INSTALL AND RELAUNCH` on the same surface as the reason to press it.

**The sheet shows every version since the installed one.** Update checks are
opt-in, so a reader can sit several revisions behind. Showing only the newest
would have hidden everything 0.5.0 changed from anyone updating from 0.4.2.
This is the decision that turns the work from one decoded field into a real
change to the update check.

## Deliverables

### 1. `app/Sources/ScreepubKit/UpdateCheck.swift`

Endpoint moves from `/releases/latest` to `/releases?per_page=30`, which
returns an array newest-first. Thirty is a bound, not a page size: a reader
more than thirty releases behind sees the newest thirty, which is preferable
to paginating for a case that will not occur.

`GitHubRelease` gains three fields:

```swift
let body: String?        // the notes, may be absent
let draft: Bool
let prerelease: Bool
```

A new public type carries one version's notes:

```swift
public struct ReleaseNote: Equatable, Sendable {
    public let version: String    // normalized, e.g. "0.5.0"
    public let markdown: String   // release body, verbatim
}
```

`AvailableUpdate` gains `notes: [ReleaseNote]`, newest first. Its existing
`version`, `downloadURL` and `releaseNotesURL` are unchanged and continue to
describe the newest release.

**Selection becomes a pure function.** This is the testability move, and the
reason the rest of the testing section is possible without a network.

`GitHubRelease` is `private` and describes GitHub's JSON, which should stay
private. `kit-check` is a separate target that imports ScreepubKit, so it can
only call `public` API, and there is no `@testable` without XCTest. Selection
therefore operates on its own public type, and `GitHubRelease` maps into it:

```swift
public struct ReleaseCandidate: Equatable, Sendable {
    public let tag: String
    public let notesURL: URL
    public let dmgURL: URL?
    public let body: String?
    public let isDraft: Bool
    public let isPrerelease: Bool
}

public static func select(
    releases: [ReleaseCandidate],
    currentVersion: String
) throws -> AvailableUpdate?
```

Rules, in order:

1. Drop any release where `isDraft` or `isPrerelease` is true. This is
   GitHub's own flag on the release, which is a separate thing from a
   semver pre-release tag inside the version string; `isNewer` handles the
   latter and is not rewritten here.
2. Keep those where the existing `isNewer(_:than:)` says the tag beats
   `currentVersion`. That helper already handles `v` prefixes and
   git-describe suffixes.
3. Sort descending using that same `isNewer` as the comparator, rather than
   trusting the array's order. One comparison rule, used everywhere.
4. If nothing survives, return nil. Unchanged behavior: no prompt.
5. The newest survivor supplies `version`, `releaseNotesURL`, and its `.dmg`
   asset supplies `downloadURL`. No `.dmg` still throws
   `noDownloadableAsset`.
6. Every survivor contributes a `ReleaseNote`. A nil or whitespace-only
   `body` still contributes one, with `markdown` empty; the sheet says so
   rather than the version silently vanishing.

`latest(currentVersion:repository:session:)` keeps its signature and its
existing error handling (`rateLimited` on 403/429, `network`,
`malformedResponse`). It decodes `[GitHubRelease]` and calls `select`.

### 2. `app/Sources/ScreepubKit/ReleaseNotes.swift` (new)

Parses the subset `docs/release-notes-template.md` already enforces:

```swift
public enum NoteBlock: Equatable, Sendable {
    case section(String)                     // "## Fixed for older Kindles"
    case bullet(lead: String?, body: String) // "- **Lead.** rest"
    case paragraph(String)
    case aside(String)                       // a wholly italic paragraph
}

public enum ReleaseNotes {
    public static func parse(_ markdown: String) -> [NoteBlock]
}
```

Rules:

- `# ...` is dropped. The sheet prints the version itself, so the title line
  would be a duplicate.
- `## X` becomes `.section(X)`.
- A line starting `- ` becomes `.bullet`. If the remainder opens with
  `**...**`, that becomes `lead` and the rest becomes `body`; otherwise
  `lead` is nil.
- A paragraph wrapped entirely in `*...*` becomes `.aside`.
- Blank lines separate blocks. Wrapped continuation lines join with a space,
  because the notes are hard-wrapped at 72 columns in the repo and must
  reflow at the sheet's width.
- Inline `**bold**` inside a body is unwrapped to plain text. The sheet
  styles the lead; it does not need a rich inline model, and a half-rendered
  asterisk is worse than none.
- `<!-- ... -->` HTML comment markers are stripped. GitHub hides these
  because it renders markdown to HTML, where a comment is invisible; our
  sheet renders plain SwiftUI `Text`, so a marker left in would show up to
  the reader as literal `<!-- ... -->`.
- A line that is exactly `---` after trimming ends the document. release.yml
  publishes the committed notes file with a machine-generated trailer
  appended after that separator (install instructions, a SHA-256 block), and
  that trailer is not authored content.

**The invariant:** any line that matches nothing becomes `.paragraph`
verbatim. Unknown syntax may render plainly, but it never disappears. This is
the one rule with a dedicated test. It is a guarantee about PROSE, not about
every byte of input: the `# ` title, `<!-- ... -->` markers, and everything
from a bare `---` onward are dropped on purpose, because each is process
punctuation rather than content the reader is meant to see. No other prose
is dropped.

### 3. `app/Sources/ScreepubApp/ReleaseNotesSheet.swift` (new)

A scrolling sheet. For each `ReleaseNote`, newest first:

- The version as a `Slugline` ("SCREEPUB 0.5.0").
- Its blocks: `.section` in `Theme.inkMuted` caps with tracking, `.bullet`
  with the lead in `Theme.ink` and the body in `Theme.ink`, `.paragraph` in
  `Theme.ink`, `.aside` in `Theme.inkMuted` italic.
- An empty `markdown` renders one muted line: "No notes were published for
  this version."

Pinned below the scroll region, not inside it:

- `INSTALL AND RELAUNCH` with `BradButtonStyle`, the same action the popover
  runs.
- `NOT NOW` with `OutlineButtonStyle`, dismisses.
- `READ ON GITHUB` with `MarginButtonStyle`, opens `releaseNotesURL`. Kept as
  the escape hatch for anything the parser renders badly.

### 4. `app/Sources/ScreepubApp/ContentView.swift`

`VIEW RELEASE NOTES` stops calling `NSWorkspace`. It dismisses the popover
and hands ContentView the update it already holds; ContentView presents the
sheet with `.sheet(item:)` over `@State private var releaseNotesUpdate:
AvailableUpdate?`, so `AvailableUpdate` conforms to `Identifiable` keyed on
`version`.

The sheet is presented from ContentView rather than from inside the popover
deliberately. A sheet presented by a view that is itself being dismissed
races its own presenter, and the popover dismisses the moment its button
fires.

It carries the update rather than a boolean for a second reason, found in
review. A boolean plus a content closure that re-reads `updates.available`
are two facts that can disagree: `checkNow()` can clear `available` either
while the sheet is open or during the one-hop deferral before it opens, and
either way the sheet renders with no content. Its dismiss controls live
inside that content, so the result is a modal with nothing to press.
`.sheet(item:)` makes "presented" and "has content" the same fact, which
removes both cases rather than guarding them.

## Testing

All Swift-side, so all in `kit-check`. No XCTest exists here.

Selection, against hand-built `[ReleaseCandidate]` values and no network:

1. Installed 0.4.2, releases 0.6.0 / 0.5.0 / 0.4.2: notes are exactly
   ["0.6.0", "0.5.0"], and `version` is 0.6.0.
2. A draft and a prerelease among the newer releases are both excluded.
3. Nothing newer returns nil.
4. Newest release without a `.dmg` throws `noDownloadableAsset`.
5. A newer release with a nil body still contributes a `ReleaseNote` with
   empty markdown.

Parsing, using the committed `docs/releases/0.5.0.md` as the fixture. That
file is the prose half of what release.yml actually publishes, not the whole
release body: the workflow appends a machine-generated trailer after it (a
bare `---` line, then install instructions and a SHA-256 block), which the
parser stops at:

6. Produces at least one of each of `.section`, `.bullet`, `.paragraph`,
   `.aside`.
7. The `# Screepub 0.5.0` title produces no block.
8. A bullet opening `**No more orphaned lines.**` yields that as `lead`, with
   the remainder as `body` and no asterisks in either.
9. **No text lost.** Concatenating every block's text contains every word of
   the source's PROSE: every non-empty line except the title, any
   `<!-- ... -->` comment markers, and anything from a bare `---` line
   onward (release.yml's trailer). This is the guard on the
   degrade-to-paragraph invariant, not a claim that the parser preserves the
   raw bytes verbatim.
10. Empty input yields an empty array rather than one empty paragraph.

## Out of scope

- A post-install "what's new" screen. This spec covers the prompt only.
- Markdown links, images, code fences and nested lists. The template uses
  none, and the degrade-to-paragraph invariant in deliverable 2 means they
  render as plain text rather than breaking.
- Caching notes for offline reading. No network means no update check, which
  means no prompt to attach notes to.
- Changing the notes template, `release.yml` or `ci.yml`. This work consumes
  what they already guarantee.

## Risk worth naming

Moving off `/releases/latest` means the app now depends on the newest
non-draft, non-prerelease entry in a list rather than on GitHub's own
"latest" designation. Those disagree when a release is published out of
order, or when the newest release is marked prerelease. Rule 1 plus the
explicit sort in rule 3 make the app's choice deterministic and independent
of GitHub's, which is the safer of the two behaviors, but it is a behavior
change and not merely a wider fetch.
