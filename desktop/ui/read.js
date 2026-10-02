// Read: the converted script, keeping its shape, with its scene rail.
//
// The engine produces this document (tokensToPreviewHtml) and its stylesheet
// (src/epub/css.ts). The reader renders exactly those, so a formatting
// decision cannot disagree between this window and the EPUB a reader
// actually gets. Nothing here styles a screenplay: there is no rule below
// for a slugline, a cue or a line of dialogue, and there must never be one.
//
// HOW, and why it is not simpler than this — all four routes were measured
// in a real window before this was written:
//
//   * The window's CSP is `default-src 'self'`, which blocks every inline
//     style, INCLUDING a <style> inside an <iframe srcdoc>: srcdoc frames
//     inherit the parent's policy. Leaving the engine's <style> in place
//     renders an unstyled script with no error anywhere.
//   * CSSOM is not blocked. So the <style> is lifted out of the document
//     and its text is adopted as a constructed stylesheet instead — same
//     bytes, same rules, nothing rewritten.
//   * The sheet must be constructed in the FRAME's realm. A CSSStyleSheet
//     built in this document is rejected when adopted into another.
//   * The asset protocol renders but is cross-origin: the parent cannot
//     script the frame (no place-keeping, no rail) and the frame cannot
//     reach the bundled fonts. Rejected.
//
// The frame is an iframe rather than a div because the engine's stylesheet
// has `html, body` rules, and an iframe is the only container that has an
// html and a body of its own to give them.
//
// The file is in two halves, the way convert.js is. Everything this surface
// DECIDES — what the rail says, where the reader is, what to render when
// there is nothing to read, how the engine's document is taken apart — is a
// pure exported function above the line, tested directly. Below the line is
// drawing, which holds no rule of its own and rides on the live run.
import { el, clear } from './dom.js';
import { SEND_BOOK } from './device.js';

// ---------------------------------------------------------------- decisions

/** The faces the engine's CSS asks for by name. Nothing inside the frame
 *  knows where those files are, so the reader says — the same four files
 *  style.css declares for the window itself, latin and latin-ext, so an
 *  accented name is not a different typeface inside the frame than outside
 *  it. This adds a face; it changes no rule the engine wrote. */
export const FONT_CSS = `
@font-face { font-family: "Courier Prime"; font-style: normal; font-weight: 400;
  src: url(fonts/courier-prime-400-latin.woff2) format("woff2"); }
@font-face { font-family: "Courier Prime"; font-style: normal; font-weight: 400;
  src: url(fonts/courier-prime-400-latin-ext.woff2) format("woff2");
  unicode-range: U+0100-02BA, U+1E00-1E9F, U+2C60-2C7F, U+A720-A7FF; }
@font-face { font-family: "Courier Prime"; font-style: normal; font-weight: 700;
  src: url(fonts/courier-prime-700-latin.woff2) format("woff2"); }
@font-face { font-family: "Courier Prime"; font-style: normal; font-weight: 700;
  src: url(fonts/courier-prime-700-latin-ext.woff2) format("woff2");
  unicode-range: U+0100-02BA, U+1E00-1E9F, U+2C60-2C7F, U+A720-A7FF; }
`;

/** The two tokens the frame needs by value rather than by name: a custom
 *  property declared on this document does not cascade into another one. */
export const PAPER_TOKENS = ['ink', 'paper'];

/** The theme, read off whatever declares it. The window hands this the
 *  computed style of its own root; a test hands it a reader that records
 *  what was asked for. Splitting it out is what makes the whole path —
 *  which properties are read, what comes back, what reaches the frame's
 *  sheet — checkable, instead of only checkable that the reading happened. */
export function paperFrom(read) {
  const tokens = {};
  for (const name of PAPER_TOKENS) tokens[name] = read(`--${name}`);
  return tokens;
}

