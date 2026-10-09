import { describe, test, expect } from 'bun:test';
import { Fountain } from 'fountain-js';
import JSZip from 'jszip';
import { DEFAULT_FORMAT_OPTIONS, resolveFormatOptions, type FormatOptions } from '../src/options';
import { screenplayCss } from '../src/epub/css';
import { ruleFor, eachRule } from './css-rules';
import { tokensToBody } from '../src/epub/html';
import { buildEpub } from '../src/epub/build';
import { toFountain } from '../src/fountain/serialize';
import { convertFountain } from '../src/convert';
import type { ScreenplayElement, ParsedScreenplay } from '../src/parser/types';

const SAMPLE = `INT. KITCHEN - DAY #7#

Jack enters.

@JACK
Hello.

EXT. YARD - NIGHT

Stars.
`;

function tokens() {
  return new Fountain().parse(SAMPLE, true).tokens;
}

// ── resolveFormatOptions ─────────────────────────────────

describe('resolveFormatOptions', () => {
  test('empty input yields defaults', () => {
    expect(resolveFormatOptions({})).toEqual(DEFAULT_FORMAT_OPTIONS);
    expect(resolveFormatOptions(undefined)).toEqual(DEFAULT_FORMAT_OPTIONS);
  });

  test('defaults match the canonical format-defaults.json', async () => {
    // The one pin on this file since F3. Until then the Swift app's own
    // checks decoded the same file and compared it to the Swift defaults,
    // so the two languages could not drift silently. With the Swift app
    // gone, options.ts and this file are the only two copies, and this is
    // what keeps them one.
    const canonical = await Bun.file(new URL('../format-defaults.json', import.meta.url)).json();
    expect(DEFAULT_FORMAT_OPTIONS).toEqual(canonical);
    expect(canonical.includeImages).toBe(true);
    expect(canonical.promoteCoverImage).toBe(true);
  });

  test('numeric knobs are clamped to sane ranges', () => {
    const opts = resolveFormatOptions({
      dialogueSideMarginPct: 90,
      cueIndentPct: -5,
      elementSpacingEm: 99,
    });
    expect(opts.dialogueSideMarginPct).toBeLessThanOrEqual(30);
    expect(opts.cueIndentPct).toBeGreaterThanOrEqual(0);
    expect(opts.elementSpacingEm).toBeLessThanOrEqual(2);
  });

  test('unknown keys are ignored', () => {
    const opts = resolveFormatOptions({ nonsense: true } as Record<string, unknown>);
    expect(opts).toEqual(DEFAULT_FORMAT_OPTIONS);
  });

  test('justifyText defaults to false and accepts a boolean', () => {
    expect(resolveFormatOptions({}).justifyText).toBe(false);
    expect(resolveFormatOptions({ justifyText: true }).justifyText).toBe(true);
    expect(resolveFormatOptions({ justifyText: 'nope' } as Record<string, unknown>).justifyText).toBe(false);
  });

  test('printSplitMinimums defaults to true and accepts a boolean', () => {
    expect(resolveFormatOptions({}).printSplitMinimums).toBe(true);
    expect(resolveFormatOptions({ printSplitMinimums: false }).printSplitMinimums).toBe(false);
    // A falsy invalid value: stays true under the correct type-guarded
    // fallback, but would flip to false under a `Boolean(p[key])`
    // regression — a truthy invalid value like 'no' can't catch that.
    expect(
      resolveFormatOptions({ printSplitMinimums: 0 } as Record<string, unknown>).printSplitMinimums,
    ).toBe(true);
  });

  test('preserveFontShifts defaults to true and accepts a boolean', () => {
    expect(resolveFormatOptions({}).preserveFontShifts).toBe(true);
    expect(resolveFormatOptions({ preserveFontShifts: false }).preserveFontShifts).toBe(false);
    // Falsy-but-invalid, per the neighbour above: a `Boolean(p[key])`
    // regression would flip this to false.
    expect(
      resolveFormatOptions({ preserveFontShifts: 0 } as Record<string, unknown>).preserveFontShifts,
    ).toBe(true);
  });

  test('includeImages defaults to true and accepts a boolean', () => {
    expect(resolveFormatOptions({}).includeImages).toBe(true);
    expect(resolveFormatOptions({ includeImages: false }).includeImages).toBe(false);
    // Falsy-but-invalid, per the two above: a `Boolean(p[key])` regression
    // would turn every script's pictures off on a stray 0.
    expect(
      resolveFormatOptions({ includeImages: 0 } as Record<string, unknown>).includeImages,
    ).toBe(true);
  });

  test('promoteCoverImage defaults to true and accepts a boolean', () => {
    expect(resolveFormatOptions({}).promoteCoverImage).toBe(true);
    expect(resolveFormatOptions({ promoteCoverImage: false }).promoteCoverImage).toBe(false);
    // Falsy-but-invalid, as for its neighbours: a `Boolean(p[key])`
    // regression would take every title card off its cover on a stray 0.
    expect(
      resolveFormatOptions({ promoteCoverImage: 0 } as Record<string, unknown>).promoteCoverImage,
    ).toBe(true);
  });

  test('resolveFormatOptions merges over a supplied base, not just the defaults', () => {
    const base: FormatOptions = { ...DEFAULT_FORMAT_OPTIONS, keepSpeechesWhole: true, justifyText: true };
    const merged = resolveFormatOptions({ dialogueSideMarginPct: 9 }, base);
    expect(merged.dialogueSideMarginPct).toBe(9);
    // fields absent from the partial come from the BASE, not from the defaults
    expect(merged.keepSpeechesWhole).toBe(true);
    expect(merged.justifyText).toBe(true);
  });

  test('resolveFormatOptions still defaults its base to DEFAULT_FORMAT_OPTIONS', () => {
    expect(resolveFormatOptions({})).toEqual(DEFAULT_FORMAT_OPTIONS);
  });

  test('an out-of-range value clamps against the base rather than being taken raw', () => {
    const base: FormatOptions = { ...DEFAULT_FORMAT_OPTIONS, dialogueSideMarginPct: 12 };
    expect(resolveFormatOptions({ dialogueSideMarginPct: 999 }, base).dialogueSideMarginPct).toBe(30);
    expect(resolveFormatOptions({ dialogueSideMarginPct: 'nope' }, base).dialogueSideMarginPct).toBe(12);
  });

  // dualDialogue used to be merged as `p.dualDialogue === 'sequential' ?
  // 'sequential' : d.dualDialogue`, which reads an explicit 'sideBySide' the
  // same as an absent or invalid value: it always fell through to the base.
  // A base of 'sequential' (a user's own app defaults, the Phone preset
  // saved as their starting point) then could never be moved back to
  // 'sideBySide' by anything downstream, no matter how explicitly asked.
  test('an explicit sideBySide overrides a sequential base', () => {
    const base: FormatOptions = { ...DEFAULT_FORMAT_OPTIONS, dualDialogue: 'sequential' };
    expect(resolveFormatOptions({ dualDialogue: 'sideBySide' }, base).dualDialogue).toBe('sideBySide');
  });

  test('an explicit sequential still overrides a sideBySide base', () => {
    const base: FormatOptions = { ...DEFAULT_FORMAT_OPTIONS, dualDialogue: 'sideBySide' };
    expect(resolveFormatOptions({ dualDialogue: 'sequential' }, base).dualDialogue).toBe('sequential');
  });

  test('an invalid dualDialogue value falls back to the base, same as the other enums', () => {
    const base: FormatOptions = { ...DEFAULT_FORMAT_OPTIONS, dualDialogue: 'sequential' };
    expect(resolveFormatOptions({ dualDialogue: 'nonsense' }, base).dualDialogue).toBe('sequential');
  });
});

