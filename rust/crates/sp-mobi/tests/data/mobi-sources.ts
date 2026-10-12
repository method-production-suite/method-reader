// The TypeScript's answers for the Fountain sources tests/mobi.test.ts parses,
// for sp-mobi's tests (tests/common/sources.rs says what each answer is).
// Transitional, like tools/oracle/: it goes when the TypeScript does.
//
//   bun rust/crates/sp-mobi/tests/data/mobi-sources.ts rust/crates/sp-mobi/tests/data/mobi-sources.json
import { Fountain } from 'fountain-js';
import { writeFileSync } from 'node:fs';

const SAMPLE = `INT. KITCHEN - DAY

Jack enters, exhausted.

@JACK
(tired)
Long day. 5 < 6 & "quotes" too.

EXT. YARD - NIGHT

> FADE OUT.
`;
const CARD = '[[img: p001-01.png w=100 full]]\n\nINT. LAB - DAY\n\n[[img: p002-01.png w=42]]\n\nEnd.\n';
const scene = (note: string) => `INT. LAB - DAY\n\nBefore.\n\n${note}\n\nAfter.\n`;
const card = '[[img: p001-01.png w=100 full]]\n\nINT. LAB - DAY\n\nWork.\n';
const sources = [
  SAMPLE,
  'INT. STORE - NIGHT\n\nThe gate rattles.\n\n.LATER\n\nStill on.\n',
  'INT. A - DAY\n\nAction.\n\nINT. B - NIGHT\n\nMore.\n',
  'Black screen. A hum builds.\n\nINT. A - DAY\n\nAction.\n',
  'INT. A - DAY\n\nAction.\n',
  'INT. A - DAY\n\nAction.\n\n.LATER\n\nMore.\n\nINT. B - NIGHT\n\nEnd.\n',
  'INT. KITCHEN - DAY\n\n= pg 47\n\nJack enters, exhausted.\n',
  'INT. KITCHEN - DAY\n\n= pg 47\n\nJack enters.\n',
  'INT. KITCHEN - DAY\n\nJack enters.\n\n= pg 48\n',
  'INT. A - DAY\n\nThe sign reads _DO NOT ENTER_.\n',
  'INT. A - DAY\n\nThe stamp is **_VOID_** now.\n',
  'INT. A - DAY\n\nAll three: ***_LOUD_*** here.\n',
  'INT. A - DAY\n\nA ***b1*** and **c1** and *d1*.\n',
  'INT. A - DAY\n\n[[fmt: +2]]\nA SIGN.\n',
  'INT. A - DAY\n\n[[fmt: -1]]\nSmall.\n',
  'INT. A - DAY\n\n[[fmt: +1]]\nMedium.\n',
  'INT. A - DAY\n\n@WREN\n[[fmt: +1]] Read it back.\n',
  'INT. A - DAY\n\n[[fmt: sans]]\nCHYRON.\n',
  'INT. A - DAY\n\n[[fmt: sans +2]]\nA SIGN.\n',
  'INT. A - DAY\n\nHe leaves [[check this]] quickly.\n',
  'INT. A - DAY\n\n[[img: p001-01.png w=50]]\n\nSomeone waits.\n',
  'INT. A - DAY\n\n@WREN\nHer line.\n\n[[img: p001-01.png w=50]]\n\nSomeone waits.\n',
  'INT. A - DAY\n\n[[img: p001-01.png w=50]]\nSomeone waits.\n',
  'INT. LAB - DAY\n\n[[img: p002-01.png w=42]]\n\nMid.\n\n[[img: p001-01.png w=60]]\n\n[[img: p002-01.png w=42]]\n\nEnd.\n',
  'INT. LAB - DAY\n\n[[img: p009-01.png w=50]]\n\n[[img: a/b.png w=50]]\n\n'
    + '[[img: p001-01.png w=50]]\nGlued.\n\n[[img: p002-01.png w=42]]\n\n'
    + 'Mid.\n\n[[img: p001-01.png w=100 full]]\n\nEnd.\n',
  CARD,
  `Opening line.\n\n${CARD}`,
  `INT. HALL - DAY\n\n${CARD}`,
  `[[img: p002-01.png w=42]]\n\n${CARD}`,
  '[[img: p009-01.png w=100 full]]\n\nINT. LAB - DAY\n\nEnd.\n',
  scene('[[img: p002-01.png w=42]]'),
  scene('[[img: p009-01.png w=42]]'),
  scene('[[img: p001-01.png w=100 full]]'),
  scene(''),
  'INT. LAB - DAY\n\nBefore.\n\nAfter.\n',
  'INT. LAB - DAY\n\n@WREN\nHer line.\n\n[[img: p002-01.png w=42]]\n\nAfter.\n',
  'INT. LAB - DAY\n\nBefore.\n\n= pg 7\n\n[[img: p002-01.png w=42]]\n\nAfter.\n',
  card,
  'INT. LAB - DAY\n\nWork.\n\n[[img: p001-01.png w=100 full]]\n',
  'Opening.\n\n[[img: p001-01.png w=100 full]]\n\n[[img: p002-01.png w=100 full]]\n\n'
    + 'INT. LAB - DAY\n\nWork.\n\n[[img: p001-01.png w=100 full]]\n\nEXT. YARD - NIGHT\n\nRest.\n',
];
import { tokensToBody } from '../../../../../src/epub/html';
import { resolveFormatOptions } from '../../../../../src/options';
const FORMATS: Record<string, object> = { on: {}, off: { includeImages: false }, noCover: { promoteCoverImage: false } };
const HAVES: Record<string, string[]> = { have: ['p001-01.png', 'p002-01.png'], haveAb: ['p001-01.png', 'p002-01.png', 'a/b.png'] };
const epub = (source: string) => Object.fromEntries(Object.entries(FORMATS).flatMap(([f, o]) => Object.entries(HAVES).map(([h, names]) => {
  const body = tokensToBody(new Fountain().parse(source, true).tokens, { format: resolveFormatOptions(o), images: new Set(names) });
  return [`${f}/${h}`, { used: [...body.usedImages], cover: body.coverImage ?? null }];
})));
import { mobiArt, tokensToMobiHtml } from '../../../../../src/mobi/html';
const MFORMATS: Record<string, object> = { on: {}, off: { includeImages: false }, breaks: { scenePageBreaks: true }, noTitle: { includeTitlePage: false }, noShifts: { preserveFontShifts: false }, noCover: { promoteCoverImage: false } };
const mobi = (source: string) => Object.fromEntries(Object.entries(MFORMATS).map(([f, o]) => {
  const tokens = new Fountain().parse(source, true).tokens;
  const format = resolveFormatOptions(o);
  const art = mobiArt(tokens, format, new Set(HAVES.have));
  return [f, { order: art.order, coverIndex: art.coverIndex ?? null, html: tokensToMobiHtml(tokens, { title: 'T', author: 'A' }, format, art) }];
}));
const out = sources.map((source) => ({ source, tokens: new Fountain().parse(source, true).tokens, epub: epub(source), mobi: mobi(source) }));
writeFileSync(process.argv[2], `${JSON.stringify(out, null, 1)}\n`);