/** The page margin the frame reads at. It is NOT a --space token, and that
 *  is the point: those are rem values for the window's own furniture, and
 *  they do not cross into the frame anyway (a custom property declared on
 *  this document does not cascade into another one). What this margin owes
 *  is the rule the EPUB's own stylesheet is held to — horizontal in %,
 *  vertical in em — so it scales with the reader's type size the way a
 *  device's margin does, and stays proportional to the column at any window
 *  width. tests/desktop-ui.test.ts pins both units so it cannot drift into
 *  px or rem. */
export const PAGE_MARGIN = { block: '1.6em', inline: '7%' };

/** What a reading device supplies on top of a book: the page margins and
 *  the theme it is read in. The engine's stylesheet sets NEITHER on purpose
 *  — `html, body { margin: 0; padding: 0 }` leaves margins to the device,
 *  and a background on html or body is the one thing an EPUB must never
 *  carry (it makes the KFX converter synthesize a wrapper block and every
 *  keep in the book dies silently). So this layer exists for the same reason
 *  a Kindle has one, it is applied the way a Kindle applies it — last, over
 *  the book's own sheet — and it goes no further than a margin and a colour.
 *  A token that did not resolve is left out rather than written as blank. */
export function deviceCss(tokens) {
  const value = (name) => String(tokens?.[name] ?? '').trim();
  const ink = value('ink');
  const paper = value('paper');
  const colours = [
    ink === '' ? null : `color: ${ink};`,
    paper === '' ? null : `background: ${paper};`,
  ].filter((part) => part !== null).join(' ');
  return `html { ${colours} }\n`
    + `body { padding: ${PAGE_MARGIN.block} ${PAGE_MARGIN.inline}; }\n`;
}

/** The whole sheet the frame adopts, in the order a device applies it: the
 *  faces first, then the book's own stylesheet UNTOUCHED, then the device
 *  layer over it. The engine's bytes are neither rewritten nor overridden
 *  by anything except the margin and the theme above. */
export function sheetText(engineCss, tokens) {
  return `${FONT_CSS}\n${String(engineCss ?? '')}\n${deviceCss(tokens)}`;
}

/** The engine's document, minus the stylesheet the CSP will not run.
 *  The parser is passed in rather than reached for so this can be exercised
 *  without a browser; the window hands it a real DOMParser. */
export function splitPreview(previewHtml, parser) {
  const parsed = parser.parseFromString(String(previewHtml ?? ''), 'text/html');
  const styleEl = parsed.querySelector('style');
  const css = styleEl === null ? '' : String(styleEl.textContent ?? '');
  if (styleEl !== null) styleEl.remove();
  // The engine writes XHTML with an XML prolog. Taking the document element
  // and re-declaring the doctype drops the prolog, which a text/html parse
  // would otherwise leave in front of the doctype and put the frame into
  // quirks mode — where the engine's vertical rhythm is not what it renders.
  return { css, html: `<!doctype html>${parsed.documentElement.outerHTML}` };
}

/** What the surface has to show. `blank` is not hypothetical: scriptFrom()
 *  normalises a missing preview to an empty string, so an engine that
 *  answered without one lands here rather than on an empty white frame. */
export function readerState(script) {
  if (script === null || script === undefined) return 'closed';
  const html = typeof script.previewHtml === 'string' ? script.previewHtml.trim() : '';
  return html === '' ? 'blank' : 'ready';
}

/** The one state that is not the script and can still be reached.
 *
 *  There is deliberately no notice for `closed`. main.js disables the Read
 *  tab whenever no script is open and there is no way to close one, so a
 *  reader can never be standing on this surface with nothing converted —
 *  the invitation belongs to Convert, which is the only surface reachable
 *  then, and says it better with the drop well. An empty reader IS an
 *  invitation rather than a blank panel; it is just that the invitation is
 *  on the other tab. Copy nobody can see reads as a considered empty state
 *  to the next person who maintains it, which is worse than none. */
export const NOTICES = {
  blank: {
    slug: 'No pages came back',
    line: 'The engine converted this script but sent no pages to read. The book '
      + 'itself is fine and can still be sent; converting again is the way to '
      + 'get the pages back.',
    way: 'Convert it again',
  },
};

