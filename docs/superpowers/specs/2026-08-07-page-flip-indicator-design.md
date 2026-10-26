# The page-flip indicator: design

Date: 2026-08-07
Status: awaiting the owner's review
Branch: main

## Goal

Give the app one looping indicator, drawn from `assets/icon.svg`, for the
waits where the user's screenplay is what is being worked on. The wait that
prompted it is the Kindle sideload: `KFXToolchain.convert` measures 16 to
24 seconds and narrates itself only in text, so the longest stage sits under
a motionless line reading "Amazon's converter is running: usually about 20
seconds…" for most of that.

Non-goal: replacing anything determinate. `convertingPage` already shows a
real percentage from the engine's per-page ticks, and a decorative loop in
place of a true fraction would be a downgrade. The flip appears only where
there is no fraction to show.

## What it is

Three screenplay pages on brass brads. The top page turns left about its
hinge, revealing the next, whose lines are a different arrangement. The cycle
is 3.9s with the three pages staggered a third apart, so a turn begins every
1.3s and each takes about 1.1s, leaving a ~0.2s beat between them. That beat
is deliberate: it reads as riffling rather than as a machine. It never
resolves and never stops, because the thing it is reporting has no known end.

The page is the icon's page: same proportions, same bar positions, same
amber. Nothing is imported as an asset; it is drawn.

## Architecture

Two files, split on the boundary `CLAUDE.md` already draws (ScreepubKit is
logic, ScreepubApp is UI, and `kit-check` is the only place either can be
tested).

### `app/Sources/ScreepubKit/PageFlip.swift`

Pure. No SwiftUI, no AppKit. This is the whole reason the feature is
testable at all, given there is no XCTest here.

**Geometry.** The icon's nine bars, as fractions of the page rect
(`x=208 y=144 w=608 h=736` in `icon.svg`), so they are resolution free:

| role | x | y | w | h |
|---|---|---|---|---|
| slugline | 0.1053 | 0.1196 | 0.5526 | 0.0408 |
| action | 0.1053 | 0.2228 | 0.7895 | 0.0272 |
| action | 0.1053 | 0.2826 | 0.7237 | 0.0272 |
| cue | 0.5000 | 0.3832 | 0.2763 | 0.0353 |
| dialogue | 0.3158 | 0.4647 | 0.5000 | 0.0272 |
| dialogue | 0.3158 | 0.5217 | 0.4342 | 0.0272 |
| cue | 0.5000 | 0.6223 | 0.2105 | 0.0353 |
| dialogue | 0.3158 | 0.7038 | 0.5000 | 0.0272 |
| transition | 0.5789 | 0.8043 | 0.3158 | 0.0299 |

Two further arrangements vary the bar widths so a turn visibly reveals a
different page. They are variations on the same nine roles, not new layouts.

**Detail levels.** `PageFlip.detail(forHeight:)` returns `.full` at or above
28pt and `.reduced` below it. `.reduced` keeps four bars (slugline, action,
cue, dialogue) at `h = 0.065`, evenly spread. Below 28pt the full nine render
at well under a point each and turn to mush; this is the same thing the
`.icns` already does across its slices, so it is in keeping.

**Motion.** `PageFlip.state(atPhase:page:reduceMotion:)` takes a phase in
`[0,1)` and a page index in `0..<3` and returns rotation in degrees, opacity,
and z-order. The turn occupies 28% of a cycle. A page sits at -180 degrees
and zero opacity until its slot comes round, then resets to 0 while it is
behind an opaque sibling, which is why the reset is never seen.

### `app/Sources/ScreepubApp/PageFlipView.swift`

`PageFlipView(height:)`. Three stacked pages driven by
`TimelineView(.animation)`, which supplies a `Date` the view turns into a
phase as `timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 3.9) / 3.9`
and hands to the model. All timing decisions stay in the pure model, so the
view holds no animation state and the tests cover the real thing. Rendering
is `rotation3DEffect` about the y-axis with `anchor: .leading` and perspective
proportional to height so foreshortening matches at every size. Two brass
brads at the hinge. Blank verso, because the back of a screenplay page is
blank. Colors from `Theme.paper`, `Theme.ink`, `Theme.inkMuted` and
`Theme.brass`, so night pages come free.

