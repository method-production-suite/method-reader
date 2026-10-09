import { describe, test, expect } from 'bun:test';
import { contrast, fromHex } from './color-contrast';

const tokens = await Bun.file(new URL('../brand/tokens.json', import.meta.url)).json();
const colors = tokens.colors;

// brand/tokens.json is the source of truth (F3 acceptance criterion 12,
// docs/superpowers/specs/2026-09-14-retire-swiftui-design.md), and nothing
// in this file reads Swift. Seven of its colours were first defined by the
// Swift Mac app's theme; tokens.json says so in their `from` field. Until
// 2026-09-23 this file pinned tokens.json TO that theme, then the other way
// round, and at F3 the theme left the repository with the app, so the copies
// pinned here are the ones that still ship: tokens.css below, and the
// window's style.css and tokens.css in their own tests.

// Every (label, foreground token, fg hex, bg hex, floor) row the contrast
// test checks. The token name rides alongside the hex value specifically
// so the completeness test below can ask "does this token have a pair"
// directly, instead of maintaining a second, hand-written list of which
// tokens are covered that could itself drift out of sync with this one.
const pairs: [label: string, fgToken: string, fg: string, bg: string, floor: number][] = [
  ['ink on paper',              'ink',       colors.ink.light,         colors.paper.light, 7],
  ['ink-soft on paper',         'ink-soft',  colors['ink-soft'].light, colors.paper.light,  7],
  ['ink-muted on paper',        'ink-muted', colors['ink-muted'].light, colors.paper.light, 4.5],
  // Both modes, because brass does not change and neither may its label.
  // `ink` itself is deliberately NOT paired against brass: it flips to cream
  // in dark mode and lands at 1.67:1, which is the bug ink-on-brass exists
  // to prevent.
  ['ink-on-brass on brass',       'ink-on-brass', colors['ink-on-brass'].light, colors.brass.light, 7],
  ['ink-on-brass on brass, dark', 'ink-on-brass', colors['ink-on-brass'].dark,  colors.brass.dark,  7],
  // The brad button's hover ground. Same label on a lighter stop of the same
  // ramp, so it must clear the floor too rather than being assumed safe.
  ['ink-on-brass on brass-highlight', 'ink-on-brass', colors['ink-on-brass'].light, colors['brass-highlight'].light, 7],
  ['alarm on paper',            'alarm',     colors.alarm.light,       colors.paper.light,  4.5],
  ['ink on paper, dark',        'ink',       colors.ink.dark,          colors.paper.dark,   7],
  ['ink-soft on paper, dark',   'ink-soft',  colors['ink-soft'].dark,  colors.paper.dark,   7],
  ['ink-muted on paper, dark',  'ink-muted', colors['ink-muted'].dark, colors.paper.dark,   4.5],
  ['brass on paper, dark',      'brass',     colors.brass.dark,        colors.paper.dark,   7],
  ['alarm on paper, dark',      'alarm',     colors.alarm.dark,        colors.paper.dark,   4.5],
];

// Tokens whose role puts text on the page and therefore need a pair
// above. Derived from tokens.json's own `"text": true` field rather than
// hand-curated here, because a hand-curated second list is exactly how
// this went wrong once already: ink-footnote was added straight to
// tokens.json and tokens.css at 2.22:1 with no contrast pair, and
// because nothing tied this array to the token data, the suite still
// went green. Marking a color text-bearing now happens in one place,
// next to the color's own definition, and the completeness test below
// fails the moment that mark exists without a matching pair above.
const textBearing = Object.entries(colors)
  .filter(([, v]: [string, any]) => v.text === true)
  .map(([name]) => name);