/** The word the engine's own table of contents uses for the run of script
 *  before the first slugline (src/epub/html.ts). The rail says what the
 *  book says. */
export const OPENING = 'Opening';

/** A slugline reads in two parts: where, and when. Splitting it at the LAST
 *  separator keeps a compound place whole — "INT./EXT. DELIVERY VAN -
 *  MOVING - LATER" is a van that is moving, later — and lets the rail set
 *  the time of day quietly beside the place instead of running one wall of
 *  identical capitals down the margin. A heading with no separator is all
 *  place; a scene with no heading is the book's Opening. */
export function sceneLabel(heading) {
  const whole = String(heading ?? '').replace(/\s+/g, ' ').trim();
  if (whole === '') return { place: OPENING, time: null };
  const separator = /\s[-–—]\s/g;
  let cut = -1;
  for (const match of whole.matchAll(separator)) cut = match.index;
  if (cut <= 0) return { place: whole, time: null };
  const place = whole.slice(0, cut).trim();
  const time = whole.slice(cut + 3).trim();
  if (place === '' || time === '') return { place: whole, time: null };
  return { place, time };
}

/** The class the engine gives a printed page number (src/epub/html.ts). */
export const PAGE_MARKER_CLASS = 'page-marker';

/** A scene heading's words, without the page number the engine may carry
 *  inside it.
 *
 *  When a scene starts a new page, the engine puts that page's marker in
 *  the heading itself, as a floated span (registry 13a), because a marker
 *  rides inside the next block rather than costing a line of its own. So
 *  the heading's textContent reads "2.EXT. FIELD STATION - DAY", and since
 *  page numbers went on by default in 0.7.0 that is what the rail showed.
 *  This walks the heading and keeps every word except a marker's.
 *
 *  Takes the node rather than its text so the decision can be made without
 *  guessing at digits: a heading that genuinely starts with a number keeps
 *  it. Pure over anything node-shaped, which is how it is tested without a
 *  DOM. */
export function headingText(node) {
  if (!node) return '';
  let out = '';
  for (const child of node.childNodes ?? []) {
    if (child.nodeType === 3) out += child.nodeValue ?? '';
    else if (child.nodeType === 1 && !child.classList?.contains(PAGE_MARKER_CLASS)) {
      out += headingText(child);
    }
  }
  return out;
}

/** How far one key press moves the reader down the script.
 *
 *  The frame is the document, so a keyboard reader has to be able to move it.
 *  It used to do that by BEING the Tab stop: WebKitGTK routed the arrow keys
 *  straight into the frame and scrolled it. That had to go, because the same
 *  window paints nothing at all for a focused iframe — no `:focus` match, no
 *  `:focus-visible`, no outline, no box-shadow, and (measured with a probe on
 *  the live window) no focus, blur or focusin event either, so not even a
 *  class could be hung on it. A frame that takes the keyboard and shows
 *  nothing is worse than one that does not take it, so the stop is now the
 *  element AROUND the frame, which is an ordinary div and rings like one —
 *  and scrolling, which came free before, is this function's job instead.
 *
 *  'top' and 'bottom' rather than a number, because Home and End are absolute
 *  and a caller that added them up would only ever approximate them. A page
 *  is nine tenths of the frame, the overlap every reader expects, and it has
 *  a floor so a very short frame still moves more than a line. */
export const READER_LINE = 60;
export const PAGE_SHARE = 0.9;
export const PAGE_FLOOR = 120;

export function scrollStep(key, frameHeight) {
  const height = Number(frameHeight);
  const page = Number.isFinite(height) && height > 0
    ? Math.max(PAGE_FLOOR, Math.round(height * PAGE_SHARE))
    : PAGE_FLOOR;
  switch (key) {
    case 'ArrowDown': return READER_LINE;
    case 'ArrowUp': return -READER_LINE;
    case 'PageDown': case ' ': case 'Spacebar': return page;
    case 'PageUp': return -page;
    case 'Home': return 'top';
    case 'End': return 'bottom';
    default: return null;
  }
}

