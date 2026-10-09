// fountain-js tokens → Final Draft (.fdx): the editable screenplay, beside
// the EPUB and MOBI renderers and fed the same tokens from the same
// .fountain. Where those two make a book to READ, this makes a script to
// keep WRITING in Final Draft, Fade In, WriterDuet, Highland or Celtx.
//
// Deliberately minimal: a FinalDraft root, the script's paragraphs and its
// title page, and nothing else. No ElementSettings, PageLayout or
// SmartType blocks: Final Draft fills its own defaults for every one of
// them, and the other importers ignore them anyway. Paragraphs carry only
// their element Type (plus Number, Alignment and StartsNewPage where the
// Fountain says so), never fonts or indents, so the receiving app's own
// screenplay template decides how the page looks.
//
// Format-option-free on purpose: this is a reconstruction of the script,
// not a rendering of the book, so no reading knob (scene numbers shown,
// title page included) changes it. Whatever the .fountain carries, the FDX
// carries.
import { Fountain, type Token } from 'fountain-js';
import { stripNotes } from '../fountain/notes';
import { isMiniSlug } from '../fountain/slug';

/**
 * The FinalDraft root's Version attribute. ONE constant, because the owner's
 * Final Draft is an older licence and this may have to come down: version 2
 * is what Final Draft 8 to 10 write, and newer releases (version 5 and up)
 * still open it. Nothing written here needs a newer schema.
 */
export const FDX_VERSION = 2;

const XML_DECLARATION = '<?xml version="1.0" encoding="UTF-8" standalone="no" ?>';

/** One styled stretch of a paragraph: a `<Text>` element. */
export interface TextRun {
  text: string;
  /** "Bold", "Italic", "Underline" joined with "+" in that order; absent
   * for plain text. */
  style?: string;
}

// Sentinels for the emphasis pass: Unicode noncharacters, which no real
// text carries (any that arrive are removed first), so they can mark where
// a style opens and closes without colliding with the script's own words.
const B_OPEN = '\uFDD0';
const B_CLOSE = '\uFDD1';
const I_OPEN = '\uFDD2';
const I_CLOSE = '\uFDD3';
const U_OPEN = '\uFDD4';
const U_CLOSE = '\uFDD5';
const LITERAL_STAR = '\uFDD6';
const LITERAL_UNDERSCORE = '\uFDD7';
const SENTINELS = /[\uFDD0-\uFDEF]/g;

/** XML 1.0's Char production, inverted: what may never appear in a
 * document, escaped or not. Lone surrogates fall out here too. */
const FORBIDDEN_XML = /[\u0000-\u0008\u000B\u000C\u000E-\u001F\uFFFE\uFFFF]|[\uD800-\uDBFF](?![\uDC00-\uDFFF])|(?<![\uD800-\uDBFF])[\uDC00-\uDFFF]/g;

/** The same four pairings the EPUB and MOBI renderers use (a marker must
 * hug its text: `2 * 3` is not emphasis), but run to a fixed point so a
 * style can nest inside another: in `**bold *and italic* bold**` the inner
 * pair resolves on the first pass, which leaves the outer one free of
 * stray asterisks for the second. */
const EMPHASIS: [RegExp, string][] = [
  [/\*\*\*(?!\s)([^*]+?)(?<!\s)\*\*\*/g, `${B_OPEN}${I_OPEN}$1${I_CLOSE}${B_CLOSE}`],
  [/\*\*(?!\s)([^*]+?)(?<!\s)\*\*/g, `${B_OPEN}$1${B_CLOSE}`],
  [/\*(?!\s)([^*]+?)(?<!\s)\*/g, `${I_OPEN}$1${I_CLOSE}`],
  [/_(?!\s)([^_]+?)(?<!\s)_/g, `${U_OPEN}$1${U_CLOSE}`],
];

/**
 * Fountain inline emphasis as styled runs. `*italic*`, `**bold**`,
 * `***bold italic***` and `_underline_` become styles; `\*` and `\_` are a
 * literal asterisk and underscore. Unpaired markers stay as written. Text
 * XML cannot carry is removed here, so every run is safe to escape and
 * write.
 */