// ── screenplayCss(options) ───────────────────────────────

describe('screenplayCss with options', () => {
  test('dialogue geometry knobs land in the stylesheet', () => {
    const css = screenplayCss(resolveFormatOptions({
      cueAlignment: 'indented',
      dialogueSideMarginPct: 25,
      cueIndentPct: 40,
      parentheticalIndentPct: 12,
    }));
    expect(css).toContain('margin-left: 25%');
    expect(css).toContain('margin-right: 25%');
    expect(ruleFor(css, 'p.character')).toContain('margin-left: 40%');
    expect(ruleFor(css, 'p.parenthetical')).toContain('margin-left: 12%');
  });

  test('element spacing scales the vertical rhythm', () => {
    const css = screenplayCss(resolveFormatOptions({ elementSpacingEm: 0.6 }));
    expect(ruleFor(css, 'p.action')).toContain('margin: 0.6em 0');
  });

  test('scene page breaks add a break rule only when enabled', () => {
    const off = screenplayCss(resolveFormatOptions({}));
    expect(off).not.toContain('section.scene { page-break-before');
    const on = screenplayCss(resolveFormatOptions({ scenePageBreaks: true }));
    expect(on).toContain('section.scene { page-break-before: always; }');
  });

  test('keepSpeechesWhole makes dialogue blocks unbreakable only when enabled', () => {
    const off = screenplayCss(resolveFormatOptions({}));
    expect(ruleFor(off, '.dialogue-block')).not.toContain('break-inside');
    const on = screenplayCss(resolveFormatOptions({ keepSpeechesWhole: true }));
    const block = ruleFor(on, '.dialogue-block');
    expect(block).toContain('page-break-inside: avoid');
    expect(block).toContain('break-inside: avoid');
  });

  test('font family option switches the body stack', () => {
    expect(screenplayCss(resolveFormatOptions({ fontFamily: 'serif' }))).toMatch(/body\s*{[^}]*font-family:\s*serif/);
    expect(screenplayCss(resolveFormatOptions({ fontFamily: 'courier' }))).toContain('"Courier Prime"');
  });

  test('body text is ragged-right (left-aligned) by default, justified when enabled', () => {
    const ragged = screenplayCss(resolveFormatOptions({}));
    expect(ruleFor(ragged, 'p.action')).toContain('text-align: left');
    expect(ruleFor(ragged, 'p.dialogue')).toContain('text-align: left');

    const justified = screenplayCss(resolveFormatOptions({ justifyText: true }));
    expect(ruleFor(justified, 'p.action')).toContain('text-align: justify');
    expect(ruleFor(justified, 'p.dialogue')).toContain('text-align: justify');
  });

  test('printSplitMinimums controls widows/orphans on dialogue and action', () => {
    // Loop over both states and both selectors so drift between the two
    // hand-copied rule bodies (the likeliest regression) can't hide in an
    // untested corner. The trailing semicolon keeps `widows: 2` from
    // matching `widows: 20`.
    for (const [partial, n] of [[{}, 2], [{ printSplitMinimums: false }, 1]] as const) {
      const css = screenplayCss(resolveFormatOptions(partial));
      for (const sel of ['p.action', 'p.dialogue']) {
        const rule = ruleFor(css, sel);
        expect(rule).toContain(`widows: ${n};`);
        expect(rule).toContain(`orphans: ${n};`);
      }
    }
  });

  test('wrapper keeps carry the column spelling in a separate rule', () => {
    const css = screenplayCss(DEFAULT_FORMAT_OPTIONS);
    // The shadow shares its selector with the rule it shadows and is
    // emitted second, so address it by position rather than by name.
    const both = eachRule(css).filter((r) => r.selector === 'table.dual-dialogue');
    expect(both).toHaveLength(2);
    expect(both[1]!.body).toContain('-webkit-column-break-inside: avoid');
    // iBooks bug: the column spelling must not share a declaration block
    // with page-break-inside, or Books ignores BOTH.
    expect(both[1]!.body).not.toContain('page-break-inside');
  });

  test('the column spelling tracks the keep set: keepSpeechesWhole joins it', () => {
    // Apple Books honors ONLY the old spelling, so a keep that is missing
    // from this rule is inert there. The selector list is derived from the
    // same gating that emits the keeps, not hand-maintained beside it.
    const off = screenplayCss(resolveFormatOptions({}));
    expect(off).toContain('table.dual-dialogue { -webkit-column-break-inside: avoid; }');
    expect(off).not.toContain('.dialogue-block { -webkit-column-break-inside');

    const on = screenplayCss(resolveFormatOptions({ keepSpeechesWhole: true }));
    const rule = ruleFor(on, 'table.dual-dialogue, .dialogue-block');
    expect(rule).toContain('-webkit-column-break-inside: avoid');
    // Still its own declaration block, per the iBooks bug above.
    expect(rule).not.toContain('page-break-inside');
  });

  test('page marker dims via opacity, not a hardcoded gray', () => {
    const css = screenplayCss(DEFAULT_FORMAT_OPTIONS);
    const rule = ruleFor(css, 'span.page-marker');
    expect(rule).toContain('opacity: 0.6');
    expect(rule).not.toContain('#777777');
  });
});