/** The rail, from the engine's own scene sections in the engine's own order.
 *  Not from a second parse of the fountain: two parsers is two answers about
 *  what a scene is. */
/** The scene index's control. It names what the CLICK will do rather than
 *  what is on screen, because that is how someone decides whether to press
 *  it; `aria-expanded` carries the current state, which is the half a screen
 *  reader needs. Splitting those two apart is the whole reason this is a
 *  function and not two string literals at the call site. */
export function indexToggle(open) {
  return open
    ? { label: 'Hide scenes', expanded: 'true' }
    : { label: 'Show scenes', expanded: 'false' };
}

export function railEntries(scenes) {
  if (!Array.isArray(scenes)) return [];
  return scenes
    .filter((scene) => typeof scene?.id === 'string' && scene.id !== '')
    .map((scene) => ({ id: scene.id, ...sceneLabel(scene.heading) }));
}

/** The dash a slugline is written with, put back between the halves the rail
 *  sets in different inks. */
export const SEPARATOR = '\u2013';

export function railCount(total) {
  return total === 1 ? '1 scene' : `${total} scenes`;
}

/** Where a key moves the scene list's one keyboard stop, from scene `at` of
 *  `count`: Up and Down one scene, Home and End to the ends, stopping at
 *  them rather than wrapping, since this is a list and not a ring. Null for
 *  any other key: Enter and Space belong to the button itself. */
export function railStep(key, at, count) {
  if (!(count > 0)) return null;
  if (key === 'ArrowDown') return Math.min(at + 1, count - 1);
  if (key === 'ArrowUp') return Math.max(at - 1, 0);
  if (key === 'Home') return 0;
  if (key === 'End') return count - 1;
  return null;
}

export const NO_SCENES = 'No scene headings in this script.';

/** Where the reader is, as a scene and a fraction into it — never as a pixel
 *  offset. Tune re-renders on every knob, and a knob that changes the type
 *  size changes every pixel in the document: a remembered scrollY would put
 *  the reader somewhere else in the script and look like a scroll jump. A
 *  scene and a fraction survive the reflow, because that is what the reader
 *  was actually looking at.
 *  `marks` are the sections in document order: { id, top, height }. */
export function readerPlace(marks, scrollTop) {
  if (!Array.isArray(marks) || marks.length === 0) return null;
  const at = Number.isFinite(scrollTop) ? Math.max(0, scrollTop) : 0;
  let found = marks[0];
  for (const mark of marks) {
    if (mark.top <= at) found = mark;
    else break;
  }
  const height = found.height > 0 ? found.height : 1;
  const into = Math.min(1, Math.max(0, (at - found.top) / height));
  return { id: found.id, into };
}

/** The same place, in the reflowed document's pixels. A place whose scene is
 *  gone — the script was re-converted and lost a heading — is the top, which
 *  is the only honest answer. */
export function scrollTarget(marks, place) {
  if (!Array.isArray(marks) || place === null || place === undefined) return 0;
  const mark = marks.find((candidate) => candidate.id === place.id);
  if (mark === undefined) return 0;
  const height = mark.height > 0 ? mark.height : 0;
  return Math.max(0, Math.round(mark.top + height * place.into));
}

// ------------------------------------------------------------------ drawing

let ctx = null;
let pane = null;
let frame = null;
let rail = null;
let readerBox = null;
let indexButton = null;
let railButtons = new Map();
let marks = [];
let place = null;
let sheetCss = '';
let markedId = null;
/** Whether `marks` describe the document the frame is holding RIGHT NOW.
 *  See keep(). */
let measured = false;
let ticking = false;
let resizing = false;
let watching = null;