## Where it appears

The rule: **the page turns when the user's screenplay is what we are waiting
on.**

| site | what is waiting | height |
|---|---|---|
| `ContentView.swift:407` | converting, before the first engine tick | 40pt |
| `ContentView.swift:491` | a transfer in flight (the 20s KFX wait) | 32pt |
| `ReaderRail.swift:93` | the reader re-rendering | 18pt |

Two bare spinners are deliberately left alone. `ContentView.swift:213` is the
updater verifying and installing Screepub itself, and `ScreepubApp.swift:300`
is installing the KFX plugin into Calibre. Neither is waiting on a script, so
a screenplay page there says the wrong thing. The updater's is also
`controlSize(.mini)` beside 9pt text, below where any of this stays legible.

## The plumbing this needs

`transferNote` is a single `String?` carrying both in-flight narration
("preparing for Kindle…") and terminal messages ("copied to X as Y",
"transfer failed: …"). There is no signal today for "a transfer is running,"
and adding a parallel `Bool` would be two channels that can drift, which is
exactly the failure this codebase keeps designing out.

So the note itself carries the answer:

```swift
enum TransferStatus {
    case working(String)   // a machine is busy: the page turns
    case resting(String)   // terminal: no motion
    var text: String { ... }
    var isWorking: Bool { ... }
}
```

`transferNote: String?` becomes `transferStatus: TransferStatus?`. Every
assignment must now declare which it is, so the compiler catches a missed
branch rather than leaving a page turning forever. Sites: `copyToDevice`
(three openers, the `note` closure, both outcomes), `sendToRemarkable`,
`emailToKindle`, the Apple Books line, and the two resets to `nil`.

### SaveFlow, which is the fiddly one

`SaveFlow.present` runs a probe, then opens a **modeless** save panel, then
does the real work. Two things follow:

1. Busy must start at `status("saving…")`, not when the flow begins. While
   the panel is open the app is waiting on a human, not a machine, and a page
   turning at a save dialog is a lie. The ~1s `"checking Kindle formats…"`
   probe is short enough to leave alone.
2. `ExportPanel.present` fires `completion` **only** on `.OK`
   (`ExportPanel.swift:72`), with no cancel path. Setting busy any earlier
   would leave it stuck on for a cancelled save, with no callback to clear it.

Starting busy at `saving…` sidesteps the missing cancel callback entirely, so
this design needs no change to `ExportPanel`. `SaveFlow.present`'s `status`
and `failure` closures change to carry `TransferStatus`.

Noted but out of scope: because there is no cancel path, a cancelled save
already leaves `transferNote` reading "choose where to save" indefinitely.
That is a pre-existing bug. This design does not create it and does not fix
it.

## Reduce motion

`accessibilityReduceMotion` drops rotation to zero at every phase and
cross-fades between the three page arrangements every two seconds instead.
Still reads as working, no vestibular motion. The model handles this, not
the view, so it is covered by `kit-check`.

## Testing

`kit-check` gains, against the pure model:

- exactly one page is frontmost at every sampled phase
- rotation is monotonic across a turn
- opacity reaches zero exactly where the page crosses the hinge
- no two pages share a z-order at any sampled phase
- `reduceMotion: true` yields zero rotation at every phase
- `detail(forHeight:)` switches at 28pt, and `.reduced`'s roles are a subset
  of `.full`'s
- a drift guard that parses `assets/icon.svg`, located from `#filePath` so
  the working directory cannot break it, and asserts the nine bars still
  match the table above

The view itself is not unit tested, which is the same position every other
SwiftUI view in this repo is in.

## What this does not do

It does not make the KFX wait shorter. Most of those 16 to 24 seconds are
Kindle Previewer cold-starting, and Previewer ships x86_64 only, so on Apple
Silicon that start is under Rosetta. Amazon owns that binary. This makes the
wait legible, not faster.