// ── tokensToBody(options) ────────────────────────────────

describe('tokensToBody with options', () => {
  test('keepSceneHeadingWithScene gates the heading break-after rule', () => {
    const heading = ruleFor(screenplayCss(resolveFormatOptions({})), 'h2.scene-heading');
    // both spellings: the legacy prefixed property AND the modern standalone
    expect(heading).toContain('page-break-after: avoid');
    expect(heading).toMatch(/[^-]break-after: avoid/);
    const off = screenplayCss(resolveFormatOptions({ keepSceneHeadingWithScene: false }));
    expect(ruleFor(off, 'h2.scene-heading')).not.toContain('break-after');
    // markup carries no wrapper either way
    for (const format of [resolveFormatOptions({}), resolveFormatOptions({ keepSceneHeadingWithScene: false })]) {
      expect(tokensToBody(tokens(), { format }).files[0].xhtml).not.toMatch(/<div class="keep-together">\s*<h2/);
    }
  });

  test('scene numbers render in headings when enabled', () => {
    const on = tokensToBody(tokens(), { format: resolveFormatOptions({ showSceneNumbers: true }) });
    expect(on.files[0].xhtml).toContain('<h2 class="scene-heading"><span class="scene-number">7</span> INT. KITCHEN - DAY</h2>');
    const off = tokensToBody(tokens(), { format: resolveFormatOptions({}) });
    expect(off.files[0].xhtml).toContain('<h2 class="scene-heading">INT. KITCHEN - DAY</h2>');
  });
});