export function mount(node, context) {
  pane = node;
  ctx = context;

  // The WINDOW's resize, not the frame's: a frame cannot be resized on its
  // own, and — as with scroll — the frame's own events do not reach this
  // document. A reflow moves every section, so marks taken before it are
  // answers about a layout that no longer exists and the rail starts naming
  // the wrong scene. Whether it names the wrong one depends on how
  // proportional the reflow was, which is to say: on luck.
  addEventListener('resize', () => {
    if (resizing) return;
    resizing = true;
    requestAnimationFrame(() => {
      resizing = false;
      measure();
      markCurrent();
    });
  });

  draw();
}

/** The script on screen changed, or went away. */
export function scriptChanged() {
  place = null;
  draw();
}

export function show() {
  if (frame === null) return;
  // Two things are finishable only now. A frame drawn while the pane was
  // hidden had no layout, so the rail could not be measured and the place
  // could not be restored. And a frame that HAS been read is coming back
  // from display:none, after which — measured in the live window — the frame
  // reports the scroll position it had but paints the document somewhere
  // else. Re-asserting the place is what puts the two back together.
  measure();
  restore();
}

/** Leaving the surface hides the pane, and a hidden iframe can lose the
 *  scroll position of the document inside it. Taking the place while it is
 *  still on screen is what makes coming back land where it left. */
export function hide() {
  keep();
}

/** Re-renders the frame from a fresh preview document, keeping the reader
 *  where they were. Called by Tune on every re-render. */
export function render(previewHtml) {
  if (frame === null) {
    draw();
    return;
  }
  keep();
  // The observer belongs to the document being replaced.
  unwatch();
  const parts = splitPreview(previewHtml, new DOMParser());
  sheetCss = parts.css;
  frame.setAttribute('srcdoc', parts.html);
  // The marks now describe a document that is being thrown away, and the
  // replacement starts at scroll 0. Until something measures the new one,
  // nothing may ask the frame where the reader is.
  measured = false;
}

function draw() {
  clear(pane);
  // Whatever was on screen is being thrown away, the frame's document with
  // it; the observer watching that document must go first.
  unwatch();
  frame = null;
  rail = null;
  railButtons = new Map();
  marks = [];
  measured = false;
  markedId = null;

  const state = readerState(ctx.state.script);
  if (state !== 'ready') {
    // `closed` draws nothing on purpose: the tab is disabled in that state,
    // so this pane is not somewhere a reader can be. See NOTICES.
    const notice = NOTICES[state];
    if (notice !== undefined) drawNotice(notice);
    return;
  }

  const script = ctx.state.script;
  indexButton = el('button', {
    type: 'button',
    class: 'btn-quiet index-toggle',
    onclick: () => setIndexOpen(!readerBox.classList.contains('index-open')),
  });
  pane.append(
    el('div', { class: 'read-head' },
      el('h2', { class: 'read-title' }, script.title),
      script.author ? el('p', { class: 'read-by' }, `by ${script.author}`) : null,
      indexButton,
      sendButton('btn-quiet')),
  );

  rail = el('nav', { class: 'scene-rail', 'aria-label': 'Scenes', onkeydown: onRailKey });
  frame = el('iframe', {
    class: 'script-frame',
    title: script.title,
    // Same origin so the parent can keep the reader's place and build the
    // rail from the document the engine wrote. Nothing more is granted, so
    // nothing inside the document can run.
    sandbox: 'allow-same-origin',
    // NOT the Tab stop: see scrollStep. This window paints nothing for a
    // focused iframe and fires no event that would let the page paint it
    // instead, so the stop is the stage around it.
    tabindex: '-1',
  });
  frame.addEventListener('load', dress);

  const stage = el('div', {
    class: 'script-stage',
    tabindex: '0',
    role: 'group',
    'aria-label': `${script.title}, the script`,
    onkeydown: onStageKey,
  }, frame);

  readerBox = el('div', { class: 'reader' }, rail, stage);
  pane.append(readerBox);
  // Open by default: the index is what the maintainer asked to see on the
  // left, and it costs the script nothing either way: it has its own slot
  // beside the script (surfaces.css, .reader), so opening it moves nothing.
  setIndexOpen(true);
  render(script.previewHtml);
}

