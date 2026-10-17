# Release notes: the template

Method Reader's readers are screenwriters, producers and executives. They read
scripts on a Kindle. They do not read our source, our commit log, or this
repository at all. The notes exist to tell one of those people what is
different about the script they are about to convert.

This file is the whole brief. It is written so that someone with no memory
of any previous release can open it, read the last two release files, and
produce notes in the same voice at the same length.

Every release file lives at `docs/releases/<version>.md` and looks like
this, in this order:

```markdown
# Method Reader <version>

<one line, only if privacy, requirements or the network surface changed>

<lede: one sentence>

## <heading named for the reader's outcome>

- **Claim in the reader's words.** Explanation.

## <heading named for who was affected>

- **Claim in the reader's words.** Explanation.

## Good to know

- <caveat, or a known gap>

## Thanks

- <who helped, and with what>

*<optional italic closer>*
```

No date line. No version-bump commentary. No checksum: the release workflow
(`.github/workflows/release.yml`) appends the real one when it writes the
draft release, and two SHAs on one page both look official. The steps
around the notes (the version bump, the pictures, the tag, the person who
checks the draft and publishes it) are in [RELEASING.md](RELEASING.md).

## 0. Read these before you write a word

1. **The two most recent real files in `docs/releases/`.** Match their
   length and their vocabulary. This is how voice actually transfers. The
   caps below are ceilings; the last two files are the target.
   `docs/releases/0.4.2.md` is a 21-word stub with no bullets and no
   voice. Stubs are not voice examples: skip them, and if that leaves
   fewer than two real files, use the one real file alone.
2. **The range facts:** `cargo xtask release-notes v<previous>` (from
   `rust/`) prints
   JSON with `commits` (subjects as data, each flagged `ownerAuthored`),
   `verdictsOpenedInRange` and `verdictsResolvedInRange` (each entry's
   number, title and `Device support:` line), `registryChanges` (the
   entry numbers added or materially edited in the range) and
   `userVisible`.
3. **The registry entries named by `registryChanges`**, in
   `docs/formatting-options-log.md`. This is where you find out WHICH
   bullets exist, so read it before you decide anything, not only when
   writing a bullet you have already chosen. It is the primary signal
   because it is the one that works: over the 0.5.0 range all 6 bullets
   trace to an entry it names, while `src/options.ts` gained exactly one
   key (`printSplitMinimums`) and so could account for 1. A changed
   option key is a secondary hint, and what it hints is narrow: that
   bullet needs a default-and-why sentence (rule 5.6).
   `registryChanges` is a shortlist to judge, not a list to transcribe:
   for 0.5.0 it named 11 entries and 6 became bullets. Read the entries
   for FACTS. See the contamination warning in section 5.
4. **`rust/crates/sp-app/src/model/tune.rs`** if a bullet announces a new
   switch. That file holds the exact label the reader will see on the
   window's Settings page.
5. **Who helped.** Ask the owner, and read the range for it: an issue that
   reported a script that converted badly, a device someone tested, a
   correction to how a script is formatted on the page. Section 10 has
   the rules.

## 1. Caps, with 0.5.0's actuals as calibration

Word counts are `wc -w` on the file, H1 included. Verified against
`docs/releases/0.5.0.md`.

| Piece | Cap | 0.5.0 actual |
| --- | --- | --- |
| Whole file | 350 words | 325 |
| Lede | 1 sentence, 25 words | 12 |
| Change sections | 2 headings, 6 bullets TOTAL | 2 and 6 |
| One bullet | bold claim 10 words or fewer, plus 50 words or fewer of explanation | longest claim 9, longest explanation 46 |
| Good to know | 3 bullets | 2 |
| Thanks | 1 bullet per person or group, 15 words each | none (0.5.0 predates the slot) |
| Closer | 1 paragraph, 70 words, optional | 47 |

Every actual is comfortably under its cap. That is the point. A release
that needs all 350 words is a release that has not been cut yet.

Six bullets TOTAL across both change sections. Not six per heading.

## 2. The cut ratio, with numbers

Count your own range: `git rev-list --no-merges --count v<previous>..HEAD`.
Do not copy a number out of this file. The range stays open until the tag
exists, so it grows under you: 0.5.0's was around 40 when its notes were
drafted and still climbing afterwards.