// ── buildEpub(options) ───────────────────────────────────

describe('buildEpub with options', () => {
  test('title page can be excluded from manifest, spine, and landmarks', async () => {
    const body = tokensToBody(tokens());
    const bytes = await buildEpub({ title: 'T' }, body, resolveFormatOptions({ includeTitlePage: false }));
    const zip = await JSZip.loadAsync(bytes);
    expect(zip.file('OEBPS/titlepage.xhtml')).toBeNull();
    const opf = await zip.file('OEBPS/package.opf')!.async('string');
    expect(opf).not.toContain('titlepage');
    const nav = await zip.file('OEBPS/nav.xhtml')!.async('string');
    expect(nav).not.toContain('titlepage');
  });
});

// ── serializer rejoin option ─────────────────────────────

describe('toFountain rejoin option', () => {
  let id = 0;
  const el = (over: Partial<ScreenplayElement> & { text: string; type: ScreenplayElement['type'] }): ScreenplayElement => ({
    id: `o${id++}`, pageNum: 1, isTitlePage: false, isReadable: true, ...over,
  });
  const sp = (elements: ScreenplayElement[]): ParsedScreenplay => ({ elements, characters: [], scenes: [], pageCount: 1 });

  const SPLIT = [
    el({ text: 'JACK', type: 'character', character: 'JACK' }),
    el({ text: 'First half', type: 'dialogue', character: 'JACK' }),
    el({ text: "JACK (CONT'D)", type: 'character', character: 'JACK' }),
    el({ text: 'second half.', type: 'dialogue', character: 'JACK' }),
  ];

  test('rejoin on (default) merges the split speech', () => {
    expect(toFountain(sp(SPLIT))).toBe('@JACK\nFirst half\nsecond half.\n');
  });

  test('rejoin off keeps the (CONT’D) cue', () => {
    const out = toFountain(sp(SPLIT), undefined, resolveFormatOptions({ rejoinSplitDialogue: false }));
    expect(out).toBe("@JACK\nFirst half\n\n@JACK (CONT'D)\nsecond half.\n");
  });
});

// ── end-to-end through convertFountain ───────────────────

describe('convertFountain carries format options into the EPUB', () => {
  test('custom geometry reaches style.css', async () => {
    const result = await convertFountain('INT. A - DAY\n\nText.\n', {
      format: { dialogueSideMarginPct: 28, scenePageBreaks: true },
    });
    const zip = await JSZip.loadAsync(result.epub);
    const css = await zip.file('OEBPS/style.css')!.async('string');
    expect(css).toContain('margin-left: 28%');
    expect(css).toContain('section.scene { page-break-before: always; }');
  });
});

// ── CONT'D modes (registry #8a) ──────────────────────────

