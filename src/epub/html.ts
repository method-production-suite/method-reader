// fountain-js tokens → XHTML body files. Scenes flow continuously as
// anchored <section>s inside one file — a spine boundary forces a page break
// in every reader, so files split only when a size budget is exceeded.
import type { Token } from 'fountain-js';
import type { FormatOptions } from '../options';
import { DEFAULT_FORMAT_OPTIONS } from '../options';
import { screenplayCss } from './css';
import { isMiniSlug } from '../fountain/slug';
import { fmtClasses, stripNotes } from '../fountain/notes';

export interface BodyFile {
  /** filename-safe id, e.g. "body001" */
  id: string;
  /** complete XHTML document */
  xhtml: string;
}

export interface TocEntry {
  title: string;
  /** href relative to OEBPS/, e.g. "text/body001.xhtml#sc-001" */
  href: string;
}

export interface BookBody {
  files: BodyFile[];
  toc: TocEntry[];
  /** Page markers, in document order, for the EPUB3 page-list nav. */
  pageList: { label: string; href: string }[];
}

/** Keep each body file comfortably under Kindle's per-flow size warnings. */
const DEFAULT_MAX_FILE_BYTES = 250_000;

/** A dual exchange whose taller column exceeds this many estimated
 * rendered lines is taller than the space typically left at a page
 * bottom; the unbreakable table then pushes whole, wasting up to its own
 * height, so it degrades to sequential speeches. Twelve lines is roughly
 * half a typical device page — past that the odds of a costly push climb
 * faster than the side-by-side reading is worth. The estimate assumes
 * ~30 chars per half-width line and counts each cell's tag-stripped
 * text, including its trailing newline. */
const DUAL_SEQUENTIAL_LINE_THRESHOLD = 12;
const EST_CHARS_PER_DUAL_LINE = 30;

/** One rendered paragraph inside a speech. `kind` drives the cue keep, so
 * it is a closed set — a typo'd literal would silently wrap every speech. */
type Cell = { kind: 'character' | 'parenthetical' | 'dialogue' | 'other'; html: string };

export function escapeXml(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&apos;');
}

/** Fountain inline emphasis → XHTML, applied AFTER escaping. */
function inlineEmphasis(escaped: string): string {
  return escaped
    .replace(/\*\*\*(?!\s)([^*]+?)(?<!\s)\*\*\*/g, '<strong><em>$1</em></strong>')
    .replace(/\*\*(?!\s)([^*]+?)(?<!\s)\*\*/g, '<strong>$1</strong>')
    .replace(/\*(?!\s)([^*]+?)(?<!\s)\*/g, '<em>$1</em>')
    .replace(/_(?!\s)([^_]+?)(?<!\s)_/g, '<span class="underline">$1</span>');
}

