// The window layout pass (docs/superpowers/specs/2026-09-29-window-layout-pass-design.md):
// the page fills the window, the brads sit near its left edge, Read gives
// the scene index a slot of its own, and Settings gives the preview the
// room. These read the stylesheets rule by rule, with comments stripped, so
// a selector or a value named in a comment can never satisfy a test.
//
// Where things land (the brads at 44, the block's edges and cap, Convert's
// 820 column, Read's 218 slot and 706 script, Settings' 420 column and its
// pinned preview, the folds at 900) is MEASURED in headless Chrome by
// tests/desktop-layout-measured.test.ts. What stays here is what that test
// cannot see: the tokens, the exact fold line, the narrow-window and
// short-window rules, states it does not visit, the brand's copy of the
// frame, colours, wrapping and [hidden]. These also run where Chrome does
// not (Linux CI).
import { describe, expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';

const REPO = join(import.meta.dir, '..');
const UI = join(REPO, 'desktop', 'ui');

function css(path: string): string {
  return readFileSync(path, 'utf8').replace(/\/\*[\s\S]*?\*\//g, '');
}
const windowCss = (name: string) => css(join(UI, name));

/** The declaration block for a selector's own (unindented, top-level) rule,
 *  never a same-named rule nested inside an @media block: those are always
 *  indented in these files, so anchoring the match to a line start finds
 *  the base rule even when a media block using the same selector sits
 *  earlier in the file. */
function ruleBlock(sheet: string, selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const match = new RegExp(`^${escaped} \\{`, 'm').exec(sheet);
  expect(match, `${selector} is missing`).not.toBeNull();
  const at = match!.index;
  return sheet.slice(at, sheet.indexOf('}', at) + 1);
}

/** Every @media block for one query, joined, braces balanced. */
function mediaBlocks(sheet: string, query: string, rule = '@media'): string {
  const out: string[] = [];
  const head = `${rule} ${query} {`;
  let at = sheet.indexOf(head);
  while (at !== -1) {
    let i = sheet.indexOf('{', at);
    const start = i;
    let depth = 0;
    for (; i < sheet.length; i++) {
      if (sheet[i] === '{') depth++;
      else if (sheet[i] === '}' && --depth === 0) break;
    }
    out.push(sheet.slice(start, i + 1));
    at = sheet.indexOf(head, i);
  }
  expect(out.length, `no ${head} block`).toBeGreaterThan(0);
  return out.join('\n');
}

/** The content block width below which Read and Settings fold to one
 *  column. A container query on the sheet, not a media query on the window:
 *  the block is the window less 148px of side margins, so a window-width
 *  breakpoint left Settings two-column with a ~300px preview between 900 and
 *  about 1050 wide. 880 = 420 (settings) + 32 (gap, --space-7) + 428, so the
 *  preview is never under 420 while it sits beside the settings. */
const FOLD = 880;
const NARROW = `(max-width: ${FOLD - 1}px)`;
const containerBlocks = (sheet: string, query: string) => mediaBlocks(sheet, query, '@container');

describe('the two-column surfaces fold on the block, not the window', () => {
  const style = windowCss('style.css');
  const surfaces = windowCss('surfaces.css');

  test('the sheet is the query container', () => {
    expect(ruleBlock(style, '.sheet')).toContain('container-type: inline-size');
  });

  test('no surface folds on the window width any more', () => {
    expect(surfaces).not.toMatch(/@media[^{]*\((max|min)-width: 90[01]px\)/);
  });

  test('Read and Settings fold on the 880 line, so two-column Settings never gives the preview less than 420', () => {
    // The measured test sees a 1132 block two-column and a 752 one folded;
    // only this pins where between them the line falls.
    const settings = 420;
    const gap = 32; // --space-7, 2rem
    expect(FOLD - settings - gap).toBeGreaterThanOrEqual(420);
    const narrow = containerBlocks(surfaces, NARROW);
    expect(narrow).toContain('.tune-split { grid-template-columns: 1fr; }');
    expect(narrow).toContain('.reader { grid-template-columns: minmax(0, 1fr); }');
  });

  test('Convert’s result folds on the same line: the panel first, the page under it', () => {
    // The measured test sees both sides of the line; this pins where it is.
    expect(containerBlocks(surfaces, NARROW)).toContain('grid-template-areas: "side" "page";');
  });
});

describe('Convert keeps its 820 column for the drop well, and the result takes the block', () => {
  // The measured test draws the result, never the idle well, so the well's
  // column is held here.
  const surfaces = windowCss('surfaces.css');

  test('the well (and every state but the result) keeps a column of at most 820', () => {
    expect(ruleBlock(surfaces, '#surface-convert')).toContain('max-width: 820px');
  });

  test('the result lets go of it', () => {
    expect(ruleBlock(surfaces, '#surface-convert[data-state="done"]')).toContain('max-width: none');
  });
});

describe('the page fills the window (frame B)', () => {
  const brand = css(join(REPO, 'brand', 'tokens.css'));
  const style = windowCss('style.css');

  test('the frame is measured in pixels from the window, not as shares of a capped page', () => {
    expect(brand).toContain('--block-max: 1180px;');
    expect(brand).toContain('--binding-margin: 100px;');
    expect(brand).toContain('--page-right: 48px;');
    expect(brand).toContain('--hole-center: 44px;');
    expect(brand).not.toContain('--page-max');
  });

  test('nothing in the window still names the old page cap', () => {
    for (const name of ['style.css', 'surfaces.css', 'tokens.css']) {
      expect(`${name}: ${windowCss(name).includes('--page-max')}`).toBe(`${name}: false`);
    }
  });

  test('the dead-engine line sits on the block’s right edge', () => {
    // The measured test sees the foot; the window never shows this line
    // there, because its engine stand-in never dies.
    expect(ruleBlock(style, '.engine-fault')).toMatch(/right:\s*0;/);
  });

  test('a narrow window narrows the binding on the page, as before', () => {
    const narrow = mediaBlocks(style, '(max-width: 720px)');
    expect(narrow).toContain('.page { padding-left: 11%; padding-right: var(--space-4); }');
    expect(narrow).not.toContain('.sheet {');
  });

  test('on a narrow window the brads sit in the middle of the narrow binding', () => {
    // Under 720 the binding is 11% of the width; a fixed 44px would put the
    // 20px brads on the text below about 490px wide.
    expect(mediaBlocks(style, '(max-width: 720px)')).toContain('.rail svg { left: 5.5%; width: 20px; height: 20px; }');
  });
});

describe('the brand draws the same frame the window uses', () => {
  const frame = readFileSync(join(REPO, 'brand', 'components', 'page-frame.html'), 'utf8');
  const preview = css(join(REPO, 'brand', 'components', '_preview.css'));

  test('the frame card’s rail spans the window and is not centred on a capped page', () => {
    expect(frame).not.toContain('--page-max');
    expect(frame).not.toContain('translateX(-50%)');
    expect(frame).toMatch(/\.rail \{\s*position: fixed; top: 0; bottom: 0; left: 0; right: 0;/);
  });

  test('the component previews keep a page-sized card made of the new tokens', () => {
    expect(preview).not.toContain('--page-max');
    expect(ruleBlock(preview, '.page'))
      .toContain('max-width: calc(var(--block-max) + var(--binding-margin) + var(--page-right))');
  });
});

describe('Read gives the scene index a slot of its own', () => {
  const surfaces = windowCss('surfaces.css');

  // The measured test opens the index (the window's default) and measures
  // it; these guard the SHUT index, which it never draws.
  test('the index keeps its slot open or shut, so the script never moves', () => {
    const rail = ruleBlock(surfaces, '.scene-rail');
    expect(rail).toContain('grid-column: 1');
    expect(rail).not.toContain('position: absolute');
    expect(rail).not.toContain('width: min(');
    expect(ruleBlock(surfaces, '.script-stage')).toContain('grid-column: 2');
  });

  test('open, it casts no shadow over the page', () => {
    expect(ruleBlock(surfaces, '.reader.index-open .scene-rail')).not.toContain('box-shadow');
  });

  test('in a narrow block a shut index takes no room above the script', () => {
    expect(containerBlocks(surfaces, NARROW)).toContain('.reader:not(.index-open) .scene-rail { display: none; }');
  });

  test('a short window only grows the rail when it is still beside the script, not stacked above it', () => {
    // In a narrow block the rail is already a short strip (the narrow
    // container block caps it at 6.5rem); a short-window override meant for
    // the two-column layout must not also apply there.
    const short = mediaBlocks(surfaces, '(max-height: 560px)');
    expect(short).toContain(`@container (min-width: ${FOLD}px) {`);
    expect(containerBlocks(short, `(min-width: ${FOLD}px)`)).toContain('.scene-rail { max-height: 62vh; }');
  });
});

describe('each Settings section is a box', () => {
  const surfaces = windowCss('surfaces.css');
  const colors = JSON.parse(readFileSync(join(REPO, 'brand', 'tokens.json'), 'utf8')).colors;

  /** WCAG 2 relative luminance of a #RRGGBB colour. */
  function luminance(hex: string): number {
    const [r, g, b] = [1, 3, 5]
      .map((i) => parseInt(hex.slice(i, i + 2), 16) / 255)
      .map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
    return 0.2126 * r + 0.7152 * g + 0.0722 * b;
  }
  function contrast(a: string, b: string): number {
    const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
    return (hi + 0.05) / (lo + 0.05);
  }

  test('each group and the foot share one box: a hairline, round corners, the panel fill', () => {
    const box = ruleBlock(surfaces, '.knob-group, .tune-foot');
    expect(box).toContain('border: 1px solid var(--hole)');
    expect(box).toContain('border-radius: var(--radius-well)');
    expect(box).toContain('background: var(--panel)');
  });

  test('the foot is one element, so it can be one box', () => {
    const tune = readFileSync(join(UI, 'tune.js'), 'utf8');
    expect(tune).toMatch(/el\('div', \{ class: 'tune-foot' \},\s*drawDefaultsFoot\(\),\s*drawKeepChoice\(\),\s*drawPageSwitches\(\)\)/);
  });

  test('in a narrow block the boxes stay a measure wide, not the whole column', () => {
    const narrow = containerBlocks(surfaces, NARROW);
    expect(narrow).toContain('.knob-group, .tune-foot { max-width: var(--measure); }');
  });

  test('the panel is a colour token in both modes, and every text on it still reads', () => {
    expect(colors.panel?.light).toMatch(/^#[0-9A-F]{6}$/);
    expect(colors.panel?.dark).toMatch(/^#[0-9A-F]{6}$/);
    for (const mode of ['light', 'dark'] as const) {
      for (const ink of ['ink', 'ink-soft', 'ink-muted']) {
        expect(
          `${ink} on panel (${mode}): ${contrast(colors[ink][mode], colors.panel[mode]) >= 4.5}`,
        ).toBe(`${ink} on panel (${mode}): true`);
      }
    }
  });
});

describe('a long unbroken line wraps inside its box', () => {
  // A library path, a script title with no spaces, or an engine sentence
  // quoting a long file path ran straight out of its box: nothing let the
  // line break inside a word.
  const style = windowCss('style.css');
  const surfaces = windowCss('surfaces.css');

  test('every box that prints a path, a title or an engine sentence may break anywhere', () => {
    for (const [sheet, selector] of [
      [surfaces, '.caption'], [surfaces, '.well-ask'], [surfaces, '.result-title'],
      [surfaces, '.fault-body'], [surfaces, '.read-title'], [style, '.engine-fault'],
    ] as const) {
      expect(`${selector}: ${ruleBlock(sheet, selector).includes('overflow-wrap: anywhere')}`)
        .toBe(`${selector}: true`);
    }
  });

  test('the library line’s words may shrink inside the flex row they sit in', () => {
    // A flex item will not go narrower than its longest word unless told it may.
    expect(ruleBlock(surfaces, '.well-ask > *')).toContain('min-width: 0');
  });
});

describe('a hidden element is hidden, whatever its class says about display', () => {
  // .btn { display: inline-block } outranked the browser's own
  // [hidden] { display: none }, so the release notes' hidden Install button
  // drew as an empty brass pill anyone could click. One global rule, marked
  // important so no class rule can beat it, replaces the per-class patches.
  const style = windowCss('style.css');
  const surfaces = windowCss('surfaces.css');

  test('style.css hides every [hidden] element, and nothing can outrank it', () => {
    expect(style).toMatch(/^\[hidden\] \{ display: none !important; \}/m);
  });

  test('no stylesheet keeps a per-class [hidden] patch the global rule covers', () => {
    for (const [name, sheet] of [['style.css', style], ['surfaces.css', surfaces]]) {
      const patches = [...sheet.matchAll(/^[^\n{]*\S\[hidden\][^{]*\{/gm)].map((m) => m[0].trim());
      expect(`${name}: ${patches.join(', ')}`).toBe(`${name}: `);
    }
  });

  test('the notes Install button that started this is a .btn built hidden', () => {
    const notes = readFileSync(join(UI, 'notes-surface.js'), 'utf8');
    expect(notes).toContain("el('button', { type: 'button', class: 'btn btn-brad btn-small', hidden: true }");
    expect(ruleBlock(surfaces, '.btn')).toContain('display: inline-block');
  });
});