/** Show or hide the scene index. The class is the whole state: the
 *  stylesheet owns where the panel sits and what hiding looks like, and
 *  nothing here measures or positions anything. */
function setIndexOpen(open) {
  if (readerBox === null || indexButton === null) return;
  readerBox.classList.toggle('index-open', open);
  const { label, expanded } = indexToggle(open);
  indexButton.textContent = label;
  indexButton.setAttribute('aria-expanded', expanded);
}

function drawNotice(notice) {
  pane.append(
    el('h2', { class: 'slug' }, notice.slug),
    el('p', { class: 'prose' }, notice.line),
    el('div', { class: 'read-ways' },
      el('button', {
        type: 'button',
        class: 'btn btn-outline',
        onclick: () => ctx.convertAgain(),
      }, notice.way),
      // The notice says the book itself can still be sent.
      sendButton('btn btn-outline')),
  );
}

/** Back to the result, where the book is sent from (main.js's sendBook).
 *  Quiet in the head, beside Hide scenes; a box beside Convert it again. */
function sendButton(look) {
  return el('button', {
    type: 'button', class: `${look} read-send`, onclick: () => ctx.sendBook?.(),
  }, SEND_BOOK);
}

/** The frame has a document. Give it the sheet, the rail and the place. */
function dress() {
  const doc = frame.contentDocument;
  const win = frame.contentWindow;
  if (!doc || !win) return;
  // An iframe with no src loads about:blank BEFORE the srcdoc that replaces
  // it, and fires `load` for both. Dressing the empty one would build a rail
  // with no scenes in it and hang a scroll listener on a document that is
  // about to be thrown away. A document readerState() let through always has
  // sections in it, so an empty body is the one that is not ours.
  if (doc.body === null || doc.body.childElementCount === 0) return;

  // The theme is the device layer, and it changes when the desktop does:
  // dressFrame() re-dresses every frame it dressed when that happens.
  adopt();

  measure();
  buildRail(doc);
  restore();

  watch(doc, win);
}

/** Where the reader is, as they read: the rail marks the scene on screen,
 *  which is what makes it a place in the script rather than a list of links.
 *
 *  It is an observer and not a scroll listener because a scroll listener does
 *  not work here, which was measured rather than assumed. In this window a
 *  sandboxed srcdoc frame delivers NO scroll event to the parent — not on the
 *  frame's window, its document, its documentElement or its body — while
 *  `scrollY` reads correctly from the parent the whole time. So a reader
 *  wired to 'scroll' renders perfectly and leaves its mark on scene one
 *  forever. An IntersectionObserver CONSTRUCTED IN THE FRAME'S REALM does
 *  fire, and at exactly the right moments: a section leaves the viewport's
 *  top edge at precisely the scroll position where the next one becomes the
 *  scene being read.
 *
 *  Which scene that is, is still readerPlace()'s answer and not the
 *  observer's — the observer only says WHEN to ask, so there are never two
 *  opinions about where the reader is. */
function watch(doc, win) {
  unwatch();
  if (typeof win.IntersectionObserver !== 'function') return;
  watching = new win.IntersectionObserver(() => {
    if (ticking) return;
    ticking = true;
    requestAnimationFrame(() => {
      ticking = false;
      markCurrent();
    });
  }, { threshold: 0 });
  for (const scene of doc.querySelectorAll('section.scene')) watching.observe(scene);
}

function unwatch() {
  if (watching !== null) watching.disconnect();
  watching = null;
}

/** The arrow keys, on the stage that stands in for the frame. The frame is
 *  same-origin, so the parent can scroll it; what each key is worth is
 *  scrollStep's answer and not this handler's. */