function xhtmlDoc(title: string, body: string): string {
  return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head>
<title>${escapeXml(title)}</title>
<link rel="stylesheet" type="text/css" href="../style.css"/>
</head>
<body>
${body}</body>
</html>
`;
}

/**
 * A page marker waiting for a block to ride inside. Carried ACROSS scene
 * sections (each section is its own `renderBlocks` call) so a marker that
 * lands at a scene seam attaches to the next slugline instead of vanishing.
 */
interface MarkerState {
  pending: { label: string; html: string } | null;
  /**
   * Labels that actually LANDED in the blocks of the current call — a marker
   * is only navigable once it has an anchor, and it belongs to whichever
   * section (hence file) absorbed it.
   */
  landed: string[];
}

/**
 * Render body tokens (no title-page tokens) as discrete blocks — one string
 * per paragraph, with a complete dialogue block as a single string.
 */
function renderBlocks(
  tokens: Token[],
  format: FormatOptions,
  markers: MarkerState = { pending: null, landed: [] },
): string[] {
  const blocks: string[] = [];
  let speech: Cell[] | null = null;
  // Simultaneous speech renders as a two-cell table — the one column
  // construct Kindle's renderer honors (floats/inline-block are not
  // reliable under Enhanced Typesetting).
  let dual: { left: Cell[]; right: Cell[]; side: 'left' | 'right' } | null = null;
  const emit = (s: string, kind: Cell['kind'] = 'other') => {
    // A pending page marker slips inside this block's opening tag — costing
    // no line of its own — and is consumed once it lands.
    if (markers.pending) {
      const open = /^<(p|h[1-6]|div)\b[^>]*>/.exec(s);
      if (open) {
        s = s.slice(0, open[0].length) + markers.pending.html + s.slice(open[0].length);
        markers.landed.push(markers.pending.label);
        markers.pending = null;
      }
    }
    if (speech) speech.push({ kind, html: s });
    else blocks.push(s);
  };
  // A speech is a flat run of paragraphs: cue, any parentheticals, then the
  // dialogue. NO wrapper. What holds a cue to its speech is the CSS chain on
  // the paragraphs themselves — break-after: avoid on p.character and
  // p.parenthetical — plus orphans on p.dialogue for the page edge (#17).
  //
  // The wrapper is gone because it was the thing breaking this. A
  // `<div class="keep-together">` carried the forward bind, and the KFX
  // converter behind Send-to-Kindle honors break-after on elements that
  // carry text (settled on h2.scene-heading, #5a) but not on a structural
  // div, and does not propagate a last child's break-after to its parent.
  // So p.character's own rule governed a break inside the wrapper that can
  // never be taken, the wrapper→dialogue boundary went ungoverned, and cues
  // stranded on device (photo-confirmed 2026-09-14, #8b).
  //
  // This is the shape #5a already proved on this renderer when the heading
  // wrapper was deleted for the same reason. Keep it flat.
  const speechBlock = (cells: Cell[]): string =>
    `<div class="dialogue-block">\n${cells.map((c) => c.html).join('')}</div>\n`;
  const closeSpeech = () => {
    if (!speech) return;
    blocks.push(speechBlock(speech));
    speech = null;
  };

  for (const t of tokens) {
    const raw = t.text ?? '';
    // Read the shift BEFORE stripping, then strip every note from every token
    // type in one place. Notes are invisible by spec; this is also the general
    // fix for hand-written ones, which used to render as literal text.
    const fmt = format.preserveFontShifts ? fmtClasses(raw) : '';
    const text = stripNotes(raw);
    switch (t.type) {
      case 'scene_heading': {
        const num = format.showSceneNumbers && t.scene_number
          ? `<span class="scene-number">${escapeXml(t.scene_number)}</span> `
          : '';
        // A mini-slug can carry a shooting-script number too (fountain-js
        // splits `#5#` off either kind of heading) — dropping it here would
        // lose it silently.
        emit(isMiniSlug(t)
          ? `<p class="mini-slug">${num}${escapeXml(text)}</p>\n`
          : `<h2 class="scene-heading">${num}${escapeXml(text)}</h2>\n`);
        break;
      }
      case 'action':
        emit(`<p class="action${fmt}">${inlineEmphasis(escapeXml(text))}</p>\n`);
        break;
      case 'dual_dialogue_begin':
        dual = { left: [], right: [], side: 'left' };
        break;
      case 'dual_dialogue_end':
        if (dual) {
          const estLines = (col: Cell[]) =>
            col.reduce((n, c) => {
              const len = c.html.replace(/<[^>]*>/g, '').length;
              return n + Math.max(1, Math.ceil(len / EST_CHARS_PER_DUAL_LINE));
            }, 0);
          if (Math.max(estLines(dual.left), estLines(dual.right)) > DUAL_SEQUENTIAL_LINE_THRESHOLD) {
            if (dual.left.length) blocks.push(speechBlock(dual.left));
            if (dual.right.length) blocks.push(speechBlock(dual.right));
          } else {
            blocks.push(
              `<table class="dual-dialogue">\n<tr>\n<td>\n${dual.left.map((c) => c.html).join('')}</td>\n<td>\n${dual.right.map((c) => c.html).join('')}</td>\n</tr>\n</table>\n`,
            );
          }
          dual = null;
        }
        break;
      case 'dialogue_begin':
        if (dual) dual.side = (t as { dual?: string }).dual === 'right' ? 'right' : 'left';
        speech = [];
        break;
      case 'dialogue_end':
        if (dual && speech) {
          dual[dual.side].push(...speech);
          speech = null;
        } else {
          closeSpeech();
        }
        break;
      case 'character':
        emit(`<p class="character">${escapeXml(text)}</p>\n`, 'character');
        break;
      case 'parenthetical':
        emit(`<p class="parenthetical">${escapeXml(text)}</p>\n`, 'parenthetical');
        break;
      case 'dialogue':
        // Multi-line speech arrives as one token with embedded newlines
        // (lyrics, verse) — each line becomes its own paragraph.
        for (const line of text.split('\n')) {
          if (line.trim()) emit(`<p class="dialogue${fmt}">${inlineEmphasis(escapeXml(line))}</p>\n`, 'dialogue');
        }
        break;
      case 'transition':
        emit(`<p class="transition">${escapeXml(text.replace(/^>\s*/, ''))}</p>\n`);
        break;
      case 'centered':
        emit(`<p class="centered">${inlineEmphasis(escapeXml(text))}</p>\n`);
        break;
      case 'lyrics':
        emit(`<p class="action${fmt}">${inlineEmphasis(escapeXml(text))}</p>\n`);
        break;
      case 'synopsis': {
        // "= pg N" lines are our page markers; other synopses stay invisible.
        // The marker doesn't emit a block — it waits for the next one, and
        // the same span doubles as the EPUB3 pagination anchor.
        const pg = /^pg\s+(\S+)$/.exec(text.trim());
        if (pg) {
          const label = escapeXml(pg[1]);
          markers.pending = {
            label: pg[1],
            html: `<span epub:type="pagebreak" role="doc-pagebreak" id="pg${label}"`
              + ` title="${label}" class="page-marker">${label}.</span>`,
          };
        }
        break;
      }
      default:
        // structural/no-render tokens: title page, page breaks, notes, etc.
        break;
    }
  }
  closeSpeech();
  return blocks;
}