export function textRuns(source: string): TextRun[] {
  let s = source
    .replace(FORBIDDEN_XML, '')
    .replace(SENTINELS, '')
    .replace(/\\\*/g, LITERAL_STAR)
    .replace(/\\_/g, LITERAL_UNDERSCORE);
  for (;;) {
    const before = s;
    for (const [pattern, replacement] of EMPHASIS) s = s.replace(pattern, replacement);
    if (s === before) break;
  }

  const runs: TextRun[] = [];
  let bold = 0;
  let italic = 0;
  let underline = 0;
  let current = '';
  const flush = () => {
    if (current === '') return;
    const style = [bold > 0 && 'Bold', italic > 0 && 'Italic', underline > 0 && 'Underline']
      .filter(Boolean)
      .join('+');
    const last = runs[runs.length - 1];
    // Two runs in the same style (an empty style pair between them) are one.
    if (last && (last.style ?? '') === style) last.text += current;
    else runs.push(style ? { text: current, style } : { text: current });
    current = '';
  };
  for (const ch of s) {
    switch (ch) {
      case B_OPEN: flush(); bold++; break;
      case B_CLOSE: flush(); bold--; break;
      case I_OPEN: flush(); italic++; break;
      case I_CLOSE: flush(); italic--; break;
      case U_OPEN: flush(); underline++; break;
      case U_CLOSE: flush(); underline--; break;
      case LITERAL_STAR: current += '*'; break;
      case LITERAL_UNDERSCORE: current += '_'; break;
      default: current += ch;
    }
  }
  flush();
  return runs;
}

