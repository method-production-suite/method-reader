import { describe, test, expect } from 'bun:test';
import { readFileSync } from 'node:fs';
import { Fountain } from 'fountain-js';
import JSZip from 'jszip';
import { tokensToBody, tokensToPreviewHtml } from '../src/epub/html';
import { buildEpub } from '../src/epub/build';
import { SCREENPLAY_CSS, screenplayCss } from '../src/epub/css';
import { ruleFor, eachRule } from './css-rules';
import { DEFAULT_FORMAT_OPTIONS, resolveFormatOptions } from '../src/options';
// Stage-2's copy, straight from the module that owns it — both renderers
// import it from here, so the test reads the same definition they do.
import { PRIMARY_SLUG } from '../src/fountain/slug';
import { PRIMARY_SLUG as PARSER_PRIMARY_SLUG } from '../src/parser/classify';

const SAMPLE = `INT. KITCHEN - DAY

Jack enters, exhausted.

@JACK
(tired)
Long day. 5 < 6 & "quotes" too.

EXT. YARD - NIGHT #2#

The stars come out.

> FADE OUT.
`;

function sampleTokens() {
  return new Fountain().parse(SAMPLE, true).tokens;
}

// ── tokensToBody ─────────────────────────────────────────

describe('tokensToBody', () => {
  test('scenes flow together in a single body file — no per-scene files', () => {
    const body = tokensToBody(sampleTokens());
    expect(body.files).toHaveLength(1);
    expect(body.files[0].id).toBe('body001');
    expect(body.files[0].xhtml).toContain('INT. KITCHEN - DAY');
    expect(body.files[0].xhtml).toContain('EXT. YARD - NIGHT');
  });

  test('each scene is an anchored section for TOC navigation', () => {
    const body = tokensToBody(sampleTokens());
    expect(body.files[0].xhtml).toContain('<section class="scene" id="sc-001">');
    expect(body.files[0].xhtml).toContain('<section class="scene" id="sc-002">');
    expect(body.toc).toEqual([
      { title: 'INT. KITCHEN - DAY', href: 'text/body001.xhtml#sc-001' },
      { title: 'EXT. YARD - NIGHT', href: 'text/body001.xhtml#sc-002' },
    ]);
  });

  test('oversized scripts split into multiple files at scene boundaries', () => {
    const body = tokensToBody(sampleTokens(), { maxFileBytes: 400 });
    expect(body.files.length).toBeGreaterThan(1);
    expect(body.files.map((f) => f.id)).toEqual(['body001', 'body002']);
    // TOC follows scenes into their files
    expect(body.toc[0].href).toContain('body001.xhtml#sc-001');
    expect(body.toc[1].href).toContain('body002.xhtml#sc-002');
  });

  test('scene heading leads its scene bare — the keep is the CSS chain, not a wrapper', () => {
    const [file] = tokensToBody(sampleTokens()).files;
    expect(file.xhtml).toMatch(
      /<section class="scene" id="sc-001">\s*<h2 class="scene-heading">INT\. KITCHEN - DAY<\/h2>\s*<p class="action">Jack enters, exhausted\.<\/p>/,
    );
    expect(file.xhtml).not.toContain('keep-together');
  });

  test('a dialogue block follows its heading directly — no outer wrapper', () => {
    const tokens = new Fountain().parse('INT. CAR - DAY\n\n@DEV\nDrive.\n\nThey drive.\n', true).tokens;
    const [file] = tokensToBody(tokens).files;
    expect(file.xhtml).toMatch(
      /<h2 class="scene-heading">INT\. CAR - DAY<\/h2>\s*<div class="dialogue-block">/,
    );
    expect(file.xhtml).not.toContain('keep-together');
  });

  test('heading-only scene renders bare without error', () => {
    const tokens = new Fountain().parse('INT. VOID - DAY\n\nEXT. VOID - NIGHT\n\nStars.\n', true).tokens;
    const [file] = tokensToBody(tokens).files;
    expect(file.xhtml).toMatch(
      /<section class="scene" id="sc-001">\s*<h2 class="scene-heading">INT\. VOID - DAY<\/h2>\s*<\/section>/,
    );
  });

  test('opening content without a heading renders directly in its section', () => {
    const tokens = new Fountain().parse('Cold open action.\n\nINT. LAB - DAY\n\nWork.\n', true).tokens;
    const [file] = tokensToBody(tokens).files;
    expect(file.xhtml).toMatch(/<section class="scene" id="sc-001">\s*<p class="action">Cold open action\.<\/p>/);
  });

  test('dialogue renders inside a dialogue block with cue/paren/line classes', () => {
    const [file] = tokensToBody(sampleTokens()).files;
    expect(file.xhtml).toContain('<div class="dialogue-block">');
    expect(file.xhtml).toContain('<p class="character">JACK</p>');
    expect(file.xhtml).toContain('<p class="parenthetical">(tired)</p>');
  });

  test('special characters are XML-escaped', () => {
    const [file] = tokensToBody(sampleTokens()).files;
    expect(file.xhtml).toContain('5 &lt; 6 &amp; &quot;quotes&quot; too.');
  });

  test('transition strips the forcing prefix', () => {
    const [file] = tokensToBody(sampleTokens()).files;
    expect(file.xhtml).toContain('<p class="transition">FADE OUT.</p>');
  });

  test('content before the first scene heading becomes an Opening section', () => {
    const tokens = new Fountain().parse('Some cold-open action.\n\nINT. LAB - DAY\n\nWork.\n', true).tokens;
    const body = tokensToBody(tokens);
    expect(body.files[0].xhtml).toContain('Some cold-open action.');
    expect(body.toc[0].title).toBe('Opening');
    expect(body.toc[1].title).toBe('INT. LAB - DAY');
  });

  test('a picture note before the first heading opens no section', () => {
    // A standalone note renders nothing, so a group made only of notes is an
    // EMPTY section: an "Opening" in the nav, a blank page wherever
    // scenePageBreaks is on, and every scene anchor moved one along. The
    // whole body is compared, files and toc and page list, with and without a
    // title page, because "the book does not change" is the claim.
    const bodyOf = (src: string) => tokensToBody(new Fountain().parse(src, true).tokens);
    const note = '[[img: p001-01.png w=100 full]]\n\n';
    const script = 'INT. LAB - DAY\n\nWork.\n\nEXT. YARD - NIGHT\n\nRest.\n';
    for (const head of ['', 'Title: T\nAuthor: A\n\n']) {
      const plain = bodyOf(head + script);
      expect(bodyOf(head + note + script)).toEqual(plain);
      expect(bodyOf(head + note + note + script)).toEqual(plain);
    }
    expect(bodyOf(note + script).toc.map((e) => e.title)).toEqual(['INT. LAB - DAY', 'EXT. YARD - NIGHT']);
  });

  test('a note ahead of real opening content leaves that Opening as it was', () => {
    // Skipping the note must not swallow the cold open that follows it.
    const bodyOf = (src: string) => tokensToBody(new Fountain().parse(src, true).tokens);
    const open = 'A cold open.\n\nINT. LAB - DAY\n\nWork.\n';
    const noted = bodyOf(`[[img: p001-01.png w=100 full]]\n\n${open}`);
    expect(noted).toEqual(bodyOf(open));
    expect(noted.toc[0].title).toBe('Opening');
  });

  test('script with no scene headings yields one section', () => {
    const body = tokensToBody(new Fountain().parse('Just action.\n\nMore action.\n', true).tokens);
    expect(body.files).toHaveLength(1);
    expect(body.files[0].xhtml).toContain('Just action.');
    expect(body.toc).toHaveLength(1);
  });
});

// ── dual dialogue height fallback ────────────────────────