Whatever that number is, **expect roughly one bullet per six commits.**
0.5.0 shipped 6.

One whole merged branch (`test-hardening`, 5 commits, 296 lines added)
produced nothing at all, because it changed only tests. Another
(`review-fixes`, 3 commits) produced exactly one bullet.

**The default disposition for a commit is NO bullet.** A commit earns a
bullet only when a reader who never opens the source would notice the
difference in a converted script: something looks different on the page,
something they could not do before now works, or something they were told
was true is not.

Things that routinely earn nothing: tests, refactors, docs, specs, plans,
build tooling, internal guards, review fixes to unreleased work, and any
change whose entire effect is that our own code is tidier. Do not reach
for these to fill space. A four-bullet release is a normal release.

When you finish, count the commits that earned nothing and be ready to say
the number. If it is small, you have probably not cut hard enough.

## 3. Six slots, not fixed headings

**Do NOT use New / Improved / Fixed / Changed as a fixed heading set.**
Readers do not care which changes were features and which were bugs. They
care what happens to their script. A heading that sorts by our engineering
category makes the reader do the translation we were supposed to do.

### Slot 1: the pre-lede line (usually absent)

One line, before the lede, if and only if this release changes privacy,
system requirements, or what the app touches on the network. The README
stakes the product's reputation on exactly five network touchpoints and on
"nothing is uploaded". A change there can never be a bullet buried under
improvements. Most releases skip this slot entirely.

Form: a single bold sentence in the reader's terms, for example
**"Method Reader now needs macOS 15 or later."** State it, then move on. Do not
argue with it.

### Slot 2: the lede

One sentence naming the theme in reader terms. It answers "why should I
install this?" before any list.

0.5.0: *This release is about page turns: fewer awkward ones, on more
devices.*

It names an experience (page turns), not a subsystem. If you cannot write
this sentence, the release has no theme and the notes are a list, which is
a signal to cut further.

### Slot 3: improvements, under ONE heading

New and improved together, under a single heading named for the reader's
OUTCOME. Fallback if no honest outcome heading exists: `## What's new`.

0.5.0: `## Your scripts break better across pages`.

### Slot 4: fixes, under ONE heading

Under a single heading named for WHO was affected. Fallback:
`## What's fixed`.

0.5.0: `## Fixed for older Kindles`. A reader with a newer Kindle knows
instantly they can skip it. That is the heading doing its job.

A short intro sentence under a fix heading is allowed when it saves
repetition across the bullets, as 0.5.0 does with "Two settings were
quietly doing nothing when you saved a file for an older Kindle. Both work
now:". Those bullets then drop the bold-claim form, because the intro
already made the claim.

### Slot 5: Good to know

`## Good to know` is the literal heading: caveats about what this release
changed, and the gaps a reader will hit in what it ships. Rules in
section 9.

### Slot 6: Thanks, then the closer

`## Thanks` is the literal heading: the people who helped make this
release, film knowledge included. Then the optional italic closer, with no
heading of its own. Rules in sections 10 and 11.

## 4. Bullet form

```markdown
- **Claim in the reader's words.** Explanation.
```

The bold claim must stand alone. Assume the reader reads the bold text and
nothing else, because most of them will. "**Apple Books stops changing
your formatting.**" survives that test. "**OPF metadata added.**" does not.

The explanation earns its words by answering the obvious next question:
what was it doing before, what does it do now, what do I do about it.

## 5. Voice rules

1. **Second person, present tense.** "Your script keeps its typewriter
   font", not "the script's font is now preserved".
2. **Keep screenplay vocabulary.** Dialogue, slugline, scene, character
   name, parenthetical, transition, title page, page turn. These are the
   reader's own words and they are precise. Do not soften them.
3. **Name devices and apps, not formats.** "Apple Books", "older Kindles",
   "Kobo". See section 7 for the one case where a format name is right.
4. **Say what the reader used to see when it was broken.** Nearly every
   good bullet has a past-tense problem clause: "They used to be a fixed
   gray that disappeared against dark and sepia backgrounds." Without it
   the reader cannot tell whether the change affects them.
