// The left half of the result: the book's first page, in its own
// typesetting (spec 2026-10-01, decision 2). For now a static preview: the
// engine's preview document in a frame dressed exactly as the reader's
// (read.js's dressFrame), scrolled to the first scene. Part 3 of the spec
// (the page writes itself) replaces what is INSIDE .result-page with the
// typed page and its wave; the box, its size and its place stay.
import { el } from './dom.js';
import { dressFrame, splitPreview } from './read.js';

export function resultPage(script) {
  const box = el('div', { class: 'result-page', 'aria-hidden': 'true' });
  const html = typeof script?.previewHtml === 'string' ? script.previewHtml.trim() : '';
  if (html === '') return box;
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
    // window around the frame too.
    const scene = inside.querySelector('section.scene');
    if (scene) frame.contentWindow?.scrollTo(0, scene.offsetTop);
  });
  frame.setAttribute('srcdoc', doc);
  box.append(frame);
  return box;
}