function escapeText(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function escapeAttr(s: string): string {
  return escapeText(s.replace(FORBIDDEN_XML, '')).replace(/[\t\n\r]/g, ' ');
}

type Attrs = [name: string, value: string | undefined][];

function openTag(attrs: Attrs): string {
  let out = '<Paragraph';
  for (const [name, value] of attrs) {
    if (value !== undefined) out += ` ${name}="${escapeAttr(value)}"`;
  }
  return `${out}>`;
}

/** A whole paragraph on one line. Whitespace inside a Paragraph is text to
 * Final Draft, so nothing is pretty-printed between its runs. */
function paragraph(attrs: Attrs, text: string): string {
  const runs = textRuns(text)
    .map((r) => (r.style ? `<Text Style="${r.style}">` : '<Text>') + escapeText(r.text) + '</Text>')
    .join('');
  return `${openTag(attrs)}${runs || '<Text></Text>'}</Paragraph>`;
}

/** The lines of a token's text worth a paragraph each. A lyrics block or a
 * multi-line speech arrives as ONE token with newlines inside, and Final
 * Draft keeps one line per paragraph (a hard return IS a new paragraph
 * there), so each line becomes its own, in the token's own type. */
function linesOf(text: string): string[] {
  return text.split(/\r?\n/).filter((line) => line.trim() !== '');
}

/** The element Type each content token becomes; tokens absent here (title
 * page, sections, synopses, notes, boneyard, speech and dual markers) are
 * structure or invisible, never a paragraph of their own. */
function paragraphType(t: Token): string | null {
  switch (t.type) {
    case 'scene_heading': return isMiniSlug(t) ? 'Shot' : 'Scene Heading';
    case 'action': return 'Action';
    case 'centered': return 'Action';
    case 'character': return 'Character';
    case 'parenthetical': return 'Parenthetical';
    case 'dialogue': return 'Dialogue';
    case 'lyrics': return 'Lyrics';
    case 'transition': return 'Transition';
    default: return null;
  }
}

function renderBody(tokens: Token[]): string[] {
  const out: string[] = [];
  // A Fountain page break has no paragraph of its own in Final Draft: it is
  // a flag on whatever paragraph comes next.
  let newPage = false;
  // Dual dialogue gathers both speeches, then lands as one wrapper.
  let dual: string[] | null = null;

  const emit = (p: string) => {
    if (dual) dual.push(p);
    else out.push(p);
  };
  const takeNewPage = (): string | undefined => {
    if (!newPage) return undefined;
    newPage = false;
    return 'Yes';
  };

  for (const t of tokens) {
    if (t.is_title) continue;
    if (t.type === 'page_break') {
      newPage = true;
      continue;
    }
    if (t.type === 'dual_dialogue_begin') {
      dual = [];
      continue;
    }
    if (t.type === 'dual_dialogue_end') {
      if (dual && dual.length > 0) {
        // Final Draft nests a dual exchange this way: one outer paragraph
        // holding a DualDialogue, holding both speakers' paragraphs in order.
        out.push(
          `${openTag([['Type', 'General'], ['StartsNewPage', takeNewPage()]])}<DualDialogue>${dual.join('')}</DualDialogue></Paragraph>`,
        );
      }
      dual = null;
      continue;
    }

    const type = paragraphType(t);
    if (type === null) continue;
    let text = stripNotes(t.text ?? '');
    // fountain-js drops a forced transition's `>` itself; this is the same
    // belt the EPUB renderer wears in case it ever does not.
    if (t.type === 'transition') text = text.replace(/^>\s*/, '');
    const number = t.type === 'scene_heading' && t.scene_number ? t.scene_number : undefined;
    const alignment = t.type === 'centered' ? 'Center' : undefined;

    for (const line of linesOf(text)) {
      // Inside a dual exchange the page flag belongs to the wrapper.
      const startsNewPage = dual ? undefined : takeNewPage();
      emit(paragraph(
        [['Type', type], ['Number', number], ['Alignment', alignment], ['StartsNewPage', startsNewPage]],
        line,
      ));
    }
  }
  return out;
}

/** About a page of title-page lines in Final Draft's default layout. The
 * centered block starts a third of the way down and the contact block
 * finishes near the bottom, the way Final Draft's own title page sits. */
const TITLE_PAGE_LINES = 54;
const TITLE_TOP_BLANKS = 18;

function renderTitlePage(tokens: Token[]): string[] {
  const titleTokens = tokens.filter((t) => t.is_title);
  if (titleTokens.length === 0) return [];
  const linesFor = (...types: string[]): string[] =>
    titleTokens
      .filter((t) => types.includes(t.type))
      .flatMap((t) => linesOf(stripNotes(t.text ?? '')));

  const centered: string[][] = [
    linesFor('title'),
    linesFor('credit'),
    linesFor('author', 'authors'),
    linesFor('source'),
  ].filter((group) => group.length > 0);
  const bottom: string[][] = [
    linesFor('contact'),
    linesFor('draft_date', 'date'),
    linesFor('revision', 'revisions'),
    linesFor('copyright'),
    linesFor('notes'),
  ].filter((group) => group.length > 0);
  if (centered.length === 0 && bottom.length === 0) return [];

  const blank = (alignment: string) => paragraph([['Alignment', alignment]], '');
  const out: string[] = [];
  for (let i = 0; i < TITLE_TOP_BLANKS; i++) out.push(blank('Left'));
  centered.forEach((group, i) => {
    if (i > 0) out.push(blank('Center'));
    for (const line of group) out.push(paragraph([['Alignment', 'Center']], line));
  });
  // A blank line between bottom groups, counted before the filler so the
  // last line lands near the foot of the page.
  const bottomLines = bottom.reduce((n, group) => n + group.length, 0) + Math.max(0, bottom.length - 1);
  const filler = Math.max(2, TITLE_PAGE_LINES - out.length - bottomLines);
  if (bottom.length > 0) for (let i = 0; i < filler; i++) out.push(blank('Left'));
  bottom.forEach((group, i) => {
    if (i > 0) out.push(blank('Left'));
    for (const line of group) out.push(paragraph([['Alignment', 'Left']], line));
  });
  return out;
}

/** The whole FDX document for a script's fountain-js tokens. */
export function tokensToFdx(tokens: Token[]): string {
  const body = renderBody(tokens);
  const title = renderTitlePage(tokens);
  const lines = [
    XML_DECLARATION,
    `<FinalDraft DocumentType="Script" Template="No" Version="${FDX_VERSION}">`,
    '  <Content>',
    ...body.map((p) => `    ${p}`),
    '  </Content>',
  ];
  if (title.length > 0) {
    lines.push('  <TitlePage>', '    <Content>', ...title.map((p) => `      ${p}`), '    </Content>', '  </TitlePage>');
  }
  lines.push('</FinalDraft>', '');
  return lines.join('\n');
}

/** The FDX for a .fountain's text: the same tokenizer call the EPUB and
 * MOBI renderers are fed from (convert.ts's renderBooks). */
export function fountainToFdx(fountainText: string): string {
  const { tokens } = new Fountain().parse(fountainText, true);
  return tokensToFdx(tokens);
}