5. **Ground defaults in print craft where that is honestly true.** "A
   printed script never leaves a single line of dialogue or action alone
   at the top or bottom of a page. Method Reader now does the same." This
   reader knows print. Do not invent a craft justification that the
   registry does not support.
6. **Every new switch states its default AND the reason to change it.**
   "on by default. Turn it off if you'd rather fit more on each page."
   A switch with no stated reason to touch it reads as a switch nobody
   should touch.
7. **Name the control exactly as the Settings page labels it, once**, in the
   sentence that tells the reader they can change it. Get the string from
   `rust/crates/sp-app/src/model/tune.rs`, not from the registry, which describes options in
   its own words. This applies when a bullet announces a new switch or
   invites the reader to go change something. A fix bullet that merely
   describes what an existing setting does may use plain English.
8. **Assign blame accurately, including to us.** "That's Amazon's rule,
   not ours" is fair because it is true. So is "Method Reader's notes have been
   corrected." Never imply a device is at fault for something we did.
9. **No em dashes.** House rule. Use a colon, a period, or a comma.

### The contamination warning

`docs/formatting-options-log.md` and the commit log are your source
material, and they are written in em dashes and jargon. The registry
contains 132 of them. Commit subjects read like
"Wrapper keeps learn the column spelling (kepub/Readium), in its own rule
for iBooks".

**You are copying FACTS out of those files, not punctuation and not
vocabulary.** If a phrase in your draft would sit comfortably in the
registry, it does not belong in the notes. Read the fact, close the file,
and write the sentence from what you now know.

## 6. Say this, not that

The left column is what to avoid. The right column is approved vocabulary:
those words belong in the notes, and several of them are already in 0.5.0.
Any check you run against this table checks the left column only.

| Do not write | Write instead |
| --- | --- |
| kepub | Kobo (name the device) |
| sideload, sideloaded | copied over USB |
| ragged-right | left-aligned the way a script should read |
| keep-together, keep-with-next, a keep (the noun) | stays with, travels with. The plain verb is fine: "your script keeps its typewriter font" |
| rendering engine, renderer, RMSDK, WebKit | name the device or app that does the rendering |
| stylesheet, CSS, selector | formatting |
| OPF, manifest, spine, metadata | say what the reader sees instead |
| monospace | typewriter font |
| dual dialogue | side-by-side dialogue when two characters talk at once |
| pagination, page-break behavior | page turns, how the script breaks across pages |
| option, flag, knob, toggle | switch, setting |
| widows and orphans, `widows: 2` | orphaned lines, a single line left alone at the top or bottom of a page |
| Enhanced Typesetting, KFX renderer | modern Kindles |
| `break-inside: avoid`, any property name | the instructions that hold a page together |
| iBooks | Apple Books |
| Readium, Thorium | several other reading apps |
| cue | character name |
| mini-slug | the smaller headings inside a scene |
| emits, renders, serializes, honors | Method Reader now (verb the reader recognizes) |
| registry, entry #17, device verdict pending | hasn't been tested on a physical device yet |

The right column is not a thesaurus exercise. Every replacement above is
drawn from how the approved 0.5.0 notes actually phrase the same fact.

## 7. Naming a format

This is a principle, not a banned-word list. A banned-word list would
reject a correct sentence the approved notes already ship.

**Name a format only where the reader must ACT on it: a file they can see
and hand around, or a limitation they will hit. Never as a stage in our
pipeline.**

Passes: *"Kindles still can't read EPUB files copied over USB."* The
reader has a file with that extension, sitting in a folder, and needs to
know it will not work.

Fails: *"the EPUB renderer now emits..."* The reader cannot act on that.
It is our pipeline talking.

When the format is only an internal detail, describe it without naming it,
as 0.5.0 does: *"Switching it off used to remove the title page from one
file format but leave it in the other."*

EPUB, MOBI, AZW3 and KFX are not banned words. They are words that have to
earn their place by being something the reader touches.

## 8. Worked pairs

Left is what you will actually be reading. Right is what shipped in
`docs/releases/0.5.0.md`. These are calibration, not boilerplate: they
show how far a fact has to travel to become a sentence. Never paste one
into a new release.

### Pair 1: three commits, one bullet, and the one thing 0.5.0 got wrong

**Source**

