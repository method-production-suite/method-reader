// fountain-js tokens → MOBI 6 HTML. The old MOBI renderer speaks a small
// HTML-3.2-ish dialect: no CSS, indents via <blockquote>, alignment via
// attributes. Screenplay mapping keeps structure readable on e-ink:
// bold sluglines, blockquoted speeches with bold cues, right-flush
// transitions. (The EPUB path stays the high-fidelity rendering; this is
// the dependency-free USB sideload format.)
// This dialect has exactly one fragmentation primitive: <mbp:pagebreak/>.
// It is wired to scenePageBreaks (registry #1), before PRIMARY scene
// headings only — a mini-slug is a micro-heading inside the current
// scene, not a new scene (registry #5b), so breaking before one would be
// wrong the same way it would be in the EPUB's section.scene handling.
// The other knob this dialect reads is includeTitlePage (#11). What it
// cannot reach is the CSS ones: it ships no stylesheet (#8c), so every
// keep, margin and alignment knob is EPUB territory. Stage-1 knobs (#8,
// #8a, #10a's mode, #13a's markers) arrive already baked into the
// .fountain and need nothing here.
// It also reads preserveFontShifts (#18), but only the SIZE arm: <font
// size> is a primitive this dialect has, while MOBI 6 face support is too
// unreliable to spend markup on, so family stays EPUB territory.
// And includeImages (#19a), through mobiArt, which asks the EPUB's own
// question. A picture note's `w=` is the knob it declines, for the reason
// the CSS ones are declined: a share of the column has no spelling here.
// promoteCoverImage (#19b) reaches the file only through mobiArt's
// coverIndex, which the writer turns into a header record: the pages here
// are the same with it on or off.
import type { Token } from 'fountain-js';
import type { FormatOptions } from '../options';
import { isMiniSlug } from '../fountain/slug';
import { fmtSizeStep, rendersImage, stripNotes } from '../fountain/notes';
import { pickCover } from '../fountain/cover';

export interface MobiMeta {
  title: string;
  author?: string;
}

/**
 * The pictures one MOBI carries, in the order its image records hold them.
 *
 * A MOBI addresses a picture by RECORD, not by name: `recindex="00002"` is the
 * second image record, and nothing in the file says which picture that was.
 * The EPUB's markup names `../images/p002-01.png`, so its packager may be
 * handed the pictures in any order; here the markup and the records are two
 * ends of one numbering, and a disagreement between them does not lose a
 * picture, it shows the WRONG one. So the numbering exists once, in this
 * list: `tokensToMobiHtml` numbers its figures from it and the caller hands
 * `buildMobi` the bytes in this same order.
 */
export interface MobiArt {
  /** `order[i]` is the picture `recindex` i+1 names. */
  order: readonly string[];
  /**
   * Which of `order` is the book's cover (registry #19b), when `pickCover`
   * chose one. The card stays in `order` and on its page: the cover names a
   * record the flow already shows, it does not add one.
   */
  coverIndex?: number;
}

/**
 * THE SAFETY GATE on pictures in the engine's own MOBI (registry #19a, #19b).
 *
 * The .mobi is the file for the oldest Kindles, the ones with no Calibre
 * route, and no device has yet opened one carrying picture records. Older
 * firmware is reported to cap a record near 127 KB, and a failure there could
 * leave the whole book unopenable rather than merely without its pictures.
 * So until a device probe passes, every route builds the .mobi text-only: no
 * image records, no EXTH 201 cover, the book from before pictures, byte for
 * byte. The EPUB, and the AZW3 and KFX Calibre builds from it, keep both.
 *
 * The probe is `bun tools/mobi-picture-probe.ts --out <folder>`, read on an
 * older Kindle with tools/device-checklist.ts's picture rows. When it passes,
 * this is the one line to change: true, and every route's .mobi carries the
 * EPUB's pictures and names its cover, as the code below already does.
 * `ConvertOptions.mobiPictures` overrides it for one build (the probe itself,
 * and the tests that keep the records proven while the gate is shut).
 */
export const MOBI_CARRIES_PICTURES = false;

/**
 * Which pictures this token stream puts in a MOBI, in document order, each
 * once: a picture shown twice is one record with two references.
 *
 * Decided by `rendersImage`, the EPUB's own predicate, so the EPUB and the
 * MOBI of one script show the same pictures. `have` is what the caller can
 * supply bytes for, as it is for the EPUB. It walks the tokens the renderer
 * walks (the title page's are not the body), and only `note` tokens: a note
 * glued to a line of text is text, and is stripped as text.
 *
 * The cover is `pickCover`'s answer for the same tokens, format and `have`,
 * the EPUB's own, so both books of a script wear the same cover or none.
 * The card it picks is a picture this walk also lists, since the picker
 * chooses only what `rendersImage` would draw and this renderer draws it.
 */
