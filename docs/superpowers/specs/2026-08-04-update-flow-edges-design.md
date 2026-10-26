# The update flow's edges: design

Date: 2026-08-04
Status: awaiting the owner's review
Branch: worktree-release-notes-in-app (continues the in-app release notes work)

## Goal

Two leftovers from
[the in-app release notes work](2026-08-04-in-app-release-notes-design.md).
Both are the update flow failing to tell the truth: one shows the user a
Foundation placeholder instead of what went wrong, the other offers two
different answers to the same question on the same screen.

## Deliverable 1: errors say what happened

### What is true today

`UpdateCheckError` conforms to `Error` and `Equatable`, and not to
`LocalizedError`. `manualUpdateCheck()` in `ScreepubApp.swift` ends its
failure branch with:

```swift
alert.informativeText = "The request didn't go through. Check your connection and try again. (\(error.localizedDescription))"
```

For an error that is not `LocalizedError`, Swift bridges to `NSError` and
`localizedDescription` becomes a generated string. Compiled and run against
the exact enum shape:

```
network("HTTP 503")  ->  "The operation couldn't be completed. (UpdateCheckError error 0.)"
rateLimited          ->  "The operation couldn't be completed. (UpdateCheckError error 1.)"
malformedResponse    ->  "The operation couldn't be completed. (UpdateCheckError error 2.)"
noDownloadableAsset  ->  "The operation couldn't be completed. (UpdateCheckError error 3.)"
```

So the alert reads "The request didn't go through. Check your connection and
try again. (The operation couldn't be completed. (ScreepubKit.UpdateCheckError
error 2.))". Two nested apologies and a case index.

Worse, `network(String)` already carries the real HTTP status or URLError
text, and it is discarded at the one place a user would read it.

This predates the release notes work. It is in scope here because that work
is what made the update flow's failure modes worth looking at, and because
the second deliverable touches the same function.

### The change

`UpdateCheckError` gains `LocalizedError` conformance with an
`errorDescription` per case:

- `rateLimited`: names GitHub's unauthenticated limit and that it is per
  network address, since that is the actionable part. The alert already says
  this in its own branch; the description must stand alone anyway, because
  it is what the generic branch prints.
- `network(let detail)`: the detail, which is either an HTTP status or
  URLSession's own description.
- `malformedResponse(let detail)`: gains an associated `String`. See below.
- `noDownloadableAsset`: says the release exists but carries no `.dmg`,
  which is a half-published release rather than anything the reader did.

`malformedResponse` gains a payload because `candidates(from:)` currently
does `try? JSONDecoder().decode(...)` and throws the detail away. One
malformed element fails all thirty releases, so which element, and which
key, is the whole diagnosis.

