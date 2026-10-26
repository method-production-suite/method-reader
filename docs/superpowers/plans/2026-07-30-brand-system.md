# Brand System Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build `brand/`: the token files and eight component previews that
codify Screepub's visual identity, so `DesignSync` can push it to a Claude
Design project and the site gets built from real components.

**Architecture:** `brand/tokens.json` is the machine-readable source, mirrored
into `brand/tokens.css` as custom properties. A Bun test parses
`app/Sources/ScreepubApp/Theme.swift` and pins the five tokens the app and web
share, so the two languages cannot drift silently. Eight self-contained HTML
previews in `brand/components/` each carry a `@dsCard` marker for the Design
System pane, and a test forbids any color literal that is not a token.

**Tech Stack:** Bun test (the existing engine suite), plain CSS custom
properties, plain HTML, inline SVG. No build step, no framework, no
dependencies added to `package.json`.

**Spec:** `docs/superpowers/specs/2026-07-30-visual-identity-design.md`

---

## A note before you start

`CLAUDE.md` says CSS value syntax must stay CSS-2.1-vintage: no `var()`, no
`min()`, no `clamp()`. **That rule is about `src/epub/css.ts` and only that.**
It exists because Adobe RMSDK (Kobo and tolino's EPUB path) can blank a whole
book on a value function it cannot parse. The website and these previews run
in modern browsers, where `var()` and `clamp()` are the correct tools and are
used throughout this plan. Do not "fix" `brand/` to match the EPUB rule, and
do not carry `var()` back into `css.ts`.

## File structure

```
brand/
  tokens.json              source of truth, with provenance per token
  tokens.css               the same values as custom properties
  README.md                how to use it, the pinning rule, the CSS caveat
  components/
    _preview.css           shared shell every preview links
    brad.html              the brass fastener and the empty punch hole
    page-frame.html        ground, page, shadow, the binding rail
    title-block.html       the hero, set as a screenplay title page
    slugline.html          section headings, h2 and h3
    transition-rule.html   the flush-right section divider
    buttons.html           brad button and outline button, both states
    device-table.html      the support table, warning rows intact
    shot-frame.html        a screenshot mounted on the page
tests/
  theme-colors.ts          Theme.swift parser + WCAG helpers (not a test file)
  brand.test.ts            pinning, agreement, contrast, component invariants
```

`tests/theme-colors.ts` is a helper module, not a suite. `tests/css-rules.ts`
already establishes that convention in this repo.

---

### Task 1: Token source and the Theme.swift pin

**Files:**
- Create: `brand/tokens.json`
- Create: `tests/theme-colors.ts`
- Create: `tests/brand.test.ts`
- Modify: `docs/superpowers/specs/2026-07-30-visual-identity-design.md`

- [ ] **Step 1: Write the Theme.swift parser helper**

Create `tests/theme-colors.ts`:

```ts
/// Parses the color literals out of the SwiftUI theme so the web tokens can
/// be pinned to them. Theme.swift is the source; brand/ mirrors it.
export type Rgba = { r: number; g: number; b: number; a: number };

const THEME = new URL('../app/Sources/ScreepubApp/Theme.swift', import.meta.url);

const byte = (v: number): number => Math.round(v * 255);

export function hex({ r, g, b }: Rgba): string {
  return '#' + [r, g, b].map((c) => c.toString(16).padStart(2, '0')).join('').toUpperCase();
}

/// Opaque colors render as hex; translucent ones as rgba(), which is how
/// tokens.json and tokens.css both spell them.
export function cssValue(c: Rgba): string {
  return c.a === 1 ? hex(c) : `rgba(${c.r},${c.g},${c.b},${c.a.toFixed(2)})`;
}

export function fromHex(h: string): Rgba {
  const s = h.replace('#', '');
  const w = s.length === 3 ? s.split('').map((c) => c + c).join('') : s;
  return {
    r: parseInt(w.slice(0, 2), 16),
    g: parseInt(w.slice(2, 4), 16),
    b: parseInt(w.slice(4, 6), 16),
    a: 1,
  };
}

function parseColor(src: string): Rgba | null {
  // NSColor(red:) must be tried before Color(red:), since the former
  // contains the latter as a substring.
  const rgb = src.match(/NSColor\(red:\s*([\d.]+),\s*green:\s*([\d.]+),\s*blue:\s*([\d.]+),\s*alpha:\s*([\d.]+)\)/);
  if (rgb) return { r: byte(+rgb[1]), g: byte(+rgb[2]), b: byte(+rgb[3]), a: +rgb[4] };

  const white = src.match(/NSColor\(white:\s*([\d.]+),\s*alpha:\s*([\d.]+)\)/);
  if (white) {
    const v = byte(+white[1]);
    return { r: v, g: v, b: v, a: +white[2] };
  }

  const plain = src.match(/Color\(red:\s*([\d.]+),\s*green:\s*([\d.]+),\s*blue:\s*([\d.]+)\)/);
  if (plain) return { r: byte(+plain[1]), g: byte(+plain[2]), b: byte(+plain[3]), a: 1 };

  return null;
}

/// Every `static let NAME` in Theme.swift that resolves to a color pair.
/// Declarations without light:/dark: labels (Theme.brass) use the same
/// literal for both, which is exactly what the app does.
export async function themeColors(): Promise<Record<string, { light: Rgba; dark: Rgba }>> {
  const text = await Bun.file(THEME).text();
  const out: Record<string, { light: Rgba; dark: Rgba }> = {};

  for (const chunk of text.split('static let ').slice(1)) {
    const name = chunk.match(/^(\w+)/)?.[1];
    if (!name) continue;
    const light = parseColor(chunk.match(/light:\s*([^\n]+)/)?.[1] ?? chunk);
    const dark = parseColor(chunk.match(/dark:\s*([^\n]+)/)?.[1] ?? chunk);
    if (light && dark) out[name] = { light, dark };
  }
  return out;
}

/// WCAG 2.1 relative luminance and contrast ratio.
export function luminance({ r, g, b }: Rgba): number {
  const f = (c: number) => {
    const s = c / 255;
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
}

export function contrast(a: Rgba, b: Rgba): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}
```

- [ ] **Step 2: Write the failing pinning test**

Create `tests/brand.test.ts`:

```ts
import { describe, test, expect } from 'bun:test';
import { themeColors, cssValue } from './theme-colors';

const tokens = await Bun.file(new URL('../brand/tokens.json', import.meta.url)).json();

describe('brand tokens', () => {
  test('tokens sourced from Theme.swift match it exactly', async () => {
    // The Swift app is the source of truth for the shared palette. This is
    // the same arrangement format-defaults.json has with options.test.ts:
    // the two languages can no longer drift silently.
    const theme = await themeColors();
    const pinned = Object.entries(tokens.colors).filter(
      ([, v]: [string, any]) => v.from === 'Theme.swift',
    );

    expect(pinned.length).toBe(5);

    for (const [name, value] of pinned as [string, any][]) {
      expect(theme[name], `Theme.swift has no color named ${name}`).toBeDefined();
      expect(cssValue(theme[name].light), `${name} light`).toBe(value.light);
      expect(cssValue(theme[name].dark), `${name} dark`).toBe(value.dark);
    }
  });
});
```

- [ ] **Step 3: Run it to verify it fails**

```bash
bun test tests/brand.test.ts
```

Expected: FAIL. Bun cannot resolve `../brand/tokens.json` because the file
does not exist yet.

- [ ] **Step 4: Write tokens.json**

Create `brand/tokens.json`:

```json
{
  "$comment": "Source of truth for Screepub's web identity. Tokens marked from:Theme.swift are pinned to app/Sources/ScreepubApp/Theme.swift by tests/brand.test.ts and must not be edited here alone. See docs/superpowers/specs/2026-07-30-visual-identity-design.md.",
  "colors": {
    "paper":     { "light": "#F7F2E6", "dark": "#1E1C19", "from": "Theme.swift", "role": "the page surface" },
    "ink":       { "light": "#1D1B16", "dark": "#E8E2D3", "from": "Theme.swift", "role": "headings, sluglines, cues" },
    "brass":     { "light": "#E8A33D", "dark": "#E8A33D", "from": "Theme.swift", "role": "the one accent, both modes" },
    "alarm":     { "light": "#AF3220", "dark": "#EA7A65", "from": "Theme.swift", "role": "errors only, never decoration" },
    "hole":      { "light": "rgba(29,27,22,0.10)", "dark": "rgba(0,0,0,0.45)", "from": "Theme.swift", "role": "punched-hole shading" },

    "ground":    { "light": "#23272F", "dark": "#141517", "from": "icon.svg", "role": "behind the page" },
    "ink-muted": { "light": "#6D6960", "dark": "#8F887C", "from": "icon.svg", "role": "transitions, captions" },
    "ink-soft":  { "light": "#4A453A", "dark": "#C4BDAC", "from": "web", "role": "body prose" },

    "brass-specular":  { "light": "#FDF1CE", "dark": "#FDF1CE", "from": "web", "role": "brass ramp stop 0.00" },
    "brass-highlight": { "light": "#F6D486", "dark": "#F6D486", "from": "web", "role": "brass ramp stop 0.17" },
    "brass-shadow":    { "light": "#BE7C1E", "dark": "#BE7C1E", "from": "web", "role": "brass ramp stop 0.76" },
    "brass-rim":       { "light": "#8B5917", "dark": "#8B5917", "from": "web", "role": "brass ramp stop 1.00" },
    "brass-edge":      { "light": "#7A5116", "dark": "#7A5116", "from": "web", "role": "brad rim disc and hairline" },
    "brass-bounce":    { "light": "#F7CE7E", "dark": "#F7CE7E", "from": "web", "role": "reflected light, lower right" }
  }
}
```

- [ ] **Step 5: Run it to verify it passes**

```bash
bun test tests/brand.test.ts
```

Expected: PASS, 1 test. If a value mismatches, the assertion message names the
token and the mode, and `Theme.swift` wins: change `tokens.json`, not the app.

- [ ] **Step 6: Align the spec's brad section with the real token names**

The spec's "The brad" section names a raw hex `#69460F` for the hairline rim
and `--brass-rim` for the disc. Both are now `--brass-edge`. In
`docs/superpowers/specs/2026-07-30-visual-identity-design.md`, replace:

```
2. Rim disc at `--brass-rim`, 41 units of a 100-unit box.
```

with:

```
2. Rim disc at `--brass-edge`, 41 units of a 100-unit box.
```

and replace:

```
6. Hairline rim stroke at 55% of `#69460F`.
```

with:

```
6. Hairline rim stroke at 55% of `--brass-edge`.
```

- [ ] **Step 7: Commit**

```bash
git add brand/tokens.json tests/theme-colors.ts tests/brand.test.ts docs/superpowers/specs/2026-07-30-visual-identity-design.md
git commit -m "Brand tokens, pinned to Theme.swift the way format-defaults.json is"
```

---

### Task 2: The CSS mirror and its agreement test

**Files:**
- Create: `brand/tokens.css`
- Modify: `tests/brand.test.ts`

- [ ] **Step 1: Write the failing agreement test**

Append inside the `describe('brand tokens', ...)` block in
`tests/brand.test.ts`:

```ts
  test('tokens.css declares every token in tokens.json, in both modes', async () => {
    const css = await Bun.file(new URL('../brand/tokens.css', import.meta.url)).text();

    const blockAfter = (marker: string): string => {
      const start = css.indexOf(marker);
      expect(start, `tokens.css is missing ${marker}`).toBeGreaterThan(-1);
      return css.slice(start, css.indexOf('}', start));
    };

    const declared = (block: string): Record<string, string> => {
      const out: Record<string, string> = {};
      for (const [, name, value] of block.matchAll(/--([\w-]+):\s*([^;]+);/g)) {
        out[name] = value.trim();
      }
      return out;
    };

    // blockAfter stops at the first '}', which for the media query is the
    // close of its inner :root, so it captures exactly the dark overrides.
    const light = declared(blockAfter(':root {'));
    const dark = declared(blockAfter('@media (prefers-color-scheme: dark)'));

    for (const [name, value] of Object.entries(tokens.colors) as [string, any][]) {
      expect(light[name], `tokens.css :root is missing --${name}`).toBe(value.light);
      // Tokens whose dark value equals their light value are correctly
      // absent from the dark block: brass is a material, not a hue.
      if (value.dark !== value.light) {
        expect(dark[name], `tokens.css dark block is missing --${name}`).toBe(value.dark);
      } else {
        expect(dark[name], `--${name} should not be re-declared in dark`).toBeUndefined();
      }
    }
  });
```

- [ ] **Step 2: Run it to verify it fails**

```bash
bun test tests/brand.test.ts
```

Expected: FAIL, because `brand/tokens.css` does not exist.

- [ ] **Step 3: Write tokens.css**

Create `brand/tokens.css`:

```css
/* Screepub web identity. Values marked "pinned" mirror Theme.swift and are
   enforced by tests/brand.test.ts. See brand/README.md before editing.
   var() and clamp() are correct here: this is the website, not the EPUB. */

:root {
  --paper: #F7F2E6;
  --ink: #1D1B16;
  --ink-soft: #4A453A;
  --ink-muted: #6D6960;
  --brass: #E8A33D;
  --ground: #23272F;
  --alarm: #AF3220;
  --hole: rgba(29,27,22,0.10);

  --brass-specular: #FDF1CE;
  --brass-highlight: #F6D486;
  --brass-shadow: #BE7C1E;
  --brass-rim: #8B5917;
  --brass-edge: #7A5116;
  --brass-bounce: #F7CE7E;

  --font-structure: 'Courier Prime', 'Courier New', ui-monospace, monospace;
  --font-prose: 'Literata', Georgia, 'Times New Roman', serif;

  --page-max: 1000px;
  --binding-margin: 17.6%;
  --page-right: 11.8%;
  --hole-center: 5.9%;
  --brad-size: clamp(20px, 3.4%, 36px);
  --measure: 66ch;
  --page-shadow: 0 3px 34px rgba(0,0,0,0.5);
}

@media (prefers-color-scheme: dark) {
  :root {
    --paper: #1E1C19;
    --ink: #E8E2D3;
    --ink-soft: #C4BDAC;
    --ink-muted: #8F887C;
    --ground: #141517;
    --alarm: #EA7A65;
    --hole: rgba(0,0,0,0.45);
  }
}
```

- [ ] **Step 4: Run it to verify it passes**

```bash
bun test tests/brand.test.ts
```

Expected: PASS, 2 tests.

- [ ] **Step 5: Commit**

```bash
git add brand/tokens.css tests/brand.test.ts
git commit -m "tokens.css mirrors tokens.json, with a test that says so"
```

---

### Task 3: The contrast guard

This is the test that already paid for itself. `Theme.inkFaint` is `ink` at
55% alpha, which lands at `#7F7C74` on paper and measures 3.7:1, a WCAG AA
failure for normal text. That is why the web has its own muted step. This test
stops the next such value from shipping.

**Files:**
- Modify: `tests/brand.test.ts`

- [ ] **Step 1: Write the failing contrast test**

Append inside the `describe('brand tokens', ...)` block:

```ts
  test('every text token clears WCAG AA on its own background', () => {
    const c = tokens.colors;
    const pairs: [string, string, string, number][] = [
      ['ink on paper',           c.ink.light,       c.paper.light,  7],
      ['ink-soft on paper',      c['ink-soft'].light, c.paper.light, 7],
      ['ink-muted on paper',     c['ink-muted'].light, c.paper.light, 4.5],
      ['ink on brass',           c.ink.light,       c.brass.light,  7],
      ['alarm on paper',         c.alarm.light,     c.paper.light,  4.5],
      ['ink on paper, dark',     c.ink.dark,        c.paper.dark,   7],
      ['ink-soft on paper, dark', c['ink-soft'].dark, c.paper.dark,  7],
      ['ink-muted on paper, dark', c['ink-muted'].dark, c.paper.dark, 4.5],
      ['brass on paper, dark',   c.brass.dark,      c.paper.dark,   7],
      ['alarm on paper, dark',   c.alarm.dark,      c.paper.dark,   4.5],
    ];

    for (const [label, fg, bg, floor] of pairs) {
      const ratio = contrast(fromHex(fg), fromHex(bg));
      expect(ratio, `${label} is ${ratio.toFixed(2)}:1, needs ${floor}:1`).toBeGreaterThanOrEqual(floor);
    }
  });
```

- [ ] **Step 2: Add the two imports it needs**

Change the import line at the top of `tests/brand.test.ts` from:

```ts
import { themeColors, cssValue } from './theme-colors';
```

to:

```ts
import { themeColors, cssValue, contrast, fromHex } from './theme-colors';
```

- [ ] **Step 3: Run it**

```bash
bun test tests/brand.test.ts
```

Expected: PASS, 3 tests. The ratios should land near ink/paper 15.4:1,
ink-soft/paper 8.5:1, ink-muted/paper 4.9:1, ink/brass 8.0:1. If any pair
fails, darken the token, and do not lower the floor.

- [ ] **Step 4: Commit**

```bash
git add tests/brand.test.ts
git commit -m "Contrast floors for every text token, in both modes"
```

---

### Task 4: The preview shell, the brad, and the component invariants

**Files:**
- Create: `brand/components/_preview.css`
- Create: `brand/components/brad.html`
- Modify: `tests/brand.test.ts`

- [ ] **Step 1: Write the failing component invariant tests**

Append a second `describe` block to `tests/brand.test.ts`:

```ts
describe('brand component previews', () => {
  const dir = new URL('../brand/components/', import.meta.url);
  const expected = [
    'brad', 'page-frame', 'title-block', 'slugline',
    'transition-rule', 'buttons', 'device-table', 'shot-frame',
  ];

  test('every preview opens with a @dsCard marker naming its group', async () => {
    for (const name of expected) {
      const text = await Bun.file(new URL(`${name}.html`, dir)).text();
      const first = text.split('\n')[0];
      expect(first, `${name}.html must open with a @dsCard marker`).toMatch(
        /^<!-- @dsCard group="(Brand|Type|Components)" -->$/,
      );
    }
  });

  test('previews use tokens, never raw brand colors', async () => {
    // White and black are allowed: they carry opacity in the brass
    // gradients and are not brand colors. Everything else must be a token.
    const carriers = new Set(['#FFF', '#FFFFFF', '#000', '#000000']);

    for (const name of expected) {
      const raw = await Bun.file(new URL(`${name}.html`, dir)).text();
      // Strip SVG fragment references first. An id like #dad or #face is
      // valid hex and would otherwise read as a hardcoded color.
      const text = raw
        .replace(/url\(#[\w-]+\)/g, '')
        .replace(/href="[^"]*"/g, '');

      for (const [literal] of text.matchAll(/#[0-9a-fA-F]{3,8}\b/g)) {
        expect(
          carriers.has(literal.toUpperCase()),
          `${name}.html hardcodes ${literal}; use a var(--token) instead`,
        ).toBe(true);
      }
    }
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

```bash
bun test tests/brand.test.ts
```

Expected: FAIL. `brad.html` does not exist, so `Bun.file(...).text()` rejects.
The remaining seven components fail the same way in Tasks 5 through 7; that is
expected and each task turns one more green.

- [ ] **Step 3: Write the preview shell**

Create `brand/components/_preview.css`:

```css
@import url('https://fonts.googleapis.com/css2?family=Courier+Prime:wght@400;700&family=Literata:opsz,wght@7..72,400;7..72,500&display=swap');
@import url('../tokens.css');

body {
  margin: 0;
  background: var(--ground);
  font-family: var(--font-prose);
  color: var(--ink-soft);
  -webkit-font-smoothing: antialiased;
}

.sheet {
  max-width: var(--page-max);
  margin: 0 auto;
  background: var(--paper);
  box-shadow: var(--page-shadow);
  padding: 3rem var(--page-right) 3.5rem var(--binding-margin);
  box-sizing: border-box;
}

.row { display: flex; gap: 2rem; align-items: center; flex-wrap: wrap; }

.slug {
  font-family: var(--font-structure);
  font-weight: 700;
  font-size: 17px;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: var(--ink);
  margin: 0 0 0.75rem;
}

.prose {
  font-size: 17px;
  line-height: 1.72;
  color: var(--ink-soft);
  max-width: var(--measure);
  margin: 0;
}

.caption {
  font-size: 14px;
  color: var(--ink-muted);
  margin: 0.6rem 0 0;
}
```

The Google Fonts import is for previews only. The site self-hosts both
families later; both are open licensed, which is what makes that legal.

- [ ] **Step 4: Write the brad**

Create `brand/components/brad.html`. The first line must be the marker, with
no leading whitespace:

```html
<!-- @dsCard group="Brand" -->
<link rel="stylesheet" href="_preview.css">
<svg width="0" height="0" style="position:absolute" aria-hidden="true">
  <defs>
    <radialGradient id="bradFace" cx="50%" cy="50%" r="58%" fx="33%" fy="27%">
      <stop offset="0" stop-color="var(--brass-specular)"/>
      <stop offset="0.17" stop-color="var(--brass-highlight)"/>
      <stop offset="0.45" stop-color="var(--brass)"/>
      <stop offset="0.76" stop-color="var(--brass-shadow)"/>
      <stop offset="1" stop-color="var(--brass-rim)"/>
    </radialGradient>
    <radialGradient id="bradSpec">
      <stop offset="0" stop-color="#fff" stop-opacity="0.88"/>
      <stop offset="0.55" stop-color="#fff" stop-opacity="0.2"/>
      <stop offset="1" stop-color="#fff" stop-opacity="0"/>
    </radialGradient>
    <radialGradient id="bradCast">
      <stop offset="0.52" stop-color="#000" stop-opacity="0.36"/>
      <stop offset="1" stop-color="#000" stop-opacity="0"/>
    </radialGradient>
    <radialGradient id="punchShade" cx="50%" cy="40%" r="54%">
      <stop offset="0.7" stop-color="#000" stop-opacity="0.6"/>
      <stop offset="1" stop-color="#000" stop-opacity="0"/>
    </radialGradient>
    <symbol id="brad" viewBox="0 0 100 100">
      <ellipse cx="53" cy="56" rx="47" ry="46" fill="url(#bradCast)"/>
      <circle cx="50" cy="50" r="41" fill="var(--brass-edge)"/>
      <circle cx="50" cy="50" r="39.5" fill="url(#bradFace)"/>
      <ellipse cx="63" cy="67" rx="15" ry="8" fill="var(--brass-bounce)" opacity="0.34" transform="rotate(-28 63 67)"/>
      <ellipse cx="38" cy="33" rx="17" ry="11" fill="url(#bradSpec)" transform="rotate(-28 38 33)"/>
      <circle cx="50" cy="50" r="39.5" fill="none" stroke="var(--brass-edge)" stroke-opacity="0.55" stroke-width="1.6"/>
    </symbol>
    <symbol id="punch" viewBox="0 0 100 100">
      <circle cx="50" cy="52.4" r="31.5" fill="var(--paper)"/>
      <circle cx="50" cy="50" r="30" fill="var(--ground)"/>
      <circle cx="50" cy="50" r="30" fill="url(#punchShade)"/>
    </symbol>
  </defs>
</svg>
<div class="sheet">
  <h2 class="slug">The brad</h2>
  <div class="row">
    <svg width="96" height="96" aria-hidden="true"><use href="#brad"/></svg>
    <svg width="48" height="48" aria-hidden="true"><use href="#brad"/></svg>
    <svg width="24" height="24" aria-hidden="true"><use href="#brad"/></svg>
    <svg width="96" height="96" aria-hidden="true"><use href="#punch"/></svg>
  </div>
  <p class="caption">
    Smooth domed head, no slot: real screenplay brads are Acco fasteners, and a
    slot would make it a screw. Light source is upper left in every brass
    element on the site. The fourth is an empty punch hole.
  </p>
</div>
```

- [ ] **Step 5: Run the invariant tests**

```bash
bun test tests/brand.test.ts -t "tokens, never raw"
```

Expected: still FAIL, but now on `page-frame.html` not existing rather than on
`brad.html`. That is the signal `brad.html` passed both invariants.

- [ ] **Step 6: Look at it**

```bash
open brand/components/brad.html
```

Expected: three brass fasteners at 96, 48 and 24 pixels plus one empty hole,
on a cream page against slate. The 24px one must still read as brass, since
that is the size it renders at in the rail.

- [ ] **Step 7: Commit**

```bash
git add brand/components/_preview.css brand/components/brad.html tests/brand.test.ts
git commit -m "The brad: 700 bytes of SVG, no raster, and the invariants that guard it"
```

---

### Task 5: The page frame

**Files:**
- Create: `brand/components/page-frame.html`

- [ ] **Step 1: Write the page frame**

Create `brand/components/page-frame.html`:

```html
<!-- @dsCard group="Brand" -->
<link rel="stylesheet" href="_preview.css">
<style>
  .rail { position: fixed; top: 0; bottom: 0; left: 0; width: 100%; pointer-events: none; }
  .rail svg {
    position: absolute;
    left: var(--hole-center);
    width: var(--brad-size);
    height: var(--brad-size);
    transform: translate(-50%, -50%);
  }
  .rail .a { top: 30.7%; }
  .rail .b { top: 50%; }
  .rail .c { top: 69.3%; }
  @media (max-height: 560px) {
    .rail .a, .rail .c { display: none; }
  }
  @media (max-width: 720px) {
    .sheet { padding-left: 11%; }
    .rail svg { width: 20px; height: 20px; }
  }
</style>
<div class="sheet" style="min-height: 150vh">
  <h2 class="slug">Int. the page frame - continuous</h2>
  <p class="prose">
    The page scrolls. The brads do not. They are fixed to the viewport, so
    paper moves under the binding the way it does when you thumb through a
    script on a desk. Scroll this preview to see it.
  </p>
  <p class="caption">
    Holes sit at 30.7%, 50% and 69.3% of viewport height: the true proportions
    of a three-hole punch on an 11 inch page. Brads fill the first and third.
    Below 560px of viewport height the rail drops to one brad, because three
    fasteners in a short window reads as a zipper.
  </p>
</div>
<div class="rail" aria-hidden="true">
  <svg class="a"><use href="brad.html#brad"/></svg>
  <svg class="b"><use href="brad.html#punch"/></svg>
  <svg class="c"><use href="brad.html#brad"/></svg>
</div>
```

- [ ] **Step 2: Check it in a browser**

```bash
open brand/components/page-frame.html
```

Expected: a cream column on slate with two brads and one empty hole down the
left gutter. Scrolling moves the text and leaves the fasteners still.

**If the brads do not appear:** cross-file `<use href="other.html#id">` needs
the file served over HTTP, not `file://`. Serve the folder and reload at
`http://localhost:8000/components/page-frame.html`:

```bash
python3 -m http.server 8000 --directory brand
```

If they still do not appear over HTTP, inline the `<defs>` block from
`brad.html` into `page-frame.html` rather than fighting it. Duplication of one
`<defs>` between two previews is acceptable; the site itself will define the
symbols once in its own layout.

- [ ] **Step 3: Verify the rail never collides with text**

Narrow the window to 400px and confirm the prose still starts clear of the
brads. The guard is `--binding-margin` at 11% on mobile against a 20px brad
centered at 5.9%.

- [ ] **Step 4: Run the suite**

```bash
bun test tests/brand.test.ts
```

Expected: still FAIL on `title-block.html`, which Task 6 creates.

- [ ] **Step 5: Commit**

```bash
git add brand/components/page-frame.html
git commit -m "Page frame: the paper scrolls, the binding does not"
```

---

### Task 6: The type components

**Files:**
- Create: `brand/components/title-block.html`
- Create: `brand/components/slugline.html`
- Create: `brand/components/transition-rule.html`

- [ ] **Step 1: Write the title block**

Create `brand/components/title-block.html`:

```html
<!-- @dsCard group="Components" -->
<link rel="stylesheet" href="_preview.css">
<style>
  .title-block { text-align: center; padding: 2rem 0 1rem; }
  .title-block h1 {
    font-family: var(--font-structure);
    font-weight: 700;
    font-size: 44px;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--ink);
    margin: 0;
  }
  .title-block .byline {
    font-size: 18px;
    line-height: 1.6;
    color: var(--ink-soft);
    margin: 1.1rem auto 0;
    max-width: 34ch;
  }
  @media (max-width: 720px) {
    .title-block h1 { font-size: 30px; }
  }
</style>
<div class="sheet">
  <div class="title-block">
    <h1>Screepub</h1>
    <p class="byline">Screenplay PDFs into e-books that hold their shape.</p>
  </div>
</div>
```

- [ ] **Step 2: Write the slugline**

Create `brand/components/slugline.html`:

```html
<!-- @dsCard group="Type" -->
<link rel="stylesheet" href="_preview.css">
<style>
  .subslug {
    font-family: var(--font-structure);
    font-weight: 700;
    font-size: 14px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--ink-muted);
    margin: 2rem 0 0.6rem;
  }
  .cue {
    font-family: var(--font-structure);
    font-weight: 700;
    font-size: 15px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--ink);
    margin: 2rem 0 0.4rem 3.5rem;
  }
</style>
<div class="sheet">
  <h2 class="slug">Int. your kindle - any text size</h2>
  <p class="prose">
    Drop a script on the window. Scenes stay scenes, cues stay attached to
    their lines, and the whole thing reflows, so it still reads right when you
    crank the font up on e-ink.
  </p>
  <h3 class="subslug">Later that same page</h3>
  <p class="prose">
    Sub-sluglines carry the same caps and tracking at a smaller size, in muted
    ink. They divide a section without competing with the slugline above it.
  </p>
  <p class="cue">Feature name</p>
  <p class="prose" style="margin-left: 3.5rem">
    A character cue sets a labelled item, indented the way dialogue is on a
    script page.
  </p>
</div>
```

The `text-transform: uppercase` does the shouting, so the HTML source stays in
sentence case and remains readable and translatable.

- [ ] **Step 3: Write the transition rule**

Create `brand/components/transition-rule.html`:

```html
<!-- @dsCard group="Type" -->
<link rel="stylesheet" href="_preview.css">
<style>
  .transition {
    font-family: var(--font-structure);
    font-size: 13px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--ink-muted);
    text-align: right;
    margin: 2.5rem 0 0;
    border-top: 1px solid var(--hole);
    padding-top: 1.2rem;
  }
</style>
<div class="sheet">
  <p class="prose">
    A section ends the way a scene does. The hairline is the page's own rule,
    set in the hole tint so it reads as an impression in the paper rather than
    a drawn line.
  </p>
  <p class="transition">Cut to:</p>
  <p class="prose" style="margin-top: 2.5rem">
    The next section starts here. The last transition on any page is
    <em>fade out.</em>, which is the footer's sign-off.
  </p>
  <p class="transition">Fade out.</p>
</div>
```

- [ ] **Step 4: Look at all three**

```bash
open brand/components/title-block.html brand/components/slugline.html brand/components/transition-rule.html
```

Expected: Courier Prime caps throughout the structure, Literata in the
paragraphs. If the paragraphs render in Georgia, the Google Fonts import
failed and you are offline; that is a preview-only problem, not a bug.

- [ ] **Step 5: Run the suite**

```bash
bun test tests/brand.test.ts
```

Expected: still FAIL on `buttons.html`, which Task 7 creates.

- [ ] **Step 6: Commit**

```bash
git add brand/components/title-block.html brand/components/slugline.html brand/components/transition-rule.html
git commit -m "Type components: title page, slugline, transition"
```

---

### Task 7: Buttons, the device table, and the screenshot frame

**Files:**
- Create: `brand/components/buttons.html`
- Create: `brand/components/device-table.html`
- Create: `brand/components/shot-frame.html`

- [ ] **Step 1: Write the buttons**

Create `brand/components/buttons.html`. Geometry matches `BradButtonStyle` and
`OutlineButtonStyle` in `app/Sources/ScreepubApp/Theme.swift`: 3px radius,
0.09em tracking, caps:

```html
<!-- @dsCard group="Components" -->
<link rel="stylesheet" href="_preview.css">
<style>
  .btn {
    font-family: var(--font-structure);
    font-weight: 700;
    font-size: 13px;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    padding: 10px 30px;
    border-radius: 3px;
    border: none;
    cursor: pointer;
    display: inline-block;
    text-decoration: none;
    transition: opacity 0.1s ease-out, transform 0.1s ease-out;
  }
  .btn-brad { background: var(--brass); color: var(--ink); }
  .btn-brad:active { opacity: 0.75; transform: scale(0.985); }
  .btn-outline {
    background: none;
    color: var(--ink);
    box-shadow: inset 0 0 0 1.2px var(--ink);
  }
  .btn-outline:active { opacity: 0.7; }
  .btn:disabled { opacity: 0.35; cursor: default; }
</style>
<div class="sheet">
  <h2 class="slug">Buttons</h2>
  <div class="row">
    <button class="btn btn-brad">Download for Mac</button>
    <button class="btn btn-outline">Read the docs</button>
    <button class="btn btn-brad" disabled>Send to Kindle</button>
  </div>
  <p class="caption">
    The brad button is the page's one brass object besides the fasteners, so
    there is never more than one on screen. Disabled is held at 35% opacity,
    not hidden: it is waiting for hardware, and saying so is the point.
    Ink on brass measures 8.0:1.
  </p>
</div>
```

- [ ] **Step 2: Write the device table**

Create `brand/components/device-table.html`:

```html
<!-- @dsCard group="Components" -->
<link rel="stylesheet" href="_preview.css">
<style>
  .devices { width: 100%; border-collapse: collapse; font-size: 15px; }
  .devices th {
    font-family: var(--font-structure);
    font-weight: 700;
    font-size: 12px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    text-align: left;
    color: var(--ink-muted);
    padding: 0 1rem 0.7rem 0;
    border-bottom: 1px solid var(--hole);
  }
  .devices td {
    padding: 0.85rem 1rem 0.85rem 0;
    border-bottom: 1px solid var(--hole);
    color: var(--ink-soft);
    vertical-align: top;
  }
  .devices .device { color: var(--ink); }
  .status { font-family: var(--font-structure); font-size: 13px; white-space: nowrap; }
  .status-ok { color: var(--ink); }
  .status-untested { color: var(--alarm); }
</style>
<div class="sheet">
  <h2 class="slug">Which readers</h2>
  <table class="devices">
    <thead>
      <tr><th>Device</th><th>How it's sent</th><th>Status</th></tr>
    </thead>
    <tbody>
      <tr>
        <td class="device">Kindle, over USB</td>
        <td>AZW3, or the engine's MOBI</td>
        <td class="status status-ok">Verified on hardware</td>
      </tr>
      <tr>
        <td class="device">Kindle, by email</td>
        <td>EPUB to your kindle.com address</td>
        <td class="status status-ok">Verified</td>
      </tr>
      <tr>
        <td class="device">Kobo</td>
        <td>EPUB or KEPUB over USB</td>
        <td class="status status-untested">Never run on a real device</td>
      </tr>
    </tbody>
  </table>
  <p class="caption">
    Untested rows are set in alarm, the only place on the site that color
    appears. They are not a hedge: nobody has plugged one in, and saying so
    plainly is the most trustworthy thing on the page.
  </p>
</div>
```

- [ ] **Step 3: Write the screenshot frame**

Create `brand/components/shot-frame.html`:

```html
<!-- @dsCard group="Components" -->
<link rel="stylesheet" href="_preview.css">
<style>
  .shot { margin: 0; }
  .shot img {
    display: block;
    width: 100%;
    height: auto;
    border-radius: 4px;
    box-shadow: 0 1px 12px rgba(0,0,0,0.28);
  }
  .shot figcaption {
    font-size: 14px;
    color: var(--ink-muted);
    margin-top: 0.8rem;
    max-width: var(--measure);
  }
</style>
<div class="sheet">
  <h2 class="slug">Look before you send</h2>
  <figure class="shot">
    <img src="../../assets/screenshot-drop.png" alt="Screepub's window, waiting for a screenplay PDF">
    <figcaption>
      Screenshots mount flat on the page with a hairline shadow, no device
      bezel and no perspective. The app already looks like a script page, so a
      frame around it would be a frame around a frame.
    </figcaption>
  </figure>
</div>
```

- [ ] **Step 4: Run the full suite**

```bash
bun test tests/brand.test.ts
```

Expected: PASS, 5 tests. All eight previews now exist, every one opens with a
valid `@dsCard` marker, and none hardcodes a brand color.

- [ ] **Step 5: Look at all three**

```bash
open brand/components/buttons.html brand/components/device-table.html brand/components/shot-frame.html
```

Expected: the brass button is the only saturated object on its page; the
untested row is the only red on its page; the screenshot sits flat on paper.

- [ ] **Step 6: Commit**

```bash
git add brand/components/buttons.html brand/components/device-table.html brand/components/shot-frame.html
git commit -m "Buttons, device table, screenshot frame: the last three previews"
```

---

### Task 8: The README and full verification

**Files:**
- Create: `brand/README.md`

- [ ] **Step 1: Write brand/README.md**

Create `brand/README.md`:

```markdown
# Screepub brand system

The identity, in a form a website can use. The design decisions and their
reasoning live in
[the spec](../docs/superpowers/specs/2026-07-30-visual-identity-design.md);
this file is the operating manual.

## What's here

- `tokens.json`: source of truth. Every color, with a `from` field saying
  where it came from.
- `tokens.css`: the same values as custom properties, light and dark.
- `components/`: eight self-contained previews. Open any of them in a
  browser. Each opens with a `@dsCard` marker so the Design System pane
  indexes it without explicit registration.

## The pinning rule

Five tokens (`paper`, `ink`, `brass`, `alarm`, `hole`) are shared with the Mac
app. `app/Sources/ScreepubApp/Theme.swift` is the source and `tokens.json`
mirrors it. `tests/brand.test.ts` parses the Swift and fails if they disagree,
the same arrangement `format-defaults.json` has with `options.test.ts`.

**If that test fails, change `tokens.json`, not the app.** Change the app only
when you mean to change the app, and then update `tokens.json` to match.

The other tokens are web-only. `Theme.swift` has exactly two text weights and
the second, `inkFaint`, is `ink` at 55% alpha. That blends to `#7F7C74` on
paper, which measures 3.7:1 and fails WCAG AA for normal text. It is fine for
a one-line caption in a native window and wrong for a web page, so the web has
its own `ink-muted` at `#6D6960` (4.9:1) and an `ink-soft` for prose that the
app never needed.

## Adding a color

Don't, if you can avoid it. The palette is six colors and one of them is for
errors. If you must:

1. Add it to `tokens.json` with an honest `from` value.
2. Add it to `tokens.css` in both blocks, or only `:root` if it is
   mode-independent the way brass is.
3. If anything sets text in it, add a pair to the contrast test in
   `tests/brand.test.ts`. Floors are 7:1 for body and headings, 4.5:1 for
   captions and transitions. Darken the color rather than lowering the floor.

## var() is fine here

`CLAUDE.md` bans `var()`, `min()` and `clamp()` in CSS. That rule is about
`src/epub/css.ts` and only that: Adobe RMSDK, which is Kobo and tolino's EPUB
path, can blank an entire book on a value function it cannot parse. This
folder ships to modern browsers. Use `var()` and `clamp()` here, and don't
carry them back into `css.ts`.

## Fonts

Courier Prime for structure, Literata for prose. The previews pull both from
Google Fonts for convenience. The site self-hosts them; both are open licensed,
which is what makes that legal.

## Pushing to Claude Design

`DesignSync` reads this folder. List, then plan, then write:

    list_files → finalize_plan (writes: brand/**) → write_files

Push components one at a time rather than replacing the project wholesale.
```

- [ ] **Step 2: Run the whole engine suite**

```bash
bun test
```

Expected: the full suite passes, including 5 new tests in `brand.test.ts`.
Nothing in `brand/` touches the engine, so any failure elsewhere is
pre-existing, so check `git stash list` and the branch point before assuming
this plan caused it.

- [ ] **Step 3: Typecheck**

```bash
bunx tsc --noEmit
```

Expected: clean. `tests/theme-colors.ts` is the only new TypeScript.

- [ ] **Step 4: Confirm the app is untouched**

```bash
git diff --stat main -- app/ src/
```

Expected: completely empty. This plan changes no engine or app code, and that
is a hard requirement of the spec's non-goals. The only file it edits outside
`brand/` and `tests/` is the spec itself, in Task 1 step 6.

- [ ] **Step 5: Commit**

```bash
git add brand/README.md
git commit -m "brand/README.md: the operating manual, including why var() is fine here"
```

---

## Done means

- `bun test` passes with 5 new tests in `tests/brand.test.ts`.
- `bunx tsc --noEmit` is clean.
- All eight previews open in a browser and render.
- `git diff --stat main -- app/ src/` shows no engine or app changes.
- Changing a color in `Theme.swift` and re-running `bun test` fails with a
  message naming the token. Try it once, then revert, so you know the pin is
  real rather than assuming it.

## Not in this plan

The site itself. `site/` gets built in Claude Design from these components,
which is the owner driving, not an implementation task. Self-hosting the fonts,
the deploy workflow, and the `site/LICENSE` question all belong to that work.