describe('dual dialogue height fallback', () => {
  const dualScript = (line: string) =>
    `INT. HALL - DAY\n\n@JACK\n${line}\n\n@JILL ^\nAlso talking here.\n`;

  test('a short exchange stays a side-by-side table', () => {
    const tokens = new Fountain().parse(dualScript('Quick word.'), true).tokens;
    const [file] = tokensToBody(tokens).files;
    expect(file.xhtml).toContain('<table class="dual-dialogue">');
  });

  test('a tall exchange emits sequential speeches instead of an unbreakable table', () => {
    const tokens = new Fountain().parse(dualScript('word '.repeat(120).trim()), true).tokens;
    const [file] = tokensToBody(tokens).files;
    expect(file.xhtml).not.toContain('<table class="dual-dialogue">');
    expect(file.xhtml).toContain('<p class="character">JACK</p>');
    expect(file.xhtml).toContain('<p class="character">JILL</p>');
    // each speech is a full dialogue block led by its cue, and JACK's
    // speech still precedes JILL's
    expect(file.xhtml).toMatch(
      /<div class="dialogue-block">\s*<p class="character">JACK<\/p>[\s\S]*?<div class="dialogue-block">\s*<p class="character">JILL<\/p>/,
    );
    const blocks = file.xhtml.match(/<div class="dialogue-block">/g) ?? [];
    expect(blocks.length).toBe(2);
  });

  // Measured against this script shape: the taller column costs one line for
  // the cue plus ceil((N + 1) / EST_CHARS_PER_DUAL_LINE) for the dialogue
  // (the + 1 is the cell's trailing newline), so the flip lands between 329
  // and 330. Change either constant and this pair fails.
  test('the table/sequential flip sits exactly where the constants put it', () => {
    const render = (n: number) =>
      tokensToBody(new Fountain().parse(dualScript('x'.repeat(n)), true).tokens).files[0].xhtml;
    expect(render(329)).toContain('<table class="dual-dialogue">');
    expect(render(330)).not.toContain('<table class="dual-dialogue">');
  });
});

// ── screenplay CSS geometry ──────────────────────────────