**Corrected after review.** An earlier draft of this spec said to carry
`error.localizedDescription`. That is wrong: on a `DecodingError` it returns
a generic Foundation sentence ("The data couldn't be read because it is
missing.") with no index and no key, which is barely better than the
placeholder this deliverable exists to remove. Verified by decoding a
three-element array whose second element was missing `tag_name`. The index
and key live on the error's associated `Context`, in `codingPath` and
`debugDescription`. Read those instead, and fall back to
`localizedDescription` only for a non-`DecodingError`.

Adding an associated value changes the pattern match at
`app/Sources/KitCheck/main.swift`, which currently does
`catch UpdateCheckError.malformedResponse`. It becomes
`catch UpdateCheckError.malformedResponse(_)`. `Equatable` still synthesizes.
No other call site pattern-matches that case.

### Copy rule

These strings reach an `NSAlert`, so they are user-facing copy. No em dashes.
State what happened and, where there is one, what to do. No apologies: the
existing informativeText already opens with "The request didn't go through",
and a description that apologizes again produces the doubled tone above.

### Tests

In `kit-check`, for each of the four cases:

1. `errorDescription` is non-nil.
2. It does not contain "couldn't be completed", the Foundation default that
   this deliverable exists to remove. This is the assertion that actually
   fails if the conformance is dropped later.
3. `network("HTTP 503").errorDescription` contains "503", proving the
   payload reaches the string rather than being decorative.
4. `malformedResponse` carries and surfaces its detail.

Plus one at the boundary: `candidates(from:)` given malformed JSON throws a
`malformedResponse` whose detail is non-empty. That is the one that proves
the `try?` was actually replaced rather than the payload being filled with a
constant.

## Deliverable 2: one answer per question

### What is true today

`manualUpdateCheck()` builds an `NSAlert` and calls `runModal()`. Its "View
Release" button calls `NSWorkspace.shared.open(update.releaseNotesURL)` and
sends the reader to a browser, which is exactly what the release notes work
just removed from the footer popover.

This is not a blackout. `checkNow()` assigns `available`, so after the alert
is dismissed the footer note appears and the in-app sheet is reachable. The
problem is that the same screen now answers the same question two ways: the
alert routes to a browser, the footer two inches below routes to a sheet.

### The change

Presentation state moves from `ContentView`'s `@State` onto the controller,
because two different surfaces now need to raise the same sheet:

```swift
// UpdateController
@Published var notesRequest: AvailableUpdate?
```

`ContentView`'s `.sheet(item:)` binds to `$updates.notesRequest` instead of
its own state. The footer popover's callback sets it. `manualUpdateCheck()`'s
"View Release" button sets it instead of calling `NSWorkspace`.

`.sheet(item:)` stays the presentation form for the reason the previous spec
records: it makes "presented" and "has content" the same fact.

### The window problem, and the assumption I am making

`manualUpdateCheck()` is a free function invoked from a `CommandGroup`
button. It has no view context. Worse, macOS apps run with zero windows, and
Check for Updates works from the menu bar regardless, so setting
`notesRequest` can raise a sheet with no `ContentView` alive to present it.
The user would pick "View Release" and nothing would happen.

The owner did not pick between the options when asked. **This spec assumes the main
window is reopened**, because the update decision belongs on the app's own
surface and silently doing nothing is the worst of the available behaviors.
Overrule this at review if you would rather keep the browser for that case.

Reopening needs two things that do not exist yet:

1. The main `WindowGroup` has no `id`, so `openWindow` cannot target it. Give
   it `id: "main"`. This is a one-word change, but it is a change to the
   app's scene identity, so it is called out rather than buried.
2. `@Environment(\.openWindow)` is only reachable from a `View` or `Commands`
   context. Extract the command into a `Commands`-conforming struct that
   holds the environment value and passes it to
   `manualUpdateCheck(openWindow:)`.

Order of operations in the "View Release" branch: reopen the window if none
is open, then set `notesRequest`. Reopening first means `ContentView` exists
to observe the change.

**Named risk.** `@Environment` inside a `Commands` struct is the documented
shape, but if it does not deliver a usable `openWindow` in this app's
configuration, the fallback is to keep `NSWorkspace.shared.open` for the
no-window case only, and use the sheet whenever a window exists. That is a
worse outcome than the assumption above but strictly better than today, and
it is a contingency rather than the plan. The implementer should report which
one they ended up with rather than quietly picking.

### What is deliberately left alone

The `.failed`-phase footer in `ContentView` also calls `NSWorkspace.shared
.open(update.releaseNotesURL)`. That path fires when the in-app updater
itself failed, so sending the reader to the browser to download manually is
the correct behavior, not an inconsistency. It stays.

### Tests

Almost none of this is reachable from `kit-check`, which links ScreepubKit
and not ScreepubApp. Being straight about that: the controller state, the
command wiring and the window reopen are verified by compiling plus the
manual pass below. Do not invent a harness.

What can be tested is the part that lives in ScreepubKit: nothing in this
deliverable does. So deliverable 2's automated coverage is honestly zero, and
the manual checklist is the verification, not a supplement to it.

### Manual checklist

With a build whose version predates the newest published release:

1. Menu, Check for Updates, with the main window open. "View Release" opens
   the in-app sheet. No browser.
2. Same, with the main window closed. The window reopens and the sheet
   appears over it.
3. The footer popover's own release notes button still works, and the two
   paths land on the same sheet.
4. An update whose install is already running still shows the OK / View
   Release pair, and View Release still opens the sheet.
5. Force a failure (offline). The alert names the real cause, and contains no
   "couldn't be completed".

## Out of scope

- Replacing the `NSAlert` with a SwiftUI surface. It is a separate decision
  about whether the manual check should look like the footer prompt at all.
- The `install()` guard reading `updates.available` fresh rather than the
  sheet's captured update, noted in the previous review as pre-existing and
  shared with the footer popover.
- Retrying or paginating past 30 releases.