describe("contdMode", () => {
  let id2 = 0;
  const el2 = (over: Partial<ScreenplayElement> & { text: string; type: ScreenplayElement['type'] }): ScreenplayElement => ({
    id: `c${id2++}`, pageNum: 1, isTitlePage: false, isReadable: true, ...over,
  });
  const sp2 = (elements: ScreenplayElement[]): ParsedScreenplay => ({ elements, characters: [], scenes: [], pageCount: 1 });

  const INTERRUPTED = [
    el2({ text: 'INT. LAB - DAY', type: 'scene' }),
    el2({ text: 'JACK', type: 'character', character: 'JACK' }),
    el2({ text: 'Look at this.', type: 'dialogue', character: 'JACK' }),
    el2({ text: 'He points.', type: 'action' }),
    el2({ text: 'JACK', type: 'character', character: 'JACK' }),
    el2({ text: 'See?', type: 'dialogue', character: 'JACK' }),
  ];

  test('auto adds (CONT’D) when the same speaker continues through action', () => {
    const out = toFountain(sp2(INTERRUPTED), undefined, resolveFormatOptions({ contdMode: 'auto' }));
    expect(out).toContain("@JACK (CONT'D)\nSee?");
  });

  test('auto strips stale (CONT’D) after a different speaker', () => {
    const out = toFountain(sp2([
      el2({ text: 'DEV', type: 'character', character: 'DEV' }),
      el2({ text: 'Hi.', type: 'dialogue', character: 'DEV' }),
      el2({ text: "JACK (CONT'D)", type: 'character', character: 'JACK' }),
      el2({ text: 'Hello.', type: 'dialogue', character: 'JACK' }),
    ]), undefined, resolveFormatOptions({ contdMode: 'auto' }));
    expect(out).toContain('@JACK\nHello.');
    expect(out).not.toContain("JACK (CONT'D)");
  });

  test('auto resets at scene boundaries', () => {
    const out = toFountain(sp2([
      el2({ text: 'JACK', type: 'character', character: 'JACK' }),
      el2({ text: 'Bye.', type: 'dialogue', character: 'JACK' }),
      el2({ text: 'INT. HALL - DAY', type: 'scene' }),
      el2({ text: 'JACK', type: 'character', character: 'JACK' }),
      el2({ text: 'New scene.', type: 'dialogue', character: 'JACK' }),
    ]), undefined, resolveFormatOptions({ contdMode: 'auto' }));
    expect(out).toContain('@JACK\nNew scene.');
  });

  test('strip removes all (CONT’D) and adds none', () => {
    const withContd = [...INTERRUPTED];
    withContd[4] = el2({ text: "JACK (CONT'D)", type: 'character', character: 'JACK' });
    const out = toFountain(sp2(withContd), undefined, resolveFormatOptions({ contdMode: 'strip' }));
    expect(out).not.toContain("CONT'D");
  });

  test('keep leaves source cues untouched', () => {
    const out = toFountain(sp2(INTERRUPTED), undefined, resolveFormatOptions({ contdMode: 'keep' }));
    expect(out).toContain('@JACK\nSee?');
  });
});

// ── cue alignment (registry #2b) ─────────────────────────

describe('cueAlignment', () => {
  test('default centers cues and parentheticals in the column', () => {
    const css = screenplayCss(resolveFormatOptions({}));
    expect(ruleFor(css, 'p.character')).toContain('text-align: center');
    expect(ruleFor(css, 'p.parenthetical')).toContain('text-align: center');
  });

  test('indented mode uses print-style % indents instead', () => {
    const css = screenplayCss(resolveFormatOptions({ cueAlignment: 'indented', cueIndentPct: 40 }));
    const cue = ruleFor(css, 'p.character');
    expect(cue).toContain('margin-left: 40%');
    expect(cue).not.toContain('text-align: center');
  });
});

// ── original page markers (registry #13a) ────────────────

