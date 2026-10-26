# Visual identity: screepub.com is a bound script page

Date: 2026-07-30
Status: approved (brainstorm complete)
Branch: visual-identity

## Goal

Give screepub.com an identity before the owner starts posting about the
project, and put that identity somewhere Claude Design can build
against.

The identity is not invented here. The Mac app has shipped a coherent
one since its first commit, locked inside `Theme.swift` and
`assets/icon.svg` where a website cannot reach it. This spec extracts
it, codifies it as tokens plus a small component library, and extends
it for the web. No engine or app code changes.

## Decisions from the brainstorm

- **Fidelity:** extract and extend, never reinvent. The site is the
  app's twin, not a sibling product.
- **Metaphor depth:** bound script page. A cream page floats on the
  slate ground the app icon already uses, three-hole punched, bound
  with two brass brads in the top and bottom holes with the middle
  hole left empty, the way a real script comes.
- **Body type:** Courier Prime carries all structure. Literata carries
  prose. Courier Prime alone is authentic and tiring past a paragraph.
- **Site scope for v1:** one page. Hero, what it does, screenshots,
  download button, device support table, links out to GitHub.
- **Home:** the Screepub repo, not a separate one. See "Repository
  layout" for the reasoning.
- **Drift protection:** the five tokens `brand/tokens.json` shares with
  `Theme.swift` (paper, ink, brass, alarm, hole) are pinned to it by a
  check, the same arrangement `format-defaults.json` already has with
  `options.test.ts` and `kit-check`. The web-only tokens are not
  pinned, because there is nothing to pin them to.

## The concept

A screenplay is a physical object before it is a document: punched,
bradded, handled. Screepub's whole argument is that this object should
survive the trip to a screen. The site makes that argument by being
the object.

The page scrolls. The brads do not. They are fixed to the viewport, so
paper moves under the binding exactly as it does when you thumb through
a script on a desk.

## Color

Most of this is lifted, not chosen. The provenance column says where
each value comes from, because "already shipping" and "invented for the
web" are different levels of confidence and the difference matters when
someone later asks whether a value can move.

| Token | Light | Dark | Role | From |
| --- | --- | --- | --- | --- |
| `--paper` | `#F7F2E6` | `#1E1C19` | the page surface | `Theme.swift` |
| `--ink` | `#1D1B16` | `#E8E2D3` | headings, sluglines, cues | `Theme.swift` |
| `--brass` | `#E8A33D` | `#E8A33D` | the one accent, both modes | `Theme.swift` |
| `--alarm` | `#AF3220` | `#EA7A65` | errors only, never decoration | `Theme.swift` |
| `--hole` | `rgba(29,27,22,.10)` | `rgba(0,0,0,.45)` | punched-hole shading | `Theme.swift` |
| `--ground` | `#23272F` | `#141517` | behind the page | `icon.svg` (dark is new) |
| `--ink-soft` | `#4A453A` | `#C4BDAC` | body prose | new |
| `--ink-muted` | `#6D6960` | `#8F887C` | transitions, captions | `icon.svg` (dark is new) |

The two new steps exist because the app never needed them. `Theme.swift`
has exactly two text weights, `ink` and `inkFaint`, and `inkFaint` is
`ink` at 55% alpha. That is fine for a one-line caption in a native
window and wrong for the web twice over. It lands at `#7F7C74` on paper,
which measures 3.7:1 and fails WCAG AA for normal text. And prose at
length wants a step between full ink and a caption grey, which the app
has no use for.

So `--ink-muted` is darkened to `#6D6960`, which is not arbitrary: it is
the grey `icon.svg` already uses for its transition bar. That reaches
4.9:1. And `--ink-soft` is new at `#4A453A` for body copy.

Contrast was measured, not assumed. Light mode: ink on paper 15.4:1,
ink-soft on paper 8.5:1, ink-muted on paper 4.9:1, ink on brass 8.0:1.
Dark mode: ink on paper 13.2:1, ink-soft on paper 9.0:1, ink-muted on
paper 4.8:1, brass on paper 7.9:1. Every pairing clears WCAG AA for
normal text, and every pairing except ink-muted clears AAA.

Brass is a constant across modes on purpose. It is a material, not a
hue, and brass does not change color when the lights go down.

## The brass ramp