function onStageKey(event) {
  const win = frame?.contentWindow;
  if (!win) return;
  const step = scrollStep(event.key, frame.clientHeight);
  if (step === null) return;
  event.preventDefault();
  if (step === 'top') win.scrollTo(0, 0);
  else if (step === 'bottom') win.scrollTo(0, win.document.documentElement.scrollHeight);
  else win.scrollBy(0, step);
}

/** Put the engine's own stylesheet into a preview frame, as a CONSTRUCTED
 *  stylesheet rather than a <style> element.
 *
 *  This is the file's most load-bearing eight lines and the reason it is
 *  exported rather than kept private: the CSP forbids inline style, INCLUDING
 *  a <style> inside an iframe srcdoc, and the failure is silent — an unstyled
 *  script renders with no error anywhere. CSSOM is not blocked, so the sheet
 *  is adopted instead. The Settings preview needs exactly this and must not
 *  own a second copy of it, because a second copy would drift and the drift
 *  would be invisible until somebody looked at the preview.
 *
 *  Takes its frame and css as arguments so it belongs to no one surface. */
export function dressFrame(frame, engineCss) {
  const doc = frame?.contentDocument;
  const win = frame?.contentWindow;
  if (!doc || !win) return false;
  const root = getComputedStyle(document.documentElement);
  const tokens = paperFrom((name) => root.getPropertyValue(name));
  const sheet = new win.CSSStyleSheet();
  sheet.replaceSync(sheetText(engineCss, tokens));
  doc.adoptedStyleSheets = [sheet];
  followTheme(frame, engineCss);
  return true;
}

/** Every frame dressed here, with the sheet it was dressed in, so a change
 *  of the desktop's theme re-dresses all of them: Read's page, the result's
 *  first page and the Settings preview alike. A frame no longer in the
 *  window is dropped at the next change, or the next dressing, rather than
 *  dressed. */
const dressed = new Map();
let themeQuery = null;

/** How many frames are being followed: for tests, which check that a frame
 *  that has left the window is let go. */
export const dressedCount = () => dressed.size;

function followTheme(frame, engineCss) {
  // Let go of frames that have left the window now too, not only at a theme
  // change: every new result draws a new frame, and a desktop that never
  // changes theme would otherwise keep every one of them.
  for (const each of [...dressed.keys()]) if (each.isConnected === false) dressed.delete(each);
  dressed.set(frame, engineCss);
  if (themeQuery !== null || typeof matchMedia !== 'function') return;
  themeQuery = matchMedia('(prefers-color-scheme: dark)');
  themeQuery.addEventListener('change', () => {
    for (const [each, css] of [...dressed]) {
      if (each.isConnected === false) dressed.delete(each);
      else dressFrame(each, css);
    }
  });
}

function adopt() {
  dressFrame(frame, sheetCss);
}

/** The sections, where they are now. Re-measured rather than remembered:
 *  the type size, the window width and the format options all move them. */
function measure() {
  const doc = frame?.contentDocument;
  if (!doc || frame.clientHeight === 0) return;
  marks = [...doc.querySelectorAll('section.scene')].map((scene) => ({
    id: scene.id,
    top: scene.offsetTop,
    height: scene.offsetHeight,
  }));
  measured = true;
}

/** Take the place while the frame can still say where it is. This runs as
 *  the surface goes away, by which time the pane is ALREADY hidden — so it
 *  must not ask for layout, and does not: `scrollY` still answers, and the
 *  marks were measured while the frame was on screen.
 *
 *  `measured` is the guard that makes that true, and it is not theoretical.
 *  Tune re-renders while this pane is hidden, so the new document cannot be
 *  measured (measure() needs layout) — and the frame holding it reports
 *  `scrollY` 0. A SECOND re-render's keep() would then read 0 against the
 *  OLD document's marks and overwrite a perfectly good place with "the top
 *  of scene one". Measured in the live window: one knob kept the reader at
 *  sc-012, 0.331 into it, across a real reflow (14486px → 16313px); two
 *  knobs in a row landed them at scroll 77 with the rail marking nothing.
 *  The place is not re-taken here between renders — it was taken when the
 *  surface was left and is still the truth. */