interface SceneSection {
  anchor: string;
  title: string;
  html: string;
  /** page-marker labels anchored inside this section, in document order */
  pageLabels: string[];
}

/**
 * Split body tokens into anchored scene sections, then pack sections into
 * as few files as the size budget allows. Content before the first scene
 * heading becomes an "Opening" section.
 */
export function tokensToBody(
  tokens: Token[],
  opts: { maxFileBytes?: number; format?: FormatOptions } = {},
): BookBody {
  const maxBytes = opts.maxFileBytes ?? DEFAULT_MAX_FILE_BYTES;
  const format = opts.format ?? DEFAULT_FORMAT_OPTIONS;
  const body = tokens.filter((t) => !t.is_title);

  const groups: { title: string; tokens: Token[] }[] = [];
  let current: { title: string; tokens: Token[] } | null = null;

  for (const t of body) {
    if (t.type === 'scene_heading' && !isMiniSlug(t)) {
      current = { title: t.text ?? 'Scene', tokens: [t] };
      groups.push(current);
    } else {
      // A standalone note renders nothing (renderBlocks' default case), so
      // one may not be what opens the Opening: a group of nothing but notes is
      // an empty section, which is an "Opening" in the contents, a blank page
      // under scenePageBreaks and every scene anchor one along. toFountain
      // writes a title card's `[[img: ...]]` exactly there. Dropping it loses
      // nothing, since there is no section yet for it to be invisible in.
      if (!current && t.type === 'note') continue;
      if (!current) {
        current = { title: 'Opening', tokens: [] };
        groups.push(current);
      }
      current.tokens.push(t);
    }
  }

  // One marker state for the whole script: a page marker at the tail of one
  // scene rides into the first block of the next.
  const markers: MarkerState = { pending: null, landed: [] };
  const sections: SceneSection[] = groups.map((g, i) => {
    const anchor = `sc-${String(i + 1).padStart(3, '0')}`;
    markers.landed = [];
    const html = `<section class="scene" id="${anchor}">\n${renderBlocks(g.tokens, format, markers).join('')}</section>\n`;
    return { anchor, title: g.title, html, pageLabels: markers.landed };
  });

  // Pack sections into files, starting a new file only when the budget
  // would be exceeded (a file always holds at least one section).
  const files: BodyFile[] = [];
  const toc: TocEntry[] = [];
  const pageList: BookBody['pageList'] = [];
  let pending: SceneSection[] = [];
  let pendingBytes = 0;

  const flush = () => {
    if (pending.length === 0) return;
    const id = `body${String(files.length + 1).padStart(3, '0')}`;
    for (const s of pending) {
      toc.push({ title: s.title, href: `text/${id}.xhtml#${s.anchor}` });
      // fragment must match the id renderBlocks stamped on the anchor span
      for (const label of s.pageLabels) {
        pageList.push({ label, href: `text/${id}.xhtml#pg${escapeXml(label)}` });
      }
    }
    files.push({ id, xhtml: xhtmlDoc(pending[0].title, pending.map((s) => s.html).join('')) });
    pending = [];
    pendingBytes = 0;
  };

  for (const s of sections) {
    const size = Buffer.byteLength(s.html, 'utf8');
    if (pending.length > 0 && pendingBytes + size > maxBytes) flush();
    pending.push(s);
    pendingBytes += size;
  }
  flush();

  return { files, toc, pageList };
}

/**
 * The whole script as ONE self-contained HTML document — the same section
 * markup as the EPUB body, with the stylesheet inlined instead of linked.
 * This is the app's reader-preview surface: what you proof is what ships.
 */
export function tokensToPreviewHtml(
  tokens: Token[],
  format: Partial<FormatOptions> = {},
): string {
  const resolved: FormatOptions = { ...DEFAULT_FORMAT_OPTIONS, ...format };
  const body = tokensToBody(tokens, {
    maxFileBytes: Number.MAX_SAFE_INTEGER,
    format: resolved,
  });
  const doc = body.files[0]?.xhtml ?? xhtmlDoc('Script', '');
  return doc.replace(
    /<link rel="stylesheet"[^>]*\/>/,
    () => `<style>\n${screenplayCss(resolved)}</style>`,
  );
}