Brass is rendered rather than filled, so the flat `#E8A33D` gains a
ramp. These five stops define every brass surface on the site.

| Stop | Value | What it is |
| --- | --- | --- |
| 0.00 | `#FDF1CE` | specular core |
| 0.17 | `#F6D486` | highlight falloff |
| 0.45 | `#E8A33D` | the base hue, unchanged |
| 0.76 | `#BE7C1E` | shadow turn |
| 1.00 | `#8B5917` | rim |

Plus `--brass-edge: #7A5116` for the brad's rim disc and hairline, and
`--brass-bounce: #F7CE7E` for reflected light on the lower right.

The ramp is a radial gradient with its focus at 33% / 27%, which puts
the light source upper left. Every brass element on the site uses that
same light source. A second light direction anywhere would read as a
mistake immediately.

## Typography

Two families, split by job. Courier Prime is already the app's face and
is already what Screepub asks e-readers for, so the site and the output
speak the same typeface. Literata was drawn for reading books on
screens, which is the product thesis stated as a font choice. Both are
open licensed and can ship in the repo.

| Role | Family | Size | Treatment |
| --- | --- | --- | --- |
| Title block | Courier Prime 700 | 44 / 30 mobile | caps, 0.09em, centered |
| Byline | Literata 400 | 18 | ink-soft, centered |
| Slugline (h2) | Courier Prime 700 | 17 | caps, 0.04em, flush left |
| Sub-slug (h3) | Courier Prime 700 | 14 | caps, 0.04em, ink-muted |
| Body | Literata 400 | 17 / 1.72 | ink-soft, max 66ch |
| Character cue | Courier Prime 700 | 15 | caps, indented, ink |
| Transition | Courier Prime 400 | 13 | caps, 0.06em, flush right |
| Button | Courier Prime 700 | 13 | caps, 0.09em |
| Code | Courier Prime 400 | 15 | as-is |
| Caption | Literata 400 | 14 | ink-muted |

Body copy is capped at 66 characters even though the script column is
wider. A 6-inch measure is correct for a printed page read at arm's
length and too wide for prose on a screen.

## The page object

Geometry is derived from a real 8.5 x 11 script page. The page column
is 1000px at its widest, which sets the scale at 117.6 pixels per inch.

- **Binding margin:** 1.5in, so 17.6% of page width. Content starts
  there.
- **Right margin:** 1in, so 11.8%.
- **Hole centers:** 0.5in from the left edge, so 5.9% of page width.
- **Brad head:** `clamp(20px, 3.4%, 36px)`. A real Acco #5 head is
  0.44in, which would be 52px at this scale and overwhelms the page on
  a screen. This is a deliberate deviation from the physical object,
  matched by the hole diameter below.
- **Hole diameter:** 0.76 of the brad head. A physically exact ratio
  would be about 0.57 (a 0.25in punch against a 0.44in Acco head), but
  that reads weedy at rail size. This is a deliberate visual choice,
  like the brad head size above.
- **Hole positions:** 30.7%, 50%, and 69.3% of viewport height. Those
  are the true proportions of a three-hole punch on an 11in page.
  Brads sit in the first and third. The second stays empty and shows
  the ground through it.
- **Page shadow:** `0 3px 34px rgba(0,0,0,.5)`, low and soft. The page
  floats a few millimetres, not a centimetre.

Below 560px of viewport height the rail collapses to a single brad at
50%, because three fasteners in a short window reads as a zipper.

Below 720px of viewport width the page goes full bleed, the binding
margin drops to 11%, and the brad head goes to 20px. The rail never
disappears, because it is the identity.

The fixed rail must never collide with text. Page padding-left is
always at least the hole center plus the brad radius plus 24px.

## The brad

One component, about 700 bytes of SVG, no raster assets. It is the
favicon, the bullet, the loading state, and the rail. Structure, in
paint order:

1. Cast shadow, offset down and right, radial fade from 36% black.
2. Rim disc at `--brass-edge`, 41 units of a 100-unit box.
3. Head at 39.5 units, filled with the brass ramp.
4. Bounce light, lower right, `--brass-bounce` at 34%.
5. Specular ellipse, upper left, white fading to nothing, rotated -28
   degrees.
6. Hairline rim stroke at 55% of `--brass-edge`.