function keep() {
  const win = frame?.contentWindow;
  if (!win || marks.length === 0 || !measured) return;
  place = readerPlace(marks, win.scrollY);
}

function restore() {
  const win = frame?.contentWindow;
  if (!win || frame.clientHeight === 0) return;
  if (place !== null) {
    const target = scrollTarget(marks, place);
    // Asked to go where it already says it is, the frame does nothing — and
    // after a display:none round trip "where it says it is" is not where it
    // is drawn. Going to the top first is what makes the second scroll a
    // real one. It is one frame of work and never visible.
    if (win.scrollY === target) win.scrollTo(0, 0);
    win.scrollTo(0, target);
  }
  markCurrent();
}

function buildRail(doc) {
  clear(rail);
  railButtons = new Map();
  const sections = [...doc.querySelectorAll('section.scene')];
  const entries = railEntries(sections.map((scene) => ({
    id: scene.id,
    heading: headingText(scene.querySelector('h2.scene-heading')),
  })));

  if (entries.length === 0) {
    rail.append(el('p', { class: 'caption' }, NO_SCENES));
    return;
  }

  rail.append(el('p', { class: 'rail-count' }, railCount(entries.length)));
  for (const entry of entries) {
    const target = doc.getElementById(entry.id);
    const button = el('button', {
      type: 'button',
      class: 'scene-link',
      // One stop for the whole list (setRailStop): the rest are reached
      // with the arrow keys, the way the tablist's are.
      tabindex: '-1',
      onclick: () => target?.scrollIntoView({
        behavior: ctx.state.reducedMotion ? 'auto' : 'smooth',
        block: 'start',
      }),
    },
    el('span', { class: 'scene-where' }, entry.place),
    // The time of day, set quieter, running on from the place rather than
    // ranged right: a long location wraps, and a time ranged right against a
    // wrapped place lines up with the wrong line of it.
    entry.time ? el('span', { class: 'scene-when' }, ` ${SEPARATOR} ${entry.time}`) : null);
    railButtons.set(entry.id, button);
    rail.append(button);
  }
  setRailStop([...railButtons.values()][0]);
}

/** Make `stop` the scene list's one Tab stop. */
function setRailStop(stop) {
  for (const button of railButtons.values()) button.tabIndex = button === stop ? 0 : -1;
}

/** Up, Down, Home and End on the scene list: move the stop and the focus.
 *  railStep decides where; this only applies it. */
function onRailKey(event) {
  const buttons = [...railButtons.values()];
  const at = buttons.indexOf(event.target);
  if (at === -1) return;
  const next = railStep(event.key, at, buttons.length);
  if (next === null) return;
  event.preventDefault();
  setRailStop(buttons[next]);
  buttons[next].focus();
}

/** Mark the scene the reader is in, and keep that mark inside the rail's own
 *  scroll window without moving anything else on the page. */
function markCurrent() {
  const win = frame?.contentWindow;
  if (!win || railButtons.size === 0) return;
  const at = readerPlace(marks, win.scrollY);
  if (at === null || at.id === markedId) return;
  railButtons.get(markedId)?.classList.remove('scene-link-on');
  railButtons.get(markedId)?.removeAttribute('aria-current');
  markedId = at.id;
  const button = railButtons.get(markedId);
  if (button === undefined) return;
  button.classList.add('scene-link-on');
  button.setAttribute('aria-current', 'true');
  // The stop follows the scene being read, so Tab lands where the reader
  // is. Not while the keyboard is walking the list: the stop is where the
  // focus is then, and taking it away would strand the focus off the stop.
  if (![...railButtons.values()].includes(document.activeElement)) setRailStop(button);
  if (button.offsetTop < rail.scrollTop) rail.scrollTop = button.offsetTop;
  const past = button.offsetTop + button.offsetHeight - rail.clientHeight;
  if (past > rail.scrollTop) rail.scrollTop = past;
}