describe('brand tokens', () => {
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

    // Known limits. blockAfter/declared assume tokens.css keeps a FLAT,
    // non-nested :root block and a flat, non-nested :root inside the dark
    // @media block, with exactly ONE occurrence of each marker string
    // (':root {' and '@media (prefers-color-scheme: dark)') in the whole
    // file, and no '}' inside a comment or a quoted/URL value ahead of
    // the block's real closing brace. Any of those would truncate the
    // slice early, and silently: the wrong (shorter) block would still
    // parse, just to the wrong values. declared's regex also treats any
    // ';' as the end of a value regardless of quoting, so a future value
    // that legitimately contains one, e.g. a data URI like
    // url("data:image/svg+xml;base64,..."), would be silently truncated
    // at the first ';' rather than rejected.
    //
    // The reason a broken dark-block slice gets caught loudly today is
    // incidental, not structural: the loop below checks EVERY token with
    // strict .toBe() equality, and tokens.json happens to list a
    // light-differs-from-dark token (paper) first, so a declared() that
    // silently returned the wrong span would already produce a value
    // mismatch on the very first iteration. Tokens whose light and dark
    // values are equal (brass and the six brass-* ramp stops) only assert
    // dark[name] is undefined, which passes vacuously if the dark block
    // failed to parse at all. Reorder tokens.json so a light-equals-dark
    // token comes first, or change this loop to early-exit semantics
    // (stop after the first pass instead of checking all of them), and
    // that guarantee quietly erodes.
    //
    // blockAfter stops at the first '}', which for the media query is the
    // close of its inner :root, so it captures exactly the dark overrides.
    const light = declared(blockAfter(':root {'));
    const dark = declared(blockAfter('@media (prefers-color-scheme: dark)'));

    for (const [name, value] of Object.entries(colors) as [string, any][]) {
      expect(light[name], `tokens.css :root is missing --${name}`).toBeDefined();
      expect(light[name], `tokens.css :root and tokens.json disagree on --${name}`).toBe(value.light);

      // Tokens whose dark value equals their light value are correctly
      // absent from the dark block: brass is a material, not a hue.
      if (value.dark !== value.light) {
        expect(dark[name], `tokens.css dark block is missing --${name}`).toBeDefined();
        expect(dark[name], `tokens.css dark block and tokens.json disagree on --${name}`).toBe(value.dark);
      } else {
        expect(dark[name], `--${name} should not be re-declared in dark`).toBeUndefined();
      }
    }
  });

  test('every text token clears WCAG AA on its own background', () => {
    for (const [label, , fg, bg, floor] of pairs) {
      const ratio = contrast(fromHex(fg), fromHex(bg));
      expect(ratio, `${label} is ${ratio.toFixed(2)}:1, needs ${floor}:1`).toBeGreaterThanOrEqual(floor);
    }
  });

  test('nothing sets a mode-flipping ink on a fixed brass ground', async () => {
    // The pair table above proves the TOKEN values are sound. It cannot see
    // a component that pairs them wrongly, which is how the brass button
    // shipped at 1.67:1 in dark mode: --brass holds across modes, --ink does
    // not, and buttons.html put one on the other. This reads the components
    // and fails on that specific combination.
    const dir = new URL('../brand/components/', import.meta.url);
    const files = [...new Bun.Glob('*.html').scanSync({ cwd: Bun.fileURLToPath(dir) })];
    expect(files.length).toBeGreaterThan(0);

    for (const file of files) {
      const css = await Bun.file(new URL(file, dir)).text();
      // Rules that paint a brass background, captured with their body.
      for (const [rule] of css.matchAll(/\{[^{}]*background:\s*var\(--brass\)[^{}]*\}/g)) {
        expect(
          /color:\s*var\(--ink\)/.test(rule),
          `${file} sets color:var(--ink) on a var(--brass) background. ` +
            `--ink flips to cream in dark mode and lands at 1.67:1 there. ` +
            `Use --ink-on-brass, which is fixed in both modes.`,
        ).toBe(false);
      }
    }
  });

  test('interactive components declare hover and keyboard focus', async () => {
    // A control with no hover response reads as text, and a control with no
    // focus-visible rule is only reachable by keyboard if the browser's own
    // ring happens to show over the component's ground. Both were missing
    // from buttons.html while :active and :disabled were present, so absence
    // here is a live gap rather than a hypothetical one.
    const css = await Bun.file(
      new URL('../brand/components/buttons.html', import.meta.url),
    ).text();

    expect(/:hover/.test(css), 'buttons.html declares no :hover state').toBe(true);
    expect(
      /:focus-visible/.test(css),
      'buttons.html declares no :focus-visible state',
    ).toBe(true);

    // Both variants, not just whichever one happened to get styled.
    for (const variant of ['btn-brad', 'btn-outline']) {
      expect(
        new RegExp(`\\.${variant}:hover`).test(css),
        `.${variant} has no :hover state`,
      ).toBe(true);
    }

    // .btn sets text-decoration and display specifically so anchors can wear
    // it, and the download CTA is an <a>. :enabled matches only form
    // controls, so gating hover on it silently drops every link variant.
    expect(
      /:hover:enabled/.test(css),
      'hover is gated on :enabled, which no <a> ever matches; use :not(:disabled)',
    ).toBe(false);
  });

  test('the contrast pair table covers every text-bearing token', () => {
    // Completeness guard for the table above. Without this, adding a new
    // text-bearing color per brand/README.md's process and forgetting to
    // add a pair fails silently, which is the same silent-inaccessibility
    // shape the contrast test itself exists to prevent.
    const exercised = new Set(pairs.map(([, fgToken]) => fgToken));
    for (const name of textBearing) {
      expect(
        exercised.has(name),
        `"${name}" is marked "text": true in tokens.json but has no pair in the contrast table above; add one.`,
      ).toBe(true);
    }
  });
});

describe('the contrast arithmetic the table above relies on', () => {
  // The floors above are only as good as contrast(). A version that
  // returned Infinity, or compared the wrong channel, would pass every one
  // of them silently. These are WCAG 2.1's own reference points.
  test('black on white is 21:1, either way round', () => {
    expect(contrast(fromHex('#000000'), fromHex('#FFFFFF'))).toBeCloseTo(21, 5);
    expect(contrast(fromHex('#FFFFFF'), fromHex('#000000'))).toBeCloseTo(21, 5);
  });

  test('a colour on itself is 1:1', () => {
    expect(contrast(fromHex('#E8A33D'), fromHex('#E8A33D'))).toBe(1);
  });

  test('three-digit hex expands the way CSS expands it', () => {
    expect(fromHex('#FA0')).toEqual(fromHex('#FFAA00'));
  });

  test('the measured case brand/README.md cites: ink-muted clears 4.5:1 where faint ink did not', () => {
    // brand/README.md: ink at 55% alpha blends to #7F7C74 on paper and
    // lands at 3.73:1, which is why ink-muted exists. Pinned so the
    // arithmetic and the README cannot quietly disagree.
    expect(contrast(fromHex('#7F7C74'), fromHex(colors.paper.light))).toBeCloseTo(3.73, 2);
    expect(contrast(fromHex(colors['ink-muted'].light), fromHex(colors.paper.light))).toBeGreaterThanOrEqual(4.5);
  });
});