No slot. Real screenplay brads are smooth domed Acco fasteners. A slot
would make it a screw, and anyone who has assembled a script would see
it.

The empty hole is the same box: a paper-colored disc offset 2.4 units
down (the lit bottom edge of punched paper), then the ground color, then
an inner shadow.

Both are decorative and carry `aria-hidden="true"`.

## Structural motifs

Site furniture maps onto script furniture. This is the rule that keeps
new pages on-brand without new decisions.

| Site element | Script element |
| --- | --- |
| Hero | title page: title in caps, byline beneath |
| Section heading | slugline, `INT. YOUR KINDLE - ANY TEXT SIZE` |
| Section divider | transition, flush right, `CUT TO:` |
| Primary button | the brad button already in `BradButtonStyle` |
| Secondary button | inked outline on paper |
| Feature label | character cue, indented caps |
| Footer sign-off | `FADE OUT.` |

## Voice

Already written. The README's register is the brand's: concrete,
plainspoken, and honest about limits. Three rules carry it over.

1. **Say what it does not do.** The device table's warning rows stay
   exactly as they are. "Nobody has plugged one in" is the most
   trustworthy sentence on the site and it should stay above the fold
   in spirit.
2. **No superlatives.** Not "effortless", not "beautiful", not
   "revolutionary". The README earns trust by declining to sell, and
   the site inherits that or loses it.
3. **Second person, present tense.** "You get scripts as PDFs." The
   reader is a person with a specific problem, not a market.

No em dashes in site copy. Colons and periods do the work.

## Component inventory

Eight previews in `brand/components/`, each a self-contained HTML file
whose first line is a `<!-- @dsCard group="..." -->` marker so the
Design System pane indexes it without explicit registration.

| Component | Group | Covers |
| --- | --- | --- |
| `page-frame` | Brand | ground, page, shadow, the rail in place |
| `brad` | Brand | brad and empty hole, three sizes |
| `title-block` | Components | the hero |
| `slugline` | Type | h2 and h3 |
| `transition-rule` | Type | the divider |
| `buttons` | Components | brad button, outline button, both states |
| `device-table` | Components | the support table with warning rows |
| `shot-frame` | Components | a screenshot mounted on the page |

## Repository layout

Everything lives in the Screepub repo. A separate site repo was
considered and rejected: the site's content is the README's content,
its screenshots are `assets/`, and its tokens derive from
`Theme.swift`. Splitting the repo creates a second copy of the device
support table, and a stale copy of that table is worse than a stale
copy of anything else on the site, because its entire value is being
accurate about what has never been tested.

```
brand/
  tokens.css          custom properties, light and dark
  tokens.json         same values, machine readable
  components/*.html   the eight previews
  README.md           how to sync, and the pinning rule
site/                 the site itself, built later in Claude Design
```

`site/` gets a path-filtered deploy workflow so site commits never
trigger the DMG release pipeline. If the site later grows a blog and
its own release rhythm, extracting a static folder into its own repo is
an afternoon. Splitting now buys nothing and costs a sync step.

The site's marketing copy is not program code. If the owner wants it under
different terms than AGPL-3.0-or-later, a `site/LICENSE` note is the
place to say so. Flagged, not decided, and it blocks nothing.

## Build order

1. `brand/tokens.css` and `brand/tokens.json`, values lifted from
   `Theme.swift`, plus the brass ramp which is new.
2. The pinning check, so the two files cannot drift from `Theme.swift`.
3. The eight component previews, `brad` and `page-frame` first since
   everything else sits inside them.
4. Push `brand/` to a Claude Design project with `DesignSync`.
5. Build the one-page site in Claude Design, composing from those
   components.

Steps 1 through 3 are the identity work and are the subject of the
implementation plan that follows this spec. Steps 4 and 5 are the owner
driving Claude Design.

## Non-goals

- Changing `Theme.swift`, the app icon, or any engine code.
- A wordmark or logotype beyond the existing icon. The title block set
  in Courier Prime caps is the wordmark.
- Docs, roadmap, changelog, or blog pages. One page for v1.
- Illustration, photography, or any raster asset. Everything is SVG,
  type, and color.
- Analytics, cookie banners, or anything that would need a consent
  dialog. The app works offline and does not phone home, and the site
  should hold the same line.