describe('screenplay CSS (Kindle-safe geometry)', () => {
  test('dialogue column narrows with % side margins — never max-width (Kindle strips it)', () => {
    const block = ruleFor(SCREENPLAY_CSS, '.dialogue-block');
    expect(block).toMatch(/margin-left:\s*\d+%/);
    expect(block).toMatch(/margin-right:\s*\d+%/);
    expect(SCREENPLAY_CSS).not.toContain('max-width');
  });

  test('cue keeps with its dialogue and centers in the column by default', () => {
    const cue = ruleFor(SCREENPLAY_CSS, 'p.character');
    expect(cue).toContain('break-after: avoid');
    expect(cue).toContain('text-align: center');
  });

  test('parenthetical centers in the column by default', () => {
    const paren = ruleFor(SCREENPLAY_CSS, 'p.parenthetical');
    expect(paren).toContain('text-align: center');
  });

  test('vertical rhythm is em-based: a full blank line between elements', () => {
    const action = ruleFor(SCREENPLAY_CSS, 'p.action');
    expect(action).toMatch(/margin:\s*1(\.\d+)?em 0/);
    const block = ruleFor(SCREENPLAY_CSS, '.dialogue-block');
    expect(block).toMatch(/margin-top:\s*1(\.\d+)?em/);
  });

  test('body text keeps device-default line height (Enhanced Typesetting owns it)', () => {
    const body = ruleFor(SCREENPLAY_CSS, 'body');
    expect(body).not.toContain('line-height');
  });

  test('keep-with-next chain covers heading, cue and parenthetical', () => {
    // The parenthetical joined the chain on 2026-09-14 when the cue keep
    // stopped being a wrapper; see 'the cue chain (no wrapper)' below for
    // the device evidence and why #5's objection no longer applies.
    const paren = ruleFor(SCREENPLAY_CSS, 'p.parenthetical');
    expect(paren).toContain('break-after: avoid');
    const heading = ruleFor(SCREENPLAY_CSS, 'h2.scene-heading');
    // both spellings: the legacy prefixed property AND the modern standalone
    expect(heading).toContain('page-break-after: avoid');
    expect(heading).toMatch(/[^-]break-after: avoid/);
  });

  test('page markers float out of the flow so they cost no line', () => {
    expect(SCREENPLAY_CSS).not.toMatch(/p\.page-marker\s*{/);
    const marker = ruleFor(SCREENPLAY_CSS, 'span.page-marker');
    expect(marker).toContain('float: right');
    // horizontal geometry in % per the CSS invariants
    expect(marker).toMatch(/margin-left:\s*\d+%/);
  });

  test('transitions may end a page but never begin one', () => {
    const t = ruleFor(SCREENPLAY_CSS, 'p.transition');
    expect(t).toContain('page-break-before: avoid');
    expect(t).toContain('break-before: avoid');
  });

  // Selectors carrying a given mechanism, in stylesheet order. Both pins
  // below sweep every rule through the SAME parser ruleFor uses, so a
  // grouped selector wrapped across lines reads as its whole list rather
  // than its last member (see eachRule's comment for the bug that idiom
  // caused).
  const selectorsCarrying = (re: RegExp) =>
    eachRule(SCREENPLAY_CSS).filter((r) => re.test(r.body)).map((r) => r.selector);

  // Every avoid link grows the chunk a renderer pushes, so the inventory is
  // deliberately closed and written out in the css.ts header. Pin it: a new
  // keep cannot slip in without this failing and sending its author to the
  // header comment. Default options, so the two gated-off entries
  // (`.dialogue-block`, `section.scene`) are absent by design.
  //
  // Six entries carry BOTH spellings of one avoid, so twelve declarations.
  // The cue keep is two of them — p.character and p.parenthetical — because
  // it is a CHAIN on the paragraphs, not a wrapper. It stopped being a
  // wrapper on 2026-09-14: a `.keep-together` div carried the forward bind,
  // and the KFX converter behind Send-to-Kindle honors break-after on
  // text-bearing elements (#5a, proved on h2.scene-heading) but not on a
  // structural div, so cues stranded on device.
  //
  // The last three selectors are the multicol SHADOW rules, which re-spell
  // the six real ones for engines that read only that vocabulary. They match
  // neither `page-break-*` nor bare `break-*`, so they add selectors without
  // adding to the count, and they are emitted last so ruleFor() answers with
  // the spelling every other engine reads (the incident 3abeba3 is named
  // for). That is why `table.dual-dialogue` and `p.transition` each appear
  // twice: once real, once shadowed.
  //
  // Default options, so the two gated-off entries (`.dialogue-block`,
  // `section.scene`) are absent by design.
  test('the avoid inventory is closed — six entries, twelve declarations', () => {
    const declarations =
      SCREENPLAY_CSS.match(/^\s*(?:page-)?break-(?:after|before|inside):\s*avoid;/gm) ?? [];
    expect(declarations).toHaveLength(12);

    expect(selectorsCarrying(/:\s*avoid;/)).toEqual([
      'h2.scene-heading',
      'p.mini-slug',
      'p.character',
      'p.parenthetical',
      'table.dual-dialogue',
      'p.transition',
      'table.dual-dialogue',
      'h2.scene-heading, p.mini-slug, p.character, p.parenthetical',
      'p.transition',
    ]);
  });

  // widows/orphans (registry #17) is a separate, gated mechanism outside
  // the avoid-link inventory above — pin it too, the same way, so it
  // cannot spread to a third selector (e.g. p.parenthetical) without this
  // failing and sending its author to the css.ts header comment.
  test('widows/orphans carry on exactly two selectors: p.action and p.dialogue', () => {
    expect(selectorsCarrying(/\b(?:widows|orphans):\s*\d+;/)).toEqual(['p.action', 'p.dialogue']);
  });
});

// ── buildEpub ────────────────────────────────────────────

describe('buildEpub', () => {
  async function build() {
    const body = tokensToBody(sampleTokens());
    const bytes = await buildEpub({ title: 'Test Script', author: 'Jane Doe' }, body);
    const zip = await JSZip.loadAsync(bytes);
    return { bytes, zip };
  }

  test('mimetype is first, stored uncompressed, with exact content', async () => {
    const { bytes } = await build();
    const head = new TextDecoder().decode(bytes.slice(0, 100));
    expect(head).toContain('mimetypeapplication/epub+zip');
  });

  test('container.xml points at the package document', async () => {
    const { zip } = await build();
    const container = await zip.file('META-INF/container.xml')!.async('string');
    expect(container).toContain('full-path="OEBPS/package.opf"');
  });

  test('package.opf carries metadata, manifest, and spine in reading order', async () => {
    const { zip } = await build();
    const opf = await zip.file('OEBPS/package.opf')!.async('string');
    expect(opf).toContain('<dc:title>Test Script</dc:title>');
    expect(opf).toContain('<dc:creator>Jane Doe</dc:creator>');
    expect(opf).toContain('properties="nav"');
    expect(opf).toMatch(/<itemref idref="titlepage"\/>\s*<itemref idref="body001"\/>/);
  });

  test('OPF declares the ibooks prefix and specified-fonts meta', async () => {
    const { zip } = await build();
    const opf = await zip.file('OEBPS/package.opf')!.async('string');
    expect(opf).toContain(
      'prefix="ibooks: http://vocabulary.itunes.apple.com/rdf/ibooks/vocabulary-extensions-1.0/"',
    );
    expect(opf).toContain('<meta property="ibooks:specified-fonts">true</meta>');
  });

  test('nav.xhtml lists every scene with an anchored link', async () => {
    const { zip } = await build();
    const nav = await zip.file('OEBPS/nav.xhtml')!.async('string');
    expect(nav).toContain('<a href="text/body001.xhtml#sc-001">INT. KITCHEN - DAY</a>');
    expect(nav).toContain('<a href="text/body001.xhtml#sc-002">EXT. YARD - NIGHT</a>');
  });

  test('nav.xhtml carries a hidden page-list when the script has markers', async () => {
    const tokens = new Fountain().parse(
      'INT. A - DAY\n\n= pg 12\n\nShe waits.\n\n= pg 13\n\nHe leaves.\n', true,
    ).tokens;
    const body = tokensToBody(tokens);
    const zip = await JSZip.loadAsync(await buildEpub({ title: 'Test Script' }, body));
    const nav = await zip.file('OEBPS/nav.xhtml')!.async('string');
    expect(nav).toContain('<nav epub:type="page-list" hidden="hidden">');
    expect(nav).toContain('<a href="text/body001.xhtml#pg12">12</a>');
    expect(nav).toContain('<a href="text/body001.xhtml#pg13">13</a>');
    // page-list follows landmarks
    expect(nav.indexOf('page-list')).toBeGreaterThan(nav.indexOf('landmarks'));
  });

  test('no page-list nav at all when the script has no markers', async () => {
    const { zip } = await build();
    const nav = await zip.file('OEBPS/nav.xhtml')!.async('string');
    expect(nav).not.toContain('page-list');
  });

  test('title page renders title and author', async () => {
    const { zip } = await build();
    const tp = await zip.file('OEBPS/titlepage.xhtml')!.async('string');
    expect(tp).toContain('Test Script');
    expect(tp).toContain('Jane Doe');
  });

  test('body file is well-formed XHTML with the stylesheet linked', async () => {
    const { zip } = await build();
    const doc = await zip.file('OEBPS/text/body001.xhtml')!.async('string');
    expect(doc).toContain('<?xml version="1.0" encoding="UTF-8"?>');
    expect(doc).toContain('xmlns="http://www.w3.org/1999/xhtml"');
    expect(doc).toContain('<link rel="stylesheet"');
  });
});

// ── the cue keep, and what it deliberately excludes ──────

// CHANGED: the keep used to close AFTER the first dialogue token. Our
// serializer writes a whole speech as ONE line, so that token is the
// entire speech, and the keep became an unbreakable block as tall as the
// speech. An eleven-line speech that would not fit moved wholesale and
// left a page ending two thirds of the way down — the same blank-bottom

// ── inline emphasis rendering ────────────────────────────

describe('inline emphasis in XHTML', () => {
  test('fountain emphasis renders as em/strong/underline', () => {
    const tokens = new Fountain().parse(
      'INT. A - DAY\n\nThe sign reads ***DO NOT ENTER*** and **STOP** and *slow* and _underlined_.\n', true,
    ).tokens;
    const [file] = tokensToBody(tokens).files;
    expect(file.xhtml).toContain('<strong><em>DO NOT ENTER</em></strong>');
    expect(file.xhtml).toContain('<strong>STOP</strong>');
    expect(file.xhtml).toContain('<em>slow</em>');
    expect(file.xhtml).toContain('<span class="underline">underlined</span>');
  });

  test('mixed marks unwrap with the underscore innermost', () => {
    // The nesting joinLine emits for a bold+underlined run. Stars are
    // replaced before underscores, so this composes without a special case —
    // asserted here rather than assumed, on both renderers.
    const tokens = new Fountain().parse(
      'INT. A - DAY\n\nThe stamp is **_VOID_** and ***_LOUD_*** and *_soft_*.\n', true,
    ).tokens;
    const [file] = tokensToBody(tokens).files;
    expect(file.xhtml).toContain('<strong><span class="underline">VOID</span></strong>');
    expect(file.xhtml).toContain(
      '<strong><em><span class="underline">LOUD</span></em></strong>',
    );
    expect(file.xhtml).toContain('<em><span class="underline">soft</span></em>');
  });

  test('escaping still applies inside emphasized text', () => {
    const tokens = new Fountain().parse('INT. A - DAY\n\nA *5 < 6* case.\n', true).tokens;
    const [file] = tokensToBody(tokens).files;
    expect(file.xhtml).toContain('<em>5 &lt; 6</em>');
  });

  test('stray asterisk-free text is untouched', () => {
    const tokens = new Fountain().parse('INT. A - DAY\n\nPlain 2 * 3 math.\n', true).tokens;
    const [file] = tokensToBody(tokens).files;
    expect(file.xhtml).toContain('Plain 2 * 3 math.');
  });
});

// ── page markers render ──────────────────────────────────

describe('page markers in XHTML', () => {
  test('a marker rides inside the next block, not in a paragraph of its own', () => {
    const tokens = new Fountain().parse(
      'INT. A - DAY\n\nAction.\n\n= pg 47\n\nAnne crosses the room.\n', true,
    ).tokens;
    const [file] = tokensToBody(tokens).files;
    expect(file.xhtml).not.toContain('<p class="page-marker">');
    expect(file.xhtml).toMatch(/<span[^>]*class="page-marker"[^>]*>47\.<\/span>/);
    // immediately inside the following paragraph's opening tag
    expect(file.xhtml).toMatch(
      /<p class="action"><span[^>]*class="page-marker"[^>]*>47\.<\/span>Anne crosses the room\./,
    );
  });

  test('the marker span is the EPUB3 pagination anchor', () => {
    const tokens = new Fountain().parse('INT. A - DAY\n\n= pg 47\n\nAction.\n', true).tokens;
    const [file] = tokensToBody(tokens).files;
    expect(file.xhtml).toContain('epub:type="pagebreak"');
    expect(file.xhtml).toContain('role="doc-pagebreak"');
    expect(file.xhtml).toContain('id="pg47"');
    expect(file.xhtml).toContain('title="47"');
  });

  test('a trailing marker with no block to ride in is dropped', () => {
    const tokens = new Fountain().parse('INT. A - DAY\n\nAnne crosses the room.\n\n= pg 48\n', true).tokens;
    const [file] = tokensToBody(tokens).files;
    expect(file.xhtml).not.toContain('page-marker');
  });

  test('a marker at a scene seam rides into the next heading, not lost', () => {
    const tokens = new Fountain().parse(
      'INT. A - DAY\n\nAction.\n\n= pg 12\n\nINT. B - NIGHT\n\nMore.\n', true,
    ).tokens;
    const [file] = tokensToBody(tokens).files;
    expect(file.xhtml).toMatch(
      /<h2 class="scene-heading"><span[^>]*class="page-marker"[^>]*>12\.<\/span>INT\. B - NIGHT<\/h2>/,
    );
  });

  test('ordinary synopsis lines stay invisible', () => {
    const tokens = new Fountain().parse('INT. A - DAY\n\n= Jack discovers the truth\n\nAction.\n', true).tokens;
    const [file] = tokensToBody(tokens).files;
    expect(file.xhtml).not.toContain('discovers the truth');
  });

  test('page-list records every marker, in document order', () => {
    const tokens = new Fountain().parse(
      'INT. A - DAY\n\n= pg 12\n\nShe waits.\n\n= pg 13\n\nHe leaves.\n', true,
    ).tokens;
    const body = tokensToBody(tokens);
    expect(body.pageList).toEqual([
      { label: '12', href: 'text/body001.xhtml#pg12' },
      { label: '13', href: 'text/body001.xhtml#pg13' },
    ]);
  });

  test('every page-list href resolves to an id in the file it names', () => {
    const tokens = new Fountain().parse(
      'INT. A - DAY\n\n= pg 12\n\nShe waits a long while by the window.\n\n'
      + 'INT. B - NIGHT\n\n= pg 13\n\nHe leaves without a word to anyone.\n', true,
    ).tokens;
    const body = tokensToBody(tokens, { maxFileBytes: 400 });
    expect(body.files.length).toBeGreaterThan(1);
    for (const p of body.pageList) {
      const [name, fragment] = p.href.split('#');
      const file = body.files.find((f) => name === `text/${f.id}.xhtml`);
      expect(file).toBeTruthy();
      expect(file!.xhtml).toContain(`id="${fragment}"`);
    }
    // the second marker followed its scene into the second file
    expect(body.pageList[1].href).toBe('text/body002.xhtml#pg13');
  });

  test('a dropped trailing marker never reaches the page-list', () => {
    const tokens = new Fountain().parse('INT. A - DAY\n\nShe waits.\n\n= pg 48\n', true).tokens;
    expect(tokensToBody(tokens).pageList).toEqual([]);
  });

  test('a script without markers has an empty page-list', () => {
    expect(tokensToBody(sampleTokens()).pageList).toEqual([]);
  });
});

// ── figures (registry #19) ───────────────────────────────

describe('figures', () => {
  const ON = DEFAULT_FORMAT_OPTIONS;
  const OFF = { ...DEFAULT_FORMAT_OPTIONS, includeImages: false };
  const HAVE = new Set(['p001-01.png', 'p002-01.png']);
  const tokensOf = (src: string) => new Fountain().parse(src, true).tokens;
  const bodyOf = (src: string, opts: Parameters<typeof tokensToBody>[1] = {}) =>
    tokensToBody(tokensOf(src), opts);
  const scene = (note: string) => `INT. LAB - DAY\n\nBefore.\n\n${note}\n\nAfter.\n`;

  test('a picture note with its picture present becomes a figure where it sat', () => {
    const body = bodyOf(scene('[[img: p002-01.png w=42]]'), { format: ON, images: HAVE });
    expect(body.files[0].xhtml).toMatch(
      /<p class="action">Before\.<\/p>\s*<p class="figure w40"><img src="\.\.\/images\/p002-01\.png" alt="Illustration"\/><\/p>\s*<p class="action">After\.<\/p>/,
    );
    expect([...body.usedImages]).toEqual(['p002-01.png']);
  });

  test('a full-page picture says so in its class', () => {
    const body = bodyOf(scene('[[img: p001-01.png w=100 full]]'), { format: ON, images: HAVE });
    expect(body.files[0].xhtml).toContain(
      '<p class="figure w100 figure-full"><img src="../images/p001-01.png" alt="Illustration"/></p>',
    );
  });

  test('the width class is the note’s width to the nearest five, inside 5 to 100', () => {
    // Twenty classes rather than an inline style: the preview is an iframe
    // whose CSP refuses `style=` attributes, and one markup path serves both.
    const classOf = (w: string) => {
      const xhtml = bodyOf(scene(`[[img: p002-01.png w=${w}]]`), { format: ON, images: HAVE }).files[0].xhtml;
      return /<p class="figure (w\d+)"/.exec(xhtml)?.[1];
    };
    expect(classOf('42')).toBe('w40');
    expect(classOf('43')).toBe('w45');
    expect(classOf('97')).toBe('w95');
    expect(classOf('98')).toBe('w100');
    expect(classOf('1')).toBe('w5');
    expect(classOf('7')).toBe('w5');
    expect(classOf('8')).toBe('w10');
  });

  test('the alt text is never empty, because an empty alt calls the picture decoration', () => {
    const xhtml = bodyOf(scene('[[img: p002-01.png w=42]]'), { format: ON, images: HAVE }).files[0].xhtml;
    expect(xhtml).toContain('alt="Illustration"');
    expect(xhtml).not.toContain('alt=""');
  });

  test('a picture the caller cannot ship renders nothing and is not reported as used', () => {
    const plain = bodyOf('INT. LAB - DAY\n\nBefore.\n\nAfter.\n');
    const missing = bodyOf(scene('[[img: p009-01.png w=42]]'), { format: ON, images: HAVE });
    expect(missing).toEqual(plain);
    expect(missing.usedImages.size).toBe(0);
  });

  test('pictures off renders nothing, even with every picture present', () => {
    const plain = bodyOf('INT. LAB - DAY\n\nBefore.\n\nAfter.\n', { format: OFF });
    const off = bodyOf(scene('[[img: p002-01.png w=42]]'), { format: OFF, images: HAVE });
    expect(off).toEqual(plain);
    expect(off.usedImages.size).toBe(0);
  });

  test('a caller that passes no pictures renders exactly what it did before', () => {
    // Every existing caller passes none. The default is the empty set, so
    // their books do not move by a byte with the option on.
    const plain = bodyOf('INT. LAB - DAY\n\nBefore.\n\nAfter.\n');
    expect(bodyOf(scene('[[img: p002-01.png w=42]]'))).toEqual(plain);
    expect(bodyOf(scene('[[img: p002-01.png w=42]]'), { format: ON })).toEqual(plain);
  });

  test('usedImages names each picture once, however often it is shown', () => {
    const src = 'INT. LAB - DAY\n\n[[img: p002-01.png w=42]]\n\nMid.\n\n'
      + '[[img: p002-01.png w=42]]\n\nINT. YARD - DAY\n\n[[img: p001-01.png w=60]]\n';
    const body = bodyOf(src, { format: ON, images: HAVE });
    expect([...body.usedImages].sort()).toEqual(['p001-01.png', 'p002-01.png']);
    expect(body.files[0].xhtml.match(/<p class="figure/g)).toHaveLength(3);
  });

  test('the src comes from the caller’s resolver, escaped as an attribute', () => {
    // The preview points the same markup at a data: URI. Whatever a resolver
    // returns goes through escapeXml, so a quote cannot end the attribute.
    const body = bodyOf(scene('[[img: p002-01.png w=42]]'), {
      format: ON, images: HAVE, imageSrc: (file) => `x?f=${file}&a="b"`,
    });
    expect(body.files[0].xhtml).toContain(
      '<img src="x?f=p002-01.png&amp;a=&quot;b&quot;" alt="Illustration"/>',
    );
  });

  test('a figure declines a pending page marker, which lands on the next text instead', () => {
    // serialize.ts's anchorsPageMarker rule, held on this side of the
    // .fountain too: a picture is not a line of text a reader can land on.
    const body = bodyOf(
      'INT. LAB - DAY\n\nBefore.\n\n= pg 7\n\n[[img: p002-01.png w=42]]\n\nAfter.\n',
      { format: ON, images: HAVE },
    );
    const xhtml = body.files[0].xhtml;
    expect(xhtml).toMatch(/<p class="figure w40"><img [^>]*\/><\/p>/);
    expect(xhtml).toMatch(
      /<p class="action"><span[^>]*class="page-marker"[^>]*>7\.<\/span>After\.<\/p>/,
    );
    expect(body.pageList.map((p) => p.label)).toEqual(['7']);
  });

  describe('a picture note before the first heading', () => {
    // toFountain writes a title card's note above the first scene heading.
    // Whether it opens an "Opening" section is the predicate's answer, the
    // same one that decides whether it renders: a section is opened for a
    // figure, never for a note that will render nothing.
    const card = '[[img: p001-01.png w=100 full]]\n\n';
    const script = 'INT. LAB - DAY\n\nWork.\n\nEXT. YARD - NIGHT\n\nRest.\n';

    test('(a) pictures off: the body is the note-free body, byte for byte', () => {
      for (const head of ['', 'Title: T\nAuthor: A\n\n']) {
        const plain = bodyOf(head + script, { format: OFF, images: HAVE });
        expect(bodyOf(head + card + script, { format: OFF, images: HAVE })).toEqual(plain);
      }
    });

    test('(b) pictures on and present: an Opening holds the figure, and the scenes follow from sc-002', () => {
      const body = bodyOf(card + script, { format: ON, images: HAVE });
      expect(body.toc).toEqual([
        { title: 'Opening', href: 'text/body001.xhtml#sc-001' },
        { title: 'INT. LAB - DAY', href: 'text/body001.xhtml#sc-002' },
        { title: 'EXT. YARD - NIGHT', href: 'text/body001.xhtml#sc-003' },
      ]);
      expect(body.files[0].xhtml).toMatch(
        /<section class="scene" id="sc-001">\s*<p class="figure w100 figure-full"><img src="\.\.\/images\/p001-01\.png" alt="Illustration"\/><\/p>\s*<\/section>\s*<section class="scene" id="sc-002">\s*<h2 class="scene-heading">INT\. LAB - DAY<\/h2>/,
      );
      expect([...body.usedImages]).toEqual(['p001-01.png']);
    });

    test('(c) pictures on but missing: identical to (a)', () => {
      const plain = bodyOf(script, { format: OFF, images: HAVE });
      expect(bodyOf(card + script, { format: ON, images: new Set(['p002-01.png']) })).toEqual(plain);
      expect(bodyOf(card + script, { format: ON })).toEqual(plain);
    });

    test('a note that renders and one that does not, in either order, open one Opening', () => {
      const dead = '[[img: p009-01.png w=50]]\n\n';
      for (const src of [dead + card + script, card + dead + script]) {
        const body = bodyOf(src, { format: ON, images: HAVE });
        expect(body.toc.map((e) => e.title)).toEqual(['Opening', 'INT. LAB - DAY', 'EXT. YARD - NIGHT']);
        expect(body.files[0].xhtml.match(/<p class="figure/g)).toHaveLength(1);
      }
    });
  });
});

// ── figure CSS (registry #19) ────────────────────────────

describe('figure CSS', () => {
  // A spread of option sets, so a figure block that leaned on one knob's
  // value, or a seam that moved under one, shows up in some row.
  const FORMATS = [
    DEFAULT_FORMAT_OPTIONS,
    resolveFormatOptions({ scenePageBreaks: true }),
    resolveFormatOptions({ keepSpeechesWhole: true, keepSceneHeadingWithScene: false }),
    resolveFormatOptions({ elementSpacingEm: 0.6, justifyText: true }),
    resolveFormatOptions({ cueAlignment: 'indented', fontFamily: 'serif' }),
    resolveFormatOptions({ printSplitMinimums: false, includeImages: false }),
  ];
  const WIDTHS = Array.from({ length: 20 }, (_, i) => (i + 1) * 5);
  const FIGURE_SELECTORS = [
    '.figure', '.figure img', ...WIDTHS.map((n) => `p.w${n} img`), 'p.figure-full',
  ];
  const withFigures = (o = DEFAULT_FORMAT_OPTIONS) => screenplayCss(o, { hasFigures: true });
  const TITLE_PAGE = '/* Title page */';

  test('without figures the sheet is the one every caller already gets', () => {
    for (const o of FORMATS) {
      expect(screenplayCss(o, { hasFigures: false })).toBe(screenplayCss(o));
    }
    expect(screenplayCss(DEFAULT_FORMAT_OPTIONS)).toBe(SCREENPLAY_CSS);
  });

  test('without figures the sheet styles no figure and no image', () => {
    for (const o of FORMATS) {
      const css = screenplayCss(o);
      for (const r of eachRule(css)) {
        expect(r.selector, r.selector).not.toMatch(/\.figure|\bimg\b|\bp\.w\d/);
      }
      expect(css).not.toContain('.figure');
    }
  });

  test('without figures the seam is exactly as it was: one blank line between p.centered and the title page', () => {
    // The figure block is spliced in here, so this is where a stray newline
    // from the splice would land in EVERY book, pictures or not. Pinned
    // character for character, since the corpus promise is byte-identity.
    expect(screenplayCss(DEFAULT_FORMAT_OPTIONS)).toContain(
      `p.centered {\n  text-align: center;\n  margin: 1em 0;\n}\n\n${TITLE_PAGE}\n`,
    );
    expect(screenplayCss(resolveFormatOptions({ elementSpacingEm: 0.6 }))).toContain(
      `p.centered {\n  text-align: center;\n  margin: 0.6em 0;\n}\n\n${TITLE_PAGE}\n`,
    );
  });

  test('with figures the sheet is that sheet plus one block, between p.centered and the title page', () => {
    for (const o of FORMATS) {
      const without = screenplayCss(o);
      const withF = withFigures(o);
      const at = without.indexOf(TITLE_PAGE);
      expect(at).toBeGreaterThan(0);
      // Everything before the seam and everything after it is untouched.
      expect(withF.slice(0, at)).toBe(without.slice(0, at));
      expect(withF.endsWith(without.slice(at))).toBe(true);
      const block = withF.slice(at, withF.length - (without.length - at));
      // The seam it opens sits right after p.centered.
      expect(withF.slice(0, at)).toMatch(/p\.centered \{[^}]*\}\n\n$/);
      // It holds the figure rules, in this order, and nothing else.
      expect(eachRule(block).map((r) => r.selector)).toEqual(FIGURE_SELECTORS);
      // And it ends with the same one blank line every rule here does.
      expect(block.endsWith('}\n\n')).toBe(true);
    }
  });

  test('the figure box: element spacing above and below, centred, in em', () => {
    expect(ruleFor(withFigures(), '.figure')).toContain('margin: 1em 0;');
    expect(ruleFor(withFigures(), '.figure')).toContain('text-align: center;');
    const tight = withFigures(resolveFormatOptions({ elementSpacingEm: 0.6 }));
    expect(ruleFor(tight, '.figure')).toContain('margin: 0.6em 0;');
  });

  test('the picture keeps its aspect: the height follows the width', () => {
    const img = ruleFor(withFigures(), '.figure img');
    expect(img).toContain('height: auto;');
    expect(img).not.toContain('width');
  });

  test('twenty width rules, 5% to 100% of the column in steps of five', () => {
    const css = withFigures();
    for (const n of WIDTHS) {
      expect(ruleFor(css, `p.w${n} img`)).toMatch(new RegExp(`\\{\\s*width: ${n}%;\\s*\\}$`));
    }
    expect(eachRule(css).filter((r) => /^p\.w\d+ img$/.test(r.selector))).toHaveLength(20);
  });

  test('a full-page plate ends the page before it and the page it is on', () => {
    const full = ruleFor(withFigures(), 'p.figure-full');
    expect(full).toContain('page-break-before: always;');
    expect(full).toContain('page-break-after: always;');
    expect(full).not.toContain('avoid');
  });

  test('a full-page plate has no vertical margin, which would push it past a blank page', () => {
    // A top margin survives a forced break. Under .figure's margin a plate
    // within about 1em of the page height stopped fitting and Blink moved it
    // to the next page, leaving the page after the break blank (measured on
    // images.pdf's title card at 6x8in). The plate's own rule cancels it;
    // p.figure-full outranks .figure by specificity, so order cannot undo it.
    expect(ruleFor(withFigures(), 'p.figure-full')).toMatch(/(^|[{;\s])margin: 0;/);
  });

  test('figures add no keep, no multicol spelling, and leave every existing pin as it was', () => {
    // No keep on .figure: it holds one line box, so there is no break inside
    // it to forbid, and a keep measured inert (the css.ts header has the
    // numbers). The forced breaks are `always`, which the avoid inventory
    // does not count, and like the scene break they have no column spelling.
    for (const o of FORMATS) {
      const without = screenplayCss(o);
      const withF = withFigures(o);
      const avoids = (css: string) =>
        css.match(/^\s*(?:page-)?break-(?:after|before|inside):\s*avoid;/gm) ?? [];
      expect(avoids(withF)).toEqual(avoids(without));
      const carrying = (css: string, re: RegExp) =>
        eachRule(css).filter((r) => re.test(r.body)).map((r) => r.selector);
      expect(carrying(withF, /:\s*avoid;/)).toEqual(carrying(without, /:\s*avoid;/));
      expect(carrying(withF, /-webkit-column-break/)).toEqual(carrying(without, /-webkit-column-break/));
      expect(carrying(withF, /\b(?:widows|orphans):/)).toEqual(carrying(without, /\b(?:widows|orphans):/));
      expect(carrying(withF, /:\s*always;/)).toEqual([
        ...carrying(without, /:\s*always;/), 'p.figure-full',
      ]);
      expect(ruleFor(withF, '.figure')).not.toMatch(/break|keep/);
    }
  });

  test('the figure sheet keeps the stylesheet invariants', () => {
    // Horizontal in %, vertical in em, no max-width anywhere (not even in a
    // comment: the sheet ships its comments), no line-height, CSS 2.1 value
    // syntax, no fixed units, and no brace inside a comment, which would
    // shift every rule boundary the tests read after it.
    for (const o of FORMATS) {
      const css = withFigures(o);
      expect(css).not.toContain('max-width');
      expect(css).not.toContain('max-height');
      const block = eachRule(css)
        .filter((r) => FIGURE_SELECTORS.includes(r.selector))
        .map((r) => r.body).join('\n');
      expect(block).not.toContain('line-height');
      expect(block).not.toMatch(/\b(?:min|max|clamp|var|calc)\(/);
      expect(block).not.toMatch(/\d(?:px|pt|cm|mm|in)\b/);
      for (const c of css.match(/\/\*[\s\S]*?\*\//g) ?? []) expect(c).not.toMatch(/[{}]/);
    }
  });

  test('every class the figure rules style is one the renderer emits, and every one it emits is styled', () => {
    // The renderer rounds a note's width to a step of five inside 5..100 and
    // writes it as a class; the sheet defines one rule per step. Two files,
    // one set: proved by running the renderer over every width a note can
    // carry, not by reading either file's source.
    const have = new Set(['p001-01.png']);
    const emitted = new Set<string>();
    for (let w = -5; w <= 105; w++) {
      for (const full of ['', ' full']) {
        const xhtml = tokensToBody(
          new Fountain().parse(`INT. A - DAY\n\n[[img: p001-01.png w=${w}${full}]]\n`, true).tokens,
          { images: have },
        ).files[0].xhtml;
        const m = /<p class="([^"]*)"><img /.exec(xhtml);
        expect(m, `w=${w}${full}`).not.toBeNull();
        for (const cls of m![1].split(/\s+/)) emitted.add(cls);
      }
    }
    const block = eachRule(withFigures())
      .filter((r) => FIGURE_SELECTORS.includes(r.selector))
      .map((r) => r.selector).join(' ');
    const styled = new Set([...block.matchAll(/\.([a-z][a-z0-9-]*)/g)].map((m) => m[1]));
    expect([...styled].sort()).toEqual([...emitted].sort());
    expect(styled.size).toBe(22);
  });

  test('a preview that shows no figure carries no figure rules', () => {
    // The preview derives the flag from the body it rendered, as the book
    // does, so a script whose note has no picture to show styles none.
    const html = tokensToPreviewHtml(
      new Fountain().parse('INT. A - DAY\n\n[[img: p001-01.png w=50]]\n\nAfter.\n', true).tokens,
    );
    expect(html).not.toContain('.figure');
    expect(html).toContain(screenplayCss(DEFAULT_FORMAT_OPTIONS));
  });
});

// ── dual dialogue side-by-side ───────────────────────────

describe('dual dialogue table rendering', () => {
  const DUAL_SRC = 'INT. A - DAY\n\n@VERA\n(overlapping)\nRead me the last page--\n\n@INFORMANT ^\n--the last page burned.\n\nThey stare.\n';

  test('dual speeches render as a two-cell table', () => {
    const tokens = new Fountain().parse(DUAL_SRC, true).tokens;
    const [file] = tokensToBody(tokens).files;
    expect(file.xhtml).toMatch(
      /<table class="dual-dialogue">\s*<tr>\s*<td>\s*<p class="character">VERA<\/p>\s*<p class="parenthetical">\(overlapping\)<\/p>\s*<p class="dialogue">Read me the last page--<\/p>\s*<\/td>\s*<td>\s*<p class="character">INFORMANT<\/p>\s*<p class="dialogue">--the last page burned\.<\/p>\s*<\/td>\s*<\/tr>\s*<\/table>/,
    );
    expect(file.xhtml).toContain('<p class="action">They stare.</p>');
  });

  test('table style keeps the pair together and splits width evenly', () => {
    expect(ruleFor(SCREENPLAY_CSS, 'table.dual-dialogue')).toContain('page-break-inside: avoid');
    expect(ruleFor(SCREENPLAY_CSS, 'table.dual-dialogue td')).toContain('width: 50%');
  });
});

// ── mini-slugs (secondary sluglines) ─────────────────────

describe('mini-slug rendering', () => {
  // "LATER", "THE KITCHEN" — forced sluglines in Fountain, which is how
  // serialize.ts writes the parser's mini-slug elements.
  const SRC = 'INT. STORE - NIGHT\n\nMargo locks up.\n\n.LATER\n\nThe lights are still on.\n';

  test('a forced non-INT/EXT slug renders as a micro-heading, not a scene heading', () => {
    const [file] = tokensToBody(new Fountain().parse(SRC, true).tokens).files;
    expect(file.xhtml).toContain('<p class="mini-slug">LATER</p>');
    expect(file.xhtml).not.toContain('<h2 class="scene-heading">LATER</h2>');
  });

  test('a mini-slug stays inside its scene — no section, no TOC entry', () => {
    const body = tokensToBody(new Fountain().parse(SRC, true).tokens);
    expect(body.toc).toEqual([
      { title: 'INT. STORE - NIGHT', href: 'text/body001.xhtml#sc-001' },
    ]);
    expect(body.files[0].xhtml).not.toContain('sc-002');
  });

  test('primary sluglines are untouched — INT/EXT/EST/I-E stay scene headings', () => {
    const src = 'INT. A - DAY\n\nOne.\n\nEXT. B - DAY\n\nTwo.\n\nEST. C - DAY\n\nThree.\n\nI/E. D - DAY\n\nFour.\n';
    const body = tokensToBody(new Fountain().parse(src, true).tokens);
    expect(body.files[0].xhtml).not.toContain('mini-slug');
    expect(body.toc.map((t) => t.title)).toEqual([
      'INT. A - DAY', 'EXT. B - DAY', 'EST. C - DAY', 'I/E. D - DAY',
    ]);
  });

  test('a mini-slug can open a script without swallowing the Opening section', () => {
    // Also the documented divergence for hand-written Fountain (README,
    // registry #5b): a dot-forced ".COLD OPEN" is a mini-slug here, not a
    // scene, so it gets no TOC entry. The trade Screepub takes for owning
    // the dot-force as its mini-slug carrier.
    const body = tokensToBody(new Fountain().parse('.COLD OPEN\n\nBlack.\n', true).tokens);
    expect(body.toc).toEqual([{ title: 'Opening', href: 'text/body001.xhtml#sc-001' }]);
    expect(body.files[0].xhtml).toContain('<p class="mini-slug">COLD OPEN</p>');
  });

  test('a numbered mini-slug keeps its number, like a slugline does', () => {
    const tokens = new Fountain().parse('INT. A - DAY\n\nOne.\n\n.LATER #5#\n\nTwo.\n', true).tokens;
    const [file] = tokensToBody(tokens, { format: { ...DEFAULT_FORMAT_OPTIONS, showSceneNumbers: true } }).files;
    expect(file.xhtml).toContain('<p class="mini-slug"><span class="scene-number">5</span> LATER</p>');
  });

  test('PRIMARY_SLUG stays in lockstep with fountain-js', () => {
    // Every opener fountain-js promotes UNFORCED must also promote when it
    // arrives forced, and nothing else may. An upstream rule change breaks
    // this table rather than silently reshuffling headings and TOC entries.
    const cases: [string, boolean][] = [
      ['INT. HOUSE - DAY', true],
      ['INT HOUSE - DAY', true],
      ['EXT. YARD - NIGHT', true],
      ['EST. THE HOUSE', true],
      ['I/E. CAR - DAY', true],
      ['I/E CAR - DAY', true],
      ['INT./EXT. CAR - DAY', true],
      ['INT/EXT CAR - DAY', true],
      ['LATER', false],
      ['THE BACK ROOM', false],
      ['ESTABLISHING SHOT', false],
      ['INTO THE WOODS', false],
      ['BACK TO:', false],
    ];
    for (const [text, promotes] of cases) {
      const unforced = new Fountain().parse(`${text}\n\nAction.\n`, true).tokens;
      expect([text, unforced[0].type === 'scene_heading']).toEqual([text, promotes]);
      const [file] = tokensToBody(new Fountain().parse(`.${text}\n\nAction.\n`, true).tokens).files;
      expect([text, file.xhtml.includes('<h2 class="scene-heading">')]).toEqual([text, promotes]);
      expect([text, file.xhtml.includes('<p class="mini-slug">')]).toEqual([text, !promotes]);
    }
  });

  test('the parser excludes from mini-slug exactly what this promotes', () => {
    // One literal, two layers: classify.ts must not mint a mini-slug that
    // the renderer would turn back into a scene heading.
    expect(PARSER_PRIMARY_SLUG.source).toBe(PRIMARY_SLUG.source);
    expect(PARSER_PRIMARY_SLUG.flags).toBe(PRIMARY_SLUG.flags);
  });

  test('the stylesheet gives it the bold uppercase micro-heading treatment', () => {
    const rule = ruleFor(SCREENPLAY_CSS, 'p.mini-slug');
    expect(rule).toContain('font-weight: bold');
    expect(rule).toContain('text-transform: uppercase');
    // Vertical rhythm in em, per the CSS invariants: more air above than below.
    expect(rule).toMatch(/margin:\s*1\.4em 0 1em 0/);
  });

  test('every class the stylesheet styles is one a renderer emits', () => {
    // The regression this suite exists for: p.mini-slug was styled for
    // months while no code path ever emitted the class.
    // Two kinds of evidence, both counting as "a renderer emits it":
    // a `class="..."` literal in the renderer source, and — for classes
    // assembled at runtime rather than written out, like the registry #18
    // fmt-* set — real rendered output. The second is the stronger form:
    // it proves emission by executing the renderer, not by grepping it.
    const fmtProbe = tokensToBody(
      new Fountain().parse(
        'INT. A - DAY\n\n[[fmt: mono -1]]\nOne.\n\n[[fmt: serif +1]]\nTwo.\n\n'
        + '[[fmt: sans +2]]\nThree.\n\n[[fmt: cursive]]\nFour.\n',
        true,
      ).tokens,
    ).files[0].xhtml;
    const emitted = readFileSync(new URL('../src/epub/html.ts', import.meta.url), 'utf8')
      + readFileSync(new URL('../src/epub/build.ts', import.meta.url), 'utf8')
      + fmtProbe;
    // Every class in every selector, descendants included (p.credit inside
    // section.titlepage counts) — comments stripped first, or "e.g." reads
    // as a class named "g".
    const selectors = [...SCREENPLAY_CSS.replace(/\/\*[\s\S]*?\*\//g, '').matchAll(/([^{}]+)\{/g)];
    const styled = selectors.flatMap((s) => [...s[1].matchAll(/\.([a-z][a-z0-9-]*)/g)].map((m) => m[1]));
    expect(new Set(styled).size).toBeGreaterThan(12);
    // Parse class LISTS, not whole attributes: a paragraph can carry several
    // classes (`class="action fmt-sans"`), so a containment check against
    // `class="action"` would miss both of them. Interpolations are blanked
    // first so the source's `class="action${fmt}"` reads as `action`.
    const emittedClasses = new Set(
      [...emitted.replace(/\$\{[^}]*\}/g, '').matchAll(/class="([^"]*)"/g)]
        .flatMap((m) => m[1].split(/\s+/))
        .filter(Boolean),
    );
    for (const cls of new Set(styled)) {
      expect(emittedClasses.has(cls), `no renderer emits .${cls}`).toBe(true);
    }
  });
});

// ── tokensToPreviewHtml ───────────────────────────────────

describe('tokensToPreviewHtml', () => {
  const tokens = new Fountain().parse(
    'INT. KITCHEN - DAY\n\nA kettle screams.\n\nMARGO\nTurn it off.\n',
    true,
  ).tokens;

  test('emits one self-contained document with inline css', () => {
    const html = tokensToPreviewHtml(tokens, { dialogueSideMarginPct: 25 });
    expect(html).toContain('class="dialogue-block"');
    expect(html).toContain('<style>');
    expect(html).toContain('margin-left: 25%');
    expect(html).not.toContain('<link');
  });

  test('defaults options when none given', () => {
    const html = tokensToPreviewHtml(tokens);
    expect(html).toContain('margin-left: 20%');
  });

  test('empty tokens still fall back to a styled document', () => {
    const html = tokensToPreviewHtml([]);
    expect(html).toContain('<style>');
  });

  test('a picture note before the first heading adds no section to the preview', () => {
    // The Read window builds its scene rail from every section.scene in this
    // document, so an empty one would be an "Opening" the script never had.
    const of = (src: string) => tokensToPreviewHtml(new Fountain().parse(src, true).tokens);
    const script = 'Title: T\n\nINT. KITCHEN - DAY\n\nA kettle screams.\n';
    const noted = of(script.replace('INT.', '[[img: p001-01.png w=100 full]]\n\nINT.'));
    expect(noted).toBe(of(script));
    expect(noted.match(/<section class="scene"/g)).toHaveLength(1);
  });
});

// ── block font shifts (registry #18) ─────────────────────

describe('font shifts', () => {
  const render = (src: string, format = DEFAULT_FORMAT_OPTIONS) => {
    const { tokens } = new Fountain().parse(src, true);
    return tokensToBody(tokens, { format }).files[0].xhtml;
  };

  test('a leading fmt note becomes classes on the action paragraph', () => {
    const out = render('INT. A - DAY\n\n[[fmt: sans]]\nCHYRON: LIVE.\n');
    expect(out).toContain('<p class="action fmt-sans">CHYRON: LIVE.</p>');
  });

  test('family and size both land', () => {
    const out = render('INT. A - DAY\n\n[[fmt: sans +2]]\nA SIGN.\n');
    expect(out).toContain('<p class="action fmt-sans fmt-plus2">A SIGN.</p>');
  });

  test('every dialogue paragraph of the token gets the classes', () => {
    const out = render('INT. A - DAY\n\n@WREN\n[[fmt: mono -1]] one\ntwo\n');
    expect(out).toContain('<p class="dialogue fmt-mono fmt-minus1">one</p>');
    expect(out).toContain('<p class="dialogue fmt-mono fmt-minus1">two</p>');
  });

  test('with the knob off the note still vanishes but adds no classes', () => {
    const out = render(
      'INT. A - DAY\n\n[[fmt: sans]]\nCHYRON: LIVE.\n',
      resolveFormatOptions({ preserveFontShifts: false }),
    );
    expect(out).toContain('<p class="action">CHYRON: LIVE.</p>');
    expect(out).not.toContain('fmt-sans');
  });

  test('notes are invisible everywhere, knob or no knob', () => {
    // Fountain says notes never render. Before registry #18 they rendered as
    // literal text; this is the general fix, not just an fmt one.
    for (const format of [DEFAULT_FORMAT_OPTIONS,
                          resolveFormatOptions({ preserveFontShifts: false })]) {
      const out = render('INT. A - DAY\n\nHe leaves [[check this]] quickly.\n', format);
      expect(out).toContain('He leaves quickly.');
      expect(out).not.toContain('[[');
      expect(out).not.toContain('check this');
    }
  });

  test('an image note renders nothing, in every shape it can arrive in', () => {
    // The .fountain records pictures before any renderer draws one, so until
    // one does, the note has to stay as invisible as any other. Three shapes:
    // on its own (what toFountain writes), after the speech it was deferred
    // past (likewise), and glued to a line by a hand edit, which folds it into
    // that line's token for stripNotes to remove.
    const sources = [
      'INT. A - DAY\n\n[[img: p001-01.png w=50]]\n\nSomeone waits.\n',
      'INT. A - DAY\n\n@WREN\nHer line.\n\n[[img: p001-01.png w=50]]\n\nSomeone waits.\n',
      'INT. A - DAY\n\n[[img: p001-01.png w=50]]\nSomeone waits.\n',
    ];
    for (const src of sources) {
      const out = render(src);
      expect(out, src).toContain('Someone waits.');
      expect(out, src).not.toContain('img:');
      expect(out, src).not.toContain('p001-01.png');
    }
  });

  test('a malformed fmt note strips clean and adds nothing', () => {
    const out = render('INT. A - DAY\n\n[[fmt: chartreuse]]\nStill fine.\n');
    expect(out).toContain('<p class="action">Still fine.</p>');
    expect(out).not.toContain('[[');
  });

  test('the stylesheet carries all seven classes, in em and CSS 2.1 syntax', () => {
    const css = screenplayCss(DEFAULT_FORMAT_OPTIONS);
    for (const cls of ['.fmt-mono', '.fmt-serif', '.fmt-sans', '.fmt-cursive']) {
      expect(ruleFor(css, cls), cls).toContain('font-family');
    }
    expect(ruleFor(css, '.fmt-minus1')).toContain('font-size: 0.85em');
    expect(ruleFor(css, '.fmt-plus1')).toContain('font-size: 1.2em');
    expect(ruleFor(css, '.fmt-plus2')).toContain('font-size: 1.5em');
    // The invariants: no max-width, no line-height, no CSS3 value functions.
    const rules = ['.fmt-mono', '.fmt-serif', '.fmt-sans', '.fmt-cursive',
                   '.fmt-minus1', '.fmt-plus1', '.fmt-plus2']
      .map((c) => ruleFor(css, c)).join('\n');
    expect(rules).not.toContain('max-width');
    expect(rules).not.toContain('line-height');
    expect(rules).not.toMatch(/\b(min|max|clamp|var)\(/);
  });
});

// ── the forward bind lives on the text, not on a wrapper ─────────────
//
// Device verdict 2026-09-14, photo-confirmed: cues stranded at page
// bottoms with their dialogue overleaf, on a script sent via Send-to-Kindle
// web (Amazon server conversion -> KFX, Enhanced Typesetting). One of the
// stranded cues introduced a ONE-LINE speech, which rules out every
// size-based explanation -- orphans, an oversized pushed chunk, a speech
// too tall to fit. Nothing bound the cue forward at all.
//
// The cause is the element the bind sat on. #5a settled that
// `break-after: avoid` binds on this exact route, but it settled it on
// `h2.scene-heading`, a text-bearing element. The cue's bind had been moved
// onto a `<div>` wrapper, and this converter neither honors break-after on
// a structural div nor propagates a last child's break-after up to its
// parent -- so `p.character`'s own rule governed a break inside the wrapper
// that can never be taken, and the boundary the reader meets was
// ungoverned. #8b's own lesson, fired a second time: a verdict about a
// PROPERTY does not transfer across a change in DOM shape.
//
// So the wrapper goes, and the chain moves onto the elements that carry
// text, which is the shape #5a already proved on this renderer.
describe('the cue chain (no wrapper)', () => {
  const body = (src: string) =>
    tokensToBody(new Fountain().parse(src, true).tokens).files[0].xhtml;

  test('the cue is a direct child of the dialogue block', () => {
    expect(body('INT. A - DAY\n\nHi.\n\n@JACK\nOnly line.\n')).toMatch(
      /<div class="dialogue-block">\s*<p class="character">JACK<\/p>\s*<p class="dialogue">Only line\.<\/p>\s*<\/div>/,
    );
  });

  test('cue, parenthetical and dialogue are siblings', () => {
    expect(body('INT. A - DAY\n\nHi.\n\n@JACK\n(tired)\nFirst line.\nSecond line.\n')).toMatch(
      /<div class="dialogue-block">\s*<p class="character">JACK<\/p>\s*<p class="parenthetical">\(tired\)<\/p>\s*<p class="dialogue">First line\.<\/p>\s*<p class="dialogue">Second line\.<\/p>\s*<\/div>/,
    );
  });

  test('no keep-together wrapper is emitted anywhere', () => {
    expect(body('INT. A - DAY\n\n@JACK\n(tired)\nSpeech.\n')).not.toContain('keep-together');
  });

  test('the stylesheet no longer defines the dead wrapper class', () => {
    expect(SCREENPLAY_CSS).not.toContain('.keep-together');
  });

  test('a parenthetical binds forward, so cue plus paren cannot strand', () => {
    // The inverse of what c3b99f0 pinned. That refusal was correct while
    // the wrapper existed: chaining the parenthetical then grew a chunk
    // that already held the cue AND the speech. With the wrapper gone the
    // chunk is cue + parenthetical + the orphans minimum (#17), about four
    // lines, which is the bound #5a traded the heading wrapper for.
    const p = ruleFor(SCREENPLAY_CSS, 'p.parenthetical');
    expect(p).toContain('page-break-after: avoid');
    expect(p).toContain('break-after: avoid');
  });

  test('the cue still binds forward itself', () => {
    const c = ruleFor(SCREENPLAY_CSS, 'p.character');
    expect(c).toContain('page-break-after: avoid');
    expect(c).toContain('break-after: avoid');
  });
});

// ── the column spelling of the FORWARD binds ─────────────────────────
//
// Some engines paginate by CSS multi-column and read only the multicol
// vocabulary for fragmentation. The stylesheet has always shadowed the
// INSIDE keeps that way (`-webkit-column-break-inside`) and never the
// forward ones, so on Apple Books and the Readium family (Thorium, Kobo's
// phone and tablet apps) EVERY keep-with-next rule we have was inert:
// heading, cue, parenthetical, mini-slug, and the transition's backward
// bind. Headings and cues could strand there exactly as they did on the
// Kindle, for the same reason wearing a different spelling.
//
// Not a regression — the gap predates the cue chain — but it is the same
// defect, so it is fixed the same way and the list is DERIVED from the
// gating that emits the binds, per the correction recorded in #8b: a
// hand-kept copy of this list once left the whole-speech keep silently
// inert in Apple Books while the cue keep worked.
describe('the column spelling of the forward binds', () => {
  const shadows = (css: string, prop: string) =>
    eachRule(css).filter((r) => r.body.includes(prop)).map((r) => r.selector);

  test('every forward bind is shadowed in the multicol spelling', () => {
    expect(shadows(SCREENPLAY_CSS, '-webkit-column-break-after')).toEqual([
      'h2.scene-heading, p.mini-slug, p.character, p.parenthetical',
    ]);
  });

  test('the backward bind on transitions is shadowed too', () => {
    expect(shadows(SCREENPLAY_CSS, '-webkit-column-break-before')).toEqual(['p.transition']);
  });

  test('the shadows never share a declaration block with the page spelling', () => {
    // iBooks drops BOTH forms when they share one block. This is the whole
    // reason the shadows are separate rules rather than extra declarations.
    for (const r of eachRule(SCREENPLAY_CSS)) {
      if (/-webkit-column-break/.test(r.body)) expect(r.body).not.toMatch(/(^|[^-])break-/);
    }
  });

  test('the heading shadow follows the heading gate', () => {
    const off = screenplayCss(resolveFormatOptions({ keepSceneHeadingWithScene: false }));
    expect(shadows(off, '-webkit-column-break-after')).toEqual([
      'p.mini-slug, p.character, p.parenthetical',
    ]);
  });
});
