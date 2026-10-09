// The left half of the result: the book's first page, in its own
// typesetting (spec 2026-10-01, decision 2): the
// engine's preview document in a frame dressed exactly as the reader's
// (read.js's dressFrame), scrolled to the first scene. While a script
// converts, page-show.js draws the same box first and types the page in it;
// when the book exists it puts resultFrame's frame under the typed page, so
// the result's page is already there when the typed one lifts away.
import { el } from './dom.js';
import { dressFrame, splitPreview } from './read.js';

/** The result's box: the book's first page, or an empty sheet when the
 *  engine sent no pages. */
export function resultPage(script) {
  const box = el('div', { class: 'result-page', 'aria-hidden': 'true' });
  const frame = resultFrame(script);
  if (frame !== null) box.append(frame);
  return box;
}

/** The frame alone, or null when there is no page to show. `onReady`, if
 *  given, hears the frame's document and the engine's stylesheet once the
 *  frame is dressed and scrolled to the first scene. */
export function resultFrame(script, onReady = null) {
  const html = typeof script?.previewHtml === 'string' ? script.previewHtml.trim() : '';
  if (html === '') return null;
  const { css, html: doc } = splitPreview(html, new DOMParser());
  // The same frame read.js draws: same origin so the parent can dress it,
  // nothing more granted, so nothing inside can run. Not a Tab stop: the
  // page is a picture of the book here, and the panel beside it is the work.
  const frame = el('iframe', {
    class: 'result-page-frame',
    title: 'First page',
    sandbox: 'allow-same-origin',
    tabindex: '-1',
  });
  frame.addEventListener('load', () => {
    const inside = frame.contentDocument;
    // about:blank loads first and fires `load` too; it has nothing to dress.
    if (!inside?.body || inside.body.childElementCount === 0) return;
    dressFrame(frame, css);
    // The frame's own scroll, not scrollIntoView: that would scroll the
    // window around the frame too. Stopped a line and a half short of the
    // scene, so its heading has the page's air above it rather than sitting
    // flush against the frame's edge.
    const scene = inside.querySelector('section.scene');
    if (scene) {
      const em = parseFloat(frame.contentWindow?.getComputedStyle?.(inside.body)?.fontSize) || 16;
      frame.contentWindow?.scrollTo(0, Math.max(0, scene.offsetTop - 1.5 * em));
    }
    onReady?.(inside, css);
  });
  frame.setAttribute('srcdoc', doc);
  return frame;
}