export function mobiArt(
  tokens: Token[],
  format: FormatOptions,
  have: ReadonlySet<string>,
): MobiArt {
  const order: string[] = [];
  for (const t of tokens) {
    if (t.is_title || t.type !== 'note') continue;
    const img = rendersImage(t.text ?? '', format.includeImages, have);
    if (img && !order.includes(img.file)) order.push(img.file);
  }
  const cover = pickCover(tokens, format, have);
  return cover ? { order, coverIndex: order.indexOf(cover.file) } : { order };
}

/**
 * The record number a figure's `recindex` names: 1-based from the header's
 * first-image-record field, zero-padded to five digits. That is the form
 * calibre 9.11.0 and kindlegen 2.9 both wrote in MOBI 6 files of the same
 * book (measured 2026-08-11 with `calibre-debug --inspect-mobi`).
 */
const recindex = (i: number) => String(i + 1).padStart(5, '0');

function esc(s: string): string {
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

/** Fountain inline emphasis → old-MOBI tags, applied AFTER escaping. */
function inline(escaped: string): string {
  return escaped
    .replace(/\*\*\*(?!\s)([^*]+?)(?<!\s)\*\*\*/g, '<b><i>$1</i></b>')
    .replace(/\*\*(?!\s)([^*]+?)(?<!\s)\*\*/g, '<b>$1</b>')
    .replace(/\*(?!\s)([^*]+?)(?<!\s)\*/g, '<i>$1</i>')
    .replace(/_(?!\s)([^_]+?)(?<!\s)_/g, '<u>$1</u>');
}

export function tokensToMobiHtml(
  tokens: Token[],
  meta: MobiMeta,
  format: FormatOptions,
  /** What `mobiArt` decided for these tokens and this format. No pictures by
   * default: the honest answer for a caller with no records to address. */
  art: MobiArt = { order: [] },
): string {
  const out: string[] = [];
  out.push('<html><head></head><body>');

  // Body content emitted since the last page break. It gates the scene
  // break below: a break is only wanted where there is something to break
  // AWAY from, so the first scene of a book gets none (the title page's
  // own break, or the start of the file, already put it at the top of a
  // page) while a scene that follows opening action does — matching the
  // EPUB, where section.scene's page-break-before separates exactly that.
  let sinceBreak = false;
  // A break owed AFTER a full-page plate, paid only by the next block, so a
  // plate that ends the book leaves no blank page behind it. Any break
  // settles it, which is why a scene heading straight after a plate gets
  // one break and not two.
  let breakOwed = false;
  const breakPage = () => {
    out.push('<mbp:pagebreak/>');
    sinceBreak = false;
    breakOwed = false;
  };

  // Title page (registry #11's MOBI arm — gated like the EPUB's, which
  // drops the file, its manifest item, spine itemref and nav landmark).
  if (format.includeTitlePage) {
    out.push('<center>');
    out.push(`<br/><br/><br/><b><font size="+2">${esc(meta.title)}</font></b>`);
    if (meta.author) out.push(`<br/><br/>Written by<br/>${esc(meta.author)}`);
    out.push('</center>');
    breakPage();
  }

  // Body: dialogue blocks accumulate into a single <blockquote>.
  let speech: string[] | null = null;
  let dual: { left: string[]; right: string[]; side: 'left' | 'right' } | null = null;
  // Page markers ride inside the NEXT block instead of taking a line of
  // their own — same rule as the EPUB, minus the EPUB3 semantics (no
  // epub:type/role/id here). There is no stylesheet in this dialect to hang
  // a float on, so the number reads as a small prefix at the head of the
  // block; <font size="-2"> is how this file expresses "small".
  let pendingMarker: string | null = null;
  // A block, with no claim on the pending page marker, paying first any
  // break a full-page plate left owing. `push` is this plus the marker; call
  // this one directly only where a block must DECLINE a marker, which today
  // is the figure alone (the EPUB's `place`/`emit`).
  const place = (s: string) => {
    if (breakOwed) breakPage();
    out.push(s);
    sinceBreak = true;
  };
  const push = (s: string) => {
    if (pendingMarker) {
      const open = /^<(p|blockquote|center|table|div|h[1-6])\b[^>]*>/.exec(s);
      if (open) {
        s = s.slice(0, open[0].length) + pendingMarker + s.slice(open[0].length);
        pendingMarker = null;
      }
    }
    place(s);
  };
  // The pictures this book has records for. A note is a figure when
  // rendersImage says yes against THIS set, so a picture with no record has
  // no `recindex` to dangle.
  const inBook = new Set(art.order);
  const closeSpeech = () => {
    if (speech) {
      push(`<blockquote>${speech.join('<br/>')}</blockquote>`);
      speech = null;
    }
  };

  for (const t of tokens.filter((t) => !t.is_title)) {
    const raw = t.text ?? '';
    // Read the shift before stripping. Every token type is stripped in one
    // place: Fountain notes are invisible by spec.
    const step = format.preserveFontShifts ? fmtSizeStep(raw) : undefined;
    const sized = (s: string) => (step ? `<font size="${step}">${s}</font>` : s);
    const text = stripNotes(raw);
    switch (t.type) {
      case 'scene_heading': {
        closeSpeech();
        // The only break primitive this dialect has (registry #1's MOBI
        // arm), and only before a PRIMARY scene heading — a mini-slug
        // stays inside the scene it appears in (registry #5b). breakPage,
        // not push(): a pending page marker belongs to the heading block,
        // never to the break itself.
        if (format.scenePageBreaks && !isMiniSlug(t) && sinceBreak) breakPage();
        push(`<p><b>${esc(text)}</b></p>`);
        break;
      }
      case 'action':
      case 'lyrics':
        closeSpeech();
        push(`<p>${sized(inline(esc(text)))}</p>`);
        break;
      case 'dual_dialogue_begin':
        dual = { left: [], right: [], side: 'left' };
        break;
      case 'dual_dialogue_end':
        if (dual) {
          push(
            `<table width="100%"><tr><td width="50%">${dual.left.join('<br/>')}</td><td width="50%">${dual.right.join('<br/>')}</td></tr></table>`,
          );
          dual = null;
        }
        break;
      case 'dialogue_begin':
        if (dual) dual.side = (t as { dual?: string }).dual === 'right' ? 'right' : 'left';
        speech = [];
        break;
      case 'character':
        if (speech) speech.push(`<b>${esc(text)}</b>`);
        break;
      case 'parenthetical':
        if (speech) speech.push(`<i>${esc(text)}</i>`);
        break;
      case 'dialogue':
        if (speech) speech.push(sized(inline(esc(text)).replace(/\n/g, '<br/>')));
        break;
      case 'dialogue_end':
        if (dual && speech) {
          dual[dual.side].push(...speech);
          speech = null;
        } else {
          closeSpeech();
        }
        break;
      case 'transition':
        closeSpeech();
        push(`<p align="right">${esc(text.replace(/^>\s*/, ''))}</p>`);
        break;
      case 'centered':
        closeSpeech();
        push(`<center>${esc(text)}</center>`);
        break;
      case 'synopsis': {
        // Emits no block of its own: it waits for the next one. That also
        // means a marker no longer splits a speech's blockquote in two.
        const pg = /^pg\s+(\S+)$/.exec(text.trim());
        if (pg) {
          pendingMarker = `<span class="page-marker"><font size="-2">${esc(pg[1])}.</font></span>`;
        }
        break;
      }
      case 'note': {
        // A standalone `[[img: ...]]` is a picture carried out of the PDF
        // (registry #19). Every other note, and a picture note the predicate
        // turns down, renders nothing: notes are invisible by spec.
        //
        // A title card that is the book's cover is drawn here too, where the
        // EPUB leaves it off its pages (#19b). Calibre's MOBI reader takes a
        // cover only from a JPEG record, and every picture here is a PNG, so
        // a card kept only as the cover could vanish from the book; kept on
        // its page, the worst case is the card met twice.
        const img = rendersImage(raw, format.includeImages, inBook);
        if (!img) break;
        closeSpeech();
        // The EPUB's p.figure-full in this dialect's one break primitive: a
        // break before the plate and one after, so it has a page to itself.
        // Not gated by scenePageBreaks, as the EPUB's is not: the script
        // itself gave this picture a page. Neither break is taken where there
        // is nothing to break from (the start of the body) or away from (the
        // end), which is what Blink does with the EPUB's (measured
        // 2026-08-11), so neither end of the book gets a blank page.
        if (img.fullPage && sinceBreak) breakPage();
        // A centred <p>: this dialect aligns by attribute, as the transition
        // does, and the EPUB's figure is a <p> too. `alt` for the EPUB's
        // reason (an empty one calls the picture decoration), and kindlegen
        // 2.9 wrote one in its own MOBI 6, so this dialect has it.
        //
        // No width, and that is a refusal rather than an omission. The note's
        // `w=` is a share of the text column; the only width attested in this
        // dialect is a pixel count, and not even the picture's own: calibre
        // 9.11.0 wrote width="672" height="504" for a 240x180 plate (measured
        // 2026-08-11). There is no share to write and no honest pixel count
        // to compute, so the device sizes the picture.
        //
        // `place`, not `push`: a figure declines a pending page marker,
        // because a picture is not a line of text a reader can land on. The
        // EPUB holds the same rule, and serialize.ts's anchorsPageMarker
        // holds it upstream of the .fountain.
        place(`<p align="center"><img recindex="${recindex(art.order.indexOf(img.file))}" alt="Illustration"/></p>`);
        if (img.fullPage) breakOwed = true;
        break;
      }
      default:
        break;
    }
  }
  closeSpeech();

  out.push('</body></html>');
  return out.join('\n');
}