describe('showPageMarkers', () => {
  let id3 = 0;
  const el3 = (over: Partial<ScreenplayElement> & { text: string; type: ScreenplayElement['type']; pageNum: number }): ScreenplayElement => ({
    id: `p${id3++}`, isTitlePage: false, isReadable: true, ...over,
  });
  const sp3 = (elements: ScreenplayElement[]): ParsedScreenplay => ({ elements, characters: [], scenes: [], pageCount: 3 });

  // Sheets 1-3; printed page numbers "1."/"2." on sheets 2-3 (title page
  // offsets the count by one, the standard screenplay situation).
  const ELS = [
    el3({ text: 'INT. LAB - DAY', type: 'scene', pageNum: 1 }),
    el3({ text: 'Work happens.', type: 'action', pageNum: 1 }),
    el3({ text: '1.', type: 'page-number', pageNum: 2 }),
    el3({ text: 'More work.', type: 'action', pageNum: 2 }),
    el3({ text: '2.', type: 'page-number', pageNum: 3 }),
    el3({ text: 'Even more.', type: 'action', pageNum: 3 }),
  ];

  test('on by default — a note about page 42 can still be found', () => {
    // Flipped 2026-09-21. Page numbers are navigation: a script gets notes
    // against printed page numbers, and a book that dropped them made every
    // one of those notes unfindable. Nothing else in the book carries the
    // printed pagination, so this was the only way back to it.
    //
    // showSceneNumbers was deliberately NOT flipped with it. A scene number
    // is an intentional property of a draft, present or absent because a
    // writer decided, and the app has no business inventing one.
    const out = toFountain(sp3(ELS));
    expect(out).toContain('= pg 1\n\nMore work.');
    expect(out).toContain('= pg 2\n\nEven more.');
  });

  test('and can still be turned off', () => {
    const out = toFountain(sp3(ELS), undefined, resolveFormatOptions({ showPageMarkers: false }));
    expect(out).not.toContain('= pg');
  });

  test('on: markers carry the PDF-printed numbering, not the sheet index', () => {
    const out = toFountain(sp3(ELS), undefined, resolveFormatOptions({ showPageMarkers: true }));
    expect(out).toContain('= pg 1\n\nMore work.');
    expect(out).toContain('= pg 2\n\nEven more.');
  });

  test('a page starting mid-speech defers its marker to the next block', () => {
    const els = [
      el3({ text: '5.', type: 'page-number', pageNum: 5 }),
      el3({ text: 'Setup.', type: 'action', pageNum: 5 }),
      el3({ text: 'JACK', type: 'character', character: 'JACK', pageNum: 5 }),
      el3({ text: 'Speech starts here', type: 'dialogue', character: 'JACK', pageNum: 5 }),
      el3({ text: 'and continues overleaf.', type: 'dialogue', character: 'JACK', pageNum: 6 }),
      el3({ text: 'He exits.', type: 'action', pageNum: 6 }),
    ];
    const out = toFountain(sp3(els), undefined, resolveFormatOptions({ showPageMarkers: true }));
    expect(out).toContain('Speech starts here\nand continues overleaf.');
    expect(out).toContain('= pg 6\n\nHe exits.');
  });
});

// ── dual dialogue rendering mode (registry #10a) ─────────

describe('dualDialogue mode', () => {
  let id4 = 0;
  const el4 = (over: Partial<ScreenplayElement> & { text: string; type: ScreenplayElement['type'] }): ScreenplayElement => ({
    id: `d${id4++}`, pageNum: 1, isTitlePage: false, isReadable: true, ...over,
  });
  const DUAL = [
    el4({ text: 'VERA', type: 'character', character: 'VERA' }),
    el4({ text: 'Read me the last page--', type: 'dialogue', character: 'VERA' }),
    el4({ text: 'INFORMANT', type: 'character', character: 'INFORMANT', dualRight: true }),
    el4({ text: '--the last page burned.', type: 'dialogue', character: 'INFORMANT' }),
  ];
  const sp4 = (): ParsedScreenplay => ({ elements: DUAL, characters: [], scenes: [], pageCount: 1 });

  test('sideBySide (default) emits the fountain caret on the right cue', () => {
    const out = toFountain(sp4());
    expect(out).toContain('@INFORMANT ^');
  });

  test('sequential mode omits the caret', () => {
    const out = toFountain(sp4(), undefined, resolveFormatOptions({ dualDialogue: 'sequential' }));
    expect(out).toContain('@INFORMANT\n');
    expect(out).not.toContain('^');
  });
});

// ── keepSpeechesWhole option ─────────────────────────────

describe('keepSpeechesWhole option', () => {
  test('defaults off and resolves from partials', () => {
    expect(resolveFormatOptions({}).keepSpeechesWhole).toBe(false);
    expect(resolveFormatOptions({ keepSpeechesWhole: true }).keepSpeechesWhole).toBe(true);
    expect(resolveFormatOptions({ keepSpeechesWhole: 'yes' }).keepSpeechesWhole).toBe(false);
  });
});