- `ce65963 printSplitMinimums option: the print two-line rule gets a knob (registry #17)`
- `b27ca1c Dialogue and action carry widows/orphans: 2 by default, 1 for tight packing`
- `18f0ff6 Reader rail: Print-style split minimums toggle in the Page group`
- Registry #17: "`printSplitMinimums` emits `widows`/`orphans` on
  `p.dialogue` and `p.action`: ON = 2 (print's two-line rule at page
  edges, pagination-reference §2), OFF = 1 (tight packing, the
  community's documented space-reclaim trick)."

**Notes**

> - **No more orphaned lines.** A printed script never leaves a single
>   line of dialogue or action alone at the top or bottom of a page.
>   Method Reader now does the same. There's a new switch for it, on by
>   default. Turn it off if you'd rather fit more on each page.

**Why:** the property name, the element selectors and the numbers 2 and 1
all disappear. The print rule behind them survives, because that is the
part the reader can judge. One bullet absorbs the engine change, the CSS
and the app control, because to a reader they are one thing.

**The correction to carry forward:** this bullet never names the switch,
so a reader who wants it off cannot find it. Under rule 5.7 the last
sentence should have read: "Turn off **Print-style split minimums** in the
reader's formatting rail if you'd rather fit more on each page." Do that
next time.

### Pair 2: registry jargon at its worst

**Source**

- `3abeba3 Wrapper keeps learn the column spelling (kepub/Readium), in its own rule for iBooks`
- Registry #8b: "`.keep-together` additionally carries
  `-webkit-column-break-inside: avoid`, in a SEPARATE rule of its own,
  which extends this keep to Apple Books, whose WebKit honors only the old
  spelling, and to the Readium family."

**Notes**

> - **Character names stay with their dialogue on more devices.** This
>   already worked on Kindle. It now works in Apple Books and several
>   other reading apps, along with side-by-side dialogue when two
>   characters talk at once.

**Why:** "wrapper keeps", "column spelling", "kepub", "Readium" and
"iBooks" have no meaning to this reader, and four of them are on the say
this, not that table. What survives is the audience change: it used to
work in one place, now it works in more. "Dual dialogue" becomes the
sentence a person would say out loud.

This pair is an example of wording, not of fact: Apple Books does not keep a
character name with its dialogue (registry #8b has the device test), so a
note today would not say it does.

### Pair 3: a change with no visible cause

**Source**

- `1a2ccb3 OPF carries ibooks:specified-fonts so Books honors our font and ragged-right`
- Registry #6b: "unless the package carries
  `<meta property="ibooks:specified-fonts">true</meta>`, a Books reader
  with Justify switched on overrides our ragged-right `text-align`
  outright (and our font-family with it)."

**Notes**

> - **Apple Books stops changing your formatting.** Your script keeps its
>   typewriter font, and lines stay left-aligned the way a script should
>   read, instead of being stretched to fill the width.

**Why:** the reader never sees metadata. They see a script that looked
wrong in one app. "Stretched to fill the width" is what justification
actually looks like to someone who did not ask for it.

### Pair 4: the past-tense problem clause doing all the work

**Source**

- `6ffcaf1 Page marker dims via opacity so every theme keeps it legible`
- Registry #13a: "the marker now recedes via `opacity: 0.6` instead of a
  hardcoded gray. A fixed gray was picked against a white page and fought
  every themed background."

**Notes**

> - **Page numbers stay readable in dark mode.** They used to be a fixed
>   gray that disappeared against dark and sepia backgrounds.

**Why:** 22 words, and the second sentence is the whole bullet. A reader
who never uses dark mode skips it correctly. A reader who does recognizes
the exact thing that annoyed them.

### Pair 5: two commits, one six-word line

**Source**

- `a7b7212 MOBI learns scene page breaks: mbp:pagebreak, the dialect's one primitive`
- `23c8830 MOBI page breaks follow primary scenes only, not mini-slugs (registry #5b)`

**Notes**, under `## Fixed for older Kindles` and its shared intro:

> - Start each scene on a new page.

**Why:** the second commit exists only because the first one was too
broad. The reader never had the broken intermediate version, so there is
nothing to tell them about it. Two commits, one setting, one line. The
heading and the intro sentence carry the who and the what, so the bullet
is just the setting, in plain English. Note that it is NOT the rail's
label, which is "Scene page breaks": rule 5.7 asks for the exact label
only when a bullet announces a new switch or sends the reader off to
change one, and this bullet does neither.

### Pair 6: naming a format only where it is invisible

**Source**

- `a2f1183 MOBI honors includeTitlePage, and breaks where the EPUB breaks`
- Registry #11: "It had ignored it since the dialect was written: OFF
  dropped the EPUB's title file, manifest item, spine itemref and nav
  landmark and left the `.mobi` opening on a title page anyway."

**Notes**

> - Include the title page. Switching it off used to remove the title page
>   from one file format but leave it in the other.

**Why:** the reader cannot act on which of our two output files misbehaved.
They asked for no title page and got one. "One file format but leave it in
the other" is exactly as specific as the reader needs, and section 7 is
why neither format is named.

### Pair 7: a commit that earns nothing

**Source**

- `df1403a A selector-exact CSS rule extractor, with the shadowing case pinned`
- `9f96a6d options.test.ts extracts rules by exact selector, not by lucky regex`
- `a7c24c8`, which made the Swift app's own checks prove every sidecar
  merge line, not just three of them
- Plus the whole `test-hardening` branch these sit on: 5 commits, 296
  lines added.

**Notes**

> (nothing)

**Why:** every converted script is byte-identical before and after. The
work is real and it is why the release is trustworthy, and it is still not
news. Resist the pull to write "improved test coverage": it tells the
reader nothing they can use and it spends a bullet from a budget of six.

### Pair 8: where the closer comes from

**Source**

- `aebf74a Registry 5a device verdict: the chain binds without the wrapper`
- `be0fc8b Appendix B is stale: the docs stop saying ET ignores keeps`
- `5dafa05 Docs tell the truth: the keeps claim, the kepub claim, and two stale pointers`

**Notes**

> *For the curious: Amazon's published guidelines say modern Kindles
> ignore the instructions that hold a page together. Testing on a real
> Kindle shows they don't ignore them at all, and Amazon's own website
> says otherwise too. Method Reader's notes have been corrected to match what
> devices actually do.*

**Why:** these three commits changed no output. What they changed is what
we believed. That is the closer's entire job. See section 11.

### Pair 9: a caveat, and what licenses the reassurance

**Source**, from `verdictsOpenedInRange`, entry 17:

- Device verdict: pending. Next KFX pass.
- Device support: "KFX honors widows/orphans from fw 5.12.3 (Kindle
  Previewer 3.35 added them ~2019); RMSDK (Kobo epub, tolino) honors book
  CSS (MobileRead t=328903). Ignored: KF8/AZW3, MOBI. Unverified on kepub
  e-ink (patch-lore says its WebKit reads them; not confirmed on device).
  Apple Books: WebKit implements the properties; untested."

**Notes**

> - The new orphaned-lines setting hasn't been tested on a physical device
>   yet. It's based on the device makers' own documentation, and it does
>   nothing on readers that ignore it, so it can't hurt.

**Why:** three clauses, each earned by a different part of one line.
"Hasn't been tested on a physical device yet" is the pending verdict said
plainly. "Based on the device makers' own documentation" is what a support
line built from a firmware version and a renderer's published behavior
amounts to, and it tells the reader exactly how strong the claim is
without a single version number. "It does nothing on readers that ignore
it, so it can't hurt" is the inert-if-ignored classification from section
9, and **the thing that licenses it is the word "Ignored" in the support
line.** A device that ignores the properties lays the page out exactly as
it did before. Without that word you may not write the reassurance. The
untested devices are not listed one by one, because a reader deciding
whether to worry does not need the matrix.

> - **Keep-together now carries the column spelling.** The EPUB renderer
>   emits `-webkit-column-break-inside: avoid` in its own rule, so kepub
>   and Readium honor the keep, and iBooks no longer drops both spellings
>   when they share a declaration block.

**Diagnosis:** fluent, accurate, and written for us. Nearly every noun is
on the say this, not that table, a format is named as a pipeline stage,
and the bold claim names one of our CSS classes where it should name what
the reader sees. This is the failure mode to watch for, because it reads
as competent.

### Failure example 2: too long

> - **No more orphaned lines.** A printed script never leaves a single
>   line of dialogue or action alone at the top or bottom of a page.
>   Method Reader now does the same, using the widows and orphans controls,
>   which modern Kindles have honored since firmware 5.12.3 and which Kobo
>   and tolino also read. There's a new switch for it, on by default. Turn
>   it off if you'd rather fit more on each page. It has no effect on
>   older Kindles, which ignore these controls entirely, or on the MOBI
>   file, which ships no stylesheet at all.

**Diagnosis:** 90 words against a cap of 50, and every added word is
registry material the reader cannot use. Firmware numbers belong in the
registry. The device coverage that genuinely matters here is a caveat, and
0.5.0 correctly put a shorter version of it in Good to know instead.

## 9. Good to know

### Sources

Three, and only three:

1. `verdictsOpenedInRange` from `cargo xtask release-notes v<previous>`:
   registry entries whose device verdict became pending during THIS range,
   each carrying its `Device support:` line, which is the sentence you
   actually write the caveat from. Worked example: pair 9.
2. Standing product constraints a reader keeps hitting, such as Kindles
   refusing EPUB files over USB.
3. Known gaps in what this release ships: something new that is not
   finished (a file format still in beta, a platform whose copies do not
   update themselves yet), or a problem we know about and have not fixed.
   Say what the reader will run into and what to do meanwhile: "Windows
   and Linux copies don't update themselves yet. Download the new version
   from the website." Never hide a gap to keep the section short. A gap a
   reader finds for themselves costs the next release's notes their
   trust.

**Not** the full pending list. Entries that were already pending before
this cycle reappear in every release forever and train the reader to skip
the section.

### The four-question filter

For anything from source 1, include it only if all four are yes:

1. Did it ship in THIS release?
2. Is it reachable without the reader opting in? A setting that is off by
   default and that nobody turned on is not a caveat.
3. Would the reader notice if our guess is wrong?
4. Has it not already been disclosed in an earlier release file?

Source 3 must answer yes to questions 1 and 3: a gap in something this
release ships, that a reader would notice. A gap that an earlier release
already disclosed is repeated only when this release's content invites the
reader to hit it, the same test source 2 gets.

Source 2 answers no to the first question by definition, so it gets a
different and stricter test: **at most one standing constraint, and only
when this release's own content invites the reader to get it wrong.**
0.5.0 talks about page turns across devices, which is exactly when someone
decides to drag a file onto a Kindle, so "Kindles still can't read EPUB
files copied over USB" belongs there. In a release about something else it
would be noise, and repeating it every time is how a section gets skipped.

### The honesty classification

Every caveat is one of three things. Say which, in the caveat itself.

- **Inert if ignored.** The device that does not support it simply does
  nothing differently. Reassure, and say why it is safe: 0.5.0's "it does
  nothing on readers that ignore it, so it can't hurt."
- **Degrades visibly.** Say exactly what they would see, and do NOT
  reassure. A reader who finds the defect after we called it harmless will
  not believe the next release's notes.
- **Invisible either way.** Omit it. It is not a caveat, it is an
  engineering note.

### The marker convention

After each caveat sourced from a registry entry, leave an HTML comment:

```markdown
- The new orphaned-lines setting hasn't been tested on a physical device
  yet. It's based on the device makers' own documentation, and it does
  nothing on readers that ignore it, so it can't hurt. <!-- caveat: registry-17 -->
```

The comment does not render on the GitHub release page. It exists so the
next release can grep the previous file and ask: did that verdict land? If
it did, the caveat is retired and may be worth a line in the new notes. If
it did not, the caveat stands and does NOT get repeated.

A marker costs 4 words against the 350-word cap, in `wc -w` and in
`rust/crates/xtask/tests/release_notes_copy.rs` alike, because neither tool knows what an
HTML comment is. Keep markers to this exact shape, and subtract 4 words
per marker when you want the count of the prose a reader will actually
see.

`docs/releases/0.5.0.md` predates this convention and carries no markers.
For that one file, read its Good to know section directly.

## 10. Thanks

Every release names who helped, film knowledge included, under
`## Thanks`. Not only code: the person who tested a page turn on their
Kobo, the reader who sent a script that converted badly, the script
supervisor who said how a scene number really sits on the page, the
person who caught a wrong claim in the last notes. Their knowledge is how
the book comes out right.

```markdown
## Thanks

- **Jo Example** tested side-by-side dialogue on a Kobo Clara.
- **@example-handle** sent a script whose title page came out wrong.
```

The names above are invented, as every example in this file is.

- **Name people the way they asked to be named**, and only with their
  say-so: a name, a GitHub handle, or "a reader who asked not to be
  named". Ask the owner when you do not know.
- **Say what they did, in the reader's words.** "tested side-by-side
  dialogue on a Kobo Clara", not "verified registry #8b".
- **Never name the script they sent**, its title, its writer or a
  character in it. Section 13's rule has no exception for thanks.
- **Do not thank the project, a tool or a company.** People only.
- If nobody outside the project helped with this release, leave the
  section out. If the list would push the file past the cap, cut a
  bullet from the change sections, never a name.

## 11. The closer

One italic paragraph, no heading, at most 70 words, and **optional**.

Its job is exactly one thing: **this release corrected something we
previously believed or documented.** A verdict that landed against
expectation. A published claim that turned out to be wrong. Our own docs
that were saying something untrue.

If nothing in the range did that, **there is no closer.** Most releases
will not have one.

It is not a parking spot for leftover technical detail that would not fit
in a bullet. If you find yourself moving a sentence down here to save a
bullet's word count, delete the sentence instead.

## 12. When the release earns no notes

If nothing in the range changes what a reader would see in a converted
script, write nothing. Do not produce a maintenance placeholder. Say this,
verbatim:

> Nothing in this range changes what a reader would see in a converted
> script, so I haven't written notes. If you're releasing anyway (a
> rebuild, a signing fix, a dependency bump), tell me the reason in your
> own words and I'll write the note from that.

Then stop. If the owner gives a reason, the notes are written from their words and
nothing else.

## 13. Pre-flight checklist

Run all of it before showing anyone anything.

**Counts**

- [ ] `wc -w docs/releases/<version>.md` is under 350, caveat markers
      included (4 words each, and the test counts them too).
- [ ] Lede is one sentence, under 25 words.
- [ ] At most 2 change headings and 6 bullets total across them.
- [ ] No bold claim over 10 words; no explanation over 50.
- [ ] At most 3 caveats or gaps under Good to know.
- [ ] One Thanks bullet per person or group, 15 words or fewer each.
- [ ] Closer, if present, is one paragraph under 70 words.
- [ ] Compare all of the above with the previous release file. Shorter is
      the correct direction.

**Language**

- [ ] `LC_ALL=C grep -n $'\xe2\x80\x94' docs/releases/<version>.md` returns
      nothing. That byte sequence is the em dash, which is banned.
- [ ] No term from the LEFT column of the say this, not that table
      appears. The right column is the approved vocabulary and belongs in
      the notes: a check that rejects "typewriter font", "Apple Books",
      "orphaned lines", "page turns" or "copied over USB" is rejecting
      0.5.0 itself.
- [ ] Every format name passes section 7's act-on-it test.
- [ ] No heading is New, Improved, Fixed or Changed on its own.
- [ ] Every bold claim stands alone if the reader reads nothing else.
- [ ] Every bullet about a fix says what the reader used to see.
- [ ] Every new switch gives its default, a reason to change it, and the
      rail's exact label.
- [ ] No real script title, author or character name anywhere. Ever.

**Truth**

- [ ] Every claim traces to a commit or registry entry in this range. No
      claim traces to something that shipped in an earlier release.
- [ ] Anything not verified on hardware is described as unverified,
      wherever it appears.
- [ ] Each caveat passes all four questions and is classified honestly.
- [ ] Every gap a reader will hit in what this release ships is under
      Good to know, with what to do meanwhile.
- [ ] Everyone who helped is under Thanks, named the way they asked to be
      named, and nothing names a script they sent.
- [ ] The previous release file's caveats were checked: retired ones are
      not repeated, standing ones are not repeated either.
- [ ] The closer, if present, corrects a belief. If it does not, cut it.
- [ ] Nothing in the file is confidential, and nothing came from a real
      script.

**Then**

- [ ] Be ready to state the count of commits that earned no bullet, and
      the two or three claims you are least sure of, with the evidence for
      each.
